import { useEffect, useRef, type CSSProperties } from "react";
import { Layered, useGlCanvas } from "./canvas.js";
import { drawGlobe, drawMap, type GlobeView, type Land } from "./draw.js";
import { themeFromCss, type Theme } from "./gl.js";
import type { Pass, PathPoint, Site, Snapshot, Station, TrackPoint } from "./wasm.js";

const DEG = Math.PI / 180;

export interface GlobeProps {
  snapshot: Snapshot;
  /** One orbit, from `Heavenly.orbitPath`. */
  path: PathPoint[];
  land?: Land | null;
  /** Colours; by default read from the page's CSS on every draw. */
  theme?: Theme;
  lowPointLabel?: string;
  className?: string;
  style?: CSSProperties;
}

/** The 3D view in GCRF. Drag to rotate, scroll to zoom. */
export function Globe({ snapshot, path, land = null, theme, lowPointLabel = "periapsis", className, style }: GlobeProps) {
  const view = useRef<GlobeView>({ az: 35 * DEG, el: 22 * DEG, zoom: 1 });
  const { gl, text, redraw, error } = useGlCanvas((g, ctx) =>
    drawGlobe(g, ctx, { snapshot, path, land, theme: theme ?? themeFromCss(), view: view.current, lowPointLabel }),
  );
  // Native listeners: React's wheel handler is passive, so it cannot stop the
  // page from scrolling.
  useEffect(() => {
    const canvas = gl.current!;
    let drag: [number, number, number, number] | null = null;
    const down = (e: PointerEvent) => {
      drag = [e.clientX, e.clientY, view.current.az, view.current.el];
      canvas.setPointerCapture(e.pointerId);
    };
    const move = (e: PointerEvent) => {
      if (!drag) return;
      view.current.az = drag[2] - (e.clientX - drag[0]) * 0.008;
      view.current.el = Math.max(-1.5, Math.min(1.5, drag[3] + (e.clientY - drag[1]) * 0.008));
      redraw();
    };
    const up = () => (drag = null);
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      view.current.zoom = Math.max(0.3, Math.min(8, view.current.zoom * Math.exp(-e.deltaY * 0.001)));
      redraw();
    };
    canvas.addEventListener("pointerdown", down);
    canvas.addEventListener("pointermove", move);
    canvas.addEventListener("pointerup", up);
    canvas.addEventListener("wheel", wheel, { passive: false });
    return () => {
      canvas.removeEventListener("pointerdown", down);
      canvas.removeEventListener("pointermove", move);
      canvas.removeEventListener("pointerup", up);
      canvas.removeEventListener("wheel", wheel);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return <Layered gl={gl} text={text} className={className} style={{ aspectRatio: "1 / 1", cursor: "grab", touchAction: "none", ...style }} label="3D view of the orbit around the Earth" error={error} />;
}

export interface GroundTrackProps {
  snapshot: Snapshot;
  /** From `Heavenly.groundTrack`. */
  track: TrackPoint[];
  sites?: Site[];
  land?: Land | null;
  theme?: Theme;
  /** The ground station the track's elevations are from. */
  station?: Station;
  className?: string;
  style?: CSSProperties;
}

/** The model's default station. */
export const LONDON: Station = { name: "London", lat: 51.5072, lon: -0.1276, heightM: 35 };

/** The ground track on an equirectangular map, with the night side, coverage and sites. */
export function GroundTrack({ snapshot, track, sites = [], land = null, theme, station = LONDON, className, style }: GroundTrackProps) {
  const { gl, text, error } = useGlCanvas((g, ctx) => drawMap(g, ctx, { snapshot, track, sites, land, theme: theme ?? themeFromCss(), station }));
  return <Layered gl={gl} text={text} className={className} style={{ aspectRatio: "2 / 1", ...style }} label="Ground track on a map of the Earth" error={error} />;
}

/** `epoch + t` as "YYYY-MM-DD hh:mm" (UTC). */
export function formatUtc(epoch: Date, t: number): string {
  return new Date(epoch.getTime() + t * 1000).toISOString().slice(0, 16).replace("T", " ");
}

export interface PassTableProps {
  passes: Pass[];
  epoch: Date;
  station?: string;
  /** Called with the pass's start, seconds after the epoch. */
  onSelect?: (t: number) => void;
}

/** The passes over a station, from `Heavenly.passes`. Click a row to jump to it. */
export function PassTable({ passes, epoch, station = "London", onSelect }: PassTableProps) {
  if (!passes.length) return <p>No passes above 10° over {station} in this span.</p>;
  return (
    <table>
      <thead>
        <tr>
          <th>Passes over {station} above 10°</th>
          <th>length</th>
          <th>highest</th>
          <th>sunlit in a dark sky</th>
        </tr>
      </thead>
      <tbody>
        {passes.map((p) => (
          <tr key={p.startS} className={p.visibleS > 0 ? "hv-vis" : undefined} onClick={() => onSelect?.(p.startS)} style={onSelect ? { cursor: "pointer" } : undefined}>
            <td>{formatUtc(epoch, p.startS)} UTC</td>
            <td>{Math.round(p.endS - p.startS)} s</td>
            <td>{p.maxElevationDeg.toFixed(0)}°</td>
            <td>{p.visibleS > 0 ? `visible for ${Math.round(p.visibleS)} s` : "not visible"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
