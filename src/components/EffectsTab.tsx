import {
  Activity,
  AudioLines,
  AudioWaveform,
  Blend,
  Circle,
  CircleDot,
  CloudRain,
  Droplets,
  Flame,
  Gauge,
  Heart,
  Orbit,
  Paintbrush,
  PartyPopper,
  Rainbow,
  RefreshCw,
  RotateCcw,
  ScanLine,
  Shuffle,
  Siren,
  Sparkle,
  Sparkles,
  Star,
  Sunrise,
  Type,
  Waves,
  Wind,
  Zap,
} from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import { defaultParams, isGridLayout, useApp } from "../store";
import type { Device, EffectInfo, ParamDef, ParamValue, Params } from "../types";
import { gradientCss } from "../util";
import { ColorField, PaletteField } from "./ColorPicker";
import { Button, Empty, Field, Select, Slider, TextInput, Toggle } from "./ui";

const ICONS: Record<string, ReactNode> = {
  solid: <Circle size={20} />,
  gradient: <Blend size={20} />,
  rainbow: <Rainbow size={20} />,
  cycle: <RefreshCw size={20} />,
  breathe: <Wind size={20} />,
  heartbeat: <Heart size={20} />,
  wipe: <Paintbrush size={20} />,
  chase: <CircleDot size={20} />,
  scanner: <ScanLine size={20} />,
  comet: <Orbit size={20} />,
  running: <Activity size={20} />,
  meteor: <Star size={20} />,
  bounce: <Shuffle size={20} />,
  juggle: <Sparkle size={20} />,
  fire: <Flame size={20} />,
  plasma: <Droplets size={20} />,
  flow: <Waves size={20} />,
  aurora: <Sunrise size={20} />,
  waves: <Waves size={20} />,
  ripple: <CircleDot size={20} />,
  rain: <CloudRain size={20} />,
  twinkle: <Sparkles size={20} />,
  sparkle: <Sparkle size={20} />,
  confetti: <PartyPopper size={20} />,
  police: <Siren size={20} />,
  strobe: <Zap size={20} />,
  text: <Type size={20} />,
  spectrum: <AudioLines size={20} />,
  vu: <Gauge size={20} />,
  beat: <Heart size={20} />,
  soundwave: <AudioWaveform size={20} />,
  glow: <Sunrise size={20} />,
};

/** The colours an effect starts with, for its card. */
function cardColors(effect: EffectInfo, paletteColors: (name: string) => string[]): string[] {
  const pal = effect.params.find((p) => p.kind === "palette");
  if (pal && pal.kind === "palette") return paletteColors(pal.default);
  const colors = effect.params.flatMap((p) => (p.kind === "color" ? [p.default] : []));
  if (colors.length) return colors.filter((c) => c !== "#000000");
  return paletteColors("Rainbow");
}

function ParamControl({ def, value, onChange }: { def: ParamDef; value: ParamValue | undefined; onChange: (v: ParamValue) => void }) {
  switch (def.kind) {
    case "slider":
      return (
        <Slider
          label={def.label}
          min={def.min}
          max={def.max}
          step={def.step}
          value={typeof value === "number" ? value : def.default}
          resetTo={def.default}
          onChange={onChange}
        />
      );
    case "toggle":
      return <Toggle label={def.label} checked={typeof value === "boolean" ? value : def.default} onChange={onChange} />;
    case "color":
      return (
        <Field label={def.label} row>
          <ColorField value={typeof value === "string" ? value : def.default} onChange={onChange} />
        </Field>
      );
    case "palette":
      return (
        <Field label={def.label}>
          <PaletteField value={Array.isArray(value) ? value : []} onChange={onChange} />
        </Field>
      );
    case "select":
      return (
        <Field label={def.label} row>
          <Select value={typeof value === "string" ? value : def.default} options={def.options.map((o) => ({ value: o, label: o }))} onChange={onChange} />
        </Field>
      );
    case "text":
      return (
        <Field label={def.label}>
          <TextInput value={typeof value === "string" ? value : def.default} onChange={onChange} />
        </Field>
      );
  }
}

