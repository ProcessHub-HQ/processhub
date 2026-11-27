import { Button, Modal } from "@processhub/design-system";
import { AboutModal } from "../components/AboutModal";
import { DeleteServiceModal } from "../components/DeleteServiceModal";
import { ServiceFormModal } from "../components/ServiceFormModal";
import { ServiceLogsModal } from "../components/ServiceLogsModal";
import { StartupSettingsModal } from "../components/StartupSettingsModal";
import { UpdateDialog } from "../components/UpdateDialog";
import { processHub } from "../lib/processhub";
import { useLogsStore } from "../stores/logsStore";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";
import { useAppCommands } from "./useAppCommands";

export function AppDialogs() {
  const services = useServicesStore((state) => state.services);
  const runtime = useServicesStore((state) => state.runtime);
  const busy = useServicesStore((state) => state.busy);
  const saveService = useServicesStore((state) => state.saveService);
  const aboutOpen = useUiStore((state) => state.aboutOpen);
  const startupSettingsOpen = useUiStore((state) => state.startupSettingsOpen);
  const quitConfirmationOpen = useUiStore(
    (state) => state.quitConfirmationOpen,
  );
  const lifecycleError = useUiStore((state) => state.lifecycleError);
  const setAboutOpen = useUiStore((state) => state.setAboutOpen);
  const setStartupSettingsOpen = useUiStore(
    (state) => state.setStartupSettingsOpen,
  );
  const setQuitConfirmationOpen = useUiStore(
    (state) => state.setQuitConfirmationOpen,
  );
  const setLifecycleError = useUiStore((state) => state.setLifecycleError);
  const form = useUiStore((state) => state.form);
  const closeEditor = useUiStore((state) => state.closeEditor);
  const pendingDelete = useUiStore((state) => state.pendingDelete);
  const setPendingDelete = useUiStore((state) => state.setPendingDelete);
  const logServiceId = useUiStore((state) => state.logServiceId);
  const setLogServiceId = useUiStore((state) => state.setLogServiceId);
  const openEditor = useUiStore((state) => state.openEditor);
  const events = useLogsStore((state) => state.events);
  const runHistory = useLogsStore((state) => state.runHistory);
  const dropped = useLogsStore((state) => state.dropped);
  const retentionDays = useLogsStore((state) => state.retentionDays);
  const load = useLogsStore((state) => state.load);
  const clear = useLogsStore((state) => state.clear);
  const changeRetention = useLogsStore((state) => state.changeRetention);
  const runAction = useServicesStore((state) => state.runAction);
  const { removeService } = useAppCommands();

  const logService = logServiceId
    ? services.find((service) => service.id === logServiceId)
    : undefined;

  return (
    <>
      <UpdateDialog />
      {aboutOpen && <AboutModal onClose={() => setAboutOpen(false)} />}
      {startupSettingsOpen && (
        <StartupSettingsModal onClose={() => setStartupSettingsOpen(false)} />
      )}
      {quitConfirmationOpen && (
        <Modal
          titleId="quit-title"
          className="quit-dialog"
          onClose={() => setQuitConfirmationOpen(false)}
        >
          <h2 id="quit-title">Stop services and quit?</h2>
          <p>ProcessHub will stop all running services before it exits.</p>
          {lifecycleError && (
            <p className="form-error" role="alert">
              {lifecycleError}
            </p>
          )}
          <div className="dialog-actions">
            <Button size="xs" onClick={() => setQuitConfirmationOpen(false)}>
              Cancel
            </Button>
            <Button
              size="xs"
              variant="danger"
              onClick={() => {
                setLifecycleError(null);
                void processHub
                  .quitApplication()
                  .catch((cause: unknown) => setLifecycleError(String(cause)));
              }}
            >
              Stop &amp; Quit
            </Button>
          </div>
        </Modal>
      )}
      {logService && (
        <ServiceLogsModal
          service={logService}
          tone={Math.max(
            0,
            services.findIndex((item) => item.id === logService.id),
          )}
          busy={busy === logService.id}
          state={runtime[logService.id]}
          events={events[logService.id] ?? []}
          runHistory={runHistory[logService.id] ?? []}
          droppedEvents={dropped[logService.id] ?? 0}
          retentionDays={retentionDays}
          onClose={() => setLogServiceId(null)}
          onStart={() => void runAction(logService, "start")}
          onStop={() => void runAction(logService, "stop")}
          onRestart={() => void runAction(logService, "restart")}
          onEdit={() => openEditor(logService)}
          onDelete={() => {
            setLogServiceId(null);
            setPendingDelete(logService);
          }}
          onClear={() => clear(logService.id)}
          onLoad={load}
          onRetentionChange={changeRetention}
        />
      )}
      {form && (
        <ServiceFormModal
          key={form.service?.id ?? "new"}
          service={form.service}
          services={services}
          busy={busy !== null}
          onClose={closeEditor}
          onSave={saveService}
        />
      )}
      {pendingDelete && (
        <DeleteServiceModal
          service={pendingDelete}
          busy={busy !== null}
          onClose={() => setPendingDelete(null)}
          onConfirm={() => void removeService()}
        />
      )}
    </>
  );
}
