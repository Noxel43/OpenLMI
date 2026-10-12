//! Visual effects that need no input other than time.

use super::font;
use super::*;
use crate::color::{fbm01, hash01, hash01_2, noise01, smoothstep};
use std::f32::consts::{PI, TAU};

fn fx(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    description: &'static str,
    params: Vec<ParamDef>,
    make: fn() -> Box<dyn Effect>,
) -> EffectDef {
    EffectDef { id, name, category, description, audio: false, grid_only: false, params, make }
}

pub fn effects() -> Vec<EffectDef> {
    vec![
        fx("solid", "Solid", "Basic", "One steady colour.",
            vec![color("color", "Colour", "#ff9d3c")],
            || Box::new(Stateless(solid))),
        fx("gradient", "Gradient", "Basic", "A palette stretched along the LEDs, optionally drifting.",
            vec![palette("Sunset"), slider("scale", "Repeats", 0.25, 4.0, 0.05, 1.0), speed(0.0)],
            || Box::new(Stateless(gradient))),
        fx("rainbow", "Rainbow", "Basic", "The full colour wheel in motion.",
            vec![slider("scale", "Density", 0.1, 4.0, 0.05, 1.0), slider("saturation", "Saturation", 0.0, 1.0, 0.01, 1.0), speed(1.0)],
            || Box::new(Stateless(rainbow))),
        fx("cycle", "Colour Cycle", "Basic", "Everything fades slowly through a palette.",
            vec![palette("Rainbow"), slider("spread", "Spread", 0.0, 1.0, 0.01, 0.0), speed(1.0)],
            || Box::new(Stateless(cycle))),
        fx("breathe", "Breathe", "Basic", "A calm pulse, changing colour on every breath.",
            vec![palette("Ice"), slider("floor", "Minimum", 0.0, 0.6, 0.01, 0.05), speed(1.0)],
            || Box::new(Stateless(breathe))),
        fx("heartbeat", "Heartbeat", "Basic", "A double thump.",
            vec![color("color", "Colour", "#ff1744"), slider("floor", "Minimum", 0.0, 0.6, 0.01, 0.04), speed(1.0)],
            || Box::new(Stateless(heartbeat))),

        fx("wipe", "Colour Wipe", "Motion", "Each palette colour sweeps over the last.",
            vec![palette("Party"), slider("soft", "Softness", 0.0, 0.3, 0.005, 0.03), toggle("bounce", "Back and forth", false), speed(1.0)],
            || Box::new(Stateless(wipe))),
        fx("chase", "Theatre Chase", "Motion", "Marquee lights marching in step.",
            vec![palette("Warm White"), color("background", "Background", "#000000"), slider("spacing", "Spacing", 2.0, 12.0, 1.0, 3.0), speed(1.0)],
            || Box::new(Stateless(chase))),
        fx("scanner", "Scanner", "Motion", "A glowing eye sweeping back and forth.",
            vec![color("color", "Colour", "#ff1a1a"), slider("width", "Width", 0.02, 0.5, 0.005, 0.1), toggle("dual", "Two eyes", false), speed(1.0)],
            || Box::new(Stateless(scanner))),
        fx("comet", "Comet", "Motion", "Bright heads with fading tails.",
            vec![palette("Ice"), slider("count", "Comets", 1.0, 8.0, 1.0, 2.0), slider("length", "Tail", 0.05, 1.0, 0.01, 0.35), toggle("reverse", "Reverse", false), speed(1.0)],
            || Box::new(Stateless(comet))),
        fx("running", "Running Lights", "Motion", "Smooth waves of light rolling along.",
            vec![palette("Cyberpunk"), slider("waves", "Waves", 1.0, 12.0, 0.5, 3.0), speed(1.0)],
            || Box::new(Stateless(running))),
        fx("meteor", "Meteor", "Motion", "A meteor with a crumbling, sparkling trail.",
            vec![palette("Lava"), slider("size", "Size", 0.02, 0.4, 0.005, 0.08), slider("decay", "Trail decay", 0.05, 1.0, 0.01, 0.35), speed(1.0)],
            || Box::new(Meteor::default())),
        fx("bounce", "Bouncing Balls", "Motion", "Balls bouncing under gravity.",
            vec![palette("Rainbow"), slider("count", "Balls", 1.0, 8.0, 1.0, 3.0), slider("size", "Size", 0.01, 0.2, 0.005, 0.045), speed(1.0)],
            || Box::new(Stateless(bounce))),
        fx("juggle", "Juggle", "Motion", "Coloured dots weaving past each other, leaving trails.",
            vec![palette("Rainbow"), slider("count", "Dots", 1.0, 8.0, 1.0, 4.0), slider("trail", "Trail", 0.0, 1.0, 0.01, 0.6), speed(1.0)],
            || Box::new(Juggle::default())),

        fx("fire", "Fire", "Nature", "Flickering flames rising from the bottom.",
            vec![palette("Fire"), slider("intensity", "Intensity", 0.0, 1.0, 0.01, 0.6), slider("scale", "Detail", 0.3, 4.0, 0.05, 1.0), speed(1.0)],
            || Box::new(Stateless(fire))),
        fx("plasma", "Plasma", "Nature", "Liquid blobs of colour.",
            vec![palette("Rainbow"), slider("scale", "Scale", 0.2, 4.0, 0.05, 1.0), speed(1.0)],
            || Box::new(Stateless(plasma))),
        fx("flow", "Flow", "Nature", "Soft drifting noise, like a lava lamp.",
            vec![palette("Lava"), slider("scale", "Scale", 0.2, 6.0, 0.05, 1.2), speed(1.0)],
            || Box::new(Stateless(flow))),
        fx("aurora", "Aurora", "Nature", "Curtains of light with dark gaps between them.",
            vec![palette("Aurora"), slider("scale", "Scale", 0.2, 6.0, 0.05, 1.0), speed(1.0)],
            || Box::new(Stateless(aurora))),
        fx("waves", "Ocean", "Nature", "Layered swells with the occasional white cap.",
            vec![palette("Ocean"), slider("scale", "Scale", 0.2, 4.0, 0.05, 1.0), speed(1.0)],
            || Box::new(Stateless(waves))),
        fx("ripple", "Ripple", "Nature", "Rings spreading out from the centre.",
            vec![palette("Aurora"), slider("rings", "Rings", 0.5, 8.0, 0.1, 2.5), speed(1.0)],
            || Box::new(Stateless(ripple))),
        fx("rain", "Digital Rain", "Nature", "Drops streaming down, each with a bright head.",
            vec![palette("Matrix"), slider("density", "Density", 0.1, 1.0, 0.01, 0.7), slider("trail", "Trail", 0.1, 1.5, 0.01, 0.6), speed(1.0)],
            || Box::new(Stateless(rain))),

        fx("twinkle", "Twinkle", "Party", "LEDs softly fading in and out at random.",
            vec![palette("Candy"), slider("density", "Density", 0.05, 1.0, 0.01, 0.55), slider("background", "Background", 0.0, 0.5, 0.01, 0.0), speed(1.0)],
            || Box::new(Stateless(twinkle))),
        fx("sparkle", "Sparkle", "Party", "Sharp glints over a steady base colour.",
            vec![color("color", "Base", "#12103a"), color("spark", "Sparkle", "#ffffff"), slider("density", "Density", 0.02, 1.0, 0.01, 0.3), speed(1.0)],
            || Box::new(Stateless(sparkle))),
        fx("confetti", "Confetti", "Party", "Random specks of colour that fade away.",
            vec![palette("Party"), slider("density", "Density", 0.05, 1.0, 0.01, 0.4), slider("fade", "Fade", 0.2, 6.0, 0.05, 2.0), speed(1.0)],
            || Box::new(Confetti::default())),
        fx("police", "Police", "Party", "Alternating double flashes.",
            vec![color("a", "Left", "#ff0000"), color("b", "Right", "#0030ff"), speed(1.0)],
            || Box::new(Stateless(police))),
        fx("strobe", "Strobe", "Party", "Hard flashes. Use with care.",
            vec![color("color", "Colour", "#ffffff"), slider("rate", "Flashes / second", 1.0, 12.0, 0.5, 6.0), slider("duty", "Flash length", 0.05, 0.5, 0.01, 0.2)],
            || Box::new(Stateless(strobe))),

        EffectDef {
            grid_only: true,
            ..fx("text", "Scrolling Text", "Matrix", "A message on a matrix. Use {time} or {date} for a clock.",
                vec![text("text", "Text", "OpenLMI  {time}"), palette("Cyberpunk"), color("background", "Background", "#000000"), toggle("scroll", "Always scroll", true), speed(1.0)],
                || Box::new(Stateless(text_fx)))
        },
    ]
}

