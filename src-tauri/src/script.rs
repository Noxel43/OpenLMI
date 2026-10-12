//! The "write your own effect" option: sandboxed Rhai scripts.
//!
//! A script defines `fn pixel(i, x, y, z, t)` (called for every LED, returns a
//! colour) and/or `fn frame(t, dt)` (called once per frame, draws with `set`,
//! `fill`, `fade`...). An optional `fn init()` returns a state object that is
//! available as `this` inside both.

use crate::audio::AudioFrame;
use crate::color::{self, palette_by_name, Rgb};
use crate::layout::LayoutMap;
use rhai::{CallFnOptions, Dynamic, Engine, EvalAltResult, Scope, AST, FLOAT, INT};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

type Fallible<T> = Result<T, Box<EvalAltResult>>;

struct Shared {
    buf: Vec<Rgb>,
    map: Arc<LayoutMap>,
    audio: AudioFrame,
}

pub struct ScriptRunner {
    engine: Engine,
    ast: AST,
    scope: Scope<'static>,
    state: Dynamic,
    has_frame: bool,
    /// Number of parameters `pixel` takes (2 or 5), if it is defined.
    pixel_arity: Option<usize>,
    shared: Rc<RefCell<Shared>>,
    /// Set when the script failed; rendering stops until it is edited.
    pub error: Option<String>,
}

fn num(d: &Dynamic) -> Fallible<f64> {
    d.as_float()
        .or_else(|_| d.as_int().map(|i| i as f64))
        .map_err(|t| format!("expected a number, got {t}").into())
}

fn col(d: &Dynamic) -> Fallible<Rgb> {
    d.as_int().map(Rgb::from_packed).map_err(|t| format!("expected a colour, got {t}").into())
}

