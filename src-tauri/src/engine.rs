//! The render loop. One thread owns every device's runtime state, draws a
//! frame for each at a steady rate, shapes it for the hardware (brightness,
//! gamma, power budget) and hands it to the output workers and the UI preview.

use crate::ambient::{AmbientState, ScreenCapture};
use crate::audio::{Analyser, AudioCapture, AudioFrame, AudioStatus};
use crate::color::Rgb;
use crate::effects::{self, Ctx, Effect, EffectDef, ParamBag};
use crate::layout::{self, LayoutMap};
use crate::model::{Animation, Config, Device, Script, Settings, Source};
use crate::output::{LinkStatus, Output};
use crate::script::{self, ScriptRunner};
use base64::Engine as _;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Patterns for checking how a device is wired.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Identify {
    /// A dot runs from the first LED to the last.
    Chase,
    /// First LED green, last LED red.
    Ends,
    Red,
    Green,
    Blue,
    White,
    /// Light a single LED.
    Index(usize),
}

impl Identify {
    pub fn parse(mode: &str, index: Option<usize>) -> Option<Identify> {
        Some(match mode {
            "chase" => Identify::Chase,
            "ends" => Identify::Ends,
            "red" => Identify::Red,
            "green" => Identify::Green,
            "blue" => Identify::Blue,
            "white" => Identify::White,
            "index" => Identify::Index(index.unwrap_or(0)),
            _ => return None,
        })
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub link: LinkStatus,
    pub leds: usize,
    /// Estimated current draw of the frame being shown.
    pub milliamps: u32,
    /// The power budget is dimming the output.
    pub limited: bool,
    /// Problems worth showing, e.g. a script error.
    pub note: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioSnapshot {
    pub level: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub beat: f32,
    pub bands: Vec<f32>,
    pub status: AudioStatus,
}

/// State the UI polls.
#[derive(Default)]
pub struct Shared {
    /// Latest preview frame per device, RGB bytes.
    pub frames: Mutex<HashMap<String, Vec<u8>>>,
    pub status: Mutex<HashMap<String, DeviceStatus>>,
    pub audio: Mutex<AudioSnapshot>,
    pub fps: Mutex<f32>,
    /// Bumped while the UI is showing the audio visualiser.
    pub ui_audio: Mutex<Option<Instant>>,
}

enum Cmd {
    /// Devices added/removed/reordered, or the library/settings changed.
    Config(Box<Config>),
    /// One device edited (the hot path while dragging sliders).
    Device(Box<Device>),
    Identify(String, Option<Identify>),
    Paint(String, Option<Vec<u8>>),
    Shutdown(Sender<()>),
}

pub struct EngineHandle {
    tx: Sender<Cmd>,
    pub shared: Arc<Shared>,
    pub audio: AudioCapture,
    pub screen: ScreenCapture,
}

impl EngineHandle {
    pub fn start(config: Config) -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Shared::default());
        let audio = AudioCapture::start();
        let screen = ScreenCapture::start();
        let (s, a, c) = (shared.clone(), audio.clone(), screen.clone());
        std::thread::Builder::new()
            .name("openlmi-engine".into())
            .spawn(move || Engine::new(s, a, c).run(rx, config))
            .expect("failed to start the render thread");
        EngineHandle { tx, shared, audio, screen }
    }

    pub fn set_config(&self, config: &Config) {
        let _ = self.tx.send(Cmd::Config(Box::new(config.clone())));
    }

    pub fn set_device(&self, device: &Device) {
        let _ = self.tx.send(Cmd::Device(Box::new(device.clone())));
    }

    pub fn identify(&self, device_id: &str, mode: Option<Identify>) {
        let _ = self.tx.send(Cmd::Identify(device_id.to_string(), mode));
    }

    /// Shows fixed pixels on a device (the animation editor's live canvas).
    /// `None` returns it to its normal source.
    pub fn paint(&self, device_id: &str, pixels: Option<Vec<u8>>) {
        let _ = self.tx.send(Cmd::Paint(device_id.to_string(), pixels));
    }

