//! A small library of 8x8 pixel-art icons for matrices, and the effect that
//! shows one.

use super::*;
use serde::Serialize;
use std::sync::OnceLock;

pub const SIZE: usize = 8;

pub struct Icon {
    pub name: &'static str,
    pub group: &'static str,
    /// Rows of pixels; `None` is unlit.
    pub px: [[Option<Rgb>; SIZE]; SIZE],
}

/// What the UI needs to draw an icon picker.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IconInfo {
    pub name: &'static str,
    pub group: &'static str,
    /// 64 hex colours, row by row; an empty string is an unlit pixel.
    pub pixels: Vec<String>,
}

fn colour(c: u8) -> Option<Rgb> {
    let hex = match c {
        b'R' => "#ff2a3c",
        b'O' => "#ff8a1e",
        b'Y' => "#ffd21f",
        b'G' => "#2fdc5a",
        b'D' => "#0e8f3c",
        b'C' => "#1fd6e8",
        b'L' => "#8fd8ff",
        b'B' => "#2f6bff",
        b'P' => "#a550ff",
        b'M' => "#ff55b8",
        b'W' => "#ffffff",
        b'N' => "#9a5b2a",
        _ => return None,
    };
    Rgb::from_hex(hex)
}

type Art = [&'static str; SIZE];

#[rustfmt::skip]
const ART: &[(&str, &str, Art)] = &[
    ("Heart", "Symbols", [
        ".RR..RR.",
        "RRRRRRRR",
        "RRRRRRRR",
        "RRRRRRRR",
        ".RRRRRR.",
        "..RRRR..",
        "...RR...",
        "........"]),
    ("Star", "Symbols", [
        "...YY...",
        "...YY...",
        "YYYYYYYY",
        ".YYYYYY.",
        "..YYYY..",
        ".YYYYYY.",
        ".YY..YY.",
        "YY....YY"]),
    ("Check", "Symbols", [
        "........",
        ".......G",
        "......GG",
        "G....GG.",
        "GG..GG..",
        ".GGGG...",
        "..GG....",
        "........"]),
    ("Cross", "Symbols", [
        "RR....RR",
        "RRR..RRR",
        ".RRRRRR.",
        "..RRRR..",
        "..RRRR..",
        ".RRRRRR.",
        "RRR..RRR",
        "RR....RR"]),
    ("Warning", "Symbols", [
        "...YY...",
        "..YYYY..",
        "..Y..Y..",
        ".YY..YY.",
        ".YY..YY.",
        "YYYYYYYY",
        "YYY..YYY",
        "YYYYYYYY"]),
    ("Question", "Symbols", [
        "..BBBB..",
        ".BB..BB.",
        ".....BB.",
        "....BB..",
        "...BB...",
        "...BB...",
        "........",
        "...BB..."]),
    ("Power", "Symbols", [
        "...GG...",
        ".G.GG.G.",
        "GG.GG.GG",
        "GG.GG.GG",
        "GG....GG",
        "GG....GG",
        ".GGGGGG.",
        "..GGGG.."]),
    ("Bell", "Symbols", [
        "...YY...",
        "..YYYY..",
        ".YYYYYY.",
        ".YYYYYY.",
        ".YYYYYY.",
        "YYYYYYYY",
        "........",
        "...YY..."]),
    ("Mail", "Symbols", [
        "........",
        "WWWWWWWW",
        "WW....WW",
        "W.W..W.W",
        "W..WW..W",
        "W......W",
        "WWWWWWWW",
        "........"]),
    ("Home", "Symbols", [
        "...RR...",
        "..RRRR..",
        ".RRRRRR.",
        "RRRRRRRR",
        ".WWWWWW.",
        ".WNNWLW.",
        ".WNNWWW.",
        ".WNNWWW."]),
    ("Music", "Symbols", [
        "..PPPPPP",
        "..PPPPPP",
        "..P....P",
        "..P....P",
        "..P....P",
        ".PP...PP",
        "PPP..PPP",
        "PP...PP."]),
    ("Clock", "Symbols", [
        "..WWWW..",
        ".W.R..W.",
        "W..R...W",
        "W..RRR.W",
        "W......W",
        "W......W",
        ".W....W.",
        "..WWWW.."]),
    ("Wi-Fi", "Symbols", [
        "........",
        ".CCCCCC.",
        "C......C",
        "..CCCC..",
        ".C....C.",
        "........",
        "...CC...",
        "...CC..."]),
    ("Play", "Symbols", [
        ".G......",
        ".GGG....",
        ".GGGGG..",
        ".GGGGGGG",
        ".GGGGGGG",
        ".GGGGG..",
        ".GGG....",
        ".G......"]),
    ("Pause", "Symbols", [
        "........",
        ".OO..OO.",
        ".OO..OO.",
        ".OO..OO.",
        ".OO..OO.",
        ".OO..OO.",
        ".OO..OO.",
        "........"]),
    ("Arrow up", "Arrows", [
        "...GG...",
        "..GGGG..",
        ".GGGGGG.",
        "GGGGGGGG",
        "...GG...",
        "...GG...",
        "...GG...",
        "...GG..."]),
    ("Smile", "Fun", [
        "..YYYY..",
        ".YYYYYY.",
        "YY.YY.YY",
        "YYYYYYYY",
        "Y.YYYY.Y",
        "YY....YY",
        ".YYYYYY.",
        "..YYYY.."]),
    ("Ghost", "Fun", [
        "..CCCC..",
        ".CCCCCC.",
        "CWWCCWWC",
        "CWBCCWBC",
        "CCCCCCCC",
        "CCCCCCCC",
        "CCCCCCCC",
        "C.CC.CC."]),
    ("Pac-Man", "Fun", [
        "..YYYY..",
        ".YYYYYY.",
        "YYYYYY..",
        "YYYY....",
        "YYYY....",
        "YYYYYY..",
        ".YYYYYY.",
        "..YYYY.."]),
    ("Invader", "Fun", [
        ".G....G.",
        "..G..G..",
        ".GGGGGG.",
        "GG.GG.GG",
        "GGGGGGGG",
        ".GGGGGG.",
        ".G....G.",
        "G......G"]),
    ("Mushroom", "Fun", [
        "..RRRR..",
        ".RWRRWR.",
        "RWWRRWWR",
        "RRRRRRRR",
        ".WWWWWW.",
        ".W.WW.W.",
        ".WWWWWW.",
        "..WWWW.."]),
    ("Skull", "Fun", [
        ".WWWWWW.",
        "WWWWWWWW",
        "W..WW..W",
        "W..WW..W",
        "WWW..WWW",
        ".WWWWWW.",
        ".W.WW.W.",
        ".W.WW.W."]),
    ("Cat", "Fun", [
        "O......O",
        "OO....OO",
        "OOOOOOOO",
        "OGOOOOGO",
        "OOOOOOOO",
        "OOOMMOOO",
        ".OWOOWO.",
        "..OOOO.."]),
    ("Rainbow", "Fun", [
        "..RRRR..",
        ".ROOOOR.",
        "ROYYYYOR",
        "OYGGGGYO",
        "YGBBBBGY",
        "GB....BG",
        "B......B",
        "........"]),
    ("Sun", "Nature", [
        "Y..YY..Y",
        ".Y....Y.",
        "..OOOO..",
        "Y.OOOO.Y",
        "Y.OOOO.Y",
        "..OOOO..",
        ".Y....Y.",
        "Y..YY..Y"]),
    ("Moon", "Nature", [
        "..WWW...",
        ".WWW....",
        "WWW.....",
        "WWW.....",
        "WWW....W",
        ".WWW..WW",
        "..WWWWW.",
        "...WWW.."]),
    ("Cloud", "Nature", [
        "........",
        "..WWW...",
        ".WWWWW..",
        ".WWWWWW.",
        "WWWWWWWW",
        "WWWWWWWW",
        ".WWWWWW.",
        "........"]),
    ("Rain", "Nature", [
        "..WWW...",
        ".WWWWWW.",
        "WWWWWWWW",
        "WWWWWWWW",
        "........",
        ".B..B..B",
        "B..B..B.",
        "........"]),
    ("Bolt", "Nature", [
        "....YY..",
        "...YY...",
        "..YY....",
        ".YYYYY..",
        "...YY...",
        "..YY....",
        ".YY.....",
        ".Y......"]),
    ("Snow", "Nature", [
        "...LL...",
        ".L.LL.L.",
        "..LLLL..",
        "LLLLLLLL",
        "LLLLLLLL",
        "..LLLL..",
        ".L.LL.L.",
        "...LL..."]),
    ("Fire", "Nature", [
        "...R....",
        "..RR.R..",
        "..RRRR..",
        ".RROORR.",
        ".ROOOOR.",
        "ROOYYOOR",
        "ROYYYYOR",
        ".OYYYYO."]),
    ("Drop", "Nature", [
        "...BB...",
        "...BB...",
        "..BBBB..",
        "..BBBB..",
        ".BBBBBB.",
        ".BLBBBB.",
        ".BBBBBB.",
        "..BBBB.."]),
    ("Tree", "Nature", [
        "...DD...",
        "..DDDD..",
        ".DDDDDD.",
        "..DDDD..",
        ".DDDDDD.",
        "DDDDDDDD",
        "...NN...",
        "...NN..."]),
    ("Flower", "Nature", [
        ".MM..MM.",
        "MMMMMMMM",
        "MMMYYMMM",
        ".MMYYMM.",
        "..MMMM..",
        "...GG...",
        "..GGG...",
        "...GG..."]),
];

type Pixels = [[Option<Rgb>; SIZE]; SIZE];

fn parse(art: &Art) -> Pixels {
    let mut px = [[None; SIZE]; SIZE];
    for (y, row) in art.iter().enumerate() {
        for (x, c) in row.bytes().take(SIZE).enumerate() {
            px[y][x] = colour(c);
        }
    }
    px
}

/// Reads `px` through a coordinate change, to turn one arrow into four.
fn remap(px: &Pixels, from: impl Fn(usize, usize) -> (usize, usize)) -> Pixels {
    let mut out = [[None; SIZE]; SIZE];
    for (y, row) in out.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            let (sx, sy) = from(x, y);
            *cell = px[sy][sx];
        }
    }
    out
}

