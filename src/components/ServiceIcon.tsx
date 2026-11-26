import { convertFileSrc } from "@tauri-apps/api/core";
import {
  Boxes,
  Code2,
  Database,
  Globe,
  type LucideIcon,
  Terminal,
} from "lucide-react";
import type { RuntimeState, Service } from "../types";

interface ServiceIconProps {
  service: Service;
  state?: RuntimeState;
  tone: number;
}

function iconFor(service: Service): LucideIcon {
  const identity = `${service.name} ${service.executable}`.toLowerCase();
  if (/redis|postgres|mysql|mongo|database|sqlite/.test(identity))
    return Database;
  if (/browser|web|http|vite|next|webpack|nginx/.test(identity)) return Globe;
  if (/docker|compose|container/.test(identity)) return Boxes;
  if (/code|editor|ide|studio/.test(identity)) return Code2;
  return Terminal;
}

export function ServiceIcon({ service, state, tone }: ServiceIconProps) {
  const Icon = iconFor(service);
  const status = state?.status ?? "stopped";

  return (
    <span className={`process-icon icon-tone-${tone % 5}`} aria-hidden="true">
      {service.iconPath ? (
        <img
          className="custom-process-icon"
          src={convertFileSrc(service.iconPath)}
          alt=""
        />
      ) : (
        <Icon size={25} strokeWidth={1.7} />
      )}
      <span className={`process-state-dot ${status}`} />
    </span>
  );
}