    /// Turns every device dark and waits briefly for the frames to go out.
    pub fn shutdown(&self) {
        let (ack, done) = mpsc::channel();
        if self.tx.send(Cmd::Shutdown(ack)).is_ok() {
            let _ = done.recv_timeout(Duration::from_secs(2));
        }
        self.audio.stop();
        self.screen.stop();
    }
}

// ----------------------------------------------------------------------------

struct AnimPlayer {
    /// (seconds, cross-fade, pixels)
    frames: Vec<(f32, bool, Vec<Rgb>)>,
    total: f32,
    pos: f32,
    looped: bool,
}

impl AnimPlayer {
    fn new(anim: &Animation, n: usize) -> Self {
        let frames: Vec<(f32, bool, Vec<Rgb>)> = anim
            .frames
            .iter()
            .map(|f| {
                let bytes = base64::engine::general_purpose::STANDARD.decode(&f.pixels).unwrap_or_default();
                let src: Vec<Rgb> = bytes.chunks_exact(3).map(|c| Rgb::from_u8(c[0], c[1], c[2])).collect();
                (f.duration_ms.max(10) as f32 / 1000.0, f.fade, resample(&src, n))
            })
            .collect();
        let total = frames.iter().map(|f| f.0).sum();
        AnimPlayer { frames, total, pos: 0.0, looped: anim.looped }
    }

    fn render(&mut self, dt: f32, out: &mut [Rgb]) {
        if self.frames.is_empty() || self.total <= 0.0 {
            out.fill(Rgb::BLACK);
            return;
        }
        self.pos += dt;
        if self.looped {
            self.pos = self.pos.rem_euclid(self.total);
        } else {
            self.pos = self.pos.min(self.total);
        }
        let last = self.frames.len() - 1;
        let mut start = 0.0;
        for (i, (dur, fade, px)) in self.frames.iter().enumerate() {
            if self.pos < start + dur || i == last {
                let next = if i == last {
                    if self.looped {
                        0
                    } else {
                        last
                    }
                } else {
                    i + 1
                };
                if *fade && next != i {
                    let k = ((self.pos - start) / dur).clamp(0.0, 1.0);
                    for ((o, a), b) in out.iter_mut().zip(px).zip(&self.frames[next].2) {
                        *o = a.lerp(*b, k);
                    }
                } else {
                    out.copy_from_slice(px);
                }
                return;
            }
            start += dur;
        }
    }
}

/// Stretches a frame painted for one LED count onto another.
fn resample(src: &[Rgb], n: usize) -> Vec<Rgb> {
    if src.len() == n {
        return src.to_vec();
    }
    if src.is_empty() {
        return vec![Rgb::BLACK; n];
    }
    (0..n).map(|i| src[(i * src.len() / n.max(1)).min(src.len() - 1)]).collect()
}

enum Renderer {
    Blank,
    Effect { def: &'static EffectDef, fx: Box<dyn Effect>, bag: ParamBag },
    Animation { source: Animation, player: AnimPlayer },
    Ambient(AmbientState),
    Script { source: Script, runner: Option<ScriptRunner> },
}

struct Library {
    animations: Vec<Animation>,
    scripts: Vec<Script>,
}

struct Env<'a> {
    audio: &'a AudioFrame,
    screen: &'a ScreenCapture,
}

/// Runtime state of one device.
struct Rt {
    cfg: Device,
    map: Arc<LayoutMap>,
    renderer: Renderer,
    note: Option<String>,
    buf: Vec<Rgb>,
    /// Animation time, scaled by the effect's speed.
    clock: f32,
    /// 0 = off, 1 = on; eases so power changes fade instead of snapping.
    fade: f32,
    out: Option<Output>,
    identify: Option<(Identify, Instant)>,
    paint: Option<Vec<Rgb>>,
    last_sent: Instant,
    was_dark: bool,
    gamma_lut: (f32, Vec<f32>),
    shaped: Vec<[f32; 6]>,
    preview: Vec<u8>,
    milliamps: u32,
    limited: bool,
}

