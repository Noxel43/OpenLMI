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

/// Source id of the built-in test signal.
pub const DEMO_SOURCE: &str = "demo";

pub fn list_inputs() -> Vec<AudioDeviceInfo> {
    let mut list = vec![
        AudioDeviceInfo { id: "loopback".into(), name: "System audio (what the PC is playing)".into() },
        AudioDeviceInfo { id: DEMO_SOURCE.into(), name: "Demo beat (silent test signal)".into() },
    ];
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
    let mut active_device = String::new();
    let mut retry_at = Instant::now();
    let mut checked_default_at = Instant::now();
    let mut demo: Option<(Instant, usize)> = None;

    while !shared.stop.load(Ordering::Relaxed) {
        let wanted = shared.wanted_at.lock().is_some_and(|t| t.elapsed() < Duration::from_secs(3));
        let source = shared.source.lock().clone();

        // The demo beat needs no device: synthesise it straight into the
        // buffer, in step with the clock.
        if wanted && source == DEMO_SOURCE {
            stream = None;
            let (started, written) = demo.get_or_insert_with(|| (Instant::now(), 0));
            let due = (started.elapsed().as_secs_f64() * DEMO_RATE as f64) as usize;
            let mut ring = shared.ring.lock();
            ring.rate = DEMO_RATE;
            // After a stall, skip ahead rather than flood the buffer.
            *written = (*written).max(due.saturating_sub(RING));
            while *written < due {
                let w = ring.write;
                ring.buf[w] = demo_song(*written, DEMO_RATE, 120.0);
                ring.write = (w + 1) % RING;
                *written += 1;
            }
            ring.last_data = Instant::now();
            drop(ring);
            *shared.status.lock() = AudioStatus { running: true, device: "the demo beat".into(), error: None };
            active_source = source;
            std::thread::sleep(Duration::from_millis(8));
            continue;
        }
        demo = None;

        if shared.failed.swap(false, Ordering::Relaxed) {
            stream = None;
            retry_at = Instant::now() + Duration::from_secs(1);
            let mut status = shared.status.lock();
            status.running = false;
            status.error = Some("The audio device stopped. Reconnecting...".into());
        }
        if stream.is_some() && (!wanted || source != active_source) {
            stream = None;
            shared.status.lock().running = false;
        }
        // "System audio" should follow the output the user is actually
        // listening on, e.g. after switching from speakers to headphones.
        if stream.is_some() && is_loopback(&active_source) && checked_default_at.elapsed() > Duration::from_secs(2) {
            checked_default_at = Instant::now();
            let current = cpal::default_host().default_output_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
            if current.is_some_and(|id| id != active_device) {
                stream = None;
            }
        }
        if stream.is_none() && wanted && Instant::now() >= retry_at {
            match open_stream(&shared, &source) {
                Ok((s, name, device)) => {
                    stream = Some(s);
                    active_source = source;
                    active_device = device;
                    checked_default_at = Instant::now();
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

const DEMO_RATE: u32 = 48_000;

/// One sample of a small looping dance track: kick, snare, hi-hats, a bass
/// line, chords and a melody. It exists so the audio effects can be tried
/// (and tested) without anything actually playing.
pub fn demo_song(n: usize, rate: u32, bpm: f32) -> f32 {
    use std::f32::consts::TAU;
    const BASS: [f32; 4] = [55.0, 55.0, 73.42, 65.41];
    const CHORDS: [[f32; 3]; 4] = [[220.0, 261.63, 329.63], [220.0, 261.63, 329.63], [293.66, 349.23, 440.0], [261.63, 329.63, 392.0]];
    const MELODY: [f32; 5] = [440.0, 523.25, 587.33, 659.25, 783.99];

    let t = n as f32 / rate as f32;
    let beat_len = 60.0 / bpm;
    let beat = (t / beat_len) as usize;
    let in_beat = t - beat as f32 * beat_len;
    let bar = (beat / 4) % 4;
    let noise = |seed: usize| crate::color::hash01(seed as u32) * 2.0 - 1.0;

    // Kick on every beat: a sine that drops in pitch as it fades.
    let kick = (TAU * (45.0 * in_beat + 2.2 * (1.0 - (-30.0 * in_beat).exp()))).sin() * (-14.0 * in_beat).exp() * 0.7;
    // Snare on beats two and four.
    let snare = if beat % 2 == 1 {
        (noise(n) * 0.22 + (TAU * 190.0 * in_beat).sin() * 0.18) * (-24.0 * in_beat).exp()
    } else {
        0.0
    };
    // Hi-hat on every half beat: the difference of two noise samples is hissy.
    let in_half = in_beat % (beat_len / 2.0);
    let hat = (noise(n) - noise(n + 1)) * (-28.0 * in_half).exp() * 0.09;

    let harmonics = |freq: f32, count: usize, rolloff: f32| -> f32 {
        (1..=count).map(|k| (TAU * freq * k as f32 * t).sin() / (k as f32).powf(rolloff)).sum()
    };
    let bass = harmonics(BASS[bar], 4, 1.0) * (0.6 + 0.4 * (-4.0 * in_beat).exp()) * 0.13;
    let chord: f32 = CHORDS[bar].iter().map(|f| harmonics(*f, 6, 1.6)).sum::<f32>() * 0.035;
    let note = MELODY[(crate::color::hash_u32(beat as u32 / 2 * 7 + 3) % 5) as usize];
    let melody = harmonics(note, 3, 1.3) * (-5.0 * in_beat).exp() * 0.07;

    (kick + snare + hat + bass + chord + melody) * 0.7
}

fn is_loopback(source: &str) -> bool {
    source == "loopback" || source.is_empty()
}

/// Whether a stream error means the stream is dead. Glitch reports and
/// scheduling notices arrive through the same callback while audio keeps
/// flowing; restarting on those would chop the sound into short bursts.
fn is_fatal(kind: cpal::ErrorKind) -> bool {
    !matches!(kind, cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied | cpal::ErrorKind::DeviceChanged)
}

fn open_stream(shared: &Arc<Shared>, source: &str) -> Result<(cpal::Stream, String, String), String> {
    let host = cpal::default_host();
    let loopback = is_loopback(source);
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
    let id = device.id().map(|id| id.to_string()).unwrap_or_default();

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
            move |err| {
                if is_fatal(err.kind()) {
                    failed.failed.store(true, Ordering::Relaxed);
                }
            },
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, name, id))
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

/// How far below the loudest recent sound a band still registers. Hearing is
/// logarithmic: on a linear scale one loud drum hit makes everything else
/// look silent, on a decibel scale the rest of the music stays visible.
const RANGE_DB: f32 = 28.0;
/// The loudness reference never sinks below this, so background hiss in a
/// quiet room is not amplified into flicker.
const QUIETEST_CEILING_DB: f32 = -48.0;
/// Nothing quieter than this ever registers: it keeps the hiss of a quiet
/// room from showing as a faint flicker in the treble.
const NOISE_FLOOR_DB: f32 = -72.0;
/// The loudness meter covers a narrower span, and is referenced to the
/// music's typical loudness rather than its peaks, so it visibly swings.
const LEVEL_RANGE_DB: f32 = 12.0;
const LEVEL_HEADROOM_DB: f32 = 4.0;
/// Treble lift per band. 32 bands span 8.6 octaves.
const TILT_DB_PER_BAND: f32 = 0.35;
/// How many standard deviations above the recent average an onset must be.
const BEAT_SPREAD: f32 = 2.0;
/// The shortest gap between two beats: a little faster than any dance track.
const BEAT_GAP_S: f32 = 0.26;

/// Turns raw samples into an `AudioFrame`. Lives on the render thread.
pub struct Analyser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    samples: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    frame: AudioFrame,
    /// The band loudness (dB) that currently maps to full scale. It rises
    /// quickly with the music and falls slowly: automatic gain.
    ceiling_db: f32,
    level_ceiling_db: f32,
    /// Each band's loudness on the previous frame, to spot sudden rises.
    previous_db: [f32; BANDS],
    /// Running average and spread of those rises; a beat stands out from them.
    onset_mean: f32,
    onset_var: f32,
    since_beat: f32,
}

impl Default for Analyser {
    fn default() -> Self {
        Self::new()
    }
}

/// Moves `value` towards `target`, covering half the distance every
/// `half_life` seconds.
fn approach(value: f32, target: f32, dt: f32, half_life: f32) -> f32 {
    target + (value - target) * 0.5f32.powf(dt / half_life)
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
            ceiling_db: QUIETEST_CEILING_DB,
            level_ceiling_db: QUIETEST_CEILING_DB,
            previous_db: [-160.0; BANDS],
            onset_mean: 0.0,
            onset_var: 0.0,
            since_beat: 1.0,
        }
    }

    pub fn frame(&self) -> &AudioFrame {
        &self.frame
    }

    pub fn update(&mut self, capture: &AudioCapture, dt: f32, gain: f32) -> &AudioFrame {
        let rate = capture.latest(&mut self.samples);
        self.process(rate, dt, gain)
    }

    /// Analyses whatever is in `self.samples`; `rate` is `None` when no audio
    /// is arriving.
    fn process(&mut self, rate: Option<u32>, dt: f32, gain: f32) -> &AudioFrame {
        let gain_db = 20.0 * gain.clamp(0.05, 20.0).log10();
        let (bands_db, level_db) = match rate {
            Some(rate) => {
                let rms = (self.samples.iter().map(|s| s * s).sum::<f32>() / FFT_SIZE as f32).sqrt();
                let mut bands = self.analyse(rate as f32);
                bands.iter_mut().for_each(|b| *b += gain_db);
                (bands, 20.0 * (rms + 1e-9).log10() + gain_db)
            }
            None => ([-160.0; BANDS], -160.0),
        };
        self.apply(bands_db, level_db, dt);
        &self.frame
    }

    /// Loudness of each band in decibels (0 dB = a full-scale tone).
    fn analyse(&mut self, rate: f32) -> [f32; BANDS] {
        for (i, c) in self.spectrum.iter_mut().enumerate() {
            *c = Complex::new(self.samples[i] * self.window[i], 0.0);
        }
        self.fft.process_with_scratch(&mut self.spectrum, &mut self.scratch);

        let bin_hz = rate / FFT_SIZE as f32;
        let (lo, hi) = (40.0f32, 16_000.0f32.min(rate * 0.48));
        // A full-scale sine through a Hann window peaks at N/4.
        let norm = 1.0 / (FFT_SIZE as f32 * 0.25).powi(2);
        let mut out = [0.0f32; BANDS];
        for (b, slot) in out.iter_mut().enumerate() {
            let f0 = lo * (hi / lo).powf(b as f32 / BANDS as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / BANDS as f32);
            let i0 = ((f0 / bin_hz) as usize).clamp(1, FFT_SIZE / 2 - 1);
            let i1 = ((f1 / bin_hz).ceil() as usize).clamp(i0 + 1, FFT_SIZE / 2);
            let power: f32 = self.spectrum[i0..i1].iter().map(|c| c.norm_sqr()).sum::<f32>() * norm;
            // Music thins out towards the top; lift the treble so it shows.
            *slot = 10.0 * (power + 1e-16).log10() + TILT_DB_PER_BAND * b as f32;
        }
        out
    }

    /// Automatic gain, smoothing and beat detection on decibel values.
    fn apply(&mut self, bands_db: [f32; BANDS], level_db: f32, dt: f32) {
        let dt = dt.clamp(0.001, 0.1);

        // The ceiling chases the loudest band: up within a fraction of a
        // second, down a few dB per second. A lone bang therefore clips for an
        // instant instead of dimming everything that follows.
        let follow = |ceiling: f32, loudest: f32| {
            let next = if loudest > ceiling { approach(ceiling, loudest, dt, 0.12) } else { ceiling - 3.0 * dt };
            next.max(QUIETEST_CEILING_DB)
        };
        let loudest = bands_db.iter().copied().fold(f32::MIN, f32::max);
        self.ceiling_db = follow(self.ceiling_db, loudest);
        // The meter's reference moves slowly both ways: it settles on how
        // loud the music usually is, and hits rise above it.
        let half_life = if level_db > self.level_ceiling_db { 0.5 } else { 2.5 };
        self.level_ceiling_db = approach(self.level_ceiling_db, level_db, dt, half_life).max(QUIETEST_CEILING_DB);

        let f = &mut self.frame;
        for (b, (band, db)) in f.bands.iter_mut().zip(bands_db).enumerate() {
            // The noise gate looks at the level before the treble lift.
            let audible = db - TILT_DB_PER_BAND * b as f32 > NOISE_FLOOR_DB;
            let target = if audible { ((db - self.ceiling_db) / RANGE_DB + 1.0).clamp(0.0, 1.0) } else { 0.0 };
            // Bars jump up at once and fall back smoothly.
            *band = if target > *band { target } else { approach(*band, target, dt, 0.09) };
        }
        let level_top = self.level_ceiling_db + LEVEL_HEADROOM_DB;
        let level = if level_db > NOISE_FLOOR_DB { ((level_db - level_top) / LEVEL_RANGE_DB + 1.0).clamp(0.0, 1.0) } else { 0.0 };
        f.level = approach(f.level, level, dt, if level > f.level { 0.015 } else { 0.13 });

        let mean = |s: &[f32]| s.iter().sum::<f32>() / s.len() as f32;
        f.bass = mean(&f.bands[0..7]);
        f.mid = mean(&f.bands[9..21]);
        f.treble = mean(&f.bands[22..BANDS]);

        // Beats. An onset is the music suddenly getting louder: add up how far
        // each band rose since the last frame, counting the bass double since
        // that is where the kick drum lives. Rises from below the audible
        // range are ignored so that sound starting after a pause is not one
        // enormous onset.
        let quietest = (self.ceiling_db - 40.0).max(NOISE_FLOOR_DB);
        let mut onset = 0.0;
        for (b, db) in bands_db.iter().enumerate() {
            let now = db.max(quietest);
            onset += (now - self.previous_db[b].max(quietest)).max(0.0) * if b < 8 { 2.0 } else { 1.0 };
            self.previous_db[b] = now;
        }
        onset /= (BANDS + 8) as f32;

        // A beat is an onset that stands clear of the recent ones, so a busy
        // hi-hat pattern between the drum hits does not count.
        self.since_beat += dt;
        let threshold = self.onset_mean + BEAT_SPREAD * self.onset_var.sqrt();
        if onset > threshold.max(1.0) && self.since_beat > BEAT_GAP_S && level_db > NOISE_FLOOR_DB {
            f.beat = 1.0;
            f.beat_count = f.beat_count.wrapping_add(1);
            self.since_beat = 0.0;
        } else {
            f.beat *= 0.5f32.powf(dt / 0.11);
        }
        let learn = 1.0 - 0.5f32.powf(dt / 1.2);
        self.onset_mean += (onset - self.onset_mean) * learn;
        self.onset_var += ((onset - self.onset_mean).powi(2) - self.onset_var) * learn;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const FPS: f32 = 60.0;

    /// Repeatable noise in `-1..1` for a sample index.
    fn noise(n: usize) -> f32 {
        crate::color::hash01(n as u32) * 2.0 - 1.0
    }

    /// The demo song rendered to samples.
    fn track(seconds: f32, bpm: f32, volume: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize + FFT_SIZE).map(|n| demo_song(n, RATE, bpm) * volume).collect()
    }

    /// Feeds part of a recording through the analyser the way the render loop
    /// does: one overlapping window per frame.
    fn play(a: &mut Analyser, samples: &[f32], from: f32, to: f32, gain: f32, mut each: impl FnMut(f32, &AudioFrame)) {
        let step = RATE as usize / FPS as usize;
        for frame in (from * FPS) as usize..(to * FPS) as usize {
            let end = FFT_SIZE + frame * step;
            a.samples.copy_from_slice(&samples[end - FFT_SIZE..end]);
            a.process(Some(RATE), 1.0 / FPS, gain);
            each(frame as f32 / FPS, &a.frame);
        }
    }

    fn mean(values: &[f32]) -> f32 {
        values.iter().sum::<f32>() / values.len().max(1) as f32
    }
    fn lowest(values: &[f32]) -> f32 {
        values.iter().copied().fold(f32::MAX, f32::min)
    }
    fn highest(values: &[f32]) -> f32 {
        values.iter().copied().fold(f32::MIN, f32::max)
    }

    #[test]
    fn a_tone_lands_in_the_right_band() {
        let mut a = Analyser::new();
        for (freq, expect_low) in [(80.0, true), (6000.0, false)] {
            a.samples = (0..FFT_SIZE).map(|i| (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin() * 0.5).collect();
            let db = a.analyse(RATE as f32);
            let loudest = db.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).unwrap().0;
            if expect_low {
                assert!(loudest < 8, "80 Hz landed in band {loudest}");
            } else {
                assert!(loudest > 20, "6 kHz landed in band {loudest}");
            }
            // Half-scale tone: about -6 dB before the treble lift.
            let level = db[loudest] - TILT_DB_PER_BAND * loudest as f32;
            assert!((-9.0..-3.0).contains(&level), "{freq} Hz measured {level} dB");
        }
    }

    /// The bug this guards against: effects flashing on the loudest hits and
    /// sitting dark the rest of the time.
    #[test]
    fn music_stays_visible_between_drum_hits() {
        let song = track(8.0, 120.0, 0.5);
        let mut a = Analyser::new();
        let (mut levels, mut basses, mut mids, mut trebles) = (vec![], vec![], vec![], vec![]);
        play(&mut a, &song, 0.0, 8.0, 1.0, |t, f| {
            if t > 2.0 {
                levels.push(f.level);
                basses.push(f.bass);
                mids.push(f.mid);
                trebles.push(f.treble);
            }
        });
        for (name, values) in [("level", &levels), ("bass", &basses), ("mid", &mids), ("treble", &trebles)] {
            // Treble in this song is only the hi-hats, which really do stop
            // between hits; everything else plays continuously.
            if name != "treble" {
                assert!(lowest(values) > 0.1, "{name} dropped out between hits: lowest {}", lowest(values));
            }
            assert!(mean(values) > if name == "treble" { 0.15 } else { 0.25 }, "{name} is faint: mean {}", mean(values));
            assert!(mean(values) < 0.92, "{name} is pinned at full: mean {}", mean(values));
        }
        println!("level {:.2}..{:.2} (mean {:.2}), bass {:.2}..{:.2}, mid {:.2}..{:.2}, treble {:.2}..{:.2}",
            lowest(&levels), highest(&levels), mean(&levels), lowest(&basses), highest(&basses),
            lowest(&mids), highest(&mids), lowest(&trebles), highest(&trebles));
        // ...and it moves with the music.
        assert!(highest(&levels) - lowest(&levels) > 0.25, "the level meter barely moves");
        assert!(highest(&basses) - lowest(&basses) > 0.15, "the bass barely moves");
    }

    #[test]
    fn beats_follow_the_kick_drum() {
        for (bpm, expected) in [(120.0, 20), (90.0, 15), (150.0, 25)] {
            let song = track(12.0, bpm, 0.5);
            let mut a = Analyser::new();
            let mut start = 0;
            play(&mut a, &song, 0.0, 12.0, 1.0, |t, f| {
                if t < 2.0 {
                    start = f.beat_count;
                }
            });
            let counted = a.frame.beat_count - start;
            assert!(
                (counted as i32 - expected).abs() <= 2,
                "{bpm} BPM: counted {counted} beats in 10 s, expected about {expected}"
            );
        }
    }

    #[test]
    fn one_loud_bang_does_not_dim_what_follows() {
        // A door slam: 40 ms of full-scale noise on top of the music.
        let mut song = track(7.0, 120.0, 0.25);
        let slam = (4.0 * RATE as f32) as usize;
        for (i, s) in song[slam..slam + RATE as usize / 25].iter_mut().enumerate() {
            *s += noise(i);
        }
        let mut a = Analyser::new();
        let (mut before, mut after) = (vec![], vec![]);
        play(&mut a, &song, 0.0, 7.0, 1.0, |t, f| {
            if (3.0..4.0).contains(&t) {
                before.push(f.mid);
            } else if (5.5..6.5).contains(&t) {
                after.push(f.mid);
            }
        });
        assert!(
            mean(&after) > mean(&before) * 0.75,
            "still dimmed 1.5 s after a bang: {} vs {} before",
            mean(&after),
            mean(&before)
        );
    }

    #[test]
    fn quiet_and_loud_playback_look_alike() {
        let run = |volume: f32| {
            let song = track(6.0, 120.0, volume);
            let mut a = Analyser::new();
            let mut mids = vec![];
            play(&mut a, &song, 0.0, 6.0, 1.0, |t, f| {
                if t > 3.0 {
                    mids.push(f.mid);
                }
            });
            mean(&mids)
        };
        let (loud, quiet) = (run(0.9), run(0.05));
        assert!((loud - quiet).abs() < 0.08, "volume changes the picture: {loud} at full vs {quiet} turned down");
    }

    #[test]
    fn silence_and_hiss_stay_dark() {
        // Faint noise, about 70 dB down: a quiet room through a microphone.
        let hiss: Vec<f32> = (0..RATE as usize * 3 + FFT_SIZE).map(|n| noise(n) * 0.0003).collect();
        let mut a = Analyser::new();
        play(&mut a, &hiss, 0.0, 3.0, 1.0, |_, f| {
            assert!(f.bands.iter().all(|b| *b < 0.05), "hiss lit a band: {:?}", f.bands);
            assert!(f.level < 0.05, "hiss moved the level meter: {}", f.level);
        });
        assert_eq!(a.frame.beat_count, 0, "hiss is not a beat");

        // Music, then nothing arriving at all (loopback with playback paused).
        let song = track(3.0, 120.0, 0.5);
        play(&mut a, &song, 0.0, 3.0, 1.0, |_, _| {});
        assert!(a.frame.mid > 0.2);
        for _ in 0..90 {
            a.process(None, 1.0 / FPS, 1.0);
        }
        assert!(a.frame.bands.iter().all(|b| *b < 0.01));
        assert!(a.frame.level < 0.01 && a.frame.beat < 0.01);
    }

    #[test]
    fn the_gain_setting_brings_up_quiet_sound() {
        // Quieter than automatic gain will reach for on its own.
        let whisper = track(3.0, 120.0, 0.004);
        let (mut plain, mut boosted) = (Analyser::new(), Analyser::new());
        let (mut plain_bass, mut boosted_bass) = (vec![], vec![]);
        play(&mut plain, &whisper, 0.0, 3.0, 1.0, |_, f| plain_bass.push(f.bass));
        play(&mut boosted, &whisper, 0.0, 3.0, 4.0, |_, f| boosted_bass.push(f.bass));
        assert!(mean(&boosted_bass) > mean(&plain_bass) + 0.15, "{} vs {}", mean(&boosted_bass), mean(&plain_bass));
    }

    #[test]
    fn the_demo_beat_reaches_the_analyser() {
        let capture = AudioCapture::start();
        capture.set_source(DEMO_SOURCE);
        let mut a = Analyser::new();
        let mut loudest = 0.0f32;
        for _ in 0..50 {
            capture.request();
            std::thread::sleep(Duration::from_millis(16));
            loudest = loudest.max(a.update(&capture, 0.016, 1.0).level);
        }
        let status = capture.status();
        capture.stop();
        assert!(status.running && status.error.is_none(), "{status:?}");
        assert!(loudest > 0.3, "the demo beat should register: {loudest}");
    }

    #[test]
    fn only_real_failures_restart_the_stream() {
        use cpal::ErrorKind::*;
        for notice in [Xrun, RealtimeDenied, DeviceChanged] {
            assert!(!is_fatal(notice), "{notice:?} is only a notice; audio keeps flowing");
        }
        for failure in [DeviceNotAvailable, StreamInvalidated, BackendError] {
            assert!(is_fatal(failure), "{failure:?} means the stream is gone");
        }
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
