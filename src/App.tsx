import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  FileText,
  Info,
  Pencil,
  Play,
  Plus,
  PowerIcon,
  RotateCw,
  Settings2,
  Square,
  Terminal,
  Trash2,
  X,
} from "lucide-react";
import {
  type MouseEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useState,
} from "react";
import { AboutModal } from "./components/AboutModal";
import { AppShell } from "./components/AppShell";
import {
  type ContextMenuEntry,
  ContextMenuProvider,
  useGlobalContextMenu,
} from "./components/ContextMenuProvider";
import { DeleteServiceModal } from "./components/DeleteServiceModal";
import { ServiceCard } from "./components/ServiceCard";
import { ServiceError } from "./components/ServiceError";
import { ServiceFormModal } from "./components/ServiceFormModal";
import { ServiceLogsModal } from "./components/ServiceLogsModal";
import { ServiceSearch } from "./components/ServiceSearch";
import { SettingsMenu } from "./components/SettingsMenu";
import { StartupSettingsModal } from "./components/StartupSettingsModal";
import { useServiceLogs } from "./hooks/useServiceLogs";
import { useServiceManager } from "./hooks/useServiceManager";
import { processHub } from "./lib/processhub";
import type { Service } from "./types";
import "./App.css";

function ProcessHubWorkspace() {
  const manager = useServiceManager();
  const logs = useServiceLogs();
  const { openContextMenu, closeContextMenu } = useGlobalContextMenu();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Service | null>(null);
  const [form, setForm] = useState<{ service?: Service } | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [startupSettingsOpen, setStartupSettingsOpen] = useState(false);
  const [quitConfirmationOpen, setQuitConfirmationOpen] = useState(false);
  const [lifecycleError, setLifecycleError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [logServiceId, setLogServiceId] = useState<string | null>(null);
  const refreshServices = manager.refresh;

  const runGlobalAction = useCallback(
    async (action: "start" | "stop" | "restart") => {
      closeContextMenu();
      setLifecycleError(null);
      try {
        if (action === "stop") {
          const failures = await processHub.stopAllServices();
          if (failures.length > 0) {
            setLifecycleError(
              `Some services could not be stopped: ${failures.join("; ")}`,
            );
          }
        } else {
          const report =
            action === "start"
              ? await processHub.startAllServices()
              : await processHub.restartAllServices();
          if (report.failures.length > 0) {
            setLifecycleError(
              `Started ${report.started.length} service(s); ${report.failures.length} failed: ${report.failures.join("; ")}`,
            );
          }
        }
        await refreshServices();
      } catch (cause) {
        setLifecycleError(String(cause));
      }
    },
    [closeContextMenu, refreshServices],
  );

  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setSearchOpen(true);
      }
    };
    const onNew = () => setForm({});
    const onStartup = () => setStartupSettingsOpen(true);
    const onAbout = () => setAboutOpen(true);
    const onQuit = () => setQuitConfirmationOpen(true);
    const onStartAll = () => void runGlobalAction("start");
    const onStopAll = () => void runGlobalAction("stop");
    const onRestartAll = () => void runGlobalAction("restart");

    window.addEventListener("keydown", onShortcut);
    window.addEventListener("processhub:new", onNew);
    window.addEventListener("processhub:startup-settings", onStartup);
    window.addEventListener("processhub:about", onAbout);
    window.addEventListener("processhub:quit", onQuit);
    window.addEventListener("processhub:start-all", onStartAll);
    window.addEventListener("processhub:stop-all", onStopAll);
    window.addEventListener("processhub:restart-all", onRestartAll);
    return () => {
      window.removeEventListener("keydown", onShortcut);
      window.removeEventListener("processhub:new", onNew);
      window.removeEventListener("processhub:startup-settings", onStartup);
      window.removeEventListener("processhub:about", onAbout);
      window.removeEventListener("processhub:quit", onQuit);
      window.removeEventListener("processhub:start-all", onStartAll);
      window.removeEventListener("processhub:stop-all", onStopAll);
      window.removeEventListener("processhub:restart-all", onRestartAll);
    };
  }, [runGlobalAction]);

  useEffect(() => {
    let disposed = false;
    const unlisten = Promise.all([
      listen<{ started: string[]; failures: string[] }>(
        "startup-report",
        ({ payload }) => {
          if (payload.failures.length > 0) {
            setLifecycleError(
              `Started ${payload.started.length} service(s); ${payload.failures.length} failed: ${payload.failures.join("; ")}`,
            );
          }
        },
      ),
      listen("quit-confirmation-required", () => setQuitConfirmationOpen(true)),
      listen<string>("open-service-details", ({ payload }) => {
        setSelectedId(payload);
        setLogServiceId(payload);
      }),
      listen<string>("tray-open-url", ({ payload }) => {
        void openUrl(payload).catch((cause: unknown) =>
          setLifecycleError(String(cause)),
        );
      }),
      listen<{ action: string; failures: string[] }>(
        "tray-action-report",
        ({ payload }) => {
          setLifecycleError(
            payload.failures.length
              ? `Some services could not be stopped: ${payload.failures.join("; ")}`
              : null,
          );
        },
      ),
    ]).then((stops) => {
      if (disposed) {
        stops.forEach((stop) => {
          stop();
        });
      } else {
        void processHub
          .recoverOrphanedServices()
          .then(async (report) => {
            if (report.unverifiable.length || report.failures.length) {
              setLifecycleError(
                `Leftover processes for ${[...report.unverifiable, ...report.failures].join(", ")} could not be verified, so they were left untouched. Those services will not auto-start to avoid duplicates; stop the processes manually, then start the service again.`,
              );
            }
            const enabled = await processHub.getSetting<boolean>(
              "startAutoStartServices",
            );
            if (enabled !== false) {
              await processHub.startAutoStartServices();
            }
          })
          .catch((cause: unknown) => setLifecycleError(String(cause)));
      }
      return stops;
    });
    return () => {
      disposed = true;
      void unlisten.then((stops) => {
        stops.forEach((stop) => {
          stop();
        });
      });
    };
  }, []);

  const logService = logServiceId
    ? manager.services.find((service) => service.id === logServiceId)
    : undefined;
  const query = searchQuery.trim().toLocaleLowerCase();
  const visibleServices = manager.services.filter((service) =>
    `${service.name} ${service.executable} ${service.description ?? ""}`
      .toLocaleLowerCase()
      .includes(query),
  );

  function openEditor(service?: Service) {
    closeContextMenu();
    setForm(service ? { service } : {});
  }

  async function runServiceAction(
    service: Service,
    action: "start" | "stop" | "restart",
  ) {
    closeContextMenu();
    await manager.runAction(service, action);
  }

  function buildServiceMenuItems(service: Service): ContextMenuEntry[] {
    const active = [
      "running",
      "starting",
      "stopping",
      "waiting_to_restart",
    ].includes(manager.runtime[service.id]?.status ?? "");
    return [
      { type: "heading", id: `heading-${service.id}`, label: service.name },
      {
        id: "service-toggle",
        label: active ? "Stop" : "Start",
        icon: active ? <Square size={14} /> : <Play size={14} />,
        shortcut: "↵",
        onSelect: () => runServiceAction(service, active ? "stop" : "start"),
      },
      {
        id: "service-restart",
        label: "Restart",
        icon: <RotateCw size={14} />,
        onSelect: () => runServiceAction(service, "restart"),
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

  function openServiceMenu(
    event: MouseEvent<HTMLButtonElement>,
    service: Service,
  ) {
    event.preventDefault();
    setSelectedId(service.id);
    openContextMenu(event, buildServiceMenuItems(service));
  }

  function openKeyboardMenu(
    event: ReactKeyboardEvent<HTMLButtonElement>,
    service: Service,
  ) {
    const bounds = event.currentTarget.getBoundingClientRect();
    setSelectedId(service.id);
    openContextMenu(
      { clientX: bounds.left + 18, clientY: bounds.bottom - 10 },
      buildServiceMenuItems(service),
    );
  }

  async function removeService() {
    if (!pendingDelete) return;
    const deleted = await manager.deleteService(pendingDelete);
    if (deleted) {
      setSelectedId(null);
      setPendingDelete(null);
    }
    closeContextMenu();
  }

  async function exportConfiguration() {
    setSettingsOpen(false);
    setLifecycleError(null);
    try {
      const path = await save({
        defaultPath: "processhub-config.json",
        filters: [{ name: "ProcessHub configuration", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      await processHub.exportConfigurationTo(path);
      setNotice("Configuration exported.");
    } catch (cause) {
      setLifecycleError(String(cause));
    }
  }

  async function checkForUpdates() {
    setSettingsOpen(false);
    setLifecycleError(null);
    try {
      const update = await processHub.checkForUpdate();
      setNotice(
        update
          ? `ProcessHub ${update.version} is available.`
          : "ProcessHub is up to date.",
      );
    } catch (cause) {
      setLifecycleError(String(cause));
    }
  }

  async function importConfiguration() {
    setSettingsOpen(false);
    setLifecycleError(null);
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "ProcessHub configuration", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      const report = await processHub.importConfigurationFrom(path);
      await manager.refresh();
      const details = [];
      if (report.renamedServices) {
        details.push(`${report.renamedServices} renamed to avoid duplicates`);
      }
      if (report.droppedSecrets) {
        details.push(`${report.droppedSecrets} secret(s) skipped`);
      }
      const detail = details.length > 0 ? ` ${details.join("; ")}.` : "";
      setNotice(`Imported ${report.importedServices} service(s).${detail}`);
    } catch (cause) {
      setLifecycleError(String(cause));
    }
  }

  return (
    <AppShell
      settingsOpen={settingsOpen}
      onNew={() => openEditor()}
      onToggleSettings={() => setSettingsOpen((open) => !open)}
    >
      {manager.error && (
        <ServiceError
          message={manager.error}
          onRetry={() => void manager.refresh()}
          onDismiss={manager.dismissError}
        />
      )}
      {lifecycleError && (
        <ServiceError
          message={lifecycleError}
          onDismiss={() => setLifecycleError(null)}
        />
      )}
      {notice && (
        <div className="notice-banner" role="status">
          <span>{notice}</span>
          <button
            type="button"
            aria-label="Dismiss"
            onClick={() => setNotice(null)}
          >
            <X size={13} />
          </button>
        </div>
      )}
      {searchOpen && (
        <ServiceSearch
          value={searchQuery}
          onChange={setSearchQuery}
          onClose={() => {
            setSearchOpen(false);
            setSearchQuery("");
          }}
        />
      )}

      {manager.loading ? (
        <div className="canvas-message" role="status">
          Connecting to your process manager…
        </div>
      ) : manager.services.length === 0 ? (
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
      ) : visibleServices.length === 0 ? (
        <div className="empty-canvas">
          <h1>No matching processes</h1>
          <p>Try another name, executable, or description.</p>
        </div>
      ) : (
        <div className="process-grid">
          {visibleServices.map((service, index) => (
            <ServiceCard
              key={service.id}
              service={service}
              state={manager.runtime[service.id]}
              busy={manager.busy === service.id}
              tone={index}
              selected={selectedId === service.id}
              onOpenDetails={() => {
                setSelectedId(service.id);
                setLogServiceId(service.id);
              }}
              onContextMenu={(event) => openServiceMenu(event, service)}
              onKeyboardContextMenu={(event) =>
                openKeyboardMenu(event, service)
              }
            />
          ))}
        </div>
      )}
      {settingsOpen && (
        <SettingsMenu
          onClose={() => setSettingsOpen(false)}
          onStartupSettings={() => {
            setSettingsOpen(false);
            setStartupSettingsOpen(true);
          }}
          onExport={() => void exportConfiguration()}
          onImport={() => void importConfiguration()}
          onCheckUpdates={() => void checkForUpdates()}
          onAbout={() => {
            setSettingsOpen(false);
            setAboutOpen(true);
          }}
          onQuit={() => {
            setSettingsOpen(false);
            setQuitConfirmationOpen(true);
          }}
        />
      )}
      {aboutOpen && <AboutModal onClose={() => setAboutOpen(false)} />}
      {startupSettingsOpen && (
        <StartupSettingsModal onClose={() => setStartupSettingsOpen(false)} />
      )}
      {quitConfirmationOpen && (
        <div className="dialog-scrim">
          <section
            className="quit-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="quit-title"
          >
            <h2 id="quit-title">Stop services and quit?</h2>
            <p>ProcessHub will stop all running services before it exits.</p>
            {lifecycleError && (
              <p className="form-error" role="alert">
                {lifecycleError}
              </p>
            )}
            <div className="dialog-actions">
              <button
                className="cancel-button"
                type="button"
                onClick={() => setQuitConfirmationOpen(false)}
              >
                Cancel
              </button>
              <button
                className="danger-button"
                type="button"
                onClick={() => {
                  setLifecycleError(null);
                  void processHub
                    .quitApplication()
                    .catch((cause: unknown) =>
                      setLifecycleError(String(cause)),
                    );
                }}
              >
                Stop &amp; Quit
              </button>
            </div>
          </section>
        </div>
      )}

      {logService && (
        <ServiceLogsModal
          service={logService}
          tone={Math.max(
            0,
            manager.services.findIndex((item) => item.id === logService.id),
          )}
          busy={manager.busy === logService.id}
          state={manager.runtime[logService.id]}
          events={logs.events[logService.id] ?? []}
          runHistory={logs.runHistory[logService.id] ?? []}
          droppedEvents={logs.dropped[logService.id] ?? 0}
          retentionDays={logs.retentionDays}
          onClose={() => setLogServiceId(null)}
          onStart={() => void runServiceAction(logService, "start")}
          onStop={() => void runServiceAction(logService, "stop")}
          onRestart={() => void runServiceAction(logService, "restart")}
          onEdit={() => {
            setLogServiceId(null);
            openEditor(logService);
          }}
          onDelete={() => {
            setLogServiceId(null);
            setPendingDelete(logService);
          }}
          onClear={() => logs.clear(logService.id)}
          onLoad={logs.load}
          onRetentionChange={logs.changeRetention}
        />
      )}

      {form && (
        <ServiceFormModal
          key={form.service?.id ?? "new"}
          service={form.service}
          services={manager.services}
          busy={manager.busy !== null}
          onClose={() => setForm(null)}
          onSave={manager.saveService}
        />
      )}
      {pendingDelete && (
        <DeleteServiceModal
          service={pendingDelete}
          busy={manager.busy !== null}
          onClose={() => setPendingDelete(null)}
          onConfirm={() => void removeService()}
        />
      )}
    </AppShell>
  );
}

function App() {
  const getDefaultItems = useCallback(
    (): ContextMenuEntry[] => [
      {
        id: "workspace-new",
        label: "New process",
        icon: <Plus size={14} />,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:new"));
        },
      },
      { type: "separator", id: "workspace-sep-services" },
      {
        id: "workspace-start-all",
        label: "Start all",
        icon: <Play size={14} />,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:start-all"));
        },
      },
      {
        id: "workspace-restart-all",
        label: "Restart all",
        icon: <RotateCw size={14} />,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:restart-all"));
        },
      },
      {
        id: "workspace-stop-all",
        label: "Stop all",
        icon: <Square size={14} />,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:stop-all"));
        },
      },
      { type: "separator", id: "workspace-sep-1" },
      {
        id: "workspace-startup",
        label: "Startup settings",
        icon: <Settings2 size={14} />,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:startup-settings"));
        },
      },
      {
        id: "workspace-about",
        label: "About",
        icon: <Info size={14} />,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:about"));
        },
      },
      { type: "separator", id: "workspace-sep-2" },
      {
        id: "workspace-quit",
        label: "Quit",
        icon: <PowerIcon size={14} />,
        danger: true,
        onSelect: () => {
          window.dispatchEvent(new CustomEvent("processhub:quit"));
        },
      },
    ],
    [],
  );

  return (
    <ContextMenuProvider getDefaultItems={getDefaultItems}>
      <ProcessHubWorkspace />
    </ContextMenuProvider>
  );
}

export default App;