fn build_engine(shared: &Rc<RefCell<Shared>>) -> Engine {
    let mut e = Engine::new();
    // Keep runaway scripts from freezing the lights.
    e.set_max_operations(3_000_000);
    e.set_max_call_levels(48);
    e.set_max_expr_depths(96, 48);
    e.set_max_string_size(4_096);
    e.set_max_array_size(65_536);
    e.set_max_map_size(4_096);
    e.on_print(|_| {});
    e.on_debug(|_, _, _| {});

    // --- colours --------------------------------------------------------
    e.register_fn("rgb", |r: Dynamic, g: Dynamic, b: Dynamic| -> Fallible<INT> {
        Ok(Rgb::new(num(&r)? as f32 / 255.0, num(&g)? as f32 / 255.0, num(&b)? as f32 / 255.0).to_packed())
    });
    e.register_fn("hsv", |h: Dynamic, s: Dynamic, v: Dynamic| -> Fallible<INT> {
        Ok(Rgb::hsv(num(&h)? as f32, num(&s)? as f32, num(&v)? as f32).to_packed())
    });
    e.register_fn("hex", |s: &str| -> Fallible<INT> {
        Rgb::from_hex(s).map(Rgb::to_packed).ok_or_else(|| format!("'{s}' is not a hex colour").into())
    });
    e.register_fn("mix", |a: Dynamic, b: Dynamic, t: Dynamic| -> Fallible<INT> {
        Ok(col(&a)?.lerp(col(&b)?, num(&t)? as f32).to_packed())
    });
    e.register_fn("dim", |c: Dynamic, f: Dynamic| -> Fallible<INT> {
        Ok(col(&c)?.scale(num(&f)?.max(0.0) as f32).to_packed())
    });
    e.register_fn("add", |a: Dynamic, b: Dynamic| -> Fallible<INT> { Ok((col(&a)? + col(&b)?).to_packed()) });
    e.register_fn("palette", |name: &str, t: Dynamic| -> Fallible<INT> {
        let pal = palette_by_name(name).ok_or_else(|| format!("unknown palette '{name}'"))?;
        Ok(pal.sample(num(&t)? as f32).to_packed())
    });

    // --- maths ------------------------------------------------------------
    e.register_fn("noise", |x: Dynamic| -> Fallible<FLOAT> { Ok(color::noise01(num(&x)? as f32, 0.0, 0.0) as FLOAT) });
    e.register_fn("noise", |x: Dynamic, y: Dynamic| -> Fallible<FLOAT> {
        Ok(color::noise01(num(&x)? as f32, num(&y)? as f32, 0.0) as FLOAT)
    });
    e.register_fn("noise", |x: Dynamic, y: Dynamic, z: Dynamic| -> Fallible<FLOAT> {
        Ok(color::noise01(num(&x)? as f32, num(&y)? as f32, num(&z)? as f32) as FLOAT)
    });
    e.register_fn("rand", fastrand::f64);
    e.register_fn("rand_int", |lo: INT, hi: INT| if hi > lo { fastrand::i64(lo..=hi) } else { lo });
    e.register_fn("wave", |x: Dynamic| -> Fallible<FLOAT> { Ok(0.5 + 0.5 * (num(&x)? * std::f64::consts::TAU).sin()) });
    e.register_fn("tri", |x: Dynamic| -> Fallible<FLOAT> {
        let f = num(&x)?.rem_euclid(1.0);
        Ok(1.0 - (f * 2.0 - 1.0).abs())
    });
    e.register_fn("fract", |x: Dynamic| -> Fallible<FLOAT> { Ok(num(&x)?.rem_euclid(1.0)) });
    e.register_fn("clamp01", |x: Dynamic| -> Fallible<FLOAT> { Ok(num(&x)?.clamp(0.0, 1.0)) });
    e.register_fn("lerp", |a: Dynamic, b: Dynamic, t: Dynamic| -> Fallible<FLOAT> {
        let (a, b) = (num(&a)?, num(&b)?);
        Ok(a + (b - a) * num(&t)?)
    });
    e.register_fn("smoothstep", |a: Dynamic, b: Dynamic, x: Dynamic| -> Fallible<FLOAT> {
        Ok(color::smoothstep(num(&a)? as f32, num(&b)? as f32, num(&x)? as f32) as FLOAT)
    });

    // --- layout -----------------------------------------------------------
    let s = shared.clone();
    e.register_fn("led_count", move || s.borrow().map.count as INT);
    let s = shared.clone();
    e.register_fn("width", move || s.borrow().map.dims[0] as INT);
    let s = shared.clone();
    e.register_fn("height", move || s.borrow().map.dims[1] as INT);
    let s = shared.clone();
    e.register_fn("depth", move || s.borrow().map.dims[2] as INT);
    for (name, axis) in [("pos_x", 0usize), ("pos_y", 1), ("pos_z", 2)] {
        let s = shared.clone();
        e.register_fn(name, move |i: INT| -> FLOAT {
            s.borrow().map.pos.get(i.max(0) as usize).map_or(0.0, |p| p[axis] as FLOAT)
        });
    }
    let s = shared.clone();
    e.register_fn("index_of", move |x: INT, y: INT| -> INT { s.borrow().map.index_at(x, y, 0).map_or(-1, |i| i as INT) });
    let s = shared.clone();
    e.register_fn("index_of", move |x: INT, y: INT, z: INT| -> INT {
        s.borrow().map.index_at(x, y, z).map_or(-1, |i| i as INT)
    });

    // --- drawing (used from `frame`) -----------------------------------------
    let s = shared.clone();
    e.register_fn("set", move |i: INT, c: INT| {
        if let Ok(i) = usize::try_from(i) {
            if let Some(px) = s.borrow_mut().buf.get_mut(i) {
                *px = Rgb::from_packed(c);
            }
        }
    });
    let s = shared.clone();
    e.register_fn("get", move |i: INT| -> INT {
        usize::try_from(i).ok().and_then(|i| s.borrow().buf.get(i).copied()).map_or(0, Rgb::to_packed)
    });
    let s = shared.clone();
    e.register_fn("fill", move |c: INT| s.borrow_mut().buf.fill(Rgb::from_packed(c)));
    let s = shared.clone();
    e.register_fn("fade", move |f: Dynamic| -> Fallible<()> {
        let f = num(&f)?.clamp(0.0, 1.0) as f32;
        s.borrow_mut().buf.iter_mut().for_each(|p| *p = p.scale(f));
        Ok(())
    });
    let s = shared.clone();
    e.register_fn("set_xy", move |x: INT, y: INT, c: INT| {
        let mut sh = s.borrow_mut();
        if let Some(i) = sh.map.index_at(x, y, 0) {
            sh.buf[i] = Rgb::from_packed(c);
        }
    });
    let s = shared.clone();
    e.register_fn("set_xyz", move |x: INT, y: INT, z: INT, c: INT| {
        let mut sh = s.borrow_mut();
        if let Some(i) = sh.map.index_at(x, y, z) {
            sh.buf[i] = Rgb::from_packed(c);
        }
    });
    let s = shared.clone();
    e.register_fn("shift", move |by: INT| {
        let mut sh = s.borrow_mut();
        let n = sh.buf.len() as INT;
        if n > 0 {
            sh.buf.rotate_right(by.rem_euclid(n) as usize);
        }
    });

    // --- audio ------------------------------------------------------------
    let s = shared.clone();
    e.register_fn("audio_level", move || s.borrow().audio.level as FLOAT);
    let s = shared.clone();
    e.register_fn("audio_bass", move || s.borrow().audio.bass as FLOAT);
    let s = shared.clone();
    e.register_fn("audio_mid", move || s.borrow().audio.mid as FLOAT);
    let s = shared.clone();
    e.register_fn("audio_treble", move || s.borrow().audio.treble as FLOAT);
    let s = shared.clone();
    e.register_fn("audio_beat", move || s.borrow().audio.beat as FLOAT);
    let s = shared.clone();
    e.register_fn("audio_band", move |f: Dynamic| -> Fallible<FLOAT> { Ok(s.borrow().audio.band_at(num(&f)? as f32) as FLOAT) });

    e
}

