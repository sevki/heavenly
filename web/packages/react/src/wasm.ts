// A typed layer over the heavenly WebAssembly module (crate `heavenly-viz`),
// which `wasm-pack` builds into ../wasm with wasm-bindgen. The bindings return
// `Float64Array`s and throw JavaScript errors; this turns the arrays into
// named fields. Every number comes from the Rust library.

import init, { Playground } from "../wasm/heavenly_viz.js";

/** Where to load the WebAssembly module from: a URL, a response, or the bytes. */
export type WasmSource = string | URL | Request | Response | BufferSource | WebAssembly.Module;

export type Vec3 = [number, number, number];

/** Classical orbital elements for the two-body model, in the units the sliders use. */
export interface Elements {
  periapsisAltitudeKm: number;
  eccentricity: number;
  inclinationDeg: number;
  raanDeg: number;
  argPeriapsisDeg: number;
  meanAnomalyDeg?: number;
}

export interface OrbitInfo {
  periodS: number;
  perigeeAltitudeKm: number;
  apogeeAltitudeKm: number;
  perigeeSpeedKmS: number;
  apogeeSpeedKmS: number;
}

/** One point of an orbit: GCRF position (km) and the visible fraction of the Sun. */
export interface PathPoint {
  position: Vec3;
  sunlit: number;
}

/** A ground station: where elevations, passes and the Sun's elevation are computed from. */
export interface Station {
  name: string;
  lat: number;
  lon: number;
  /** Height above the WGS84 ellipsoid, metres. */
  heightM?: number;
}

export interface TrackPoint {
  lat: number;
  lon: number;
  /** Elevation from the station, degrees. */
  elevationDeg: number;
  sunlit: number;
}

export type ShadowKind = "sunlit" | "penumbra" | "umbra" | "antumbra";

export interface SatelliteState {
  /** GCRF, km. */
  position: Vec3;
  /** GCRF, km/s. */
  velocity: Vec3;
  lat: number;
  lon: number;
  altitudeKm: number;
  /** From the station. */
  elevationDeg: number;
  azimuthDeg: number;
  rangeKm: number;
  earthRotationAngleDeg: number;
  /** Periapsis (two-body) or the lowest point of the next orbit (SGP4), GCRF km. */
  periapsis: Vec3;
  sunlitFraction: number;
  shadow: ShadowKind;
  /** The Sun's elevation at the station, degrees. */
  sunElevationDeg: number;
  subsolar: { lat: number; lon: number };
  /** Unit vector towards the Sun, GCRF. */
  sunDirection: Vec3;
}

export interface Pass {
  /** Seconds after the epoch. */
  startS: number;
  endS: number;
  maxElevationDeg: number;
  /** Seconds of the pass with the satellite sunlit in a dark sky. */
  visibleS: number;
}

export interface Site {
  name: string;
  lat: number;
  lon: number;
}

export interface Terminator {
  subsolar: { lat: number; lon: number };
  /** West to east, one point per longitude step: `[lat, lon]`. */
  points: [number, number][];
}

export interface Constellation {
  satellites: { lat: number; lon: number; sunlit: number }[];
  /** Pairs of satellite indices with a line of sight. */
  links: [number, number][];
  /** Per site, the number of satellites above 25°. */
  sitesCovered: number[];
}

export interface WalkerPattern {
  total: number;
  planes: number;
  phasing: number;
}

/** Everything the views draw at one instant. */
export interface Snapshot {
  t: number;
  state: SatelliteState;
  /** GCRS → ITRS rotation, row-major. */
  earth: number[];
  terminator: Terminator;
  siteElevationsDeg: number[];
  constellation: Constellation | null;
}

const SHADOW: ShadowKind[] = ["sunlit", "penumbra", "umbra", "antumbra"];

function chunk<T extends number[]>(a: ArrayLike<number>, n: number): T[] {
  const out: T[] = [];
  for (let k = 0; k + n <= a.length; k += n) out.push(Array.from({ length: n }, (_, i) => a[k + i]) as T);
  return out;
}

/** The library, loaded and ready. One instance holds one model (epoch and satellite). */
export class Heavenly {
  readonly sites: Site[];
  private readonly p: Playground;

  constructor() {
    this.p = new Playground();
    const names = Playground.siteNames();
    this.sites = chunk<[number, number]>(Playground.sites(), 2).map(([lat, lon], k) => ({ name: names[k], lat, lon }));
  }

  /** Releases the model's WebAssembly memory. */
  free(): void {
    this.p.free();
  }

  /** Sets the epoch (UTC). Keeps the current satellite. */
  setEpoch(d: Date): void {
    if (isNaN(d.getTime())) throw new Error("invalid epoch");
    this.p.setEpoch(d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds() + d.getUTCMilliseconds() / 1000);
  }

  /** Switches to a two-body orbit with these elements, at the current epoch. */
  setElements(e: Elements): void {
    this.p.setOrbit(e.periapsisAltitudeKm, e.eccentricity, e.inclinationDeg, e.raanDeg, e.argPeriapsisDeg, e.meanAnomalyDeg ?? 0);
  }

