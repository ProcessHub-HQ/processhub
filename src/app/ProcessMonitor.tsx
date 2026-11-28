import { Button, StatusDot } from "@processhub/design-system";
import { ArrowLeft, ChevronDown, ChevronUp, Cpu } from "lucide-react";
import { useEffect, useState } from "react";
import {
  SegmentedControl,
  type SegmentedControlOption,
} from "../components/SegmentedControl";
import { ServiceIcon } from "../components/ServiceIcon";
import { Sparkline } from "../components/Sparkline";
import { friendlyError } from "../lib/errors";
import { processHub } from "../lib/processhub";
import { useMetricsStore } from "../stores/metricsStore";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";
import type { RuntimeState, RuntimeStatus, SystemProcess } from "../types";

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

type MonitorMode = "services" | "system";
type SortKey = "cpu" | "memory" | "name";
type SortDirection = "asc" | "desc";
type SortState = { key: SortKey; direction: SortDirection };

const MONITOR_MODES: readonly SegmentedControlOption<MonitorMode>[] = [
  { value: "services", label: "Services" },
  { value: "system", label: "System" },
];

const SYSTEM_POLL_INTERVAL_MS = 2000;
const SYSTEM_ERROR_FALLBACK = "The process list couldn't be loaded. Try again.";

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

function formatRunTime(seconds: number) {
  const totalMinutes = Math.max(0, Math.floor(seconds / 60));
  if (totalMinutes < 1) return "<1m";
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours < 24) return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

function processCountOf(state?: RuntimeState) {
  return state?.processCount ?? (state?.pid ? 1 : 0);
}

