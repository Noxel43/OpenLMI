/*
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
 *   Connection: Serial (USB)   Protocol: Adalight   Baud rate: 500000
 */

#include <FastLED.h>

// ---- Settings ---------------------------------------------------------------
#define NUM_LEDS        60        // how many LEDs are connected
#define DATA_PIN        6         // pin wired to the strip's data input (DIN)
#define LED_TYPE        WS2812B
#define COLOR_ORDER     GRB
#define SERIAL_BAUD     500000    // must match the baud rate set in OpenLMI
#define MAX_MILLIAMPS   0         // power limit for a 5 V supply, 0 = no limit
#define IDLE_TIMEOUT_MS 5000      // go dark this long after the PC stops sending, 0 = never
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
  FastLED.addLeds<LED_TYPE, DATA_PIN, COLOR_ORDER>(leds, NUM_LEDS);
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
