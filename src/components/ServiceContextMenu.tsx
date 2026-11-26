import {
  ArrowDownToLine,
  ArrowUpToLine,
  Copy,
  FileText,
  Pencil,
  Play,
  RotateCw,
  Square,
  Trash2,
} from "lucide-react";
import type { Service } from "../types";

interface ServiceContextMenuProps {
  service: Service;
  active: boolean;
  position: { x: number; y: number };
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onMoveFirst: () => void;
  onMoveLast: () => void;
  onViewLogs: () => void;
}

export function ServiceContextMenu({
  service,
  active,
  position,
  onStart,
  onStop,
  onRestart,
  onEdit,
  onDuplicate,
  onDelete,
  onMoveFirst,
  onMoveLast,
  onViewLogs,
}: ServiceContextMenuProps) {
  return (
    <div
      className="context-menu"
      data-context-menu
      role="menu"
      style={{ left: position.x, top: position.y }}
    >
      <div className="context-heading">{service.name}</div>
      <button type="button" role="menuitem" onClick={active ? onStop : onStart}>
        {active ? <Square size={14} /> : <Play size={14} />}
        <span>{active ? "Stop" : "Start"}</span>
        <span className="menu-shortcut">↵</span>
      </button>
      <button type="button" role="menuitem" onClick={onRestart}>
        <RotateCw size={14} />
        <span>Restart</span>
      </button>
      <button type="button" role="menuitem" onClick={onViewLogs}>
        <FileText size={14} />
        <span>View logs</span>
      </button>
      <div className="menu-rule" />
      <button type="button" role="menuitem" onClick={onEdit}>
        <Pencil size={14} />
        <span>Edit details</span>
      </button>
      <button type="button" role="menuitem" onClick={onDuplicate}>
        <Copy size={14} />
        <span>Duplicate</span>
      </button>
      <button type="button" role="menuitem" onClick={onMoveFirst}>
        <ArrowUpToLine size={14} />
        <span>Move to first</span>
      </button>
      <button type="button" role="menuitem" onClick={onMoveLast}>
        <ArrowDownToLine size={14} />
        <span>Move to last</span>
      </button>
      <div className="menu-rule" />
      <button
        className="danger-item"
        type="button"
        role="menuitem"
        onClick={onDelete}
      >
        <Trash2 size={14} />
        <span>Delete</span>
      </button>
    </div>
  );
}
