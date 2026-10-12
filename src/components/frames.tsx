import { useEffect, useMemo, useRef } from "react";
import type { LayoutPreview } from "../types";
import { drawLeds, project } from "./ledView";

/** Nearest-neighbour stretch of a frame onto a different LED count. */
export function resamplePixels(src: Uint8Array, count: number): Uint8Array {
  const from = Math.floor(src.length / 3);
  if (from === count) return src;
  const out = new Uint8Array(count * 3);
  if (from === 0) return out;
  for (let i = 0; i < count; i++) {
    const s = Math.min(from - 1, Math.floor((i * from) / count)) * 3;
    out[i * 3] = src[s];
    out[i * 3 + 1] = src[s + 1];
    out[i * 3 + 2] = src[s + 2];
  }
  return out;
}

/** A small, cheap rendering of one frame. */
export function FrameThumb({ layout, pixels, width = 96, height = 54 }: { layout: LayoutPreview; pixels: Uint8Array; width?: number; height?: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const proj = useMemo(() => project(layout, width, height, undefined, 6), [layout, width, height]);
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    // Thumbnails skip the monitor frame and its label to stay legible.
    drawLeds(ctx, null, { ...proj, screen: null }, pixels, width, height);
  }, [proj, pixels, width, height]);
  return <canvas ref={ref} style={{ width, height }} className="thumb" />;
}