pub fn all() -> &'static [Icon] {
    static ICONS: OnceLock<Vec<Icon>> = OnceLock::new();
    ICONS.get_or_init(|| {
        let mut icons = Vec::new();
        for (name, group, art) in ART {
            let px = parse(art);
            icons.push(Icon { name, group, px });
            if *name == "Arrow up" {
                let last = SIZE - 1;
                icons.push(Icon { name: "Arrow down", group, px: remap(&px, |x, y| (x, last - y)) });
                icons.push(Icon { name: "Arrow left", group, px: remap(&px, |x, y| (y, x)) });
                icons.push(Icon { name: "Arrow right", group, px: remap(&px, |x, y| (y, last - x)) });
            }
        }
        icons
    })
}

pub fn find(name: &str) -> Option<&'static Icon> {
    all().iter().find(|i| i.name == name)
}

pub fn infos() -> Vec<IconInfo> {
    all()
        .iter()
        .map(|icon| IconInfo {
            name: icon.name,
            group: icon.group,
            pixels: icon
                .px
                .iter()
                .flatten()
                .map(|p| p.map_or(String::new(), |c| {
                    let [r, g, b] = c.to_u8();
                    format!("#{r:02x}{g:02x}{b:02x}")
                }))
                .collect(),
        })
        .collect()
}