  /** Moves the ground station (London by default). */
  setStation(s: Station): void {
    this.p.setStation(s.lat, s.lon, s.heightM ?? 0);
  }

  /** Switches to a TLE propagated with SGP4; the epoch becomes the TLE's. */
  setTle(text: string): void {
    this.p.setTle(text);
  }

  /** The model epoch (UTC). */
  epoch(): Date {
    const f = this.p.epochFields();
    return new Date(Date.UTC(f[0], f[1] - 1, f[2], f[3], f[4], 0) + f[5] * 1000);
  }

  info(): OrbitInfo {
    const [periodS, perigeeAltitudeKm, apogeeAltitudeKm, perigeeSpeedKmS, apogeeSpeedKmS] = this.p.info();
    return { periodS, perigeeAltitudeKm, apogeeAltitudeKm, perigeeSpeedKmS, apogeeSpeedKmS };
  }

  /** One orbit as `n` points. */
  orbitPath(n = 361): PathPoint[] {
    return chunk<[number, number, number, number]>(this.p.orbitPath(n), 4).map(([x, y, z, sunlit]) => ({ position: [x, y, z], sunlit }));
  }

  /** The state `t` seconds after the epoch. */
  state(t: number): SatelliteState {
    const s = this.p.state(t);
    return {
      position: [s[0], s[1], s[2]],
      velocity: [s[3], s[4], s[5]],
      lat: s[6],
      lon: s[7],
      altitudeKm: s[8],
      elevationDeg: s[9],
      azimuthDeg: s[10],
      rangeKm: s[11],
      earthRotationAngleDeg: s[12],
      periapsis: [s[13], s[14], s[15]],
      sunlitFraction: s[16],
      shadow: SHADOW[s[17]] ?? "sunlit",
      sunElevationDeg: s[18],
      subsolar: { lat: s[19], lon: s[20] },
      sunDirection: [s[21], s[22], s[23]],
    };
  }

  groundTrack(t0: number, t1: number, n = 1200): TrackPoint[] {
    return chunk<[number, number, number, number]>(this.p.groundTrack(t0, t1, n), 4).map(([lat, lon, elevationDeg, sunlit]) => ({ lat, lon, elevationDeg, sunlit }));
  }

  /** GCRS → ITRS rotation at `t`, row-major. */
  earthFrame(t: number): number[] {
    return Array.from(this.p.earthFrame(t));
  }

  terminator(t: number, n = 181): Terminator {
    const r = this.p.terminator(t, n);
    return { subsolar: { lat: r[0], lon: r[1] }, points: chunk<[number, number]>(r.subarray(2), 2) };
  }

  /** Passes over the station above 10° between `t0` and `t1` (seconds after the epoch). */
  passes(t0: number, t1: number): Pass[] {
    return chunk<[number, number, number, number]>(this.p.passes(t0, t1), 4).map(([startS, endS, maxElevationDeg, visibleS]) => ({ startS, endS, maxElevationDeg, visibleS }));
  }

  /** The satellite's elevation from each of `sites`, degrees. */
  siteElevations(t: number): number[] {
    return Array.from(this.p.siteView(t));
  }

  /** A dawn–dusk sun-synchronous orbit at `altitudeKm` for the epoch. */
  dawnDusk(altitudeKm: number): { inclinationDeg: number; raanDeg: number } {
    const [inclinationDeg, raanDeg] = this.p.dawnDusk(altitudeKm);
    return { inclinationDeg, raanDeg };
  }

  /** A Walker shell built from the two-body elements, at `t`. */
  constellation(w: WalkerPattern, t: number): Constellation {
    const r = this.p.constellation(w.total, w.planes, w.phasing, t);
    const n = r[0];
    const m = r[1 + 3 * n];
    return {
      satellites: chunk<[number, number, number]>(r.subarray(1, 1 + 3 * n), 3).map(([lat, lon, sunlit]) => ({ lat, lon, sunlit })),
      links: chunk<[number, number]>(r.subarray(2 + 3 * n, 2 + 3 * n + 2 * m), 2),
      sitesCovered: Array.from(r.subarray(2 + 3 * n + 2 * m)),
    };
  }

  /** Everything the views need at `t`. */
  snapshot(t: number, walker: WalkerPattern | null = null): Snapshot {
    return {
      t,
      state: this.state(t),
      earth: this.earthFrame(t),
      terminator: this.terminator(t),
      siteElevationsDeg: this.siteElevations(t),
      constellation: walker ? this.constellation(walker, t) : null,
    };
  }
}

let ready: Promise<unknown> | null = null;

/**
 * Initialises the WebAssembly module (once) and returns a new model. By
 * default the module comes bundled with this package; pass a URL or bytes to
 * load it from elsewhere.
 */
export async function loadHeavenly(source?: WasmSource): Promise<Heavenly> {
  ready ??= init(source === undefined ? undefined : { module_or_path: source });
  await ready;
  return new Heavenly();
}
