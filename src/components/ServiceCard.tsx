import { IconButton } from "@processhub/design-system";
import { LoaderCircle, MoreHorizontal, Play, Square } from "lucide-react";
import type { KeyboardEvent, MouseEvent } from "react";
import type { RuntimeState, RuntimeStatus, Service } from "../types";
import { Card } from "./Card";
import { ServiceIcon } from "./ServiceIcon";

interface ServiceCardProps {
  service: Service;
  state?: RuntimeState;
  busy: boolean;
  tone: number;
  selected: boolean;
  onOpenDetails: () => void;
  onToggle: () => void;
  onOpenMenu: (x: number, y: number) => void;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void;
  onKeyboardContextMenu: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

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

function describeProcess(state?: RuntimeState) {
  if (!state) return "No process";
  if (state.pid) {
    return state.startedAtMs
      ? `PID ${state.pid} · ${formatUptime(state.startedAtMs)}`
      : `PID ${state.pid}`;
  }
  if (state.status === "waiting_to_restart" && state.restartAtMs) {
    const seconds = Math.max(
      0,
      Math.ceil((state.restartAtMs - Date.now()) / 1000),
    );
    return `retry in ${seconds}s`;
  }
  if (state.exitCode !== null && state.exitCode !== undefined) {
    return `exit ${state.exitCode}`;
  }
  return "No process";
}

export function ServiceCard({
  service,
  state,
  busy,
  tone,
  selected,
  onOpenDetails,
  onToggle,
  onOpenMenu,
  onContextMenu,
  onKeyboardContextMenu,
}: ServiceCardProps) {
  const status = state?.status ?? "stopped";
  const active = activeStatuses.has(status);
  const statusLabel = busy ? "Working…" : statusLabels[status];
  const meta = describeProcess(state);

  function handleKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (
      event.key === "ContextMenu" ||
      (event.shiftKey && event.key === "F10")
    ) {
      event.preventDefault();
      onKeyboardContextMenu(event);
    } else if (event.key === "Enter") {
      event.preventDefault();
      onOpenDetails();
    }
  }

  return (
    <div className="process-card">
      <Card
        selected={selected}
        aria-label={`${service.name}, ${statusLabel}, ${meta}. Open details.`}
        onClick={onOpenDetails}
        onContextMenu={onContextMenu}
        onKeyDown={handleKeyDown}
      >
        <ServiceIcon service={service} state={state} tone={tone} />
        <span className="process-name">{service.name}</span>
        <span className={`process-state ${status}`}>{statusLabel}</span>
        <span className="process-meta">{meta}</span>
      </Card>
      <div className="process-card-actions">
        <IconButton
          size="xs"
          className="process-card-action process-card-action--primary"
          label={active ? `Stop ${service.name}` : `Start ${service.name}`}
          title={active ? "Stop" : "Start"}
          disabled={busy}
          onClick={onToggle}
        >
          {busy ? (
            <LoaderCircle className="is-spinning" size={13} />
          ) : active ? (
            <Square size={12} strokeWidth={2.2} />
          ) : (
            <Play size={12} strokeWidth={2.2} />
          )}
        </IconButton>
        <IconButton
          size="xs"
          className="process-card-action process-card-menu"
          label={`More actions for ${service.name}`}
          title="More actions"
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            onOpenMenu(rect.left, rect.bottom + 4);
          }}
        >
          <MoreHorizontal size={14} />
        </IconButton>
      </div>
    </div>
  );
}
