export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function uid(): string {
  return crypto.randomUUID().replace(/-/g, "").slice(0, 12);
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

export function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

export type RGB = [number, number, number];

export function hexToRgb(hex: string): RGB {
  let h = hex.trim().replace(/^#/, "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const v = parseInt(h, 16);
  if (h.length !== 6 || Number.isNaN(v)) return [255, 255, 255];
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

export function rgbToHex([r, g, b]: RGB): string {
  const part = (v: number) => clamp(Math.round(v), 0, 255).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

/** h, s, v in 0..1 */
export function hsvToRgb(h: number, s: number, v: number): RGB {
  const hh = (((h % 1) + 1) % 1) * 6;
  const i = Math.floor(hh);
  const f = hh - i;
  const p = v * (1 - s);
  const q = v * (1 - s * f);
  const t = v * (1 - s * (1 - f));
  const [r, g, b] = [
    [v, t, p],
    [q, v, p],
    [p, v, t],
    [p, q, v],
    [t, p, v],
    [v, p, q],
  ][i % 6];
  return [r * 255, g * 255, b * 255];
}

export function rgbToHsv([r, g, b]: RGB): [number, number, number] {
  const [rr, gg, bb] = [r / 255, g / 255, b / 255];
  const max = Math.max(rr, gg, bb);
  const min = Math.min(rr, gg, bb);
  const d = max - min;
  let h = 0;
  if (d > 0) {
    if (max === rr) h = ((gg - bb) / d) % 6;
    else if (max === gg) h = (bb - rr) / d + 2;
    else h = (rr - gg) / d + 4;
    h /= 6;
    if (h < 0) h += 1;
  }
  return [h, max === 0 ? 0 : d / max, max];
}

/** Samples a list of hex stops as a one-way gradient, t in 0..1. */
export function samplePalette(colors: string[], t: number): RGB {
  if (colors.length === 0) return [255, 255, 255];
  if (colors.length === 1) return hexToRgb(colors[0]);
  const p = clamp(t, 0, 1) * (colors.length - 1);
  const i = Math.min(Math.floor(p), colors.length - 2);
  const a = hexToRgb(colors[i]);
  const b = hexToRgb(colors[i + 1]);
  const k = p - i;
  return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
}

export function gradientCss(colors: string[], loop = false): string {
  const stops = loop && colors.length > 1 ? [...colors, colors[0]] : colors;
  if (stops.length === 1) return stops[0];
  return `linear-gradient(90deg, ${stops.join(", ")})`;
}

/** Runs `fn` at most once per animation frame with the latest arguments. */
export function frameThrottle<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
  let pending: A | null = null;
  let scheduled = false;
  return (...args: A) => {
    pending = args;
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      if (pending) fn(...pending);
      pending = null;
    });
  };
}

export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms: number): (...args: A) => void {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return (...args: A) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

export function formatNumber(v: number, step: number): string {
  if (step >= 1) return String(Math.round(v));
  const digits = step >= 0.1 ? 1 : step >= 0.01 ? 2 : 3;
  return v.toFixed(digits);
}

export function plural(n: number, word: string): string {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}
