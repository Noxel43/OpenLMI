// Turns a layout into on-screen LED positions and draws frames onto a canvas.
// Shared by the live preview, the wiring diagram and the animation editor.

import type { LayoutPreview } from "../types";
import { clamp, hsvToRgb } from "../util";

export interface Rotation {
  yaw: number;
  pitch: number;
}

export const DEFAULT_ROTATION: Rotation = { yaw: -0.6, pitch: 0.35 };

export interface Projection {
  count: number;
  x: Float32Array;
  y: Float32Array;
  /** Per-LED radius in CSS pixels. */
  r: Float32Array;
  /** Typical radius, used for hit testing and glow strength. */
  radius: number;
  /** Draw order; cubes are painted back to front. */
  order: Uint32Array;
  square: boolean;
  /** 3D layouts: LEDs overlap, so unlit ones shrink and lit ones add up. */
  volumetric: boolean;
  /** The picture area of a monitor layout. */
  screen: { x: number; y: number; w: number; h: number } | null;
  /** Dense matrices are blitted as an image instead of dot by dot. */
  raster: { x: number; y: number; cell: number; w: number; h: number; index: Int32Array } | null;
}

const OFF = "#1b1e28";

export function project(lp: LayoutPreview, w: number, h: number, rot: Rotation = DEFAULT_ROTATION, pad = 20): Projection {
  const n = lp.count;
  const x = new Float32Array(n);
  const y = new Float32Array(n);
  const r = new Float32Array(n);
  const order = new Uint32Array(n);
  for (let i = 0; i < n; i++) order[i] = i;
  const availW = Math.max(10, w - pad * 2);
  const availH = Math.max(10, h - pad * 2);
  const proj: Projection = { count: n, x, y, r, radius: 4, order, square: false, volumetric: false, screen: null, raster: null };

  switch (lp.kind) {
    case "strip": {
      // Long strips wrap onto several rows so each LED stays visible.
      let cols = Math.min(n, Math.max(1, Math.floor(availW / 11)));
      const rows = Math.ceil(n / cols);
      cols = Math.ceil(n / rows);
      const pitch = Math.min(availW / cols, availH / rows, 38);
      const rowPitch = Math.min(availH / rows, pitch * 1.7);
      const offX = (w - cols * pitch) / 2;
      const offY = (h - rows * rowPitch) / 2;
      for (let i = 0; i < n; i++) {
        const slot = lp.grid[i][0];
        x[i] = offX + ((slot % cols) + 0.5) * pitch;
        y[i] = offY + (Math.floor(slot / cols) + 0.5) * rowPitch;
      }
      proj.radius = pitch * 0.37;
      break;
    }

    case "matrix": {
      const [gw, gh] = lp.dims;
      const cell = Math.min(availW / gw, availH / gh, 46);
      const offX = (w - gw * cell) / 2;
      const offY = (h - gh * cell) / 2;
      for (let i = 0; i < n; i++) {
        x[i] = offX + (lp.grid[i][0] + 0.5) * cell;
        y[i] = offY + (lp.grid[i][1] + 0.5) * cell;
      }
      proj.radius = cell * 0.38;
      proj.square = cell < 7;
      if (n > 1500) {
        const index = new Int32Array(gw * gh).fill(-1);
        for (let i = 0; i < n; i++) index[lp.grid[i][0] + lp.grid[i][1] * gw] = i;
        proj.raster = { x: offX, y: offY, cell, w: gw, h: gh, index };
      }
      break;
    }

    case "ring": {
      const R = Math.max(8, Math.min(availW, availH) / 2 - 8);
      for (let i = 0; i < n; i++) {
        x[i] = w / 2 + (lp.pos[i][0] - 0.5) * 2 * R;
        y[i] = h / 2 + (lp.pos[i][1] - 0.5) * 2 * R;
      }
      proj.radius = clamp(((2 * Math.PI * R) / Math.max(n, 1)) * 0.37, 1.5, 15);
      break;
    }

    case "monitor": {
      const margin = 30;
      const sw = Math.max(40, Math.min(availW - margin * 2, (availH - margin * 2) * (16 / 9)));
      const sh = (sw * 9) / 16;
      const sx = (w - sw) / 2;
      const sy = (h - sh) / 2;
      const out = 13;
      for (let i = 0; i < n; i++) {
        const [px, py] = lp.pos[i];
        x[i] = sx + px * sw + (px <= 0 ? -out : px >= 1 ? out : 0);
        y[i] = sy + py * sh + (py <= 0 ? -out : py >= 1 ? out : 0);
      }
      proj.radius = clamp(((2 * (sw + sh)) / Math.max(n, 1)) * 0.36, 2, 10);
      proj.screen = { x: sx, y: sy, w: sw, h: sh };
      break;
    }

    case "cube": {
      const m = Math.max(...lp.dims);
      const [kx, ky, kz] = [lp.dims[0] / m, lp.dims[1] / m, lp.dims[2] / m];
      const size = Math.min(availW, availH) * 0.66;
      const [cy, sy] = [Math.cos(rot.yaw), Math.sin(rot.yaw)];
      const [cp, sp] = [Math.cos(rot.pitch), Math.sin(rot.pitch)];
      const base = clamp((size / m) * 0.17, 1.4, 9);
      const depth = new Float32Array(n);
      for (let i = 0; i < n; i++) {
        const X = (lp.pos[i][0] - 0.5) * kx;
        const Y = (lp.pos[i][1] - 0.5) * ky;
        const Z = (lp.pos[i][2] - 0.5) * kz;
        const x1 = X * cy + Z * sy;
        const z1 = -X * sy + Z * cy;
        const y2 = Y * cp - z1 * sp;
        const z2 = Y * sp + z1 * cp;
        const persp = 2.6 / (2.6 + z2);
        x[i] = w / 2 + x1 * size * persp;
        y[i] = h / 2 + y2 * size * persp;
        r[i] = base * persp;
        depth[i] = z2;
      }
      order.sort((a, b) => depth[b] - depth[a]);
      proj.radius = base;
      proj.volumetric = true;
      return proj;
    }

    case "custom": {
      const side = Math.min(availW, availH);
      for (let i = 0; i < n; i++) {
        x[i] = (w - side) / 2 + lp.pos[i][0] * side;
        y[i] = (h - side) / 2 + lp.pos[i][1] * side;
      }
      proj.radius = clamp((side / Math.sqrt(Math.max(n, 1))) * 0.22, 2, 12);
      break;
    }
  }
  r.fill(proj.radius);
  return proj;
}