/// True if the script reads audio, so the analyser only runs when needed.
pub fn uses_audio(code: &str) -> bool {
    code.contains("audio_")
}

/// Compile-only check used by the editor.
pub fn check(code: &str) -> Result<(), String> {
    let shared = Rc::new(RefCell::new(Shared {
        buf: Vec::new(),
        map: Arc::new(crate::layout::build(&Default::default())),
        audio: AudioFrame::default(),
    }));
    let engine = build_engine(&shared);
    let ast = engine.compile(code).map_err(|e| e.to_string())?;
    entry_points(&ast).map(|_| ())
}

fn entry_points(ast: &AST) -> Result<(bool, Option<usize>, bool), String> {
    let mut has_frame = false;
    let mut pixel_arity = None;
    let mut has_init = false;
    for f in ast.iter_functions() {
        match (f.name, f.params.len()) {
            ("frame", 2) => has_frame = true,
            ("frame", n) => return Err(format!("frame() takes (t, dt), but yours has {n} parameter(s)")),
            ("pixel", n @ (2 | 5)) => pixel_arity = Some(n),
            ("pixel", n) => {
                return Err(format!("pixel() takes (i, t) or (i, x, y, z, t), but yours has {n} parameter(s)"))
            }
            ("init", 0) => has_init = true,
            _ => {}
        }
    }
    if !has_frame && pixel_arity.is_none() {
        return Err("Define fn pixel(i, x, y, z, t) or fn frame(t, dt) so there is something to draw".into());
    }
    Ok((has_frame, pixel_arity, has_init))
}

impl ScriptRunner {
    pub fn new(code: &str, map: Arc<LayoutMap>) -> Result<Self, String> {
        let shared = Rc::new(RefCell::new(Shared {
            buf: vec![Rgb::BLACK; map.count],
            map,
            audio: AudioFrame::default(),
        }));
        let engine = build_engine(&shared);
        let ast = engine.compile(code).map_err(|e| e.to_string())?;
        let (has_frame, pixel_arity, has_init) = entry_points(&ast)?;

        let mut scope = Scope::new();
        // Run top-level statements once.
        engine.run_ast_with_scope(&mut scope, &ast).map_err(|e| e.to_string())?;
        let state = if has_init {
            engine
                .call_fn_with_options::<Dynamic>(CallFnOptions::new().eval_ast(false), &mut scope, &ast, "init", ())
                .map_err(|e| e.to_string())?
        } else {
            Dynamic::from_map(rhai::Map::new())
        };

        Ok(ScriptRunner { engine, ast, scope, state, has_frame, pixel_arity, shared, error: None })
    }

    pub fn render(&mut self, t: f32, dt: f32, audio: &AudioFrame, out: &mut [Rgb]) {
        if self.error.is_some() {
            out.copy_from_slice(&self.shared.borrow().buf[..out.len()]);
            return;
        }
        {
            let mut sh = self.shared.borrow_mut();
            sh.audio = audio.clone();
            if sh.buf.len() != out.len() {
                sh.buf = vec![Rgb::BLACK; out.len()];
            }
        }
        if let Err(e) = self.run(t as FLOAT, dt as FLOAT, out.len()) {
            self.error = Some(e.to_string());
        }
        out.copy_from_slice(&self.shared.borrow().buf[..out.len()]);
    }

