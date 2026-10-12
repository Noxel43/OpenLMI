// Thin, typed wrappers around the backend commands (src-tauri/src/commands.rs).

import { invoke } from "@tauri-apps/api/core";
import type {
  AmbientSettings,
  Animation,
  AudioDeviceInfo,
  AudioSnapshot,
  Bootstrap,
  Config,
  Device,
  FirmwareOpts,
  Layout,
  LayoutPreview,
  MonitorInfo,
  PortInfo,
  Preset,
  Script,
  Settings,
  Status,
} from "./types";

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  getConfig: () => invoke<Config>("get_config"),

  saveDevice: (device: Device) => invoke<void>("save_device", { device }),
  deleteDevice: (id: string) => invoke<void>("delete_device", { id }),
  reorderDevices: (ids: string[]) => invoke<void>("reorder_devices", { ids }),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  saveAnimation: (animation: Animation) => invoke<void>("save_animation", { animation }),
  deleteAnimation: (id: string) => invoke<void>("delete_animation", { id }),
  saveScript: (script: Script) => invoke<void>("save_script", { script }),
  deleteScript: (id: string) => invoke<void>("delete_script", { id }),
  savePreset: (preset: Preset) => invoke<void>("save_preset", { preset }),
  deletePreset: (id: string) => invoke<void>("delete_preset", { id }),
  importConfig: (json: string) => invoke<Config>("import_config", { json }),

  layoutPreview: (layout: Layout) => invoke<LayoutPreview>("layout_preview", { layout }),
  ambientZones: (layout: Layout, settings: AmbientSettings, aspect: number) =>
    invoke<[number, number, number, number][]>("ambient_zones", { layout, settings, aspect }),

  /** Latest preview frame as RGB bytes. */
  getFrame: async (id: string) => new Uint8Array(await invoke<ArrayBuffer>("get_frame", { id })),
  getStatus: () => invoke<Status>("get_status"),
  getAudio: () => invoke<AudioSnapshot>("get_audio"),
  /** Downsampled screen: [width, height, left, top, right, bottom, ...rgb]. */
  getScreen: async (monitor: string) => new Uint8Array(await invoke<ArrayBuffer>("get_screen", { monitor })),

  listSerialPorts: () => invoke<PortInfo[]>("list_serial_ports"),
  listMonitors: () => invoke<MonitorInfo[]>("list_monitors"),
  listAudioInputs: () => invoke<AudioDeviceInfo[]>("list_audio_inputs"),

  identify: (id: string, mode: string, index?: number) => invoke<void>("identify", { id, mode, index }),
  paint: (id: string, pixels: string | null) => invoke<void>("paint", { id, pixels }),

  checkScript: (code: string) => invoke<void>("check_script", { code }),
  firmwareSketch: (opts: FirmwareOpts) => invoke<string>("firmware_sketch", { opts }),
  animationSketch: (id: string, opts: FirmwareOpts) => invoke<string>("animation_sketch", { id, opts }),

  writeTextFile: (path: string, contents: string) => invoke<void>("write_text_file", { path, contents }),
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
};
