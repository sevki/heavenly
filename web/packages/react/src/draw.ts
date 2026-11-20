// The two views, drawn with the WebGL renderer (gl.ts) plus a 2D canvas for
// text. All positions come from the library; this only turns them into
// geometry.

import { line, mul, point, rgba, sphere, type Renderer, type Rgba, type Theme } from "./gl.js";
import type { PathPoint, Site, Snapshot, TrackPoint, Vec3 } from "./wasm.js";

export const R_EARTH_KM = 6378.137;
const DEG = Math.PI / 180;
const f32 = (a: number[]) => new Float32Array(a);
const sph = (lat: number, lon: number, r = R_EARTH_KM): Vec3 => [r * Math.cos(lat) * Math.cos(lon), r * Math.cos(lat) * Math.sin(lon), r * Math.sin(lat)];
const shade = (c: Rgba, base: number, k: number): Rgba => [base + k * c[0], base + k * c[1], base + k * c[2], 1];

/** The coastlines: Natural Earth land as rings of `[lon, lat, lon, lat, …]`. */
export interface Land {
  rings: number[][];
}

/** Where the camera looks from, and how close. */
export interface GlobeView {
  /** Azimuth and elevation of the viewpoint, radians. */
  az: number;
  el: number;
  zoom: number;
}

export interface GlobeScene {
  snapshot: Snapshot;
  path: PathPoint[];
  land: Land | null;
  theme: Theme;
  view: GlobeView;
  /** Label for the marked low point: "periapsis", or for SGP4 "lowest point". */
  lowPointLabel: string;
}

/** Draws the 3D view: a globe lit by the Sun, its graticule and coastlines turning with the Earth, and the orbit. */
export function drawGlobe(g: Renderer, text: CanvasRenderingContext2D, scene: GlobeScene): void {
  const { snapshot: now, path, land, theme: c, view } = scene;
  const [w, h] = g.begin();
  const st = now.state;
  const rmax = Math.max(...path.map((q) => Math.hypot(...q.position)), R_EARTH_KM * 1.5);
  const right: Vec3 = [-Math.sin(view.az), Math.cos(view.az), 0];
  const up: Vec3 = [-Math.sin(view.el) * Math.cos(view.az), -Math.sin(view.el) * Math.sin(view.az), Math.cos(view.el)];
  const toward: Vec3 = [Math.cos(view.el) * Math.cos(view.az), Math.cos(view.el) * Math.sin(view.az), Math.sin(view.el)];
  const s = (view.zoom * 0.45 * Math.min(w, h)) / rmax; // CSS px per km
  // Orthographic: x, y in clip space; z from the distance along the view.
  const kx = (2 * s) / w, ky = (2 * s) / h, kz = 1 / (4 * rmax);
  const mvp = f32([
    right[0] * kx, up[0] * ky, -toward[0] * kz, 0,
    right[1] * kx, up[1] * ky, -toward[1] * kz, 0,
    right[2] * kx, up[2] * ky, -toward[2] * kz, 0,
    0, 0, 0, 1,
  ]);
  const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const project = (v: Vec3): [number, number] => [w / 2 + dot(v, right) * s, h / 2 - dot(v, up) * s];
  // ITRF → GCRF (the transpose of the library's GCRS → ITRS), so the globe's
  // lines turn with the Earth.
  const m = now.earth;
  const earthMvp = mul(mvp, f32([m[0], m[1], m[2], 0, m[3], m[4], m[5], 0, m[6], m[7], m[8], 0, 0, 0, 0, 1]));

  // The globe, lit by the Sun; it hides what is behind it.
  const accent = rgba(c.accent);
  g.mesh(() => sphere(R_EARTH_KM), mvp, shade(accent, 0.82, 0.18), { depth: true, depthWrite: true, sun: st.sunDirection, night: shade(accent, 0.35, 0.25) }, "globe");
  // Graticule and coastlines in ITRF, just above the surface.
  g.lines(
    () => {
      const out: number[] = [], col = rgba(c.muted, 0.7), lift = R_EARTH_KM * 1.002;
      for (let lon = 0; lon < 360; lon += 30) {
        const pts: Vec3[] = [];
        for (let lat = -90; lat <= 90; lat += 3) pts.push(sph(lat * DEG, lon * DEG, lift));
        line(out, pts, col, 0.8);
      }
      for (let lat = -60; lat <= 60; lat += 30) {
        const pts: Vec3[] = [];
        for (let lon = 0; lon <= 360; lon += 3) pts.push(sph(lat * DEG, lon * DEG, lift));
        line(out, pts, col, 0.8);
      }
      if (land) {
        const fg = rgba(c.fg, 0.9);
        for (const ring of land.rings) {
          const pts: Vec3[] = [];
          for (let k = 0; k < ring.length; k += 2) pts.push(sph(ring[k + 1] * DEG, ring[k] * DEG, lift));
          line(out, pts, fg, 1);
        }
      }
      return f32(out);
    },
    earthMvp,
    s,
    { depth: true },
    `globe-lines:${c.muted}:${c.fg}:${land ? 1 : 0}`,
  );

  // Axes, the orbit (grey in the Earth's shadow, dashed below the equator)
  // and the velocity.
  const dyn: number[] = [];
  line(dyn, [[0, 0, 0], [rmax * 1.05, 0, 0]], rgba(c.muted), 1, 7);
  line(dyn, [[0, 0, 0], [0, 0, rmax * 0.9]], rgba(c.muted), 1, 7);
  let along = 0;
  for (let k = 1; k < path.length; k++) {
    const a = path[k - 1].position, b = path[k].position;
    const col = path[k].sunlit < 0.5 ? rgba(c.muted) : accent;
    dyn.push(...a, ...b, ...col, 2.2, a[2] + b[2] < 0 ? 9 : 0, along);
    along += Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]);
  }
  const sat = st.position, v = st.velocity, vs = (0.08 * rmax) / Math.max(Math.hypot(...v), 1e-9);
  line(dyn, [sat, [sat[0] + v[0] * vs, sat[1] + v[1] * vs, sat[2] + v[2] * vs]], rgba(c.hot), 1.8);
  const lines = f32(dyn);
  // Twice: faint without the depth test, so what passes behind the Earth
  // shows through; then full strength with it, so the globe hides the rest.
  const faint = lines.slice();
  for (let k = 9; k < faint.length; k += 13) faint[k] *= 0.2;
  g.lines(faint, mvp, s, { depth: false });
  g.lines(lines, mvp, s, { depth: true });
  const pts: number[] = [];
  const sunR = rmax * 1.15;
  const sunPos: Vec3 = [st.sunDirection[0] * sunR, st.sunDirection[1] * sunR, st.sunDirection[2] * sunR];
  point(pts, sunPos, rgba(c.sun), 14);
  point(pts, st.periapsis, rgba(c.fg), 6);
  point(pts, sat, rgba(c.hot), 10);
  g.points(f32(pts), mvp, { depth: true });

  // Text.
  text.font = "12px sans-serif";
  text.fillStyle = c.muted;
  const label = (q: Vec3, s2: string, dx = 6, dy = -4) => {
    const [x, y] = project(q);
    text.fillText(s2, x + dx, y + dy);
  };
  label([rmax * 1.05, 0, 0], "x (GCRF)", 4, 0);
  label([0, 0, rmax * 0.9], "z", 4, 0);
  label(sunPos, "Sun", 10, 4);
  const [px, py] = project(st.periapsis);
  const hidden = Math.hypot(px - w / 2, py - h / 2) < R_EARTH_KM * s && dot(st.periapsis, toward) < 0;
  if (!hidden) {
    text.fillStyle = c.fg;
    label(st.periapsis, scene.lowPointLabel);
  }
  text.fillStyle = c.muted;
  text.fillText("drag to rotate · scroll to zoom · dashed: below the equator · grey: in the Earth's shadow", 8, h - 8);
}

