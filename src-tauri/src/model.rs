//! The persisted data model. Everything here is plain serde data shared with
//! the UI as JSON (camelCase on the wire).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_LEDS: u32 = 16_384;

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub version: u32,
    pub devices: Vec<Device>,
    pub animations: Vec<Animation>,
    pub scripts: Vec<Script>,
    pub presets: Vec<Preset>,
    pub settings: Settings,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Render loop rate.
    pub fps: u32,
    /// Screen capture rate used by ambient mode.
    pub capture_fps: u32,
    /// Keep running in the tray when the window is closed.
    pub close_to_tray: bool,
    /// Start hidden in the tray (used together with launch-at-login).
    pub start_minimized: bool,
    /// `"loopback"` for what the PC is playing, otherwise an input device id.
    pub audio_source: String,
    pub audio_gain: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            fps: 60,
            capture_fps: 30,
            close_to_tray: false,
            start_minimized: false,
            audio_source: "loopback".into(),
            audio_gain: 1.0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub power: bool,
    pub connection: Connection,
    pub layout: Layout,
    #[serde(default)]
    pub output: OutputSettings,
    #[serde(default)]
    pub source: Source,
}

// ----------------------------------------------------------------------------
// Connections
// ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Connection {
    /// No hardware: frames only go to the on-screen preview.
    Simulator,
    #[serde(rename_all = "camelCase")]
    Serial {
        port: String,
        baud: u32,
        #[serde(default)]
        protocol: SerialProtocol,
    },
    #[serde(rename_all = "camelCase")]
    Network {
        host: String,
        port: u16,
        #[serde(default)]
        protocol: NetProtocol,
        /// First DMX universe (Art-Net and E1.31 only).
        #[serde(default)]
        universe: u16,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SerialProtocol {
    #[default]
    Adalight,
    Tpm2,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum NetProtocol {
    #[default]
    Ddp,
    Wled,
    Artnet,
    E131,
}

impl NetProtocol {
    pub fn default_port(self) -> u16 {
        match self {
            NetProtocol::Ddp => 4048,
            NetProtocol::Wled => 21324,
            NetProtocol::Artnet => 6454,
            NetProtocol::E131 => 5568,
        }
    }
}

// ----------------------------------------------------------------------------
// Layouts
// ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn is_right(self) -> bool {
        matches!(self, Corner::TopRight | Corner::BottomRight)
    }
    pub fn is_bottom(self) -> bool {
        matches!(self, Corner::BottomLeft | Corner::BottomRight)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// The strip runs along rows first.
    #[default]
    Horizontal,
    /// The strip runs along columns first.
    Vertical,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    X,
    Y,
    #[default]
    Z,
}

/// How a strip snakes through a 2D grid.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MatrixWiring {
    /// Where LED 0 sits.
    pub start: Corner,
    pub direction: Direction,
    /// Zig-zag: every other line runs backwards.
    pub serpentine: bool,
}