const LUT: usize = 1024;

impl Rt {
    fn new(dev: &Device, lib: &Library) -> Self {
        let map = Arc::new(layout::build(&dev.layout));
        let mut rt = Rt {
            cfg: dev.clone(),
            buf: vec![Rgb::BLACK; map.count],
            map,
            renderer: Renderer::Blank,
            note: None,
            clock: 0.0,
            fade: 0.0,
            out: Output::open(&dev.connection),
            identify: None,
            paint: None,
            last_sent: Instant::now(),
            was_dark: false,
            gamma_lut: (0.0, Vec::new()),
            shaped: Vec::new(),
            preview: Vec::new(),
            milliamps: 0,
            limited: false,
        };
        rt.rebuild_renderer(lib);
        rt
    }

    /// Applies an edited device, keeping as much running state as possible.
    fn sync(&mut self, dev: &Device, lib: &Library) {
        let layout_changed = self.cfg.layout != dev.layout;
        if layout_changed {
            self.map = Arc::new(layout::build(&dev.layout));
            self.buf = vec![Rgb::BLACK; self.map.count];
            self.paint = None;
        }
        if self.cfg.connection != dev.connection {
            self.out = Output::open(&dev.connection);
        }
        self.cfg = dev.clone();

        let keep = !layout_changed
            && match (&mut self.renderer, &self.cfg.source) {
                (Renderer::Effect { def, bag, .. }, Source::Effect { effect, params }) if def.id == effect => {
                    // Same effect: just refresh its parameters so stateful
                    // effects don't restart on every slider nudge.
                    *bag = ParamBag::build(&def.params, params);
                    true
                }
                (Renderer::Animation { source, .. }, Source::Animation { animation_id, .. }) => {
                    lib.animations.iter().find(|a| a.id == *animation_id) == Some(&*source)
                }
                (Renderer::Ambient(_), Source::Ambient(_)) => true,
                (Renderer::Script { source, .. }, Source::Script { script_id }) => {
                    lib.scripts.iter().find(|s| s.id == *script_id) == Some(&*source)
                }
                _ => false,
            };
        if !keep {
            self.rebuild_renderer(lib);
        }
    }

    fn rebuild_renderer(&mut self, lib: &Library) {
        self.note = None;
        self.renderer = match &self.cfg.source {
            Source::Effect { effect, params } => match effects::find(effect) {
                Some(def) => Renderer::Effect { def, fx: (def.make)(), bag: ParamBag::build(&def.params, params) },
                None => {
                    self.note = Some(format!("Unknown effect \"{effect}\""));
                    Renderer::Blank
                }
            },
            Source::Animation { animation_id, .. } => match lib.animations.iter().find(|a| a.id == *animation_id) {
                Some(anim) => Renderer::Animation { source: anim.clone(), player: AnimPlayer::new(anim, self.map.count) },
                None => {
                    self.note = Some("This animation no longer exists".into());
                    Renderer::Blank
                }
            },
            Source::Ambient(_) => Renderer::Ambient(AmbientState::default()),
            Source::Script { script_id } => match lib.scripts.iter().find(|s| s.id == *script_id) {
                Some(src) => {
                    let runner = match ScriptRunner::new(&src.code, self.map.clone()) {
                        Ok(r) => Some(r),
                        Err(e) => {
                            self.note = Some(format!("Script error: {e}"));
                            None
                        }
                    };
                    self.clock = 0.0;
                    Renderer::Script { source: src.clone(), runner }
                }
                None => {
                    self.note = Some("This script no longer exists".into());
                    Renderer::Blank
                }
            },
        };
    }

    fn needs_audio(&self) -> bool {
        self.cfg.power
            && match &self.renderer {
                Renderer::Effect { def, .. } => def.audio,
                Renderer::Script { source, .. } => script::uses_audio(&source.code),
                _ => false,
            }
    }

