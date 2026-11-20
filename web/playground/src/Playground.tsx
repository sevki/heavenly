import { useEffect, useMemo, useState, type ChangeEvent } from "react";
import {
  Globe,
  GroundTrack,
  LONDON,
  PassTable,
  currentPosition,
  formatUtc,
  useClock,
  useHeavenly,
  useLand,
  type Elements,
  type Heavenly,
  type Snapshot,
  type Station,
  type WalkerPattern,
} from "@heavenly/react";

const ISS_TLE =
  "1 25544U 98067A   20194.88612269 -.00002218  00000-0 -31515-4 0  9992\n" +
  "2 25544  51.6461 221.2784 0001413  89.1723 280.4612 15.49507896236008";

// The altitude slider is logarithmic, 150 km to 40,000 km.
const ALT_MIN = 150, ALT_MAX = 40000;
const altFromSlider = (v: number) => ALT_MIN * Math.pow(ALT_MAX / ALT_MIN, v / 1000);
const sliderFromAlt = (a: number) => (1000 * Math.log(a / ALT_MIN)) / Math.log(ALT_MAX / ALT_MIN);

interface Sliders {
  alt: number; // slider position, 0..1000
  e: number;
  i: number;
  raan: number;
  argp: number;
}
const sliders = (alt: number, e: number, i: number, raan: number, argp: number): Sliders => ({ alt: sliderFromAlt(alt), e, i, raan, argp });
const PRESETS: [string, Sliders][] = [
  ["ISS", sliders(420, 0.0005, 51.64, 0, 0)],
  ["Sun-synchronous", sliders(700, 0.001, 98.2, 40, 0)],
  ["Molniya", sliders(600, 0.74, 63.4, 300, 270)],
  ["GPS", sliders(20180, 0.005, 55, 120, 0)],
  ["Geostationary", sliders(35786, 0, 0.05, 0, 0)],
];
const WALKER: WalkerPattern = { total: 24, planes: 4, phasing: 1 };
const DAY = 86400;

// What the model is set to: two-body elements at an epoch, or a TLE.
type Source = { kind: "elements"; sliders: Sliders; epoch: string } | { kind: "tle"; text: string };

const elementsOf = (s: Sliders): Elements => ({
  periapsisAltitudeKm: altFromSlider(s.alt),
  eccentricity: s.e,
  inclinationDeg: s.i,
  raanDeg: s.raan,
  argPeriapsisDeg: s.argp,
});
const parseEpoch = (text: string) => new Date(text + "Z");
const isoMinute = (d: Date) => d.toISOString().slice(0, 16);
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
const sameStation = (a: Station, b: Station) => a.name === b.name && a.lat === b.lat && a.lon === b.lon;

function fmtTime(s: number): string {
  const sign = s < 0 ? "−" : "+";
  s = Math.abs(s);
  const d = Math.floor(s / DAY), h = Math.floor((s % DAY) / 3600), m = Math.floor((s % 3600) / 60), sec = Math.floor(s % 60);
  return `${sign}${d ? d + "d " : ""}${h}h ${String(m).padStart(2, "0")}m ${String(sec).padStart(2, "0")}s`;
}

// Points the model at `source` and the station, and reads what does not
// change with time over the window [start, start + span].
function prepare(h: Heavenly, source: Source, station: Station, start: number) {
  if (source.kind === "elements") {
    h.setEpoch(parseEpoch(source.epoch));
    h.setElements(elementsOf(source.sliders));
  } else {
    h.setTle(source.text);
  }
  h.setStation(station);
  const info = h.info();
  const span = source.kind === "tle" ? 2 * DAY : Math.min(Math.max(3 * info.periodS, 3 * 3600), 2 * DAY);
  return {
    info,
    span,
    epoch: h.epoch(),
    path: h.orbitPath(),
    track: h.groundTrack(start, start + span),
    passes: h.passes(start, start + span),
  };
}