impl Default for MatrixWiring {
    fn default() -> Self {
        MatrixWiring { start: Corner::TopLeft, direction: Direction::Horizontal, serpentine: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Layout {
    #[serde(rename_all = "camelCase")]
    Strip {
        count: u32,
        #[serde(default)]
        reversed: bool,
    },
    #[serde(rename_all = "camelCase")]
    Matrix {
        width: u32,
        height: u32,
        #[serde(default)]
        wiring: MatrixWiring,
    },
    #[serde(rename_all = "camelCase")]
    Cube {
        width: u32,
        height: u32,
        depth: u32,
        #[serde(default)]
        wiring: MatrixWiring,
        /// The axis the 2D layers are stacked along.
        #[serde(default)]
        layer_axis: Axis,
        #[serde(default)]
        layers_reversed: bool,
        /// Every other layer is wired back-to-front.
        #[serde(default)]
        layers_serpentine: bool,
    },
    #[serde(rename_all = "camelCase")]
    Ring {
        count: u32,
        /// Degrees clockwise from the top where LED 0 sits.
        #[serde(default)]
        start_angle: f32,
        #[serde(default = "yes")]
        clockwise: bool,
    },
    /// A strip running around the back of a monitor. All directions are as
    /// seen from the front, i.e. sitting at the desk looking at the screen.
    #[serde(rename_all = "camelCase")]
    Monitor {
        top: u32,
        right: u32,
        bottom: u32,
        left: u32,
        #[serde(default)]
        start: Corner,
        #[serde(default = "yes")]
        clockwise: bool,
        /// How many LEDs past the start corner the strip actually begins.
        #[serde(default)]
        offset: i32,
        /// Fraction of the bottom edge left empty (for a stand), centred.
        #[serde(default)]
        bottom_gap: f32,
    },
    /// Free-form 2D placement. Points are normalised `0..1`.
    Custom { points: Vec<[f32; 2]> },
}

impl Default for Layout {
    fn default() -> Self {
        Layout::Strip { count: 60, reversed: false }
    }
}

// ----------------------------------------------------------------------------
// Output shaping
// ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum ColorOrder {
    #[default]
    Rgb,
    Grb,
    Brg,
    Rbg,
    Gbr,
    Bgr,
}

impl ColorOrder {
    pub fn apply(self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        match self {
            ColorOrder::Rgb => [r, g, b],
            ColorOrder::Grb => [g, r, b],
            ColorOrder::Brg => [b, r, g],
            ColorOrder::Rbg => [r, b, g],
            ColorOrder::Gbr => [g, b, r],
            ColorOrder::Bgr => [b, g, r],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputSettings {
    pub color_order: ColorOrder,
    /// Master brightness `0..1`.
    pub brightness: f32,
    /// Gamma applied before sending. `1.0` disables it.
    pub gamma: f32,
    /// Per-channel multipliers to tame a blue/green tint.
    pub white_balance: [f32; 3],
    /// Power budget; frames are dimmed to stay under it. `0` = unlimited.
    pub max_milliamps: u32,
    /// Upper bound on frames sent to the hardware per second.
    pub max_fps: u32,
}

impl Default for OutputSettings {
    fn default() -> Self {
        OutputSettings {
            color_order: ColorOrder::Rgb,
            brightness: 0.8,
            gamma: 2.2,
            white_balance: [1.0, 1.0, 1.0],
            max_milliamps: 0,
            max_fps: 60,
        }
    }
}

// ----------------------------------------------------------------------------
// Sources: what a device is currently showing
// ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum ParamValue {
    Bool(bool),
    Num(f64),
    Text(String),
    List(Vec<String>),
}

pub type Params = BTreeMap<String, ParamValue>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Source {
    #[serde(rename_all = "camelCase")]
    Effect {
        effect: String,
        #[serde(default)]
        params: Params,
    },
    #[serde(rename_all = "camelCase")]
    Animation {
        animation_id: String,
        #[serde(default = "one")]
        speed: f32,
    },
    Ambient(AmbientSettings),
    #[serde(rename_all = "camelCase")]
    Script { script_id: String },
}

impl Default for Source {
    fn default() -> Self {
        Source::Effect { effect: "rainbow".into(), params: Params::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AmbientMapping {
    /// Each LED samples the part of the screen it sits behind.
    #[default]
    Zones,
    /// Every LED shows the average colour of the whole screen.
    Average,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AmbientSettings {
    /// Display name as reported by the OS; empty means the primary display.
    pub monitor: String,
    /// How far into the picture each edge LED looks, as a fraction of the
    /// screen height.
    pub depth: f32,
    /// `0` = instant, towards `1` = slow and dreamy.
    pub smoothing: f32,
    pub saturation: f32,
    pub gain: f32,
    /// Floor so the backlight never goes completely dark.
    pub min_brightness: f32,
    /// Ignore letterbox / pillarbox bars.
    pub black_bars: bool,
    pub mapping: AmbientMapping,
}

impl Default for AmbientSettings {
    fn default() -> Self {
        AmbientSettings {
            monitor: String::new(),
            depth: 0.14,
            smoothing: 0.55,
            saturation: 1.35,
            gain: 1.0,
            min_brightness: 0.0,
            black_bars: true,
            mapping: AmbientMapping::Zones,
        }
    }
}

// ----------------------------------------------------------------------------
// User content
// ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Animation {
    pub id: String,
    pub name: String,
    /// LED count the frames were painted for.
    pub count: u32,
    #[serde(default = "yes")]
    pub looped: bool,
    pub frames: Vec<AnimFrame>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AnimFrame {
    pub duration_ms: u32,
    /// Cross-fade into the next frame instead of cutting.
    #[serde(default)]
    pub fade: bool,
    /// Base64 of raw RGB bytes, three per LED.
    pub pixels: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub name: String,
    pub code: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub source: Source,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_json_round_trip() {
        let json = r##"{
            "id": "a", "name": "Desk",
            "connection": { "type": "serial", "port": "COM3", "baud": 115200 },
            "layout": { "type": "monitor", "top": 30, "right": 17, "bottom": 30, "left": 17,
                        "start": "bottomLeft", "clockwise": true, "bottomGap": 0.2 },
            "source": { "type": "ambient", "depth": 0.2 }
        }"##;
        let dev: Device = serde_json::from_str(json).unwrap();
        assert!(dev.power);
        assert_eq!(dev.output.color_order, ColorOrder::Rgb);
        match &dev.source {
            Source::Ambient(a) => {
                assert_eq!(a.depth, 0.2);
                assert!(a.black_bars);
            }
            other => panic!("unexpected source {other:?}"),
        }
        let back: Device = serde_json::from_str(&serde_json::to_string(&dev).unwrap()).unwrap();
        assert_eq!(back, dev);
    }

    #[test]
    fn params_accept_mixed_values() {
        let json = r##"{ "type": "effect", "effect": "comet",
            "params": { "speed": 1.5, "reverse": true, "color": "#ff0000", "palette": ["#000", "#fff"] } }"##;
        let src: Source = serde_json::from_str(json).unwrap();
        let Source::Effect { params, .. } = src else { panic!() };
        assert_eq!(params["speed"], ParamValue::Num(1.5));
        assert_eq!(params["reverse"], ParamValue::Bool(true));
        assert_eq!(params["color"], ParamValue::Text("#ff0000".into()));
        assert_eq!(params["palette"], ParamValue::List(vec!["#000".into(), "#fff".into()]));
    }
}
