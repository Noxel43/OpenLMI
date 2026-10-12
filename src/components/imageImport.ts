// Turning image files into animation frames.
//
// On a matrix (or cube) an image becomes one frame, a GIF becomes one frame
// per GIF frame, and several images become one frame each.
//
// On a strip, ring or backlight there is no second dimension to draw into, so
// time takes its place: every ROW of the image is one step of the animation,
// and the columns are spread along the LEDs.

import type { LayoutPreview } from "../types";
import { clamp } from "../util";

export interface ImportedFrame {
  durationMs: number;
  px: Uint8Array;
}

/** The most frames one animation may have. */
export const MAX_FRAMES = 400;

/** Step length used when an image's rows become frames. */
const ROW_STEP_MS = 50;

interface Picture {
  source: CanvasImageSource;
  width: number;
  height: number;
  durationMs: number;
  close?: () => void;
}

/** Decodes a file into one or more pictures (several for an animated GIF). */
async function decode(file: File, limit: number): Promise<Picture[]> {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Decoder = (window as any).ImageDecoder;
  if (file.type === "image/gif" && Decoder) {
    const decoder = new Decoder({ data: await file.arrayBuffer(), type: file.type });
    // The frame count is only final once the whole file has been parsed.
    await decoder.completed;
    await decoder.tracks.ready;
    const total = Math.min(decoder.tracks.selectedTrack.frameCount, limit);
    const pictures: Picture[] = [];
    for (let i = 0; i < total; i++) {
      const { image } = await decoder.decode({ frameIndex: i });
      pictures.push({
        source: image,
        width: image.displayWidth,
        height: image.displayHeight,
        durationMs: clamp(Math.round((image.duration ?? 100_000) / 1000), 20, 60_000),
        close: () => image.close(),
      });
    }
    return pictures;
  }
  const bitmap = await createImageBitmap(file);
  return [{ source: bitmap, width: bitmap.width, height: bitmap.height, durationMs: 200, close: () => bitmap.close() }];
}

/** Draws a picture onto a `w` x `h` canvas and returns its RGBA bytes. */
function rasterise(ctx: CanvasRenderingContext2D, picture: Picture, w: number, h: number, letterbox: boolean): Uint8ClampedArray {
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, w, h);
  // Pixel art that is an exact multiple of the target stays crisp.
  const exact = picture.width % w === 0 && picture.height % h === 0;
  ctx.imageSmoothingEnabled = !exact;
  ctx.imageSmoothingQuality = "high";
  if (letterbox) {
    const k = Math.min(w / picture.width, h / picture.height);
    ctx.drawImage(picture.source, (w - picture.width * k) / 2, (h - picture.height * k) / 2, picture.width * k, picture.height * k);
  } else {
    ctx.drawImage(picture.source, 0, 0, w, h);
  }
  return ctx.getImageData(0, 0, w, h).data;
}

/**
 * Reads image files into frames for `layout`. Files are taken in name order
 * ("step2" before "step10"). At most `maxFrames` frames are returned.
 */
export async function importImages(files: File[], layout: LayoutPreview, maxFrames: number): Promise<ImportedFrame[]> {
  const ordered = [...files].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
  const frames: ImportedFrame[] = [];
  const n = layout.count;
  const resize = (w: number, h: number) => {
    canvas.width = Math.max(1, w);
    canvas.height = Math.max(1, h);
  };

  for (const file of ordered) {
    if (frames.length >= maxFrames) break;
    const pictures = await decode(file, maxFrames - frames.length);
    try {
      if (layout.isGrid) {
        const [w, h] = layout.dims;
        resize(w, h);
        for (const picture of pictures) {
          const data = rasterise(ctx, picture, w, h, true);
          const px = new Uint8Array(n * 3);
          layout.grid.forEach((g, i) => {
            const o = (g[0] + g[1] * w) * 4;
            px.set([data[o], data[o + 1], data[o + 2]], i * 3);
          });
          frames.push({ durationMs: picture.durationMs, px });
        }
      } else {
        const animated = pictures.length > 1;
        for (const picture of pictures) {
          // A still image: every row is a step. An animated one already has
          // its steps, so each of its frames is squashed into a single row.
          const rows = animated ? 1 : Math.min(picture.height, maxFrames - frames.length);
          if (rows <= 0) break;
          resize(n, rows);
          const data = rasterise(ctx, picture, n, rows, false);
          for (let row = 0; row < rows; row++) {
            const px = new Uint8Array(n * 3);
            layout.grid.forEach((g, i) => {
              const o = (row * n + Math.min(g[0], n - 1)) * 4;
              px.set([data[o], data[o + 1], data[o + 2]], i * 3);
            });
            frames.push({ durationMs: animated ? picture.durationMs : ROW_STEP_MS, px });
          }
        }
      }
    } finally {
      pictures.forEach((p) => p.close?.());
    }
  }
  return frames.slice(0, maxFrames);
}