interface BrowserProps {
  device: Device;
  /** Which effects to list. */
  filter: (e: EffectInfo) => boolean;
  /** Extra content above the parameter list. */
  aside?: ReactNode;
}

/** Effect cards on the left, the active effect's controls on the right. */
export function EffectBrowser({ device, filter, aside }: BrowserProps) {
  const effects = useApp((s) => s.effects);
  const palettes = useApp((s) => s.palettes);
  const memory = useApp((s) => s.effectMemory);
  const setSource = useApp((s) => s.setSource);
  const [category, setCategory] = useState("All");

  const list = useMemo(() => effects.filter(filter), [effects, filter]);
  const categories = useMemo(() => ["All", ...new Set(list.map((e) => e.category))], [list]);
  const shown = category === "All" ? list : list.filter((e) => e.category === category);
  const grid = isGridLayout(device.layout);

  const src = device.source;
  const active = src.type === "effect" ? list.find((e) => e.id === src.effect) : undefined;
  const params: Params = src.type === "effect" && active ? { ...defaultParams(active, palettes), ...src.params } : {};
  const paletteColors = (name: string) => palettes.find((p) => p.name === name)?.colors ?? ["#ffffff"];

  const choose = (effect: EffectInfo) => {
    const remembered = memory[`${device.id}:${effect.id}`];
    setSource(device.id, { type: "effect", effect: effect.id, params: remembered ?? defaultParams(effect, palettes) });
  };

  return (
    <div className="split">
      <div className="split__main">
        {categories.length > 2 && (
          <div className="filter-row">
            {categories.map((c) => (
              <button type="button" key={c} className={`pill ${c === category ? "is-active" : ""}`} onClick={() => setCategory(c)}>
                {c}
              </button>
            ))}
          </div>
        )}
        <div className="card-grid">
          {shown.map((e) => {
            const needsGrid = e.gridOnly && !grid;
            return (
              <button
                type="button"
                key={e.id}
                className={`fx-card ${active?.id === e.id ? "is-active" : ""} ${needsGrid ? "is-dim" : ""}`}
                onClick={() => choose(e)}
                title={needsGrid ? `${e.description} (made for matrices)` : e.description}
              >
                <span className="fx-card__icon">{ICONS[e.id] ?? <Sparkles size={20} />}</span>
                <span className="fx-card__name">{e.name}</span>
                <span className="fx-card__bar" style={{ background: gradientCss(cardColors(e, paletteColors)) }} />
              </button>
            );
          })}
        </div>
      </div>

      <aside className="split__side">
        {aside}
        {active ? (
          <div className="panel">
            <div className="panel__head">
              <div>
                <h3>{active.name}</h3>
                <p>{active.description}</p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                icon={<RotateCcw size={14} />}
                title="Reset to defaults"
                onClick={() => setSource(device.id, { type: "effect", effect: active.id, params: defaultParams(active, palettes) })}
              />
            </div>
            <div className="panel__body">
              {active.params.map((def) => (
                <ParamControl
                  key={def.key}
                  def={def}
                  value={params[def.key]}
                  onChange={(v) => setSource(device.id, { type: "effect", effect: active.id, params: { ...params, [def.key]: v } })}
                />
              ))}
              {active.params.length === 0 && <p className="muted">Nothing to adjust here.</p>}
            </div>
          </div>
        ) : (
          <div className="panel">
            <Empty title="Pick an effect">Choose a card and its controls show up here.</Empty>
          </div>
        )}
      </aside>
    </div>
  );
}

const notAudio = (e: EffectInfo) => !e.audio;

export function EffectsTab({ device }: { device: Device }) {
  return <EffectBrowser device={device} filter={notAudio} />;
}
