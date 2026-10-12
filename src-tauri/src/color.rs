//! Colour maths shared by every renderer: float RGB, HSV, palettes and noise.

use std::ops::{Add, AddAssign, Mul};

/// A colour with float channels in `0.0..=1.0`. Values may temporarily exceed
/// that range while blending; they are clamped when converted to bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Rgb { r, g, b }
    }

    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Rgb::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// Parses `#rrggbb`, `rrggbb` or the short `#rgb` form.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let h = hex.trim().trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).ok()?;
        match h.len() {
            6 => Some(Rgb::from_u8((v >> 16) as u8, (v >> 8) as u8, v as u8)),
            3 => {
                let (r, g, b) = ((v >> 8) & 0xf, (v >> 4) & 0xf, v & 0xf);
                Some(Rgb::from_u8((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
            }
            _ => None,
        }
    }

    pub fn from_packed(v: i64) -> Self {
        Rgb::from_u8((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    pub fn to_packed(self) -> i64 {
        let [r, g, b] = self.to_u8();
        ((r as i64) << 16) | ((g as i64) << 8) | b as i64
    }

    pub fn to_u8(self) -> [u8; 3] {
        [to_byte(self.r), to_byte(self.g), to_byte(self.b)]
    }

    pub fn scale(self, f: f32) -> Self {
        Rgb::new(self.r * f, self.g * f, self.b * f)
    }

    pub fn lerp(self, other: Rgb, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Rgb::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    /// Per-channel maximum, useful for layering lights without washing out.
    pub fn max(self, other: Rgb) -> Self {
        Rgb::new(self.r.max(other.r), self.g.max(other.g), self.b.max(other.b))
    }

    pub fn clamped(self) -> Self {
        Rgb::new(self.r.clamp(0.0, 1.0), self.g.clamp(0.0, 1.0), self.b.clamp(0.0, 1.0))
    }

    pub fn luma(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    pub fn peak(self) -> f32 {
        self.r.max(self.g).max(self.b)
    }

    /// Pushes the colour away from (or towards) grey. `amount` 1.0 is a no-op.
    pub fn saturate(self, amount: f32) -> Self {
        let l = self.luma();
        Rgb::new(
            l + (self.r - l) * amount,
            l + (self.g - l) * amount,
            l + (self.b - l) * amount,
        )
        .clamped()
    }

    /// `h`, `s`, `v` all in `0..=1`; hue wraps.
    pub fn hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(1.0) * 6.0;
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);
        let i = h.floor();
        let f = h - i;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        match i as i32 % 6 {
            0 => Rgb::new(v, t, p),
            1 => Rgb::new(q, v, p),
            2 => Rgb::new(p, v, t),
            3 => Rgb::new(p, q, v),
            4 => Rgb::new(t, p, v),
            _ => Rgb::new(v, p, q),
        }
    }
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

impl Add for Rgb {
    type Output = Rgb;
    fn add(self, o: Rgb) -> Rgb {
        Rgb::new(self.r + o.r, self.g + o.g, self.b + o.b)
    }
}

impl AddAssign for Rgb {
    fn add_assign(&mut self, o: Rgb) {
        self.r += o.r;
        self.g += o.g;
        self.b += o.b;
    }
}

impl Mul<f32> for Rgb {
    type Output = Rgb;
    fn mul(self, f: f32) -> Rgb {
        self.scale(f)
    }
}

/// An ordered list of colour stops that can be sampled as a gradient.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub stops: Vec<Rgb>,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::new(vec![Rgb::WHITE])
    }
}

impl Palette {
    pub fn new(stops: Vec<Rgb>) -> Self {
        Palette { stops }
    }

    pub fn from_hex(list: &[&str]) -> Self {
        Palette::new(list.iter().filter_map(|h| Rgb::from_hex(h)).collect())
    }

    /// Samples a seamless loop: the last stop blends back into the first.
    pub fn sample(&self, t: f32) -> Rgb {
        let n = self.stops.len();
        match n {
            0 => Rgb::WHITE,
            1 => self.stops[0],
            _ => {
                let p = t.rem_euclid(1.0) * n as f32;
                let i = (p.floor() as usize).min(n - 1);
                self.stops[i].lerp(self.stops[(i + 1) % n], p - i as f32)
            }
        }
    }

    /// Samples a one-way gradient: `0` is the first stop and `1` the last.
    pub fn sample_clamped(&self, t: f32) -> Rgb {
        let n = self.stops.len();
        match n {
            0 => Rgb::WHITE,
            1 => self.stops[0],
            _ => {
                let p = t.clamp(0.0, 1.0) * (n - 1) as f32;
                let i = (p.floor() as usize).min(n - 2);
                self.stops[i].lerp(self.stops[i + 1], p - i as f32)
            }
        }
    }
}

/// Built-in palettes offered by the UI and available to scripts by name.
pub const PALETTES: &[(&str, &[&str])] = &[
    ("Rainbow", &["#ff0000", "#ff8a00", "#ffe600", "#3dff00", "#00ffd0", "#0066ff", "#8f00ff", "#ff00a8"]),
    ("Sunset", &["#ff5e3a", "#ff9f43", "#ffd166", "#ff4d8d", "#8e44ff", "#2b1a5e"]),
    ("Ocean", &["#001845", "#0353a4", "#00a6fb", "#48e5c2", "#0582ca"]),
    ("Forest", &["#0b3d0b", "#1e7b1e", "#5cb85c", "#c3e88d", "#2e8b57"]),
    ("Lava", &["#1a0000", "#8b0000", "#ff2a00", "#ff8c00", "#ffd000"]),
    ("Fire", &["#000000", "#5a0a00", "#d42a00", "#ff8a00", "#ffe680", "#ffffff"]),
    ("Ice", &["#03045e", "#0077b6", "#00b4d8", "#90e0ef", "#ffffff"]),
    ("Aurora", &["#001a2c", "#00c896", "#34e89e", "#3a7bd5", "#8e2de2", "#00d2ff"]),
    ("Cyberpunk", &["#ff00c8", "#7a00ff", "#00e5ff", "#fcee0a", "#ff2a6d"]),
    ("Candy", &["#ff9aa2", "#ffb7b2", "#ffdac1", "#e2f0cb", "#b5ead7", "#c7ceea"]),
    ("Party", &["#5500ab", "#84007c", "#b5004b", "#e5001b", "#e81700", "#b84700", "#ab7700", "#abab00", "#00ab55", "#0000ff"]),
    ("Synthwave", &["#2b0f54", "#ab1f65", "#ff4f69", "#ff8142", "#ffda45", "#3368dc"]),
    ("Mint", &["#0f2027", "#1cb5a0", "#a8ff78", "#78ffd6"]),
    ("Warm White", &["#ffb46b", "#ffd6a5", "#fff1dc"]),
    ("Matrix", &["#001a00", "#00b828", "#5dff7f", "#e6ffe6"]),
    ("Meter", &["#00ff3c", "#b6ff00", "#ffe600", "#ff7a00", "#ff0000"]),
    ("Monochrome", &["#000000", "#ffffff"]),
];

pub fn palette_by_name(name: &str) -> Option<Palette> {
    PALETTES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, stops)| Palette::from_hex(stops))
}

// ---------------------------------------------------------------------------
// Deterministic hashing and gradient noise. Effects use these instead of an
// RNG so the same LED keeps its "random" character from frame to frame.
// ---------------------------------------------------------------------------

pub fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Stable pseudo-random value in `0..1` for an integer seed.
pub fn hash01(x: u32) -> f32 {
    (hash_u32(x) >> 8) as f32 / 16_777_216.0
}

/// Stable pseudo-random value in `0..1` for a pair of seeds.
pub fn hash01_2(a: u32, b: u32) -> f32 {
    hash01(a ^ hash_u32(b.wrapping_add(0x9e37_79b9)))
}

fn hash3(x: i32, y: i32, z: i32) -> u32 {
    hash_u32(
        (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ (z as u32).wrapping_mul(0xcb1a_b31f),
    )
}

fn grad(h: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = h & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Perlin-style gradient noise, roughly in `-1..1`.
pub fn noise3(x: f32, y: f32, z: f32) -> f32 {
    let (xf, yf, zf) = (x.floor(), y.floor(), z.floor());
    let (xi, yi, zi) = (xf as i32, yf as i32, zf as i32);
    let (dx, dy, dz) = (x - xf, y - yf, z - zf);
    let (u, v, w) = (fade(dx), fade(dy), fade(dz));

    let g = |ox: i32, oy: i32, oz: i32| {
        grad(
            hash3(xi + ox, yi + oy, zi + oz),
            dx - ox as f32,
            dy - oy as f32,
            dz - oz as f32,
        )
    };

    let x00 = mix(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = mix(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = mix(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = mix(g(0, 1, 1), g(1, 1, 1), u);
    mix(mix(x00, x10, v), mix(x01, x11, v), w)
}

/// Noise remapped to `0..1` with a little extra contrast.
pub fn noise01(x: f32, y: f32, z: f32) -> f32 {
    (noise3(x, y, z) * 0.6 + 0.5).clamp(0.0, 1.0)
}

/// Fractal noise in `0..1` built from `octaves` layers.
pub fn fbm01(x: f32, y: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for _ in 0..octaves.max(1) {
        sum += noise3(x * freq, y * freq, z * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    (sum / norm * 0.7 + 0.5).clamp(0.0, 1.0)
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if (e1 - e0).abs() < f32::EPSILON {
        return if x < e0 { 0.0 } else { 1.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Shortest distance between two points on a loop of length 1.
pub fn wrap_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Rgb::from_hex("#ff8000").unwrap().to_u8(), [255, 128, 0]);
        assert_eq!(Rgb::from_hex("0f0").unwrap().to_u8(), [0, 255, 0]);
        assert!(Rgb::from_hex("nope").is_none());
        assert_eq!(Rgb::from_packed(0x12_34_56).to_packed(), 0x12_34_56);
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(Rgb::hsv(0.0, 1.0, 1.0).to_u8(), [255, 0, 0]);
        assert_eq!(Rgb::hsv(1.0 / 3.0, 1.0, 1.0).to_u8(), [0, 255, 0]);
        assert_eq!(Rgb::hsv(2.0 / 3.0, 1.0, 1.0).to_u8(), [0, 0, 255]);
        assert_eq!(Rgb::hsv(0.3, 0.0, 1.0).to_u8(), [255, 255, 255]);
    }

    #[test]
    fn palette_sampling() {
        let p = Palette::from_hex(&["#000000", "#ffffff"]);
        assert_eq!(p.sample_clamped(0.0).to_u8(), [0, 0, 0]);
        assert_eq!(p.sample_clamped(1.0).to_u8(), [255, 255, 255]);
        assert_eq!(p.sample_clamped(0.5).to_u8(), [128, 128, 128]);
        // The looping form returns to the first stop.
        assert_eq!(p.sample(1.0).to_u8(), p.sample(0.0).to_u8());
    }

    #[test]
    fn noise_stays_in_range() {
        for i in 0..2000 {
            let f = i as f32 * 0.137;
            let n = noise3(f, f * 0.71, f * 1.9);
            assert!((-1.2..=1.2).contains(&n), "noise out of range: {n}");
            let m = noise01(f, f * 0.3, 0.5);
            assert!((0.0..=1.0).contains(&m));
        }
    }
}