export function Playground({ wasmUrl, landUrl }: { wasmUrl?: string; landUrl: string }) {
  const { heavenly, error: loadError } = useHeavenly(wasmUrl);
  const land = useLand(landUrl);
  const [sl, setSl] = useState<Sliders>(PRESETS[0][1]);
  const [epochText, setEpochText] = useState("2026-11-19T17:00");
  const [source, setSource] = useState<Source>({ kind: "elements", sliders: PRESETS[0][1], epoch: "2026-11-19T17:00" });
  const [tleText, setTleText] = useState(ISS_TLE);
  const [walker, setWalker] = useState<WalkerPattern | null>(null);
  const [speed, setSpeed] = useState(300);
  const [station, setStation] = useState<Station>(LONDON);
  const [located, setLocated] = useState<Station | null>(null);
  // Live: time follows the wall clock. `start` is where the window of
  // track and passes begins, in seconds after the epoch.
  const [live, setLive] = useState(false);
  const [start, setStart] = useState(0);
  const [note, setNote] = useState("");

  const orbit = useMemo(() => {
    if (!heavenly) return null;
    try {
      return { ok: prepare(heavenly, source, station, start) } as const;
    } catch (e) {
      return { err: message(e) } as const;
    }
  }, [heavenly, source, station, start]);
  const ready = orbit && "ok" in orbit ? orbit.ok : null;
  const span = ready?.span ?? 10800;
  const clock = useClock(live ? 1 : speed, span, start);
  const wall = ready ? (Date.now() - ready.epoch.getTime()) / 1000 : 0;
  const t = live ? wall : clock.t;
  // Live time runs past the window: move the window along.
  useEffect(() => {
    if (live && ready && wall > start + span) setStart(wall);
  }, [live, ready, wall, start, span]);

  let now: Snapshot | null = null;
  let frameError: string | null = null;
  if (heavenly && ready) {
    try {
      now = heavenly.snapshot(t, walker);
    } catch (e) {
      frameError = message(e);
    }
  }

  // Leaves live mode, carrying on from the current moment.
  const stopLive = (at = t) => {
    if (!live) return;
    setLive(false);
    clock.setT(at);
  };
  const applyElements = (s: Sliders, epoch = epochText, opts: { walker?: WalkerPattern | null; restart?: boolean } = {}) => {
    setLive(false);
    setSl(s);
    setEpochText(epoch);
    setSource({ kind: "elements", sliders: s, epoch });
    if (opts.walker !== undefined) setWalker(opts.walker);
    if (opts.restart) {
      setStart(0);
      clock.setT(0);
    }
    setNote("");
  };
  const slide = (k: keyof Sliders) => (e: ChangeEvent<HTMLInputElement>) => applyElements({ ...sl, [k]: +e.target.value });
  const applyTle = (text: string) => {
    if (!heavenly) return;
    try {
      heavenly.setTle(text); // checked here, so a bad TLE leaves the model as it was
      setLive(false);
      setEpochText(isoMinute(heavenly.epoch()));
      setWalker(null);
      setSource({ kind: "tle", text });
      setStart(0);
      clock.setT(0);
      setNote("Propagating the TLE with SGP4. Move a slider or pick a preset to go back to two-body elements.");
    } catch (e) {
      setNote(message(e));
    }
  };
  // A dawn–dusk sun-synchronous orbit at 550 km for the epoch: the library
  // gives the inclination and the node that faces the Sun.
  const orbitalDataCentre = () => {
    if (!heavenly) return;
    try {
      heavenly.setEpoch(parseEpoch(epochText));
      const { inclinationDeg, raanDeg } = heavenly.dawnDusk(550);
      applyElements(sliders(550, 0, inclinationDeg, raanDeg, 0), epochText, { walker: null, restart: true });
    } catch (e) {
      setNote(message(e));
    }
  };
  // Live: the orbit as it is now, moving in real time. Slider orbits move
  // their epoch to now; a TLE keeps its epoch and the window moves to now.
  const goLive = () => {
    if (!ready) return;
    const nowDate = new Date();
    if (source.kind === "elements") {
      const epoch = isoMinute(nowDate);
      setEpochText(epoch);
      setSource({ ...source, epoch });
      setStart(0);
      setNote("");
    } else {
      const ahead = (nowDate.getTime() - ready.epoch.getTime()) / 1000;
      setStart(ahead);
      const days = Math.abs(ahead) / DAY;
      setNote(
        days > 14
          ? `This TLE's epoch is ${days.toFixed(0)} days from now, and SGP4's error grows by kilometres a day: paste a current TLE (from CelesTrak, for example) to see where the satellite is.`
          : "Live: SGP4 propagates the TLE to the current time.",
      );
    }
    setLive(true);
    clock.setPlaying(true);
  };
  const locate = () => {
    setNote("Asking the browser for your location…");
    currentPosition().then(
      (s) => {
        setLocated(s);
        setStation(s);
        setNote("");
      },
      (e) => setNote(`Your location: ${message(e)}.`),
    );
  };

  const tle = source.kind === "tle";
  const status = loadError
    ? `The playground could not start: ${loadError}. Build it with \`book/build-playground.sh\` and serve the book over HTTP (mdbook serve).`
    : !heavenly
      ? "Loading the WebAssembly module…"
      : orbit && "err" in orbit
        ? orbit.err
        : (frameError ?? note);
  const sites = heavenly?.sites ?? [];
  const stations: Station[] = [LONDON, ...sites.map((s) => ({ ...s, heightM: 0 })), ...(located ? [located] : [])];

  return (
    <div className={`hv-play${heavenly ? " hv-ready" : ""}${tle ? " hv-tle-mode" : ""}`}>
      <div className="hv-presets">
        {PRESETS.map(([name, p]) => (
          <button key={name} onClick={() => applyElements(p, epochText, { walker: null, restart: true })}>
            {name}
          </button>
        ))}
        <button
          onClick={() => {
            setTleText(ISS_TLE);
            applyTle(ISS_TLE);
          }}
        >
          ISS (TLE, SGP4)
        </button>
        <button onClick={() => applyElements(sliders(550, 0, 53, 0, 0), epochText, { walker: WALKER, restart: true })}>Constellation</button>
        <button onClick={orbitalDataCentre}>Orbital data centre</button>
      </div>
      <details className="hv-tle">
        <summary>Paste a TLE</summary>
        <textarea rows={3} spellCheck={false} aria-label="Two-line element set" value={tleText} onChange={(e) => setTleText(e.target.value)} />
        <button onClick={() => applyTle(tleText)}>Propagate with SGP4</button>
        <span className="hv-note">
          from <a href="https://celestrak.org/NORAD/elements/">CelesTrak</a>, for example. The epoch moves to the TLE's.
        </span>
      </details>
      <div className="hv-controls">
        <label>
          Periapsis altitude <input type="range" min={0} max={1000} step={1} value={sl.alt} onChange={slide("alt")} /> <span>{altFromSlider(sl.alt).toFixed(0)} km</span>
        </label>
        <label>
          Eccentricity e <input type="range" min={0} max={0.9} step={0.001} value={sl.e} onChange={slide("e")} /> <span>{sl.e.toFixed(3)}</span>
        </label>
        <label>
          Inclination i <input type="range" min={0} max={180} step={0.1} value={sl.i} onChange={slide("i")} /> <span>{sl.i.toFixed(1)}°</span>
        </label>
        <label>
          Ascending node Ω <input type="range" min={0} max={360} step={1} value={sl.raan} onChange={slide("raan")} /> <span>{sl.raan.toFixed(0)}°</span>
        </label>
        <label>
          Arg. of periapsis ω <input type="range" min={0} max={360} step={1} value={sl.argp} onChange={slide("argp")} /> <span>{sl.argp.toFixed(0)}°</span>
        </label>
        <label>
          Epoch (UTC) <input type="datetime-local" step={60} value={epochText} onChange={(e) => applyElements(sl, e.target.value)} />
        </label>
      </div>
      <div className="hv-tabs" role="tablist" aria-label="Ground station">
        <span className="hv-tabs-label">Ground station</span>
        {stations.map((s) => (
          <button key={s.name} role="tab" aria-selected={sameStation(s, station)} onClick={() => setStation(s)}>
            {s.name}
          </button>
        ))}
        {!located && (
          <button role="tab" aria-selected={false} onClick={locate} title="Uses your browser's location; it stays in this page">
            Your location…
          </button>
        )}
      </div>
      <div className="hv-canvases">
        {now && ready ? (
          <>
            <Globe className="hv-canvas-orbit" snapshot={now} path={ready.path} land={land} lowPointLabel={tle ? "lowest point" : "periapsis"} />
            <GroundTrack className="hv-canvas-track" snapshot={now} track={ready.track} sites={sites} land={land} station={station} />
          </>
        ) : null}
      </div>
      <div className="hv-timebar">
        <button
          onClick={() => {
            stopLive();
            clock.setPlaying(!clock.playing);
          }}
        >
          {clock.playing ? "Pause" : "Play"}
        </button>
        <button className={live ? "hv-live-on" : undefined} aria-pressed={live} onClick={() => (live ? stopLive() : goLive())} title="Follow the current time, at real speed">
          ● Live
        </button>
        <input
          type="range"
          min={start}
          max={start + span}
          step={1}
          value={Math.min(Math.max(t, start), start + span)}
          aria-label="Time after the epoch"
          onChange={(e) => {
            setLive(false);
            clock.setT(+e.target.value);
          }}
        />
        <label>
          speed{" "}
          <select
            value={live ? 1 : speed}
            onChange={(e) => {
              stopLive();
              setSpeed(+e.target.value);
            }}
          >
            {live && <option value={1}>×1</option>}
            {[60, 300, 1200, 3600].map((v) => (
              <option key={v} value={v}>
                ×{v}
              </option>
            ))}
          </select>
        </label>
      </div>
      {now && ready && <Readout now={now} info={ready.info} epoch={ready.epoch} walker={walker} sites={sites.map((s) => s.name)} station={station} live={live} />}
      <div className="hv-passes">
        {ready && (
          <PassTable
            passes={ready.passes}
            epoch={ready.epoch}
            station={station.name}
            onSelect={(at) => {
              setLive(false);
              clock.setT(at);
            }}
          />
        )}
      </div>
      <div className="hv-status">{status}</div>
    </div>
  );
}

