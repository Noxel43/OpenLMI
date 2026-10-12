import { listen } from "@tauri-apps/api/event";
import { Lightbulb, Plus } from "lucide-react";
import { useEffect } from "react";
import { api } from "./api";
import { DeviceEditor } from "./components/DeviceEditor";
import { DeviceView } from "./components/DeviceView";
import { FirmwareDialog } from "./components/FirmwareDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { Sidebar } from "./components/Sidebar";
import { Button, Empty } from "./components/ui";
import { useApp, useSelectedDevice } from "./store";

export default function App() {
  const ready = useApp((s) => s.ready);
  const error = useApp((s) => s.error);
  const dialog = useApp((s) => s.dialog);
  const toasts = useApp((s) => s.toasts);
  const openDialog = useApp((s) => s.openDialog);
  const dismissToast = useApp((s) => s.dismissToast);
  const device = useSelectedDevice();

  useEffect(() => {
    void useApp.getState().init();
    // The tray menu can change things behind the UI's back.
    const unlisten = listen("config-changed", () => void useApp.getState().reloadConfig());
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  // Connection state, power draw and error notes.
  useEffect(() => {
    if (!ready) return;
    const poll = () => api.getStatus().then((status) => useApp.setState({ status })).catch(() => {});
    void poll();
    const timer = setInterval(poll, 500);
    return () => clearInterval(timer);
  }, [ready]);

  if (error) {
    return (
      <div className="boot">
        <Empty title="OpenLMI could not start">{error}</Empty>
      </div>
    );
  }
  if (!ready) return <div className="boot" />;

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        {device ? (
          <DeviceView key={device.id} device={device} />
        ) : (
          <Empty
            icon={<Lightbulb size={30} />}
            title="Add your first LEDs"
            action={
              <Button variant="primary" icon={<Plus size={16} />} onClick={() => openDialog({ type: "device", deviceId: null })}>
                Add device
              </Button>
            }
          >
            Tell OpenLMI what you have - a strip, a matrix, a cube or a backlight behind your monitor - and how it is
            connected. No hardware yet? Pick the simulator and explore.
          </Empty>
        )}
      </main>

      {dialog?.type === "device" && <DeviceEditor deviceId={dialog.deviceId} />}
      {dialog?.type === "settings" && <SettingsDialog />}
      {dialog?.type === "firmware" && <FirmwareDialog deviceId={dialog.deviceId} animationId={dialog.animationId} />}

      <div className="toasts">
        {toasts.map((t) => (
          <button type="button" key={t.id} className={`toast toast--${t.kind}`} onClick={() => dismissToast(t.id)}>
            {t.text}
          </button>
        ))}
      </div>
    </div>
  );
}
