import { useEffect, useMemo, useRef, useState } from "react";
import type { LogEvent, RunHistory, RuntimeState, Service } from "../types";
import { Button } from "./Button";
import { Modal } from "./Modal";

interface ServiceLogsModalProps {
  service: Service;
  events: LogEvent[];
  runHistory: RunHistory[];
  droppedEvents: number;
  retentionDays: number;
  state?: RuntimeState;
  onClose: () => void;
  onClear: () => void;
  onLoad: (serviceId: string) => Promise<void>;
  onRetentionChange: (days: number) => Promise<void>;
}

function formatTimestamp(timestampMs: number) {
  return new Date(timestampMs).toLocaleTimeString([], { hour12: false });
}

function formatUptime(startedAtMs: number) {
  const totalSeconds = Math.max(
    0,
    Math.floor((Date.now() - startedAtMs) / 1000),
  );
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return hours > 0
    ? `${hours}h ${minutes}m ${seconds}s`
    : minutes > 0
      ? `${minutes}m ${seconds}s`
      : `${seconds}s`;
}

function formatMemory(bytes: number) {
  const megabytes = bytes / (1024 * 1024);
  return megabytes >= 1024
    ? `${(megabytes / 1024).toFixed(1)} GB`
    : `${megabytes.toFixed(0)} MB`;
}

