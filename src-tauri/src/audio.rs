//! Audio capture and analysis for the sound-reactive effects.
//!
//! A background thread owns the capture stream (system loopback or an input
//! device) and fills a ring buffer. The render loop pulls the latest samples
//! through an FFT each frame to get band levels and beat pulses.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const BANDS: usize = 32;
const FFT_SIZE: usize = 2048;
const RING: usize = 8192;

/// One analysed slice of audio. All levels are `0..1`.
#[derive(Clone, Debug)]
pub struct AudioFrame {
    pub level: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// Log-spaced from roughly 40 Hz to 16 kHz.
    pub bands: [f32; BANDS],
    /// Jumps to 1 on a beat and decays.
    pub beat: f32,
    pub beat_count: u32,
}

impl Default for AudioFrame {
    fn default() -> Self {
        AudioFrame { level: 0.0, bass: 0.0, mid: 0.0, treble: 0.0, bands: [0.0; BANDS], beat: 0.0, beat_count: 0 }
    }
}

impl AudioFrame {
    /// Band level at a fractional position `0..1` across the spectrum.
    pub fn band_at(&self, f: f32) -> f32 {
        let p = f.clamp(0.0, 1.0) * (BANDS - 1) as f32;
        let i = (p.floor() as usize).min(BANDS - 2);
        let k = p - i as f32;
        self.bands[i] + (self.bands[i + 1] - self.bands[i]) * k
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub running: bool,
    pub device: String,
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
}

struct Ring {
    buf: Vec<f32>,
    write: usize,
    rate: u32,
    last_data: Instant,
}

struct Shared {
    ring: Mutex<Ring>,
    /// The render loop bumps this while something needs audio.
    wanted_at: Mutex<Option<Instant>>,
    source: Mutex<String>,
    status: Mutex<AudioStatus>,
    failed: AtomicBool,
    stop: AtomicBool,
}

/// Handle to the capture thread.
#[derive(Clone)]
pub struct AudioCapture {
    shared: Arc<Shared>,
}

impl AudioCapture {
    pub fn start() -> Self {
        let shared = Arc::new(Shared {
            ring: Mutex::new(Ring { buf: vec![0.0; RING], write: 0, rate: 48_000, last_data: Instant::now() }),
            wanted_at: Mutex::new(None),
            source: Mutex::new("loopback".into()),
            status: Mutex::new(AudioStatus::default()),
            failed: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("openlmi-audio".into())
            .spawn(move || capture_thread(worker))
            .ok();
        AudioCapture { shared }
    }

    /// Ask for audio to keep flowing; capture stops a few seconds after the
    /// last call.
    pub fn request(&self) {
        *self.shared.wanted_at.lock() = Some(Instant::now());
    }

    pub fn set_source(&self, source: &str) {
        let mut s = self.shared.source.lock();
        if *s != source {
            *s = source.to_string();
        }
    }

    pub fn status(&self) -> AudioStatus {
        self.shared.status.lock().clone()
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }

    /// Copies the most recent `out.len()` samples. Returns the sample rate, or
    /// `None` if nothing has arrived recently (loopback is silent when the PC
    /// is not playing anything).
    fn latest(&self, out: &mut [f32]) -> Option<u32> {
        let ring = self.shared.ring.lock();
        if ring.last_data.elapsed() > Duration::from_millis(250) {
            return None;
        }
        let n = out.len();
        let start = (ring.write + RING - n) % RING;
        for (i, o) in out.iter_mut().enumerate() {
            *o = ring.buf[(start + i) % RING];
        }
        Some(ring.rate)
    }
}

pub fn list_inputs() -> Vec<AudioDeviceInfo> {
    let mut list = vec![AudioDeviceInfo { id: "loopback".into(), name: "System audio (what the PC is playing)".into() }];
    let host = cpal::default_host();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let (Ok(id), Ok(desc)) = (d.id(), d.description()) {
                list.push(AudioDeviceInfo { id: id.to_string(), name: desc.name().to_string() });
            }
        }
    }
    list
}

fn capture_thread(shared: Arc<Shared>) {
    let mut stream: Option<cpal::Stream> = None;
    let mut active_source = String::new();
    let mut retry_at = Instant::now();

    while !shared.stop.load(Ordering::Relaxed) {
        let wanted = shared.wanted_at.lock().is_some_and(|t| t.elapsed() < Duration::from_secs(3));
        let source = shared.source.lock().clone();

        if shared.failed.swap(false, Ordering::Relaxed) {
            stream = None;
            retry_at = Instant::now() + Duration::from_secs(2);
        }
        if stream.is_some() && (!wanted || source != active_source) {
            stream = None;
            shared.status.lock().running = false;
        }
        if stream.is_none() && wanted && Instant::now() >= retry_at {
            match open_stream(&shared, &source) {
                Ok((s, name)) => {
                    stream = Some(s);
                    active_source = source;
                    *shared.status.lock() = AudioStatus { running: true, device: name, error: None };
                }
                Err(e) => {
                    retry_at = Instant::now() + Duration::from_secs(3);
                    *shared.status.lock() = AudioStatus { running: false, device: String::new(), error: Some(e) };
                }
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn open_stream(shared: &Arc<Shared>, source: &str) -> Result<(cpal::Stream, String), String> {
    let host = cpal::default_host();
    let loopback = source == "loopback" || source.is_empty();
    let device = if loopback {
        host.default_output_device().ok_or("No audio output device found")?
    } else {
        source
            .parse::<cpal::DeviceId>()
            .ok()
            .and_then(|id| host.device_by_id(&id))
            .or_else(|| host.default_input_device())
            .ok_or("No audio input device found")?
    };
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Audio device".into());

    // Capturing from an output device is what makes WASAPI hand us a loopback
    // of whatever is being played.
    let config = if loopback || !device.supports_input() {
        device.default_output_config()
    } else {
        device.default_input_config()
    }
    .map_err(|e| e.to_string())?;

    let channels = config.channels().max(1) as usize;
    let format = config.sample_format();
    {
        let mut ring = shared.ring.lock();
        ring.rate = config.sample_rate();
    }

    let sink = shared.clone();
    let failed = shared.clone();
    let stream = device
        .build_input_stream_raw(
            config.config(),
            format,
            move |data: &cpal::Data, _| push_samples(&sink, data, channels),
            move |_err| failed.failed.store(true, Ordering::Relaxed),
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name))
}

fn push_samples(shared: &Shared, data: &cpal::Data, channels: usize) {
    let mut ring = shared.ring.lock();
    let mut push = |frame_sum: f32| {
        let w = ring.write;
        ring.buf[w] = frame_sum / channels as f32;
        ring.write = (w + 1) % RING;
    };
    macro_rules! mixdown {
        ($ty:ty, $conv:expr) => {
            if let Some(samples) = data.as_slice::<$ty>() {
                for frame in samples.chunks_exact(channels) {
                    push(frame.iter().map(|s| $conv(*s)).sum());
                }
            }
        };
    }
    match data.sample_format() {
        cpal::SampleFormat::F32 => mixdown!(f32, |s: f32| s),
        cpal::SampleFormat::F64 => mixdown!(f64, |s: f64| s as f32),
        cpal::SampleFormat::I16 => mixdown!(i16, |s: i16| s as f32 / 32_768.0),
        cpal::SampleFormat::U16 => mixdown!(u16, |s: u16| (s as f32 - 32_768.0) / 32_768.0),
        cpal::SampleFormat::I32 => mixdown!(i32, |s: i32| s as f32 / 2_147_483_648.0),
        cpal::SampleFormat::I8 => mixdown!(i8, |s: i8| s as f32 / 128.0),
        cpal::SampleFormat::U8 => mixdown!(u8, |s: u8| (s as f32 - 128.0) / 128.0),
        _ => {}
    }
    ring.last_data = Instant::now();
}

/// Turns raw samples into an `AudioFrame`. Lives on the render thread.
pub struct Analyser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    samples: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    frame: AudioFrame,
    /// Slowly decaying loudness references used for automatic gain.
    band_peak: f32,
    level_peak: f32,
    bass_avg: f32,
    since_beat: f32,
}

impl Default for Analyser {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyser {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos())
            .collect();
        Analyser {
            fft,
            window,
            samples: vec![0.0; FFT_SIZE],
            spectrum: vec![Complex::default(); FFT_SIZE],
            scratch,
            frame: AudioFrame::default(),
            band_peak: 0.05,
            level_peak: 0.05,
            bass_avg: 0.0,
            since_beat: 1.0,
        }
    }

    pub fn frame(&self) -> &AudioFrame {
        &self.frame
    }

    pub fn update(&mut self, capture: &AudioCapture, dt: f32, gain: f32) -> &AudioFrame {
        let rate = capture.latest(&mut self.samples);
        let mut raw = [0.0f32; BANDS];
        let mut rms = 0.0f32;
        if let Some(rate) = rate {
            raw = self.analyse(rate as f32);
            rms = (self.samples.iter().map(|s| s * s).sum::<f32>() / FFT_SIZE as f32).sqrt();
        }
        self.apply(raw, rms, dt, gain);
        &self.frame
    }

    fn analyse(&mut self, rate: f32) -> [f32; BANDS] {
        for (i, c) in self.spectrum.iter_mut().enumerate() {
            *c = Complex::new(self.samples[i] * self.window[i], 0.0);
        }
        self.fft.process_with_scratch(&mut self.spectrum, &mut self.scratch);

        let bin_hz = rate / FFT_SIZE as f32;
        let (lo, hi) = (40.0f32, 16_000.0f32.min(rate * 0.48));
        let mut out = [0.0f32; BANDS];
        for (b, slot) in out.iter_mut().enumerate() {
            let f0 = lo * (hi / lo).powf(b as f32 / BANDS as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / BANDS as f32);
            let i0 = ((f0 / bin_hz) as usize).clamp(1, FFT_SIZE / 2 - 1);
            let i1 = ((f1 / bin_hz).ceil() as usize).clamp(i0 + 1, FFT_SIZE / 2);
            let peak = self.spectrum[i0..i1].iter().map(|c| c.norm()).fold(0.0f32, f32::max);
            // Music carries far less energy up high; tilt so treble still shows.
            let tilt = 1.0 + 2.5 * (b as f32 / BANDS as f32);
            *slot = peak / (FFT_SIZE as f32 * 0.25) * tilt;
        }
        out
    }

    /// Smoothing, automatic gain and beat detection.
    fn apply(&mut self, raw: [f32; BANDS], rms: f32, dt: f32, gain: f32) {
        let dt = dt.clamp(0.001, 0.1);
        let slow_decay = 0.5f32.powf(dt / 6.0); // references halve every ~6 s

        let loudest = raw.iter().copied().fold(0.0f32, f32::max);
        self.band_peak = (self.band_peak * slow_decay).max(loudest).max(0.015);
        self.level_peak = (self.level_peak * slow_decay).max(rms).max(0.008);

        let release = 0.5f32.powf(dt / 0.12);
        let f = &mut self.frame;
        for (band, r) in f.bands.iter_mut().zip(raw) {
            let target = (r / self.band_peak * gain).clamp(0.0, 1.0).powf(0.8);
            *band = if target > *band { target } else { (*band * release).max(target) };
        }
        let level = (rms / self.level_peak * gain).clamp(0.0, 1.0);
        f.level = if level > f.level { level } else { (f.level * release).max(level) };

        let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
        f.bass = mean(&f.bands[0..6]);
        f.mid = mean(&f.bands[8..20]);
        f.treble = mean(&f.bands[22..BANDS]);

        // A beat is bass energy jumping well above its recent average.
        let bass_now = raw[..5].iter().sum::<f32>() / 5.0;
        self.since_beat += dt;
        if bass_now > self.bass_avg * 1.45 + self.band_peak * 0.06 && self.since_beat > 0.2 {
            f.beat = 1.0;
            f.beat_count = f.beat_count.wrapping_add(1);
            self.since_beat = 0.0;
        } else {
            f.beat *= 0.5f32.powf(dt / 0.11);
        }
        let follow = 1.0 - 0.5f32.powf(dt / 0.4);
        self.bass_avg += (bass_now - self.bass_avg) * follow;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, amp: f32) -> Vec<f32> {
        (0..FFT_SIZE).map(|i| (std::f32::consts::TAU * freq * i as f32 / rate).sin() * amp).collect()
    }

    #[test]
    fn a_tone_lands_in_the_right_band() {
        let mut a = Analyser::new();
        for (freq, expect_low) in [(80.0, true), (6000.0, false)] {
            a.samples = sine(freq, 48_000.0, 0.5);
            let raw = a.analyse(48_000.0);
            let loudest = raw.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).unwrap().0;
            if expect_low {
                assert!(loudest < 8, "80 Hz landed in band {loudest}");
            } else {
                assert!(loudest > 20, "6 kHz landed in band {loudest}");
            }
        }
    }

    #[test]
    fn silence_decays_and_beats_fire() {
        let mut a = Analyser::new();
        let mut loud = [0.0f32; BANDS];
        loud[..5].fill(1.0);
        a.apply(loud, 0.5, 0.016, 1.0);
        assert_eq!(a.frame.beat_count, 1, "a sudden bass hit is a beat");
        assert!(a.frame.bands[0] > 0.9);
        for _ in 0..120 {
            a.apply([0.0; BANDS], 0.0, 0.016, 1.0);
        }
        assert!(a.frame.bands[0] < 0.01);
        assert!(a.frame.level < 0.01);
        assert!(a.frame.beat < 0.01);
    }

    #[test]
    fn band_interpolation_is_bounded() {
        let mut f = AudioFrame::default();
        f.bands[0] = 1.0;
        f.bands[BANDS - 1] = 0.5;
        assert_eq!(f.band_at(0.0), 1.0);
        assert_eq!(f.band_at(1.0), 0.5);
        assert_eq!(f.band_at(5.0), 0.5);
    }
}
