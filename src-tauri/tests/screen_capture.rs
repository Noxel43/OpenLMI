//! A diagnostic for real desktops: checks that the fast capture path sees the
//! same picture as the plain one on every display (including rotated ones),
//! and prints what each costs. It needs a logged-in desktop session, so it
//! only runs on request:
//!
//!     cargo test --release --test screen_capture -- --ignored --nocapture
#![cfg(windows)]

use openlmi_lib::ambient::{downsample, GRID_H, GRID_W};
use openlmi_lib::screen_dxgi::Duplicator;
use std::time::{Duration, Instant};

#[test]
#[ignore = "needs a real desktop session"]
fn fast_capture_matches_plain_capture() {
    for monitor in xcap::Monitor::all().unwrap() {
        let name = monitor.name().unwrap();
        let (w, h) = (monitor.width().unwrap(), monitor.height().unwrap());

        let mut dup = Duplicator::open(&name).expect("desktop duplication should open");
        // The first frame can take a moment to arrive.
        let deadline = Instant::now() + Duration::from_secs(2);
        let opened = Instant::now();
        let shot = loop {
            if let Some(shot) = dup.grab().expect("grab should not fail") {
                break shot;
            }
            assert!(Instant::now() < deadline, "no frame from {name}");
            std::thread::sleep(Duration::from_millis(2));
        };
        let first_frame = opened.elapsed();
        let image = monitor.capture_image().unwrap();

        // Same orientation and shape as the desktop.
        let fast_aspect = shot.width as f32 / shot.height as f32;
        assert!(
            (fast_aspect - w as f32 / h as f32).abs() < 0.03,
            "{name}: got {}x{} for a {w}x{h} display",
            shot.width,
            shot.height
        );

        let fast = downsample(&shot.rgba, shot.width, shot.height, 1);
        let plain = downsample(image.as_raw(), w as usize, h as usize, 1);
        let distance = |other: &mut dyn Iterator<Item = &openlmi_lib::color::Rgb>| {
            fast.px.iter().zip(other).map(|(a, b)| ((a.r - b.r).abs() + (a.g - b.g).abs() + (a.b - b.b).abs()) / 3.0).sum::<f32>()
                / (GRID_W * GRID_H) as f32
        };
        let diff = distance(&mut plain.px.iter());
        // Reversing the grid turns it upside down: if the picture has any
        // structure this must be a worse match, or the rotation is wrong.
        let diff_flipped = distance(&mut plain.px.iter().rev());

        // What a grab costs when a new frame is waiting, and when none is.
        let (mut busy, mut busy_n, mut idle, mut idle_n) = (Duration::ZERO, 0u32, Duration::ZERO, 0u32);
        let window = Instant::now();
        while window.elapsed() < Duration::from_millis(800) {
            let g = Instant::now();
            let got = dup.grab().unwrap().is_some();
            if got {
                busy += g.elapsed();
                busy_n += 1;
            } else {
                idle += g.elapsed();
                idle_n += 1;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        let g = Instant::now();
        let _ = monitor.capture_image().unwrap();
        let plain_cost = g.elapsed();

        println!(
            "{name} {w}x{h}: reduced to {}x{}, mean difference {diff:.3} (upside down it would be {diff_flipped:.3}), first frame after {first_frame:.1?}",
            shot.width, shot.height
        );
        println!(
            "    fast grab: {:.2?} with a new frame ({busy_n} seen), {:.3?} when unchanged; plain grab: {plain_cost:.1?}",
            busy.checked_div(busy_n).unwrap_or_default(),
            idle.checked_div(idle_n).unwrap_or_default(),
        );
        // The two grabs are a moment apart, so allow for a little movement.
        assert!(diff < 0.08, "{name}: the two capture paths disagree (mean difference {diff:.3})");
    }
}
