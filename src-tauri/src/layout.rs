//! Turns a `Layout` description (how the LEDs are physically arranged and
//! wired) into per-LED coordinates the renderers can work with.

use crate::model::{Axis, Corner, Direction, Layout, MatrixWiring, MAX_LEDS};
use serde::Serialize;

/// One side of a monitor, as seen from the front.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

/// The stretch of a monitor edge a backlight LED covers. `a..b` runs along
/// the edge: x for top/bottom, y for left/right, both `0..1`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct EdgeSpan {
    pub edge: Edge,
    pub a: f32,
    pub b: f32,
}

#[derive(Clone, Debug)]
pub struct LayoutMap {
    pub kind: &'static str,
    pub count: usize,
    /// Grid size `[width, height, depth]`; non-grid layouts use `[count, 1, 1]`.
    pub dims: [usize; 3],
    /// Normalised position: x right, y down, z into the screen.
    pub pos: Vec<[f32; 3]>,
    /// Position along the "main run" of the layout, `0..1`.
    pub u: Vec<f32>,
    /// Height above the floor, `0` (bottom) to `1` (top).
    pub up: Vec<f32>,
    /// Integer cell for grid layouts.
    pub grid: Vec<[u16; 3]>,
    /// True when `u` wraps around (rings, monitor loops).
    pub looped: bool,
    /// True when LEDs sit on a regular grid with meaningful rows and columns.
    pub is_grid: bool,
    /// Monitor layouts only: the edge segment behind each LED.
    pub edges: Vec<EdgeSpan>,
    index_of: Vec<u32>,
}

/// What the UI needs to draw a layout.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPreview {
    pub kind: &'static str,
    pub count: usize,
    pub dims: [usize; 3],
    pub pos: Vec<[f32; 3]>,
    pub grid: Vec<[u16; 3]>,
    pub looped: bool,
    pub is_grid: bool,
}

impl LayoutMap {
    pub fn preview(&self) -> LayoutPreview {
        LayoutPreview {
            kind: self.kind,
            count: self.count,
            dims: self.dims,
            pos: self.pos.clone(),
            grid: self.grid.clone(),
            looped: self.looped,
            is_grid: self.is_grid,
        }
    }

    /// LED index at a grid cell, if one exists there.
    pub fn index_at(&self, x: i64, y: i64, z: i64) -> Option<usize> {
        let [w, h, d] = self.dims;
        if x < 0 || y < 0 || z < 0 || x >= w as i64 || y >= h as i64 || z >= d as i64 {
            return None;
        }
        let i = *self.index_of.get(x as usize + y as usize * w + z as usize * w * h)?;
        (i != u32::MAX).then_some(i as usize)
    }

    /// Coordinate for effects with a natural "up": flames, meters, bouncing
    /// balls. Anything with real height uses it; a bare strip (or a one-row
    /// matrix) has none, so those run along the strip instead.
    pub fn along(&self, i: usize) -> f32 {
        let flat = match self.kind {
            "strip" => true,
            "matrix" | "cube" => self.dims[1] <= 1,
            _ => false,
        };
        if flat {
            self.u[i]
        } else {
            self.up[i]
        }
    }

    /// A stable lane number for effects that drop things down columns.
    pub fn lane(&self, i: usize) -> u32 {
        if self.is_grid && self.dims[1] > 1 {
            let g = self.grid[i];
            g[0] as u32 + g[2] as u32 * self.dims[0] as u32
        } else {
            0
        }
    }

    pub fn lane_count(&self) -> u32 {
        if self.is_grid && self.dims[1] > 1 {
            (self.dims[0] * self.dims[2]) as u32
        } else {
            1
        }
    }
}

fn clamp_count(n: u32) -> usize {
    n.clamp(1, MAX_LEDS) as usize
}