// --- helpers ----------------------------------------------------------------

/// A coordinate for effects that travel: along the strip, or diagonally
/// across a grid so 2D layouts don't look like a plain column sweep.
fn travel(map: &LayoutMap, i: usize) -> f32 {
    if map.is_grid && map.dims[1] > 1 {
        let p = map.pos[i];
        p[0] + p[1] * 0.5 + p[2] * 0.25
    } else {
        map.u[i]
    }
}

/// Position with square cells, so circles stay round on non-square grids.
fn space(map: &LayoutMap, i: usize) -> [f32; 3] {
    let p = map.pos[i];
    if map.is_grid {
        let m = map.dims.iter().copied().max().unwrap_or(1).max(1) as f32;
        [p[0] * map.dims[0] as f32 / m, p[1] * map.dims[1] as f32 / m, p[2] * map.dims[2] as f32 / m]
    } else {
        p
    }
}

fn centre(map: &LayoutMap) -> [f32; 3] {
    if map.is_grid {
        let m = map.dims.iter().copied().max().unwrap_or(1).max(1) as f32;
        [0.5 * map.dims[0] as f32 / m, 0.5 * map.dims[1] as f32 / m, 0.5 * map.dims[2] as f32 / m]
    } else {
        [0.5, 0.5, 0.5]
    }
}

