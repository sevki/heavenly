// heavenly playground: the physics runs in WebAssembly (crate `heavenly-viz`,
// built from the heavenly crates); this file only draws, with WebGL2, and
// wires controls. No libraries.
(function () {
  "use strict";
  const root = document.getElementById("hv-play");
  if (!root) return;
  const $ = (id) => document.getElementById(id);
  const R_EARTH = 6378.137;
  const DEG = Math.PI / 180;

  let wasm = null;
  let land = null;
  let t = 0; // seconds after the epoch
  let playing = true;
  let lastFrame = null;
  let view = { az: 35 * DEG, el: 22 * DEG, zoom: 1 };
  let path = [];
  let track = [];
  let info = [];
  let passes = [];
  let sites = []; // [{name, lat, lon}], from the library
  let walker = null; // {total, planes, phasing} while showing a constellation
  let epochMs = 0; // the model epoch, as JS milliseconds (UTC)
  const ISS_TLE =
    "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n" +
    "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

  // --- WebAssembly ----------------------------------------------------------
  function read(n) {
    return Array.from(new Float64Array(wasm.memory.buffer, wasm.out_ptr(), n));
  }
  function error() {
    return String.fromCharCode(...read(wasm.last_error()));
  }
  function call(fn, ...args) {
    const n = wasm[fn](...args);
    if (n === 0) throw new Error(error());
    return read(n);
  }

  async function load() {
    try {
      orbitGL = makeGL($("hv-orbit"));
      trackGL = makeGL($("hv-track"));
      if (!orbitGL || !trackGL) throw new Error("this browser has no WebGL2");
      const [bytes, landData] = await Promise.all([
        fetch("playground/heavenly_viz.wasm").then((r) => {
          if (!r.ok) throw new Error("heavenly_viz.wasm not found (" + r.status + ")");
          return r.arrayBuffer();
        }),
        fetch("playground/land-110m.json").then((r) => (r.ok ? r.json() : null)).catch(() => null),
      ]);
      const { instance } = await WebAssembly.instantiate(bytes, {});
      wasm = instance.exports;
      land = landData;
      const names = String.fromCharCode(...read(wasm.site_names())).split("\n");
      sites = chunk(read(wasm.sites()), 2).map(([lat, lon], k) => ({ name: names[k], lat, lon }));
      $("hv-status").textContent = "";
      root.classList.add("hv-ready");
      applyOrbit();
      requestAnimationFrame(frame);
    } catch (e) {
      $("hv-status").textContent =
        "The playground could not start: " + e.message +
        ". Build it with `book/build-playground.sh` and serve the book over HTTP (mdbook serve).";
    }
  }

  // --- Controls ---------------------------------------------------------------
  const ALT_MIN = 150, ALT_MAX = 40000;
  const altFromSlider = (v) => ALT_MIN * Math.pow(ALT_MAX / ALT_MIN, v / 1000);
  const sliderFromAlt = (a) => (1000 * Math.log(a / ALT_MIN)) / Math.log(ALT_MAX / ALT_MIN);
  const presets = {
    iss: { alt: 420, e: 0.0005, i: 51.64, raan: 0, argp: 0 },
    sso: { alt: 700, e: 0.001, i: 98.2, raan: 40, argp: 0 },
    molniya: { alt: 600, e: 0.74, i: 63.4, raan: 300, argp: 270 },
    gps: { alt: 20180, e: 0.005, i: 55, raan: 120, argp: 0 },
    geo: { alt: 35786, e: 0, i: 0.05, raan: 0, argp: 0 },
  };

  function setControls(p) {
    $("hv-alt").value = sliderFromAlt(p.alt);
    $("hv-e").value = p.e;
    $("hv-i").value = p.i;
    $("hv-raan").value = p.raan;
    $("hv-argp").value = p.argp;
  }

  function applyOrbit() {
    const alt = altFromSlider(+$("hv-alt").value);
    const e = +$("hv-e").value, i = +$("hv-i").value;
    const raan = +$("hv-raan").value, argp = +$("hv-argp").value;
    $("hv-alt-v").textContent = alt.toFixed(0) + " km";
    $("hv-e-v").textContent = e.toFixed(3);
    $("hv-i-v").textContent = i.toFixed(1) + "°";
    $("hv-raan-v").textContent = raan.toFixed(0) + "°";
    $("hv-argp-v").textContent = argp.toFixed(0) + "°";
    if (!wasm) return;
    const d = new Date($("hv-epoch").value + "Z");
    try {
      if (isNaN(d)) throw new Error("invalid epoch");
      if (wasm.set_epoch(d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds()) !== 0) throw new Error(error());
      if (wasm.set_orbit(alt, e, i, raan, argp, 0) !== 0) throw new Error(error());
      refresh(Math.min(Math.max(3 * call("info")[0], 3 * 3600), 2 * 86400));
      root.classList.remove("hv-tle-mode");
    } catch (err) {
      $("hv-status").textContent = err.message;
    }
  }

  // Loads a TLE into the model (SGP4). The epoch becomes the TLE's.
  function applyTle(text) {
    if (!wasm) return;
    try {
      const bytes = new TextEncoder().encode(text);
      // Ask for the buffer first: it can grow memory, which replaces
      // wasm.memory.buffer.
      const at = wasm.input_ptr(bytes.length);
      new Uint8Array(wasm.memory.buffer, at, bytes.length).set(bytes);
      if (wasm.set_tle(bytes.length) !== 0) throw new Error(error());
      t = 0;
      refresh(2 * 86400);
      // Show the TLE's epoch, and that the sliders are not in use.
      $("hv-epoch").value = new Date(epochMs).toISOString().slice(0, 16);
      root.classList.add("hv-tle-mode");
      $("hv-status").textContent = "Propagating the TLE with SGP4. Move a slider or pick a preset to go back to two-body elements.";
    } catch (err) {
      $("hv-status").textContent = err.message;
    }
  }

  // Everything below comes from the library; this only stores it for drawing.
  function refresh(span) {
    info = call("info");
    const f = call("epoch_fields");
    epochMs = Date.UTC(f[0], f[1] - 1, f[2], f[3], f[4], 0) + f[5] * 1000;
    path = chunk(call("orbit_path", 361), 4);
    $("hv-time").max = span;
    track = chunk(call("ground_track", 0, span, 1200), 4);
    passes = chunk(call("passes", 0, span), 4);
    drawPasses();
    $("hv-status").textContent = "";
  }

  function utc(ts) {
    return new Date(epochMs + ts * 1000).toISOString().slice(0, 16).replace("T", " ");
  }

  function drawPasses() {
    if (!passes.length) { $("hv-passes").innerHTML = "No passes above 10° over London in this span."; return; }
    const rows = passes.map(([a, b, top, vis]) =>
      `<tr data-t="${a}" class="${vis > 0 ? "hv-vis" : ""}"><td>${utc(a)} UTC</td><td>${Math.round(b - a)} s</td>` +
      `<td>${top.toFixed(0)}°</td><td>${vis > 0 ? "visible for " + Math.round(vis) + " s" : "not visible"}</td></tr>`).join("");
    $("hv-passes").innerHTML =
      `<table><tr><th>Passes over London above 10°</th><th>length</th><th>highest</th><th>sunlit in a dark sky</th></tr>${rows}</table>`;
    for (const tr of $("hv-passes").querySelectorAll("tr[data-t]")) {
      tr.addEventListener("click", () => { t = +tr.dataset.t; $("hv-time").value = t; });
    }
  }

  function chunk(a, n) {
    const out = [];
    for (let k = 0; k + n <= a.length; k += n) out.push(a.slice(k, k + n));
    return out;
  }

  for (const id of ["hv-alt", "hv-e", "hv-i", "hv-raan", "hv-argp", "hv-epoch"]) {
    $(id).addEventListener("input", applyOrbit);
  }
  // A Walker 24/4/1 shell built from the slider orbit; the sliders keep
  // shaping it (altitude, inclination, node).
  $("hv-preset-walker").addEventListener("click", () => {
    walker = { total: 24, planes: 4, phasing: 1 };
    setControls({ alt: 550, e: 0, i: 53, raan: 0, argp: 0 });
    t = 0;
    applyOrbit();
  });
  // Dawn–dusk sun-synchronous orbit at 550 km for the chosen epoch: the
  // library gives the inclination and the node that faces the Sun.
  $("hv-preset-odc").addEventListener("click", () => {
    if (!wasm) return;
    walker = null;
    root.classList.remove("hv-tle-mode");
    applyOrbit(); // sets the epoch from the input
    try {
      const [i, raan] = call("dawn_dusk", 550);
      setControls({ alt: 550, e: 0, i, raan, argp: 0 });
      t = 0;
      applyOrbit();
    } catch (err) {
      $("hv-status").textContent = err.message;
    }
  });
  $("hv-tle").value = ISS_TLE;
  $("hv-preset-tle").addEventListener("click", () => { walker = null; $("hv-tle").value = ISS_TLE; applyTle(ISS_TLE); });
  $("hv-tle-btn").addEventListener("click", () => { walker = null; applyTle($("hv-tle").value); });
  for (const [name, p] of Object.entries(presets)) {
    const b = $("hv-preset-" + name);
    if (b) b.addEventListener("click", () => { walker = null; setControls(p); t = 0; applyOrbit(); });
  }
  $("hv-play-btn").addEventListener("click", () => {
    playing = !playing;
    $("hv-play-btn").textContent = playing ? "Pause" : "Play";
  });
  $("hv-time").addEventListener("input", () => { t = +$("hv-time").value; });

  // Drag to rotate, wheel to zoom.
  const oc = $("hv-orbit");
  let drag = null;
  oc.addEventListener("pointerdown", (e) => { drag = [e.clientX, e.clientY, view.az, view.el]; oc.setPointerCapture(e.pointerId); });
  oc.addEventListener("pointermove", (e) => {
    if (!drag) return;
    view.az = drag[2] - (e.clientX - drag[0]) * 0.008;
    view.el = Math.max(-1.5, Math.min(1.5, drag[3] + (e.clientY - drag[1]) * 0.008));
  });
  oc.addEventListener("pointerup", () => { drag = null; });
  oc.addEventListener("wheel", (e) => {
    e.preventDefault();
    view.zoom = Math.max(0.3, Math.min(8, view.zoom * Math.exp(-e.deltaY * 0.001)));
  }, { passive: false });

  // --- WebGL2 ------------------------------------------------------------------
  // Hand-written: lines are instanced quads expanded in screen space (so they
  // can be thick and dashed, which GL lines cannot), points are instanced
  // sprites, and fills are triangles. Concave fills (the land on the map) use
  // the stencil buffer's even-odd rule, so no triangulation is needed. Text
  // goes on a transparent 2D canvas laid over each view.

  const LINE_VS = `#version 300 es
layout(location=0) in vec2 corner;
layout(location=1) in vec3 a;
layout(location=2) in vec3 b;
layout(location=3) in vec4 color;
layout(location=4) in vec3 wdd; // width px, dash period px (0: solid), distance along the line (model units)
uniform mat4 mvp;
uniform vec2 viewport;
uniform float pxPerUnit;
uniform float dpr;
out vec4 vColor;
out float vDist;
out float vDash;
void main() {
  vec4 pa = mvp * vec4(a, 1.0), pb = mvp * vec4(b, 1.0);
  vec2 sa = pa.xy * 0.5 * viewport, sb = pb.xy * 0.5 * viewport;
  vec2 d = sb - sa;
  float len = length(d);
  vec2 dir = len > 1e-6 ? d / len : vec2(1.0, 0.0);
  vec4 p = mix(pa, pb, corner.x);
  p.xy += vec2(-dir.y, dir.x) * corner.y * wdd.x * dpr / viewport;
  gl_Position = p;
  vColor = color;
  vDash = wdd.y * dpr;
  vDist = wdd.z * pxPerUnit + corner.x * len;
}`;
  const LINE_FS = `#version 300 es
precision highp float;
in vec4 vColor;
in float vDist;
in float vDash;
out vec4 o;
void main() {
  if (vDash > 0.0 && mod(vDist, vDash) > 0.55 * vDash) discard;
  o = vColor;
}`;
  const POINT_VS = `#version 300 es
layout(location=0) in vec2 corner;
layout(location=1) in vec3 pos;
layout(location=2) in vec4 color;
layout(location=3) in vec2 sizeShape; // diameter px, shape (0 disc, 1 square)
uniform mat4 mvp;
uniform vec2 viewport;
uniform float dpr;
out vec4 vColor;
out vec2 vUv;
out float vShape;
void main() {
  vec4 p = mvp * vec4(pos, 1.0);
  p.xy += corner * sizeShape.x * dpr / viewport;
  gl_Position = p;
  vColor = color;
  vUv = corner;
  vShape = sizeShape.y;
}`;
  const POINT_FS = `#version 300 es
precision highp float;
in vec4 vColor;
in vec2 vUv;
in float vShape;
out vec4 o;
void main() {
  if (vShape < 0.5 && dot(vUv, vUv) > 1.0) discard;
  o = vColor;
}`;
  const MESH_VS = `#version 300 es
layout(location=0) in vec3 pos;
uniform mat4 mvp;
out vec3 vN;
void main() {
  gl_Position = mvp * vec4(pos, 1.0);
  vN = pos;
}`;
  const MESH_FS = `#version 300 es
precision highp float;
in vec3 vN;
uniform vec4 color;
uniform vec4 night; // colour on the night side; a = 0 for no shading
uniform vec3 sun;
out vec4 o;
void main() {
  if (night.a == 0.0) { o = color; return; }
  float d = dot(normalize(vN), sun);
  o = mix(night, color, smoothstep(-0.08, 0.08, d));
}`;

  function makeGL(canvas) {
    const gl = canvas.getContext("webgl2", { antialias: true, stencil: true, premultipliedAlpha: false });
    if (!gl) return null;
    const program = (vs, fs) => {
      const p = gl.createProgram();
      for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
        const s = gl.createShader(type);
        gl.shaderSource(s, src);
        gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
        gl.attachShader(p, s);
      }
      gl.linkProgram(p);
      if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
      const u = {};
      for (let i = 0; i < gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS); i++) {
        const name = gl.getActiveUniform(p, i).name;
        u[name] = gl.getUniformLocation(p, name);
      }
      return { p, u };
    };
    const progs = { line: program(LINE_VS, LINE_FS), point: program(POINT_VS, POINT_FS), mesh: program(MESH_VS, MESH_FS) };
    const staticBuffer = (data) => {
      const b = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, b);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      return b;
    };
    const lineCorners = staticBuffer(new Float32Array([0, -1, 1, -1, 0, 1, 1, 1]));
    const pointCorners = staticBuffer(new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]));
    const cover = staticBuffer(new Float32Array([-1, -1, 0, 1, -1, 0, -1, 1, 0, 1, 1, 0]));
    const identity = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
    gl.bindVertexArray(gl.createVertexArray());
    const dynamic = gl.createBuffer();
    const cache = new Map(); // key → buffer, for geometry that never changes

    // Uploads `data` (a Float32Array, or a function making one) and returns
    // its buffer; with a key, it is uploaded once and kept.
    function buffer(data, key) {
      if (key) {
        let b = cache.get(key);
        if (!b) {
          const d = typeof data === "function" ? data() : data;
          b = { buf: staticBuffer(d), length: d.length };
          cache.set(key, b);
        }
        return b;
      }
      gl.bindBuffer(gl.ARRAY_BUFFER, dynamic);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STREAM_DRAW);
      return { buf: dynamic, length: data.length };
    }
    function attr(loc, buf, size, stride, offset, divisor) {
      gl.bindBuffer(gl.ARRAY_BUFFER, buf);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, gl.FLOAT, false, stride * 4, offset * 4);
      gl.vertexAttribDivisor(loc, divisor);
    }
    function reset() {
      for (let i = 0; i < 5; i++) {
        gl.disableVertexAttribArray(i);
        gl.vertexAttribDivisor(i, 0);
      }
    }
    function state(opts) {
      opts.depth ? gl.enable(gl.DEPTH_TEST) : gl.disable(gl.DEPTH_TEST);
      gl.depthMask(!!opts.depthWrite);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    }
    const r = {
      dpr: 1,
      // Sizes the drawing buffer to the canvas and clears it.
      begin() {
        const rect = canvas.getBoundingClientRect();
        r.dpr = window.devicePixelRatio || 1;
        const w = Math.max(1, Math.round(rect.width * r.dpr)), h = Math.max(1, Math.round(rect.height * r.dpr));
        if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
        gl.viewport(0, 0, w, h);
        gl.clearColor(0, 0, 0, 0);
        gl.clearDepth(1);
        gl.clearStencil(0);
        gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT | gl.STENCIL_BUFFER_BIT);
        return [rect.width, rect.height];
      },
      // Line instances: 13 floats each (a, b, rgba, width px, dash px, distance).
      lines(data, mvp, pxPerUnit, opts = {}, key) {
        const b = buffer(data, key);
        if (!b.length) return;
        const { p, u } = progs.line;
        gl.useProgram(p);
        state(opts);
        reset();
        attr(0, lineCorners, 2, 2, 0, 0);
        attr(1, b.buf, 3, 13, 0, 1);
        attr(2, b.buf, 3, 13, 3, 1);
        attr(3, b.buf, 4, 13, 6, 1);
        attr(4, b.buf, 3, 13, 10, 1);
        gl.uniformMatrix4fv(u.mvp, false, mvp);
        gl.uniform2f(u.viewport, canvas.width, canvas.height);
        gl.uniform1f(u.pxPerUnit, pxPerUnit);
        gl.uniform1f(u.dpr, r.dpr);
        gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, b.length / 13);
      },
      // Point instances: 9 floats each (position, rgba, diameter px, shape).
      points(data, mvp, opts = {}) {
        const b = buffer(data);
        if (!b.length) return;
        const { p, u } = progs.point;
        gl.useProgram(p);
        state(opts);
        reset();
        attr(0, pointCorners, 2, 2, 0, 0);
        attr(1, b.buf, 3, 9, 0, 1);
        attr(2, b.buf, 4, 9, 3, 1);
        attr(3, b.buf, 2, 9, 7, 1);
        gl.uniformMatrix4fv(u.mvp, false, mvp);
        gl.uniform2f(u.viewport, canvas.width, canvas.height);
        gl.uniform1f(u.dpr, r.dpr);
        gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, b.length / 9);
      },
      // Triangles (3 floats per vertex) in one colour, optionally shaded by
      // a Sun direction into a night colour.
      mesh(data, mvp, color, opts = {}, key) {
        const b = buffer(data, key);
        if (!b.length) return;
        const { p, u } = progs.mesh;
        gl.useProgram(p);
        state(opts);
        reset();
        attr(0, b.buf, 3, 3, 0, 0);
        gl.uniformMatrix4fv(u.mvp, false, mvp);
        gl.uniform4fv(u.color, color);
        gl.uniform4fv(u.night, opts.night || [0, 0, 0, 0]);
        gl.uniform3fv(u.sun, opts.sun || [1, 0, 0]);
        gl.drawArrays(gl.TRIANGLES, 0, b.length / 3);
      },
      // Fills possibly concave polygons given as triangle fans: the fans
      // flip the stencil (even-odd), then one quad colours what is set.
      fillEvenOdd(data, mvp, color, key) {
        const b = buffer(data, key);
        if (!b.length) return;
        const { p, u } = progs.mesh;
        gl.useProgram(p);
        reset();
        gl.disable(gl.DEPTH_TEST);
        gl.depthMask(false);
        gl.enable(gl.STENCIL_TEST);
        gl.colorMask(false, false, false, false);
        gl.stencilFunc(gl.ALWAYS, 0, 0xff);
        gl.stencilOp(gl.KEEP, gl.KEEP, gl.INVERT);
        attr(0, b.buf, 3, 3, 0, 0);
        gl.uniformMatrix4fv(u.mvp, false, mvp);
        gl.uniform4fv(u.night, [0, 0, 0, 0]);
        gl.drawArrays(gl.TRIANGLES, 0, b.length / 3);
        gl.colorMask(true, true, true, true);
        gl.stencilFunc(gl.NOTEQUAL, 0, 0xff);
        gl.stencilOp(gl.ZERO, gl.ZERO, gl.ZERO); // clears as it colours
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        attr(0, cover, 3, 3, 0, 0);
        gl.uniformMatrix4fv(u.mvp, false, identity);
        gl.uniform4fv(u.color, color);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
        gl.disable(gl.STENCIL_TEST);
      },
    };
    return r;
  }
  let orbitGL = null, trackGL = null;

  // A transparent 2D canvas over a WebGL canvas, for text.
  function overlay(canvas) {
    const wrap = document.createElement("div");
    wrap.style.position = "relative";
    canvas.parentNode.insertBefore(wrap, canvas);
    wrap.appendChild(canvas);
    const over = document.createElement("canvas");
    over.style.cssText = "position:absolute;left:0;top:0;width:100%;height:100%;pointer-events:none";
    wrap.appendChild(over);
    return () => {
      const rect = canvas.getBoundingClientRect(), dpr = window.devicePixelRatio || 1;
      const w = Math.round(rect.width * dpr), h = Math.round(rect.height * dpr);
      if (over.width !== w || over.height !== h) { over.width = w; over.height = h; }
      const ctx = over.getContext("2d");
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, rect.width, rect.height);
      return ctx;
    };
  }
  const orbitText = overlay($("hv-orbit"));
  const trackText = overlay($("hv-track"));

  // Theme colours (CSS) as [r, g, b, a] in 0..1.
  const swatch = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  const parsed = new Map();
  function rgba(css, alpha = 1) {
    let c = parsed.get(css);
    if (!c) {
      swatch.clearRect(0, 0, 1, 1);
      swatch.fillStyle = "#000";
      swatch.fillStyle = css;
      swatch.fillRect(0, 0, 1, 1);
      const d = swatch.getImageData(0, 0, 1, 1).data;
      c = [d[0] / 255, d[1] / 255, d[2] / 255, d[3] / 255];
      parsed.set(css, c);
    }
    return [c[0], c[1], c[2], c[3] * alpha];
  }
  function colours() {
    const cs = getComputedStyle(document.documentElement);
    const v = (n, d) => (cs.getPropertyValue(n).trim() || d);
    return { fg: v("--fg", "#333"), accent: v("--links", "#1f6feb"), muted: v("--icons", "#888"), bg: v("--bg", "#fff"), hot: "#e8590c", sun: "#f2b705" };
  }

  // Geometry builders. Lines: one instance per segment.
  function line(out, pts, color, width, dash = 0) {
    let dist = 0;
    for (let k = 1; k < pts.length; k++) {
      const a = pts[k - 1], b = pts[k];
      out.push(a[0], a[1], a[2] || 0, b[0], b[1], b[2] || 0, color[0], color[1], color[2], color[3], width, dash, dist);
      dist += Math.hypot(b[0] - a[0], b[1] - a[1], (b[2] || 0) - (a[2] || 0));
    }
  }
  function point(out, p, color, size, square = false) {
    out.push(p[0], p[1], p[2] || 0, color[0], color[1], color[2], color[3], size, square ? 1 : 0);
  }
  const f32 = (a) => new Float32Array(a);

  // Column-major 4×4 helpers.
  function mul(a, b) {
    const o = new Float32Array(16);
    for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
    return o;
  }

  // Unit sphere surface points on a latitude/longitude grid, as triangles.
  function sphere(radius, nLat = 48, nLon = 96) {
    const v = (i, j) => {
      const lat = -Math.PI / 2 + (Math.PI * i) / nLat, lon = (2 * Math.PI * j) / nLon;
      return [radius * Math.cos(lat) * Math.cos(lon), radius * Math.cos(lat) * Math.sin(lon), radius * Math.sin(lat)];
    };
    const out = [];
    for (let i = 0; i < nLat; i++) for (let j = 0; j < nLon; j++) {
      const [a, b, c, d] = [v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)];
      out.push(...a, ...b, ...c, ...a, ...c, ...d);
    }
    return f32(out);
  }
  const sph = (lat, lon, r = R_EARTH) => [r * Math.cos(lat) * Math.cos(lon), r * Math.cos(lat) * Math.sin(lon), r * Math.sin(lat)];

  // --- The 3D view ------------------------------------------------------------
  function drawOrbit(c, now) {
    const g = orbitGL;
    const [w, h] = g.begin();
    const rmax = Math.max(...path.map((q) => Math.hypot(q[0], q[1], q[2])), R_EARTH * 1.5);
    const { az, el, zoom } = view;
    const right = [-Math.sin(az), Math.cos(az), 0];
    const up = [-Math.sin(el) * Math.cos(az), -Math.sin(el) * Math.sin(az), Math.cos(el)];
    const toward = [Math.cos(el) * Math.cos(az), Math.cos(el) * Math.sin(az), Math.sin(el)];
    const s = (zoom * 0.45 * Math.min(w, h)) / rmax; // CSS px per km
    // Orthographic: x, y in clip space; z from the distance along the view.
    const kx = (2 * s) / w, ky = (2 * s) / h, kz = 1 / (4 * rmax);
    const mvp = f32([
      right[0] * kx, up[0] * ky, -toward[0] * kz, 0,
      right[1] * kx, up[1] * ky, -toward[1] * kz, 0,
      right[2] * kx, up[2] * ky, -toward[2] * kz, 0,
      0, 0, 0, 1,
    ]);
    const project = (v) => [w / 2 + (v[0] * right[0] + v[1] * right[1] + v[2] * right[2]) * s, h / 2 - (v[0] * up[0] + v[1] * up[1] + v[2] * up[2]) * s];
    const depth = (v) => v[0] * toward[0] + v[1] * toward[1] + v[2] * toward[2];
    // ITRF → GCRF (the transpose of the library's GCRS → ITRS matrix), so the
    // globe's lines turn with the Earth.
    const m = now.earth;
    const earthMvp = mul(mvp, f32([m[0], m[1], m[2], 0, m[3], m[4], m[5], 0, m[6], m[7], m[8], 0, 0, 0, 0, 1]));

    // The globe, lit by the Sun, hides what is behind it.
    g.mesh(() => sphere(R_EARTH), mvp, rgba(c.accent, 1).map((x, i) => (i < 3 ? 0.82 + 0.18 * x : 1)), { depth: true, depthWrite: true, sun: now.sun, night: rgba(c.accent).map((x, i) => (i < 3 ? 0.35 + 0.25 * x : 1)) }, "globe");
    // Graticule and coastlines in ITRF, just above the surface.
    g.lines(() => {
      const out = [], col = rgba(c.muted, 0.7), lift = R_EARTH * 1.002;
      for (let lon = 0; lon < 360; lon += 30) {
        const pts = [];
        for (let lat = -90; lat <= 90; lat += 3) pts.push(sph(lat * DEG, lon * DEG, lift));
        line(out, pts, col, 0.8);
      }
      for (let lat = -60; lat <= 60; lat += 30) {
        const pts = [];
        for (let lon = 0; lon <= 360; lon += 3) pts.push(sph(lat * DEG, lon * DEG, lift));
        line(out, pts, col, 0.8);
      }
      if (land) {
        const fg = rgba(c.fg, 0.9);
        for (const ring of land.rings) {
          const pts = [];
          for (let k = 0; k < ring.length; k += 2) pts.push(sph(ring[k + 1] * DEG, ring[k] * DEG, lift));
          line(out, pts, fg, 1);
        }
      }
      return f32(out);
    }, earthMvp, s, { depth: true }, "globe-lines:" + c.muted + c.fg + (land ? 1 : 0));

    // Axes, the Sun, the orbit.
    const dyn = [];
    line(dyn, [[0, 0, 0], [rmax * 1.05, 0, 0]], rgba(c.muted), 1, 7);
    line(dyn, [[0, 0, 0], [0, 0, rmax * 0.9]], rgba(c.muted), 1, 7);
    // The orbit: grey in the Earth's shadow, dashed below the equator.
    let along = 0;
    for (let k = 1; k < path.length; k++) {
      const a = path[k - 1], b = path[k];
      const col = b[3] < 0.5 ? rgba(c.muted) : rgba(c.accent);
      const below = a[2] + b[2] < 0;
      dyn.push(a[0], a[1], a[2], b[0], b[1], b[2], ...col, 2.2, below ? 9 : 0, along);
      along += Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    }
    const sat = now.pos, v = now.vel, vs = (0.08 * rmax) / Math.max(Math.hypot(...v), 1e-9);
    line(dyn, [sat, [sat[0] + v[0] * vs, sat[1] + v[1] * vs, sat[2] + v[2] * vs]], rgba(c.hot), 1.8);
    const lines = f32(dyn);
    // Twice: faint and without the depth test, so what passes behind the
    // Earth still shows through it; then at full strength with the depth
    // test, so the globe hides the rest of that faint copy.
    const faint = lines.slice();
    for (let k = 9; k < faint.length; k += 13) faint[k] *= 0.2;
    g.lines(faint, mvp, s, { depth: false });
    g.lines(lines, mvp, s, { depth: true });
    const pts = [];
    const sunR = rmax * 1.15;
    point(pts, [now.sun[0] * sunR, now.sun[1] * sunR, now.sun[2] * sunR], rgba(c.sun), 14);
    point(pts, now.peri, rgba(c.fg), 6);
    point(pts, sat, rgba(c.hot), 10);
    g.points(f32(pts), mvp, { depth: true });

    // Text.
    const ctx = orbitText();
    ctx.font = "12px sans-serif";
    ctx.fillStyle = c.muted;
    const label = (v, text, dx = 6, dy = -4) => { const [x, y] = project(v); ctx.fillText(text, x + dx, y + dy); };
    label([rmax * 1.05, 0, 0], "x (GCRF)", 4, 0);
    label([0, 0, rmax * 0.9], "z", 4, 0);
    label([now.sun[0] * sunR, now.sun[1] * sunR, now.sun[2] * sunR], "Sun", 10, 4);
    const behind = (q) => {
      const [x, y] = project(q);
      return Math.hypot(x - w / 2, y - h / 2) < R_EARTH * s && depth(q) < 0;
    };
    if (!behind(now.peri)) {
      ctx.fillStyle = c.fg;
      label(now.peri, root.classList.contains("hv-tle-mode") ? "lowest point" : "periapsis");
    }
    ctx.fillStyle = c.muted;
    ctx.fillText("drag to rotate · scroll to zoom · dashed: below the equator · grey: in the Earth's shadow", 8, h - 8);
  }

  // --- The map ----------------------------------------------------------------
  // Longitude/latitude → clip space: x = lon/180, y = lat/90.
  const MAP = new Float32Array([1 / 180, 0, 0, 0, 0, 1 / 90, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);

  // Land rings, unwrapped across ±180° and closed through the pole when they
  // encircle one (Antarctica), at three longitude offsets.
  let landRings = null;
  function rings() {
    if (landRings || !land) return landRings || [];
    landRings = [];
    for (const r of land.rings) {
      const pts = [];
      let off = 0;
      for (let k = 0; k < r.length; k += 2) {
        if (k > 0) {
          const d = r[k] - r[k - 2];
          if (d > 180) off -= 360; else if (d < -180) off += 360;
        }
        pts.push([r[k] + off, r[k + 1]]);
      }
      if (Math.abs(pts[pts.length - 1][0] - pts[0][0]) > 180) {
        const pole = pts.reduce((a, p) => a + p[1], 0) < 0 ? -90 : 90;
        pts.push([pts[pts.length - 1][0], pole], [pts[0][0], pole]);
      }
      for (const shift of [-360, 0, 360]) landRings.push(pts.map(([lo, la]) => [lo + shift, la]));
    }
    return landRings;
  }

  function drawTrack(c, now) {
    const g = trackGL;
    const [w, h] = g.begin();
    const px = w / 360; // CSS px per degree of longitude

    // Land: filled (even-odd through the stencil), then outlined.
    if (land) {
      g.fillEvenOdd(() => {
        const out = [];
        for (const r of rings()) for (let k = 1; k + 1 < r.length; k++) out.push(r[0][0], r[0][1], 0, r[k][0], r[k][1], 0, r[k + 1][0], r[k + 1][1], 0);
        return f32(out);
      }, MAP, rgba(c.accent, 0.14), "land-fill");
      g.lines(() => {
        const out = [];
        for (const r of rings()) line(out, [...r, r[0]], rgba(c.fg), 0.8);
        return f32(out);
      }, MAP, px, {}, "land-lines:" + c.fg);
    }
    // Graticule.
    {
      const out = [], col = rgba(c.muted, 0.8);
      for (let lon = -180; lon <= 180; lon += 30) line(out, [[lon, -90], [lon, 90]], col, 0.7);
      for (let lat = -60; lat <= 60; lat += 30) line(out, [[-180, lat], [180, lat]], col, lat === 0 ? 1.3 : 0.7);
      g.lines(f32(out), MAP, px);
    }
    // Night side: between the library's terminator (west to east) and the
    // pole in darkness, one quad per step in longitude.
    const [sLat, sLon] = now.term.slice(0, 2);
    {
      const pts = chunk(now.term.slice(2), 2);
      const pole = sLat > 0 ? -90 : 90, out = [];
      for (let k = 1; k < pts.length; k++) {
        const [la0, lo0] = pts[k - 1], [la1, lo1] = pts[k];
        out.push(lo0, la0, 0, lo1, la1, 0, lo1, pole, 0, lo0, la0, 0, lo1, pole, 0, lo0, pole, 0);
      }
      g.mesh(f32(out), MAP, [0, 0, 0, 0.28]);
    }
    const dyn = [];
    // Ground track: orange above London's horizon, dashed dark in the
    // Earth's shadow.
    let dist = 0;
    for (let k = 1; k < track.length; k++) {
      const [la0, lo0] = track[k - 1], [la1, lo1, el1, lit1] = track[k];
      if (Math.abs(lo1 - lo0) > 180) continue;
      const dark = lit1 < 0.5 && el1 <= 0;
      const col = el1 > 0 ? rgba(c.hot) : dark ? rgba(c.fg) : rgba(c.accent);
      dyn.push(lo0, la0, 0, lo1, la1, 0, ...col, el1 > 0 ? 3 : dark ? 2 : 1.5, dark ? 6 : 0, dist);
      dist += Math.hypot(lo1 - lo0, la1 - la0);
    }
    // Coverage circle: where the satellite is above the horizon.
    {
      const lat = now.lat * DEG, lon = now.lon * DEG;
      const lam = Math.acos(R_EARTH / (R_EARTH + Math.max(now.alt, 1)));
      let prev = null, run = [];
      const flush = () => { if (run.length > 1) line(dyn, run, rgba(c.hot), 1.2, 7); run = []; };
      for (let k = 0; k <= 180; k++) {
        const b = (2 * Math.PI * k) / 180;
        const la = Math.asin(Math.sin(lat) * Math.cos(lam) + Math.cos(lat) * Math.sin(lam) * Math.cos(b));
        const lo = lon + Math.atan2(Math.sin(b) * Math.sin(lam) * Math.cos(lat), Math.cos(lam) - Math.sin(lat) * Math.sin(la));
        const x = ((lo / DEG + 540) % 360) - 180;
        if (prev !== null && Math.abs(x - prev) > 180) flush();
        run.push([x, la / DEG]);
        prev = x;
      }
      flush();
    }
    // A constellation's inter-satellite links.
    if (now.cons) {
      for (const [a, b] of now.cons.links) {
        const [pa, pb] = [now.cons.sats[a], now.cons.sats[b]];
        if (Math.abs(pa[1] - pb[1]) > 180) continue; // across the map's edge
        line(dyn, [[pa[1], pa[0]], [pb[1], pb[0]]], rgba(c.hot, 0.6), 1);
      }
    }
    g.lines(f32(dyn), MAP, px);

    // Points: constellation, sites, London, the Sun and the satellite.
    const pts = [];
    if (now.cons) for (const [la, lo, lit] of now.cons.sats) point(pts, [lo, la], lit < 0.5 ? rgba(c.muted) : rgba(c.accent), 8);
    sites.forEach((st, k) => {
      const up = now.cons ? now.cons.siteUp[k] > 0 : now.siteEl[k] > 10;
      point(pts, [st.lon, st.lat], up ? rgba(c.hot) : rgba(c.fg), 6, true);
    });
    point(pts, [-0.1276, 51.5072], rgba(c.fg), 7);
    point(pts, [sLon, sLat], rgba(c.sun), 12);
    point(pts, [now.lon, now.lat], rgba(c.hot), 10);
    g.points(f32(pts), MAP);

    // Text.
    const ctx = trackText();
    const X = (lon) => ((lon + 180) / 360) * w, Y = (lat) => ((90 - lat) / 180) * h;
    ctx.font = "11px sans-serif";
    sites.forEach((st, k) => {
      const up = now.cons ? now.cons.siteUp[k] > 0 : now.siteEl[k] > 10;
      ctx.fillStyle = up ? c.hot : c.fg;
      ctx.fillText(st.name, X(st.lon) + 5, Y(st.lat) + 12);
    });
    ctx.font = "12px sans-serif";
    ctx.fillStyle = c.fg;
    ctx.fillText("London", X(-0.1276) + 6, Y(51.5072) - 5);
  }

  function fmtTime(s) {
    const sign = s < 0 ? "−" : "+";
    s = Math.abs(s);
    const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), sec = Math.floor(s % 60);
    return `${sign}${h}h ${String(m).padStart(2, "0")}m ${String(sec).padStart(2, "0")}s`;
  }

  function frame(ts) {
    const dt = lastFrame === null ? 0 : (ts - lastFrame) / 1000;
    lastFrame = ts;
    if (playing) {
      t += dt * +$("hv-speed").value;
      if (t > +$("hv-time").max) t = 0;
      $("hv-time").value = t;
    }
    try {
      const s = call("state", t);
      const now = {
        pos: s.slice(0, 3), vel: s.slice(3, 6), lat: s[6], lon: s[7], alt: s[8],
        el: s[9], az: s[10], range: s[11], era: s[12], peri: s.slice(13, 16),
        lit: s[16], shadow: s[17], sunEl: s[18], sun: s.slice(21, 24),
        earth: call("earth_frame", t),
        term: call("terminator", t, 181),
        siteEl: call("site_view", t),
        cons: null,
      };
      if (walker) {
        // [n, n × (lat, lon, lit), m, m × (a, b), per-site counts]
        const r = call("constellation", walker.total, walker.planes, walker.phasing, t);
        const n = r[0], m = r[1 + 3 * n];
        now.cons = {
          sats: chunk(r.slice(1, 1 + 3 * n), 3),
          links: chunk(r.slice(2 + 3 * n, 2 + 3 * n + 2 * m), 2),
          siteUp: r.slice(2 + 3 * n + 2 * m),
        };
      }
      const c = colours();
      drawOrbit(c, now);
      drawTrack(c, now);
      const speed = Math.hypot(...now.vel);
      const visible = now.el > 0;
      $("hv-readout").innerHTML =
        `<b>${fmtTime(t)}</b> after epoch &nbsp;·&nbsp; period ${(info[0] / 60).toFixed(1)} min &nbsp;·&nbsp; ` +
        `perigee ${info[1].toFixed(0)} km, apogee ${info[2].toFixed(0)} km<br>` +
        `altitude <b>${now.alt.toFixed(1)} km</b> &nbsp;·&nbsp; speed ${speed.toFixed(3)} km/s &nbsp;·&nbsp; ` +
        `sub-satellite ${now.lat.toFixed(2)}°, ${now.lon.toFixed(2)}° &nbsp;·&nbsp; ERA ${now.era.toFixed(2)}°<br>` +
        `from London: elevation <b class="${visible ? "hv-up" : ""}">${now.el.toFixed(1)}°</b>, azimuth ${now.az.toFixed(1)}°, ` +
        `range ${now.range.toFixed(0)} km — ${visible ? "<b class=\"hv-up\">above the horizon</b>" : "below the horizon"}<br>` +
        `${utc(t)} UTC &nbsp;·&nbsp; satellite ${["in sunlight", "in the penumbra", "in the Earth's shadow", "in the antumbra"][now.shadow]}` +
        `${now.shadow === 1 ? " (" + (100 * now.lit).toFixed(0) + "% of the Sun)" : ""} &nbsp;·&nbsp; ` +
        `Sun at London ${now.sunEl.toFixed(1)}° (${now.sunEl > 0 ? "day" : now.sunEl > -6 ? "twilight" : "dark"})<br>` +
        (now.cons
          ? `constellation: ${now.cons.sats.length} satellites (Walker ${walker.total}/${walker.planes}/${walker.phasing}), ` +
            `${now.cons.sats.filter((s) => s[2] >= 0.5).length} in sunlight, ${now.cons.links.length} inter-satellite links &nbsp;·&nbsp; ` +
            `data centres with a satellite above 25°: ${sites.filter((_, k) => now.cons.siteUp[k] > 0).map((st) => st.name).join(", ") || "none"}`
          : `data centres in view (above 10°): ${sites.filter((_, k) => now.siteEl[k] > 10).map((st) => st.name).join(", ") || "none"}`);
    } catch (err) {
      $("hv-status").textContent = err.message;
    }
    requestAnimationFrame(frame);
  }

  setControls(presets.iss);
  load();
})();
