import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Download, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../api";
import { useApp } from "../store";
import { Button, Field, Modal, Slider, Toggle } from "./ui";

export function SettingsDialog() {
  const settings = useApp((s) => s.config.settings);
  const version = useApp((s) => s.version);
  const configPath = useApp((s) => s.configPath);
  const saveSettings = useApp((s) => s.saveSettings);
  const replaceConfig = useApp((s) => s.replaceConfig);
  const openDialog = useApp((s) => s.openDialog);
  const toast = useApp((s) => s.toast);
  const [autostart, setAutostart] = useState(false);

  useEffect(() => {
    isEnabled().then(setAutostart).catch(() => {});
  }, []);

  const toggleAutostart = async (on: boolean) => {
    try {
      await (on ? enable() : disable());
      setAutostart(on);
    } catch (e) {
      toast(`Could not change the start-up setting: ${e}`, "error");
    }
  };

  const exportBackup = async () => {
    const path = await save({ defaultPath: "openlmi-backup.json", filters: [{ name: "OpenLMI backup", extensions: ["json"] }] });
    if (!path) return;
    try {
      await api.writeTextFile(path, JSON.stringify(await api.getConfig(), null, 2));
      toast("Backup saved", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  const importBackup = async () => {
    const path = await open({ multiple: false, filters: [{ name: "OpenLMI backup", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      replaceConfig(await api.importConfig(await api.readTextFile(path)));
      toast("Backup restored", "success");
    } catch (e) {
      toast(String(e), "error");
    }
  };

  return (
    <Modal title="Settings" onClose={() => openDialog(null)} width={560} footer={<><span className="spacer" /><Button variant="primary" onClick={() => openDialog(null)}>Done</Button></>}>
      <div className="settings">
        <section>
          <h4>General</h4>
          <Toggle
            label="Keep running when the window is closed"
            hint="OpenLMI stays in the system tray so the lights keep going. Quit from the tray icon."
            checked={settings.closeToTray}
            onChange={(closeToTray) => saveSettings({ ...settings, closeToTray })}
          />
          <Toggle label="Start with the computer" hint="Launch OpenLMI when you sign in." checked={autostart} onChange={toggleAutostart} />
          <Toggle
            label="Start hidden in the tray"
            hint="Only applies when started with the computer."
            checked={settings.startMinimized}
            onChange={(startMinimized) => saveSettings({ ...settings, startMinimized })}
          />
        </section>

        <section>
          <h4>Performance</h4>
          <Slider
            label="Render rate"
            min={20}
            max={120}
            step={1}
            value={settings.fps}
            resetTo={60}
            format={(v) => `${v} fps`}
            hint="How often frames are drawn. Each device also has its own maximum under Set up."
            onChange={(fps) => saveSettings({ ...settings, fps })}
          />
        </section>

        <section>
          <h4>Backup</h4>
          <p className="muted small">Devices, animations, scripts and presets in one file.</p>
          <div className="tool-row">
            <Button icon={<Download size={15} />} onClick={exportBackup}>
              Export...
            </Button>
            <Button icon={<Upload size={15} />} onClick={importBackup}>
              Restore...
            </Button>
          </div>
        </section>

        <section>
          <h4>About</h4>
          <Field label="Version" row>
            <span>OpenLMI {version}</span>
          </Field>
          <Field label="Settings file">
            <code className="path">{configPath}</code>
          </Field>
          <p className="muted small">Open Microcontroller LED Integration. MIT licensed. github.com/Noxel43/OpenLMI</p>
        </section>
      </div>
    </Modal>
  );
}
