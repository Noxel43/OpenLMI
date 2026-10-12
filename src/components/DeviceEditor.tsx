import { Cable, Layers, RefreshCw, SlidersHorizontal, Trash2, Usb, Wifi } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import { ledCount, useApp, useLayoutPreview } from "../store";
import type { ColorOrder, Connection, Corner, Device, Layout, LayoutType, MatrixWiring, NetProtocol, PortInfo } from "../types";
import { uid } from "../util";
import { LedCanvas } from "./LedCanvas";
import { LAYOUT_ICONS, LAYOUT_NAMES } from "./Sidebar";
import { Button, Field, Modal, Notice, NumberInput, Segmented, Select, Slider, TextInput, Toggle } from "./ui";

const TYPES: { type: LayoutType; blurb: string }[] = [
  { type: "strip", blurb: "A single line of LEDs" },
  { type: "matrix", blurb: "A flat grid or panel" },
  { type: "cube", blurb: "Stacked layers in 3D" },
  { type: "ring", blurb: "LEDs in a circle" },
  { type: "monitor", blurb: "Behind a screen, for ambient light" },
];

const DEFAULT_WIRING: MatrixWiring = { start: "topLeft", direction: "horizontal", serpentine: true };

function defaultLayout(type: LayoutType): Layout {
  switch (type) {
    case "strip":
      return { type, count: 60, reversed: false };
    case "matrix":
      return { type, width: 16, height: 16, wiring: DEFAULT_WIRING };
    case "cube":
      return { type, width: 8, height: 8, depth: 8, wiring: DEFAULT_WIRING, layerAxis: "z", layersReversed: false, layersSerpentine: false };
    case "ring":
      return { type, count: 24, startAngle: 0, clockwise: true };
    case "monitor":
      return { type, top: 30, right: 17, bottom: 30, left: 17, start: "bottomLeft", clockwise: true, offset: 0, bottomGap: 0 };
    case "custom":
      return { type, points: [[0.5, 0.5]] };
  }
}

function newDevice(count: number): Device {
  return {
    id: uid(),
    name: `LEDs ${count + 1}`,
    power: true,
    connection: { type: "simulator" },
    layout: defaultLayout("strip"),
    output: { colorOrder: "RGB", brightness: 0.8, gamma: 2.2, whiteBalance: [1, 1, 1], maxMilliamps: 0, maxFps: 60 },
    source: { type: "effect", effect: "rainbow", params: {} },
  };
}

const NET_PORTS: Record<NetProtocol, number> = { ddp: 4048, wled: 21324, artnet: 6454, e131: 5568 };
const NET_HINTS: Record<NetProtocol, string> = {
  ddp: "Works with WLED out of the box. Best choice for most Wi-Fi controllers.",
  wled: "WLED's own realtime UDP protocol.",
  artnet: "For Art-Net nodes and pixel controllers. 170 LEDs per universe.",
  e131: "sACN, for pixel controllers such as ESPixelStick. 170 LEDs per universe.",
};
const BAUDS = [115200, 230400, 250000, 460800, 500000, 921600, 1000000, 2000000];

function CornerPicker({ value, onChange }: { value: Corner; onChange: (c: Corner) => void }) {
  const corners: Corner[] = ["topLeft", "topRight", "bottomLeft", "bottomRight"];
  return (
    <div className="corner-picker" title="Where the first LED is">
      {corners.map((c) => (
        <button type="button" key={c} className={`corner corner--${c} ${c === value ? "is-active" : ""}`} onClick={() => onChange(c)} aria-label={c} />
      ))}
    </div>
  );
}

function WiringFields({ wiring, onChange }: { wiring: MatrixWiring; onChange: (w: MatrixWiring) => void }) {
  return (
    <>
      <Field label="First LED is at" row>
        <CornerPicker value={wiring.start} onChange={(start) => onChange({ ...wiring, start })} />
      </Field>
      <Field label="The strip runs">
        <Segmented
          stretch
          value={wiring.direction}
          options={[
            { value: "horizontal", label: "Along rows" },
            { value: "vertical", label: "Along columns" },
          ]}
          onChange={(direction) => onChange({ ...wiring, direction })}
        />
      </Field>
      <Toggle
        label="Zig-zag (serpentine)"
        hint="Each line runs the opposite way to the one before. Off means every line starts from the same side."
        checked={wiring.serpentine}
        onChange={(serpentine) => onChange({ ...wiring, serpentine })}
      />
    </>
  );
}