const SHADOW = { sunlit: "in sunlight", penumbra: "in the penumbra", umbra: "in the Earth's shadow", antumbra: "in the antumbra" };

function Readout(props: { now: Snapshot; info: ReturnType<Heavenly["info"]>; epoch: Date; walker: WalkerPattern | null; sites: string[]; station: Station; live: boolean }) {
  const { now, info, epoch, walker, sites, station, live } = props;
  const s = now.state;
  const speed = Math.hypot(...s.velocity);
  const up = s.elevationDeg > 0;
  const cons = now.constellation;
  const sep = " · ";
  return (
    <div className="hv-readout">
      <b>{fmtTime(now.t)}</b> after epoch{sep}period {(info.periodS / 60).toFixed(1)} min{sep}perigee {info.perigeeAltitudeKm.toFixed(0)} km, apogee {info.apogeeAltitudeKm.toFixed(0)} km
      <br />
      altitude <b>{s.altitudeKm.toFixed(1)} km</b>
      {sep}speed {speed.toFixed(3)} km/s{sep}sub-satellite {s.lat.toFixed(2)}°, {s.lon.toFixed(2)}°{sep}ERA {s.earthRotationAngleDeg.toFixed(2)}°
      <br />
      from {station.name} ({station.lat.toFixed(2)}°, {station.lon.toFixed(2)}°): elevation <b className={up ? "hv-up" : undefined}>{s.elevationDeg.toFixed(1)}°</b>, azimuth {s.azimuthDeg.toFixed(1)}°, range{" "}
      {s.rangeKm.toFixed(0)} km — {up ? <b className="hv-up">above the horizon</b> : "below the horizon"}
      <br />
      {live && <b className="hv-up">live </b>}
      {formatUtc(epoch, now.t)} UTC{sep}satellite {SHADOW[s.shadow]}
      {s.shadow === "penumbra" ? ` (${(100 * s.sunlitFraction).toFixed(0)}% of the Sun)` : ""}
      {sep}Sun at {station.name} {s.sunElevationDeg.toFixed(1)}° ({s.sunElevationDeg > 0 ? "day" : s.sunElevationDeg > -6 ? "twilight" : "dark"})
      <br />
      {cons && walker
        ? `constellation: ${cons.satellites.length} satellites (Walker ${walker.total}/${walker.planes}/${walker.phasing}), ` +
          `${cons.satellites.filter((q) => q.sunlit >= 0.5).length} in sunlight, ${cons.links.length} inter-satellite links${sep}` +
          `data centres with a satellite above 25°: ${sites.filter((_, k) => cons.sitesCovered[k] > 0).join(", ") || "none"}`
        : `data centres in view (above 10°): ${sites.filter((_, k) => now.siteElevationsDeg[k] > 10).join(", ") || "none"}`}
    </div>
  );
}
