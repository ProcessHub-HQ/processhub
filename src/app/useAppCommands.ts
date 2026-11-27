import { open, save } from "@tauri-apps/plugin-dialog";
import { useCallback } from "react";
import { processHub } from "../lib/processhub";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";

export function useAppCommands() {
  const runGlobalAction = useCallback(
    async (action: "start" | "stop" | "restart") => {
      const ui = useUiStore.getState();
      ui.setLifecycleError(null);
      try {
        if (action === "stop") {
          const failures = await processHub.stopAllServices();
          if (failures.length > 0) {
            ui.setLifecycleError(
              `Some services could not be stopped: ${failures.join("; ")}`,
            );
          }
        } else {
          const report =
            action === "start"
              ? await processHub.startAllServices()
              : await processHub.restartAllServices();
          if (report.failures.length > 0) {
            ui.setLifecycleError(
              `Started ${report.started.length} service(s); ${report.failures.length} failed: ${report.failures.join("; ")}`,
            );
          }
        }
        await useServicesStore.getState().refresh();
      } catch (cause) {
        useUiStore.getState().setLifecycleError(String(cause));
      }
    },
    [],
  );

  const exportConfiguration = useCallback(async () => {
    const ui = useUiStore.getState();
    ui.setLifecycleError(null);
    try {
      const path = await save({
        defaultPath: "processhub-config.json",
        filters: [{ name: "ProcessHub configuration", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      await processHub.exportConfigurationTo(path);
      useUiStore.getState().setNotice("Configuration exported.");
    } catch (cause) {
      useUiStore.getState().setLifecycleError(String(cause));
    }
  }, []);

  const importConfiguration = useCallback(async () => {
    const ui = useUiStore.getState();
    ui.setLifecycleError(null);
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "ProcessHub configuration", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      const report = await processHub.importConfigurationFrom(path);
      await useServicesStore.getState().refresh();
      const details: string[] = [];
      if (report.renamedServices) {
        details.push(`${report.renamedServices} renamed to avoid duplicates`);
      }
      if (report.droppedSecrets) {
        details.push(`${report.droppedSecrets} secret(s) skipped`);
      }
      const detail = details.length > 0 ? ` ${details.join("; ")}.` : "";
      useUiStore
        .getState()
        .setNotice(`Imported ${report.importedServices} service(s).${detail}`);
    } catch (cause) {
      useUiStore.getState().setLifecycleError(String(cause));
    }
  }, []);

  const removeService = useCallback(async () => {
    const ui = useUiStore.getState();
    if (!ui.pendingDelete) return;
    const deleted = await useServicesStore
      .getState()
      .deleteService(ui.pendingDelete);
    if (deleted) {
      useUiStore.getState().setSelectedId(null);
      useUiStore.getState().setPendingDelete(null);
    }
  }, []);

  return {
    runGlobalAction,
    exportConfiguration,
    importConfiguration,
    removeService,
  };
}
