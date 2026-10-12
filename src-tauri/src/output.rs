//! Getting frames onto real hardware: wire protocols plus one worker thread
//! per device so a slow serial port can never stall the render loop.

use crate::model::{Connection, NetProtocol, SerialProtocol};
use parking_lot::{Condvar, Mutex};
use serde::Serialize;
use std::io::Write;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ----------------------------------------------------------------------------
// Protocol encoders. `rgb` is always three bytes per LED, already in the
// channel order the hardware expects.
// ----------------------------------------------------------------------------

/// Adalight: `"Ada"`, LED count minus one (big endian), checksum, pixels.
pub fn adalight(rgb: &[u8]) -> Vec<u8> {
    let n = (rgb.len() / 3).max(1) - 1;
    let (hi, lo) = ((n >> 8) as u8, n as u8);
    let mut out = Vec::with_capacity(rgb.len() + 6);
    out.extend_from_slice(b"Ada");
    out.extend_from_slice(&[hi, lo, hi ^ lo ^ 0x55]);
    out.extend_from_slice(rgb);
    out
}

/// TPM2 serial: start byte, data-frame type, payload size, pixels, end byte.
pub fn tpm2(rgb: &[u8]) -> Vec<u8> {
    let len = rgb.len().min(u16::MAX as usize);
    let mut out = Vec::with_capacity(len + 5);
    out.extend_from_slice(&[0xC9, 0xDA, (len >> 8) as u8, len as u8]);
    out.extend_from_slice(&rgb[..len]);
    out.push(0x36);
    out
}

/// DDP: 10-byte header, up to 480 LEDs per packet, "push" flag on the last.
pub fn ddp(rgb: &[u8], seq: u8) -> Vec<Vec<u8>> {
    const CHUNK: usize = 1440;
    let total = rgb.chunks(CHUNK).count();
    rgb.chunks(CHUNK)
        .enumerate()
        .map(|(i, chunk)| {
            let offset = (i * CHUNK) as u32;
            let last = i + 1 == total;
            let mut p = Vec::with_capacity(chunk.len() + 10);
            p.push(0x40 | u8::from(last)); // version 1, push on the final packet
            p.push(seq & 0x0F);
            p.push(0x0B); // RGB, 8 bits per channel
            p.push(0x01); // default output device
            p.extend_from_slice(&offset.to_be_bytes());
            p.extend_from_slice(&(chunk.len() as u16).to_be_bytes());
            p.extend_from_slice(chunk);
            p
        })
        .collect()
}

/// WLED realtime UDP: DRGB when everything fits in one packet, DNRGB (with a
/// start index) otherwise.
pub fn wled(rgb: &[u8]) -> Vec<Vec<u8>> {
    const TIMEOUT_S: u8 = 2;
    let leds = rgb.len() / 3;
    if leds <= 490 {
        let mut p = Vec::with_capacity(rgb.len() + 2);
        p.extend_from_slice(&[2, TIMEOUT_S]);
        p.extend_from_slice(rgb);
        return vec![p];
    }
    const PER: usize = 489;
    rgb.chunks(PER * 3)
        .enumerate()
        .map(|(i, chunk)| {
            let start = (i * PER) as u16;
            let mut p = Vec::with_capacity(chunk.len() + 4);
            p.extend_from_slice(&[4, TIMEOUT_S]);
            p.extend_from_slice(&start.to_be_bytes());
            p.extend_from_slice(chunk);
            p
        })
        .collect()
}

/// Art-Net ArtDmx: 170 LEDs (510 channels) per universe.
pub fn artnet(rgb: &[u8], first_universe: u16, seq: u8) -> Vec<Vec<u8>> {
    rgb.chunks(510)
        .enumerate()
        .map(|(i, chunk)| {
            let universe = first_universe.wrapping_add(i as u16) & 0x7FFF;
            // DMX payloads must have an even length.
            let len = chunk.len() + chunk.len() % 2;
            let mut p = Vec::with_capacity(18 + len);
            p.extend_from_slice(b"Art-Net\0");
            p.extend_from_slice(&[0x00, 0x50]); // OpDmx, little endian
            p.extend_from_slice(&[0, 14]); // protocol version
            p.push(seq);
            p.push(0); // physical port
            p.push(universe as u8);
            p.push((universe >> 8) as u8);
            p.extend_from_slice(&(len as u16).to_be_bytes());
            p.extend_from_slice(chunk);
            p.resize(18 + len, 0);
            p
        })
        .collect()
}

