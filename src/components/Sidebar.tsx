import { Box, Circle, Cpu, Grid3x3, Minus, Monitor, Plus, Power, Settings, Shapes } from "lucide-react";
import type { ReactNode } from "react";
import { ledCount, useApp } from "../store";
import type { Device, LayoutType, LinkState } from "../types";

export const LAYOUT_ICONS: Record<LayoutType, (size: number) => ReactNode> = {
  strip: (s) => <Minus size={s} />,
  matrix: (s) => <Grid3x3 size={s} />,
  cube: (s) => <Box size={s} />,
  ring: (s) => <Circle size={s} />,
  monitor: (s) => <Monitor size={s} />,
  custom: (s) => <Shapes size={s} />,
};

export const LAYOUT_NAMES: Record<LayoutType, string> = {
  strip: "Strip",
  matrix: "Matrix",
  cube: "3D cube",
  ring: "Ring",
  monitor: "Monitor backlight",
  custom: "Free-form",
};

export function connectionLabel(device: Device): string {
  const c = device.connection;
  if (c.type === "serial") return c.port || "Serial";
  if (c.type === "network") return c.host || "Network";
  return "Simulator";
}

export function linkClass(state: LinkState | undefined, power: boolean): string {
  if (!power) return "off";
  return state ?? "simulator";
}

export function Sidebar() {
  const devices = useApp((s) => s.config.devices);
  const selectedId = useApp((s) => s.selectedId);
  const status = useApp((s) => s.status);
  const version = useApp((s) => s.version);
  const select = useApp((s) => s.select);
  const openDialog = useApp((s) => s.openDialog);
  const updateDevice = useApp((s) => s.updateDevice);

  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand__mark" aria-hidden>
          {Array.from({ length: 8 }, (_, i) => (
            <i key={i} style={{ "--i": i } as React.CSSProperties} />
          ))}
        </div>
        <div>
          <div className="brand__name">OpenLMI</div>
          <div className="brand__sub">LED control</div>
        </div>
      </div>

      <div className="sidebar__label">Devices</div>
      <nav className="device-list">
        {devices.map((d) => {
          const link = status?.devices[d.id]?.link;
          return (
            <div
              key={d.id}
              className={`device-item ${d.id === selectedId ? "is-selected" : ""}`}
              onClick={() => select(d.id)}
              role="button"
              tabIndex={0}
              onKeyDown={(e) => e.key === "Enter" && select(d.id)}
            >
              <span className="device-item__icon">{LAYOUT_ICONS[d.layout.type](17)}</span>
              <span className="device-item__text">
                <span className="device-item__name">{d.name}</span>
                <span className="device-item__sub">
                  <i className={`dot dot--${linkClass(link?.state, d.power)}`} title={link?.message} />
                  {ledCount(d.layout)} LEDs · {connectionLabel(d)}
                </span>
              </span>
              <button
                type="button"
                className={`power-mini ${d.power ? "is-on" : ""}`}
                title={d.power ? "Turn off" : "Turn on"}
                onClick={(e) => {
                  e.stopPropagation();
                  updateDevice(d.id, (x) => ({ ...x, power: !x.power }));
                }}
              >
                <Power size={14} />
              </button>
            </div>
          );
        })}
        <button type="button" className="device-add" onClick={() => openDialog({ type: "device", deviceId: null })}>
          <Plus size={16} /> Add device
        </button>
      </nav>

      <div className="sidebar__foot">
        <button type="button" className="side-link" onClick={() => openDialog({ type: "firmware" })}>
          <Cpu size={16} /> Firmware
        </button>
        <button type="button" className="side-link" onClick={() => openDialog({ type: "settings" })}>
          <Settings size={16} /> Settings
        </button>
        <div className="sidebar__meta">
          v{version}
          {status ? ` · ${Math.round(status.fps)} fps` : ""}
        </div>
      </div>
    </aside>
  );
}