    fn tick(&mut self, dt: f32, env: &Env) {
        let target = if self.cfg.power { 1.0 } else { 0.0 };
        let step = dt / 0.3;
        self.fade = if self.fade < target { (self.fade + step).min(target) } else { (self.fade - step).max(target) };

        if self.identify.is_some_and(|(_, until)| Instant::now() > until) {
            self.identify = None;
        }

        if let Some((mode, until)) = self.identify {
            let elapsed = 60.0 - until.saturating_duration_since(Instant::now()).as_secs_f32();
            draw_identify(mode, elapsed, &mut self.buf);
        } else if let Some(paint) = self.paint.as_ref().filter(|p| p.len() == self.buf.len()) {
            self.buf.copy_from_slice(paint);
        } else if self.fade > 0.0 {
            self.render(dt, env);
        } else {
            self.buf.fill(Rgb::BLACK);
        }
        self.finish();
    }

    fn render(&mut self, dt: f32, env: &Env) {
        match &mut self.renderer {
            Renderer::Blank => self.buf.fill(Rgb::BLACK),
            Renderer::Effect { fx, bag, .. } => {
                let scaled = dt * bag.num("speed");
                // Wrap long before f32 loses enough precision to look choppy.
                self.clock = (self.clock + scaled) % 14_400.0;
                let cx = Ctx { t: self.clock, dt: scaled, real_dt: dt, map: &self.map, p: bag, audio: env.audio };
                fx.render(&cx, &mut self.buf);
            }
            Renderer::Animation { player, .. } => {
                let speed = match &self.cfg.source {
                    Source::Animation { speed, .. } => speed.clamp(0.05, 10.0),
                    _ => 1.0,
                };
                player.render(dt * speed, &mut self.buf);
            }
            Renderer::Ambient(state) => {
                if let Source::Ambient(settings) = &self.cfg.source {
                    env.screen.request(&settings.monitor);
                    let grid = env.screen.grid(&settings.monitor);
                    state.render(grid.as_deref(), &self.map, settings, dt, &mut self.buf);
                }
            }
            Renderer::Script { runner, .. } => match runner {
                Some(r) => {
                    self.clock = (self.clock + dt) % 14_400.0;
                    r.render(self.clock, dt, env.audio, &mut self.buf);
                    if self.note.is_none() {
                        if let Some(e) = &r.error {
                            self.note = Some(format!("Script error: {e}"));
                        }
                    }
                }
                None => self.buf.fill(Rgb::BLACK),
            },
        }
    }

