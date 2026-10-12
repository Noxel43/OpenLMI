//! Generates ready-to-upload Arduino sketches: the serial receiver that pairs
//! with OpenLMI, and standalone players for animations made in the editor.

use crate::model::Animation;
use base64::Engine as _;
use serde::Deserialize;

// Chipsets whose FastLED template takes (pin, colour order), plus the clocked
// ones below. NEOPIXEL is deliberately absent: it takes no colour order.
const CHIPSETS: &[&str] = &["WS2812B", "WS2812", "WS2811", "WS2813", "WS2815", "SK6812", "APA102", "SK9822", "WS2801"];
const CLOCKED: &[&str] = &["APA102", "SK9822", "WS2801"];
const ORDERS: &[&str] = &["GRB", "RGB", "BRG", "RBG", "GBR", "BGR"];

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FirmwareOpts {
    pub chipset: String,
    pub data_pin: u8,
    /// Only used by clocked chipsets such as APA102.
    pub clock_pin: u8,
    pub count: u32,
    pub color_order: String,
    pub baud: u32,
    /// 0 disables the limit.
    pub max_milliamps: u32,
    /// Seconds without data before the LEDs switch off; 0 keeps the last frame.
    pub idle_timeout_s: u32,
}

impl Default for FirmwareOpts {
    fn default() -> Self {
        FirmwareOpts {
            chipset: "WS2812B".into(),
            data_pin: 6,
            clock_pin: 7,
            count: 60,
            color_order: "GRB".into(),
            baud: 500_000,
            max_milliamps: 0,
            idle_timeout_s: 5,
        }
    }
}

impl FirmwareOpts {
    /// Falls back to safe values so nothing odd can end up in the sketch.
    fn clean(&self) -> FirmwareOpts {
        let d = FirmwareOpts::default();
        FirmwareOpts {
            chipset: if CHIPSETS.contains(&self.chipset.as_str()) { self.chipset.clone() } else { d.chipset },
            color_order: if ORDERS.contains(&self.color_order.as_str()) { self.color_order.clone() } else { d.color_order },
            count: self.count.clamp(1, 4096),
            baud: if self.baud >= 9600 { self.baud } else { d.baud },
            ..self.clone()
        }
    }

    fn add_leds(&self) -> String {
        if CLOCKED.contains(&self.chipset.as_str()) {
            "FastLED.addLeds<LED_TYPE, DATA_PIN, CLOCK_PIN, COLOR_ORDER>(leds, NUM_LEDS);".into()
        } else {
            "FastLED.addLeds<LED_TYPE, DATA_PIN, COLOR_ORDER>(leds, NUM_LEDS);".into()
        }
    }

    fn clock_define(&self) -> String {
        if CLOCKED.contains(&self.chipset.as_str()) {
            format!("#define CLOCK_PIN       {:<9} // pin wired to the strip's clock input (CI / CLK)\n", self.clock_pin)
        } else {
            String::new()
        }
    }
}

/// The receiver sketch: the board shows whatever OpenLMI sends over USB.
pub fn receiver_sketch(opts: &FirmwareOpts) -> String {
    let o = opts.clean();
    RECEIVER
        .replace("{{NUM_LEDS}}", &format!("{:<9}", o.count))
        .replace("{{DATA_PIN}}", &format!("{:<9}", o.data_pin))
        .replace("{{CLOCK_DEFINE}}", &o.clock_define())
        .replace("{{LED_TYPE}}", &o.chipset)
        .replace("{{COLOR_ORDER}}", &o.color_order)
        .replace("{{BAUD_PADDED}}", &format!("{:<9}", o.baud))
        .replace("{{BAUD}}", &o.baud.to_string())
        .replace("{{MAX_MILLIAMPS}}", &format!("{:<9}", o.max_milliamps))
        .replace("{{IDLE_MS}}", &format!("{:<9}", o.idle_timeout_s.min(3600) * 1000))
        .replace("{{ADD_LEDS}}", &o.add_leds())
}

const RECEIVER: &str = r#"/*
 * OpenLMI firmware - serial LED receiver
 * https://github.com/Noxel43/OpenLMI
 *
 * Makes an Arduino-compatible board show whatever OpenLMI sends over USB.
 * It speaks the Adalight protocol, so other Adalight software works too.
 *
 * Needs the FastLED library:
 *   Arduino IDE > Tools > Manage Libraries... > search "FastLED" > Install
 *
 * Then, in OpenLMI, add a device with
 *   Connection: Serial (USB)   Protocol: Adalight   Baud rate: {{BAUD}}
 */

