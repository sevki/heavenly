// heavenly playground: the physics runs in WebAssembly (crate `heavenly-viz`,
// built from the heavenly crates); this file only draws and wires controls.
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
    const rows = passes.map(([a, b, top, vis], k) =>
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
  // Dawn–dusk sun-synchronous orbit at 550 km for the chosen epoch: the
  // library gives the inclination and the node that faces the Sun.
  // A Walker 24/4/1 shell built from the slider orbit; the sliders keep
  // shaping it (altitude, inclination, node).
  $("hv-preset-walker").addEventListener("click", () => {
    walker = { total: 24, planes: 4, phasing: 1 };
    setControls({ alt: 550, e: 0, i: 53, raan: 0, argp: 0 });
    t = 0;
    applyOrbit();
  });
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

  // --- Drawing ----------------------------------------------------------------
  function colours() {
    const cs = getComputedStyle(document.documentElement);
    const v = (n, d) => (cs.getPropertyValue(n).trim() || d);
    return { fg: v("--fg", "#333"), accent: v("--links", "#1f6feb"), muted: v("--icons", "#888"), bg: v("--bg", "#fff"), hot: "#e8590c" };
  }

  function fit(canvas) {
    const r = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.round(r.width * dpr), h = Math.round(r.height * dpr);
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    const ctx = canvas.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return [ctx, r.width, r.height];
  }

  function camera(w, h, rmax) {
    const { az, el, zoom } = view;
    const right = [-Math.sin(az), Math.cos(az), 0];
    const up = [-Math.sin(el) * Math.cos(az), -Math.sin(el) * Math.sin(az), Math.cos(el)];
    const toward = [Math.cos(el) * Math.cos(az), Math.cos(el) * Math.sin(az), Math.sin(el)];
    const s = (zoom * 0.45 * Math.min(w, h)) / rmax;
    const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    return {
      s,
      p: (v) => [w / 2 + dot(v, right) * s, h / 2 - dot(v, up) * s],
      depth: (v) => dot(v, toward),
    };
  }

  function drawOrbit(c, now) {
    const [ctx, w, h] = fit(oc);
    ctx.clearRect(0, 0, w, h);
    const rmax = Math.max(...path.map((q) => Math.hypot(q[0], q[1], q[2])), R_EARTH * 1.5);
    const cam = camera(w, h, rmax);
    const [ex, ey] = cam.p([0, 0, 0]);
    const er = R_EARTH * cam.s;
    // Earth disc.
    ctx.beginPath(); ctx.arc(ex, ey, er, 0, 2 * Math.PI);
    ctx.fillStyle = c.accent; ctx.globalAlpha = 0.12; ctx.fill(); ctx.globalAlpha = 1;
    ctx.strokeStyle = c.fg; ctx.lineWidth = 1.2; ctx.stroke();
    // Rotating graticule: ITRF lines mapped to GCRF with the Earth-orientation matrix.
    const m = now.earth; // GCRS→ITRS, row-major; ITRS→GCRS is its transpose
    const toGcrs = (v) => [
      m[0] * v[0] + m[3] * v[1] + m[6] * v[2],
      m[1] * v[0] + m[4] * v[1] + m[7] * v[2],
      m[2] * v[0] + m[5] * v[1] + m[8] * v[2],
    ];
    const sph = (lat, lon) => [R_EARTH * Math.cos(lat) * Math.cos(lon), R_EARTH * Math.cos(lat) * Math.sin(lon), R_EARTH * Math.sin(lat)];
    ctx.strokeStyle = c.muted; ctx.lineWidth = 0.7;
    const strokeFront = (pts) => {
      ctx.beginPath(); let pen = false;
      for (const v of pts) {
        const g = toGcrs(v);
        if (cam.depth(g) < 0) { pen = false; continue; }
        const [x, y] = cam.p(g);
        pen ? ctx.lineTo(x, y) : ctx.moveTo(x, y); pen = true;
      }
      ctx.stroke();
    };
    for (let lon = 0; lon < 360; lon += 30) {
      const pts = []; for (let lat = -90; lat <= 90; lat += 5) pts.push(sph(lat * DEG, lon * DEG)); strokeFront(pts);
    }
    for (let lat = -60; lat <= 60; lat += 30) {
      const pts = []; for (let lon = 0; lon <= 360; lon += 5) pts.push(sph(lat * DEG, lon * DEG)); strokeFront(pts);
    }
    if (land) {
      ctx.strokeStyle = c.fg; ctx.lineWidth = 0.8;
      for (const r of land.rings) {
        const pts = []; for (let k = 0; k < r.length; k += 2) pts.push(sph(r[k + 1] * DEG, r[k] * DEG)); strokeFront(pts);
      }
    }
    // Axes.
    const axis = (v, label) => {
      const [x, y] = cam.p(v);
      ctx.strokeStyle = c.muted; ctx.lineWidth = 1; ctx.setLineDash([4, 3]);
      ctx.beginPath(); ctx.moveTo(ex, ey); ctx.lineTo(x, y); ctx.stroke(); ctx.setLineDash([]);
      ctx.fillStyle = c.muted; ctx.font = "12px sans-serif"; ctx.fillText(label, x + 4, y);
    };
    axis([rmax * 1.05, 0, 0], "x (GCRF)");
    axis([0, 0, rmax * 0.9], "z");
    // The Sun's direction.
    {
      const d = now.sun, r = rmax * 1.15;
      const [x, y] = cam.p([d[0] * r, d[1] * r, d[2] * r]);
      ctx.fillStyle = "#f2b705"; ctx.beginPath(); ctx.arc(x, y, 7, 0, 2 * Math.PI); ctx.fill();
      ctx.fillStyle = c.muted; ctx.font = "12px sans-serif"; ctx.fillText("Sun", x + 9, y + 4);
    }
    // Orbit: hidden where it passes behind the Earth.
    ctx.lineWidth = 2; ctx.strokeStyle = c.accent;
    for (let k = 1; k < path.length; k++) {
      const a = path[k - 1], b = path[k];
      const [x0, y0] = cam.p(a), [x1, y1] = cam.p(b);
      const mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2];
      const [mx, my] = cam.p(mid);
      const hidden = Math.hypot(mx - ex, my - ey) < er && cam.depth(mid) < 0;
      const below = mid[2] < 0;
      ctx.strokeStyle = b[3] < 0.5 ? c.muted : c.accent; // grey in the Earth's shadow
      ctx.globalAlpha = hidden ? 0.15 : 1;
      ctx.setLineDash(below ? [5, 4] : []);
      ctx.beginPath(); ctx.moveTo(x0, y0); ctx.lineTo(x1, y1); ctx.stroke();
    }
    ctx.globalAlpha = 1; ctx.setLineDash([]);
    // Periapsis and the satellite.
    const [px, py] = cam.p(now.peri);
    ctx.fillStyle = c.fg; ctx.beginPath(); ctx.arc(px, py, 3, 0, 2 * Math.PI); ctx.fill();
    ctx.font = "12px sans-serif"; ctx.fillText(root.classList.contains("hv-tle-mode") ? "lowest point" : "periapsis", px + 6, py - 4);
    const sat = now.pos;
    const [sx, sy] = cam.p(sat);
    const behind = Math.hypot(sx - ex, sy - ey) < er && cam.depth(sat) < 0;
    ctx.globalAlpha = behind ? 0.3 : 1;
    ctx.fillStyle = c.hot; ctx.beginPath(); ctx.arc(sx, sy, 5, 0, 2 * Math.PI); ctx.fill();
    const v = now.vel, vs = 0.08 * rmax / Math.max(Math.hypot(...v), 1e-9);
    const [vx, vy] = cam.p([sat[0] + v[0] * vs, sat[1] + v[1] * vs, sat[2] + v[2] * vs]);
    ctx.strokeStyle = c.hot; ctx.lineWidth = 1.5; ctx.beginPath(); ctx.moveTo(sx, sy); ctx.lineTo(vx, vy); ctx.stroke();
    ctx.globalAlpha = 1;
    ctx.fillStyle = c.muted; ctx.fillText("drag to rotate · scroll to zoom · dashed: below the equator · grey: in the Earth's shadow", 8, h - 8);
  }

  const mtrack = $("hv-track");
  function drawTrack(c, now) {
    const [ctx, w, h] = fit(mtrack);
    ctx.clearRect(0, 0, w, h);
    const X = (lon) => ((lon + 180) / 360) * w, Y = (lat) => ((90 - lat) / 180) * h;
    ctx.strokeStyle = c.muted; ctx.lineWidth = 0.6;
    for (let lon = -180; lon <= 180; lon += 30) { ctx.beginPath(); ctx.moveTo(X(lon), 0); ctx.lineTo(X(lon), h); ctx.stroke(); }
    for (let lat = -60; lat <= 60; lat += 30) {
      ctx.lineWidth = lat === 0 ? 1.2 : 0.6;
      ctx.beginPath(); ctx.moveTo(0, Y(lat)); ctx.lineTo(w, Y(lat)); ctx.stroke();
    }
    if (land) {
      ctx.fillStyle = c.accent; ctx.strokeStyle = c.fg; ctx.lineWidth = 0.7;
      for (const r of land.rings) {
        // Unwrap longitudes so rings crossing ±180° stay continuous; a ring
        // that encircles a pole (Antarctica) is closed through that pole.
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
        // Draw at three offsets; the canvas clips whatever falls outside.
        for (const shift of [-360, 0, 360]) {
          ctx.beginPath();
          pts.forEach(([lo, la], k) => (k === 0 ? ctx.moveTo : ctx.lineTo).call(ctx, X(lo + shift), Y(la)));
          ctx.closePath();
          ctx.globalAlpha = 0.14; ctx.fill(); ctx.globalAlpha = 1; ctx.stroke();
        }
      }
    }
    // Night side: the library's terminator runs west to east; close it
    // through the pole that is in darkness.
    {
      const [sLat, sLon] = now.term.slice(0, 2);
      const pts = chunk(now.term.slice(2), 2);
      const pole = sLat > 0 ? -90 : 90;
      ctx.beginPath();
      ctx.moveTo(X(-180), Y(pts[0][0]));
      for (const [la, lo] of pts) ctx.lineTo(X(lo), Y(la));
      ctx.lineTo(X(180), Y(pts[pts.length - 1][0]));
      ctx.lineTo(X(180), Y(pole)); ctx.lineTo(X(-180), Y(pole)); ctx.closePath();
      ctx.fillStyle = "#000"; ctx.globalAlpha = 0.18; ctx.fill(); ctx.globalAlpha = 1;
      ctx.fillStyle = "#f2b705"; ctx.beginPath(); ctx.arc(X(sLon), Y(sLat), 6, 0, 2 * Math.PI); ctx.fill();
    }
    // Ground track: orange above London's horizon, grey in the Earth's shadow.
    ctx.lineWidth = 1.6;
    for (let k = 1; k < track.length; k++) {
      const [la0, lo0] = track[k - 1], [la1, lo1, el1, lit1] = track[k];
      if (Math.abs(lo1 - lo0) > 180) continue;
      ctx.strokeStyle = el1 > 0 ? c.hot : lit1 < 0.5 ? c.fg : c.accent;
      ctx.lineWidth = el1 > 0 ? 3 : lit1 < 0.5 ? 2 : 1.4;
      ctx.setLineDash(lit1 < 0.5 && el1 <= 0 ? [3, 3] : []);
      ctx.beginPath(); ctx.moveTo(X(lo0), Y(la0)); ctx.lineTo(X(lo1), Y(la1)); ctx.stroke();
    }
    ctx.setLineDash([]);
    // Coverage circle: where the satellite is above the horizon.
    const lat = now.lat * DEG, lon = now.lon * DEG;
    const lam = Math.acos(R_EARTH / (R_EARTH + Math.max(now.alt, 1)));
    ctx.strokeStyle = c.hot; ctx.lineWidth = 1; ctx.setLineDash([4, 3]);
    let prev = null;
    ctx.beginPath();
    for (let k = 0; k <= 180; k++) {
      const b = (2 * Math.PI * k) / 180;
      const la = Math.asin(Math.sin(lat) * Math.cos(lam) + Math.cos(lat) * Math.sin(lam) * Math.cos(b));
      const lo = lon + Math.atan2(Math.sin(b) * Math.sin(lam) * Math.cos(lat), Math.cos(lam) - Math.sin(lat) * Math.sin(la));
      const x = X(((lo / DEG + 540) % 360) - 180), y = Y(la / DEG);
      if (prev && Math.abs(x - prev) < w / 2) ctx.lineTo(x, y); else ctx.moveTo(x, y);
      prev = x;
    }
    ctx.stroke(); ctx.setLineDash([]);
    // A constellation: links, then satellites (grey in the Earth's shadow).
    if (now.cons) {
      ctx.strokeStyle = c.hot; ctx.lineWidth = 0.8; ctx.globalAlpha = 0.6;
      for (const [a, b] of now.cons.links) {
        const [pa, pb] = [now.cons.sats[a], now.cons.sats[b]];
        if (Math.abs(pa[1] - pb[1]) > 180) continue; // across the map's edge
        ctx.beginPath(); ctx.moveTo(X(pa[1]), Y(pa[0])); ctx.lineTo(X(pb[1]), Y(pb[0])); ctx.stroke();
      }
      ctx.globalAlpha = 1;
      for (const [la, lo, lit] of now.cons.sats) {
        ctx.fillStyle = lit < 0.5 ? c.muted : c.accent;
        ctx.beginPath(); ctx.arc(X(lo), Y(la), 4, 0, 2 * Math.PI); ctx.fill();
      }
    }
    // Demo data centres: orange when the satellite (or, for a
    // constellation, any satellite above 25°) is in view.
    ctx.font = "11px sans-serif";
    sites.forEach((st, k) => {
      const up = now.cons ? now.cons.siteUp[k] > 0 : now.siteEl[k] > 10;
      ctx.fillStyle = up ? c.hot : c.fg;
      ctx.fillRect(X(st.lon) - 3, Y(st.lat) - 3, 6, 6);
      ctx.fillText(st.name, X(st.lon) + 5, Y(st.lat) + 12);
    });
    // London and the sub-satellite point.
    ctx.fillStyle = c.fg; ctx.beginPath(); ctx.arc(X(-0.1276), Y(51.5072), 3.5, 0, 2 * Math.PI); ctx.fill();
    ctx.font = "12px sans-serif"; ctx.fillText("London", X(-0.1276) + 6, Y(51.5072) - 5);
    ctx.fillStyle = c.hot; ctx.beginPath(); ctx.arc(X(now.lon), Y(now.lat), 5, 0, 2 * Math.PI); ctx.fill();
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
