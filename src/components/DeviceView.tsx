import { AudioLines, Bookmark, Clapperboard, Code, MonitorPlay, Power, SlidersHorizontal, Sparkles, Sun, X, Zap } from "lucide-react";
import { useState, type ReactNode } from "react";
import { ledCount, useApp, useLayoutPreview, type Tab } from "../store";
import type { Device, DeviceStatus, Source } from "../types";
import { uid } from "../util";
import { AmbientTab } from "./AmbientTab";
import { AnimationsTab } from "./AnimationsTab";
import { AudioTab } from "./AudioTab";
import { CodeTab } from "./CodeTab";
import { EffectsTab } from "./EffectsTab";
import { LedCanvas } from "./LedCanvas";
import { LAYOUT_ICONS, LAYOUT_NAMES, linkClass } from "./Sidebar";
import { Button, Notice } from "./ui";

const TABS: { id: Tab; label: string; icon: ReactNode }[] = [
  { id: "effects", label: "Effects", icon: <Sparkles size={16} /> },
  { id: "animations", label: "Animations", icon: <Clapperboard size={16} /> },
  { id: "ambient", label: "Ambient", icon: <MonitorPlay size={16} /> },
  { id: "audio", label: "Audio", icon: <AudioLines size={16} /> },
  { id: "code", label: "Code", icon: <Code size={16} /> },
];

function StatusChip({ device, status }: { device: Device; status?: DeviceStatus }) {
  const link = status?.link;
  const state = linkClass(link?.state, device.power);
  const text = !device.power
    ? "Off"
    : !link
      ? "Starting..."
      : link.state === "simulator"
        ? "Simulator"
        : link.state === "error"
          ? link.message
          : `${link.message}${link.fps > 0 ? ` · ${Math.round(link.fps)} fps` : ""}`;
  return (
    <span className={`chip chip--${state}`} title={link?.message}>
      <i className={`dot dot--${state}`} />
      {text}
    </span>
  );
}

export function DeviceView({ device }: { device: Device }) {
  const tab = useApp((s) => s.tab);
  const setTab = useApp((s) => s.setTab);
  const status = useApp((s) => s.status?.devices[device.id]);
  const effects = useApp((s) => s.effects);
  const animations = useApp((s) => s.config.animations);
  const scripts = useApp((s) => s.config.scripts);
  const presets = useApp((s) => s.config.presets);
  const updateDevice = useApp((s) => s.updateDevice);
  const setSource = useApp((s) => s.setSource);
  const savePreset = useApp((s) => s.savePreset);
  const removePreset = useApp((s) => s.removePreset);
  const openDialog = useApp((s) => s.openDialog);
  const layout = useLayoutPreview(device.layout);

  const [editing, setEditing] = useState(false);
  const [naming, setNaming] = useState<string | null>(null);

  const src = device.source;
  const activeEffect = src.type === "effect" ? effects.find((e) => e.id === src.effect) : undefined;
  const activeTab: Tab =
    src.type === "effect" ? (activeEffect?.audio ? "audio" : "effects") : src.type === "animation" ? "animations" : src.type === "ambient" ? "ambient" : "code";

  const sourceLabel = (s: Source): string => {
    switch (s.type) {
      case "effect":
        return effects.find((e) => e.id === s.effect)?.name ?? s.effect;
      case "animation":
        return animations.find((a) => a.id === s.animationId)?.name ?? "Animation";
      case "ambient":
        return "Ambient";
      case "script":
        return scripts.find((x) => x.id === s.scriptId)?.name ?? "Script";
    }
  };

  const amps = status ? status.milliamps / 1000 : 0;

  return (
    <div className="device-view">
      <header className="topbar">
        <div className="topbar__title">
          <h1>{device.name}</h1>
          <div className="topbar__meta">
            <span className="meta-item">
              {LAYOUT_ICONS[device.layout.type](14)} {LAYOUT_NAMES[device.layout.type]} · {ledCount(device.layout)} LEDs
            </span>
            <StatusChip device={device} status={status} />
            {status && device.power && (
              <span className={`meta-item ${status.limited ? "is-warn" : ""}`} title="Estimated current draw for 5 V addressable LEDs">
                <Zap size={13} /> {amps >= 1 ? `${amps.toFixed(1)} A` : `${status.milliamps} mA`}
                {status.limited ? " (limited)" : ""}
              </span>
            )}
          </div>
        </div>

        <div className="topbar__actions">
          <label className="brightness" title="Brightness">
            <Sun size={16} />
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={device.output.brightness}
              style={{ "--fill": `${device.output.brightness * 100}%` } as React.CSSProperties}
              onChange={(e) => updateDevice(device.id, (d) => ({ ...d, output: { ...d.output, brightness: Number(e.target.value) } }))}
            />
            <output>{Math.round(device.output.brightness * 100)}%</output>
          </label>
          <Button icon={<SlidersHorizontal size={16} />} onClick={() => openDialog({ type: "device", deviceId: device.id })}>
            Set up
          </Button>
          <button
            type="button"
            className={`power-btn ${device.power ? "is-on" : ""}`}
            title={device.power ? "Turn off" : "Turn on"}
            onClick={() => updateDevice(device.id, (d) => ({ ...d, power: !d.power }))}
          >
            <Power size={18} />
          </button>
        </div>
      </header>

      {status?.note && <Notice kind="error">{status.note}</Notice>}

      {!editing && (
        <>
          <section className="stage">
            <LedCanvas layout={layout} deviceId={device.id} autoRotate />
            <div className="stage__label">{device.power ? sourceLabel(src) : "Off"}</div>
          </section>

          <div className="presets">
            <Bookmark size={14} className="presets__icon" />
            {presets.map((p) => (
              <span key={p.id} className="preset">
                <button type="button" onClick={() => setSource(device.id, p.source)} title={`Apply "${p.name}"`}>
                  {p.name}
                </button>
                <button type="button" className="preset__x" onClick={() => removePreset(p.id)} title="Delete preset">
                  <X size={12} />
                </button>
              </span>
            ))}
            {naming == null ? (
              <button type="button" className="preset preset--add" onClick={() => setNaming(sourceLabel(src))}>
                + Save as preset
              </button>
            ) : (
              <form
                className="preset preset--form"
                onSubmit={(e) => {
                  e.preventDefault();
                  if (naming.trim()) savePreset({ id: uid(), name: naming.trim(), source: src });
                  setNaming(null);
                }}
              >
                <input
                  autoFocus
                  value={naming}
                  onChange={(e) => setNaming(e.target.value)}
                  onBlur={() => setNaming(null)}
                  onKeyDown={(e) => e.key === "Escape" && setNaming(null)}
                  placeholder="Preset name"
                />
              </form>
            )}
          </div>
        </>
      )}

      {!editing && (
        <nav className="tabs">
          {TABS.map((t) => (
            <button type="button" key={t.id} className={tab === t.id ? "is-active" : ""} onClick={() => setTab(t.id)}>
              {t.icon}
              {t.label}
              {activeTab === t.id && <i className="tabs__live" title="This is what the device is showing" />}
            </button>
          ))}
        </nav>
      )}

      <section className="tab-body">
        {tab === "effects" && <EffectsTab device={device} />}
        {tab === "animations" && <AnimationsTab device={device} onEditing={setEditing} />}
        {tab === "ambient" && <AmbientTab device={device} />}
        {tab === "audio" && <AudioTab device={device} />}
        {tab === "code" && <CodeTab device={device} />}
      </section>
    </div>
  );
}
