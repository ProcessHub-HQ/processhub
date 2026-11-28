import { Button, StatusDot } from "@processhub/design-system";
import { ArrowLeft, ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";
import { ServiceIcon } from "../components/ServiceIcon";
import { Sparkline } from "../components/Sparkline";
import { useMetricsStore } from "../stores/metricsStore";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";
import type { RuntimeState, RuntimeStatus } from "../types";

const activeStatuses = new Set([
  "running",
  "starting",
  "stopping",
  "waiting_to_restart",
]);

const statusLabels: Record<RuntimeStatus, string> = {
  stopped: "Stopped",
  starting: "Starting",
  running: "Running",
  stopping: "Stopping",
  exited: "Exited",
  waiting_to_restart: "Waiting to restart",
  failed: "Failed",
  unknown: "Unknown",
};

type SortKey = "cpu" | "memory" | "name";
type SortDirection = "asc" | "desc";

function cpuOf(state?: RuntimeState) {
  return state?.treeCpuUsagePercent ?? state?.cpuUsagePercent ?? 0;
}

function memoryOf(state?: RuntimeState) {
  return state?.treeMemoryBytes ?? state?.memoryUsageBytes ?? 0;
}

function formatMemory(bytes: number) {
  const megabytes = bytes / (1024 * 1024);
  return megabytes >= 1024
    ? `${(megabytes / 1024).toFixed(1)} GB`
    : `${Math.round(megabytes)} MB`;
}

function formatUptime(startedAtMs: number) {
  const totalMinutes = Math.max(
    0,
    Math.floor((Date.now() - startedAtMs) / 60_000),
  );
  if (totalMinutes < 1) return "<1m";
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
}

function processCountOf(state?: RuntimeState) {
  return state?.processCount ?? (state?.pid ? 1 : 0);
}

export function ProcessMonitor() {
  const services = useServicesStore((state) => state.services);
  const runtime = useServicesStore((state) => state.runtime);
  const history = useMetricsStore((state) => state.history);
  const setView = useUiStore((state) => state.setView);
  const [sort, setSort] = useState<{ key: SortKey; direction: SortDirection }>({
    key: "cpu",
    direction: "desc",
  });
  const tones = new Map(services.map((service, index) => [service.id, index]));

  let activeCount = 0;
  let totalProcesses = 0;
  let totalCpu = 0;
  let totalMemory = 0;

  for (const service of services) {
    const state = runtime[service.id];
    if (activeStatuses.has(state?.status ?? "")) {
      activeCount += 1;
      totalProcesses += processCountOf(state);
    }
    totalCpu += cpuOf(state);
    totalMemory += memoryOf(state);
  }

  function toggleSort(key: SortKey) {
    setSort((current) =>
      current.key === key
        ? {
            key,
            direction: current.direction === "asc" ? "desc" : "asc",
          }
        : { key, direction: key === "name" ? "asc" : "desc" },
    );
  }

  const rows = [...services].sort((left, right) => {
    if (sort.key === "name") {
      const order = left.name.localeCompare(right.name);
      return sort.direction === "asc" ? order : -order;
    }
    const leftValue =
      sort.key === "cpu" ? cpuOf(runtime[left.id]) : memoryOf(runtime[left.id]);
    const rightValue =
      sort.key === "cpu"
        ? cpuOf(runtime[right.id])
        : memoryOf(runtime[right.id]);
    const order = rightValue - leftValue;
    if (order !== 0) return sort.direction === "desc" ? order : -order;
    return left.name.localeCompare(right.name);
  });

  function sortIndicator(key: SortKey) {
    if (sort.key !== key) return null;
    return sort.direction === "asc" ? (
      <ChevronUp size={12} />
    ) : (
      <ChevronDown size={12} />
    );
  }

  function ariaSort(key: SortKey) {
    if (sort.key !== key) return "none" as const;
    return sort.direction === "asc"
      ? ("ascending" as const)
      : ("descending" as const);
  }

  return (
    <div className="monitor">
      <header className="monitor-header">
        <div>
          <h1>Process Monitor</h1>
          <p>Live CPU and memory for every service.</p>
        </div>
        <div className="monitor-header-actions">
          <Button
            size="sm"
            variant="secondary"
            icon={<ArrowLeft size={14} />}
            onClick={() => setView("workspace")}
          >
            Processes
          </Button>
        </div>
      </header>

      <div className="monitor-summary">
        <span>
          <strong>{activeCount}</strong> running
        </span>
        <span>
          <strong>{totalProcesses}</strong>{" "}
          {totalProcesses === 1 ? "process" : "processes"}
        </span>
        <span>
          CPU <strong>{totalCpu.toFixed(1)}%</strong>
        </span>
        <span>
          Memory <strong>{formatMemory(totalMemory)}</strong>
        </span>
        <span className="monitor-live">
          <StatusDot status="running" aria-hidden="true" />
          Live
        </span>
      </div>

      {services.length === 0 ? (
        <div className="monitor-empty">No processes to monitor yet.</div>
      ) : (
        <div
          className="monitor-table"
          role="table"
          aria-label="Process resource usage"
        >
          <div className="monitor-table-head" role="row">
            <div
              className="monitor-th"
              role="columnheader"
              aria-sort={ariaSort("name")}
            >
              <button
                type="button"
                className="monitor-sort-header"
                onClick={() => toggleSort("name")}
              >
                Process
                {sortIndicator("name")}
              </button>
            </div>
            <div className="monitor-th monitor-th-number" role="columnheader">
              PID
            </div>
            <div
              className="monitor-th monitor-th-number"
              role="columnheader"
              aria-sort={ariaSort("cpu")}
            >
              <button
                type="button"
                className="monitor-sort-header"
                onClick={() => toggleSort("cpu")}
              >
                CPU
                {sortIndicator("cpu")}
              </button>
            </div>
            <div
              className="monitor-th monitor-th-number"
              role="columnheader"
              aria-sort={ariaSort("memory")}
            >
              <button
                type="button"
                className="monitor-sort-header"
                onClick={() => toggleSort("memory")}
              >
                Memory
                {sortIndicator("memory")}
              </button>
            </div>
            <div className="monitor-th monitor-th-number" role="columnheader">
              Processes
            </div>
            <div className="monitor-th monitor-th-number" role="columnheader">
              Uptime
            </div>
          </div>

          <div role="rowgroup">
            {rows.map((service) => {
              const state = runtime[service.id];
              const status = state?.status ?? "stopped";
              const active = activeStatuses.has(status);
              const samples = history[service.id] ?? [];
              const cpu = cpuOf(state);
              const memory = memoryOf(state);
              const processes = processCountOf(state);

              return (
                <div
                  key={service.id}
                  className={`monitor-row ${active ? "" : "is-idle"}`}
                  role="row"
                >
                  <div
                    className="monitor-cell monitor-cell-process"
                    role="cell"
                  >
                    <div className="monitor-process">
                      <ServiceIcon
                        service={service}
                        state={state}
                        tone={tones.get(service.id) ?? 0}
                      />
                      <div className="monitor-process-info">
                        <span className="monitor-process-name">
                          {service.name}
                        </span>
                        <span className={`monitor-process-status ${status}`}>
                          {statusLabels[status]}
                        </span>
                      </div>
                    </div>
                  </div>
                  <div className="monitor-cell monitor-number" role="cell">
                    {state?.pid ?? "—"}
                  </div>
                  <div className="monitor-cell" role="cell">
                    <div className="monitor-metric">
                      <span className="monitor-value">
                        {active ? `${cpu.toFixed(1)}%` : "—"}
                      </span>
                      <Sparkline
                        values={
                          active ? samples.map((sample) => sample.cpu) : []
                        }
                        className="sparkline--cpu"
                      />
                    </div>
                  </div>
                  <div className="monitor-cell" role="cell">
                    <div className="monitor-metric">
                      <span className="monitor-value">
                        {active ? formatMemory(memory) : "—"}
                      </span>
                      <Sparkline
                        values={
                          active ? samples.map((sample) => sample.memory) : []
                        }
                        className="sparkline--memory"
                      />
                    </div>
                  </div>
                  <div className="monitor-cell monitor-number" role="cell">
                    {processes > 0 ? processes : "—"}
                  </div>
                  <div className="monitor-cell monitor-number" role="cell">
                    {state?.startedAtMs ? formatUptime(state.startedAtMs) : "—"}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
