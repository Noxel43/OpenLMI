// The built-in 8x8 pixel-art icons: thumbnails, a picker, and the maths for
// placing one on a grid (kept in step with src-tauri/src/effects/icons.rs).

import { useMemo } from "react";
import { useApp } from "../store";
import type { IconInfo } from "../types";
import { hexToRgb, type RGB } from "../util";

export const ICON_SIZE = 8;

const urls = new Map<string, string>();

/** A tiny image of the icon, meant to be scaled up with crisp pixels. */
export function iconUrl(icon: IconInfo): string {
  let url = urls.get(icon.name);
  if (!url) {
    const canvas = document.createElement("canvas");
    canvas.width = ICON_SIZE;
    canvas.height = ICON_SIZE;
    const ctx = canvas.getContext("2d")!;
    icon.pixels.forEach((hex, i) => {
      if (!hex) return;
      ctx.fillStyle = hex;
      ctx.fillRect(i % ICON_SIZE, Math.floor(i / ICON_SIZE), 1, 1);
    });
    url = canvas.toDataURL();
    urls.set(icon.name, url);
  }
  return url;
}

/**
 * The icon's colour at grid cell (x, y) of a `width` x `height` matrix, or
 * null where the icon is unlit or does not reach. The icon is centred and
 * zoomed by the largest whole number that fits.
 */
export function iconColourAt(icon: IconInfo, width: number, height: number, x: number, y: number): RGB | null {
  const fit = Math.min(width, height) / ICON_SIZE;
  const zoom = fit >= 1 ? Math.floor(fit) : fit;
  const size = ICON_SIZE * zoom;
  const ix = Math.floor((x - Math.floor((width - size) / 2)) / zoom);
  const iy = Math.floor((y - Math.floor((height - size) / 2)) / zoom);
  if (ix < 0 || iy < 0 || ix >= ICON_SIZE || iy >= ICON_SIZE) return null;
  const hex = icon.pixels[iy * ICON_SIZE + ix];
  return hex ? hexToRgb(hex) : null;
}

/** Every icon, grouped, as a grid of buttons. */
export function IconPicker({ value, onPick }: { value?: string; onPick: (icon: IconInfo) => void }) {
  const icons = useApp((s) => s.icons);
  const groups = useMemo(() => {
    const byGroup = new Map<string, IconInfo[]>();
    for (const icon of icons) byGroup.set(icon.group, [...(byGroup.get(icon.group) ?? []), icon]);
    return [...byGroup.entries()];
  }, [icons]);

  return (
    <div className="icon-picker">
      {groups.map(([group, list]) => (
        <div key={group}>
          <div className="icon-picker__group">{group}</div>
          <div className="icon-picker__grid">
            {list.map((icon) => (
              <button
                type="button"
                key={icon.name}
                className={icon.name === value ? "is-active" : ""}
                title={icon.name}
                onClick={() => onPick(icon)}
              >
                <img src={iconUrl(icon)} alt={icon.name} draggable={false} />
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
