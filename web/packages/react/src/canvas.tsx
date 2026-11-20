import { useEffect, useRef, useState, type CSSProperties, type ReactNode, type RefObject } from "react";
import { createRenderer, type Renderer } from "./gl.js";

type Draw = (g: Renderer, text: CanvasRenderingContext2D) => void;

/**
 * A WebGL2 canvas with a transparent 2D canvas over it for text. `draw` runs
 * after every render, on resize, and whenever the returned `redraw` is called.
 */
export function useGlCanvas(draw: Draw): {
  gl: RefObject<HTMLCanvasElement | null>;
  text: RefObject<HTMLCanvasElement | null>;
  redraw: () => void;
  error: string | null;
} {
  const gl = useRef<HTMLCanvasElement>(null);
  const text = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<Renderer | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(draw);
  latest.current = draw;
  const frame = useRef(0);

  const paint = () => {
    const g = renderer.current, over = text.current, canvas = gl.current;
    if (!g || !over || !canvas) return;
    const rect = canvas.getBoundingClientRect(), dpr = window.devicePixelRatio || 1;
    const w = Math.round(rect.width * dpr), h = Math.round(rect.height * dpr);
    if (over.width !== w || over.height !== h) {
      over.width = w;
      over.height = h;
    }
    const ctx = over.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, rect.width, rect.height);
    latest.current(g, ctx);
  };
  const redraw = () => {
    cancelAnimationFrame(frame.current);
    frame.current = requestAnimationFrame(paint);
  };

  useEffect(() => {
    const canvas = gl.current!;
    try {
      renderer.current = createRenderer(canvas);
      if (!renderer.current) setError("this browser has no WebGL2");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    paint();
    const resize = new ResizeObserver(redraw);
    resize.observe(canvas);
    return () => {
      resize.disconnect();
      cancelAnimationFrame(frame.current);
      renderer.current?.dispose();
      renderer.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(paint);

  return { gl, text, redraw, error };
}

const fill: CSSProperties = { position: "absolute", left: 0, top: 0, width: "100%", height: "100%", pointerEvents: "none" };

/** Lays the text canvas over the WebGL canvas. */
export function Layered(props: {
  gl: RefObject<HTMLCanvasElement | null>;
  text: RefObject<HTMLCanvasElement | null>;
  className?: string;
  style?: CSSProperties;
  label: string;
  error: string | null;
  children?: ReactNode;
}) {
  return (
    <div style={{ position: "relative" }}>
      <canvas ref={props.gl} className={props.className} style={{ display: "block", width: "100%", ...props.style }} aria-label={props.label} />
      <canvas ref={props.text} style={fill} aria-hidden />
      {props.error && <div className="hv-status">{props.error}</div>}
      {props.children}
    </div>
  );
}