export function ServiceLogsModal({
  service,
  events,
  runHistory,
  droppedEvents,
  retentionDays,
  state,
  onClose,
  onClear,
  onLoad,
  onRetentionChange,
}: ServiceLogsModalProps) {
  const [query, setQuery] = useState("");
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [pausedEvents, setPausedEvents] = useState<LogEvent[] | null>(null);
  const [copyError, setCopyError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [retentionError, setRetentionError] = useState<string | null>(null);
  const [selectedEvents, setSelectedEvents] = useState<Set<LogEvent>>(
    () => new Set(),
  );
  const streamRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    void onLoad(service.id).catch(() =>
      setLoadError("Saved log history could not be loaded."),
    );
  }, [onLoad, service.id]);
  const processMeta = state?.pid
    ? `PID ${state.pid}${state.startedAtMs ? ` · up ${formatUptime(state.startedAtMs)}` : ""}${state.cpuUsagePercent !== null && state.cpuUsagePercent !== undefined ? ` · CPU ${state.cpuUsagePercent.toFixed(1)}%` : ""}${state.memoryUsageBytes !== null && state.memoryUsageBytes !== undefined ? ` · ${formatMemory(state.memoryUsageBytes)}` : ""}`
    : `${state?.status ?? "stopped"}${state?.exitCode !== null && state?.exitCode !== undefined ? ` · exit ${state.exitCode}` : ""}`;
  const visibleEvents = paused ? (pausedEvents ?? events) : events;
  const eventKeys = useMemo(
    () =>
      new Map(
        visibleEvents.map((event, index) => [
          event,
          `${event.timestampMs}-${event.stream}-${index}`,
        ]),
      ),
    [visibleEvents],
  );
  const filteredEvents = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return visibleEvents.filter((event) =>
      `${event.stream} ${event.text}`
        .toLocaleLowerCase()
        .includes(normalizedQuery),
    );
  }, [query, visibleEvents]);

  useEffect(() => {
    if (!paused && autoScroll)
      streamRef.current?.scrollTo({ top: streamRef.current.scrollHeight });
  });

  async function copyVisible() {
    try {
      await navigator.clipboard.writeText(
        filteredEvents
          .filter(
            (event) => selectedEvents.size === 0 || selectedEvents.has(event),
          )
          .map(
            (event) =>
              `${formatTimestamp(event.timestampMs)} [${event.stream}] ${event.text}`,
          )
          .join(""),
      );
      setCopyError(null);
    } catch {
      setCopyError("Clipboard access is unavailable. Export the logs instead.");
    }
  }

  function exportVisible() {
    const contents = filteredEvents
      .map(
        (event) =>
          `${new Date(event.timestampMs).toISOString()} [${event.stream}] ${event.text}`,
      )
      .join("");
    const url = URL.createObjectURL(
      new Blob([contents], { type: "text/plain" }),
    );
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${service.name.replace(/[^a-z0-9-_]+/gi, "-")}-logs.txt`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  return (
    <Modal
      titleId="service-logs-title"
      className="log-dialog"
      onClose={onClose}
    >
      <div className="log-heading">
        <div>
          <h2 id="service-logs-title">{service.name} logs</h2>
          <p title="CPU and memory are estimates for the root process only.">
            {processMeta} · {visibleEvents.length} buffered events
          </p>
        </div>
        <Button onClick={onClose}>Done</Button>
      </div>
      <details className="run-history">
        <summary>Recent runs · {runHistory.length}</summary>
        {runHistory.length === 0 ? (
          <p>No completed runs yet.</p>
        ) : (
          <ul>
            {runHistory.map((run) => (
              <li key={run.id}>
                <time dateTime={new Date(run.startedAtMs).toISOString()}>
                  {new Date(run.startedAtMs).toLocaleString()}
                </time>
                <span>{run.status}</span>
                <span>
                  {Math.max(
                    0,
                    Math.round((run.endedAtMs - run.startedAtMs) / 1000),
                  )}
                  s
                </span>
                {run.exitCode !== null && <span>exit {run.exitCode}</span>}
              </li>
            ))}
          </ul>
        )}
      </details>
      <div className="log-toolbar">
        <input
          type="search"
          aria-label="Search loaded logs"
          placeholder="Search loaded logs…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <Button
          onClick={() => {
            if (!paused) setPausedEvents(events);
            else setPausedEvents(null);
            setPaused((current) => !current);
          }}
        >
          {paused ? "Resume" : "Pause"}
        </Button>
        <Button onClick={() => setAutoScroll((current) => !current)}>
          Follow: {autoScroll ? "On" : "Off"}
        </Button>
        <Button onClick={() => void copyVisible()}>Copy</Button>
        <Button onClick={exportVisible}>Export</Button>
        <label className="log-retention">
          Keep
          <select
            aria-label="Log retention period"
            title="Expired files are removed the next time this service starts."
            value={retentionDays}
            onChange={(event) => {
              setRetentionError(null);
              void onRetentionChange(Number(event.target.value)).catch(() =>
                setRetentionError("Could not save the log retention setting."),
              );
            }}
          >
            {[7, 14, 30, 90].map((days) => (
              <option key={days} value={days}>
                {days} days
              </option>
            ))}
          </select>
        </label>
        <Button
          onClick={() => {
            setSelectedEvents(new Set());
            onClear();
          }}
        >
          Clear view
        </Button>
      </div>
      {copyError && (
        <p className="log-copy-error" role="alert">
          {copyError}
        </p>
      )}
      {loadError && (
        <p className="log-copy-error" role="alert">
          {loadError}
        </p>
      )}
      {retentionError && (
        <p className="log-copy-error" role="alert">
          {retentionError}
        </p>
      )}
      {droppedEvents > 0 && (
        <p className="log-copy-error" role="status">
          {droppedEvents} live events were skipped while the viewer was busy.
          Saved logs remain available.
        </p>
      )}
      <div
        className="log-stream"
        ref={streamRef}
        role="log"
        aria-live={paused ? "off" : "polite"}
      >
        {filteredEvents.length === 0 ? (
          <div className="log-empty">
            {events.length === 0
              ? "Output will appear here when this process runs."
              : "No loaded lines match this search."}
          </div>
        ) : (
          filteredEvents.map((event, index) => (
            <div
              className={`log-line ${event.stream}`}
              key={eventKeys.get(event)}
            >
              <input
                type="checkbox"
                aria-label={`Select ${event.stream} log line ${index + 1}`}
                checked={selectedEvents.has(event)}
                onChange={(inputEvent) =>
                  setSelectedEvents((current) => {
                    const next = new Set(current);
                    if (inputEvent.target.checked) next.add(event);
                    else next.delete(event);
                    return next;
                  })
                }
              />
              <time>{formatTimestamp(event.timestampMs)}</time>
              <span className="log-stream-label">{event.stream}</span>
              <pre>{event.text}</pre>
            </div>
          ))
        )}
      </div>
    </Modal>
  );
}
