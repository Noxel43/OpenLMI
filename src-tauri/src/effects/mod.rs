//! The built-in effect library: a registry of effects, each with a parameter
//! schema the UI turns into controls automatically.

mod audio_fx;
mod font;
mod library;

use crate::audio::AudioFrame;
use crate::color::{palette_by_name, Palette, Rgb};
use crate::layout::LayoutMap;
use crate::model::{ParamValue, Params};
use serde::Serialize;
use std::collections::HashMap;

/// Everything an effect can see while drawing a frame.
pub struct Ctx<'a> {
    /// Seconds of animation time, already scaled by the effect's `speed`.
    pub t: f32,
    /// Scaled time since the previous frame.
    pub dt: f32,
    /// Wall-clock time since the previous frame.
    pub real_dt: f32,
    pub map: &'a LayoutMap,
    pub p: &'a ParamBag,
    pub audio: &'a AudioFrame,
}

pub trait Effect {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]);
}

/// Wraps a plain function for effects that keep no state between frames.
pub struct Stateless(pub fn(&Ctx, &mut [Rgb]));

impl Effect for Stateless {
    fn render(&mut self, cx: &Ctx, out: &mut [Rgb]) {
        (self.0)(cx, out)
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ParamDef {
    pub key: &'static str,
    pub label: &'static str,
    #[serde(flatten)]
    pub kind: ParamKind,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParamKind {
    Slider { min: f32, max: f32, step: f32, default: f32 },
    Color { default: &'static str },
    /// `default` is the name of a built-in palette.
    Palette { default: &'static str },
    Toggle { default: bool },
    Select { options: &'static [&'static str], default: &'static str },
    Text { default: &'static str },
}

pub fn slider(key: &'static str, label: &'static str, min: f32, max: f32, step: f32, default: f32) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Slider { min, max, step, default } }
}
pub fn speed(default: f32) -> ParamDef {
    slider("speed", "Speed", 0.0, 4.0, 0.05, default)
}
pub fn color(key: &'static str, label: &'static str, default: &'static str) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Color { default } }
}
pub fn palette(default: &'static str) -> ParamDef {
    ParamDef { key: "palette", label: "Palette", kind: ParamKind::Palette { default } }
}
pub fn toggle(key: &'static str, label: &'static str, default: bool) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Toggle { default } }
}
pub fn select(key: &'static str, label: &'static str, options: &'static [&'static str], default: &'static str) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Select { options, default } }
}
pub fn text(key: &'static str, label: &'static str, default: &'static str) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Text { default } }
}

/// Parameter values resolved against an effect's schema, with typed getters.
/// Built once whenever the user changes something, not per frame.
#[derive(Default, Debug)]
pub struct ParamBag {
    nums: HashMap<&'static str, f32>,
    flags: HashMap<&'static str, bool>,
    texts: HashMap<&'static str, String>,
    colors: HashMap<&'static str, Rgb>,
    palettes: HashMap<&'static str, Palette>,
    fallback: Palette,
}

impl ParamBag {
    pub fn build(defs: &[ParamDef], values: &Params) -> Self {
        let mut bag = ParamBag::default();
        for def in defs {
            let given = values.get(def.key);
            match &def.kind {
                ParamKind::Slider { min, max, default, .. } => {
                    let v = match given {
                        Some(ParamValue::Num(n)) => *n as f32,
                        _ => *default,
                    };
                    bag.nums.insert(def.key, if v.is_finite() { v.clamp(*min, *max) } else { *default });
                }
                ParamKind::Toggle { default } => {
                    let v = match given {
                        Some(ParamValue::Bool(b)) => *b,
                        _ => *default,
                    };
                    bag.flags.insert(def.key, v);
                }
                ParamKind::Color { default } => {
                    let v = match given {
                        Some(ParamValue::Text(s)) => Rgb::from_hex(s),
                        _ => None,
                    };
                    bag.colors.insert(def.key, v.or_else(|| Rgb::from_hex(default)).unwrap_or(Rgb::WHITE));
                }
                ParamKind::Palette { default } => {
                    let v = match given {
                        Some(ParamValue::List(list)) => {
                            let stops: Vec<Rgb> = list.iter().filter_map(|h| Rgb::from_hex(h)).collect();
                            (!stops.is_empty()).then(|| Palette::new(stops))
                        }
                        Some(ParamValue::Text(name)) => palette_by_name(name),
                        _ => None,
                    };
                    bag.palettes.insert(def.key, v.or_else(|| palette_by_name(default)).unwrap_or_default());
                }
                ParamKind::Select { options, default } => {
                    let v = match given {
                        Some(ParamValue::Text(s)) if options.contains(&s.as_str()) => s.clone(),
                        _ => default.to_string(),
                    };
                    bag.texts.insert(def.key, v);
                }
                ParamKind::Text { default } => {
                    let v = match given {
                        Some(ParamValue::Text(s)) => s.clone(),
                        _ => default.to_string(),
                    };
                    bag.texts.insert(def.key, v);
                }
            }
        }
        bag
    }

