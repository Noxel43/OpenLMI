import { indentWithTab } from "@codemirror/commands";
import { rust } from "@codemirror/lang-rust";
import { EditorState } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { BookOpen, CheckCircle2, Code, Play, Plus, Trash2, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useApp } from "../store";
import type { Device, Script } from "../types";
import { uid } from "../util";
import { Button, Empty, Select } from "./ui";

const editorTheme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px", backgroundColor: "transparent" },
  ".cm-scroller": { fontFamily: "'Cascadia Code', 'JetBrains Mono', Consolas, monospace", lineHeight: "1.55" },
  ".cm-gutters": { backgroundColor: "transparent", border: "none" },
  "&.cm-focused": { outline: "none" },
});

function CodeEditor({ value, onChange }: { value: string; onChange: (code: string) => void }) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  useEffect(() => {
    const v = new EditorView({
      parent: host.current!,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          // Rhai reads close enough to Rust for highlighting purposes.
          rust(),
          oneDark,
          editorTheme,
          keymap.of([indentWithTab]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChangeRef.current(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => v.destroy();
    // The editor is created once; `value` is synced below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const v = view.current;
    if (v && value !== v.state.doc.toString()) {
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
    }
  }, [value]);

  return <div ref={host} className="code-editor" />;
}

const REFERENCE: { title: string; items: [string, string][] }[] = [
  {
    title: "What to define",
    items: [
      ["fn pixel(i, x, y, z, t)", "Runs for every LED each frame; return its colour. x, y, z are 0..1, t is seconds."],
      ["fn pixel(i, t)", "Short form when you only need the LED number."],
      ["fn frame(t, dt)", "Runs once per frame. Draw with set / fill / fade; LEDs keep their colour between frames."],
      ["fn init()", "Optional. Return a map like #{ n: 0 }; it is available as `this` in frame and pixel."],
    ],
  },
  {
    title: "Colours",
    items: [
      ["hsv(h, s, v)", "Hue, saturation, brightness - all 0..1. Hue wraps."],
      ["rgb(r, g, b)", "Red, green, blue from 0 to 255."],
      ['hex("#ff8800")', "A colour from a hex code."],
      ['palette("Sunset", t)', "Sample a built-in palette; t wraps at 1."],
      ["mix(a, b, t)", "Blend from colour a to b."],
      ["dim(c, amount)", "Scale a colour's brightness."],
      ["add(a, b)", "Add two colours together."],
    ],
  },
  {
    title: "Drawing (in frame)",
    items: [
      ["set(i, c) / get(i)", "Write or read one LED."],
      ["fill(c)", "Set every LED."],
      ["fade(amount)", "Multiply everything, e.g. fade(0.9) for trails."],
      ["shift(n)", "Slide all LEDs along by n."],
      ["set_xy(x, y, c)", "Write a matrix cell (also set_xyz for cubes)."],
      ["index_of(x, y)", "LED number at a cell, or -1."],
    ],
  },
  {
    title: "Layout",
    items: [
      ["led_count()", "How many LEDs there are."],
      ["width() height() depth()", "Grid size for matrices and cubes."],
      ["pos_x(i) pos_y(i) pos_z(i)", "Where LED i is, 0..1."],
    ],
  },
  {
    title: "Maths",
    items: [
      ["wave(x)", "Smooth 0..1 wave with period 1."],
      ["tri(x)", "Triangle 0..1..0 with period 1."],
      ["noise(x) noise(x, y) noise(x, y, z)", "Smooth random value, 0..1."],
      ["rand() / rand_int(a, b)", "Random float 0..1 / whole number a..b."],
      ["lerp(a, b, t) smoothstep(a, b, x)", "Interpolation helpers."],
      ["fract(x) clamp01(x)", "Fractional part / clamp to 0..1."],
      ["sin cos sqrt abs min max floor", "The usual maths functions are built in."],
    ],
  },
  {
    title: "Audio",
    items: [
      ["audio_level()", "Overall loudness, 0..1."],
      ["audio_bass() audio_mid() audio_treble()", "Loudness of each range."],
      ["audio_band(f)", "Loudness across the spectrum; f = 0 (bass) .. 1 (treble)."],
      ["audio_beat()", "Jumps to 1 on a beat, then decays."],
    ],
  },
];

export function CodeTab({ device }: { device: Device }) {
  const scripts = useApp((s) => s.config.scripts);
  const examples = useApp((s) => s.scriptExamples);
  const palettes = useApp((s) => s.palettes);
  const note = useApp((s) => s.status?.devices[device.id]?.note ?? null);
  const saveScript = useApp((s) => s.saveScript);
  const removeScript = useApp((s) => s.removeScript);
  const setSource = useApp((s) => s.setSource);

  const src = device.source;
  const runningId = src.type === "script" ? src.scriptId : null;
  const [selectedId, setSelectedId] = useState<string | null>(runningId ?? scripts[0]?.id ?? null);
  const [showHelp, setShowHelp] = useState(true);
  const [compileError, setCompileError] = useState<string | null>(null);

  const selected = scripts.find((s) => s.id === selectedId) ?? null;
  // The editor owns the text while typing; the store is updated shortly after.
  const [draft, setDraft] = useState<Script | null>(selected);
  useEffect(() => {
    setDraft(selected);
    setCompileError(null);
    // Reset only when a different script is opened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId]);

  useEffect(() => {
    if (!draft) return;
    const stored = useApp.getState().config.scripts.find((s) => s.id === draft.id);
    if (stored && stored.code === draft.code && stored.name === draft.name) return;
    const timer = setTimeout(() => {
      saveScript(draft);
      api.checkScript(draft.code).then(
        () => setCompileError(null),
        (e) => setCompileError(String(e)),
      );
    }, 350);
    return () => clearTimeout(timer);
  }, [draft, saveScript]);

  const create = (name: string, code: string) => {
    const script: Script = { id: uid(), name, code };
    saveScript(script);
    setSelectedId(script.id);
  };

  if (scripts.length === 0) {
    return (
      <div className="centered">
        <Empty
          icon={<Code size={30} />}
          title="Write your own effect"
          action={
            <div className="example-list">
              {examples.map((ex) => (
                <Button key={ex.name} onClick={() => create(ex.name, ex.code)}>
                  {ex.name}
                </Button>
              ))}
            </div>
          }
        >
          Everything else in OpenLMI works without code. When you want full control, a few lines of script can colour
          every LED however you like. Start from an example:
        </Empty>
      </div>
    );
  }

  const running = draft != null && runningId === draft.id;
  const runtimeError = running && note?.startsWith("Script error") ? note : null;
  const error = compileError ?? runtimeError;

  return (
    <div className="code">
      <div className="code__list">
        <div className="code__list-head">
          <span>Scripts</span>
          <Button size="sm" variant="ghost" icon={<Plus size={14} />} title="New script" onClick={() => create(`Script ${scripts.length + 1}`, examples[0]?.code ?? "")} />
        </div>
        {scripts.map((s) => (
          <button type="button" key={s.id} className={`code__item ${s.id === selectedId ? "is-active" : ""}`} onClick={() => setSelectedId(s.id)}>
            <Code size={14} />
            <span>{s.name}</span>
            {runningId === s.id && <i className="tabs__live" title="Running on this device" />}
          </button>
        ))}
        <div className="code__examples">
          <Select
            value=""
            options={[{ value: "", label: "New from example..." }, ...examples.map((ex) => ({ value: ex.name, label: ex.name }))]}
            onChange={(name) => {
              const ex = examples.find((x) => x.name === name);
              if (ex) create(ex.name, ex.code);
            }}
          />
        </div>
      </div>

      {draft ? (
        <div className="code__main">
          <div className="code__bar">
            <input className="editor__name" value={draft.name} spellCheck={false} onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
            <span className="spacer" />
            <Button variant="ghost" icon={<BookOpen size={16} />} active={showHelp} onClick={() => setShowHelp((v) => !v)}>
              Reference
            </Button>
            <Button
              variant="ghost"
              icon={<Trash2 size={16} />}
              title="Delete this script"
              onClick={() => {
                removeScript(draft.id);
                setSelectedId(scripts.find((s) => s.id !== draft.id)?.id ?? null);
              }}
            />
            <Button
              variant={running ? "default" : "primary"}
              icon={running ? <CheckCircle2 size={16} /> : <Play size={15} />}
              onClick={() => setSource(device.id, { type: "script", scriptId: draft.id })}
              title={running ? "Edits are applied as you type" : "Show this script on the device"}
            >
              {running ? "Running" : "Run on device"}
            </Button>
          </div>
          <div className="code__edit">
            <CodeEditor key={draft.id} value={draft.code} onChange={(code) => setDraft((d) => (d ? { ...d, code } : d))} />
          </div>
          {error ? (
            <div className="code__status is-error">
              <TriangleAlert size={15} />
              <span>{error.replace(/^Script error: /, "")}</span>
            </div>
          ) : (
            <div className="code__status">
              <CheckCircle2 size={15} />
              <span>{running ? "Running - changes apply as you type." : "No problems found."}</span>
            </div>
          )}
        </div>
      ) : (
        <div className="code__main">
          <Empty title="No script selected">Pick one from the list.</Empty>
        </div>
      )}

      {showHelp && draft && (
        <aside className="code__help">
          {REFERENCE.map((section) => (
            <section key={section.title}>
              <h4>{section.title}</h4>
              {section.items.map(([sig, text]) => (
                <div className="code__ref" key={sig}>
                  <code>{sig}</code>
                  <span>{text}</span>
                </div>
              ))}
            </section>
          ))}
          <section>
            <h4>Palette names</h4>
            <p className="muted small">{palettes.map((p) => p.name).join(", ")}</p>
          </section>
        </aside>
      )}
    </div>
  );
}