function LayoutFields({ layout, onChange }: { layout: Layout; onChange: (l: Layout) => void }) {
  const count = (label: string, value: number, set: (v: number) => void, max: number) => (
    <Field label={label} row>
      <NumberInput value={value} min={1} max={max} onChange={(v) => set(Math.max(1, Math.round(v)))} />
    </Field>
  );

  switch (layout.type) {
    case "strip":
      return (
        <>
          {count("LEDs", layout.count, (count) => onChange({ ...layout, count }), 16384)}
          <Toggle label="Reversed" hint="The controller is connected to the far end of the strip." checked={layout.reversed} onChange={(reversed) => onChange({ ...layout, reversed })} />
        </>
      );
    case "matrix":
      return (
        <>
          <div className="field-pair">
            {count("Width", layout.width, (width) => onChange({ ...layout, width }), 256)}
            {count("Height", layout.height, (height) => onChange({ ...layout, height }), 256)}
          </div>
          <WiringFields wiring={layout.wiring} onChange={(wiring) => onChange({ ...layout, wiring })} />
        </>
      );
    case "cube":
      return (
        <>
          <div className="field-pair field-pair--3">
            {count("Width", layout.width, (width) => onChange({ ...layout, width }), 32)}
            {count("Height", layout.height, (height) => onChange({ ...layout, height }), 32)}
            {count("Depth", layout.depth, (depth) => onChange({ ...layout, depth }), 32)}
          </div>
          <Field label="Layers are stacked">
            <Select
              value={layout.layerAxis}
              options={[
                { value: "z", label: "Front to back (upright panels)" },
                { value: "y", label: "Top to bottom (flat floors)" },
                { value: "x", label: "Left to right (upright slices)" },
              ]}
              onChange={(layerAxis) => onChange({ ...layout, layerAxis })}
            />
          </Field>
          <Toggle label="Layers in reverse order" checked={layout.layersReversed} onChange={(layersReversed) => onChange({ ...layout, layersReversed })} />
          <Toggle
            label="Alternate layers run backwards"
            hint="For one continuous strip that snakes from the end of a layer into the next."
            checked={layout.layersSerpentine}
            onChange={(layersSerpentine) => onChange({ ...layout, layersSerpentine })}
          />
          <div className="field__label">Within each layer</div>
          <WiringFields wiring={layout.wiring} onChange={(wiring) => onChange({ ...layout, wiring })} />
        </>
      );
    case "ring":
      return (
        <>
          {count("LEDs", layout.count, (count) => onChange({ ...layout, count }), 4096)}
          <Slider label="First LED position" min={0} max={359} step={1} value={layout.startAngle} format={(v) => `${v} deg`} hint="Clockwise from the top." onChange={(startAngle) => onChange({ ...layout, startAngle })} />
          <Field label="LED numbers increase">
            <Segmented
              stretch
              value={layout.clockwise ? "cw" : "ccw"}
              options={[
                { value: "cw", label: "Clockwise" },
                { value: "ccw", label: "Counter-clockwise" },
              ]}
              onChange={(v) => onChange({ ...layout, clockwise: v === "cw" })}
            />
          </Field>
        </>
      );
    case "monitor":
      return (
        <>
          <Notice>
            Everything here is <b>as seen from the front</b> - sitting at your desk looking at the screen - even though the
            LEDs are stuck to the back.
          </Notice>
          <div className="field__label">LEDs on each side</div>
          <div className="side-counts">
            <div className="side-counts__top">
              <NumberInput value={layout.top} min={0} max={1024} onChange={(v) => onChange({ ...layout, top: Math.round(v) })} />
            </div>
            <div className="side-counts__left">
              <NumberInput value={layout.left} min={0} max={1024} onChange={(v) => onChange({ ...layout, left: Math.round(v) })} />
            </div>
            <div className="side-counts__screen">screen</div>
            <div className="side-counts__right">
              <NumberInput value={layout.right} min={0} max={1024} onChange={(v) => onChange({ ...layout, right: Math.round(v) })} />
            </div>
            <div className="side-counts__bottom">
              <NumberInput value={layout.bottom} min={0} max={1024} onChange={(v) => onChange({ ...layout, bottom: Math.round(v) })} />
            </div>
          </div>
          <Field label="The strip starts at this corner" row>
            <CornerPicker value={layout.start} onChange={(start) => onChange({ ...layout, start })} />
          </Field>
          <Field label="From there it runs">
            <Segmented
              stretch
              value={layout.clockwise ? "cw" : "ccw"}
              options={[
                { value: "cw", label: "Clockwise" },
                { value: "ccw", label: "Counter-clockwise" },
              ]}
              onChange={(v) => onChange({ ...layout, clockwise: v === "cw" })}
            />
          </Field>
          <Field label="Start offset" row hint="If the strip begins part-way along an edge (say, bottom centre), how many LEDs past the corner it starts. Can be negative.">
            <NumberInput value={layout.offset} min={-2048} max={2048} suffix="LEDs" onChange={(v) => onChange({ ...layout, offset: Math.round(v) })} />
          </Field>
          <Slider
            label="Gap for the stand"
            min={0}
            max={0.8}
            step={0.01}
            value={layout.bottomGap}
            format={(v) => (v === 0 ? "None" : `${Math.round(v * 100)}%`)}
            hint="Part of the bottom edge with no LEDs, centred."
            onChange={(bottomGap) => onChange({ ...layout, bottomGap })}
          />
        </>
      );
    case "custom":
      return <Notice>This device uses a free-form layout from an imported configuration.</Notice>;
  }
}

