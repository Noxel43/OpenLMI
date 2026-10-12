import { Minus, Plus } from "lucide-react";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useApp } from "../store";
import { clamp, gradientCss, hexToRgb, hsvToRgb, rgbToHex, rgbToHsv } from "../util";
import { Button, Popover } from "./ui";

const SWATCHES = ["#ff0000", "#ff7a00", "#ffd000", "#3dff00", "#00ffd0", "#0066ff", "#8f00ff", "#ff00a8", "#ffffff", "#ffb46b", "#000000"];

function useDrag(onMove: (fx: number, fy: number) => void) {
  const ref = useRef<HTMLDivElement>(null);
  const handle = (e: React.PointerEvent) => {
    const rect = ref.current!.getBoundingClientRect();
    onMove(clamp((e.clientX - rect.left) / rect.width, 0, 1), clamp((e.clientY - rect.top) / rect.height, 0, 1));
  };
  return {
    ref,
    onPointerDown: (e: React.PointerEvent) => {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      handle(e);
    },
    onPointerMove: (e: React.PointerEvent) => {
      if (e.buttons & 1) handle(e);
    },
  };
}

/** Saturation/value square, hue bar, hex field and quick swatches. */
export function ColorPicker({ value, onChange }: { value: string; onChange: (hex: string) => void }) {
  // Hue is kept locally: it is undefined for greys and would otherwise snap
  // back to red whenever the colour passes through black or white.
  const [hsv, setHsv] = useState(() => rgbToHsv(hexToRgb(value)));
  const [text, setText] = useState(value);

  useEffect(() => {
    if (rgbToHex(hsvToRgb(...hsv)).toLowerCase() !== value.toLowerCase()) {
      const next = rgbToHsv(hexToRgb(value));
      setHsv([next[1] === 0 || next[2] === 0 ? hsv[0] : next[0], next[1], next[2]]);
    }
    setText(value);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  const apply = (next: [number, number, number]) => {
    setHsv(next);
    onChange(rgbToHex(hsvToRgb(...next)));
  };

  const sv = useDrag((fx, fy) => apply([hsv[0], fx, 1 - fy]));
  const hue = useDrag((fx) => apply([fx, hsv[1], hsv[2]]));

  return (
    <div className="picker">
      <div
        className="picker__sv"
        {...sv}
        style={{ "--hue": `hsl(${hsv[0] * 360} 100% 50%)` } as CSSProperties}
      >
        <span className="picker__thumb" style={{ left: `${hsv[1] * 100}%`, top: `${(1 - hsv[2]) * 100}%`, background: value }} />
      </div>
      <div className="picker__hue" {...hue}>
        <span className="picker__thumb" style={{ left: `${hsv[0] * 100}%`, top: "50%", background: `hsl(${hsv[0] * 360} 100% 50%)` }} />
      </div>
      <div className="picker__row">
        <span className="picker__preview" style={{ background: value }} />
        <input
          className="input input--mono"
          value={text}
          spellCheck={false}
          onChange={(e) => {
            setText(e.target.value);
            if (/^#?[0-9a-fA-F]{6}$/.test(e.target.value.trim())) onChange(rgbToHex(hexToRgb(e.target.value)));
          }}
        />
      </div>
      <div className="picker__swatches">
        {SWATCHES.map((c) => (
          <button type="button" key={c} style={{ background: c }} title={c} onClick={() => onChange(c)} />
        ))}
      </div>
    </div>
  );
}

/** A swatch that opens the colour picker. */
export function ColorField({ value, onChange, label }: { value: string; onChange: (hex: string) => void; label?: string }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button type="button" ref={anchor} className="color-field" onClick={() => setOpen((o) => !o)}>
        <span className="color-field__chip" style={{ background: value }} />
        <span className="color-field__text">{label ?? value.toUpperCase()}</span>
      </button>
      {open && (
        <Popover anchor={anchor} onClose={() => setOpen(false)} width={248}>
          <ColorPicker value={value} onChange={onChange} />
        </Popover>
      )}
    </>
  );
}

/** Picks a built-in palette or edits the colour stops by hand. */
export function PaletteField({ value, onChange }: { value: string[]; onChange: (colors: string[]) => void }) {
  const palettes = useApp((s) => s.palettes);
  const [open, setOpen] = useState(false);
  const [stop, setStop] = useState(0);
  const anchor = useRef<HTMLButtonElement>(null);
  const colors = value.length ? value : ["#ffffff"];
  const active = Math.min(stop, colors.length - 1);
  const named = palettes.find((p) => p.colors.length === colors.length && p.colors.every((c, i) => c.toLowerCase() === colors[i].toLowerCase()));

  return (
    <>
      <button type="button" ref={anchor} className="palette-field" onClick={() => setOpen((o) => !o)}>
        <span className="palette-field__bar" style={{ background: gradientCss(colors) }} />
        <span className="palette-field__name">{named?.name ?? "Custom"}</span>
      </button>
      {open && (
        <Popover anchor={anchor} onClose={() => setOpen(false)} width={300}>
          <div className="palette-pop">
            <div className="palette-pop__list">
              {palettes.map((p) => (
                <button
                  type="button"
                  key={p.name}
                  className={named?.name === p.name ? "is-active" : ""}
                  onClick={() => {
                    onChange(p.colors);
                    setStop(0);
                  }}
                >
                  <span style={{ background: gradientCss(p.colors) }} />
                  {p.name}
                </button>
              ))}
            </div>
            <div className="palette-pop__custom">
              <div className="palette-pop__title">Colours</div>
              <div className="palette-pop__stops">
                {colors.map((c, i) => (
                  <button
                    type="button"
                    key={i}
                    className={i === active ? "is-active" : ""}
                    style={{ background: c }}
                    onClick={() => setStop(i)}
                    aria-label={`Colour ${i + 1}`}
                  />
                ))}
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Plus size={14} />}
                  title="Add a colour"
                  disabled={colors.length >= 12}
                  onClick={() => {
                    onChange([...colors, colors[colors.length - 1]]);
                    setStop(colors.length);
                  }}
                />
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Minus size={14} />}
                  title="Remove this colour"
                  disabled={colors.length <= 1}
                  onClick={() => {
                    onChange(colors.filter((_, i) => i !== active));
                    setStop(Math.max(0, active - 1));
                  }}
                />
              </div>
              <ColorPicker value={colors[active]} onChange={(hex) => onChange(colors.map((c, i) => (i === active ? hex : c)))} />
            </div>
          </div>
        </Popover>
      )}
    </>
  );
}
