import { MoreHorizontal } from "lucide-react";
import type { KeyboardEvent, MouseEvent } from "react";
import type { RuntimeState, Service } from "../types";
import { Card } from "./Card";
import { ServiceIcon } from "./ServiceIcon";

interface ServiceCardProps {
  service: Service;
  state?: RuntimeState;
  busy: boolean;
  tone: number;
  selected: boolean;
  onOpenDetails: () => void;
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void;
  onKeyboardContextMenu: (event: KeyboardEvent<HTMLButtonElement>) => void;
}

export function ServiceCard({
  service,
  state,
  busy,
  tone,
  selected,
  onOpenDetails,
  onContextMenu,
  onKeyboardContextMenu,
}: ServiceCardProps) {
  const status = state?.status ?? "stopped";

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
    <Card
      selected={selected}
      aria-label={`${service.name}, ${status.replace(/_/g, " ")}. Click for details or right-click for actions.`}
      onClick={onOpenDetails}
      onContextMenu={onContextMenu}
      onKeyDown={handleKeyDown}
    >
      <ServiceIcon service={service} state={state} tone={tone} />
      <span className="process-name">{service.name}</span>
      <span className={`process-state ${status}`}>
        {busy ? "Working…" : status.replace(/_/g, " ")}
      </span>
      {state?.pid && <span className="process-pid">PID {state.pid}</span>}
      <MoreHorizontal className="context-hint" aria-hidden="true" size={15} />
    </Card>
  );
}