    fn run(&mut self, t: FLOAT, dt: FLOAT, n: usize) -> Fallible<()> {
        if self.has_frame {
            let opts = CallFnOptions::new().eval_ast(false).bind_this_ptr(&mut self.state);
            let _: Dynamic = self.engine.call_fn_with_options(opts, &mut self.scope, &self.ast, "frame", (t, dt))?;
        }
        let Some(arity) = self.pixel_arity else { return Ok(()) };
        let map = self.shared.borrow().map.clone();
        for i in 0..n {
            let opts = CallFnOptions::new().eval_ast(false).bind_this_ptr(&mut self.state);
            let result: Dynamic = if arity == 2 {
                self.engine.call_fn_with_options(opts, &mut self.scope, &self.ast, "pixel", (i as INT, t))?
            } else {
                let p = map.pos[i];
                let args = (i as INT, p[0] as FLOAT, p[1] as FLOAT, p[2] as FLOAT, t);
                self.engine.call_fn_with_options(opts, &mut self.scope, &self.ast, "pixel", args)?
            };
            let c = result
                .as_int()
                .map_err(|ty| format!("pixel() must return a colour such as hsv(...) or rgb(...), but returned {ty}"))?;
            self.shared.borrow_mut().buf[i] = Rgb::from_packed(c);
        }
        Ok(())
    }
}

