import { Clapperboard, Copy, Download, ImagePlus, Pencil, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ledCount, useApp, useLayoutPreview } from "../store";
import type { Animation, Device } from "../types";
import { base64ToBytes, bytesToBase64, plural, uid } from "../util";
import { AnimationEditor } from "./AnimationEditor";
import { FrameThumb, resamplePixels } from "./frames";
import { importImages, inNameOrder, MAX_FRAMES } from "./imageImport";
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
  const toast = useApp((s) => s.toast);
  const [editingId, setEditingId] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const count = ledCount(device.layout);
  const grid = layout?.isGrid ?? false;

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

  /** Makes an animation out of image files and starts playing it. */
  const importFiles = async (files: File[]) => {
    if (!layout || files.length === 0) return;
    try {
      const frames = await importImages(files, layout, MAX_FRAMES);
      if (frames.length === 0) return;
      const animation: Animation = {
        id: uid(),
        name: inNameOrder(files)[0].name.replace(/\.[^.]+$/, "") || "Imported",
        count: layout.count,
        looped: true,
        frames: frames.map((f) => ({ durationMs: f.durationMs, fade: false, pixels: bytesToBase64(f.px) })),
      };
      saveAnimation(animation);
      setSource(device.id, { type: "animation", animationId: animation.id, speed: 1 });
      toast(frames.length === 1 ? "Image imported" : `Imported ${frames.length} frames`, "success");
    } catch (e) {
      toast(`Could not read that image: ${e}`, "error");
    }
  };

  const importHint = grid
    ? "Pictures are scaled to fit. A GIF, or several images at once, become the frames of one animation."
    : "Every row of the picture becomes one step of the animation, with its columns spread along the LEDs.";
  const importButton = (
    <>
      <Button icon={<ImagePlus size={16} />} onClick={() => fileInput.current?.click()} title={importHint}>
        {grid ? "Import images or GIF" : "Import image"}
      </Button>
      <input
        ref={fileInput}
        type="file"
        accept="image/*"
        multiple
        hidden
        onChange={(e) => {
          const files = [...(e.target.files ?? [])];
          e.target.value = "";
          void importFiles(files);
        }}
      />
    </>
  );

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
            <div className="example-list">
              <Button variant="primary" icon={<Plus size={16} />} onClick={create}>
                New animation
              </Button>
              {importButton}
            </div>
          }
        >
          Paint each LED frame by frame, set how long every frame lasts, and choose whether frames cut or fade into each
          other - or start from a picture. {importHint}
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
          {importButton}
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