/// Cell of the `i`-th LED in a `w` x `h` grid wired as described.
fn wiring_cell(i: usize, w: usize, h: usize, wiring: &MatrixWiring) -> (usize, usize) {
    let (line_len, lines) = match wiring.direction {
        Direction::Horizontal => (w, h),
        Direction::Vertical => (h, w),
    };
    let line = (i / line_len).min(lines - 1);
    let mut k = i % line_len;
    if wiring.serpentine && line % 2 == 1 {
        k = line_len - 1 - k;
    }
    let (mut col, mut row) = match wiring.direction {
        Direction::Horizontal => (k, line),
        Direction::Vertical => (line, k),
    };
    if wiring.start.is_right() {
        col = w - 1 - col;
    }
    if wiring.start.is_bottom() {
        row = h - 1 - row;
    }
    (col, row)
}

pub fn build(layout: &Layout) -> LayoutMap {
    let mut m = LayoutMap {
        kind: "strip",
        count: 0,
        dims: [1, 1, 1],
        pos: vec![],
        u: vec![],
        up: vec![],
        grid: vec![],
        looped: false,
        is_grid: false,
        edges: vec![],
        index_of: vec![],
    };

    match layout {
        Layout::Strip { count, reversed } => {
            let n = clamp_count(*count);
            m.kind = "strip";
            m.dims = [n, 1, 1];
            for i in 0..n {
                let k = if *reversed { n - 1 - i } else { i };
                let u = if n > 1 { k as f32 / (n - 1) as f32 } else { 0.0 };
                m.pos.push([(k as f32 + 0.5) / n as f32, 0.5, 0.5]);
                m.u.push(u);
                m.up.push(u);
                m.grid.push([k as u16, 0, 0]);
            }
        }

        Layout::Matrix { width, height, wiring } => {
            let (w, h) = grid_dims2(*width, *height);
            m.kind = "matrix";
            m.dims = [w, h, 1];
            m.is_grid = true;
            for i in 0..w * h {
                let (col, row) = wiring_cell(i, w, h, wiring);
                let x = (col as f32 + 0.5) / w as f32;
                let y = (row as f32 + 0.5) / h as f32;
                m.pos.push([x, y, 0.5]);
                m.u.push(x);
                m.up.push(1.0 - y);
                m.grid.push([col as u16, row as u16, 0]);
            }
        }

        Layout::Cube { width, height, depth, wiring, layer_axis, layers_reversed, layers_serpentine } => {
            let (w, h, d) = grid_dims3(*width, *height, *depth);
            m.kind = "cube";
            m.dims = [w, h, d];
            m.is_grid = true;
            // Each layer is a 2D grid (la x lb) and there are `layers` of them.
            let (la, lb, layers) = match layer_axis {
                Axis::Z => (w, h, d),
                Axis::Y => (w, d, h),
                Axis::X => (d, h, w),
            };
            let per = la * lb;
            for i in 0..per * layers {
                let mut layer = i / per;
                let mut k = i % per;
                if *layers_serpentine && layer % 2 == 1 {
                    k = per - 1 - k;
                }
                if *layers_reversed {
                    layer = layers - 1 - layer;
                }
                let (a, b) = wiring_cell(k, la, lb, wiring);
                let (gx, gy, gz) = match layer_axis {
                    Axis::Z => (a, b, layer),
                    Axis::Y => (a, layer, b),
                    Axis::X => (layer, b, a),
                };
                let x = (gx as f32 + 0.5) / w as f32;
                let y = (gy as f32 + 0.5) / h as f32;
                let z = (gz as f32 + 0.5) / d as f32;
                m.pos.push([x, y, z]);
                m.u.push(x);
                m.up.push(1.0 - y);
                m.grid.push([gx as u16, gy as u16, gz as u16]);
            }
        }

        Layout::Ring { count, start_angle, clockwise } => {
            let n = clamp_count(*count);
            m.kind = "ring";
            m.dims = [n, 1, 1];
            m.looped = true;
            for i in 0..n {
                let turn = i as f32 / n as f32;
                let deg = start_angle + if *clockwise { turn } else { -turn } * 360.0;
                let a = deg.to_radians();
                let (x, y) = (0.5 + 0.5 * a.sin(), 0.5 - 0.5 * a.cos());
                m.pos.push([x, y, 0.5]);
                m.u.push(turn);
                m.up.push(1.0 - y);
                m.grid.push([i as u16, 0, 0]);
            }
        }

        Layout::Monitor { top, right, bottom, left, start, clockwise, offset, bottom_gap } => {
            let loop_ = monitor_loop(*top, *right, *bottom, *left, *start, *clockwise, *offset, *bottom_gap);
            let n = loop_.len();
            m.kind = "monitor";
            m.dims = [n, 1, 1];
            m.looped = true;
            for (i, (p, span)) in loop_.into_iter().enumerate() {
                m.pos.push([p[0], p[1], 0.5]);
                m.u.push(i as f32 / n as f32);
                m.up.push(1.0 - p[1]);
                m.grid.push([i as u16, 0, 0]);
                m.edges.push(span);
            }
        }

        Layout::Custom { points } => {
            let n = points.len().clamp(1, MAX_LEDS as usize);
            m.kind = "custom";
            m.dims = [n, 1, 1];
            for i in 0..n {
                let p = points.get(i).copied().unwrap_or([0.5, 0.5]);
                let (x, y) = (p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0));
                m.pos.push([x, y, 0.5]);
                m.u.push(if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 });
                m.up.push(1.0 - y);
                m.grid.push([i as u16, 0, 0]);
            }
        }
    }

    m.count = m.pos.len();
    let [w, h, d] = m.dims;
    m.index_of = vec![u32::MAX; w * h * d];
    for (i, g) in m.grid.iter().enumerate() {
        let slot = g[0] as usize + g[1] as usize * w + g[2] as usize * w * h;
        if let Some(s) = m.index_of.get_mut(slot) {
            *s = i as u32;
        }
    }
    m
}