    /// Brightness, white balance, gamma and the power budget; then publish.
    fn finish(&mut self) {
        let o = &self.cfg.output;
        let level = if self.identify.is_some() { 1.0 } else { self.fade };
        let bright = o.brightness.clamp(0.0, 1.0) * level;
        let wb = o.white_balance.map(|c| if c.is_finite() { c.clamp(0.0, 1.0) } else { 1.0 });
        let gamma = if o.gamma.is_finite() { o.gamma.clamp(1.0, 4.0) } else { 1.0 };
        if self.gamma_lut.0 != gamma || self.gamma_lut.1.len() != LUT + 1 {
            self.gamma_lut = (gamma, (0..=LUT).map(|i| (i as f32 / LUT as f32).powf(gamma)).collect());
        }
        let lut = &self.gamma_lut.1;

        // Pass 1: shape every LED and add up what the frame would draw.
        self.shaped.clear();
        let mut sum = 0.0f32;
        for c in &self.buf {
            let c = c.clamped();
            let seen = [c.r * wb[0] * bright, c.g * wb[1] * bright, c.b * wb[2] * bright];
            let wire = seen.map(|v| lut[(v * LUT as f32) as usize]);
            sum += wire[0] + wire[1] + wire[2];
            self.shaped.push([seen[0], seen[1], seen[2], wire[0], wire[1], wire[2]]);
        }

        // WS2812-class LEDs: about 20 mA per channel at full, 1 mA idle each.
        let n = self.buf.len() as f32;
        let lit_ma = sum * 20.0;
        let budget = o.max_milliamps as f32;
        let k = if budget > 0.0 && lit_ma + n > budget { ((budget - n).max(0.0) / lit_ma.max(1e-6)).min(1.0) } else { 1.0 };
        self.limited = k < 1.0;
        self.milliamps = (lit_ma * k + n).round() as u32;
        let seen_k = k.powf(1.0 / gamma);

        // Pass 2: bytes for the wire and for the on-screen preview.
        let mut wire = Vec::with_capacity(self.shaped.len() * 3);
        self.preview.clear();
        for s in &self.shaped {
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            self.preview.extend_from_slice(&[byte(s[0] * seen_k), byte(s[1] * seen_k), byte(s[2] * seen_k)]);
            wire.extend_from_slice(&o.color_order.apply([byte(s[3] * k), byte(s[4] * k), byte(s[5] * k)]));
        }

        if let Some(out) = &self.out {
            let dark = level <= 0.0 && self.paint.is_none();
            let gap = self.last_sent.elapsed().as_secs_f32();
            let due = if dark {
                // Once dark, just repeat the blackout now and then so a device
                // that reconnects (or was reset) goes dark too.
                !self.was_dark || gap > 1.0
            } else {
                gap >= 0.95 / o.max_fps.clamp(1, 240) as f32
            };
            if due {
                out.send(wire);
                self.last_sent = Instant::now();
                self.was_dark = dark;
            }
        }
    }

    fn status(&self) -> DeviceStatus {
        DeviceStatus {
            link: self.out.as_ref().map_or_else(LinkStatus::simulator, Output::status),
            leds: self.map.count,
            milliamps: self.milliamps,
            limited: self.limited,
            note: self.note.clone(),
        }
    }

    /// Queue a final all-off frame.
    fn blackout(&self) {
        if let Some(out) = &self.out {
            out.send(vec![0; self.map.count * 3]);
        }
    }
}

fn draw_identify(mode: Identify, elapsed: f32, buf: &mut [Rgb]) {
    let n = buf.len();
    let green = Rgb::new(0.0, 1.0, 0.0);
    let red = Rgb::new(1.0, 0.0, 0.0);
    match mode {
        Identify::Red => buf.fill(red),
        Identify::Green => buf.fill(green),
        Identify::Blue => buf.fill(Rgb::new(0.0, 0.0, 1.0)),
        Identify::White => buf.fill(Rgb::WHITE),
        Identify::Ends => {
            buf.fill(Rgb::WHITE.scale(0.04));
            buf[0] = green;
            buf[n - 1] = red;
        }
        Identify::Index(i) => {
            buf.fill(Rgb::WHITE.scale(0.03));
            if let Some(px) = buf.get_mut(i) {
                *px = Rgb::WHITE;
            }
        }
        Identify::Chase => {
            // One pass takes 2-8 s depending on length, so it is easy to follow.
            let pass = (n as f32 / 30.0).clamp(2.0, 8.0);
            let head = (elapsed / pass).fract() * n as f32;
            let tail = (n as f32 / 20.0).max(3.0);
            for (i, px) in buf.iter_mut().enumerate() {
                let behind = head - i as f32;
                *px = if (0.0..tail).contains(&behind) { Rgb::WHITE.scale(1.0 - behind / tail) } else { Rgb::BLACK };
            }
            buf[0] = green;
            buf[n - 1] = red;
        }
    }
}

// ----------------------------------------------------------------------------

struct Engine {
    shared: Arc<Shared>,
    audio: AudioCapture,
    screen: ScreenCapture,
    analyser: Analyser,
    rts: Vec<Rt>,
    lib: Library,
    settings: Settings,
}

