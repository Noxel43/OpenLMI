//! Ambient lighting: capture the screen, shrink it to a small grid, and work
//! out which part of the picture each LED should mirror.

use crate::color::Rgb;
use crate::layout::{Edge, LayoutMap};
use crate::model::{AmbientMapping, AmbientSettings};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The screen is reduced to this many cells before any LED looks at it.
pub const GRID_W: usize = 96;
pub const GRID_H: usize = 54;

pub struct ScreenGrid {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
    /// Width / height of the captured display.
    pub aspect: f32,
    /// Increments with every capture.
    pub seq: u64,
    /// Picture area inside any black bars: `[left, top, right, bottom]`, `0..1`.
    pub content: [f32; 4],
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

pub fn list_monitors() -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = xcap::Monitor::all()
        .unwrap_or_default()
        .iter()
        .filter_map(|m| {
            Some(MonitorInfo {
                id: m.name().ok()?,
                name: m.friendly_name().unwrap_or_else(|_| "Display".into()),
                width: m.width().ok()?,
                height: m.height().ok()?,
                primary: m.is_primary().unwrap_or(false),
            })
        })
        .collect();
    list.sort_by(|a, b| b.primary.cmp(&a.primary).then(a.id.cmp(&b.id)));
    list
}

struct Shared {
    grids: Mutex<HashMap<String, Arc<ScreenGrid>>>,
    wanted: Mutex<HashMap<String, Instant>>,
    fps: AtomicU32,
    stop: AtomicBool,
    error: Mutex<Option<String>>,
}

/// Handle to the capture thread. Capturing only happens while something keeps
/// asking for a display.
#[derive(Clone)]
pub struct ScreenCapture {
    shared: Arc<Shared>,
}