fn grid_dims2(w: u32, h: u32) -> (usize, usize) {
    let w = w.clamp(1, 1024) as usize;
    let h = h.clamp(1, 1024) as usize;
    if w * h > MAX_LEDS as usize {
        (w, (MAX_LEDS as usize / w).max(1))
    } else {
        (w, h)
    }
}

fn grid_dims3(w: u32, h: u32, d: u32) -> (usize, usize, usize) {
    let w = w.clamp(1, 64) as usize;
    let h = h.clamp(1, 64) as usize;
    let d = d.clamp(1, 64) as usize;
    if w * h * d > MAX_LEDS as usize {
        (w, h, (MAX_LEDS as usize / (w * h)).max(1))
    } else {
        (w, h, d)
    }
}

/// Positions around a monitor in wiring order, with the edge span each LED
/// lights. Coordinates are the front view: x right, y down.
#[allow(clippy::too_many_arguments)]
fn monitor_loop(
    top: u32,
    right: u32,
    bottom: u32,
    left: u32,
    start: Corner,
    clockwise: bool,
    offset: i32,
    bottom_gap: f32,
) -> Vec<([f32; 2], EdgeSpan)> {
    let cap = |n: u32| n.min(MAX_LEDS / 4) as usize;
    let (top, right, bottom, left) = (cap(top), cap(right), cap(bottom), cap(left));
    let gap = bottom_gap.clamp(0.0, 0.9);

    // Clockwise loop starting at the top-left corner.
    let mut cw: Vec<([f32; 2], EdgeSpan)> = Vec::with_capacity(top + right + bottom + left);

    for j in 0..top {
        let (a, b) = (j as f32 / top as f32, (j + 1) as f32 / top as f32);
        cw.push(([(a + b) / 2.0, 0.0], EdgeSpan { edge: Edge::Top, a, b }));
    }
    for j in 0..right {
        let (a, b) = (j as f32 / right as f32, (j + 1) as f32 / right as f32);
        cw.push(([1.0, (a + b) / 2.0], EdgeSpan { edge: Edge::Right, a, b }));
    }
    // The bottom runs right-to-left and may be split around a stand.
    let half = 0.5 - gap / 2.0;
    let (n_right, n_left) = if gap > 0.0 { (bottom.div_ceil(2), bottom / 2) } else { (bottom, 0) };
    let seg = if gap > 0.0 { half } else { 1.0 };
    for j in 0..n_right {
        let hi = 1.0 - j as f32 / n_right as f32 * seg;
        let lo = 1.0 - (j + 1) as f32 / n_right as f32 * seg;
        cw.push(([(lo + hi) / 2.0, 1.0], EdgeSpan { edge: Edge::Bottom, a: lo, b: hi }));
    }
    for j in 0..n_left {
        let hi = half - j as f32 / n_left as f32 * half;
        let lo = half - (j + 1) as f32 / n_left as f32 * half;
        cw.push(([(lo + hi) / 2.0, 1.0], EdgeSpan { edge: Edge::Bottom, a: lo, b: hi }));
    }
    for j in 0..left {
        let hi = 1.0 - j as f32 / left as f32;
        let lo = 1.0 - (j + 1) as f32 / left as f32;
        cw.push(([0.0, (lo + hi) / 2.0], EdgeSpan { edge: Edge::Left, a: lo, b: hi }));
    }

    if cw.is_empty() {
        // Nothing configured yet: keep one LED so the rest of the app is happy.
        return vec![([0.5, 0.0], EdgeSpan { edge: Edge::Top, a: 0.0, b: 1.0 })];
    }

    // Rotate so the loop begins at the chosen corner.
    let rot = match start {
        Corner::TopLeft => 0,
        Corner::TopRight => top,
        Corner::BottomRight => top + right,
        Corner::BottomLeft => top + right + bottom,
    };
    let n = cw.len();
    cw.rotate_left(rot % n);
    if !clockwise {
        cw.reverse();
    }
    let shift = (offset as i64).rem_euclid(n as i64) as usize;
    cw.rotate_left(shift);
    cw
}

