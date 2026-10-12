// Mirrors the Rust data model in src-tauri/src/model.rs (camelCase on the wire).

export type Corner = "topLeft" | "topRight" | "bottomLeft" | "bottomRight";
export type Direction = "horizontal" | "vertical";
export type Axis = "x" | "y" | "z";

export interface MatrixWiring {
  start: Corner;
  direction: Direction;
  serpentine: boolean;
}

export type Layout =
  | { type: "strip"; count: number; reversed: boolean }
  | { type: "matrix"; width: number; height: number; wiring: MatrixWiring }
  | {
      type: "cube";
      width: number;
      height: number;
      depth: number;
      wiring: MatrixWiring;
      layerAxis: Axis;
      layersReversed: boolean;
      layersSerpentine: boolean;
    }
  | { type: "ring"; count: number; startAngle: number; clockwise: boolean }
  | {
      type: "monitor";
      top: number;
      right: number;
      bottom: number;
      left: number;
      start: Corner;
      clockwise: boolean;
      offset: number;
      bottomGap: number;
    }
  | { type: "custom"; points: [number, number][] };

export type LayoutType = Layout["type"];

export type SerialProtocol = "adalight" | "tpm2";
export type NetProtocol = "ddp" | "wled" | "artnet" | "e131";

export type Connection =
  | { type: "simulator" }
  | { type: "serial"; port: string; baud: number; protocol: SerialProtocol }
  | { type: "network"; host: string; port: number; protocol: NetProtocol; universe: number };

export type ColorOrder = "RGB" | "GRB" | "BRG" | "RBG" | "GBR" | "BGR";

export interface OutputSettings {
  colorOrder: ColorOrder;
  brightness: number;
  gamma: number;
  whiteBalance: [number, number, number];
  maxMilliamps: number;
  maxFps: number;
}

export type ParamValue = number | boolean | string | string[];
export type Params = Record<string, ParamValue>;

export interface AmbientSettings {
  monitor: string;
  depth: number;
  smoothing: number;
  saturation: number;
  gain: number;
  minBrightness: number;
  blackBars: boolean;
  mapping: "zones" | "average";
}

export type Source =
  | { type: "effect"; effect: string; params: Params }
  | { type: "animation"; animationId: string; speed: number }
  | ({ type: "ambient" } & AmbientSettings)
  | { type: "script"; scriptId: string };

export interface Device {
  id: string;
  name: string;
  power: boolean;
  connection: Connection;
  layout: Layout;
  output: OutputSettings;
  source: Source;
}

export interface AnimFrame {
  durationMs: number;
  fade: boolean;
  /** Base64 of raw RGB bytes, three per LED. */
  pixels: string;
}

export interface Animation {
  id: string;
  name: string;
  count: number;
  looped: boolean;
  frames: AnimFrame[];
}

export interface Script {
  id: string;
  name: string;
  code: string;
}

export interface Preset {
  id: string;
  name: string;
  source: Source;
}

export interface Settings {
  fps: number;
  captureFps: number;
  closeToTray: boolean;
  startMinimized: boolean;
  audioSource: string;
  audioGain: number;
}

export interface Config {
  version: number;
  devices: Device[];
  animations: Animation[];
  scripts: Script[];
  presets: Preset[];
  settings: Settings;
}

// --- things the backend reports ----------------------------------------------

export type ParamDef = { key: string; label: string } & (
  | { kind: "slider"; min: number; max: number; step: number; default: number }
  | { kind: "color"; default: string }
  | { kind: "palette"; default: string }
  | { kind: "toggle"; default: boolean }
  | { kind: "select"; options: string[]; default: string }
  | { kind: "text"; default: string }
);

export interface EffectInfo {
  id: string;
  name: string;
  category: string;
  description: string;
  audio: boolean;
  gridOnly: boolean;
  params: ParamDef[];
}

export interface PaletteInfo {
  name: string;
  colors: string[];
}

export interface ScriptExample {
  name: string;
  code: string;
}

export interface Bootstrap {
  config: Config;
  effects: EffectInfo[];
  palettes: PaletteInfo[];
  scriptExamples: ScriptExample[];
  version: string;
  configPath: string;
}

export interface LayoutPreview {
  kind: LayoutType;
  count: number;
  dims: [number, number, number];
  pos: [number, number, number][];
  grid: [number, number, number][];
  looped: boolean;
  isGrid: boolean;
}

export type LinkState = "simulator" | "connecting" | "connected" | "streaming" | "error";

export interface DeviceStatus {
  link: { state: LinkState; message: string; fps: number };
  leds: number;
  milliamps: number;
  limited: boolean;
  note: string | null;
}

export interface Status {
  devices: Record<string, DeviceStatus>;
  fps: number;
  captureError: string | null;
}

export interface AudioSnapshot {
  level: number;
  bass: number;
  mid: number;
  treble: number;
  beat: number;
  bands: number[];
  status: { running: boolean; device: string; error: string | null };
}

export interface PortInfo {
  name: string;
  label: string;
}

export interface MonitorInfo {
  id: string;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export interface AudioDeviceInfo {
  id: string;
  name: string;
}

export interface FirmwareOpts {
  chipset: string;
  dataPin: number;
  clockPin: number;
  count: number;
  colorOrder: string;
  baud: number;
  maxMilliamps: number;
  idleTimeoutS: number;
}
