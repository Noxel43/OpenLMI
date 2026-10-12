# Scripting reference

The **Code** tab runs scripts written in [Rhai](https://rhai.rs/book/), a small language that looks a little like JavaScript and Rust. You do not need it for anything - every other feature of OpenLMI works without code - but it lets you write effects of your own.

Scripts are sandboxed. They cannot touch files, the network or anything else on your computer; all they can do is colour LEDs. A script that loops forever is stopped automatically.

Edits are applied as you type. If the script has an error, the message appears under the editor and the last working version keeps running on the LEDs.

## The shape of a script

Define one or both of these functions.

### `fn pixel(i, x, y, z, t)` - colour each LED

Called for every LED on every frame. Whatever colour it returns is what that LED shows.

| Argument | Meaning |
| --- | --- |
| `i` | LED number, starting at 0 |
| `x`, `y`, `z` | Where the LED is, each from `0.0` to `1.0`. `x` runs left to right, `y` top to bottom, `z` front to back. On a strip only `x` changes. |
| `t` | Seconds since the script started |

```rust
fn pixel(i, x, y, z, t) {
    hsv(x + t * 0.2, 1.0, 1.0)
}
```

There is a short form, `fn pixel(i, t)`, for when the LED number is all you need.

### `fn frame(t, dt)` - draw a whole frame

Called once per frame, before `pixel`. Use the drawing functions below. LEDs keep their colour from one frame to the next, which is what makes trails and build-ups possible.

| Argument | Meaning |
| --- | --- |
| `t` | Seconds since the script started |
| `dt` | Seconds since the previous frame (about `0.016`) |

```rust
fn frame(t, dt) {
    fade(0.92);
    set(rand_int(0, led_count() - 1), hsv(rand(), 1.0, 1.0));
}
```

### `fn init()` - remembering things (optional)

Functions cannot see variables declared outside them. To keep values between frames, return a map from `init()`; it is then available as `this` inside `frame` and `pixel`.

```rust
fn init() {
    #{ position: 0.0, hue: 0.0 }
}

fn frame(t, dt) {
    this.position += dt * 30.0;
    this.hue += dt * 0.1;
    if this.position >= led_count() { this.position = 0.0; }
    fade(0.85);
    set(this.position.to_int(), hsv(this.hue, 1.0, 1.0));
}
```

## Colours

| Function | Result |
| --- | --- |
| `hsv(h, s, v)` | Hue, saturation and brightness, each `0.0`-`1.0`. Hue wraps, so `hsv(1.3, 1, 1)` equals `hsv(0.3, 1, 1)`. |
| `rgb(r, g, b)` | Red, green and blue, each `0`-`255`. |
| `hex("#ff8800")` | A colour from a hex code. |
| `palette("Sunset", t)` | A colour from a built-in palette. `t` wraps at `1.0`, and the palette loops seamlessly. |
| `mix(a, b, t)` | Blend from colour `a` (`t = 0`) to colour `b` (`t = 1`). |
| `dim(c, amount)` | Colour `c` scaled in brightness; `dim(c, 0.5)` is half as bright. |
| `add(a, b)` | Two colours added together. |

Whole numbers and decimals can be mixed freely: `hsv(0, 1, 1)` and `hsv(0.0, 1.0, 1.0)` are the same.

Palette names: Rainbow, Sunset, Ocean, Forest, Lava, Fire, Ice, Aurora, Cyberpunk, Candy, Party, Synthwave, Mint, Warm White, Matrix, Meter, Monochrome.

## Drawing

For use inside `frame`.

| Function | What it does |
| --- | --- |
| `set(i, c)` | Set LED `i` to colour `c`. Out-of-range numbers are ignored. |
| `get(i)` | The colour LED `i` currently has. |
| `fill(c)` | Set every LED. |
| `fade(amount)` | Multiply every LED's brightness; `fade(0.9)` dims by 10% per frame. |
| `shift(n)` | Slide all colours along by `n` LEDs, wrapping round. |
| `set_xy(x, y, c)` | Set the LED at a matrix cell. Follows the wiring you configured. |
| `set_xyz(x, y, z, c)` | The same for cubes. |
| `index_of(x, y)` / `index_of(x, y, z)` | The LED number at a cell, or `-1` if there is none. |

## Layout

| Function | Result |
| --- | --- |
| `led_count()` | Number of LEDs. |
| `width()`, `height()`, `depth()` | Grid size of a matrix or cube. For other shapes, `width()` is the LED count and the others are `1`. |
| `pos_x(i)`, `pos_y(i)`, `pos_z(i)` | Position of LED `i`, `0.0`-`1.0`. |

## Maths

| Function | Result |
| --- | --- |
| `wave(x)` | A smooth wave between 0 and 1 that repeats every time `x` grows by 1. |
| `tri(x)` | A triangle wave: 0 up to 1 and back, repeating every 1. |
| `noise(x)`, `noise(x, y)`, `noise(x, y, z)` | Smooth random values between 0 and 1. Nearby inputs give nearby outputs. |
| `rand()` | A random decimal from 0 to 1. |
| `rand_int(a, b)` | A random whole number from `a` to `b` inclusive. |
| `lerp(a, b, t)` | The value `t` of the way from `a` to `b`. |
| `smoothstep(a, b, x)` | 0 below `a`, 1 above `b`, a smooth ramp between. |
| `fract(x)` | The fractional part of `x`. |
| `clamp01(x)` | `x` limited to the range 0-1. |

Rhai's own maths is available too: `sin`, `cos`, `sqrt`, `abs`, `floor`, `min`, `max`, `PI()` and so on. See the [Rhai book](https://rhai.rs/book/language/num-fn.html).

## Audio

Using any of these turns on audio capture for the device. Choose the input in the **Audio** tab.

| Function | Result |
| --- | --- |
| `audio_level()` | Overall loudness, 0-1. |
| `audio_bass()`, `audio_mid()`, `audio_treble()` | Loudness of each part of the spectrum, 0-1. |
| `audio_band(f)` | Loudness at a point across the spectrum: `f = 0.0` is deep bass, `1.0` is high treble. |
| `audio_beat()` | Jumps to 1 on a beat, then falls back towards 0. |

## Examples

### Fire that follows the height of the layout

```rust
fn pixel(i, x, y, z, t) {
    let heat = noise(x * 4.0, y * 3.0 + t * 1.5) * 1.4 - (1.0 - y) * 0.9;
    palette("Fire", clamp01(heat) * 0.8)
}
```

### Bouncing dot

```rust
fn init() { #{ pos: 0.0, speed: 30.0 } }

fn frame(t, dt) {
    this.pos += this.speed * dt;
    if this.pos >= led_count() - 1 || this.pos <= 0 {
        this.speed = -this.speed;
    }
    fade(0.8);
    set(this.pos.to_int(), hsv(t * 0.05, 1.0, 1.0));
}
```

### Music bars on a matrix

```rust
fn pixel(i, x, y, z, t) {
    let level = audio_band(x);
    if 1.0 - y <= level { hsv(x * 0.8, 1.0, 1.0) } else { rgb(0, 0, 0) }
}
```

### Checkerboard that swaps on the beat

```rust
fn init() { #{ flip: 0 } }

fn frame(t, dt) {
    if audio_beat() > 0.9 { this.flip = 1 - this.flip; }
    for y in 0..height() {
        for x in 0..width() {
            let on = (x + y + this.flip) % 2 == 0;
            set_xy(x, y, if on { hex("#ff00c8") } else { hex("#00e5ff") });
        }
    }
}
```

## Tips

- Returning anything other than a colour from `pixel` is an error; end the function with `hsv(...)`, `rgb(...)` or another colour.
- `to_int()` turns a decimal into a whole number, which `set` needs: `set(this.pos.to_int(), c)`.
- `pixel` is called once per LED per frame, so keep it light on very large layouts. For a few thousand LEDs, `frame` with explicit loops is often the better fit.
- More starting points are under *New from example* in the Code tab.