fn gauss(d: f32, width: f32) -> f32 {
    let x = d / width.max(1e-4);
    (-x * x).exp()
}

fn stop(pal: &Palette, k: i64) -> Rgb {
    let n = pal.stops.len().max(1) as i64;
    pal.stops.get(k.rem_euclid(n) as usize).copied().unwrap_or(Rgb::WHITE)
}

// --- basic -------------------------------------------------------------------

fn solid(cx: &Ctx, out: &mut [Rgb]) {
    out.fill(cx.p.color("color"));
}

fn gradient(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let scale = cx.p.num("scale");
    let seamless = cx.map.looped || cx.p.num("speed") > 0.0;
    for (i, o) in out.iter_mut().enumerate() {
        let u = cx.map.u[i] * scale;
        *o = if seamless {
            pal.sample(u + cx.t * 0.15)
        } else if u > 1.0 {
            pal.sample_clamped(u.rem_euclid(1.0))
        } else {
            pal.sample_clamped(u)
        };
    }
}

fn rainbow(cx: &Ctx, out: &mut [Rgb]) {
    let (scale, sat) = (cx.p.num("scale"), cx.p.num("saturation"));
    for (i, o) in out.iter_mut().enumerate() {
        *o = Rgb::hsv(travel(cx.map, i) * scale - cx.t * 0.2, sat, 1.0);
    }
}

fn cycle(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let spread = cx.p.num("spread");
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal.sample(cx.t * 0.08 + travel(cx.map, i) * spread);
    }
}

fn breathe(cx: &Ctx, out: &mut [Rgb]) {
    let phase = cx.t * 0.25;
    let f = phase.fract();
    let level = ((f * TAU - PI / 2.0).sin() * 0.5 + 0.5).powf(1.6);
    let floor = cx.p.num("floor");
    out.fill(stop(cx.p.palette(), phase.floor() as i64).scale(floor + (1.0 - floor) * level));
}