#include <FastLED.h>

// ---- Settings ---------------------------------------------------------------
#define NUM_LEDS        {{NUM_LEDS}} // how many LEDs are connected
#define DATA_PIN        {{DATA_PIN}} // pin wired to the strip's data input (DIN)
{{CLOCK_DEFINE}}#define LED_TYPE        {{LED_TYPE}}
#define COLOR_ORDER     {{COLOR_ORDER}}
#define SERIAL_BAUD     {{BAUD_PADDED}} // must match the baud rate set in OpenLMI
#define MAX_MILLIAMPS   {{MAX_MILLIAMPS}} // power limit for a 5 V supply, 0 = no limit
#define IDLE_TIMEOUT_MS {{IDLE_MS}} // go dark this long after the PC stops sending, 0 = never
// -----------------------------------------------------------------------------

CRGB leds[NUM_LEDS];

enum State : uint8_t { HEADER_A, HEADER_D, HEADER_A2, COUNT_HI, COUNT_LO, CHECKSUM, PIXELS };

State state = HEADER_A;
uint8_t countHi = 0;
uint8_t countLo = 0;
uint32_t expected = 0;    // bytes of pixel data in the frame being received
uint32_t received = 0;
uint32_t lastByteAt = 0;
bool dark = true;

void setup() {
  {{ADD_LEDS}}
#if MAX_MILLIAMPS > 0
  FastLED.setMaxPowerInVoltsAndMilliamps(5, MAX_MILLIAMPS);
#endif
  FastLED.clear(true);

  Serial.begin(SERIAL_BAUD);
  Serial.print("Ada\n");  // tells the PC the board is ready
}

void loop() {
  while (Serial.available() > 0) {
    uint8_t b = Serial.read();
    lastByteAt = millis();

    switch (state) {
      // Every frame starts with the three characters "Ada".
      case HEADER_A:
        state = (b == 'A') ? HEADER_D : HEADER_A;
        break;
      case HEADER_D:
        state = (b == 'd') ? HEADER_A2 : (b == 'A') ? HEADER_D : HEADER_A;
        break;
      case HEADER_A2:
        state = (b == 'a') ? COUNT_HI : (b == 'A') ? HEADER_D : HEADER_A;
        break;

      // Then the LED count minus one, and a checksum of those two bytes.
      case COUNT_HI:
        countHi = b;
        state = COUNT_LO;
        break;
      case COUNT_LO:
        countLo = b;
        state = CHECKSUM;
        break;
      case CHECKSUM:
        if (b == (countHi ^ countLo ^ 0x55)) {
          expected = ((uint32_t)(((uint16_t)countHi << 8) | countLo) + 1) * 3;
          received = 0;
          state = PIXELS;
        } else {
          state = HEADER_A;
        }
        break;

      // Then red, green, blue for each LED.
      case PIXELS: {
        uint32_t led = received / 3;
        if (led < NUM_LEDS) {
          leds[led].raw[received % 3] = b;
        }
        received++;
        if (received >= expected) {
          FastLED.show();
          dark = false;
          state = HEADER_A;
        }
        break;
      }
    }
  }

  // A frame that stalls half way is abandoned so the next one can sync up.
  if (state != HEADER_A && millis() - lastByteAt > 250) {
    state = HEADER_A;
  }

#if IDLE_TIMEOUT_MS > 0
  if (!dark && millis() - lastByteAt > IDLE_TIMEOUT_MS) {
    FastLED.clear(true);
    dark = true;
  }
#endif
}
"#;