impl Engine {
    fn new(shared: Arc<Shared>, audio: AudioCapture, screen: ScreenCapture) -> Self {
        Engine {
            shared,
            audio,
            screen,
            analyser: Analyser::new(),
            rts: Vec::new(),
            lib: Library { animations: Vec::new(), scripts: Vec::new() },
            settings: Settings::default(),
        }
    }

    fn apply_config(&mut self, config: Config) {
        self.lib = Library { animations: config.animations, scripts: config.scripts };
        self.settings = config.settings;
        self.audio.set_source(&self.settings.audio_source);
        self.screen.set_fps(self.settings.capture_fps);

        let mut old: Vec<Rt> = std::mem::take(&mut self.rts);
        for dev in &config.devices {
            let rt = match old.iter().position(|r| r.cfg.id == dev.id) {
                Some(i) => {
                    let mut rt = old.swap_remove(i);
                    rt.sync(dev, &self.lib);
                    rt
                }
                None => Rt::new(dev, &self.lib),
            };
            self.rts.push(rt);
        }
        // Devices that were deleted go dark on the way out.
        for gone in &old {
            gone.blackout();
        }
        let ids: Vec<&str> = self.rts.iter().map(|r| r.cfg.id.as_str()).collect();
        self.shared.frames.lock().retain(|id, _| ids.contains(&id.as_str()));
        self.shared.status.lock().retain(|id, _| ids.contains(&id.as_str()));
    }

    fn handle(&mut self, cmd: Cmd) -> Option<Sender<()>> {
        match cmd {
            Cmd::Config(config) => self.apply_config(*config),
            Cmd::Device(dev) => {
                if let Some(rt) = self.rts.iter_mut().find(|r| r.cfg.id == dev.id) {
                    rt.sync(&dev, &self.lib);
                }
            }
            Cmd::Identify(id, mode) => {
                if let Some(rt) = self.rts.iter_mut().find(|r| r.cfg.id == id) {
                    rt.identify = mode.map(|m| (m, Instant::now() + Duration::from_secs(60)));
                }
            }
            Cmd::Paint(id, pixels) => {
                if let Some(rt) = self.rts.iter_mut().find(|r| r.cfg.id == id) {
                    rt.paint = pixels.map(|bytes| {
                        let px: Vec<Rgb> = bytes.chunks_exact(3).map(|c| Rgb::from_u8(c[0], c[1], c[2])).collect();
                        resample(&px, rt.map.count)
                    });
                }
            }
            Cmd::Shutdown(ack) => return Some(ack),
        }
        None
    }