function ConnectionFields({ value, onChange }: { value: Connection; onChange: (c: Connection) => void }) {
  const [ports, setPorts] = useState<PortInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const refresh = () => {
    setLoading(true);
    api
      .listSerialPorts()
      .then(setPorts)
      .catch(() => {})
      .finally(() => setLoading(false));
  };
  useEffect(refresh, []);

  const pick = (type: Connection["type"]) => {
    if (type === value.type) return;
    if (type === "simulator") onChange({ type });
    // The port is left for the user to pick: guessing could send LED data to
    // some unrelated serial device.
    if (type === "serial") onChange({ type, port: "", baud: 500000, protocol: "adalight" });
    if (type === "network") onChange({ type, host: "", port: NET_PORTS.ddp, protocol: "ddp", universe: 0 });
  };

  return (
    <>
      <div className="choice-row">
        {(
          [
            ["simulator", <Layers size={20} key="i" />, "Simulator", "No hardware. Preview on screen only."],
            ["serial", <Usb size={20} key="i" />, "USB / Serial", "Arduino, ESP32, Pico and similar boards on a USB cable."],
            ["network", <Wifi size={20} key="i" />, "Wi-Fi / Network", "WLED and other network controllers."],
          ] as [Connection["type"], ReactNode, string, string][]
        ).map(([type, icon, title, text]) => (
          <button type="button" key={type} className={`choice ${value.type === type ? "is-active" : ""}`} onClick={() => pick(type)}>
            {icon}
            <b>{title}</b>
            <span>{text}</span>
          </button>
        ))}
      </div>

      {value.type === "simulator" && <Notice>Nothing is sent anywhere. Useful for trying things out or designing before the hardware arrives.</Notice>}

      {value.type === "serial" && (
        <>
          <Field label="Port" hint={ports.length === 0 ? "No serial ports found. Plug the board in and press refresh." : undefined}>
            <div className="tool-row">
              <Select
                value={value.port}
                options={[
                  ...(ports.some((p) => p.name === value.port) || !value.port ? [] : [{ value: value.port, label: `${value.port} (not connected)` }]),
                  ...(value.port ? [] : [{ value: "", label: "Choose a port..." }]),
                  ...ports.map((p) => ({ value: p.name, label: p.label })),
                ]}
                onChange={(port) => onChange({ ...value, port })}
              />
              <Button icon={<RefreshCw size={15} className={loading ? "spin" : ""} />} onClick={refresh} title="Look for ports again" />
            </div>
          </Field>
          <div className="field-pair">
            <Field label="Baud rate" hint="Must match the firmware.">
              <Select value={value.baud} options={BAUDS.map((b) => ({ value: b, label: b.toLocaleString("en-US") }))} onChange={(baud) => onChange({ ...value, baud })} />
            </Field>
            <Field label="Protocol">
              <Select
                value={value.protocol}
                options={[
                  { value: "adalight", label: "Adalight" },
                  { value: "tpm2", label: "TPM2" },
                ]}
                onChange={(protocol) => onChange({ ...value, protocol })}
              />
            </Field>
          </div>
          <Notice>
            The board needs firmware that understands these frames. Use <b>Firmware</b> in the sidebar to generate a sketch for
            your board, or run WLED, which also accepts Adalight and TPM2 over USB.
          </Notice>
        </>
      )}

      {value.type === "network" && (
        <>
          <Field label="Protocol" hint={NET_HINTS[value.protocol]}>
            <Select
              value={value.protocol}
              options={[
                { value: "ddp", label: "DDP (WLED and others)" },
                { value: "wled", label: "WLED realtime UDP" },
                { value: "artnet", label: "Art-Net" },
                { value: "e131", label: "E1.31 / sACN" },
              ]}
              onChange={(protocol) =>
                // Follow the standard port unless the user picked their own.
                onChange({ ...value, protocol, port: value.port === NET_PORTS[value.protocol] ? NET_PORTS[protocol] : value.port })
              }
            />
          </Field>
          <div className="field-pair">
            <Field label="IP address or hostname">
              <TextInput mono value={value.host} placeholder="192.168.1.50" onChange={(host) => onChange({ ...value, host: host.trim() })} />
            </Field>
            <Field label="Port">
              <NumberInput value={value.port} min={1} max={65535} onChange={(port) => onChange({ ...value, port: Math.round(port) })} />
            </Field>
          </div>
          {(value.protocol === "artnet" || value.protocol === "e131") && (
            <Field label="First universe" row>
              <NumberInput value={value.universe} min={0} max={32767} onChange={(universe) => onChange({ ...value, universe: Math.round(universe) })} />
            </Field>
          )}
        </>
      )}
    </>
  );
}

