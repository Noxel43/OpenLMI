import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Brush,
  ChevronLeft,
  ChevronRight,
  Copy,
  Download,
  Eraser,
  ImagePlus,
  PaintBucket,
  Pipette,
  Play,
  Plus,
  Square,
  Trash2,
  Undo2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { useApp } from "../store";
import type { Animation, Device, LayoutPreview } from "../types";
import { base64ToBytes, bytesToBase64, clamp, frameThrottle, hexToRgb, rgbToHex, samplePalette, type RGB } from "../util";
import { FrameThumb, resamplePixels } from "./frames";
import { ColorPicker, PaletteField } from "./ColorPicker";
import { LedCanvas, type PaintPhase } from "./LedCanvas";
import { Button, Field, NumberInput, Segmented, Slider, Toggle } from "./ui";

interface EditFrame {
  durationMs: number;
  fade: boolean;
  px: Uint8Array;
}

type Tool = "brush" | "eraser" | "picker";

const MAX_FRAMES = 400;

function decode(animation: Animation, count: number): EditFrame[] {
  const frames = animation.frames.map((f) => ({
    durationMs: f.durationMs,
    fade: f.fade,
    px: resamplePixels(base64ToBytes(f.pixels), count),
  }));
  return frames.length ? frames : [{ durationMs: 200, fade: false, px: new Uint8Array(count * 3) }];
}

interface Props {
  device: Device;
  layout: LayoutPreview;
  animation: Animation;
  onClose: () => void;
}