fn heartbeat(cx: &Ctx, out: &mut [Rgb]) {
    let f = (cx.t / 1.1).fract();
    let beat = (gauss(f - 0.08, 0.045) + 0.7 * gauss(f - 0.3, 0.06)).min(1.0);
    let floor = cx.p.num("floor");
    out.fill(cx.p.color("color").scale(floor + (1.0 - floor) * beat));
}

// --- motion ------------------------------------------------------------------

fn wipe(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let soft = cx.p.num("soft");
    let phase = cx.t * 0.4;
    let k = phase.floor() as i64;
    let backwards = cx.p.flag("bounce") && k.rem_euclid(2) == 1;
    // Push the edge slightly past both ends so the wipe fully completes.
    let edge = phase.fract() * (1.0 + 2.0 * soft) - soft;
    let (new, old) = (stop(pal, k), stop(pal, k - 1));
    for (i, o) in out.iter_mut().enumerate() {
        let u = if backwards { 1.0 - cx.map.u[i] } else { cx.map.u[i] };
        *o = new.lerp(old, smoothstep(edge - soft, edge + soft, u));
    }
}

fn chase(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let bg = cx.p.color("background");
    let spacing = cx.p.num("spacing").round().max(2.0);
    let shift = cx.t * 6.0;
    for (i, o) in out.iter_mut().enumerate() {
        let index = if cx.map.is_grid {
            let g = cx.map.grid[i];
            (g[0] as u32 + g[1] as u32 + g[2] as u32) as f32
        } else {
            i as f32
        };
        let ph = (index - shift).rem_euclid(spacing);
        let lit = (1.0 - ph.min(spacing - ph)).clamp(0.0, 1.0);
        *o = bg.lerp(pal.sample(travel(cx.map, i) + cx.t * 0.03), lit);
    }
}

fn scanner(cx: &Ctx, out: &mut [Rgb]) {
    let c = cx.p.color("color");
    let width = cx.p.num("width");
    let f = (cx.t * 0.35).fract();
    let tri = 1.0 - (f * 2.0 - 1.0).abs();
    let pos = 0.5 - 0.5 * (tri * PI).cos();
    let dual = cx.p.flag("dual");
    for (i, o) in out.iter_mut().enumerate() {
        let u = cx.map.u[i];
        let mut b = gauss(u - pos, width * 0.5);
        if dual {
            b = b.max(gauss(u - (1.0 - pos), width * 0.5));
        }
        *o = c.scale(b);
    }
}

fn comet(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let count = cx.p.num("count").round().max(1.0) as usize;
    let len = cx.p.num("length");
    let reverse = cx.p.flag("reverse");
    for (i, o) in out.iter_mut().enumerate() {
        let u = cx.map.u[i];
        let mut c = Rgb::BLACK;
        for k in 0..count {
            let kf = k as f32 / count as f32;
            let head = (cx.t * 0.25 + kf).fract();
            let behind = if reverse { u - (1.0 - head) } else { head - u }.rem_euclid(1.0);
            if behind < len {
                let b = (1.0 - behind / len).powf(2.2);
                // The very front of the comet burns white.
                let tint = pal.sample(kf + behind * 0.4).lerp(Rgb::WHITE, (1.0 - behind / len * 12.0).max(0.0) * 0.6);
                c = c.max(tint.scale(b));
            }
        }
        *o = c;
    }
}

fn running(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let waves = cx.p.num("waves");
    for (i, o) in out.iter_mut().enumerate() {
        let v = travel(cx.map, i);
        let b = ((v * waves - cx.t * 0.6) * TAU).sin() * 0.5 + 0.5;
        *o = pal.sample(v * 0.5 + cx.t * 0.05).scale(b * b);
    }
}

#[derive(Default)]
struct Meteor {
    buf: Vec<Rgb>,
}