/// A self-contained sketch that plays an animation with no PC attached.
pub fn animation_sketch(anim: &Animation, opts: &FirmwareOpts) -> String {
    let o = opts.clean();
    let n = o.count as usize;
    let frames: Vec<Vec<u8>> = anim
        .frames
        .iter()
        .map(|f| {
            let src = base64::engine::general_purpose::STANDARD.decode(&f.pixels).unwrap_or_default();
            let src_leds = src.len() / 3;
            let mut out = vec![0u8; n * 3];
            if src_leds > 0 {
                for i in 0..n {
                    // Stretch frames painted for a different LED count.
                    let s = (i * src_leds / n).min(src_leds - 1) * 3;
                    out[i * 3..i * 3 + 3].copy_from_slice(&src[s..s + 3]);
                }
            }
            out
        })
        .collect();

    let join = |items: Vec<String>| items.join(", ");
    let durations = join(anim.frames.iter().map(|f| f.duration_ms.clamp(10, 65_535).to_string()).collect());
    let fades = join(anim.frames.iter().map(|f| u8::from(f.fade).to_string()).collect());
    let data = frames
        .iter()
        .map(|f| {
            let rows: Vec<String> =
                f.chunks(24).map(|row| format!("    {}", join(row.iter().map(|b| format!("0x{b:02X}")).collect()))).collect();
            format!("  {{\n{}\n  }}", rows.join(",\n"))
        })
        .collect::<Vec<_>>()
        .join(",\n");

    let name: String = anim.name.chars().filter(|c| !c.is_control() && *c != '*' && *c != '/').collect();
    PLAYER
        .replace("{{NAME}}", &name)
        .replace("{{BYTES}}", &(frames.len() * n * 3).to_string())
        .replace("{{NUM_LEDS}}", &format!("{:<9}", o.count))
        .replace("{{DATA_PIN}}", &format!("{:<9}", o.data_pin))
        .replace("{{CLOCK_DEFINE}}", &o.clock_define())
        .replace("{{LED_TYPE}}", &o.chipset)
        .replace("{{COLOR_ORDER}}", &o.color_order)
        .replace("{{MAX_MILLIAMPS}}", &format!("{:<9}", o.max_milliamps))
        .replace("{{FRAME_COUNT}}", &format!("{:<9}", frames.len().max(1)))
        .replace("{{LOOP}}", if anim.looped { "1" } else { "0" })
        .replace("{{DURATIONS}}", if frames.is_empty() { "1000" } else { &durations })
        .replace("{{FADES}}", if frames.is_empty() { "0" } else { &fades })
        .replace("{{FRAMES}}", if frames.is_empty() { "  { 0 }" } else { &data })
        .replace("{{ADD_LEDS}}", &o.add_leds())
}

const PLAYER: &str = r#"/*
 * OpenLMI standalone animation: {{NAME}}
 * https://github.com/Noxel43/OpenLMI
 *
 * Exported from the OpenLMI animation editor. Upload it and the board plays
 * the animation by itself - no PC needed.
 *
 * Needs the FastLED library (Tools > Manage Libraries... > "FastLED").
 * Frame data uses {{BYTES}} bytes of flash; an Arduino Uno has roughly 30,000.
 */

#include <FastLED.h>

#define NUM_LEDS        {{NUM_LEDS}}
#define DATA_PIN        {{DATA_PIN}}
{{CLOCK_DEFINE}}#define LED_TYPE        {{LED_TYPE}}
#define COLOR_ORDER     {{COLOR_ORDER}}
#define BRIGHTNESS      200       // 0-255
#define MAX_MILLIAMPS   {{MAX_MILLIAMPS}} // power limit for a 5 V supply, 0 = no limit
#define FRAME_COUNT     {{FRAME_COUNT}}
#define LOOP_ANIMATION  {{LOOP}}

// How long each frame is shown, in milliseconds.
const uint16_t durations[FRAME_COUNT] PROGMEM = { {{DURATIONS}} };
// 1 = cross-fade into the next frame, 0 = hard cut.
const uint8_t fades[FRAME_COUNT] PROGMEM = { {{FADES}} };
// Red, green, blue for every LED of every frame.
const uint8_t frames[FRAME_COUNT][NUM_LEDS * 3] PROGMEM = {
{{FRAMES}}
};

CRGB leds[NUM_LEDS];

CRGB pixelAt(uint16_t frame, uint16_t led) {
  const uint8_t *p = frames[frame] + led * 3;
  return CRGB(pgm_read_byte(p), pgm_read_byte(p + 1), pgm_read_byte(p + 2));
}

void showFrame(uint16_t frame, uint16_t next, uint8_t amount) {
  for (uint16_t i = 0; i < NUM_LEDS; i++) {
    CRGB c = pixelAt(frame, i);
    leds[i] = amount > 0 ? blend(c, pixelAt(next, i), amount) : c;
  }
  FastLED.show();
}

void setup() {
  {{ADD_LEDS}}
  FastLED.setBrightness(BRIGHTNESS);
#if MAX_MILLIAMPS > 0
  FastLED.setMaxPowerInVoltsAndMilliamps(5, MAX_MILLIAMPS);
#endif
}