export function AnimationEditor({ device, layout, animation, onClose }: Props) {
  const n = layout.count;
  const saveAnimation = useApp((s) => s.saveAnimation);
  const setSource = useApp((s) => s.setSource);
  const openDialog = useApp((s) => s.openDialog);
  const toast = useApp((s) => s.toast);
  const palettes = useApp((s) => s.palettes);

  const [name, setName] = useState(animation.name);
  const [looped, setLooped] = useState(animation.looped);
  const [frames, setFrames] = useState<EditFrame[]>(() => decode(animation, n));
  const [cur, setCur] = useState(0);
  const [tool, setTool] = useState<Tool>("brush");
  const [color, setColor] = useState("#ff3b6b");
  const [brush, setBrush] = useState(1);
  const [playing, setPlaying] = useState(false);
  const [layer, setLayer] = useState(0);
  const [gradient, setGradient] = useState<string[]>(() => palettes.find((p) => p.name === "Sunset")?.colors ?? ["#ff0000", "#0000ff"]);

  const undoStack = useRef<{ frames: EditFrame[]; cur: number }[]>([]);
  const dirty = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const index = Math.min(cur, frames.length - 1);
  const frame = frames[index];
  const isCube = layout.kind === "cube";
  const isMatrix = layout.kind === "matrix";

  // --- saving ---------------------------------------------------------------

  const persist = useCallback(() => {
    if (!dirty.current) return;
    dirty.current = false;
    saveAnimation({
      id: animation.id,
      name: name.trim() || "Untitled",
      count: n,
      looped,
      frames: frames.map((f) => ({ durationMs: f.durationMs, fade: f.fade, pixels: bytesToBase64(f.px) })),
    });
  }, [animation.id, frames, looped, n, name, saveAnimation]);

  const persistRef = useRef(persist);
  persistRef.current = persist;

  useEffect(() => {
    if (!dirty.current) return;
    const timer = setTimeout(persist, 500);
    return () => clearTimeout(timer);
  }, [persist]);

  // On the way out: save, and hand the LEDs back to whatever was playing.
  useEffect(
    () => () => {
      persistRef.current();
      void api.paint(device.id, null);
    },
    [device.id],
  );

  // --- showing the frame on the real LEDs while painting ---------------------

  const sendPaint = useMemo(() => frameThrottle((id: string, pixels: string | null) => void api.paint(id, pixels)), []);
  useEffect(() => {
    if (!playing && frame) sendPaint(device.id, bytesToBase64(frame.px));
  }, [device.id, frame, playing, sendPaint]);

  const togglePlay = () => {
    if (playing) {
      setPlaying(false);
      return;
    }
    dirty.current = true;
    persist();
    setSource(device.id, { type: "animation", animationId: animation.id, speed: 1 });
    sendPaint(device.id, null);
    setPlaying(true);
  };

  // --- editing helpers --------------------------------------------------------

  const remember = () => {
    undoStack.current.push({ frames, cur: index });
    if (undoStack.current.length > 60) undoStack.current.shift();
  };

  const undo = () => {
    const prev = undoStack.current.pop();
    if (!prev) return;
    dirty.current = true;
    setFrames(prev.frames);
    setCur(prev.cur);
  };

  const editFrames = (change: (frames: EditFrame[]) => EditFrame[]) => {
    dirty.current = true;
    setFrames(change);
  };

  const editPixels = (change: (px: Uint8Array) => void) => {
    editFrames((fs) =>
      fs.map((f, i) => {
        if (i !== index) return f;
        const px = f.px.slice();
        change(px);
        return { ...f, px };
      }),
    );
  };

  // A cube is painted one slice at a time, shown as a flat grid.
  const slice = useMemo(() => {
    if (!isCube) return null;
    const [w, h] = layout.dims;
    const map = new Int32Array(w * h).fill(-1);
    layout.grid.forEach((g, i) => {
      if (g[2] === layer) map[g[0] + g[1] * w] = i;
    });
    const lp: LayoutPreview = {
      kind: "matrix",
      count: w * h,
      dims: [w, h, 1],
      pos: Array.from({ length: w * h }, (_, k) => [((k % w) + 0.5) / w, (Math.floor(k / w) + 0.5) / h, 0.5] as [number, number, number]),
      grid: Array.from({ length: w * h }, (_, k) => [k % w, Math.floor(k / w), 0] as [number, number, number]),
      looped: false,
      isGrid: true,
    };
    return { lp, map };
  }, [isCube, layout, layer]);

  const slicePixels = useMemo(() => {
    if (!slice || !frame) return null;
    const out = new Uint8Array(slice.map.length * 3);
    slice.map.forEach((led, k) => {
      if (led >= 0) out.set(frame.px.subarray(led * 3, led * 3 + 3), k * 3);
    });
    return out;
  }, [slice, frame]);

  /** LEDs an action applies to: the visible slice of a cube, otherwise all. */
  const targets = useMemo(() => (slice ? [...slice.map].filter((i) => i >= 0) : Array.from({ length: n }, (_, i) => i)), [slice, n]);

  const onPaint = (hits: number[], phase: PaintPhase) => {
    if (phase === "end" || playing) return;
    const leds = slice ? hits.map((k) => slice.map[k]).filter((i) => i >= 0) : hits;
    if (tool === "picker") {
      if (leds.length) {
        const i = leds[0] * 3;
        setColor(rgbToHex([frame.px[i], frame.px[i + 1], frame.px[i + 2]]));
        setTool("brush");
      }
      return;
    }
    if (phase === "start") remember();
    if (!leds.length) return;
    const [r, g, b] = tool === "eraser" ? [0, 0, 0] : hexToRgb(color);
    editPixels((px) => {
      for (const i of leds) {
        px[i * 3] = r;
        px[i * 3 + 1] = g;
        px[i * 3 + 2] = b;
      }
    });
  };

  const fill = (rgb: RGB) => {
    remember();
    editPixels((px) => targets.forEach((i) => px.set(rgb, i * 3)));
  };

  const fillGradient = () => {
    remember();
    editPixels((px) => {
      targets.forEach((i, k) => {
        const t = layout.isGrid ? layout.pos[i][0] : targets.length > 1 ? k / (targets.length - 1) : 0;
        px.set(samplePalette(gradient, t).map(Math.round), i * 3);
      });
    });
  };

  const shift = (dx: number, dy: number) => {
    remember();
    editPixels((px) => {
      const old = px.slice();
      if (layout.isGrid) {
        const [w, h] = layout.dims;
        const at = new Map<number, number>();
        layout.grid.forEach((g, i) => at.set(g[0] + g[1] * w + g[2] * w * h, i));
        layout.grid.forEach((g, i) => {
          if (slice && g[2] !== layer) return;
          const from = at.get(((g[0] - dx + w) % w) + ((g[1] - dy + h) % h) * w + g[2] * w * h);
          if (from != null) px.set(old.subarray(from * 3, from * 3 + 3), i * 3);
        });
      } else {
        for (let i = 0; i < n; i++) {
          const from = (((i - dx) % n) + n) % n;
          px.set(old.subarray(from * 3, from * 3 + 3), i * 3);
        }
      }
    });
  };

  // --- frames -----------------------------------------------------------------

  const insertAfterCurrent = (extra: EditFrame[]) => {
    if (frames.length + extra.length > MAX_FRAMES) {
      toast(`An animation can have up to ${MAX_FRAMES} frames.`, "error");
      return;
    }
    remember();
    editFrames((fs) => [...fs.slice(0, index + 1), ...extra, ...fs.slice(index + 1)]);
    setCur(index + extra.length);
  };

  const addFrame = () => insertAfterCurrent([{ durationMs: frame.durationMs, fade: frame.fade, px: new Uint8Array(n * 3) }]);
  const duplicateFrame = () => insertAfterCurrent([{ ...frame, px: frame.px.slice() }]);

  const deleteFrame = () => {
    if (frames.length <= 1) return;
    remember();
    editFrames((fs) => fs.filter((_, i) => i !== index));
    setCur(Math.max(0, index - 1));
  };

  const moveFrame = (by: number) => {
    const to = index + by;
    if (to < 0 || to >= frames.length) return;
    remember();
    editFrames((fs) => {
      const next = fs.slice();
      [next[index], next[to]] = [next[to], next[index]];
      return next;
    });
    setCur(to);
  };

  const setFrameProp = (patch: Partial<EditFrame>) => editFrames((fs) => fs.map((f, i) => (i === index ? { ...f, ...patch } : f)));

  // --- image and GIF import (matrices) -----------------------------------------

  const importFile = async (file: File) => {
    const [w, h] = layout.dims;
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    const toPixels = (source: CanvasImageSource, sw: number, sh: number) => {
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, w, h);
      // Pixel art that is an exact multiple of the matrix stays crisp.
      const exact = sw % w === 0 && sh % h === 0 && sw / w === sh / h;
      ctx.imageSmoothingEnabled = !exact;
      ctx.imageSmoothingQuality = "high";
      const k = Math.min(w / sw, h / sh);
      ctx.drawImage(source, (w - sw * k) / 2, (h - sh * k) / 2, sw * k, sh * k);
      const data = ctx.getImageData(0, 0, w, h).data;
      const px = new Uint8Array(n * 3);
      layout.grid.forEach((g, i) => {
        const o = (g[0] + g[1] * w) * 4;
        px.set([data[o], data[o + 1], data[o + 2]], i * 3);
      });
      return px;
    };

    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const Decoder = (window as any).ImageDecoder;
      if (file.type === "image/gif" && Decoder) {
        const decoder = new Decoder({ data: await file.arrayBuffer(), type: file.type });
        await decoder.tracks.ready;
        const total = Math.min(decoder.tracks.selectedTrack.frameCount, 240);
        const imported: EditFrame[] = [];
        for (let i = 0; i < total; i++) {
          const { image } = await decoder.decode({ frameIndex: i });
          imported.push({
            durationMs: clamp(Math.round((image.duration ?? 100_000) / 1000), 20, 60_000),
            fade: false,
            px: toPixels(image, image.displayWidth, image.displayHeight),
          });
          image.close();
        }
        const blank = frames.length === 1 && frames[0].px.every((v) => v === 0);
        if (blank) {
          remember();
          editFrames(() => imported.slice(0, MAX_FRAMES));
          setCur(0);
        } else {
          insertAfterCurrent(imported);
        }
        toast(`Imported ${imported.length} frames`, "success");
      } else {
        const bitmap = await createImageBitmap(file);
        const px = toPixels(bitmap, bitmap.width, bitmap.height);
        remember();
        editPixels((target) => target.set(px));
      }
    } catch (e) {
      toast(`Could not read that image: ${e}`, "error");
    }
  };

  // --- keyboard ---------------------------------------------------------------

  const keys = useRef({ undo, togglePlay });
  keys.current = { undo, togglePlay };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement;
      if (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable) return;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        keys.current.undo();
      } else if (e.key === " ") {
        e.preventDefault();
        keys.current.togglePlay();
      } else if (e.key === "b") setTool("brush");
      else if (e.key === "e") setTool("eraser");
      else if (e.key === "i") setTool("picker");
      else if (e.key === "ArrowLeft") setCur((c) => Math.max(0, c - 1));
      else if (e.key === "ArrowRight") setCur((c) => c + 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (cur > frames.length - 1) setCur(frames.length - 1);
  }, [cur, frames.length]);

  const totalMs = frames.reduce((sum, f) => sum + f.durationMs, 0);

  return (
    <div className="editor">
      <div className="editor__bar">
        <Button variant="ghost" icon={<ArrowLeft size={16} />} onClick={onClose}>
          Animations
        </Button>
        <input
          className="editor__name"
          value={name}
          spellCheck={false}
          onChange={(e) => {
            dirty.current = true;
            setName(e.target.value);
          }}
        />
        <span className="editor__meta">
          {frames.length} frame{frames.length === 1 ? "" : "s"} · {(totalMs / 1000).toFixed(1)} s
        </span>
        <span className="spacer" />
        <Toggle
          label="Loop"
          checked={looped}
          onChange={(v) => {
            dirty.current = true;
            setLooped(v);
          }}
        />
        <Button
          icon={<Download size={16} />}
          title="Export as a standalone Arduino sketch"
          onClick={() => {
            dirty.current = true;
            persist();
            openDialog({ type: "firmware", deviceId: device.id, animationId: animation.id });
          }}
        >
          Export
        </Button>
        <Button variant="primary" icon={playing ? <Square size={15} /> : <Play size={15} />} onClick={togglePlay}>
          {playing ? "Stop" : "Play"}
        </Button>
      </div>

      <div className="editor__work">
        <div className="editor__tools">
          <Segmented
            stretch
            value={tool}
            onChange={setTool}
            options={[
              { value: "brush", label: <Brush size={16} />, title: "Brush (B)" },
              { value: "eraser", label: <Eraser size={16} />, title: "Eraser (E)" },
              { value: "picker", label: <Pipette size={16} />, title: "Pick a colour from the frame (I)" },
            ]}
          />
          <Slider label="Brush size" min={1} max={6} step={1} value={brush} onChange={setBrush} />
          <ColorPicker value={color} onChange={setColor} />

          <div className="tool-row">
            <Button size="sm" icon={<PaintBucket size={14} />} onClick={() => fill(hexToRgb(color))} title="Fill with the current colour">
              Fill
            </Button>
            <Button size="sm" icon={<Eraser size={14} />} onClick={() => fill([0, 0, 0])} title="Turn every LED off">
              Clear
            </Button>
            <Button size="sm" icon={<Undo2 size={14} />} onClick={undo} title="Undo (Ctrl+Z)">
              Undo
            </Button>
          </div>

          <Field label="Gradient">
            <div className="tool-row">
              <PaletteField value={gradient} onChange={setGradient} />
              <Button size="sm" onClick={fillGradient}>
                Apply
              </Button>
            </div>
          </Field>

          <Field label="Shift pixels">
            <div className="tool-row">
              <Button size="sm" icon={<ArrowLeft size={14} />} onClick={() => shift(-1, 0)} title="Shift left" />
              <Button size="sm" icon={<ArrowRight size={14} />} onClick={() => shift(1, 0)} title="Shift right" />
              {layout.isGrid && (
                <>
                  <Button size="sm" icon={<ArrowUp size={14} />} onClick={() => shift(0, -1)} title="Shift up" />
                  <Button size="sm" icon={<ArrowDown size={14} />} onClick={() => shift(0, 1)} title="Shift down" />
                </>
              )}
            </div>
          </Field>

          {isMatrix && (
            <>
              <Button size="sm" icon={<ImagePlus size={14} />} onClick={() => fileInput.current?.click()}>
                Import image or GIF
              </Button>
              <input
                ref={fileInput}
                type="file"
                accept="image/*"
                hidden
                onChange={(e) => {
                  const file = e.target.files?.[0];
                  if (file) void importFile(file);
                  e.target.value = "";
                }}
              />
            </>
          )}
        </div>

        <div className="editor__canvas">
          {playing ? (
            <LedCanvas layout={layout} deviceId={device.id} autoRotate />
          ) : slice ? (
            <LedCanvas layout={slice.lp} pixels={slicePixels} onPaint={onPaint} brush={brush} labels={slice.map} />
          ) : (
            <LedCanvas layout={layout} pixels={frame.px} onPaint={onPaint} brush={brush} />
          )}
          {isCube && !playing && (
            <div className="editor__cube">
              <LedCanvas layout={layout} pixels={frame.px} />
              <Slider
                label={`Slice ${layer + 1} of ${layout.dims[2]} (front to back)`}
                min={0}
                max={layout.dims[2] - 1}
                step={1}
                value={layer}
                format={(v) => String(v + 1)}
                onChange={setLayer}
              />
            </div>
          )}
        </div>
      </div>

      <div className="editor__timeline">
        <div className="timeline__frames">
          {frames.map((f, i) => (
            <button type="button" key={i} className={`timeline__frame ${i === index ? "is-active" : ""}`} onClick={() => setCur(i)}>
              <FrameThumb layout={layout} pixels={f.px} width={92} height={50} />
              <span>
                {i + 1}
                <em>
                  {f.durationMs} ms{f.fade ? " · fade" : ""}
                </em>
              </span>
            </button>
          ))}
          <button type="button" className="timeline__add" onClick={addFrame} title="Add an empty frame">
            <Plus size={18} />
          </button>
        </div>
        <div className="timeline__controls">
          <Field label="Frame length" row>
            <NumberInput value={frame.durationMs} min={10} max={60000} step={10} suffix="ms" onChange={(durationMs) => setFrameProp({ durationMs: Math.round(durationMs) })} />
          </Field>
          <Toggle label="Fade into next" checked={frame.fade} onChange={(fade) => setFrameProp({ fade })} />
          <div className="tool-row">
            <Button size="sm" icon={<ChevronLeft size={14} />} onClick={() => moveFrame(-1)} title="Move frame earlier" disabled={index === 0} />
            <Button size="sm" icon={<ChevronRight size={14} />} onClick={() => moveFrame(1)} title="Move frame later" disabled={index === frames.length - 1} />
            <Button size="sm" icon={<Copy size={14} />} onClick={duplicateFrame} title="Duplicate this frame" />
            <Button size="sm" icon={<Trash2 size={14} />} onClick={deleteFrame} title="Delete this frame" disabled={frames.length <= 1} />
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              remember();
              editFrames((fs) => fs.map((f) => ({ ...f, durationMs: frame.durationMs, fade: frame.fade })));
            }}
            title="Give every frame this length and fade setting"
          >
            Apply timing to all frames
          </Button>
        </div>
      </div>
    </div>
  );
}
