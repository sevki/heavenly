// A small WebGL2 renderer, written by hand: lines are instanced quads expanded
// in screen space (so they can be thick and dashed, which GL lines cannot),
// points are instanced sprites, fills are triangles, and concave fills use the
// stencil buffer's even-odd rule, so no triangulation is needed.

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

export type Rgba = [number, number, number, number];
type Data = Float32Array | (() => Float32Array);
interface Opts {
  depth?: boolean;
  depthWrite?: boolean;
  sun?: number[];
  night?: Rgba;
}

/** A WebGL2 renderer bound to one canvas. */
export interface Renderer {
  dpr: number;
  /** Sizes the drawing buffer to the canvas and clears it; returns the CSS size. */
  begin(): [number, number];
  /** Line instances: 13 floats each (a, b, rgba, width px, dash px, distance). */
  lines(data: Data, mvp: Float32Array, pxPerUnit: number, opts?: Opts, key?: string): void;
  /** Point instances: 9 floats each (position, rgba, diameter px, shape). */
  points(data: Float32Array, mvp: Float32Array, opts?: Opts): void;
  /** Triangles (3 floats per vertex) in one colour, optionally shaded by a Sun direction. */
  mesh(data: Data, mvp: Float32Array, color: Rgba, opts?: Opts, key?: string): void;
  /** Possibly concave polygons as triangle fans, filled with the even-odd rule. */
  fillEvenOdd(data: Data, mvp: Float32Array, color: Rgba, key?: string): void;
  dispose(): void;
}

export function createRenderer(canvas: HTMLCanvasElement): Renderer | null {
  const context = canvas.getContext("webgl2", { antialias: true, stencil: true, premultipliedAlpha: false });
  if (!context) return null;
  const gl: WebGL2RenderingContext = context;
  const program = (vs: string, fs: string) => {
    const p = gl.createProgram()!;
    for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]] as const) {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader");
      gl.attachShader(p, s);
    }
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? "link");
    const u: Record<string, WebGLUniformLocation | null> = {};
    const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS) as number;
    for (let i = 0; i < n; i++) {
      const name = gl.getActiveUniform(p, i)!.name;
      u[name] = gl.getUniformLocation(p, name);
    }
    return { p, u };
  };
  const progs = { line: program(LINE_VS, LINE_FS), point: program(POINT_VS, POINT_FS), mesh: program(MESH_VS, MESH_FS) };
  const staticBuffer = (data: Float32Array) => {
    const b = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    return b;
  };
  const lineCorners = staticBuffer(new Float32Array([0, -1, 1, -1, 0, 1, 1, 1]));
  const pointCorners = staticBuffer(new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]));
  const cover = staticBuffer(new Float32Array([-1, -1, 0, 1, -1, 0, -1, 1, 0, 1, 1, 0]));
  const identity = new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
  gl.bindVertexArray(gl.createVertexArray());
  const dynamic = gl.createBuffer()!;
  const cache = new Map<string, { buf: WebGLBuffer; length: number }>();

  // Uploads data and returns its buffer; with a key, uploads once and keeps it.
  function buffer(data: Data, key?: string) {
    if (key) {
      let b = cache.get(key);
      if (!b) {
        const d = typeof data === "function" ? data() : data;
        b = { buf: staticBuffer(d), length: d.length };
        cache.set(key, b);
      }
      return b;
    }
    const d = typeof data === "function" ? data() : data;
    gl.bindBuffer(gl.ARRAY_BUFFER, dynamic);
    gl.bufferData(gl.ARRAY_BUFFER, d, gl.STREAM_DRAW);
    return { buf: dynamic, length: d.length };
  }
  function attr(loc: number, buf: WebGLBuffer, size: number, stride: number, offset: number, divisor: number) {
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
  function state(opts: Opts) {
    if (opts.depth) gl.enable(gl.DEPTH_TEST);
    else gl.disable(gl.DEPTH_TEST);
    gl.depthMask(!!opts.depthWrite);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }
  const r: Renderer = {
    dpr: 1,
    begin() {
      const rect = canvas.getBoundingClientRect();
      r.dpr = window.devicePixelRatio || 1;
      const w = Math.max(1, Math.round(rect.width * r.dpr));
      const h = Math.max(1, Math.round(rect.height * r.dpr));
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      gl.viewport(0, 0, w, h);
      gl.clearColor(0, 0, 0, 0);
      gl.clearDepth(1);
      gl.clearStencil(0);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT | gl.STENCIL_BUFFER_BIT);
      return [rect.width, rect.height];
    },
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
      gl.uniform4fv(u.night, opts.night ?? [0, 0, 0, 0]);
      gl.uniform3fv(u.sun, opts.sun ?? [1, 0, 0]);
      gl.drawArrays(gl.TRIANGLES, 0, b.length / 3);
    },
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
    dispose() {
      for (const b of cache.values()) gl.deleteBuffer(b.buf);
      cache.clear();
    },
  };
  return r;
}