const MOTIONS: &[&str] = &["Still", "Pulse", "Bounce", "Scroll", "Blink"];

pub fn effect() -> EffectDef {
    EffectDef {
        id: "icon",
        name: "Icon",
        category: "Matrix",
        description: "A pixel-art icon from the built-in library, still or in motion.",
        audio: false,
        grid_only: true,
        params: vec![
            ParamDef { key: "icon", label: "Icon", kind: ParamKind::Icon { default: "Heart" } },
            select("motion", "Motion", MOTIONS, "Still"),
            color("background", "Background", "#000000"),
            speed(1.0),
        ],
        make: || Box::new(Stateless(draw)),
    }
}

/// How many LEDs one icon pixel covers on a `w` x `h` grid: the largest whole
/// number that fits, or a fraction when the grid is smaller than the icon.
pub fn zoom(w: f32, h: f32) -> f32 {
    let fit = w.min(h) / SIZE as f32;
    if fit >= 1.0 {
        fit.floor()
    } else {
        fit
    }
}

fn draw(cx: &Ctx, out: &mut [Rgb]) {
    let icon = find(cx.p.text("icon")).unwrap_or(&all()[0]);
    let background = cx.p.color("background");
    let (w, h) = (cx.map.dims[0] as f32, cx.map.dims[1] as f32);
    let zoom = zoom(w, h);
    let size = SIZE as f32 * zoom;
    // Top-left corner of the icon, centred by default.
    let (mut left, mut top) = (((w - size) / 2.0).floor(), ((h - size) / 2.0).floor());
    let mut level = 1.0;

    match cx.p.text("motion") {
        "Pulse" => level = 0.3 + 0.7 * (0.5 + 0.5 * (cx.t * 3.0).sin()),
        "Blink" => level = if (cx.t * 1.2).fract() < 0.6 { 1.0 } else { 0.0 },
        "Bounce" => {
            // Use the spare room above and below, or one icon pixel if there is none.
            let room = ((h - size) / 2.0).max(zoom.max(1.0));
            top += ((cx.t * 4.0).sin() * room).round();
        }
        "Scroll" => {
            // In from the right edge, out past the left, and round again.
            let travelled = (cx.t * 6.0 * zoom.max(1.0)) % (w + size);
            left = (w - travelled).floor();
        }
        _ => {}
    }

    for (i, o) in out.iter_mut().enumerate() {
        let g = cx.map.grid[i];
        let (x, y) = (((g[0] as f32 - left) / zoom).floor(), ((g[1] as f32 - top) / zoom).floor());
        let inside = (0.0..SIZE as f32).contains(&x) && (0.0..SIZE as f32).contains(&y);
        *o = match inside.then(|| icon.px[y as usize][x as usize]).flatten() {
            Some(c) => c.scale(level).max(background.scale(1.0 - level)),
            None => background,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioFrame;
    use crate::layout;
    use crate::model::{Layout, MatrixWiring, ParamValue, Params};

    #[test]
    fn artwork_is_well_formed() {
        let mut names = std::collections::HashSet::new();
        for (name, _, art) in ART {
            assert!(names.insert(*name), "two icons are called {name}");
            for row in art {
                assert_eq!(row.len(), SIZE, "{name}: row {row:?} is not {SIZE} wide");
                for c in row.bytes() {
                    assert!(c == b'.' || colour(c).is_some(), "{name}: unknown colour letter {:?}", c as char);
                }
            }
            assert!(art.iter().any(|row| row.bytes().any(|c| c != b'.')), "{name} is blank");
        }
        assert!(all().len() >= 35);
    }

    fn lit(px: &Pixels) -> Vec<(usize, usize)> {
        (0..SIZE).flat_map(|y| (0..SIZE).map(move |x| (x, y))).filter(|(x, y)| px[*y][*x].is_some()).collect()
    }

    #[test]
    fn arrows_point_where_they_say() {
        let centre = |name: &str| {
            let cells = lit(&find(name).unwrap().px);
            let n = cells.len() as f32;
            (cells.iter().map(|c| c.0 as f32).sum::<f32>() / n, cells.iter().map(|c| c.1 as f32).sum::<f32>() / n)
        };
        // The head is the heavy end, so the centre of mass leans that way.
        assert!(centre("Arrow up").1 < 3.5);
        assert!(centre("Arrow down").1 > 3.5);
        assert!(centre("Arrow left").0 < 3.5);
        assert!(centre("Arrow right").0 > 3.5);
        // All four are the same shape.
        let count = lit(&find("Arrow up").unwrap().px).len();
        for name in ["Arrow down", "Arrow left", "Arrow right"] {
            assert_eq!(lit(&find(name).unwrap().px).len(), count);
        }
    }

    fn render(width: u32, height: u32, icon: &str, motion: &str, t: f32) -> (layout::LayoutMap, Vec<Rgb>) {
        let map = layout::build(&Layout::Matrix { width, height, wiring: MatrixWiring::default() });
        let def = effect();
        let mut params = Params::new();
        params.insert("icon".into(), ParamValue::Text(icon.into()));
        params.insert("motion".into(), ParamValue::Text(motion.into()));
        let bag = ParamBag::build(&def.params, &params);
        let mut out = vec![Rgb::BLACK; map.count];
        let audio = AudioFrame::default();
        draw(&Ctx { t, dt: 0.016, real_dt: 0.016, map: &map, p: &bag, audio: &audio }, &mut out);
        (map, out)
    }

    fn picture(map: &layout::LayoutMap, out: &[Rgb]) -> Vec<String> {
        let [w, h, _] = map.dims;
        (0..h)
            .map(|y| (0..w).map(|x| if out[map.index_at(x as i64, y as i64, 0).unwrap()].peak() > 0.0 { '#' } else { '.' }).collect())
            .collect()
    }

    #[test]
    fn an_icon_fills_an_8x8_matrix_exactly() {
        let (map, out) = render(8, 8, "Heart", "Still", 0.0);
        assert_eq!(
            picture(&map, &out),
            vec![".##..##.", "########", "########", "########", ".######.", "..####..", "...##...", "........"]
        );
        // The serpentine wiring is respected: row 1 runs right-to-left.
        assert_eq!(out[map.index_at(0, 1, 0).unwrap()].to_u8(), [255, 42, 60]);
    }

    #[test]
    fn bigger_matrices_zoom_by_whole_pixels_and_centre() {
        // 20x16: room for 2x zoom (16x16), centred with 2 spare columns each side.
        let (map, out) = render(20, 16, "Heart", "Still", 0.0);
        let rows = picture(&map, &out);
        assert_eq!(rows[0], "....####....####....");
        assert_eq!(rows[1], rows[0], "each icon pixel is two LEDs tall");
        assert_eq!(rows[2], "..################..");
        assert!(rows[14].chars().all(|c| c == '.'));
    }

    #[test]
    fn an_unknown_icon_falls_back_instead_of_failing() {
        let (_, out) = render(8, 8, "No such icon", "Still", 0.0);
        assert!(out.iter().any(|c| c.peak() > 0.0));
    }

    #[test]
    fn scrolling_moves_the_icon_across_and_off() {
        let column_of = |t: f32| {
            let (map, out) = render(16, 8, "Heart", "Scroll", t);
            let cols: Vec<usize> = (0..16).filter(|x| (0..8).any(|y| out[map.index_at(*x as i64, y, 0).unwrap()].peak() > 0.0)).collect();
            cols.first().copied()
        };
        // Starts off the right edge, appears, moves left.
        assert_eq!(column_of(0.0), None);
        let early = column_of(0.5).expect("visible soon after starting");
        let later = column_of(1.5).expect("still visible");
        assert!(later < early, "moved left: {early} -> {later}");
    }
}