    fn run(mut self, rx: Receiver<Cmd>, config: Config) {
        self.apply_config(config);
        let mut last = Instant::now();
        let mut fps_since = Instant::now();
        let mut fps_frames = 0u32;
        let mut status_at = Instant::now() - Duration::from_secs(1);

        loop {
            let started = Instant::now();
            loop {
                match rx.try_recv() {
                    Ok(cmd) => {
                        if let Some(ack) = self.handle(cmd) {
                            self.shutdown();
                            let _ = ack.send(());
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.shutdown();
                        return;
                    }
                }
            }

            let dt = started.duration_since(last).as_secs_f32().clamp(0.0005, 0.25);
            last = started;

            let ui_listening = self.shared.ui_audio.lock().is_some_and(|t| t.elapsed() < Duration::from_secs(1));
            if ui_listening || self.rts.iter().any(Rt::needs_audio) {
                self.audio.request();
                let f = self.analyser.update(&self.audio, dt, self.settings.audio_gain);
                let mut snap = self.shared.audio.lock();
                snap.level = f.level;
                snap.bass = f.bass;
                snap.mid = f.mid;
                snap.treble = f.treble;
                snap.beat = f.beat;
                snap.bands.clear();
                snap.bands.extend_from_slice(&f.bands);
                snap.status = self.audio.status();
            }

            let env = Env { audio: self.analyser.frame(), screen: &self.screen };
            for rt in &mut self.rts {
                // A bug in one source must not take the whole loop down.
                if catch_unwind(AssertUnwindSafe(|| rt.tick(dt, &env))).is_err() {
                    rt.renderer = Renderer::Blank;
                    rt.note = Some("This source crashed and was stopped".into());
                }
            }

            {
                let mut frames = self.shared.frames.lock();
                for rt in &self.rts {
                    match frames.get_mut(&rt.cfg.id) {
                        Some(f) => f.clone_from(&rt.preview),
                        None => {
                            frames.insert(rt.cfg.id.clone(), rt.preview.clone());
                        }
                    }
                }
            }
            if status_at.elapsed() > Duration::from_millis(250) {
                status_at = Instant::now();
                let mut status = self.shared.status.lock();
                for rt in &self.rts {
                    status.insert(rt.cfg.id.clone(), rt.status());
                }
            }

            fps_frames += 1;
            if fps_since.elapsed() >= Duration::from_secs(1) {
                *self.shared.fps.lock() = fps_frames as f32 / fps_since.elapsed().as_secs_f32();
                fps_frames = 0;
                fps_since = Instant::now();
            }

            let period = Duration::from_secs_f32(1.0 / self.settings.fps.clamp(10, 240) as f32);
            if let Some(rest) = period.checked_sub(started.elapsed()) {
                std::thread::sleep(rest);
            }
        }
    }

    fn shutdown(&mut self) {
        for rt in &self.rts {
            rt.blackout();
        }
        // Dropping the outputs lets each worker flush that last frame and exit.
        self.rts.clear();
        std::thread::sleep(Duration::from_millis(300));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AnimFrame, ColorOrder, Connection, Layout, OutputSettings, Params};

    fn device(source: Source) -> Device {
        Device {
            id: "d".into(),
            name: "Test".into(),
            power: true,
            connection: Connection::Simulator,
            layout: Layout::Strip { count: 4, reversed: false },
            output: OutputSettings { brightness: 1.0, gamma: 1.0, ..Default::default() },
            source,
        }
    }

    fn solid(hex: &str) -> Source {
        let mut params = Params::new();
        params.insert("color".into(), crate::model::ParamValue::Text(hex.into()));
        Source::Effect { effect: "solid".into(), params }
    }

    fn lib() -> Library {
        Library { animations: vec![], scripts: vec![] }
    }

    fn run(rt: &mut Rt, frames: usize) {
        let audio = AudioFrame::default();
        let screen = ScreenCapture::start();
        let env = Env { audio: &audio, screen: &screen };
        for _ in 0..frames {
            rt.tick(1.0 / 60.0, &env);
        }
        screen.stop();
    }

    fn pixels(b64: &[[u8; 3]]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b64.concat())
    }

    #[test]
    fn power_fades_in_and_out() {
        let mut rt = Rt::new(&device(solid("#ffffff")), &lib());
        run(&mut rt, 1);
        assert!(rt.preview[0] > 0 && rt.preview[0] < 60, "first frame is still fading in");
        run(&mut rt, 60);
        assert_eq!(rt.preview, vec![255; 12]);

        let mut off = rt.cfg.clone();
        off.power = false;
        rt.sync(&off, &lib());
        run(&mut rt, 60);
        assert_eq!(rt.preview, vec![0; 12]);
    }

    #[test]
    fn brightness_white_balance_and_colour_order() {
        let mut dev = device(solid("#ffffff"));
        dev.output.brightness = 0.5;
        dev.output.white_balance = [1.0, 0.5, 0.0];
        dev.output.color_order = ColorOrder::Grb;
        let mut rt = Rt::new(&dev, &lib());
        run(&mut rt, 60);
        assert_eq!(&rt.preview[..3], &[128, 64, 0]);
    }

    #[test]
    fn power_budget_dims_the_frame() {
        let mut dev = device(solid("#ffffff"));
        dev.output.max_milliamps = 124; // 4 LEDs: 4 mA idle + 120 mA of light = half of full white
        let mut rt = Rt::new(&dev, &lib());
        run(&mut rt, 60);
        assert!(rt.limited);
        assert_eq!(rt.milliamps, 124);
        assert_eq!(rt.preview[0], 128);

        let mut free = dev.clone();
        free.output.max_milliamps = 0;
        rt.sync(&free, &lib());
        run(&mut rt, 2);
        assert!(!rt.limited);
        assert_eq!(rt.milliamps, 244);
    }

