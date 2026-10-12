import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../api";
import type { LayoutPreview } from "../types";
import { clamp } from "../util";
import { DEFAULT_ROTATION, drawLeds, hitArea, hitTest, project, type Projection, type Rotation } from "./ledView";

export type PaintPhase = "start" | "move" | "end";

interface Props {
  layout: LayoutPreview | null;
  /** Live mode: shows what this device is doing right now. */
  deviceId?: string;
  /** Static mode: RGB bytes to show. */
  pixels?: Uint8Array | null;
  /** Wiring diagram: path, arrows and start/end markers. */
  diagram?: boolean;
  highlight?: number;
  /** Called with the LEDs under the pointer while dragging. */
  onPaint?: (indices: number[], phase: PaintPhase) => void;
  /** Brush size in LED widths; 1 paints single LEDs. */
  brush?: number;
  className?: string;
  /** Slowly spin cubes when nobody is dragging them. */
  autoRotate?: boolean;
}

export function LedCanvas({ layout, deviceId, pixels, diagram, highlight, onPaint, brush = 1, className, autoRotate }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const glowRef = useRef<HTMLCanvasElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [hover, setHover] = useState<{ x: number; y: number; index: number } | null>(null);
  const [rotTick, setRotTick] = useState(0);

  const rot = useRef<Rotation>({ ...DEFAULT_ROTATION });
  const proj = useRef<Projection | null>(null);
  const latest = useRef<Uint8Array | null>(null);
  const drag = useRef<{ x: number; y: number; painting: boolean } | null>(null);
  // The draw loop reads these through refs so it never needs restarting.
  const opts = useRef({ diagram, highlight });
  opts.current = { diagram, highlight };

  const isCube = layout?.kind === "cube";

  useLayoutEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const measure = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const redraw = () => {
    const canvas = canvasRef.current;
    const p = proj.current;
    if (!canvas || !p || size.w === 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    drawLeds(ctx, glowRef.current?.getContext("2d") ?? null, p, latest.current, size.w, size.h, opts.current);
  };

  // Resize the backing stores and rebuild the projection.
  useLayoutEffect(() => {
    if (!layout || size.w === 0 || size.h === 0) {
      proj.current = null;
      return;
    }
    const dpr = window.devicePixelRatio || 1;
    for (const c of [canvasRef.current, glowRef.current]) {
      if (!c) continue;
      const [cw, ch] = [Math.round(size.w * dpr), Math.round(size.h * dpr)];
      if (c.width !== cw || c.height !== ch) {
        c.width = cw;
        c.height = ch;
      }
      c.getContext("2d")?.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
    proj.current = project(layout, size.w, size.h, rot.current);
    if (glowRef.current) {
      glowRef.current.style.filter = `blur(${clamp(proj.current.radius * 1.9, 5, 22)}px) saturate(1.35)`;
    }
    redraw();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layout, size.w, size.h, rotTick]);

  // Static frames.
  useEffect(() => {
    if (deviceId) return;
    latest.current = pixels ?? null;
    redraw();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pixels, deviceId, diagram, highlight, layout, size.w, size.h]);

  // Live frames: one request per animation frame, never overlapping.
  useEffect(() => {
    if (!deviceId) return;
    let alive = true;
    let raf = 0;
    const tick = async () => {
      try {
        const frame = await api.getFrame(deviceId);
        if (!alive) return;
        latest.current = frame;
        redraw();
      } catch {
        // The backend may be restarting; just try again next frame.
      }
      if (alive) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      alive = false;
      cancelAnimationFrame(raf);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [deviceId, size.w, size.h]);

  // Gentle spin for cubes.
  useEffect(() => {
    if (!isCube || !autoRotate) return;
    const timer = setInterval(() => {
      if (drag.current) return;
      rot.current.yaw += 0.006;
      setRotTick((t) => t + 1);
    }, 33);
    return () => clearInterval(timer);
  }, [isCube, autoRotate]);

  const localPoint = (e: React.PointerEvent) => {
    const rect = wrapRef.current!.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  };

  const paintAt = (from: { x: number; y: number }, to: { x: number; y: number }, phase: PaintPhase) => {
    const p = proj.current;
    if (!p || !onPaint) return;
    const reach = brush <= 1 ? 0 : p.radius * 2.7 * (brush - 1) * 0.5 + p.radius;
    const hits = new Set<number>();
    // Walk the segment so fast strokes don't leave gaps.
    const steps = Math.max(1, Math.ceil(Math.hypot(to.x - from.x, to.y - from.y) / Math.max(p.radius, 3)));
    for (let s = 0; s <= steps; s++) {
      const x = from.x + ((to.x - from.x) * s) / steps;
      const y = from.y + ((to.y - from.y) * s) / steps;
      if (reach > 0) hitArea(p, x, y, reach).forEach((i) => hits.add(i));
      else {
        const i = hitTest(p, x, y);
        if (i >= 0) hits.add(i);
      }
    }
    onPaint([...hits], phase);
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const pt = localPoint(e);
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag.current = { ...pt, painting: !!onPaint };
    if (onPaint) paintAt(pt, pt, "start");
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const pt = localPoint(e);
    const d = drag.current;
    if (d) {
      if (d.painting) {
        paintAt(d, pt, "move");
      } else if (isCube) {
        rot.current.yaw += (pt.x - d.x) * 0.01;
        rot.current.pitch = clamp(rot.current.pitch + (pt.y - d.y) * 0.01, -1.4, 1.4);
        setRotTick((t) => t + 1);
      }
      d.x = pt.x;
      d.y = pt.y;
    }
    const p = proj.current;
    const index = p ? hitTest(p, pt.x, pt.y, 1.3) : -1;
    setHover(index >= 0 ? { x: pt.x, y: pt.y, index } : null);
  };

  const onPointerUp = () => {
    if (drag.current?.painting) onPaint?.([], "end");
    drag.current = null;
  };

  return (
    <div
      ref={wrapRef}
      className={`led-canvas ${onPaint ? "is-paintable" : ""} ${isCube && !onPaint ? "is-rotatable" : ""} ${className ?? ""}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onPointerLeave={() => setHover(null)}
    >
      <canvas ref={glowRef} className="led-canvas__glow" />
      <canvas ref={canvasRef} className="led-canvas__leds" />
      {hover && (
        <div className="led-canvas__tip" style={{ left: hover.x, top: hover.y }}>
          LED {hover.index}
        </div>
      )}
      {isCube && !onPaint && <div className="led-canvas__hint">Drag to rotate</div>}
    </div>
  );
}
