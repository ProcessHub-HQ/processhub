import { ArrowUpCircle, Play, Square } from "lucide-react";
import { useState } from "react";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";
import { useAppCommands } from "./useAppCommands";

function formatMemory(bytes: number) {
  const megabytes = bytes / (1024 * 1024);
  return megabytes >= 1024
    ? `${(megabytes / 1024).toFixed(1)} GB`
    : `${Math.round(megabytes)} MB`;
}

export function StatusBar() {
  const services = useServicesStore((state) => state.services);
  const runtime = useServicesStore((state) => state.runtime);
  const availableUpdate = useUiStore((state) => state.availableUpdate);
  const openUpdateDialog = useUiStore((state) => state.openUpdateDialog);
  const { runGlobalAction } = useAppCommands();
  const [bulkAction, setBulkAction] = useState<"start" | "stop" | null>(null);

  const activeCount = services.filter((service) =>
    ["running", "starting", "stopping", "waiting_to_restart"].includes(
      runtime[service.id]?.status ?? "",
    ),
  ).length;
  const failedCount = services.filter(
    (service) => runtime[service.id]?.status === "failed",
  ).length;

  let cpu = 0;
  let memory = 0;
  let hasCpu = false;
  let hasMemory = false;
  for (const service of services) {
    const state = runtime[service.id];
    if (!state) continue;
    const serviceCpu = state.treeCpuUsagePercent ?? state.cpuUsagePercent;
    if (serviceCpu !== null && serviceCpu !== undefined) {
      cpu += serviceCpu;
      hasCpu = true;
    }
    const serviceMemory = state.treeMemoryBytes ?? state.memoryUsageBytes;
    if (serviceMemory !== null && serviceMemory !== undefined) {
      memory += serviceMemory;
      hasMemory = true;
    }
  }

  const idle = services.length > 0 && activeCount === 0;
  const label =
    services.length === 0 ? "Ready" : idle ? "Idle" : `${activeCount} active`;
  const dotClass =
    services.length === 0 || idle
      ? "idle"
      : failedCount > 0
        ? "warning"
        : "running";

  return (
    <footer className="status-bar">
      <div className="status-group">
        <span className="status-item">
          <span className={`status-dot ${dotClass}`} aria-hidden="true" />
          {label}
        </span>
        {failedCount > 0 && (
          <span className="status-item failed">
            <span className="status-dot failed" aria-hidden="true" />
            {failedCount} failed
          </span>
        )}
      </div>

      <div className="status-group status-group-right">
        {(hasCpu || hasMemory) && (
          <span className="status-item status-metrics">
            {hasCpu && <span>CPU {cpu.toFixed(1)}%</span>}
            {hasCpu && hasMemory && <span aria-hidden="true">·</span>}
            {hasMemory && <span>MEM {formatMemory(memory)}</span>}
          </span>
        )}
        <span className="status-actions">
          <button
            className="status-action"
            type="button"
            disabled={services.length === 0 || bulkAction !== null}
            aria-busy={bulkAction === "start"}
            onClick={() => {
              setBulkAction("start");
              void runGlobalAction("start").finally(() => setBulkAction(null));
            }}
          >
            <Play size={10} />
            Start all
          </button>
          <button
            className="status-action"
            type="button"
            disabled={activeCount === 0 || bulkAction !== null}
            aria-busy={bulkAction === "stop"}
            onClick={() => {
              setBulkAction("stop");
              void runGlobalAction("stop").finally(() => setBulkAction(null));
            }}
          >
            <Square size={10} />
            Stop all
          </button>
        </span>
        {availableUpdate && (
          <button
            className="status-update"
            type="button"
            title={`Update to ${availableUpdate.version}`}
            onClick={openUpdateDialog}
          >
            <ArrowUpCircle size={11} />
            Update {availableUpdate.version}
          </button>
        )}
        <span className="status-version">v{__APP_VERSION__}</span>
      </div>
    </footer>
  );
}