/// E1.31 (sACN) data packets: 170 LEDs per universe.
pub fn e131(rgb: &[u8], first_universe: u16, seq: u8) -> Vec<Vec<u8>> {
    const CID: [u8; 16] = *b"OpenLMI-sender01";
    rgb.chunks(510)
        .enumerate()
        .map(|(i, chunk)| {
            let universe = first_universe.max(1).wrapping_add(i as u16);
            let total = 126 + chunk.len();
            let flags_len = |from: usize| (0x7000u16 | (total - from) as u16).to_be_bytes();
            let mut p = Vec::with_capacity(total);
            // Root layer
            p.extend_from_slice(&[0x00, 0x10, 0x00, 0x00]);
            p.extend_from_slice(b"ASC-E1.17\0\0\0");
            p.extend_from_slice(&flags_len(16));
            p.extend_from_slice(&[0, 0, 0, 4]);
            p.extend_from_slice(&CID);
            // Framing layer
            p.extend_from_slice(&flags_len(38));
            p.extend_from_slice(&[0, 0, 0, 2]);
            let mut name = [0u8; 64];
            name[..7].copy_from_slice(b"OpenLMI");
            p.extend_from_slice(&name);
            p.push(100); // priority
            p.extend_from_slice(&[0, 0]); // no synchronisation universe
            p.push(seq);
            p.push(0); // options
            p.extend_from_slice(&universe.to_be_bytes());
            // DMP layer
            p.extend_from_slice(&flags_len(115));
            p.extend_from_slice(&[0x02, 0xA1, 0x00, 0x00, 0x00, 0x01]);
            p.extend_from_slice(&(chunk.len() as u16 + 1).to_be_bytes());
            p.push(0); // DMX start code
            p.extend_from_slice(chunk);
            p
        })
        .collect()
}

// ----------------------------------------------------------------------------
// Link status
// ----------------------------------------------------------------------------

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LinkState {
    Simulator,
    Connecting,
    Connected,
    /// UDP: frames are going out, but nothing confirms they arrive.
    Streaming,
    Error,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    pub state: LinkState,
    pub message: String,
    /// Frames actually written per second.
    pub fps: f32,
}

impl LinkStatus {
    pub fn simulator() -> Self {
        LinkStatus { state: LinkState::Simulator, message: "Preview only".into(), fps: 0.0 }
    }
}

// ----------------------------------------------------------------------------
// Worker
// ----------------------------------------------------------------------------

struct Slot {
    /// Only the newest frame is kept; stale ones are dropped.
    frame: Mutex<Option<Vec<u8>>>,
    ready: Condvar,
    stop: AtomicBool,
    status: Mutex<LinkStatus>,
}

pub struct Output {
    slot: Arc<Slot>,
}

impl Output {
    /// Returns `None` for the simulator, which needs no worker.
    pub fn open(conn: &Connection) -> Option<Output> {
        if matches!(conn, Connection::Simulator) {
            return None;
        }
        let slot = Arc::new(Slot {
            frame: Mutex::new(None),
            ready: Condvar::new(),
            stop: AtomicBool::new(false),
            status: Mutex::new(LinkStatus { state: LinkState::Connecting, message: "Connecting...".into(), fps: 0.0 }),
        });
        let worker = slot.clone();
        let conn = conn.clone();
        std::thread::Builder::new()
            .name("openlmi-output".into())
            .spawn(move || match conn {
                Connection::Serial { port, baud, protocol } => serial_worker(&worker, &port, baud, protocol),
                Connection::Network { host, port, protocol, universe } => {
                    network_worker(&worker, &host, port, protocol, universe)
                }
                Connection::Simulator => {}
            })
            .ok()?;
        Some(Output { slot })
    }

    pub fn send(&self, rgb: Vec<u8>) {
        *self.slot.frame.lock() = Some(rgb);
        self.slot.ready.notify_one();
    }

    pub fn status(&self) -> LinkStatus {
        self.slot.status.lock().clone()
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // The worker flushes any frame still waiting (usually the final
        // blackout) and then exits on its own; we never block on it.
        self.slot.stop.store(true, Ordering::Relaxed);
        self.slot.ready.notify_one();
    }
}

impl Slot {
    /// Blocks for the next frame. `None` means it is time to shut down.
    fn next(&self) -> Option<Vec<u8>> {
        let mut frame = self.frame.lock();
        loop {
            if let Some(f) = frame.take() {
                return Some(f);
            }
            if self.stop.load(Ordering::Relaxed) {
                return None;
            }
            self.ready.wait_for(&mut frame, Duration::from_millis(250));
        }
    }

    fn set(&self, state: LinkState, message: impl Into<String>) {
        let mut s = self.status.lock();
        s.state = state;
        s.message = message.into();
        if state == LinkState::Error {
            s.fps = 0.0;
        }
    }
}

