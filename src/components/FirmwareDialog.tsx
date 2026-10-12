import { save } from "@tauri-apps/plugin-dialog";
import { Copy, Save } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../api";
import { ledCount, useApp } from "../store";
import type { FirmwareOpts } from "../types";
import { Button, Field, Modal, Notice, NumberInput, Select } from "./ui";

const CHIPSETS: { value: string; label: string; order: string }[] = [
  { value: "WS2812B", label: "WS2812B / NeoPixel", order: "GRB" },
  { value: "SK6812", label: "SK6812 (RGB)", order: "GRB" },
  { value: "WS2811", label: "WS2811 (12 V pixels)", order: "RGB" },
  { value: "WS2813", label: "WS2813", order: "GRB" },
  { value: "WS2815", label: "WS2815 (12 V)", order: "GRB" },
  { value: "APA102", label: "APA102 / DotStar (data + clock)", order: "BGR" },
  { value: "SK9822", label: "SK9822 (data + clock)", order: "BGR" },
  { value: "WS2801", label: "WS2801 (data + clock)", order: "RGB" },
];
const CLOCKED = ["APA102", "SK9822", "WS2801"];

export function FirmwareDialog({ deviceId, animationId }: { deviceId?: string; animationId?: string }) {
  const device = useApp((s) => s.config.devices.find((d) => d.id === deviceId));
  const animation = useApp((s) => s.config.animations.find((a) => a.id === animationId));
  const openDialog = useApp((s) => s.openDialog);
  const toast = useApp((s) => s.toast);

  const [opts, setOpts] = useState<FirmwareOpts>(() => ({
    chipset: "WS2812B",
    dataPin: 6,
    clockPin: 7,
    count: device ? ledCount(device.layout) : (animation?.count ?? 60),
    colorOrder: "GRB",
    baud: device?.connection.type === "serial" ? device.connection.baud : 500000,
    maxMilliamps: device?.output.maxMilliamps ?? 0,
    idleTimeoutS: 5,
  }));
  const [code, setCode] = useState("");
  const set = (patch: Partial<FirmwareOpts>) => setOpts((o) => ({ ...o, ...patch }));

  useEffect(() => {
    let cancelled = false;
    const request = animationId ? api.animationSketch(animationId, opts) : api.firmwareSketch(opts);
    request.then(
      (text) => !cancelled && setCode(text),
      (e) => !cancelled && setCode(`// ${e}`),
    );
    return () => {
      cancelled = true;
    };
  }, [opts, animationId, animation]);

  const fileName = animationId ? "OpenLMI_Animation" : "OpenLMI_Receiver";

  const saveFile = async () => {
    const path = await save({ defaultPath: `${fileName}.ino`, filters: [{ name: "Arduino sketch", extensions: ["ino"] }] });
    if (!path) return;
    try {
      await api.writeTextFile(path, code);
      toast("Sketch saved", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const copy = async () => {
    await navigator.clipboard.writeText(code);
    toast("Copied to the clipboard", "success");
  };

  return (
    <Modal
      title={animationId ? `Export "${animation?.name ?? "animation"}" for Arduino` : "Firmware for your board"}
      subtitle={
        animationId
          ? "A standalone sketch: the board plays this animation by itself, no PC attached."
          : "A small sketch that makes the board show whatever OpenLMI sends over USB."
      }
      onClose={() => openDialog(null)}
      width={1000}
      footer={
        <>
          <span className="spacer" />
          <Button icon={<Copy size={15} />} onClick={copy}>
            Copy
          </Button>
          <Button variant="primary" icon={<Save size={15} />} onClick={saveFile}>
            Save sketch...
          </Button>
        </>
      }
    >
      <div className="firmware">
        <div className="firmware__form">
          <Field label="LED type">
            <Select
              value={opts.chipset}
              options={CHIPSETS.map((c) => ({ value: c.value, label: c.label }))}
              onChange={(chipset) => set({ chipset, colorOrder: CHIPSETS.find((c) => c.value === chipset)?.order ?? opts.colorOrder })}
            />
          </Field>
          <div className="field-pair">
            <Field label="Data pin">
              <NumberInput value={opts.dataPin} min={0} max={99} onChange={(dataPin) => set({ dataPin: Math.round(dataPin) })} />
            </Field>
            {CLOCKED.includes(opts.chipset) ? (
              <Field label="Clock pin">
                <NumberInput value={opts.clockPin} min={0} max={99} onChange={(clockPin) => set({ clockPin: Math.round(clockPin) })} />
              </Field>
            ) : (
              <Field label="LEDs">
                <NumberInput value={opts.count} min={1} max={4096} onChange={(count) => set({ count: Math.round(count) })} />
              </Field>
            )}
          </div>
          {CLOCKED.includes(opts.chipset) && (
            <Field label="LEDs">
              <NumberInput value={opts.count} min={1} max={4096} onChange={(count) => set({ count: Math.round(count) })} />
            </Field>
          )}
          <div className="field-pair">
            <Field label="Colour order" hint="Leave as suggested unless colours come out swapped.">
              <Select value={opts.colorOrder} options={["GRB", "RGB", "BRG", "RBG", "GBR", "BGR"].map((o) => ({ value: o, label: o }))} onChange={(colorOrder) => set({ colorOrder })} />
            </Field>
            {!animationId && (
              <Field label="Baud rate" hint="Use the same in the device's connection settings.">
                <Select
                  value={opts.baud}
                  options={[115200, 230400, 250000, 500000, 921600, 1000000].map((b) => ({ value: b, label: b.toLocaleString("en-US") }))}
                  onChange={(baud) => set({ baud })}
                />
              </Field>
            )}
          </div>
          <div className="field-pair">
            <Field label="Power limit" hint="0 = no limit.">
              <NumberInput value={opts.maxMilliamps} min={0} max={100000} step={100} suffix="mA" onChange={(v) => set({ maxMilliamps: Math.round(v) })} />
            </Field>
            {!animationId && (
              <Field label="Switch off after" hint="With no data from the PC. 0 = never.">
                <NumberInput value={opts.idleTimeoutS} min={0} max={3600} suffix="s" onChange={(v) => set({ idleTimeoutS: Math.round(v) })} />
              </Field>
            )}
          </div>

          <div className="steps">
            <div className="field__label">How to install it</div>
            <ol>
              <li>
                Install the free <b>Arduino IDE</b>, then add the <b>FastLED</b> library (Tools &gt; Manage Libraries).
              </li>
              <li>
                Save this sketch and open it in the IDE. It will offer to move the file into a folder called <code>{fileName}</code> - accept.
              </li>
              <li>Pick your board and port under Tools, then press Upload.</li>
              {!animationId && (
                <li>
                  Back in OpenLMI, set the device's connection to <b>USB / Serial</b>, protocol <b>Adalight</b>, baud{" "}
                  <b>{opts.baud.toLocaleString("en-US")}</b>.
                </li>
              )}
            </ol>
          </div>
          {!animationId && (
            <Notice>
              Have an ESP32 or ESP8266? <b>WLED</b> is a great ready-made alternative - flash it from install.wled.me and connect over
              Wi-Fi using the DDP protocol, no sketch needed.
            </Notice>
          )}
          {animationId && animation && (
            <Notice kind={animation.frames.length * opts.count * 3 > 28000 ? "warn" : "info"}>
              Frame data: {(animation.frames.length * opts.count * 3).toLocaleString("en-US")} bytes. An Arduino Uno or Nano has
              about 30,000 bytes of space; ESP32 and Pico boards have far more.
            </Notice>
          )}
        </div>
        <pre className="firmware__code">{code}</pre>
      </div>
    </Modal>
  );
}