/** Index of the LED under a point, or -1. */
export function hitTest(proj: Projection, px: number, py: number, reach = 1.7): number {
  let best = -1;
  let bestD = Infinity;
  const limit = Math.max(proj.radius * reach, 7);
  for (let i = 0; i < proj.count; i++) {
    const d = Math.hypot(proj.x[i] - px, proj.y[i] - py);
    if (d < bestD && d <= limit) {
      bestD = d;
      best = i;
    }
  }
  return best;
}

/** Every LED within `radius` pixels of a point. */
export function hitArea(proj: Projection, px: number, py: number, radius: number): number[] {
  const hits: number[] = [];
  for (let i = 0; i < proj.count; i++) {
    if (Math.hypot(proj.x[i] - px, proj.y[i] - py) <= radius) hits.push(i);
  }
  return hits;
}

export interface DrawOptions {
  /** Ring one LED, e.g. the one being tested. */
  highlight?: number;
  /** Draw the wiring path, start/end markers and index numbers. */
  diagram?: boolean;
}

let rasterCanvas: HTMLCanvasElement | null = null;

function drawScreenFrame(ctx: CanvasRenderingContext2D, s: NonNullable<Projection["screen"]>) {
  ctx.fillStyle = "#0c0e14";
  ctx.strokeStyle = "#2b3040";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.roundRect(s.x, s.y, s.w, s.h, 6);
  ctx.fill();
  ctx.stroke();
  // A hint of a stand so "bottom" is unmistakable.
  ctx.fillStyle = "#1a1d27";
  ctx.beginPath();
  ctx.roundRect(s.x + s.w / 2 - s.w * 0.035, s.y + s.h + 2, s.w * 0.07, 16, 2);
  ctx.roundRect(s.x + s.w / 2 - s.w * 0.12, s.y + s.h + 16, s.w * 0.24, 5, 2);
  ctx.fill();
  ctx.fillStyle = "#3a4052";
  ctx.font = "11px system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillText("SCREEN (front view)", s.x + s.w / 2, s.y + s.h / 2);
}

/**
 * Draws one frame. `rgb` holds three bytes per LED; `glow` is an optional
 * second canvas (blurred with CSS) that carries the bloom.
 */