pub const EXAMPLES: &[(&str, &str)] = &[
    (
        "Rainbow wave",
        r#"// pixel() runs once for every LED, every frame, and returns its colour.
//   i        LED number (0 = first)
//   x, y, z  where the LED is, from 0.0 to 1.0
//   t        seconds since the script started
fn pixel(i, x, y, z, t) {
    hsv(x + y * 0.5 + t * 0.2, 1.0, 1.0)
}
"#,
    ),
    (
        "Breathing colours",
        r#"// The short form of pixel() only takes the LED number and the time.
fn pixel(i, t) {
    let glow = wave(t * 0.25);            // 0..1, one breath every 4 seconds
    dim(hsv(t * 0.05, 0.8, 1.0), glow)
}
"#,
    ),
    (
        "Comet with trail",
        r#"// frame() runs once per frame and draws with set(), fill() and fade().
// The LEDs keep their colour between frames, which is how trails work.
fn init() {
    #{ head: 0.0 }
}

fn frame(t, dt) {
    fade(0.9);                                  // dim everything a little
    this.head += dt * 40.0;                     // 40 LEDs per second
    if this.head >= led_count() { this.head = 0.0; }
    set(this.head.to_int(), hsv(t * 0.1, 1.0, 1.0));
}
"#,
    ),
    (
        "Plasma (2D)",
        r#"fn pixel(i, x, y, z, t) {
    let v = wave(x * 2.0 + t * 0.3)
          + wave(y * 3.0 - t * 0.2)
          + wave((x + y) * 1.5 + t * 0.1);
    palette("Sunset", v / 3.0 + t * 0.05)
}
"#,
    ),
    (
        "Music bars",
        r#"// audio_band(f) is the loudness at a point across the spectrum:
// 0.0 = deep bass, 1.0 = high treble.
fn pixel(i, x, y, z, t) {
    let level = audio_band(x);
    let lit = if height() > 1 { if 1.0 - y <= level { 1.0 } else { 0.0 } } else { level };
    dim(hsv(x * 0.8, 1.0, 1.0), lit)
}
"#,
    ),
    (
        "Sparkle rain",
        r##"fn frame(t, dt) {
    fade(0.92);
    // Light a few random LEDs each frame.
    for n in 0..3 {
        let i = rand_int(0, led_count() - 1);
        set(i, mix(hex("#40a0ff"), hex("#ffffff"), rand()));
    }
}
"##,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;
    use crate::model::{Layout, MatrixWiring};

    fn strip(n: u32) -> Arc<LayoutMap> {
        Arc::new(layout::build(&Layout::Strip { count: n, reversed: false }))
    }

    fn render(code: &str, map: Arc<LayoutMap>, frames: usize) -> (Vec<Rgb>, Option<String>) {
        let n = map.count;
        let mut runner = ScriptRunner::new(code, map).expect("script should compile");
        let mut out = vec![Rgb::BLACK; n];
        let audio = AudioFrame::default();
        for f in 0..frames {
            runner.render(f as f32 / 60.0, 1.0 / 60.0, &audio, &mut out);
        }
        (out, runner.error)
    }

    #[test]
    fn pixel_function_colours_every_led() {
        let (out, err) = render("fn pixel(i, x, y, z, t) { if i == 2 { rgb(255, 0, 0) } else { rgb(0, 0, 255) } }", strip(4), 1);
        assert_eq!(err, None);
        assert_eq!(out[2].to_u8(), [255, 0, 0]);
        assert_eq!(out[0].to_u8(), [0, 0, 255]);
    }

    #[test]
    fn integers_and_floats_mix_freely() {
        // People will write hsv(0, 1, 1) just as often as hsv(0.0, 1.0, 1.0).
        let (out, err) = render("fn pixel(i, t) { hsv(0, 1, 1) }", strip(1), 1);
        assert_eq!(err, None);
        assert_eq!(out[0].to_u8(), [255, 0, 0]);
    }

    #[test]
    fn frame_function_keeps_state_and_pixels() {
        let code = r#"
            fn init() { #{ n: 0 } }
            fn frame(t, dt) { set(this.n, rgb(0, 255, 0)); this.n += 1; }
        "#;
        let (out, err) = render(code, strip(5), 3);
        assert_eq!(err, None);
        let lit: Vec<bool> = out.iter().map(|c| c.g > 0.9).collect();
        assert_eq!(lit, vec![true, true, true, false, false]);
    }

    #[test]
    fn grid_helpers_follow_the_wiring() {
        let map = Arc::new(layout::build(&Layout::Matrix { width: 3, height: 2, wiring: MatrixWiring::default() }));
        // Serpentine: the second row runs right-to-left, so (0, 1) is LED 5.
        let (out, err) = render("fn frame(t, dt) { set_xy(0, 1, rgb(255, 255, 255)); set_xy(9, 9, rgb(255, 0, 0)); }", map, 1);
        assert_eq!(err, None);
        assert_eq!(out[5].to_u8(), [255, 255, 255]);
        assert_eq!(out.iter().filter(|c| c.peak() > 0.0).count(), 1);
    }

    #[test]
    fn every_example_runs_cleanly() {
        let matrix = Arc::new(layout::build(&Layout::Matrix { width: 8, height: 8, wiring: MatrixWiring::default() }));
        for (name, code) in EXAMPLES {
            assert_eq!(check(code), Ok(()), "{name} failed the editor check");
            for map in [strip(30), matrix.clone()] {
                let (_, err) = render(code, map, 20);
                assert_eq!(err, None, "{name} failed at runtime");
            }
        }
    }

    /// Every script shown in the README and the scripting guide must work.
    #[test]
    fn documented_scripts_run() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
        let matrix = Arc::new(layout::build(&Layout::Matrix { width: 8, height: 8, wiring: MatrixWiring::default() }));
        let mut found = 0;
        for file in ["README.md", "docs/scripting.md"] {
            let text = std::fs::read_to_string(format!("{root}/{file}")).expect("docs should exist");
            for block in text.split("```rust").skip(1) {
                let code = block.split("```").next().unwrap_or_default();
                if !code.contains("fn pixel") && !code.contains("fn frame") {
                    continue;
                }
                found += 1;
                assert_eq!(check(code), Ok(()), "a script in {file} does not compile:
{code}");
                for map in [strip(30), matrix.clone()] {
                    let (_, err) = render(code, map, 90);
                    assert_eq!(err, None, "a script in {file} fails when run:
{code}");
                }
            }
        }
        assert!(found >= 8, "expected to find the documented examples, found {found}");
    }

    #[test]
    fn mistakes_are_reported_not_fatal() {
        assert!(check("fn pixel(i, t) { hsv(0, 1, 1 }").is_err(), "syntax error");
        assert!(check("let x = 1;").unwrap_err().contains("pixel"));
        assert!(check("fn pixel(a) { 0 }").unwrap_err().contains("pixel()"));

        let (_, err) = render("fn pixel(i, t) { nothing_like_this(i) }", strip(3), 2);
        assert!(err.unwrap().contains("nothing_like_this"));

        let (_, err) = render("fn pixel(i, t) { \"red\" }", strip(3), 1);
        assert!(err.unwrap().contains("must return a colour"));
    }

    #[test]
    fn infinite_loops_are_cut_short() {
        let started = std::time::Instant::now();
        let (_, err) = render("fn frame(t, dt) { loop { } }", strip(3), 1);
        assert!(err.is_some());
        assert!(started.elapsed().as_secs() < 10);
    }
}