void loop() {
  static uint16_t frame = 0;
  static uint32_t startedAt = 0;

  uint16_t duration = pgm_read_word(&durations[frame]);
  uint32_t elapsed = millis() - startedAt;

  if (elapsed >= duration) {
    if (frame + 1 < FRAME_COUNT) {
      frame++;
    } else if (LOOP_ANIMATION) {
      frame = 0;
    } else {
      showFrame(frame, frame, 0);  // hold the last frame
      delay(50);
      return;
    }
    startedAt = millis();
    elapsed = 0;
  }

  uint16_t next = frame + 1 < FRAME_COUNT ? frame + 1 : (LOOP_ANIMATION ? 0 : frame);
  uint8_t amount = pgm_read_byte(&fades[frame]) ? (uint8_t)(elapsed * 255UL / duration) : 0;
  showFrame(frame, next, amount);
  delay(10);
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AnimFrame;

    #[test]
    fn receiver_uses_the_chosen_settings() {
        let sketch = receiver_sketch(&FirmwareOpts { count: 144, data_pin: 5, baud: 115_200, ..Default::default() });
        assert!(sketch.contains("#define NUM_LEDS        144"));
        assert!(sketch.contains("#define DATA_PIN        5"));
        assert!(sketch.contains("#define SERIAL_BAUD     115200"));
        assert!(sketch.contains("Baud rate: 115200"));
        assert!(sketch.contains("addLeds<LED_TYPE, DATA_PIN, COLOR_ORDER>"));
        assert!(!sketch.contains("CLOCK_PIN"));
        assert!(!sketch.contains("{{"), "a placeholder was left unfilled");
    }

    #[test]
    fn clocked_chipsets_get_a_clock_pin() {
        let sketch = receiver_sketch(&FirmwareOpts { chipset: "APA102".into(), clock_pin: 13, ..Default::default() });
        assert!(sketch.contains("#define CLOCK_PIN       13"));
        assert!(sketch.contains("addLeds<LED_TYPE, DATA_PIN, CLOCK_PIN, COLOR_ORDER>"));
    }

    #[test]
    fn unknown_values_cannot_inject_code() {
        let sketch = receiver_sketch(&FirmwareOpts {
            chipset: "WS2812B\n#include <evil.h>".into(),
            color_order: "GRB; system(1)".into(),
            ..Default::default()
        });
        assert!(!sketch.contains("evil"));
        assert!(!sketch.contains("system"));
        assert!(sketch.contains("#define LED_TYPE        WS2812B\n"));
    }

    /// The copy in the repository must be exactly what the app generates.
    /// Run with `UPDATE_FIRMWARE=1` to refresh it after changing the template.
    #[test]
    fn checked_in_sketch_is_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../firmware/OpenLMI_Receiver/OpenLMI_Receiver.ino");
        let expected = receiver_sketch(&FirmwareOpts::default());
        if std::env::var_os("UPDATE_FIRMWARE").is_some() {
            std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap()).unwrap();
            std::fs::write(path, &expected).unwrap();
        }
        let on_disk = std::fs::read_to_string(path).unwrap_or_default().replace("\r\n", "\n");
        assert_eq!(on_disk, expected, "firmware/OpenLMI_Receiver is stale; rerun with UPDATE_FIRMWARE=1");
    }

    #[test]
    fn animation_export_embeds_the_frames() {
        let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let anim = Animation {
            id: "a".into(),
            name: "Blink */ evil".into(),
            count: 2,
            looped: true,
            frames: vec![
                AnimFrame { duration_ms: 250, fade: true, pixels: b64(&[255, 0, 0, 0, 255, 0]) },
                AnimFrame { duration_ms: 500, fade: false, pixels: b64(&[0, 0, 255, 16, 32, 48]) },
            ],
        };
        let sketch = animation_sketch(&anim, &FirmwareOpts { count: 2, ..Default::default() });
        assert!(sketch.contains("#define FRAME_COUNT     2"));
        assert!(sketch.contains("PROGMEM = { 250, 500 };"));
        assert!(sketch.contains("PROGMEM = { 1, 0 };"));
        assert!(sketch.contains("0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00"));
        assert!(sketch.contains("0x00, 0x00, 0xFF, 0x10, 0x20, 0x30"));
        assert!(sketch.contains("uses 12 bytes"));
        assert!(!sketch.contains("*/ evil"), "the name must not be able to close the comment");
        assert!(!sketch.contains("{{"));

        // Exporting for a longer strip stretches the frame.
        let wide = animation_sketch(&anim, &FirmwareOpts { count: 4, ..Default::default() });
        assert!(wide.contains("0xFF, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0x00"));
    }
}