pub fn led_count(layout: &Layout) -> usize {
    build(layout).count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(m: &LayoutMap) -> Vec<(u16, u16)> {
        m.grid.iter().map(|g| (g[0], g[1])).collect()
    }

    #[test]
    fn matrix_progressive_rows() {
        let m = build(&Layout::Matrix {
            width: 3,
            height: 2,
            wiring: MatrixWiring { start: Corner::TopLeft, direction: Direction::Horizontal, serpentine: false },
        });
        assert_eq!(cells(&m), vec![(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]);
    }

    #[test]
    fn matrix_serpentine_rows() {
        let m = build(&Layout::Matrix {
            width: 3,
            height: 2,
            wiring: MatrixWiring { start: Corner::TopLeft, direction: Direction::Horizontal, serpentine: true },
        });
        assert_eq!(cells(&m), vec![(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    }

    #[test]
    fn matrix_serpentine_columns_from_bottom_right() {
        let m = build(&Layout::Matrix {
            width: 2,
            height: 3,
            wiring: MatrixWiring { start: Corner::BottomRight, direction: Direction::Vertical, serpentine: true },
        });
        // Up the right column, then down the left one.
        assert_eq!(cells(&m), vec![(1, 2), (1, 1), (1, 0), (0, 0), (0, 1), (0, 2)]);
    }

    #[test]
    fn every_grid_cell_is_used_once() {
        for &serp in &[false, true] {
            for dir in [Direction::Horizontal, Direction::Vertical] {
                for start in [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight] {
                    let m = build(&Layout::Matrix {
                        width: 5,
                        height: 4,
                        wiring: MatrixWiring { start, direction: dir, serpentine: serp },
                    });
                    let mut seen = [false; 20];
                    for (x, y) in cells(&m) {
                        let i = x as usize + y as usize * 5;
                        assert!(!seen[i], "cell visited twice");
                        seen[i] = true;
                    }
                    assert!(seen.iter().all(|s| *s));
                    // And the reverse lookup agrees.
                    for i in 0..m.count {
                        let g = m.grid[i];
                        assert_eq!(m.index_at(g[0] as i64, g[1] as i64, 0), Some(i));
                    }
                }
            }
        }
    }

    #[test]
    fn cube_covers_volume() {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            let m = build(&Layout::Cube {
                width: 3,
                height: 4,
                depth: 5,
                wiring: MatrixWiring::default(),
                layer_axis: axis,
                layers_reversed: true,
                layers_serpentine: true,
            });
            assert_eq!(m.count, 60);
            let mut seen = std::collections::HashSet::new();
            for g in &m.grid {
                assert!(g[0] < 3 && g[1] < 4 && g[2] < 5);
                assert!(seen.insert(*g));
            }
        }
    }

    fn monitor(start: Corner, clockwise: bool, offset: i32) -> LayoutMap {
        build(&Layout::Monitor { top: 4, right: 2, bottom: 4, left: 2, start, clockwise, offset, bottom_gap: 0.0 })
    }

    #[test]
    fn monitor_clockwise_from_bottom_left() {
        let m = monitor(Corner::BottomLeft, true, 0);
        assert_eq!(m.count, 12);
        // Up the left side first...
        assert_eq!(m.edges[0].edge, Edge::Left);
        assert!(m.pos[0][1] > m.pos[1][1], "left side should climb");
        // ...then across the top left-to-right, down the right, back along the bottom.
        assert_eq!(m.edges[2].edge, Edge::Top);
        assert!(m.pos[2][0] < m.pos[5][0]);
        assert_eq!(m.edges[6].edge, Edge::Right);
        assert_eq!(m.edges[8].edge, Edge::Bottom);
        assert!(m.pos[8][0] > m.pos[11][0], "bottom runs right-to-left");
    }

    #[test]
    fn monitor_counter_clockwise_from_bottom_left() {
        let m = monitor(Corner::BottomLeft, false, 0);
        // Along the bottom left-to-right first, then up the right side.
        assert_eq!(m.edges[0].edge, Edge::Bottom);
        assert!(m.pos[0][0] < m.pos[3][0]);
        assert_eq!(m.edges[4].edge, Edge::Right);
        assert!(m.pos[4][1] > m.pos[5][1], "right side should climb");
        assert_eq!(m.edges[6].edge, Edge::Top);
        assert_eq!(m.edges[10].edge, Edge::Left);
    }

    #[test]
    fn monitor_offset_rotates_the_start() {
        let base = monitor(Corner::TopLeft, true, 0);
        let shifted = monitor(Corner::TopLeft, true, 3);
        assert_eq!(shifted.pos[0], base.pos[3]);
        let back = monitor(Corner::TopLeft, true, -1);
        assert_eq!(back.pos[0], base.pos[11]);
    }

    #[test]
    fn monitor_bottom_gap_leaves_the_middle_empty() {
        let m = build(&Layout::Monitor {
            top: 0,
            right: 0,
            bottom: 6,
            left: 0,
            start: Corner::TopLeft,
            clockwise: true,
            offset: 0,
            bottom_gap: 0.4,
        });
        assert_eq!(m.count, 6);
        for e in &m.edges {
            assert!(e.b <= 0.3 + 1e-5 || e.a >= 0.7 - 1e-5, "span {e:?} overlaps the gap");
        }
    }

    #[test]
    fn up_means_up_wherever_there_is_height() {
        // On a monitor loop "along" is the height on screen, so fire sits at
        // the bottom edge no matter where the strip happens to start.
        let m = monitor(Corner::TopRight, false, 3);
        for i in 0..m.count {
            assert_eq!(m.along(i), 1.0 - m.pos[i][1]);
        }
        // A plain strip has no height: effects run from the first LED.
        let s = build(&Layout::Strip { count: 10, reversed: false });
        assert_eq!(s.along(0), 0.0);
        assert_eq!(s.along(9), 1.0);
    }

    #[test]
    fn strip_and_ring_basics() {
        let s = build(&Layout::Strip { count: 5, reversed: true });
        assert_eq!(s.u[0], 1.0);
        assert_eq!(s.u[4], 0.0);
        let r = build(&Layout::Ring { count: 4, start_angle: 0.0, clockwise: true });
        assert!(r.looped);
        assert!((r.pos[0][1] - 0.0).abs() < 1e-5, "LED 0 at the top");
        assert!((r.pos[1][0] - 1.0).abs() < 1e-5, "LED 1 at the right");
    }
}