// --- Geometry helpers ---------------------------------------------------------

type Pt = [number, number] | [number, number, number];

/** Appends a polyline as line instances (13 floats per segment). */
export function line(out: number[], pts: Pt[], color: Rgba, width: number, dash = 0): void {
  let dist = 0;
  for (let k = 1; k < pts.length; k++) {
    const a = pts[k - 1], b = pts[k];
    const az = a[2] ?? 0, bz = b[2] ?? 0;
    out.push(a[0], a[1], az, b[0], b[1], bz, color[0], color[1], color[2], color[3], width, dash, dist);
    dist += Math.hypot(b[0] - a[0], b[1] - a[1], bz - az);
  }
}

/** Appends a point instance (9 floats). */
export function point(out: number[], p: Pt, color: Rgba, size: number, square = false): void {
  out.push(p[0], p[1], p[2] ?? 0, color[0], color[1], color[2], color[3], size, square ? 1 : 0);
}

/** Column-major 4×4 product `a · b`. */
export function mul(a: Float32Array, b: Float32Array): Float32Array {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++)
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      o[c * 4 + r] = s;
    }
  return o;
}

/** A sphere on a latitude/longitude grid, as triangles. */
export function sphere(radius: number, nLat = 48, nLon = 96): Float32Array {
  const v = (i: number, j: number) => {
    const lat = -Math.PI / 2 + (Math.PI * i) / nLat, lon = (2 * Math.PI * j) / nLon;
    return [radius * Math.cos(lat) * Math.cos(lon), radius * Math.cos(lat) * Math.sin(lon), radius * Math.sin(lat)];
  };
  const out: number[] = [];
  for (let i = 0; i < nLat; i++)
    for (let j = 0; j < nLon; j++) {
      const [a, b, c, d] = [v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)];
      out.push(...a, ...b, ...c, ...a, ...c, ...d);
    }
  return new Float32Array(out);
}

// --- Colours -----------------------------------------------------------------

let swatch: CanvasRenderingContext2D | null = null;
const parsed = new Map<string, Rgba>();

/** A CSS colour as [r, g, b, a] in 0..1, with `alpha` multiplied in. */
export function rgba(css: string, alpha = 1): Rgba {
  let c = parsed.get(css);
  if (!c) {
    swatch ??= document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
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

/** The colours the views use, as CSS colours. */
export interface Theme {
  fg: string;
  accent: string;
  muted: string;
  hot: string;
  sun: string;
}

/**
 * Reads the theme from CSS custom properties on the document (mdBook's
 * `--fg`, `--links`, `--icons`), with fallbacks.
 */
export function themeFromCss(): Theme {
  const cs = getComputedStyle(document.documentElement);
  const v = (n: string, d: string) => cs.getPropertyValue(n).trim() || d;
  return { fg: v("--fg", "#333"), accent: v("--links", "#1f6feb"), muted: v("--icons", "#888"), hot: "#e8590c", sun: "#f2b705" };
}