impl Effect for Meteor {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        if self.buf.len() != out.len() {
            self.buf = vec![Rgb::BLACK; out.len()];
        }
        let pal = cx.p.palette();
        let size = cx.p.num("size");
        // Each LED crumbles at random, which gives the trail its sparkle.
        let chance = (cx.p.num("decay") * cx.real_dt * 30.0).min(1.0);
        for px in self.buf.iter_mut() {
            if fastrand::f32() < chance {
                *px = px.scale(0.6);
            }
        }
        let head = (cx.t * 0.25).fract() * (1.0 + size * 2.0) - size;
        for (i, px) in self.buf.iter_mut().enumerate() {
            let d = head - cx.map.u[i];
            if (0.0..size).contains(&d) {
                *px = pal.sample_clamped(1.0 - d / size);
            }
        }
        out.copy_from_slice(&self.buf);
    }
}

fn bounce(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let count = cx.p.num("count").round().max(1.0) as usize;
    let size = cx.p.num("size");
    let lanes = cx.map.is_grid && cx.map.dims[1] > 1;
    let lane_w = (0.6 / count as f32).max(size);
    out.fill(Rgb::BLACK);
    for k in 0..count {
        let seed = k as u32;
        let period = 1.0 + hash01(seed * 7 + 1) * 0.9;
        let f = (cx.t / period + hash01(seed * 13 + 5)).fract();
        let height = 4.0 * f * (1.0 - f) * (0.55 + 0.45 * hash01(seed * 3 + 50)) * (1.0 - size) + size * 0.5;
        let c = pal.sample(k as f32 / count as f32);
        let lx = (k as f32 + 0.5) / count as f32;
        let lz = hash01(seed * 29 + 99);
        for (i, o) in out.iter_mut().enumerate() {
            let mut b = gauss(cx.map.along(i) - height, size);
            if lanes {
                let p = cx.map.pos[i];
                b *= gauss(p[0] - lx, lane_w);
                if cx.map.dims[2] > 1 {
                    b *= gauss(p[2] - lz, 0.3);
                }
            }
            *o = o.max(c.scale(b));
        }
    }
}

#[derive(Default)]
struct Juggle {
    buf: Vec<Rgb>,
}

impl Effect for Juggle {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        if self.buf.len() != out.len() {
            self.buf = vec![Rgb::BLACK; out.len()];
        }
        let pal = cx.p.palette();
        let count = cx.p.num("count").round().max(1.0) as usize;
        // Longer trail = slower fade.
        let fade = (-cx.real_dt * (14.0 - 12.5 * cx.p.num("trail"))).exp();
        for px in self.buf.iter_mut() {
            *px = px.scale(fade);
        }
        let width = (1.5 / out.len().max(1) as f32).max(0.012);
        for k in 0..count {
            let kf = k as f32;
            let pos = 0.5 + 0.5 * (cx.t * (0.7 + kf * 0.23) + kf * 1.3).sin();
            let c = pal.sample(kf / count as f32 + cx.t * 0.02);
            for (i, px) in self.buf.iter_mut().enumerate() {
                let b = gauss(cx.map.u[i] - pos, width);
                if b > 0.01 {
                    *px = px.max(c.scale(b));
                }
            }
        }
        out.copy_from_slice(&self.buf);
    }
}

// --- nature ------------------------------------------------------------------

fn fire(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let intensity = cx.p.num("intensity");
    let s = cx.p.num("scale") * 3.0;
    for (i, o) in out.iter_mut().enumerate() {
        let p = space(cx.map, i);
        let h = cx.map.along(i);
        let n = fbm01(p[0] * s + p[2] * 1.7, h * s * 1.4 - cx.t * 1.3, p[2] * s + cx.t * 0.25, 3);
        let heat = (n * 1.35 - h * (1.7 - intensity) + 0.22).clamp(0.0, 1.0);
        *o = pal.sample_clamped(heat);
    }
}

fn plasma(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let s = cx.p.num("scale") * 6.0;
    let c = centre(cx.map);
    let t = cx.t;
    for (i, o) in out.iter_mut().enumerate() {
        let [x, y, z] = space(cx.map, i);
        let r = ((x - c[0]).powi(2) + (y - c[1]).powi(2) + (z - c[2]).powi(2)).sqrt();
        let v = (x * s + t).sin()
            + (y * s * 1.3 - t * 0.8).sin()
            + ((x + y + z) * s * 0.7 + t * 0.6).sin()
            + (r * s * 1.5 - t * 1.2).sin();
        *o = pal.sample(v * 0.125 + 0.5 + t * 0.03);
    }
}

