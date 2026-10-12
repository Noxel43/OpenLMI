import { useEffect } from "react";
import { create } from "zustand";
import { api } from "./api";
import type {
  AmbientSettings,
  Animation,
  Config,
  Device,
  EffectInfo,
  IconInfo,
  Layout,
  LayoutPreview,
  PaletteInfo,
  Params,
  Preset,
  Script,
  ScriptExample,
  Settings,
  Source,
  Status,
} from "./types";

export type Tab = "effects" | "animations" | "ambient" | "audio" | "code";

export type Dialog =
  | null
  | { type: "device"; deviceId: string | null }
  | { type: "settings" }
  | { type: "firmware"; deviceId?: string; animationId?: string };

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "error" | "success";
}

const EMPTY_CONFIG: Config = {
  version: 1,
  devices: [],
  animations: [],
  scripts: [],
  presets: [],
  settings: { fps: 60, captureFps: 30, closeToTray: false, startMinimized: false, audioSource: "loopback", audioGain: 1 },
};

export const DEFAULT_AMBIENT: AmbientSettings = {
  monitor: "",
  depth: 0.14,
  smoothing: 0.55,
  saturation: 1.35,
  gain: 1,
  minBrightness: 0,
  blackBars: true,
  mapping: "zones",
  average: "mean",
};

interface AppState {
  ready: boolean;
  error: string | null;
  config: Config;
  effects: EffectInfo[];
  palettes: PaletteInfo[];
  icons: IconInfo[];
  scriptExamples: ScriptExample[];
  version: string;
  configPath: string;

  selectedId: string | null;
  tab: Tab;
  status: Status | null;
  layouts: Record<string, LayoutPreview>;
  dialog: Dialog;
  toasts: Toast[];
  /** Last parameters used for each effect on each device, so switching
   *  between effects doesn't throw tweaks away. */
  effectMemory: Record<string, Params>;
  ambientMemory: Record<string, AmbientSettings>;

  init: () => Promise<void>;
  reloadConfig: () => Promise<void>;
  select: (id: string | null) => void;
  setTab: (tab: Tab) => void;
  openDialog: (dialog: Dialog) => void;
  toast: (text: string, kind?: Toast["kind"]) => void;
  dismissToast: (id: number) => void;

  updateDevice: (id: string, change: (device: Device) => Device) => void;
  setSource: (id: string, source: Source) => void;
  addDevice: (device: Device) => void;
  removeDevice: (id: string) => void;

  saveAnimation: (animation: Animation) => void;
  removeAnimation: (id: string) => void;
  saveScript: (script: Script) => void;
  removeScript: (id: string) => void;
  savePreset: (preset: Preset) => void;
  removePreset: (id: string) => void;
  saveSettings: (settings: Settings) => void;
  replaceConfig: (config: Config) => void;
}

// Device edits are coalesced to one backend call per animation frame.
const pendingDevices = new Map<string, Device>();
let flushScheduled = false;
function queueDeviceSave(device: Device) {
  pendingDevices.set(device.id, device);
  if (flushScheduled) return;
  flushScheduled = true;
  requestAnimationFrame(() => {
    flushScheduled = false;
    for (const d of pendingDevices.values()) void api.saveDevice(d);
    pendingDevices.clear();
  });
}

let toastId = 0;

function upsert<T extends { id: string }>(list: T[], item: T): T[] {
  return list.some((x) => x.id === item.id) ? list.map((x) => (x.id === item.id ? item : x)) : [...list, item];
}