function useSystemProcesses(active: boolean) {
  const [processes, setProcesses] = useState<SystemProcess[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reloadKey restarts the poll after a retry.
  useEffect(() => {
    if (!active) return;

    let cancelled = false;
    let timer: number | undefined;

    setLoading(true);
    setError(null);

    const load = async () => {
      try {
        const next = await processHub.listSystemProcesses();
        if (cancelled) return;
        setProcesses(next);
      } catch (cause) {
        if (cancelled) return;
        setError(friendlyError(cause, SYSTEM_ERROR_FALLBACK));
        setLoading(false);
        return;
      }
      if (cancelled) return;
      setLoading(false);
      timer = window.setTimeout(() => void load(), SYSTEM_POLL_INTERVAL_MS);
    };

    void load();

    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [active, reloadKey]);

  const retry = () => {
    setError(null);
    setLoading(true);
    setReloadKey((key) => key + 1);
  };

  return { processes, loading, error, retry };
}

export function ProcessMonitor() {
  const services = useServicesStore((state) => state.services);
  const runtime = useServicesStore((state) => state.runtime);
  const history = useMetricsStore((state) => state.history);
  const setView = useUiStore((state) => state.setView);
  const [mode, setMode] = useState<MonitorMode>("services");
  const [sort, setSort] = useState<SortState>({
    key: "cpu",
    direction: "desc",
  });
  const [systemSort, setSystemSort] = useState<SortState>({
    key: "cpu",
    direction: "desc",
  });
  const system = useSystemProcesses(mode === "system");
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

  const systemTotalCpu = system.processes.reduce(
    (total, process) => total + process.cpuPercent,
    0,
  );
  const systemTotalMemory = system.processes.reduce(
    (total, process) => total + process.memoryBytes,
    0,
  );

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

  function toggleSystemSort(key: SortKey) {
    setSystemSort((current) =>
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

  const systemRows = [...system.processes].sort((left, right) => {
    if (systemSort.key === "name") {
      const order = left.name.localeCompare(right.name);
      return systemSort.direction === "asc" ? order : -order;
    }
    const leftValue =
      systemSort.key === "cpu" ? left.cpuPercent : left.memoryBytes;
    const rightValue =
      systemSort.key === "cpu" ? right.cpuPercent : right.memoryBytes;
    const order = rightValue - leftValue;
    if (order !== 0) return systemSort.direction === "desc" ? order : -order;
    return left.name.localeCompare(right.name);
  });

  function sortIndicator(current: SortState, key: SortKey) {
    if (current.key !== key) return null;
    return current.direction === "asc" ? (
      <ChevronUp size={12} />
    ) : (
      <ChevronDown size={12} />
    );
  }

  function ariaSort(current: SortState, key: SortKey) {
    if (current.key !== key) return "none" as const;
    return current.direction === "asc"
      ? ("ascending" as const)
      : ("descending" as const);
  }

  return (
    <div className="monitor">
      <header className="monitor-header">
        <div>
          <h1>Process Monitor</h1>
          <p>
            {mode === "system"
              ? "Live CPU and memory for your processes."
              : "Live CPU and memory for every service."}
          </p>
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

      <SegmentedControl
        label="Process monitor view"
        value={mode}
        options={MONITOR_MODES}
        onChange={setMode}
        className="monitor-tabs"
      />

      {mode === "system" ? (
        system.error ? (
          <div className="monitor-error" role="alert">
            <span>{system.error}</span>
            <Button size="sm" variant="secondary" onClick={system.retry}>
              Try again
            </Button>
          </div>
        ) : system.loading ? (
          <div className="monitor-empty">Loading processes…</div>
        ) : (
          <>
            <div className="monitor-summary">
              <span>
                <strong>{system.processes.length}</strong>{" "}
                {system.processes.length === 1 ? "process" : "processes"}
              </span>
              <span>
                CPU <strong>{systemTotalCpu.toFixed(1)}%</strong>
              </span>
              <span>
                Memory <strong>{formatMemory(systemTotalMemory)}</strong>
              </span>
              <span className="monitor-live">
                <StatusDot status="running" aria-hidden="true" />
                Live
              </span>
            </div>

            {systemRows.length === 0 ? (
              <div className="monitor-empty">No processes to show.</div>
            ) : (
              <div
                className="monitor-table monitor-table--system"
                role="table"
                aria-label="System process resource usage"
              >
                <div className="monitor-table-head" role="row">
                  <div
                    className="monitor-th"
                    role="columnheader"
                    aria-sort={ariaSort(systemSort, "name")}
                  >
                    <button
                      type="button"
                      className="monitor-sort-header"
                      onClick={() => toggleSystemSort("name")}
                    >
                      Process
                      {sortIndicator(systemSort, "name")}
                    </button>
                  </div>
                  <div
                    className="monitor-th monitor-th-number"
                    role="columnheader"
                  >
                    PID
                  </div>
                  <div
                    className="monitor-th monitor-th-number"
                    role="columnheader"
                    aria-sort={ariaSort(systemSort, "cpu")}
                  >
                    <button
                      type="button"
                      className="monitor-sort-header"
                      onClick={() => toggleSystemSort("cpu")}
                    >
                      CPU
                      {sortIndicator(systemSort, "cpu")}
                    </button>
                  </div>
                  <div
                    className="monitor-th monitor-th-number"
                    role="columnheader"
                    aria-sort={ariaSort(systemSort, "memory")}
                  >
                    <button
                      type="button"
                      className="monitor-sort-header"
                      onClick={() => toggleSystemSort("memory")}
                    >
                      Memory
                      {sortIndicator(systemSort, "memory")}
                    </button>
                  </div>
                  <div className="monitor-th" role="columnheader">
                    Parent
                  </div>
                  <div
                    className="monitor-th monitor-th-number"
                    role="columnheader"
                  >
                    Uptime
                  </div>
                </div>

                <div role="rowgroup">
                  {systemRows.map((process) => (
                    <div key={process.pid} className="monitor-row" role="row">
                      <div
                        className="monitor-cell monitor-cell-process"
                        role="cell"
                      >
                        <div className="monitor-process">
                          <span
                            className="monitor-system-icon"
                            aria-hidden="true"
                          >
                            <Cpu size={14} />
                          </span>
                          <div className="monitor-process-info">
                            <span className="monitor-process-name">
                              {process.name}
                            </span>
                            {process.path && (
                              <span
                                className="monitor-system-path"
                                title={process.path}
                              >
                                {process.path}
                              </span>
                            )}
                          </div>
                        </div>
                      </div>
                      <div className="monitor-cell monitor-number" role="cell">
                        {process.pid}
                      </div>
                      <div className="monitor-cell monitor-number" role="cell">
                        {process.cpuPercent.toFixed(1)}%
                      </div>
                      <div className="monitor-cell monitor-number" role="cell">
                        {formatMemory(process.memoryBytes)}
                      </div>
                      <div
                        className="monitor-cell monitor-parent"
                        role="cell"
                        title={
                          process.parentName && process.parentPid !== null
                            ? `PID ${process.parentPid}`
                            : undefined
                        }
                      >
                        {process.parentName ?? "—"}
                      </div>
                      <div className="monitor-cell monitor-number" role="cell">
                        {formatRunTime(process.runTimeSeconds)}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )
      ) : (
        <>
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
                  aria-sort={ariaSort(sort, "name")}
                >
                  <button
                    type="button"
                    className="monitor-sort-header"
                    onClick={() => toggleSort("name")}
                  >
                    Process
                    {sortIndicator(sort, "name")}
                  </button>
                </div>
                <div
                  className="monitor-th monitor-th-number"
                  role="columnheader"
                >
                  PID
                </div>
                <div
                  className="monitor-th monitor-th-number"
                  role="columnheader"
                  aria-sort={ariaSort(sort, "cpu")}
                >
                  <button
                    type="button"
                    className="monitor-sort-header"
                    onClick={() => toggleSort("cpu")}
                  >
                    CPU
                    {sortIndicator(sort, "cpu")}
                  </button>
                </div>
                <div
                  className="monitor-th monitor-th-number"
                  role="columnheader"
                  aria-sort={ariaSort(sort, "memory")}
                >
                  <button
                    type="button"
                    className="monitor-sort-header"
                    onClick={() => toggleSort("memory")}
                  >
                    Memory
                    {sortIndicator(sort, "memory")}
                  </button>
                </div>
                <div
                  className="monitor-th monitor-th-number"
                  role="columnheader"
                >
                  Processes
                </div>
                <div
                  className="monitor-th monitor-th-number"
                  role="columnheader"
                >
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
                            <span
                              className={`monitor-process-status ${status}`}
                            >
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
                              active
                                ? samples.map((sample) => sample.memory)
                                : []
                            }
                            className="sparkline--memory"
                          />
                        </div>
                      </div>
                      <div className="monitor-cell monitor-number" role="cell">
                        {processes > 0 ? processes : "—"}
                      </div>
                      <div className="monitor-cell monitor-number" role="cell">
                        {state?.startedAtMs
                          ? formatUptime(state.startedAtMs)
                          : "—"}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}