    pub fn num(&self, key: &str) -> f32 {
        self.nums.get(key).copied().unwrap_or(1.0)
    }
    pub fn flag(&self, key: &str) -> bool {
        self.flags.get(key).copied().unwrap_or(false)
    }
    pub fn text(&self, key: &str) -> &str {
        self.texts.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn color(&self, key: &str) -> Rgb {
        self.colors.get(key).copied().unwrap_or(Rgb::WHITE)
    }
    pub fn palette(&self) -> &Palette {
        self.palettes.get("palette").unwrap_or(&self.fallback)
    }
}

pub struct EffectDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    /// Needs the audio analyser running.
    pub audio: bool,
    /// Only meaningful on 2D/3D grids.
    pub grid_only: bool,
    pub params: Vec<ParamDef>,
    pub make: fn() -> Box<dyn Effect>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub audio: bool,
    pub grid_only: bool,
    pub params: Vec<ParamDef>,
}

pub fn registry() -> &'static [EffectDef] {
    use std::sync::OnceLock;
    static REG: OnceLock<Vec<EffectDef>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut all = library::effects();
        all.extend(audio_fx::effects());
        all
    })
}

pub fn find(id: &str) -> Option<&'static EffectDef> {
    registry().iter().find(|e| e.id == id)
}

pub fn infos() -> Vec<EffectInfo> {
    registry()
        .iter()
        .map(|e| EffectInfo {
            id: e.id,
            name: e.name,
            category: e.category,
            description: e.description,
            audio: e.audio,
            grid_only: e.grid_only,
            params: e.params.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;
    use crate::model::{Layout, MatrixWiring};

    /// Every effect must render every layout without panicking or producing
    /// NaNs, including the degenerate one-LED case.
    #[test]
    fn all_effects_render_all_layouts() {
        let layouts = [
            Layout::Strip { count: 1, reversed: false },
            Layout::Strip { count: 60, reversed: false },
            Layout::Matrix { width: 16, height: 8, wiring: MatrixWiring::default() },
            Layout::Matrix { width: 1, height: 1, wiring: MatrixWiring::default() },
            Layout::Cube {
                width: 4,
                height: 4,
                depth: 4,
                wiring: MatrixWiring::default(),
                layer_axis: Default::default(),
                layers_reversed: false,
                layers_serpentine: false,
            },
            Layout::Ring { count: 24, start_angle: 0.0, clockwise: true },
            Layout::Monitor {
                top: 10,
                right: 6,
                bottom: 10,
                left: 6,
                start: Default::default(),
                clockwise: true,
                offset: 0,
                bottom_gap: 0.2,
            },
        ];
        let mut audio = AudioFrame { level: 0.7, beat: 1.0, ..Default::default() };
        for (i, b) in audio.bands.iter_mut().enumerate() {
            *b = (i as f32 / 32.0).sin().abs();
        }

        let mut ids = std::collections::HashSet::new();
        for def in registry() {
            assert!(ids.insert(def.id), "duplicate effect id {}", def.id);
            let bag = ParamBag::build(&def.params, &Params::new());
            for layout in &layouts {
                let map = layout::build(layout);
                let mut fx = (def.make)();
                let mut out = vec![Rgb::BLACK; map.count];
                let mut t = 0.0;
                for _ in 0..90 {
                    let cx = Ctx { t, dt: 1.0 / 30.0, real_dt: 1.0 / 30.0, map: &map, p: &bag, audio: &audio };
                    fx.render(&cx, &mut out);
                    t += 1.0 / 30.0;
                }
                for c in &out {
                    assert!(
                        c.r.is_finite() && c.g.is_finite() && c.b.is_finite(),
                        "{} produced a non-finite colour on {}",
                        def.id,
                        map.kind
                    );
                }
            }
        }
        assert!(registry().len() >= 25);
    }

    #[test]
    fn most_effects_light_something() {
        let map = layout::build(&Layout::Matrix { width: 16, height: 16, wiring: MatrixWiring::default() });
        let audio = AudioFrame { level: 0.8, beat: 1.0, bass: 0.8, bands: [0.7; 32], ..Default::default() };
        for def in registry() {
            let bag = ParamBag::build(&def.params, &Params::new());
            let mut fx = (def.make)();
            let mut out = vec![Rgb::BLACK; map.count];
            let mut peak = 0.0f32;
            let mut t = 0.0;
            for _ in 0..240 {
                let cx = Ctx { t, dt: 1.0 / 60.0, real_dt: 1.0 / 60.0, map: &map, p: &bag, audio: &audio };
                fx.render(&cx, &mut out);
                t += 1.0 / 60.0;
                peak = out.iter().fold(peak, |m, c| m.max(c.peak()));
            }
            assert!(peak > 0.2, "{} never lit an LED (peak {peak})", def.id);
        }
    }
}