fn flow(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let s = cx.p.num("scale") * 2.0;
    for (i, o) in out.iter_mut().enumerate() {
        let [x, y, z] = space(cx.map, i);
        let n = fbm01(x * s, y * s + cx.t * 0.15, z * s + cx.t * 0.2, 3);
        *o = pal.sample(n * 1.6 + cx.t * 0.04);
    }
}

fn aurora(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let s = cx.p.num("scale") * 2.0;
    for (i, o) in out.iter_mut().enumerate() {
        let [x, y, z] = space(cx.map, i);
        let hue = noise01(x * s + cx.t * 0.07, y * s * 0.5, z * s + cx.t * 0.11);
        let veil = smoothstep(0.3, 0.72, noise01(x * s * 1.6 - cx.t * 0.13, y * s * 0.6 + 9.0, z * s + cx.t * 0.21));
        *o = pal.sample(hue * 1.3 + cx.t * 0.02).scale(veil);
    }
}

fn waves(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let s = cx.p.num("scale");
    let t = cx.t;
    for (i, o) in out.iter_mut().enumerate() {
        let v = travel(cx.map, i) * s;
        let swell = (v * TAU + t * 0.9).sin()
            + 0.6 * (v * TAU * 2.3 - t * 1.3 + 1.7).sin()
            + 0.4 * (v * TAU * 3.7 + t * 0.6 + 4.1).sin()
            + 0.3 * (cx.map.up[i] * TAU * 1.5 + t * 0.5).sin();
        let level = (swell / 2.3 * 0.5 + 0.5).clamp(0.0, 1.0);
        let cap = smoothstep(0.82, 0.97, level);
        *o = pal.sample_clamped(level).lerp(Rgb::WHITE, cap * 0.7);
    }
}

fn ripple(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let rings = cx.p.num("rings");
    let c = centre(cx.map);
    for (i, o) in out.iter_mut().enumerate() {
        let [x, y, z] = space(cx.map, i);
        let d = ((x - c[0]).powi(2) + (y - c[1]).powi(2) + (z - c[2]).powi(2)).sqrt();
        let v = (d * rings * TAU - cx.t * 3.0).sin() * 0.5 + 0.5;
        *o = pal.sample(d * 0.8 - cx.t * 0.1).scale(0.12 + 0.88 * v * v);
    }
}

fn rain(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let density = cx.p.num("density");
    let trail = cx.p.num("trail");
    let lanes = cx.map.lane_count() > 1;
    // A lone strip has one lane, so give it several drops instead.
    let drops = if lanes { 2 } else { (density * 6.0).round().max(1.0) as u32 };
    for (i, o) in out.iter_mut().enumerate() {
        let lane = cx.map.lane(i);
        let depth = if lanes { 1.0 - cx.map.along(i) } else { cx.map.u[i] };
        let mut best = 0.0f32;
        for k in 0..drops {
            let seed = lane.wrapping_mul(131).wrapping_add(k * 17 + 1);
            let cyc = cx.t * (0.3 + 0.45 * hash01(seed)) + hash01(seed ^ 0x5bd1);
            if lanes && hash01_2(seed, cyc.floor() as u32) > density {
                continue;
            }
            let delta = cyc.fract() * (1.0 + trail) - depth;
            if (0.0..trail).contains(&delta) {
                best = best.max((1.0 - delta / trail).powf(1.5));
            }
        }
        *o = pal.sample_clamped(best).scale(best.sqrt());
    }
}

// --- party -------------------------------------------------------------------

