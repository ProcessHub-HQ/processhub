import { useEffect } from "react";
import { useUiStore } from "../stores/uiStore";

export function useAppShortcuts(
  runGlobalAction: (action: "start" | "stop" | "restart") => Promise<void>,
) {
  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        useUiStore.getState().setSearchOpen(true);
      }
    };
    const onNew = () => useUiStore.getState().openEditor();
    const onStartup = () => useUiStore.getState().setStartupSettingsOpen(true);
    const onAbout = () => useUiStore.getState().setAboutOpen(true);
    const onQuit = () => useUiStore.getState().setQuitConfirmationOpen(true);
    const onStartAll = () => void runGlobalAction("start");
    const onStopAll = () => void runGlobalAction("stop");
    const onRestartAll = () => void runGlobalAction("restart");

    window.addEventListener("keydown", onShortcut);
    window.addEventListener("processhub:new", onNew);
    window.addEventListener("processhub:startup-settings", onStartup);
    window.addEventListener("processhub:about", onAbout);
    window.addEventListener("processhub:quit", onQuit);
    window.addEventListener("processhub:start-all", onStartAll);
    window.addEventListener("processhub:stop-all", onStopAll);
    window.addEventListener("processhub:restart-all", onRestartAll);
    return () => {
      window.removeEventListener("keydown", onShortcut);
      window.removeEventListener("processhub:new", onNew);
      window.removeEventListener("processhub:startup-settings", onStartup);
      window.removeEventListener("processhub:about", onAbout);
      window.removeEventListener("processhub:quit", onQuit);
      window.removeEventListener("processhub:start-all", onStartAll);
      window.removeEventListener("processhub:stop-all", onStopAll);
      window.removeEventListener("processhub:restart-all", onRestartAll);
    };
  }, [runGlobalAction]);
}
