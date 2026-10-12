//! Effects driven by the audio analyser.

use super::*;
use std::collections::VecDeque;

fn fx(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    params: Vec<ParamDef>,
    make: fn() -> Box<dyn Effect>,
) -> EffectDef {
    EffectDef { id, name, category: "Audio", description, audio: true, grid_only: false, params, make }
}

fn sensitivity() -> ParamDef {
    slider("sensitivity", "Sensitivity", 0.3, 3.0, 0.05, 1.0)
}

pub fn effects() -> Vec<EffectDef> {
    vec![
        fx("spectrum", "Spectrum", "Frequency bars: bass on one side, treble on the other.",
            vec![palette("Synthwave"), sensitivity(), toggle("mirror", "Mirror", false)],
            || Box::new(Stateless(spectrum))),
        fx("vu", "VU Meter", "A level meter with a falling peak marker.",
            vec![palette("Meter"), sensitivity(), toggle("mirror", "From the centre", false)],
            || Box::new(Vu::default())),
        fx("beat", "Beat Pulse", "Flashes a new colour on every beat.",
            vec![palette("Cyberpunk"), sensitivity(), slider("floor", "Base glow", 0.0, 0.6, 0.01, 0.08)],
            || Box::new(Beat::default())),
        fx("soundwave", "Sound Wave", "The music scrolls along the LEDs as colour.",
            vec![palette("Rainbow"), sensitivity(), toggle("mirror", "From the centre", true), speed(1.0)],
            || Box::new(SoundWave::default())),
        fx("glow", "Music Glow", "Everything swells with the volume and drifts in colour.",
            vec![palette("Sunset"), sensitivity(), slider("floor", "Base glow", 0.0, 0.6, 0.01, 0.06)],
            || Box::new(Stateless(glow))),
    ]
}

fn spectrum(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let sens = cx.p.num("sensitivity");
    let mirror = cx.p.flag("mirror");
    let bars = cx.map.is_grid && cx.map.dims[1] > 1;
    let rows = cx.map.dims[1] as f32;
    for (i, o) in out.iter_mut().enumerate() {
        let across = if bars { cx.map.pos[i][0] } else { cx.map.u[i] };
        let f = if mirror { (across - 0.5).abs() * 2.0 } else { across };
        let level = (cx.audio.band_at(f) * sens).min(1.0);
        *o = if bars {
            // Soft top edge so bars glide rather than step.
            let lit = ((level - cx.map.up[i]) * rows + 0.5).clamp(0.0, 1.0);
            pal.sample_clamped(f).lerp(Rgb::WHITE, cx.map.up[i] * 0.35).scale(lit)
        } else {
            pal.sample_clamped(f).scale(level.powf(1.3))
        };
    }
}

#[derive(Default)]
struct Vu {
    level: f32,
    peak: f32,
}

impl Effect for Vu {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        let pal = cx.p.palette();
        let target = (cx.audio.level * cx.p.num("sensitivity")).min(1.0);
        self.level += (target - self.level) * (cx.real_dt * 18.0).min(1.0);
        self.peak = (self.peak - cx.real_dt * 0.35).max(self.level);
        let mirror = cx.p.flag("mirror");
        let cell = 1.0 / out.len().max(1) as f32;
        for (i, o) in out.iter_mut().enumerate() {
            let along = cx.map.along(i);
            let a = if mirror { (along - 0.5).abs() * 2.0 } else { along };
            *o = if a <= self.level {
                pal.sample_clamped(a)
            } else if self.peak > 0.03 && (a - self.peak).abs() <= cell.max(0.012) {
                Rgb::WHITE
            } else {
                Rgb::BLACK
            };
        }
    }
}

#[derive(Default)]
struct Beat {
    seen: u32,
    index: i64,
    env: f32,
}

impl Effect for Beat {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        let pal = cx.p.palette();
        if cx.audio.beat_count != self.seen {
            self.seen = cx.audio.beat_count;
            self.index += 1;
            self.env = 1.0;
        } else {
            self.env *= 0.5f32.powf(cx.real_dt / 0.14);
        }
        let n = pal.stops.len().max(1) as i64;
        let c = pal.stops.get(self.index.rem_euclid(n) as usize).copied().unwrap_or(Rgb::WHITE);
        let floor = cx.p.num("floor");
        let swell = (cx.audio.bass * cx.p.num("sensitivity") * 0.35).min(0.35);
        out.fill(c.scale((floor + swell + self.env * (1.0 - floor)).min(1.0)));
    }
}

#[derive(Default)]
struct SoundWave {
    history: VecDeque<Rgb>,
    owed: f32,
}

const WAVE_SPAN: usize = 160;

impl Effect for SoundWave {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        let pal = cx.p.palette();
        let sens = cx.p.num("sensitivity");
        let a = cx.audio;
        // Bass, mids and treble each pull towards a different part of the palette.
        let now = (pal.sample_clamped(0.05).scale(a.bass)
            + pal.sample_clamped(0.5).scale(a.mid)
            + pal.sample_clamped(0.95).scale(a.treble))
        .scale(sens * 1.2)
        .clamped();

        self.owed += cx.dt * 90.0;
        while self.owed >= 1.0 {
            self.owed -= 1.0;
            self.history.push_front(now);
        }
        self.history.truncate(WAVE_SPAN + 2);

        let mirror = cx.p.flag("mirror");
        for (i, o) in out.iter_mut().enumerate() {
            let u = cx.map.u[i];
            let a = if mirror { (u - 0.5).abs() * 2.0 } else { u };
            let p = a * (WAVE_SPAN - 1) as f32;
            let k = p.floor() as usize;
            let c0 = self.history.get(k).copied().unwrap_or(Rgb::BLACK);
            let c1 = self.history.get(k + 1).copied().unwrap_or(Rgb::BLACK);
            *o = c0.lerp(c1, p - k as f32);
        }
    }
}

fn glow(cx: &Ctx, out: &mut [Rgb]) {
    let pal = cx.p.palette();
    let floor = cx.p.num("floor");
    let level = (cx.audio.level * cx.p.num("sensitivity")).min(1.0);
    let b = (floor + (1.0 - floor) * level.powf(1.4)).min(1.0);
    for (i, o) in out.iter_mut().enumerate() {
        *o = pal.sample(cx.t * 0.03 + cx.audio.treble * 0.15 + cx.map.u[i] * 0.2).scale(b);
    }
}
