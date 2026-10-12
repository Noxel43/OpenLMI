//! A diagnostic for a real machine: listens to whatever the PC is playing for
//! a few seconds and reports how the analysis responds. Play some music, then:
//!
//!     cargo test --release --test audio_live -- --ignored --nocapture
//!
//! It makes no sound itself and passes even in silence; the numbers are for
//! judging by eye.

use openlmi_lib::audio::{Analyser, AudioCapture, BANDS};
use std::time::{Duration, Instant};

fn percentile(sorted: &[f32], p: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * p) as usize]
}

#[test]
#[ignore = "needs a real audio device"]
fn listen_to_what_is_playing() {
    let capture = AudioCapture::start();
    let mut analyser = Analyser::new();
    let seconds = 12.0;
    let (mut levels, mut basses, mut mids, mut trebles) = (vec![], vec![], vec![], vec![]);
    let mut band_sum = [0.0f32; BANDS];
    let (mut dropouts, mut was_running, mut frames) = (0, false, 0u32);
    let (mut beat_times, mut seen_beats) = (Vec::new(), 0u32);

    let started = Instant::now();
    let mut last = started;
    while started.elapsed().as_secs_f32() < seconds {
        capture.request();
        std::thread::sleep(Duration::from_millis(16));
        let now = Instant::now();
        let f = analyser.update(&capture, (now - last).as_secs_f32(), 1.0).clone();
        last = now;
        if f.beat_count != seen_beats {
            seen_beats = f.beat_count;
            beat_times.push(started.elapsed().as_secs_f32());
        }
        let running = capture.status().running;
        if was_running && !running {
            dropouts += 1;
        }
        was_running = running;
        // Skip the first second while capture starts and the gain settles.
        if started.elapsed().as_secs_f32() > 1.5 {
            frames += 1;
            levels.push(f.level);
            basses.push(f.bass);
            mids.push(f.mid);
            trebles.push(f.treble);
            for (sum, b) in band_sum.iter_mut().zip(f.bands) {
                *sum += b;
            }
        }
    }
    let beats = analyser.frame().beat_count;
    let status = capture.status();
    capture.stop();

    println!("device: {} (error: {:?}), stream restarts: {dropouts}", status.device, status.error);
    for (name, values) in [("level", &mut levels), ("bass", &mut basses), ("mid", &mut mids), ("treble", &mut trebles)] {
        values.sort_by(f32::total_cmp);
        let dark = values.iter().filter(|v| **v < 0.05).count() as f32 / values.len().max(1) as f32;
        println!(
            "{name:>6}: 10% {:.2}  median {:.2}  90% {:.2}  max {:.2}  | dark {:.0}% of the time",
            percentile(values, 0.1),
            percentile(values, 0.5),
            percentile(values, 0.9),
            percentile(values, 1.0),
            dark * 100.0
        );
    }
    println!("beats: {beats} in {seconds:.0} s (about {:.0} per minute)", beats as f32 / seconds * 60.0);
    // A steady rhythm shows up as gaps that cluster around one value (or
    // simple multiples of it).
    let mut gaps: Vec<f32> = beat_times.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f32::total_cmp);
    if let Some(&typical) = gaps.get(gaps.len() / 2) {
        let regular = gaps.iter().filter(|g| [1.0f32, 2.0, 0.5, 1.5, 3.0].iter().any(|m| (*g / (typical * m) - 1.0).abs() < 0.15)).count();
        println!(
            "beat gaps: typical {typical:.2} s (shortest {:.2}, longest {:.2}); {:.0}% fit a steady pulse",
            gaps[0],
            gaps[gaps.len() - 1],
            regular as f32 / gaps.len() as f32 * 100.0
        );
    }
    let bars: Vec<String> = band_sum.iter().map(|s| format!("{:.0}", s / frames.max(1) as f32 * 9.0)).collect();
    println!("average per band, bass to treble (0-9): {}", bars.join(""));
    assert_eq!(dropouts, 0, "the stream should keep running");
}
