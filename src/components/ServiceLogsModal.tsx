import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Copy,
  ExternalLink,
  Pencil,
  Play,
  RotateCw,
  Square,
  TextSelect,
  Trash2,
  X,
} from "lucide-react";
import {
  type MouseEvent,
  memo,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { parseAnsi } from "../lib/ansi";
import type { LogEvent, RunHistory, RuntimeState, Service } from "../types";
import { Button } from "./Button";
import { useGlobalContextMenu } from "./ContextMenuProvider";
import { Modal } from "./Modal";
import { ServiceIcon } from "./ServiceIcon";

interface ServiceLogsModalProps {
  service: Service;
  tone: number;
  busy: boolean;
  events: LogEvent[];
  runHistory: RunHistory[];
  droppedEvents: number;
  retentionDays: number;
  state?: RuntimeState;
  onClose: () => void;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onEdit: () => void;
  onDelete: () => void;
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

const eventKeys = new WeakMap<LogEvent, number>();
let nextEventKey = 1;

function eventKey(event: LogEvent): number {
  const existing = eventKeys.get(event);
  if (existing !== undefined) return existing;
  const key = nextEventKey;
  nextEventKey += 1;
  eventKeys.set(event, key);
  return key;
}

const LogLine = memo(function LogLine({ event }: { event: LogEvent }) {
  const segments = useMemo(() => parseAnsi(event.text), [event.text]);
  return (
    <div className={`log-line ${event.stream}`}>
      <time>{formatTimestamp(event.timestampMs)}</time>
      <span className="log-stream-label">{event.stream}</span>
      <pre>
        {segments.map((segment) => (
          <span key={segment.offset} style={segment.style}>
            {segment.text}
          </span>
        ))}
      </pre>
    </div>
  );
});

export function ServiceLogsModal({
  service,
  tone,
  busy,
  events,
  runHistory,
  droppedEvents,
  retentionDays,
  state,
  onClose,
  onStart,
  onStop,
  onRestart,
  onEdit,
  onDelete,
  onClear,
  onLoad,
  onRetentionChange,
}: ServiceLogsModalProps) {
  const [query, setQuery] = useState("");
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [pausedEvents, setPausedEvents] = useState<LogEvent[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [retentionError, setRetentionError] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  const streamRef = useRef<HTMLDivElement>(null);
  const { openContextMenu } = useGlobalContextMenu();

  useEffect(() => {
    void onLoad(service.id).catch(() =>
      setLoadError("Saved log history could not be loaded."),
    );
  }, [onLoad, service.id]);

  const status = state?.status ?? "stopped";
  const active = [
    "running",
    "starting",
    "stopping",
    "waiting_to_restart",
  ].includes(status);
  const waitingToRestart = status === "waiting_to_restart";
  const [, setClockTick] = useState(0);

  useEffect(() => {
    if (!waitingToRestart) return;
    const timer = window.setInterval(
      () => setClockTick((tick) => tick + 1),
      1000,
    );
    return () => window.clearInterval(timer);
  }, [waitingToRestart]);

  const processMeta = waitingToRestart
    ? `Restarting in ${Math.max(
        0,
        Math.ceil(((state?.restartAtMs ?? Date.now()) - Date.now()) / 1000),
      )}s · attempt ${state?.restartAttempt ?? 1}`
    : state?.pid
      ? `PID ${state.pid}${state.startedAtMs ? ` · up ${formatUptime(state.startedAtMs)}` : ""}${state.cpuUsagePercent !== null && state.cpuUsagePercent !== undefined ? ` · CPU ${state.cpuUsagePercent.toFixed(1)}%` : ""}${state.memoryUsageBytes !== null && state.memoryUsageBytes !== undefined ? ` · ${formatMemory(state.memoryUsageBytes)}` : ""}`
      : `${status.replace(/_/g, " ")}${state?.exitCode !== null && state?.exitCode !== undefined ? ` · exit ${state.exitCode}` : ""}`;

  const visibleEvents = paused ? (pausedEvents ?? events) : events;
  const filteredEvents = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return visibleEvents.filter((event) =>
      `${event.stream} ${event.text}`
        .toLocaleLowerCase()
        .includes(normalizedQuery),
    );
  }, [query, visibleEvents]);

  const eventCount = filteredEvents.length;

  useEffect(() => {
    if (!paused && autoScroll && eventCount > 0) {
      streamRef.current?.scrollTo({ top: streamRef.current.scrollHeight });
    }
  }, [paused, autoScroll, eventCount]);

  function openDetectedUrl() {
    const url = state?.detectedUrl;
    if (!url) return;
    setOpenError(null);
    void openUrl(url).catch(() =>
      setOpenError("Could not open the detected URL."),
    );
  }

  function handleStreamContextMenu(event: MouseEvent<HTMLDivElement>) {
    event.preventDefault();
    const selectedText = window.getSelection()?.toString() ?? "";
    const hasLogs = filteredEvents.length > 0;
    openContextMenu(event, [
      {
        id: "logs-copy",
        label: "Copy",
        icon: <Copy size={14} />,
        shortcut: "⌘C",
        disabled: !selectedText,
        onSelect: async () => {
          if (selectedText) {
            await navigator.clipboard.writeText(selectedText);
          }
        },
      },
      {
        id: "logs-select-all",
        label: "Select all",
        icon: <TextSelect size={14} />,
        shortcut: "⌘A",
        disabled: !hasLogs,
        onSelect: () => {
          if (!streamRef.current) return;
          const selection = window.getSelection();
          if (!selection) return;
          const range = document.createRange();
          range.selectNodeContents(streamRef.current);
          selection.removeAllRanges();
          selection.addRange(range);
        },
      },
    ]);
  }

  return (
    <Modal
      titleId="service-details-title"
      className="log-dialog details-dialog"
      onClose={onClose}
    >
      <div className="details-header">
        <div className="details-title-group">
          <ServiceIcon service={service} state={state} tone={tone} />
          <div className="details-title-meta">
            <div className="details-title-row">
              <h2 id="service-details-title">{service.name}</h2>
              <span className={`status-pill ${status}`}>
                {busy ? "Working…" : status.replace(/_/g, " ")}
              </span>
              {state && state.health !== "not_configured" && (
                <span
                  className={`health-pill ${state.health}`}
                  title={state.healthError ?? "Health check"}
                >
                  {state.health}
                </span>
              )}
            </div>
            {service.description && (
              <p className="details-description">{service.description}</p>
            )}
            <p
              className="details-runtime-meta"
              title="CPU and memory are estimates for the root process only."
            >
              {processMeta} · {visibleEvents.length} buffered events · max{" "}
              {service.logMaxSizeMb} MB/file
            </p>
            {state?.healthError && (
              <p className="details-health-error" role="status">
                {state.healthError}
              </p>
            )}
          </div>
        </div>
        <Button
          variant="icon"
          aria-label="Close details"
          icon={<X size={17} />}
          onClick={onClose}
        />
      </div>

      <div className="details-actions">
        <Button
          variant={active ? "secondary" : "primary"}
          disabled={busy}
          icon={active ? <Square size={13} /> : <Play size={13} />}
          onClick={active ? onStop : onStart}
        >
          {active ? "Stop" : "Start"}
        </Button>
        <Button
          disabled={busy}
          icon={<RotateCw size={13} />}
          onClick={onRestart}
        >
          Restart
        </Button>
        {state?.detectedUrl && (
          <Button icon={<ExternalLink size={13} />} onClick={openDetectedUrl}>
            Open
          </Button>
        )}
        <span className="details-actions-divider" aria-hidden="true" />
        <Button icon={<Pencil size={13} />} onClick={onEdit}>
          Edit
        </Button>
        <Button
          className="details-delete-button"
          icon={<Trash2 size={13} />}
          onClick={onDelete}
        >
          Delete
        </Button>
      </div>

      {openError && (
        <p className="log-copy-error" role="alert">
          {openError}
        </p>
      )}

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
        <Button onClick={onClear}>Clear view</Button>
      </div>
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
        aria-live="off"
        onContextMenu={handleStreamContextMenu}
      >
        {filteredEvents.length === 0 ? (
          <div className="log-empty">
            {events.length === 0
              ? "Output will appear here when this process runs."
              : "No loaded lines match this search."}
          </div>
        ) : (
          filteredEvents.map((event) => (
            <LogLine key={eventKey(event)} event={event} />
          ))
        )}
      </div>
    </Modal>
  );
}