export function drawLeds(
  ctx: CanvasRenderingContext2D,
  glow: CanvasRenderingContext2D | null,
  proj: Projection,
  rgb: Uint8Array | null,
  w: number,
  h: number,
  opts: DrawOptions = {},
) {
  ctx.clearRect(0, 0, w, h);
  glow?.clearRect(0, 0, w, h);
  if (proj.screen) drawScreenFrame(ctx, proj.screen);

  const n = proj.count;
  const have = rgb && rgb.length >= n * 3 ? rgb : null;

  if (proj.raster && !opts.diagram) {
    const ra = proj.raster;
    rasterCanvas ??= document.createElement("canvas");
    if (rasterCanvas.width !== ra.w || rasterCanvas.height !== ra.h) {
      rasterCanvas.width = ra.w;
      rasterCanvas.height = ra.h;
    }
    const rctx = rasterCanvas.getContext("2d")!;
    const img = rctx.createImageData(ra.w, ra.h);
    for (let c = 0; c < ra.index.length; c++) {
      const i = ra.index[c];
      const o = c * 4;
      img.data[o] = have && i >= 0 ? Math.max(have[i * 3], 22) : 22;
      img.data[o + 1] = have && i >= 0 ? Math.max(have[i * 3 + 1], 24) : 24;
      img.data[o + 2] = have && i >= 0 ? Math.max(have[i * 3 + 2], 32) : 32;
      img.data[o + 3] = 255;
    }
    rctx.putImageData(img, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(rasterCanvas, ra.x, ra.y, ra.w * ra.cell, ra.h * ra.cell);
    if (glow) {
      glow.imageSmoothingEnabled = true;
      glow.drawImage(rasterCanvas, ra.x, ra.y, ra.w * ra.cell, ra.h * ra.cell);
    }
    return;
  }

  const fancy = n <= 700;
  const useGlow = glow && n <= 2500;
  if (useGlow) glow.globalCompositeOperation = "lighter";

  // Unlit LEDs in one path: far cheaper than a fill per dot. In a cube they
  // are drawn as faint specks so they don't hide the lit LEDs behind them.
  const vol = proj.volumetric && !opts.diagram;
  ctx.fillStyle = vol ? "rgba(150,160,190,0.2)" : OFF;
  ctx.beginPath();
  for (let k = 0; k < n; k++) {
    const i = proj.order[k];
    const lit = have ? (have[i * 3] | have[i * 3 + 1] | have[i * 3 + 2]) > 5 : opts.diagram;
    if (lit) continue;
    const rr = vol ? proj.r[i] * 0.3 : proj.r[i];
    if (proj.square) ctx.rect(proj.x[i] - rr, proj.y[i] - rr, rr * 2, rr * 2);
    else {
      ctx.moveTo(proj.x[i] + rr, proj.y[i]);
      ctx.arc(proj.x[i], proj.y[i], rr, 0, Math.PI * 2);
    }
  }
  ctx.fill();

  for (let k = 0; k < n; k++) {
    const i = proj.order[k];
    let cr: number, cg: number, cb: number;
    if (have) {
      cr = have[i * 3];
      cg = have[i * 3 + 1];
      cb = have[i * 3 + 2];
      if ((cr | cg | cb) <= 5) continue;
    } else if (opts.diagram) {
      // No frame: colour by position along the wiring, start (green) to end (pink).
      [cr, cg, cb] = hsvToRgb(0.33 + 0.55 * (n > 1 ? i / (n - 1) : 0), 0.75, 0.95);
    } else {
      continue;
    }
    const px = proj.x[i];
    const py = proj.y[i];
    let rr = proj.r[i];
    if (vol) {
      // Size and opacity follow brightness, which reads as depth in a cube.
      const level = Math.max(cr, cg, cb) / 255;
      rr *= 0.45 + 0.75 * level;
      ctx.globalAlpha = 0.35 + 0.65 * level;
      ctx.fillStyle = `rgb(${cr},${cg},${cb})`;
    } else {
      ctx.fillStyle = `rgb(${Math.max(cr, 27)},${Math.max(cg, 30)},${Math.max(cb, 40)})`;
    }
    if (proj.square) {
      ctx.fillRect(px - rr, py - rr, rr * 2, rr * 2);
    } else {
      ctx.beginPath();
      ctx.arc(px, py, rr, 0, Math.PI * 2);
      ctx.fill();
      if (fancy && rr > 3) {
        // A brighter core makes each dot read as a light source.
        ctx.fillStyle = `rgba(255,255,255,${0.1 + 0.25 * (Math.max(cr, cg, cb) / 255)})`;
        ctx.beginPath();
        ctx.arc(px, py, rr * 0.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
    if (useGlow && !opts.diagram) {
      glow.globalAlpha = vol ? 0.5 : 1;
      glow.fillStyle = `rgb(${cr},${cg},${cb})`;
      glow.beginPath();
      glow.arc(px, py, rr * 2.1, 0, Math.PI * 2);
      glow.fill();
    }
  }
  ctx.globalAlpha = 1;
  if (glow) glow.globalAlpha = 1;

  if (opts.diagram) drawWiring(ctx, proj);

  if (opts.highlight != null && opts.highlight >= 0 && opts.highlight < n) {
    const i = opts.highlight;
    ctx.strokeStyle = "#ffffff";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(proj.x[i], proj.y[i], proj.r[i] + 4, 0, Math.PI * 2);
    ctx.stroke();
  }
}

/** The path the data takes from LED to LED, with direction arrows. */
function drawWiring(ctx: CanvasRenderingContext2D, proj: Projection) {
  const n = proj.count;
  if (n < 1) return;
  const jump = Math.max(proj.radius * 5, 26);

  ctx.lineWidth = 1.4;
  ctx.lineJoin = "round";
  for (let i = 0; i < n - 1; i++) {
    const [x0, y0, x1, y1] = [proj.x[i], proj.y[i], proj.x[i + 1], proj.y[i + 1]];
    const long = Math.hypot(x1 - x0, y1 - y0) > jump;
    // Dashes mark where the wire has to travel between rows or layers.
    ctx.setLineDash(long ? [4, 5] : []);
    ctx.strokeStyle = long ? "rgba(255,255,255,0.22)" : "rgba(255,255,255,0.5)";
    ctx.beginPath();
    ctx.moveTo(x0, y0);
    ctx.lineTo(x1, y1);
    ctx.stroke();
  }
  ctx.setLineDash([]);

  // Arrowheads every so often.
  const every = Math.max(1, Math.round(n / 14));
  ctx.fillStyle = "rgba(255,255,255,0.9)";
  for (let i = Math.floor(every / 2); i < n - 1; i += every) {
    const [x0, y0, x1, y1] = [proj.x[i], proj.y[i], proj.x[i + 1], proj.y[i + 1]];
    const len = Math.hypot(x1 - x0, y1 - y0);
    if (len < 4 || len > jump) continue;
    const a = Math.atan2(y1 - y0, x1 - x0);
    const [mx, my] = [(x0 + x1) / 2, (y0 + y1) / 2];
    const s = clamp(proj.radius * 0.9, 3.5, 7);
    ctx.beginPath();
    ctx.moveTo(mx + Math.cos(a) * s, my + Math.sin(a) * s);
    ctx.lineTo(mx + Math.cos(a + 2.5) * s, my + Math.sin(a + 2.5) * s);
    ctx.lineTo(mx + Math.cos(a - 2.5) * s, my + Math.sin(a - 2.5) * s);
    ctx.closePath();
    ctx.fill();
  }

  // Index numbers when there is room for them.
  if (proj.radius >= 8.5) {
    ctx.fillStyle = "rgba(0,0,0,0.8)";
    ctx.font = `600 ${Math.min(11, proj.radius)}px system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    for (let i = 0; i < n; i++) ctx.fillText(String(i), proj.x[i], proj.y[i] + 0.5);
  }

  const marker = (i: number, colour: string, label: string, preferAbove: boolean) => {
    const [px, py] = [proj.x[i], proj.y[i]];
    ctx.strokeStyle = colour;
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.arc(px, py, proj.r[i] + 4, 0, Math.PI * 2);
    ctx.stroke();
    ctx.font = "600 11px system-ui, sans-serif";
    const tw = ctx.measureText(label).width + 12;
    // Keep the tag on the canvas.
    const tx = clamp(px, tw / 2 + 2, ctx.canvas.clientWidth - tw / 2 - 2);
    const roomAbove = py - proj.r[i] - 26 > 2;
    const roomBelow = py + proj.r[i] + 26 < ctx.canvas.clientHeight - 2;
    const above = preferAbove ? roomAbove || !roomBelow : !roomBelow && roomAbove;
    const ty = above ? py - proj.r[i] - 17 : py + proj.r[i] + 17;
    ctx.fillStyle = colour;
    ctx.beginPath();
    ctx.roundRect(tx - tw / 2, ty - 9, tw, 18, 9);
    ctx.fill();
    ctx.fillStyle = "#06120a";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(label, tx, ty + 0.5);
  };
  // START goes above its LED and END below, so the tags stay apart even when
  // a loop brings the two ends next to each other.
  if (n > 1) marker(n - 1, "#fb7185", `END ${n - 1}`, false);
  marker(0, "#4ade80", "START 0", true);
}