struct FpsMeter {
    since: Instant,
    frames: u32,
}

impl FpsMeter {
    fn new() -> Self {
        FpsMeter { since: Instant::now(), frames: 0 }
    }
    fn tick(&mut self, slot: &Slot) {
        self.frames += 1;
        let elapsed = self.since.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            slot.status.lock().fps = self.frames as f32 / elapsed;
            self.frames = 0;
            self.since = Instant::now();
        }
    }
}

fn serial_worker(slot: &Slot, port_name: &str, baud: u32, protocol: SerialProtocol) {
    let mut port: Option<Box<dyn serialport::SerialPort>> = None;
    let mut retry_at = Instant::now();
    let mut meter = FpsMeter::new();

    while let Some(rgb) = slot.next() {
        if port.is_none() {
            if Instant::now() < retry_at {
                continue;
            }
            slot.set(LinkState::Connecting, format!("Opening {port_name}..."));
            match serialport::new(port_name, baud).timeout(Duration::from_millis(1500)).open() {
                Ok(mut p) => {
                    let _ = p.write_data_terminal_ready(true);
                    // Most Arduinos reboot when the port opens; give the
                    // bootloader time to hand over before talking to it.
                    std::thread::sleep(Duration::from_millis(1800));
                    port = Some(p);
                    slot.set(LinkState::Connected, format!("{port_name} @ {baud} baud"));
                }
                Err(e) => {
                    slot.set(LinkState::Error, friendly_serial_error(port_name, &e));
                    retry_at = Instant::now() + Duration::from_secs(2);
                    continue;
                }
            }
        }

        let packet = match protocol {
            SerialProtocol::Adalight => adalight(&rgb),
            SerialProtocol::Tpm2 => tpm2(&rgb),
        };
        let started = Instant::now();
        let result = port.as_mut().map(|p| p.write_all(&packet).and_then(|_| p.flush()));
        match result {
            Some(Ok(())) => {
                meter.tick(slot);
                // Don't queue more than the wire can carry, and leave the
                // microcontroller time to clock the pixels out: it cannot
                // listen to the serial port while it is doing that.
                let wire = Duration::from_secs_f64(packet.len() as f64 * 10.0 / baud.max(1) as f64);
                let latch = Duration::from_micros(rgb.len() as u64 / 3 * 30 + 500);
                if let Some(rest) = (wire + latch).checked_sub(started.elapsed()) {
                    std::thread::sleep(rest);
                }
            }
            Some(Err(e)) => {
                port = None;
                slot.set(LinkState::Error, format!("Lost {port_name}: {e}"));
                retry_at = Instant::now() + Duration::from_secs(1);
            }
            None => {}
        }
    }
}

fn friendly_serial_error(port: &str, e: &serialport::Error) -> String {
    match e.kind() {
        serialport::ErrorKind::NoDevice => format!("{port} not found. Is the board plugged in?"),
        serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) => {
            format!("{port} is in use by another program (close the Arduino serial monitor)")
        }
        _ => format!("Could not open {port}: {e}"),
    }
}