const TEST_MODES: [string, string][] = [
  ["chase", "Chase"],
  ["ends", "Ends"],
  ["red", "Red"],
  ["green", "Green"],
  ["blue", "Blue"],
  ["white", "White"],
];

export function DeviceEditor({ deviceId }: { deviceId: string | null }) {
  const devices = useApp((s) => s.config.devices);
  const addDevice = useApp((s) => s.addDevice);
  const updateDevice = useApp((s) => s.updateDevice);
  const removeDevice = useApp((s) => s.removeDevice);
  const openDialog = useApp((s) => s.openDialog);

  const existing = devices.find((d) => d.id === deviceId) ?? null;
  // What to go back to if the dialog is cancelled.
  const original = useRef(existing);
  const [draft, setDraft] = useState<Device>(() => existing ?? newDevice(devices.length));
  const [tab, setTab] = useState<"layout" | "connection" | "output">("layout");
  const [test, setTest] = useState<string | null>(null);
  const [probe, setProbe] = useState(0);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const preview = useLayoutPreview(draft.layout);
  const total = ledCount(draft.layout);
  const isNew = existing == null;

  // Changes to a device that already exists show up on the LEDs immediately -
  // except the connection, which only switches when asked, so half-typed
  // addresses and ports are never sent anything.
  const [liveConnection, setLiveConnection] = useState<Connection | null>(existing?.connection ?? null);
  const link = useApp((s) => (deviceId ? s.status?.devices[deviceId]?.link : undefined));
  const change = (next: Device) => {
    setDraft(next);
    if (!isNew && liveConnection) updateDevice(next.id, () => ({ ...next, connection: liveConnection }));
  };
  const connectionPending = !isNew && JSON.stringify(draft.connection) !== JSON.stringify(liveConnection);
  const applyConnection = () => {
    setLiveConnection(draft.connection);
    updateDevice(draft.id, () => draft);
  };

  const runTest = (mode: string | null, index?: number) => {
    setTest(mode);
    if (!isNew) void api.identify(draft.id, mode ?? "off", index);
  };

  useEffect(
    () => () => {
      if (deviceId) void api.identify(deviceId, "off");
    },
    [deviceId],
  );

  const close = () => openDialog(null);
  const cancel = () => {
    if (original.current) updateDevice(original.current.id, () => original.current!);
    close();
  };
  const save = () => {
    if (isNew) addDevice({ ...draft, name: draft.name.trim() || "LEDs" });
    else updateDevice(draft.id, () => draft);
    close();
  };

  const setType = (type: LayoutType) => {
    if (type !== draft.layout.type) change({ ...draft, layout: defaultLayout(type) });
  };

  const setConnection = (connection: Connection) => {
    // WLED-style controllers already gamma-correct; bare strips do not.
    const gamma = isNew && connection.type !== draft.connection.type ? (connection.type === "network" ? 1 : 2.2) : draft.output.gamma;
    change({ ...draft, connection, output: { ...draft.output, gamma } });
  };

  const out = draft.output;
  const setOut = (patch: Partial<Device["output"]>) => change({ ...draft, output: { ...out, ...patch } });
  const fullWhiteAmps = (total * 60) / 1000;

  return (
    <Modal
      title={isNew ? "Add a device" : `Set up ${existing.name}`}
      subtitle="Describe the LEDs and how they are connected."
      onClose={cancel}
      width={980}
      footer={
        <>
          {!isNew &&
            (confirmDelete ? (
              <>
                <span className="muted">Remove this device?</span>
                <Button
                  variant="danger"
                  onClick={() => {
                    removeDevice(draft.id);
                    close();
                  }}
                >
                  Yes, remove
                </Button>
                <Button onClick={() => setConfirmDelete(false)}>Keep</Button>
              </>
            ) : (
              <Button variant="ghost" icon={<Trash2 size={15} />} onClick={() => setConfirmDelete(true)}>
                Remove device
              </Button>
            ))}
          <span className="spacer" />
          <Button onClick={cancel}>Cancel</Button>
          <Button variant="primary" onClick={save}>
            {isNew ? "Add device" : "Done"}
          </Button>
        </>
      }
    >
      <div className="setup">
        <div className="setup__form">
          <Field label="Name">
            <TextInput value={draft.name} onChange={(name) => change({ ...draft, name })} placeholder="Desk backlight" />
          </Field>

          <Segmented
            stretch
            value={tab}
            onChange={setTab}
            options={[
              { value: "layout", label: (<><Cable size={15} /> Shape &amp; wiring</>) },
              { value: "connection", label: (<><Usb size={15} /> Connection</>) },
              { value: "output", label: (<><SlidersHorizontal size={15} /> Colour &amp; power</>) },
            ]}
          />

          {tab === "layout" && (
            <>
              <div className="type-grid">
                {TYPES.map((t) => (
                  <button type="button" key={t.type} className={`type-card ${draft.layout.type === t.type ? "is-active" : ""}`} onClick={() => setType(t.type)}>
                    {LAYOUT_ICONS[t.type](22)}
                    <b>{LAYOUT_NAMES[t.type]}</b>
                    <span>{t.blurb}</span>
                  </button>
                ))}
              </div>
              <LayoutFields layout={draft.layout} onChange={(layout) => change({ ...draft, layout })} />
              {total > 16384 && <Notice kind="warn">A device can have up to 16,384 LEDs. Anything beyond that is ignored.</Notice>}
            </>
          )}

          {tab === "connection" && (
            <>
              <ConnectionFields value={draft.connection} onChange={setConnection} />
              {!isNew &&
                (connectionPending ? (
                  <div className="tool-row">
                    <Button variant="primary" onClick={applyConnection}>
                      Connect now
                    </Button>
                    <span className="muted small">Or press Done. Nothing is sent to the new connection until then.</span>
                  </div>
                ) : (
                  link && (
                    <div className="link-status">
                      <i className={`dot dot--${link.state}`} />
                      {link.state === "simulator" ? "Simulator: nothing is being sent." : link.message}
                      {link.fps > 0 ? ` (${Math.round(link.fps)} frames per second)` : ""}
                    </div>
                  )
                ))}
            </>
          )}

          {tab === "output" && (
            <>
              <Field label="Colour order" row hint="If a pure red test shows green (or any other mix-up), change this until Red, Green and Blue all match.">
                <Select
                  value={out.colorOrder}
                  options={(["RGB", "GRB", "BRG", "RBG", "GBR", "BGR"] as ColorOrder[]).map((o) => ({ value: o, label: o }))}
                  onChange={(colorOrder) => setOut({ colorOrder })}
                />
              </Field>
              <Slider
                label="Gamma"
                min={1}
                max={3}
                step={0.05}
                value={out.gamma}
                resetTo={2.2}
                format={(v) => (v === 1 ? "Off" : v.toFixed(2))}
                hint="Makes fades look even. About 2.2 for bare strips; turn off if the controller (e.g. WLED) already corrects."
                onChange={(gamma) => setOut({ gamma })}
              />
              <div className="field__label">White balance</div>
              {(["Red", "Green", "Blue"] as const).map((name, i) => (
                <Slider
                  key={name}
                  label={name}
                  min={0.3}
                  max={1}
                  step={0.01}
                  value={out.whiteBalance[i]}
                  resetTo={1}
                  format={(v) => `${Math.round(v * 100)}%`}
                  onChange={(v) => {
                    const whiteBalance = [...out.whiteBalance] as [number, number, number];
                    whiteBalance[i] = v;
                    setOut({ whiteBalance });
                  }}
                />
              ))}
              <Field
                label="Power limit"
                hint={`Dims frames that would draw more than this. All ${total} LEDs at full white is roughly ${fullWhiteAmps.toFixed(1)} A.`}
              >
                <div className="tool-row">
                  <NumberInput value={out.maxMilliamps} min={0} max={200000} step={100} suffix="mA" onChange={(v) => setOut({ maxMilliamps: Math.round(v) })} />
                  {(
                    [
                      [0, "Off"],
                      [500, "USB"],
                      [2000, "2 A"],
                      [5000, "5 A"],
                      [10000, "10 A"],
                    ] as [number, string][]
                  ).map(([ma, label]) => (
                    <Button key={ma} size="sm" active={out.maxMilliamps === ma} onClick={() => setOut({ maxMilliamps: ma })}>
                      {label}
                    </Button>
                  ))}
                </div>
              </Field>
              <Slider label="Maximum frame rate" min={10} max={120} step={1} value={out.maxFps} resetTo={60} format={(v) => `${v} fps`} onChange={(maxFps) => setOut({ maxFps })} />
            </>
          )}
        </div>

        <div className="setup__preview">
          <div className="setup__diagram">
            <LedCanvas layout={preview} diagram highlight={test === "index" ? probe : undefined} autoRotate />
          </div>
          <div className="setup__caption">
            <b>{total} LEDs.</b> The line shows the order the data travels, from <span className="tag tag--start">START</span> to{" "}
            <span className="tag tag--end">END</span>. It should match your wiring.
          </div>

          <div className="setup__test">
            <div className="field__label">Check it on the real LEDs</div>
            {isNew ? (
              <p className="muted small">Add the device first, then reopen this dialog to run wiring tests on the hardware.</p>
            ) : (
              <>
                <div className="tool-row">
                  {TEST_MODES.map(([mode, label]) => (
                    <Button key={mode} size="sm" active={test === mode} onClick={() => runTest(test === mode ? null : mode)}>
                      {label}
                    </Button>
                  ))}
                </div>
                <Slider
                  label="Light a single LED"
                  min={0}
                  max={Math.max(0, total - 1)}
                  step={1}
                  value={Math.min(probe, Math.max(0, total - 1))}
                  format={(v) => `LED ${v}`}
                  onChange={(v) => {
                    setProbe(v);
                    runTest("index", v);
                  }}
                />
                <p className="muted small">
                  <b>Chase</b> runs a dot from the first LED (green) to the last (red). <b>Light a single LED</b> rings it here
                  and lights it for real, so you can check any spot.
                  {test && (
                    <>
                      {" "}
                      <button type="button" className="link" onClick={() => runTest(null)}>
                        Stop test
                      </button>
                    </>
                  )}
                </p>
              </>
            )}
          </div>
        </div>
      </div>
    </Modal>
  );
}
