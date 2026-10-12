import { MonitorPlay, SlidersHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { DEFAULT_AMBIENT, useApp } from "../store";
import type { AmbientSettings, Device, MonitorInfo } from "../types";
import { Button, Empty, Field, Notice, Segmented, Select, Slider, Toggle } from "./ui";

type Zone = [number, number, number, number];

/** The screen as the LEDs see it, with each LED's sample area drawn on top in
 *  the colour that LED is currently showing. */
function ScreenPreview({ device, settings, aspect }: { device: Device; settings: AmbientSettings; aspect: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const zones = useRef<Zone[]>([]);
  const layoutKey = JSON.stringify(device.layout);

  useEffect(() => {
    let cancelled = false;
    api.ambientZones(device.layout, settings, aspect).then((z) => {
      if (!cancelled) zones.current = z;
    });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [layoutKey, settings.depth, settings.mapping, aspect]);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    const small = document.createElement("canvas");
    const tick = async () => {
      try {
        const [screen, frame] = await Promise.all([api.getScreen(settings.monitor), api.getFrame(device.id)]);
        if (!alive) return;
        const canvas = canvasRef.current;
        if (canvas && screen.length > 6) {
          const [gw, gh] = [screen[0], screen[1]];
          const bars = settings.blackBars ? [screen[2] / 255, screen[3] / 255, screen[4] / 255, screen[5] / 255] : [0, 0, 1, 1];
          if (small.width !== gw || small.height !== gh) {
            small.width = gw;
            small.height = gh;
          }
          const sctx = small.getContext("2d")!;
          const img = sctx.createImageData(gw, gh);
          for (let i = 0, o = 0; i < gw * gh; i++, o += 4) {
            img.data[o] = screen[6 + i * 3];
            img.data[o + 1] = screen[7 + i * 3];
            img.data[o + 2] = screen[8 + i * 3];
            img.data[o + 3] = 255;
          }
          sctx.putImageData(img, 0, 0);

          const dpr = window.devicePixelRatio || 1;
          const [w, h] = [canvas.clientWidth, canvas.clientHeight];
          if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
            canvas.width = Math.round(w * dpr);
            canvas.height = Math.round(h * dpr);
          }
          const ctx = canvas.getContext("2d")!;
          ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
          ctx.clearRect(0, 0, w, h);
          ctx.imageSmoothingEnabled = true;
          ctx.globalAlpha = 0.55;
          ctx.drawImage(small, 0, 0, w, h);
          ctx.globalAlpha = 1;

          const [cl, ct, cr, cb] = bars;
          const zs = zones.current;
          ctx.lineWidth = 1;
          for (let i = 0; i < zs.length; i++) {
            const [x0, y0, x1, y1] = zs[i];
            const rx = (cl + x0 * (cr - cl)) * w;
            const ry = (ct + y0 * (cb - ct)) * h;
            const rw = (x1 - x0) * (cr - cl) * w;
            const rh = (y1 - y0) * (cb - ct) * h;
            if (frame.length >= (i + 1) * 3) {
              ctx.fillStyle = `rgba(${frame[i * 3]},${frame[i * 3 + 1]},${frame[i * 3 + 2]},0.9)`;
              ctx.fillRect(rx, ry, rw, rh);
            }
            if (zs.length <= 400) {
              ctx.strokeStyle = "rgba(255,255,255,0.28)";
              ctx.strokeRect(rx + 0.5, ry + 0.5, rw - 1, rh - 1);
            }
          }
        }
      } catch {
        // Capture may not have produced a frame yet.
      }
      if (alive) timer = setTimeout(tick, 80);
    };
    void tick();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [device.id, settings.monitor, settings.blackBars]);

  return (
    <div className="screen-preview" style={{ aspectRatio: String(aspect) }}>
      <canvas ref={canvasRef} />
    </div>
  );
}

export function AmbientTab({ device }: { device: Device }) {
  const setSource = useApp((s) => s.setSource);
  const openDialog = useApp((s) => s.openDialog);
  const remembered = useApp((s) => s.ambientMemory[device.id]);
  const captureError = useApp((s) => s.status?.captureError);
  const settingsAll = useApp((s) => s.config.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);

  useEffect(() => {
    api.listMonitors().then(setMonitors).catch(() => {});
  }, []);

  const src = device.source;
  const active = src.type === "ambient";
  const isMonitor = device.layout.type === "monitor";

  const layoutHint = !isMonitor && (
    <Notice kind="info">
      This device is set up as a {device.layout.type}. If the LEDs sit behind a monitor, switch its type to{" "}
      <b>Monitor backlight</b> so every LED follows its own part of the screen.{" "}
      <button type="button" className="link" onClick={() => openDialog({ type: "device", deviceId: device.id })}>
        Open set up
      </button>
    </Notice>
  );

  if (!active) {
    return (
      <div className="centered">
        <Empty
          icon={<MonitorPlay size={30} />}
          title="Ambient lighting"
          action={
            <Button
              variant="primary"
              icon={<MonitorPlay size={16} />}
              onClick={() => setSource(device.id, { type: "ambient", ...(remembered ?? DEFAULT_AMBIENT) })}
            >
              Start ambient lighting
            </Button>
          }
        >
          The LEDs follow what is on your screen: films, games, anything. Each LED mirrors the part of the picture it sits
          behind.
        </Empty>
        {layoutHint}
      </div>
    );
  }

  const { type: _type, ...settings } = src;
  const update = (patch: Partial<AmbientSettings>) => setSource(device.id, { ...src, ...patch });
  const chosen = monitors.find((m) => m.id === settings.monitor) ?? monitors.find((m) => m.primary) ?? monitors[0];
  const aspect = chosen ? chosen.width / chosen.height : 16 / 9;

  return (
    <div className="split">
      <div className="split__main">
        {captureError && <Notice kind="error">{captureError}</Notice>}
        {layoutHint}
        <ScreenPreview device={device} settings={settings} aspect={aspect} />
        <p className="muted small">
          The coloured blocks are the areas each LED samples, filled with the colour it is showing right now.
          {isMonitor && (
            <>
              {" "}
              Colours on the wrong side or running backwards? Fix the wiring in{" "}
              <button type="button" className="link" onClick={() => openDialog({ type: "device", deviceId: device.id })}>
                <SlidersHorizontal size={12} /> Set up
              </button>
              .
            </>
          )}
        </p>
      </div>

      <aside className="split__side">
        <div className="panel">
          <div className="panel__head">
            <div>
              <h3>Ambient</h3>
              <p>Tune how the LEDs follow the screen.</p>
            </div>
          </div>
          <div className="panel__body">
            <Field label="Display">
              <Select
                value={settings.monitor}
                options={[
                  { value: "", label: "Primary display" },
                  ...monitors.map((m) => ({ value: m.id, label: `${m.name} (${m.width} x ${m.height})${m.primary ? " - primary" : ""}` })),
                ]}
                onChange={(monitor) => update({ monitor })}
              />
            </Field>
            <Field label="Colour from">
              <Segmented
                stretch
                value={settings.mapping}
                options={[
                  { value: "zones", label: "Screen areas", title: "Each LED follows the part of the screen behind it" },
                  { value: "average", label: "Whole screen", title: "All LEDs show the average colour of the screen" },
                ]}
                onChange={(mapping) => update({ mapping })}
              />
            </Field>
            {settings.mapping === "zones" && device.layout.type !== "matrix" && device.layout.type !== "strip" && (
              <Slider
                label="Sampling depth"
                min={0.02}
                max={0.5}
                step={0.01}
                value={settings.depth}
                resetTo={DEFAULT_AMBIENT.depth}
                format={(v) => `${Math.round(v * 100)}%`}
                hint="How far in from the edge each LED looks."
                onChange={(depth) => update({ depth })}
              />
            )}
            <Slider
              label="Smoothing"
              min={0}
              max={0.95}
              step={0.01}
              value={settings.smoothing}
              resetTo={DEFAULT_AMBIENT.smoothing}
              format={(v) => `${Math.round(v * 100)}%`}
              hint="Higher is calmer, lower is snappier."
              onChange={(smoothing) => update({ smoothing })}
            />
            <Slider
              label="Saturation"
              min={0}
              max={2.5}
              step={0.05}
              value={settings.saturation}
              resetTo={DEFAULT_AMBIENT.saturation}
              format={(v) => `${v.toFixed(2)}x`}
              onChange={(saturation) => update({ saturation })}
            />
            <Slider
              label="Brightness boost"
              min={0.2}
              max={3}
              step={0.05}
              value={settings.gain}
              resetTo={1}
              format={(v) => `${v.toFixed(2)}x`}
              onChange={(gain) => update({ gain })}
            />
            <Slider
              label="Minimum glow"
              min={0}
              max={0.5}
              step={0.01}
              value={settings.minBrightness}
              resetTo={0}
              format={(v) => `${Math.round(v * 100)}%`}
              hint="Keeps a soft backlight during dark scenes."
              onChange={(minBrightness) => update({ minBrightness })}
            />
            <Toggle
              label="Ignore black bars"
              hint="Skips letterbox bars so widescreen films still light the top and bottom."
              checked={settings.blackBars}
              onChange={(blackBars) => update({ blackBars })}
            />
            <Slider
              label="Capture rate"
              min={10}
              max={60}
              step={1}
              value={settingsAll.captureFps}
              resetTo={30}
              format={(v) => `${v} fps`}
              hint="Applies to every device. Higher uses more CPU."
              onChange={(captureFps) => saveSettings({ ...settingsAll, captureFps })}
            />
          </div>
        </div>
      </aside>
    </div>
  );
}