// Longitude/latitude → clip space: x = lon/180, y = lat/90.
const MAP = new Float32Array([1 / 180, 0, 0, 0, 0, 1 / 90, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);

const ringCache = new WeakMap<Land, [number, number][][]>();
// Land rings, unwrapped across ±180° and closed through the pole when they
// encircle one (Antarctica), at three longitude offsets.
function rings(land: Land): [number, number][][] {
  let out = ringCache.get(land);
  if (out) return out;
  out = [];
  for (const r of land.rings) {
    const pts: [number, number][] = [];
    let off = 0;
    for (let k = 0; k < r.length; k += 2) {
      if (k > 0) {
        const d = r[k] - r[k - 2];
        if (d > 180) off -= 360;
        else if (d < -180) off += 360;
      }
      pts.push([r[k] + off, r[k + 1]]);
    }
    if (Math.abs(pts[pts.length - 1][0] - pts[0][0]) > 180) {
      const pole = pts.reduce((a, p) => a + p[1], 0) < 0 ? -90 : 90;
      pts.push([pts[pts.length - 1][0], pole], [pts[0][0], pole]);
    }
    for (const shift of [-360, 0, 360]) out.push(pts.map(([lo, la]) => [lo + shift, la]));
  }
  ringCache.set(land, out);
  return out;
}

export interface MapScene {
  snapshot: Snapshot;
  track: TrackPoint[];
  sites: Site[];
  land: Land | null;
  theme: Theme;
  /** The ground station shown with its name. */
  station: { name: string; lat: number; lon: number };
}

/** Draws the map: land, night side, ground track, coverage, sites and constellation. */
export function drawMap(g: Renderer, text: CanvasRenderingContext2D, scene: MapScene): void {
  const { snapshot: now, track, sites, land, theme: c, station } = scene;
  const [w, h] = g.begin();
  const px = w / 360; // CSS px per degree of longitude
  const st = now.state;

  // Land: filled (even-odd through the stencil), then outlined.
  if (land) {
    g.fillEvenOdd(
      () => {
        const out: number[] = [];
        for (const r of rings(land)) for (let k = 1; k + 1 < r.length; k++) out.push(r[0][0], r[0][1], 0, r[k][0], r[k][1], 0, r[k + 1][0], r[k + 1][1], 0);
        return f32(out);
      },
      MAP,
      rgba(c.accent, 0.14),
      "land-fill",
    );
    g.lines(
      () => {
        const out: number[] = [];
        for (const r of rings(land)) line(out, [...r, r[0]], rgba(c.fg), 0.8);
        return f32(out);
      },
      MAP,
      px,
      {},
      `land-lines:${c.fg}`,
    );
  }
  // Graticule.
  {
    const out: number[] = [], col = rgba(c.muted, 0.8);
    for (let lon = -180; lon <= 180; lon += 30) line(out, [[lon, -90], [lon, 90]], col, 0.7);
    for (let lat = -60; lat <= 60; lat += 30) line(out, [[-180, lat], [180, lat]], col, lat === 0 ? 1.3 : 0.7);
    g.lines(f32(out), MAP, px);
  }
  // Night side: between the terminator (west to east) and the pole in
  // darkness, one quad per step in longitude.
  const { subsolar, points } = now.terminator;
  {
    const pole = subsolar.lat > 0 ? -90 : 90, out: number[] = [];
    for (let k = 1; k < points.length; k++) {
      const [la0, lo0] = points[k - 1], [la1, lo1] = points[k];
      out.push(lo0, la0, 0, lo1, la1, 0, lo1, pole, 0, lo0, la0, 0, lo1, pole, 0, lo0, pole, 0);
    }
    g.mesh(f32(out), MAP, [0, 0, 0, 0.28]);
  }
  const dyn: number[] = [];
  // Ground track: orange above the station's horizon, dashed dark in shadow.
  let dist = 0;
  for (let k = 1; k < track.length; k++) {
    const p0 = track[k - 1], p1 = track[k];
    if (Math.abs(p1.lon - p0.lon) > 180) continue;
    const dark = p1.sunlit < 0.5 && p1.elevationDeg <= 0;
    const col = p1.elevationDeg > 0 ? rgba(c.hot) : dark ? rgba(c.fg) : rgba(c.accent);
    dyn.push(p0.lon, p0.lat, 0, p1.lon, p1.lat, 0, ...col, p1.elevationDeg > 0 ? 3 : dark ? 2 : 1.5, dark ? 6 : 0, dist);
    dist += Math.hypot(p1.lon - p0.lon, p1.lat - p0.lat);
  }
  // Coverage circle: where the satellite is above the horizon.
  {
    const lat = st.lat * DEG, lon = st.lon * DEG;
    const lam = Math.acos(R_EARTH_KM / (R_EARTH_KM + Math.max(st.altitudeKm, 1)));
    let prev: number | null = null;
    let run: [number, number][] = [];
    const flush = () => {
      if (run.length > 1) line(dyn, run, rgba(c.hot), 1.2, 7);
      run = [];
    };
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
  const cons = now.constellation;
  if (cons) {
    for (const [a, b] of cons.links) {
      const pa = cons.satellites[a], pb = cons.satellites[b];
      if (Math.abs(pa.lon - pb.lon) > 180) continue; // across the map's edge
      line(dyn, [[pa.lon, pa.lat], [pb.lon, pb.lat]], rgba(c.hot, 0.6), 1);
    }
  }
  g.lines(f32(dyn), MAP, px);

  // Points: constellation, sites, the station, the Sun and the satellite.
  const covered = (k: number) => (cons ? cons.sitesCovered[k] > 0 : now.siteElevationsDeg[k] > 10);
  const pts: number[] = [];
  if (cons) for (const q of cons.satellites) point(pts, [q.lon, q.lat], q.sunlit < 0.5 ? rgba(c.muted) : rgba(c.accent), 8);
  // A site that is also the station is drawn (and labelled) as the station.
  const isStation = (site: Site) => site.lat === station.lat && site.lon === station.lon;
  sites.forEach((site, k) => isStation(site) || point(pts, [site.lon, site.lat], covered(k) ? rgba(c.hot) : rgba(c.fg), 6, true));
  point(pts, [station.lon, station.lat], rgba(c.fg), 7);
  point(pts, [subsolar.lon, subsolar.lat], rgba(c.sun), 12);
  point(pts, [st.lon, st.lat], rgba(c.hot), 10);
  g.points(f32(pts), MAP);

  // Text.
  const X = (lon: number) => ((lon + 180) / 360) * w, Y = (lat: number) => ((90 - lat) / 180) * h;
  text.font = "11px sans-serif";
  sites.forEach((site, k) => {
    if (isStation(site)) return;
    text.fillStyle = covered(k) ? c.hot : c.fg;
    text.fillText(site.name, X(site.lon) + 5, Y(site.lat) + 12);
  });
  text.font = "12px sans-serif";
  text.fillStyle = c.fg;
  text.fillText(station.name, X(station.lon) + 6, Y(station.lat) - 5);
}
