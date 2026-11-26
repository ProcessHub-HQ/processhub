import { listen } from "@tauri-apps/api/event";
import { Terminal } from "lucide-react";
import {
  type MouseEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  useEffect,
  useState,
} from "react";
import { AboutModal } from "./components/AboutModal";
import { AppShell } from "./components/AppShell";
import { DeleteServiceModal } from "./components/DeleteServiceModal";
import { ServiceCard } from "./components/ServiceCard";
import { ServiceContextMenu } from "./components/ServiceContextMenu";
import { ServiceError } from "./components/ServiceError";
import { ServiceFormModal } from "./components/ServiceFormModal";
import { ServiceLogsModal } from "./components/ServiceLogsModal";
import { ServiceSearch } from "./components/ServiceSearch";
import { SettingsMenu } from "./components/SettingsMenu";
import { StartupSettingsModal } from "./components/StartupSettingsModal";
import { useContextMenu } from "./hooks/useContextMenu";
import { useServiceLogs } from "./hooks/useServiceLogs";
import { useServiceManager } from "./hooks/useServiceManager";
import { processHub } from "./lib/processhub";
import type { Service } from "./types";
import "./App.css";

function App() {
  const manager = useServiceManager();
  const logs = useServiceLogs();
  const contextMenu = useContextMenu();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<Service | null>(null);
  const [form, setForm] = useState<{ service?: Service } | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [aboutOpen, setAboutOpen] = useState(false);
  const [startupSettingsOpen, setStartupSettingsOpen] = useState(false);
  const [quitConfirmationOpen, setQuitConfirmationOpen] = useState(false);
  const [lifecycleError, setLifecycleError] = useState<string | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [logServiceId, setLogServiceId] = useState<string | null>(null);
  useEffect(() => {
    const onShortcut = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener("keydown", onShortcut);
    return () => window.removeEventListener("keydown", onShortcut);
  }, []);
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
  const menuService = contextMenu.menu
    ? manager.services.find(
        (service) => service.id === contextMenu.menu?.serviceId,
      )
    : undefined;
  const menuActive = menuService
    ? ["running", "starting", "stopping"].includes(
        manager.runtime[menuService.id]?.status ?? "",
      )
    : false;
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
    contextMenu.close();
    setForm(service ? { service } : {});
  }

  function openServiceMenu(
    event: MouseEvent<HTMLButtonElement>,
    service: Service,
  ) {
    setSelectedId(service.id);
    contextMenu.open(event, service.id);
  }

  function openKeyboardMenu(
    event: ReactKeyboardEvent<HTMLButtonElement>,
    service: Service,
  ) {
    const bounds = event.currentTarget.getBoundingClientRect();
    contextMenu.open(
      {
        preventDefault: () => {},
        clientX: bounds.left + 18,
        clientY: bounds.bottom - 10,
      },
      service.id,
    );
    setSelectedId(service.id);
  }

  async function runServiceAction(
    service: Service,
    action: "start" | "stop" | "restart",
  ) {
    contextMenu.close();
    await manager.runAction(service, action);
  }

  async function removeService() {
    if (!pendingDelete) return;
    const deleted = await manager.deleteService(pendingDelete);
    if (deleted) {
      setSelectedId(null);
      setPendingDelete(null);
    }
    contextMenu.close();
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
          {visibleServices.map((service, index) => {
            const status = manager.runtime[service.id]?.status ?? "stopped";
            return (
              <ServiceCard
                key={service.id}
                service={service}
                state={manager.runtime[service.id]}
                busy={manager.busy === service.id}
                tone={index}
                selected={selectedId === service.id}
                onSelect={() => setSelectedId(service.id)}
                onAction={() =>
                  void runServiceAction(
                    service,
                    status === "running" ? "stop" : "start",
                  )
                }
                onContextMenu={(event) => openServiceMenu(event, service)}
                onKeyboardContextMenu={(event) =>
                  openKeyboardMenu(event, service)
                }
              />
            );
          })}
        </div>
      )}
      {settingsOpen && (
        <SettingsMenu
          onStartupSettings={() => {
            setSettingsOpen(false);
            setStartupSettingsOpen(true);
          }}
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

      {contextMenu.menu && menuService && (
        <ServiceContextMenu
          service={menuService}
          active={menuActive}
          position={contextMenu.menu}
          onStart={() => void runServiceAction(menuService, "start")}
          onStop={() => void runServiceAction(menuService, "stop")}
          onRestart={() => void runServiceAction(menuService, "restart")}
          onEdit={() => openEditor(menuService)}
          onDuplicate={() => {
            contextMenu.close();
            void manager.duplicateService(menuService);
          }}
          onDelete={() => {
            contextMenu.close();
            setPendingDelete(menuService);
          }}
          onMoveFirst={() => {
            contextMenu.close();
            void manager.moveService(menuService, "first");
          }}
          onMoveLast={() => {
            contextMenu.close();
            void manager.moveService(menuService, "last");
          }}
          onViewLogs={() => {
            contextMenu.close();
            setLogServiceId(menuService.id);
          }}
        />
      )}

      {logService && (
        <ServiceLogsModal
          service={logService}
          state={manager.runtime[logService.id]}
          events={logs.events[logService.id] ?? []}
          runHistory={logs.runHistory[logService.id] ?? []}
          droppedEvents={logs.dropped[logService.id] ?? 0}
          retentionDays={logs.retentionDays}
          onClose={() => setLogServiceId(null)}
          onClear={() => logs.clear(logService.id)}
          onLoad={logs.load}
          onRetentionChange={logs.changeRetention}
        />
      )}

      {form && (
        <ServiceFormModal
          key={form.service?.id ?? "new"}
          service={form.service}
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

export default App;
