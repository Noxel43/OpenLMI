import { Clapperboard, Copy, Download, Pencil, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { ledCount, useApp, useLayoutPreview } from "../store";
import type { Animation, Device } from "../types";
import { base64ToBytes, bytesToBase64, plural, uid } from "../util";
import { AnimationEditor } from "./AnimationEditor";
import { FrameThumb, resamplePixels } from "./frames";
import { Button, Empty, Slider } from "./ui";

function totalSeconds(a: Animation): string {
  const ms = a.frames.reduce((sum, f) => sum + f.durationMs, 0);
  return ms >= 10_000 ? `${Math.round(ms / 1000)} s` : `${(ms / 1000).toFixed(1)} s`;
}

export function AnimationsTab({ device, onEditing }: { device: Device; onEditing: (editing: boolean) => void }) {
  const animations = useApp((s) => s.config.animations);
  const saveAnimation = useApp((s) => s.saveAnimation);
  const removeAnimation = useApp((s) => s.removeAnimation);
  const setSource = useApp((s) => s.setSource);
  const openDialog = useApp((s) => s.openDialog);
  const layout = useLayoutPreview(device.layout);
  const [editingId, setEditingId] = useState<string | null>(null);
  const count = ledCount(device.layout);

  const editing = animations.find((a) => a.id === editingId);
  useEffect(() => {
    onEditing(!!editing);
    return () => onEditing(false);
  }, [editing, onEditing]);

  const src = device.source;
  const playingId = src.type === "animation" ? src.animationId : null;

  const create = () => {
    const animation: Animation = {
      id: uid(),
      name: `Animation ${animations.length + 1}`,
      count,
      looped: true,
      frames: [{ durationMs: 200, fade: false, pixels: bytesToBase64(new Uint8Array(count * 3)) }],
    };
    saveAnimation(animation);
    setEditingId(animation.id);
  };

  if (editing && layout) {
    return <AnimationEditor key={editing.id} device={device} layout={layout} animation={editing} onClose={() => setEditingId(null)} />;
  }

  if (animations.length === 0) {
    return (
      <div className="centered">
        <Empty
          icon={<Clapperboard size={30} />}
          title="Make your own animation"
          action={
            <Button variant="primary" icon={<Plus size={16} />} onClick={create}>
              New animation
            </Button>
          }
        >
          Paint each LED frame by frame, set how long every frame lasts, and choose whether frames cut or fade into each
          other. On a matrix you can also import images and GIFs.
        </Empty>
      </div>
    );
  }

  return (
    <div className="split">
      <div className="split__main">
        <div className="filter-row">
          <Button variant="primary" icon={<Plus size={16} />} onClick={create}>
            New animation
          </Button>
        </div>
        <div className="anim-grid">
          {animations.map((a) => (
            <div key={a.id} className={`anim-card ${playingId === a.id ? "is-active" : ""}`}>
              <button
                type="button"
                className="anim-card__thumb"
                onClick={() => setSource(device.id, { type: "animation", animationId: a.id, speed: 1 })}
                title="Play on this device"
              >
                {layout && a.frames[0] && (
                  <FrameThumb layout={layout} pixels={resamplePixels(base64ToBytes(a.frames[0].pixels), layout.count)} width={196} height={96} />
                )}
                <span className="anim-card__play">
                  <Play size={18} />
                </span>
              </button>
              <div className="anim-card__info">
                <div className="anim-card__name">{a.name}</div>
                <div className="anim-card__sub">
                  {plural(a.frames.length, "frame")} · {totalSeconds(a)}
                  {a.count !== count ? ` · made for ${a.count} LEDs` : ""}
                </div>
              </div>
              <div className="anim-card__actions">
                <Button size="sm" variant="ghost" icon={<Pencil size={14} />} title="Edit" onClick={() => setEditingId(a.id)} />
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Copy size={14} />}
                  title="Duplicate"
                  onClick={() => saveAnimation({ ...a, id: uid(), name: `${a.name} copy` })}
                />
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Download size={14} />}
                  title="Export as a standalone Arduino sketch"
                  onClick={() => openDialog({ type: "firmware", deviceId: device.id, animationId: a.id })}
                />
                <Button size="sm" variant="ghost" icon={<Trash2 size={14} />} title="Delete" onClick={() => removeAnimation(a.id)} />
              </div>
            </div>
          ))}
        </div>
      </div>

      <aside className="split__side">
        <div className="panel">
          {src.type === "animation" ? (
            <>
              <div className="panel__head">
                <div>
                  <h3>{animations.find((a) => a.id === src.animationId)?.name ?? "Animation"}</h3>
                  <p>Now playing on this device.</p>
                </div>
              </div>
              <div className="panel__body">
                <Slider
                  label="Playback speed"
                  min={0.1}
                  max={4}
                  step={0.05}
                  value={src.speed}
                  resetTo={1}
                  format={(v) => `${v.toFixed(2)}x`}
                  onChange={(speed) => setSource(device.id, { ...src, speed })}
                />
              </div>
            </>
          ) : (
            <Empty title="Nothing playing">Click an animation to play it on this device.</Empty>
          )}
        </div>
      </aside>
    </div>
  );
}
