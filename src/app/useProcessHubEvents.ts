import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect } from "react";
import { processHubEvents } from "../lib/events";
import { processHub } from "../lib/processhub";
import { useLogsStore } from "../stores/logsStore";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";

const FLUSH_INTERVAL_MS = 50;
const TERMINAL_STATUSES = ["stopped", "exited", "failed", "waiting_to_restart"];

export function useProcessHubEvents() {
  useEffect(() => {
    void useServicesStore.getState().refresh();

    void processHub
      .getSetting<number>("logRetentionDays")
      .then((days) => {
        if (days !== null) useLogsStore.getState().setRetentionDays(days);
      })
      .catch(() => {});

    const flushInterval = window.setInterval(
      () => useLogsStore.getState().flush(),
      FLUSH_INTERVAL_MS,
    );

    let disposed = false;
    let unlistenRuntime: (() => void) | undefined;
    let unlistenLogs: (() => void) | undefined;

    void processHubEvents
      .onRuntimeState((state) => {
        useServicesStore.getState().setRuntimeState(state);
        if (TERMINAL_STATUSES.includes(state.status)) {
          void useLogsStore
            .getState()
            .load(state.serviceId)
            .catch(() => {});
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlistenRuntime = stop;
      })
      .catch(() => {});

    void processHubEvents
      .onLogBatch((events) => useLogsStore.getState().enqueue(events))
      .then((stop) => {
        if (disposed) stop();
        else unlistenLogs = stop;
      })
      .catch(() => {});

    const unlisten = Promise.all([
      listen<{ started: string[]; failures: string[] }>(
        "startup-report",
        ({ payload }) => {
          if (payload.failures.length > 0) {
            useUiStore
              .getState()
              .setLifecycleError(
                `Started ${payload.started.length} service(s); ${payload.failures.length} failed: ${payload.failures.join("; ")}`,
              );
          }
        },
      ),
      listen("quit-confirmation-required", () =>
        useUiStore.getState().setQuitConfirmationOpen(true),
      ),
      listen<string>("open-service-details", ({ payload }) => {
        useUiStore.getState().setSelectedId(payload);
        useUiStore.getState().setLogServiceId(payload);
      }),
      listen<string>("tray-open-url", ({ payload }) => {
        void openUrl(payload).catch((cause: unknown) =>
          useUiStore.getState().setLifecycleError(String(cause)),
        );
      }),
      listen<{ action: string; failures: string[] }>(
        "tray-action-report",
        ({ payload }) => {
          useUiStore
            .getState()
            .setLifecycleError(
              payload.failures.length
                ? `Some services could not be stopped: ${payload.failures.join("; ")}`
                : null,
            );
        },
      ),
    ]).then((stops) => {
      if (disposed) {
        stops.forEach((stop) => {
          stop();
        });
        return stops;
      }
      void processHub
        .recoverOrphanedServices()
        .then(async (report) => {
          if (report.unverifiable.length || report.failures.length) {
            useUiStore
              .getState()
              .setLifecycleError(
                `Leftover processes for ${[...report.unverifiable, ...report.failures].join(", ")} could not be verified, so they were left untouched. Those services will not auto-start to avoid duplicates; stop the processes manually, then start the service again.`,
              );
          }
          const enabled = await processHub.getSetting<boolean>(
            "startAutoStartServices",
          );
          if (enabled !== false) {
            await processHub.startAutoStartServices();
          }
        })
        .catch((cause: unknown) =>
          useUiStore.getState().setLifecycleError(String(cause)),
        );
      return stops;
    });

    return () => {
      disposed = true;
      window.clearInterval(flushInterval);
      unlistenRuntime?.();
      unlistenLogs?.();
      void unlisten.then((stops) => {
        stops.forEach((stop) => {
          stop();
        });
      });
    };
  }, []);
}