fn twinkle(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let density = cx.p.num("density");
    let bg = cx.p.num("background");
    for (i, o) in out.iter_mut().enumerate() {
        let id = i as u32;
        let ph = hash01(id) + cx.t * (0.25 + 0.55 * hash01(id.wrapping_add(7919)));
        let cyc = ph.floor() as u32;
        let base = pal.sample(hash01(id.wrapping_mul(3) + 11)).scale(bg);
        *o = if hash01_2(id, cyc) < density {
            let b = (ph.fract() * PI).sin().powi(2);
            base.max(pal.sample(hash01_2(id.wrapping_mul(3), cyc)).scale(b))
        } else {
            base
        };
    }
}

fn sparkle(cx: &Ctx, out: &mut [Rgb]) {
    let base = cx.p.color("color");
    let spark = cx.p.color("spark");
    let chance = cx.p.num("density") * 0.05;
    let tick = (cx.t * 24.0).max(0.0).floor() as u32;
    for (i, o) in out.iter_mut().enumerate() {
        let mut s = 0.0f32;
        for back in 0..3u32 {
            if hash01_2(i as u32, tick.wrapping_sub(back)) < chance {
                s = s.max(1.0 - back as f32 / 3.0);
            }
        }
        *o = (base + spark.scale(s)).clamped();
    }
}

#[derive(Default)]
struct Confetti {
    buf: Vec<Rgb>,
    owed: f32,
}

impl Effect for Confetti {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        if self.buf.len() != out.len() {
            self.buf = vec![Rgb::BLACK; out.len()];
        }
        let pal = cx.p.palette();
        let fade = (-cx.real_dt * cx.p.num("fade")).exp();
        for px in self.buf.iter_mut() {
            *px = px.scale(fade);
        }
        let n = out.len();
        self.owed += cx.p.num("density") * cx.dt * (n as f32 * 1.5).max(8.0);
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            let i = fastrand::usize(..n.max(1));
            self.buf[i] = pal.sample(fastrand::f32() * 0.25 + cx.t * 0.05);
        }
        out.copy_from_slice(&self.buf);
    }
}

fn police(cx: &Ctx, out: &mut [Rgb]) {
    let (a, b) = (cx.p.color("a"), cx.p.color("b"));
    let phase = cx.t * 1.6;
    let left_turn = (phase.floor() as i64).rem_euclid(2) == 0;
    let f = phase.fract();
    let on = f < 0.2 || (0.32..0.52).contains(&f);
    for (i, o) in out.iter_mut().enumerate() {
        let left = cx.map.u[i] < 0.5;
        *o = if on && left == left_turn {
            if left {
                a
            } else {
                b
            }
        } else {
            Rgb::BLACK
        };
    }
}

fn strobe(cx: &Ctx, out: &mut [Rgb]) {
    let on = (cx.t * cx.p.num("rate")).fract() < cx.p.num("duty");
    out.fill(if on { cx.p.color("color") } else { Rgb::BLACK });
}

// --- matrix ------------------------------------------------------------------

fn text_fx(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let bg = cx.p.color("background");
    let now = chrono::Local::now();
    let message = cx
        .p
        .text("text")
        .replace("{time}", &now.format("%H:%M").to_string())
        .replace("{seconds}", &now.format("%H:%M:%S").to_string())
        .replace("{date}", &now.format("%d %b").to_string());
    let chars: Vec<char> = message.chars().collect();
    let [w, h, _] = cx.map.dims;
    let (w, h) = (w as i64, h as i64);
    let tw = font::width(&chars) as i64;
    let scrolling = cx.p.flag("scroll") || tw - 1 > w;
    let shift = if scrolling {
        // Enter from the right edge, leave on the left, then start again.
        ((cx.t * 9.0) as i64).rem_euclid((tw + w).max(1)) - w
    } else {
        -(w - (tw - 1)) / 2
    };
    let top = (h - font::GLYPH_H as i64) / 2;
    for (i, o) in out.iter_mut().enumerate() {
        let g = cx.map.grid[i];
        let (col, row) = (g[0] as i64 + shift, g[1] as i64 - top);
        *o = if font::pixel(&chars, col, row) {
            pal.sample(col as f32 * 0.02 + cx.t * 0.04)
        } else {
            bg
        };
    }
}
