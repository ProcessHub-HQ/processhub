import {
  FileText,
  Pencil,
  Play,
  RotateCw,
  Square,
  Terminal,
  Trash2,
} from "lucide-react";
import type { MouseEvent, KeyboardEvent as ReactKeyboardEvent } from "react";
import type { ContextMenuEntry } from "../components/ContextMenuProvider";
import { useGlobalContextMenu } from "../components/ContextMenuProvider";
import { ServiceCard } from "../components/ServiceCard";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";
import type { Service } from "../types";

export function WorkspaceContent() {
  const services = useServicesStore((state) => state.services);
  const runtime = useServicesStore((state) => state.runtime);
  const loading = useServicesStore((state) => state.loading);
  const busy = useServicesStore((state) => state.busy);
  const runAction = useServicesStore((state) => state.runAction);
  const selectedId = useUiStore((state) => state.selectedId);
  const setSelectedId = useUiStore((state) => state.setSelectedId);
  const setLogServiceId = useUiStore((state) => state.setLogServiceId);
  const openEditor = useUiStore((state) => state.openEditor);
  const setPendingDelete = useUiStore((state) => state.setPendingDelete);
  const query = useUiStore((state) => state.searchQuery);
  const { openContextMenu, closeContextMenu } = useGlobalContextMenu();

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const visibleServices = services.filter((service) =>
    `${service.name} ${service.executable} ${service.description ?? ""}`
      .toLocaleLowerCase()
      .includes(normalizedQuery),
  );

  function isActive(service: Service) {
    const status = runtime[service.id]?.status ?? "";
    return ["running", "starting", "stopping", "waiting_to_restart"].includes(
      status,
    );
  }

  function buildServiceMenuItems(service: Service): ContextMenuEntry[] {
    const active = isActive(service);
    return [
      { type: "heading", id: `heading-${service.id}`, label: service.name },
      {
        id: "service-toggle",
        label: active ? "Stop" : "Start",
        icon: active ? <Square size={14} /> : <Play size={14} />,
        shortcut: "↵",
        onSelect: () => runAction(service, active ? "stop" : "start"),
      },
      {
        id: "service-restart",
        label: "Restart",
        icon: <RotateCw size={14} />,
        onSelect: () => runAction(service, "restart"),
      },
      {
        id: "service-details",
        label: "View details",
        icon: <FileText size={14} />,
        onSelect: () => setLogServiceId(service.id),
      },
      { type: "separator", id: "service-sep-1" },
      {
        id: "service-edit",
        label: "Edit details",
        icon: <Pencil size={14} />,
        onSelect: () => openEditor(service),
      },
      { type: "separator", id: "service-sep-2" },
      {
        id: "service-delete",
        label: "Delete",
        icon: <Trash2 size={14} />,
        danger: true,
        onSelect: () => setPendingDelete(service),
      },
    ];
  }

  function openServiceMenuAt(x: number, y: number, service: Service) {
    setSelectedId(service.id);
    openContextMenu({ clientX: x, clientY: y }, buildServiceMenuItems(service));
  }

  function openServiceMenu(event: MouseEvent<HTMLElement>, service: Service) {
    event.preventDefault();
    openServiceMenuAt(event.clientX, event.clientY, service);
  }

  function openKeyboardMenu(
    event: ReactKeyboardEvent<HTMLElement>,
    service: Service,
  ) {
    const bounds = event.currentTarget.getBoundingClientRect();
    openServiceMenuAt(bounds.left + 18, bounds.bottom - 10, service);
  }

  if (loading) {
    return (
      <div className="canvas-message" role="status">
        Connecting to your process manager…
      </div>
    );
  }

  if (services.length === 0) {
    return (
      <div className="empty-canvas">
        <div className="empty-icon">
          <Terminal size={22} strokeWidth={1.5} />
        </div>
        <h1>Your process space is ready</h1>
        <p>
          Choose New to add a local executable, its arguments, and an optional
          working directory.
        </p>
      </div>
    );
  }

  if (visibleServices.length === 0) {
    return (
      <div className="empty-canvas">
        <h1>No matching processes</h1>
        <p>Try another name, executable, or description.</p>
      </div>
    );
  }

  return (
    <div className="process-grid">
      {visibleServices.map((service, index) => (
        <ServiceCard
          key={service.id}
          service={service}
          state={runtime[service.id]}
          busy={busy === service.id}
          tone={index}
          selected={selectedId === service.id}
          onOpenDetails={() => {
            closeContextMenu();
            setSelectedId(service.id);
            setLogServiceId(service.id);
          }}
          onToggle={() => {
            setSelectedId(service.id);
            void runAction(service, isActive(service) ? "stop" : "start");
          }}
          onOpenMenu={(x, y) => openServiceMenuAt(x, y, service)}
          onContextMenu={(event) => openServiceMenu(event, service)}
          onKeyboardContextMenu={(event) => openKeyboardMenu(event, service)}
        />
      ))}
    </div>
  );
}
