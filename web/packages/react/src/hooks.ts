import { useEffect, useRef, useState } from "react";
import type { Land } from "./draw.js";
import { loadHeavenly, type Heavenly, type WasmSource } from "./wasm.js";

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
 * wraps to 0 after `span` seconds.
 */
export function useClock(speed: number, span: number): Clock {
  const [t, setT] = useState(0);
  const [playing, setPlaying] = useState(true);
  const params = useRef({ speed, span });
  params.current = { speed, span };
  useEffect(() => {
    if (!playing) return;
    let last: number | null = null, id = 0;
    const tick = (ts: number) => {
      const dt = last === null ? 0 : (ts - last) / 1000;
      last = ts;
      setT((t0) => {
        const t1 = t0 + dt * params.current.speed;
        return t1 > params.current.span ? 0 : t1;
      });
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, [playing]);
  return { t, setT, playing, setPlaying };
}