impl ScreenCapture {
    pub fn start() -> Self {
        let shared = Arc::new(Shared {
            grids: Mutex::new(HashMap::new()),
            wanted: Mutex::new(HashMap::new()),
            fps: AtomicU32::new(30),
            stop: AtomicBool::new(false),
            error: Mutex::new(None),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("openlmi-capture".into())
            .spawn(move || capture_thread(worker))
            .ok();
        ScreenCapture { shared }
    }

    /// `monitor` is a display id from `list_monitors`, or empty for the primary.
    pub fn request(&self, monitor: &str) {
        self.shared.wanted.lock().insert(monitor.to_string(), Instant::now());
    }

    pub fn grid(&self, monitor: &str) -> Option<Arc<ScreenGrid>> {
        self.shared.grids.lock().get(monitor).cloned()
    }

    pub fn set_fps(&self, fps: u32) {
        self.shared.fps.store(fps.clamp(5, 120), Ordering::Relaxed);
    }

    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().clone()
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn capture_thread(shared: Arc<Shared>) {
    let mut monitors: Vec<xcap::Monitor> = Vec::new();
    let mut listed_at = Instant::now();
    let mut seq = 0u64;

    while !shared.stop.load(Ordering::Relaxed) {
        let started = Instant::now();
        let wanted: Vec<String> = {
            let mut w = shared.wanted.lock();
            w.retain(|_, at| at.elapsed() < Duration::from_secs(2));
            w.keys().cloned().collect()
        };
        if wanted.is_empty() {
            shared.grids.lock().clear();
            monitors.clear();
            std::thread::sleep(Duration::from_millis(150));
            continue;
        }

        // Displays come and go; re-enumerate now and then.
        if monitors.is_empty() || listed_at.elapsed() > Duration::from_secs(5) {
            monitors = xcap::Monitor::all().unwrap_or_default();
            listed_at = Instant::now();
        }

        for name in &wanted {
            let target = monitors
                .iter()
                .find(|m| !name.is_empty() && m.name().ok().as_deref() == Some(name.as_str()))
                .or_else(|| monitors.iter().find(|m| m.is_primary().unwrap_or(false)))
                .or_else(|| monitors.first());
            let Some(monitor) = target else {
                *shared.error.lock() = Some("No display found to capture".into());
                continue;
            };
            match monitor.capture_image() {
                Ok(image) => {
                    seq += 1;
                    let (w, h) = (image.width() as usize, image.height() as usize);
                    let grid = downsample(image.as_raw(), w, h, seq);
                    shared.grids.lock().insert(name.clone(), Arc::new(grid));
                    *shared.error.lock() = None;
                }
                Err(e) => {
                    *shared.error.lock() = Some(format!("Screen capture failed: {e}"));
                    monitors.clear();
                }
            }
        }

        let period = Duration::from_secs_f32(1.0 / shared.fps.load(Ordering::Relaxed).max(1) as f32);
        std::thread::sleep(period.saturating_sub(started.elapsed()).max(Duration::from_millis(2)));
    }
}

/// Box-averages an RGBA image down to the working grid.
pub fn downsample(rgba: &[u8], w: usize, h: usize, seq: u64) -> ScreenGrid {
    let mut px = vec![Rgb::BLACK; GRID_W * GRID_H];
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return ScreenGrid { w: GRID_W, h: GRID_H, px, aspect: 16.0 / 9.0, seq, content: [0.0, 0.0, 1.0, 1.0] };
    }
    // A handful of samples per cell is plenty and keeps 4K captures cheap.
    let step_x = (w / GRID_W / 4).max(1);
    let step_y = (h / GRID_H / 4).max(1);
    for gy in 0..GRID_H {
        let (y0, y1) = (gy * h / GRID_H, ((gy + 1) * h / GRID_H).max(gy * h / GRID_H + 1).min(h));
        for gx in 0..GRID_W {
            let (x0, x1) = (gx * w / GRID_W, ((gx + 1) * w / GRID_W).max(gx * w / GRID_W + 1).min(w));
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            let mut y = y0;
            while y < y1 {
                let row = y * w * 4;
                let mut x = x0;
                while x < x1 {
                    let i = row + x * 4;
                    r += rgba[i] as u32;
                    g += rgba[i + 1] as u32;
                    b += rgba[i + 2] as u32;
                    n += 1;
                    x += step_x;
                }
                y += step_y;
            }
            let k = 1.0 / (n.max(1) as f32 * 255.0);
            px[gy * GRID_W + gx] = Rgb::new(r as f32 * k, g as f32 * k, b as f32 * k);
        }
    }
    let content = detect_content(&px);
    ScreenGrid { w: GRID_W, h: GRID_H, px, aspect: w as f32 / h as f32, seq, content }
}

/// Finds letterbox / pillarbox bars. Bars are assumed symmetric, which avoids
/// mistaking a dark sky at the top of a scene for a bar.
fn detect_content(px: &[Rgb]) -> [f32; 4] {
    const DARK: f32 = 0.035;
    let row_dark = |y: usize| (0..GRID_W).all(|x| px[y * GRID_W + x].peak() < DARK);
    let col_dark = |x: usize| (0..GRID_H).all(|y| px[y * GRID_W + x].peak() < DARK);

    let max_rows = GRID_H / 3;
    let top = (0..max_rows).take_while(|&y| row_dark(y)).count();
    let bottom = (0..max_rows).take_while(|&y| row_dark(GRID_H - 1 - y)).count();
    let max_cols = GRID_W / 3;
    let left = (0..max_cols).take_while(|&x| col_dark(x)).count();
    let right = (0..max_cols).take_while(|&x| col_dark(GRID_W - 1 - x)).count();

    // A fully dark screen is not "all bars".
    let v = if top == max_rows || bottom == max_rows { 0 } else { top.min(bottom) };
    let hz = if left == max_cols || right == max_cols { 0 } else { left.min(right) };
    [
        hz as f32 / GRID_W as f32,
        v as f32 / GRID_H as f32,
        1.0 - hz as f32 / GRID_W as f32,
        1.0 - v as f32 / GRID_H as f32,
    ]
}

/// The screen area each LED samples, as `[x0, y0, x1, y1]` in `0..1`.
pub fn sample_rects(map: &LayoutMap, s: &AmbientSettings, aspect: f32) -> Vec<[f32; 4]> {
    if s.mapping == AmbientMapping::Average {
        return vec![[0.0, 0.0, 1.0, 1.0]; map.count];
    }
    let depth_y = s.depth.clamp(0.01, 0.5);
    // Keep the sampled band the same physical thickness on every side.
    let depth_x = (depth_y / aspect.max(0.1)).clamp(0.005, 0.5);

    (0..map.count)
        .map(|i| {
            if let Some(e) = map.edges.get(i) {
                return match e.edge {
                    Edge::Top => [e.a, 0.0, e.b, depth_y],
                    Edge::Bottom => [e.a, 1.0 - depth_y, e.b, 1.0],
                    Edge::Left => [0.0, e.a, depth_x, e.b],
                    Edge::Right => [1.0 - depth_x, e.a, 1.0, e.b],
                };
            }
            let p = map.pos[i];
            if map.is_grid {
                let (hw, hh) = (0.5 / map.dims[0] as f32, 0.5 / map.dims[1] as f32);
                [p[0] - hw, p[1] - hh, p[0] + hw, p[1] + hh]
            } else if map.kind == "strip" {
                // A plain strip mirrors vertical slices of the picture.
                let hw = 0.5 / map.count as f32;
                [p[0] - hw, 0.0, p[0] + hw, 1.0]
            } else {
                [p[0] - depth_x, p[1] - depth_y, p[0] + depth_x, p[1] + depth_y]
            }
        })
        .map(|[x0, y0, x1, y1]| [x0.clamp(0.0, 1.0), y0.clamp(0.0, 1.0), x1.clamp(0.0, 1.0), y1.clamp(0.0, 1.0)])
        .collect()
}

/// Per-device ambient state: smoothed colours and cached sample areas.
#[derive(Default)]
pub struct AmbientState {
    target: Vec<Rgb>,
    current: Vec<Rgb>,
    cells: Vec<[usize; 4]>,
    cells_key: Option<(f32, AmbientMapping, [i32; 4], usize)>,
    content: [f32; 4],
    last_seq: u64,
    primed: bool,
}

impl AmbientState {
    pub fn render(&mut self, grid: Option<&ScreenGrid>, map: &LayoutMap, s: &AmbientSettings, dt: f32, out: &mut [Rgb]) {
        let n = out.len();
        if self.current.len() != n {
            self.current = vec![Rgb::BLACK; n];
            self.target = vec![Rgb::BLACK; n];
            self.cells_key = None;
            self.primed = false;
            self.content = [0.0, 0.0, 1.0, 1.0];
        }

        if let Some(g) = grid.filter(|g| g.seq != self.last_seq) {
            self.last_seq = g.seq;
            // Ease the bar estimate so a single dark frame can't make the
            // sample areas jump.
            let want = if s.black_bars { g.content } else { [0.0, 0.0, 1.0, 1.0] };
            for (have, want) in self.content.iter_mut().zip(want) {
                *have += (want - *have) * if self.primed { 0.12 } else { 1.0 };
            }
            self.refresh_cells(g, map, s);
            for (t, c) in self.target.iter_mut().zip(&self.cells) {
                let [x0, y0, x1, y1] = *c;
                let mut sum = Rgb::BLACK;
                for y in y0..y1 {
                    for px in &g.px[y * g.w + x0..y * g.w + x1] {
                        sum += *px;
                    }
                }
                let avg = sum.scale(1.0 / ((x1 - x0) * (y1 - y0)).max(1) as f32);
                let lit = avg.saturate(s.saturation).scale(s.gain).clamped();
                *t = lit.max(Rgb::new(s.min_brightness, s.min_brightness, s.min_brightness));
            }
            if !self.primed {
                self.current.copy_from_slice(&self.target);
                self.primed = true;
            }
        }

        // Frame-rate independent smoothing between captures.
        let keep = s.smoothing.clamp(0.0, 0.98).powf(dt * 60.0);
        for ((cur, target), o) in self.current.iter_mut().zip(&self.target).zip(out.iter_mut()) {
            *cur = target.lerp(*cur, keep);
            *o = *cur;
        }
    }

    fn refresh_cells(&mut self, g: &ScreenGrid, map: &LayoutMap, s: &AmbientSettings) {
        let quant = self.content.map(|v| (v * 1000.0) as i32);
        let key = (s.depth, s.mapping, quant, map.count);
        if self.cells_key == Some(key) && self.cells.len() == map.count {
            return;
        }
        let [cl, ct, cr, cb] = self.content;
        self.cells = sample_rects(map, s, g.aspect)
            .into_iter()
            .map(|[x0, y0, x1, y1]| {
                let to_x = |v: f32| (cl + v * (cr - cl)) * g.w as f32;
                let to_y = |v: f32| (ct + v * (cb - ct)) * g.h as f32;
                let gx0 = (to_x(x0).floor() as usize).min(g.w - 1);
                let gy0 = (to_y(y0).floor() as usize).min(g.h - 1);
                let gx1 = (to_x(x1).ceil() as usize).clamp(gx0 + 1, g.w);
                let gy1 = (to_y(y1).ceil() as usize).clamp(gy0 + 1, g.h);
                [gx0, gy0, gx1, gy1]
            })
            .collect();
        self.cells_key = Some(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout;
    use crate::model::{Corner, Layout};

    /// A synthetic screen: red left third, green middle, blue right third,
    /// with a white stripe along the very top.
    fn test_screen(w: usize, h: usize) -> Vec<u8> {
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let c = if y < h / 10 {
                    [255, 255, 255]
                } else if x < w / 3 {
                    [255, 0, 0]
                } else if x < 2 * w / 3 {
                    [0, 255, 0]
                } else {
                    [0, 0, 255]
                };
                rgba[i..i + 3].copy_from_slice(&c);
                rgba[i + 3] = 255;
            }
        }
        rgba
    }

    fn monitor_map() -> LayoutMap {
        layout::build(&Layout::Monitor {
            top: 9,
            right: 5,
            bottom: 9,
            left: 5,
            start: Corner::BottomLeft,
            clockwise: true,
            offset: 0,
            bottom_gap: 0.0,
        })
    }

    #[test]
    fn leds_pick_up_the_colour_behind_them() {
        let grid = downsample(&test_screen(1920, 1080), 1920, 1080, 1);
        assert!((grid.aspect - 16.0 / 9.0).abs() < 0.01);
        let map = monitor_map();
        let settings = AmbientSettings { smoothing: 0.0, saturation: 1.0, depth: 0.08, ..Default::default() };
        let mut state = AmbientState::default();
        let mut out = vec![Rgb::BLACK; map.count];
        state.render(Some(&grid), &map, &settings, 0.016, &mut out);

        // Wiring: 5 up the left, 9 across the top, 5 down the right, 9 along the bottom.
        let left_mid = out[2].to_u8();
        assert!(left_mid[0] > 200 && left_mid[1] < 40 && left_mid[2] < 40, "left side should be red: {left_mid:?}");
        let top_mid = out[5 + 4].to_u8();
        assert!(top_mid.iter().all(|c| *c > 200), "top edge should be white: {top_mid:?}");
        let right_mid = out[5 + 9 + 2].to_u8();
        assert!(right_mid[2] > 200 && right_mid[0] < 40, "right side should be blue: {right_mid:?}");
        // The bottom runs right-to-left: blue first, green in the middle, red last.
        let bottom = &out[19..28];
        assert!(bottom[0].b > 0.8, "bottom-right should be blue");
        assert!(bottom[4].g > 0.8, "bottom-middle should be green");
        assert!(bottom[8].r > 0.8, "bottom-left should be red");
    }

    #[test]
    fn letterbox_bars_are_skipped() {
        let (w, h) = (1920usize, 1080usize);
        let mut rgba = vec![0u8; w * h * 4];
        // A bright picture with 12% black bars above and below.
        for y in h * 12 / 100..h * 88 / 100 {
            for x in 0..w {
                let i = (y * w + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[200, 150, 40, 255]);
            }
        }
        let grid = downsample(&rgba, w, h, 1);
        assert!(grid.content[1] > 0.09 && grid.content[1] < 0.14, "top bar: {:?}", grid.content);
        assert!(grid.content[3] > 0.86 && grid.content[3] < 0.91);
        assert_eq!(grid.content[0], 0.0);

        let map = monitor_map();
        let mut out = vec![Rgb::BLACK; map.count];
        let on = AmbientSettings { smoothing: 0.0, depth: 0.06, ..Default::default() };
        AmbientState::default().render(Some(&grid), &map, &on, 0.016, &mut out);
        assert!(out[5 + 4].peak() > 0.5, "with bar detection the top LEDs see the picture");

        let off = AmbientSettings { black_bars: false, ..on };
        AmbientState::default().render(Some(&grid), &map, &off, 0.016, &mut out);
        assert!(out[5 + 4].peak() < 0.05, "without it they see the bar");
    }

    #[test]
    fn a_black_screen_is_not_all_bars() {
        let grid = downsample(&vec![0u8; 640 * 360 * 4], 640, 360, 1);
        assert_eq!(grid.content, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn smoothing_eases_towards_the_target() {
        let bright = downsample(&vec![255u8; 320 * 180 * 4], 320, 180, 1);
        let dark = downsample(&vec![0u8; 320 * 180 * 4], 320, 180, 2);
        let map = monitor_map();
        let s = AmbientSettings { smoothing: 0.8, black_bars: false, saturation: 1.0, ..Default::default() };
        let mut state = AmbientState::default();
        let mut out = vec![Rgb::BLACK; map.count];
        state.render(Some(&bright), &map, &s, 0.016, &mut out);
        assert!(out[0].r > 0.95, "the first frame snaps straight to the picture");
        state.render(Some(&dark), &map, &s, 0.016, &mut out);
        assert!(out[0].r > 0.5 && out[0].r < 0.95, "then it eases: {}", out[0].r);
        for _ in 0..200 {
            state.render(None, &map, &s, 0.016, &mut out);
        }
        assert!(out[0].r < 0.01);
    }
}
