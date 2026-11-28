import { open, save } from "@tauri-apps/plugin-dialog";
import { useCallback } from "react";
import { friendlyError, friendlyFailures } from "../lib/errors";
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
            ui.setLifecycleError(friendlyFailures(failures, "stop"));
          }
        } else {
          const report =
            action === "start"
              ? await processHub.startAllServices()
              : await processHub.restartAllServices();
          if (report.failures.length > 0) {
            ui.setLifecycleError(
              `Started ${report.started.length} service(s); ${friendlyFailures(report.failures, action)}`,
            );
          }
        }
        await useServicesStore.getState().refresh();
      } catch (cause) {
        const fallback =
          action === "start"
            ? "Services couldn't start. Check their logs for details."
            : action === "stop"
              ? "Services couldn't stop. Check their logs for details."
              : "Services couldn't restart. Check their logs for details.";
        useUiStore.getState().setLifecycleError(friendlyError(cause, fallback));
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
      useUiStore
        .getState()
        .setLifecycleError(
          friendlyError(
            cause,
            "The configuration couldn't be exported. Choose another location and try again.",
          ),
        );
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
      useUiStore
        .getState()
        .setLifecycleError(
          friendlyError(
            cause,
            "That configuration couldn't be imported. Check the file and try again.",
          ),
        );
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