export const useApp = create<AppState>((set, get) => ({
  ready: false,
  error: null,
  config: EMPTY_CONFIG,
  effects: [],
  palettes: [],
  icons: [],
  scriptExamples: [],
  version: "",
  configPath: "",
  selectedId: null,
  tab: "effects",
  status: null,
  layouts: {},
  dialog: null,
  toasts: [],
  effectMemory: {},
  ambientMemory: {},

  init: async () => {
    try {
      const boot = await api.bootstrap();
      set({
        ready: true,
        config: boot.config,
        effects: boot.effects,
        palettes: boot.palettes,
        icons: boot.icons,
        scriptExamples: boot.scriptExamples,
        version: boot.version,
        configPath: boot.configPath,
        selectedId: boot.config.devices[0]?.id ?? null,
      });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  reloadConfig: async () => {
    const config = await api.getConfig();
    get().replaceConfig(config);
  },

  replaceConfig: (config) =>
    set((s) => ({
      config,
      selectedId: config.devices.some((d) => d.id === s.selectedId) ? s.selectedId : (config.devices[0]?.id ?? null),
    })),

  select: (id) => set({ selectedId: id }),
  setTab: (tab) => set({ tab }),
  openDialog: (dialog) => set({ dialog }),

  toast: (text, kind = "info") => {
    const id = ++toastId;
    set((s) => ({ toasts: [...s.toasts, { id, text, kind }] }));
    setTimeout(() => get().dismissToast(id), kind === "error" ? 7000 : 3500);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  updateDevice: (id, change) => {
    const current = get().config.devices.find((d) => d.id === id);
    if (!current) return;
    const next = change(current);
    set((s) => ({ config: { ...s.config, devices: s.config.devices.map((d) => (d.id === id ? next : d)) } }));
    queueDeviceSave(next);
  },

  setSource: (id, source) => {
    // Remember what was dialled in so it can be restored later.
    if (source.type === "effect") {
      set((s) => ({ effectMemory: { ...s.effectMemory, [`${id}:${source.effect}`]: source.params } }));
    } else if (source.type === "ambient") {
      const { type: _type, ...settings } = source;
      set((s) => ({ ambientMemory: { ...s.ambientMemory, [id]: settings } }));
    }
    get().updateDevice(id, (d) => ({ ...d, source }));
  },

  addDevice: (device) => {
    set((s) => ({ config: { ...s.config, devices: [...s.config.devices, device] }, selectedId: device.id }));
    void api.saveDevice(device);
  },

  removeDevice: (id) => {
    set((s) => {
      const devices = s.config.devices.filter((d) => d.id !== id);
      return { config: { ...s.config, devices }, selectedId: s.selectedId === id ? (devices[0]?.id ?? null) : s.selectedId };
    });
    void api.deleteDevice(id);
  },

  saveAnimation: (animation) => {
    set((s) => ({ config: { ...s.config, animations: upsert(s.config.animations, animation) } }));
    void api.saveAnimation(animation);
  },
  removeAnimation: (id) => {
    set((s) => ({ config: { ...s.config, animations: s.config.animations.filter((a) => a.id !== id) } }));
    void api.deleteAnimation(id);
  },
  saveScript: (script) => {
    set((s) => ({ config: { ...s.config, scripts: upsert(s.config.scripts, script) } }));
    void api.saveScript(script);
  },
  removeScript: (id) => {
    set((s) => ({ config: { ...s.config, scripts: s.config.scripts.filter((x) => x.id !== id) } }));
    void api.deleteScript(id);
  },
  savePreset: (preset) => {
    set((s) => ({ config: { ...s.config, presets: upsert(s.config.presets, preset) } }));
    void api.savePreset(preset);
  },
  removePreset: (id) => {
    set((s) => ({ config: { ...s.config, presets: s.config.presets.filter((p) => p.id !== id) } }));
    void api.deletePreset(id);
  },
  saveSettings: (settings) => {
    set((s) => ({ config: { ...s.config, settings } }));
    void api.saveSettings(settings);
  },
}));

export function useSelectedDevice(): Device | null {
  return useApp((s) => s.config.devices.find((d) => d.id === s.selectedId) ?? null);
}

/** Positions for drawing a layout, fetched from the backend once and cached. */
export function useLayoutPreview(layout: Layout | null | undefined): LayoutPreview | null {
  const key = layout ? JSON.stringify(layout) : "";
  const preview = useApp((s) => (key ? s.layouts[key] : undefined));
  useEffect(() => {
    if (!layout || preview) return;
    let cancelled = false;
    api.layoutPreview(layout).then((p) => {
      if (!cancelled) useApp.setState((s) => ({ layouts: { ...s.layouts, [key]: p } }));
    });
    return () => {
      cancelled = true;
    };
    // `key` captures the layout's contents.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, preview]);
  return preview ?? null;
}

/** Starting parameter values for an effect. */
export function defaultParams(effect: EffectInfo, palettes: PaletteInfo[]): Params {
  const params: Params = {};
  for (const p of effect.params) {
    if (p.kind === "palette") {
      params[p.key] = palettes.find((x) => x.name === p.default)?.colors ?? ["#ffffff"];
    } else {
      params[p.key] = p.default;
    }
  }
  return params;
}

export function isGridLayout(layout: Layout): boolean {
  return layout.type === "matrix" || layout.type === "cube";
}

export function ledCount(layout: Layout): number {
  switch (layout.type) {
    case "strip":
    case "ring":
      return layout.count;
    case "matrix":
      return layout.width * layout.height;
    case "cube":
      return layout.width * layout.height * layout.depth;
    case "monitor":
      return layout.top + layout.right + layout.bottom + layout.left;
    case "custom":
      return layout.points.length;
  }
}
