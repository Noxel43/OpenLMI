import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useApp } from "../store";
import type { AudioDeviceInfo, AudioSnapshot, Device, EffectInfo } from "../types";
import { hsvToRgb } from "../util";
import { EffectBrowser } from "./EffectsTab";
import { Field, Select, Slider } from "./ui";

/** Live spectrum so it is obvious whether OpenLMI can hear anything. */
function AudioMeter() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [info, setInfo] = useState<AudioSnapshot["status"] | null>(null);

  useEffect(() => {
    let alive = true;
    let raf = 0;
    let lastStatus = "";
    const tick = async () => {
      try {
        const a = await api.getAudio();
        if (!alive) return;
        const key = JSON.stringify(a.status);
        if (key !== lastStatus) {
          lastStatus = key;
          setInfo(a.status);
        }
        draw(a);
      } catch {
        // Try again next frame.
      }
      if (alive) raf = requestAnimationFrame(tick);
    };
    const draw = (a: AudioSnapshot) => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      const dpr = window.devicePixelRatio || 1;
      const [w, h] = [canvas.clientWidth, canvas.clientHeight];
      if (canvas.width !== Math.round(w * dpr)) {
        canvas.width = Math.round(w * dpr);
        canvas.height = Math.round(h * dpr);
      }
      const ctx = canvas.getContext("2d")!;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const n = a.bands.length || 32;
      const gap = 2;
      const bw = (w - gap * (n - 1)) / n;
      for (let i = 0; i < n; i++) {
        const v = a.bands[i] ?? 0;
        const bh = Math.max(2, v * (h - 4));
        const [r, g, b] = hsvToRgb(0.72 - (i / n) * 0.55, 0.75, 0.55 + 0.45 * v);
        ctx.fillStyle = `rgb(${r | 0},${g | 0},${b | 0})`;
        ctx.beginPath();
        ctx.roundRect(i * (bw + gap), h - bh, bw, bh, 2);
        ctx.fill();
      }
      // Beat flash along the top edge.
      if (a.beat > 0.05) {
        ctx.fillStyle = `rgba(255,255,255,${a.beat * 0.8})`;
        ctx.fillRect(0, 0, w, 2);
      }
    };
    raf = requestAnimationFrame(tick);
    return () => {
      alive = false;
      cancelAnimationFrame(raf);
    };
  }, []);

  return (
    <div className="meter">
      <canvas ref={canvasRef} />
      <div className={`meter__status ${info?.error ? "is-error" : ""}`}>
        {info?.error ? info.error : info?.running ? `Listening to ${info.device}` : "Starting audio capture..."}
      </div>
    </div>
  );
}

const audioOnly = (e: EffectInfo) => e.audio;

export function AudioTab({ device }: { device: Device }) {
  const settings = useApp((s) => s.config.settings);
  const saveSettings = useApp((s) => s.saveSettings);
  const [inputs, setInputs] = useState<AudioDeviceInfo[]>([{ id: "loopback", name: "System audio (what the PC is playing)" }]);

  useEffect(() => {
    api.listAudioInputs().then(setInputs).catch(() => {});
  }, []);

  const known = inputs.some((i) => i.id === settings.audioSource);

  return (
    <EffectBrowser
      device={device}
      filter={audioOnly}
      below={
        <div className="panel">
          <div className="panel__head">
            <div>
              <h3>Sound input</h3>
              <p>Audio effects react to this. Play some music to see it move.</p>
            </div>
          </div>
          <div className="panel__body">
            <AudioMeter />
            <div className="field-pair">
            <Field label="Listen to">
              <Select
                value={settings.audioSource}
                options={[...inputs, ...(known ? [] : [{ id: settings.audioSource, name: "Unavailable device" }])].map((i) => ({
                  value: i.id,
                  label: i.name,
                }))}
                onChange={(audioSource) => saveSettings({ ...settings, audioSource })}
              />
            </Field>
            <Slider
              label="Input gain"
              min={0.2}
              max={4}
              step={0.05}
              value={settings.audioGain}
              resetTo={1}
              format={(v) => `${v.toFixed(2)}x`}
              onChange={(audioGain) => saveSettings({ ...settings, audioGain })}
            />
            </div>
          </div>
        </div>
      }
    />
  );
}