fn network_worker(slot: &Slot, host: &str, port: u16, protocol: NetProtocol, universe: u16) {
    let mut socket: Option<UdpSocket> = None;
    let mut retry_at = Instant::now();
    let mut meter = FpsMeter::new();
    let mut seq: u8 = 0;

    while let Some(rgb) = slot.next() {
        if socket.is_none() {
            if Instant::now() < retry_at {
                continue;
            }
            let opened = UdpSocket::bind("0.0.0.0:0").and_then(|s| {
                s.connect((host, port))?;
                Ok(s)
            });
            match opened {
                Ok(s) => {
                    socket = Some(s);
                    slot.set(LinkState::Streaming, format!("Streaming to {host}:{port}"));
                }
                Err(e) => {
                    slot.set(LinkState::Error, format!("Cannot reach {host}:{port}: {e}"));
                    retry_at = Instant::now() + Duration::from_secs(3);
                    continue;
                }
            }
        }

        seq = seq.wrapping_add(1);
        let packets = match protocol {
            // DDP sequence numbers run 1..=15; 0 means "not used".
            NetProtocol::Ddp => ddp(&rgb, seq % 15 + 1),
            NetProtocol::Wled => wled(&rgb),
            NetProtocol::Artnet => artnet(&rgb, universe, seq),
            NetProtocol::E131 => e131(&rgb, universe, seq),
        };
        let sock = socket.as_ref().expect("socket opened above");
        match packets.iter().try_for_each(|p| sock.send(p).map(|_| ())) {
            Ok(()) => {
                meter.tick(slot);
                if slot.status.lock().state != LinkState::Streaming {
                    slot.set(LinkState::Streaming, format!("Streaming to {host}:{port}"));
                }
            }
            Err(e) => {
                // UDP reports unreachable hosts on a later send; keep trying
                // with the same socket unless it keeps failing.
                slot.set(LinkState::Error, format!("{host}:{port} is not answering: {e}"));
                socket = None;
                retry_at = Instant::now() + Duration::from_secs(1);
            }
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub name: String,
    pub label: String,
}

pub fn list_serial_ports() -> Vec<PortInfo> {
    let mut ports: Vec<PortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let detail = match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => usb
                    .product
                    .clone()
                    .or_else(|| usb.manufacturer.clone())
                    .unwrap_or_else(|| format!("USB {:04x}:{:04x}", usb.vid, usb.pid)),
                serialport::SerialPortType::BluetoothPort => "Bluetooth".into(),
                serialport::SerialPortType::PciPort => "Built-in".into(),
                serialport::SerialPortType::Unknown => String::new(),
            };
            let label = if detail.is_empty() { p.port_name.clone() } else { format!("{} - {}", p.port_name, detail) };
            PortInfo { name: p.port_name, label }
        })
        .collect();
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adalight_header() {
        let p = adalight(&[1, 2, 3, 4, 5, 6]);
        // Two LEDs -> count field is 1.
        assert_eq!(&p[..6], &[b'A', b'd', b'a', 0, 1, 0 ^ 1 ^ 0x55]);
        assert_eq!(&p[6..], &[1, 2, 3, 4, 5, 6]);

        let big = adalight(&vec![0; 300 * 3]);
        assert_eq!((big[3], big[4]), (0x01, 0x2B)); // 299
        assert_eq!(big[5], 0x01 ^ 0x2B ^ 0x55);
    }

    #[test]
    fn tpm2_frame() {
        let p = tpm2(&[9, 8, 7]);
        assert_eq!(p, vec![0xC9, 0xDA, 0, 3, 9, 8, 7, 0x36]);
    }

    #[test]
    fn ddp_splits_and_pushes_last() {
        let rgb = vec![7u8; 1000 * 3];
        let packets = ddp(&rgb, 5);
        assert_eq!(packets.len(), 3);
        assert_eq!(packets[0][0], 0x40);
        assert_eq!(packets[2][0], 0x41);
        assert_eq!(&packets[1][4..8], &1440u32.to_be_bytes());
        assert_eq!(&packets[0][8..10], &1440u16.to_be_bytes());
        assert_eq!(&packets[2][8..10], &120u16.to_be_bytes());
        let payload: usize = packets.iter().map(|p| p.len() - 10).sum();
        assert_eq!(payload, 3000);
    }

    #[test]
    fn wled_picks_the_right_mode() {
        assert_eq!(wled(&[0u8; 30])[0][0], 2);
        let big = wled(&vec![0u8; 1000 * 3]);
        assert_eq!(big.len(), 3);
        assert!(big.iter().all(|p| p[0] == 4));
        assert_eq!(&big[1][2..4], &489u16.to_be_bytes());
    }

    #[test]
    fn artnet_packet_layout() {
        let packets = artnet(&vec![1u8; 171 * 3], 3, 9);
        assert_eq!(packets.len(), 2);
        let p = &packets[0];
        assert_eq!(&p[..8], b"Art-Net\0");
        assert_eq!(&p[8..10], &[0x00, 0x50]);
        assert_eq!(p[12], 9);
        assert_eq!((p[14], p[15]), (3, 0));
        assert_eq!(&p[16..18], &510u16.to_be_bytes());
        assert_eq!(p.len(), 18 + 510);
        // The second universe carries one LED, padded to an even length.
        assert_eq!(packets[1][14], 4);
        assert_eq!(&packets[1][16..18], &4u16.to_be_bytes());
    }

    #[test]
    fn e131_packet_layout() {
        let p = &e131(&vec![5u8; 170 * 3], 1, 2)[0];
        assert_eq!(p.len(), 126 + 510);
        assert_eq!(&p[4..13], b"ASC-E1.17");
        assert_eq!(&p[113..115], &1u16.to_be_bytes());
        assert_eq!(p[117], 0x02);
        assert_eq!(&p[123..125], &511u16.to_be_bytes());
        assert_eq!(p[125], 0);
        assert_eq!(p[126], 5);
        // Root-layer length covers everything after the 16-byte preamble.
        assert_eq!(u16::from_be_bytes([p[16], p[17]]) & 0x0FFF, (p.len() - 16) as u16);
    }
}
