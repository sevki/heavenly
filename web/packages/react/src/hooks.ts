import { useEffect, useRef, useState } from "react";
import type { Land } from "./draw.js";
import { loadHeavenly, type Heavenly, type Station, type WasmSource } from "./wasm.js";

/**
 * Loads the WebAssembly module and returns a model, or `null` while it
 * loads. The model is freed on unmount.
 */
export function useHeavenly(source?: WasmSource): { heavenly: Heavenly | null; error: string | null } {
  const [heavenly, setHeavenly] = useState<Heavenly | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let model: Heavenly | null = null;
    let live = true;
    loadHeavenly(source).then(
      (h) => {
        if (live) setHeavenly((model = h));
        else h.free();
      },
      (e: unknown) => live && setError(e instanceof Error ? e.message : String(e)),
    );
    return () => {
      live = false;
      model?.free();
    };
    // The source is read once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return { heavenly, error };
}

/**
 * The device's position from the browser's Geolocation API, as a station.
 * The browser asks the user first; the position stays in the page.
 */
export function currentPosition(name = "Your location"): Promise<Station> {
  return new Promise((resolve, reject) => {
    if (!("geolocation" in navigator)) return reject(new Error("this browser has no geolocation"));
    navigator.geolocation.getCurrentPosition(
      (p) => resolve({ name, lat: p.coords.latitude, lon: p.coords.longitude, heightM: p.coords.altitude ?? 0 }),
      (e) => reject(new Error(e.code === e.PERMISSION_DENIED ? "location permission was denied" : `could not get the location (${e.message})`)),
      { enableHighAccuracy: false, timeout: 15000, maximumAge: 600000 },
    );
  });
}

/** Fetches coastlines (`{ rings }` JSON); `null` until loaded or if missing. */
export function useLand(url: string | null): Land | null {
  const [land, setLand] = useState<Land | null>(null);
  useEffect(() => {
    if (!url) return;
    let live = true;
    fetch(url)
      .then((r) => (r.ok ? (r.json() as Promise<Land>) : null))
      .then((l) => live && setLand(l))
      .catch(() => live && setLand(null));
    return () => {
      live = false;
    };
  }, [url]);
  return land;
}

export interface Clock {
  /** Seconds after the epoch. */
  t: number;
  setT: (t: number) => void;
  playing: boolean;
  setPlaying: (p: boolean) => void;
}

/**
 * Simulation time that advances at `speed` × real time while playing, and
 * wraps back to `start` after `span` seconds. With `speed` 1 it keeps pace
 * with the wall clock.
 */
export function useClock(speed: number, span: number, start = 0): Clock {
  const [t, setT] = useState(start);
  const [playing, setPlaying] = useState(true);
  const params = useRef({ speed, span, start });
  params.current = { speed, span, start };
  useEffect(() => {
    if (!playing) return;
    let last: number | null = null, id = 0;
    const tick = (ts: number) => {
      const dt = last === null ? 0 : (ts - last) / 1000;
      last = ts;
      setT((t0) => {
        const { speed: k, span: n, start: s } = params.current;
        const t1 = t0 + dt * k;
        return t1 > s + n || t1 < s ? s : t1;
      });
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, [playing]);
  return { t, setT, playing, setPlaying };
}