    #[test]
    fn slider_changes_keep_effect_state_but_new_effects_rebuild() {
        let mut rt = Rt::new(&device(solid("#ff0000")), &lib());
        run(&mut rt, 60);
        assert_eq!(&rt.preview[..3], &[255, 0, 0]);
        let clock = rt.clock;

        rt.sync(&device(solid("#0000ff")), &lib());
        run(&mut rt, 1);
        assert_eq!(&rt.preview[..3], &[0, 0, 255]);
        assert!(rt.clock > clock, "the clock keeps running across parameter edits");

        rt.sync(&device(Source::Effect { effect: "nope".into(), params: Params::new() }), &lib());
        assert!(rt.note.as_deref().unwrap().contains("Unknown effect"));
    }

    #[test]
    fn animations_play_cut_and_fade() {
        let red = [255, 0, 0];
        let blue = [0, 0, 255];
        let anim = Animation {
            id: "a".into(),
            name: "Blink".into(),
            count: 4,
            looped: true,
            frames: vec![
                AnimFrame { duration_ms: 500, fade: false, pixels: pixels(&[red; 4]) },
                AnimFrame { duration_ms: 500, fade: true, pixels: pixels(&[blue; 4]) },
            ],
        };
        let library = Library { animations: vec![anim], scripts: vec![] };
        let mut rt = Rt::new(&device(Source::Animation { animation_id: "a".into(), speed: 1.0 }), &library);
        run(&mut rt, 24); // 0.4 s: first frame, hard cut
        assert_eq!(&rt.preview[..3], &[255, 0, 0]);
        run(&mut rt, 12); // 0.6 s: second frame just starting its fade back to red
        assert!(rt.preview[2] > 180 && rt.preview[0] < 80, "{:?}", &rt.preview[..3]);
        run(&mut rt, 21); // ~0.95 s: nearly faded back to the first frame
        assert!(rt.preview[0] > 200 && rt.preview[2] < 60, "{:?}", &rt.preview[..3]);
    }

    #[test]
    fn animations_stretch_to_other_led_counts() {
        let src = [Rgb::new(1.0, 0.0, 0.0), Rgb::new(0.0, 0.0, 1.0)];
        let out = resample(&src, 4);
        assert_eq!(out.iter().map(|c| c.to_u8()).collect::<Vec<_>>(), vec![[255, 0, 0], [255, 0, 0], [0, 0, 255], [0, 0, 255]]);
        assert_eq!(resample(&[], 3).len(), 3);
    }

    #[test]
    fn identify_marks_both_ends_and_painting_overrides_the_source() {
        let mut rt = Rt::new(&device(solid("#ffffff")), &lib());
        rt.identify = Some((Identify::Ends, Instant::now() + Duration::from_secs(60)));
        run(&mut rt, 1);
        assert_eq!(&rt.preview[..3], &[0, 255, 0]);
        assert_eq!(&rt.preview[9..], &[255, 0, 0]);

        rt.identify = None;
        rt.paint = Some(vec![Rgb::new(0.0, 0.0, 1.0); 4]);
        run(&mut rt, 60);
        assert_eq!(&rt.preview[..3], &[0, 0, 255]);
    }

    #[test]
    fn script_errors_surface_as_notes() {
        let library = Library {
            animations: vec![],
            scripts: vec![Script { id: "s".into(), name: "Bad".into(), code: "fn pixel(i, t) { boom() }".into() }],
        };
        let mut rt = Rt::new(&device(Source::Script { script_id: "s".into() }), &library);
        run(&mut rt, 3);
        assert!(rt.note.as_deref().unwrap().contains("boom"));
    }
}
