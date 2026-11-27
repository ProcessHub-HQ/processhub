import { X } from "lucide-react";
import { useEffect } from "react";
import { AppShell } from "../components/AppShell";
import { ServiceError } from "../components/ServiceError";
import { ServiceSearch } from "../components/ServiceSearch";
import { useServicesStore } from "../stores/servicesStore";
import { useUiStore } from "../stores/uiStore";
import { AppDialogs } from "./AppDialogs";
import { useAppCommands } from "./useAppCommands";
import { useAppShortcuts } from "./useAppShortcuts";
import { useProcessHubEvents } from "./useProcessHubEvents";
import { WorkspaceContent } from "./WorkspaceContent";

export function AppWorkspace() {
  useProcessHubEvents();
  const commands = useAppCommands();
  useAppShortcuts(commands.runGlobalAction);

  const error = useServicesStore((state) => state.error);
  const refresh = useServicesStore((state) => state.refresh);
  const dismissError = useServicesStore((state) => state.dismissError);
  const lifecycleError = useUiStore((state) => state.lifecycleError);
  const notice = useUiStore((state) => state.notice);
  const setNotice = useUiStore((state) => state.setNotice);
  const setLifecycleError = useUiStore((state) => state.setLifecycleError);
  const searchOpen = useUiStore((state) => state.searchOpen);
  const searchQuery = useUiStore((state) => state.searchQuery);
  const setSearchOpen = useUiStore((state) => state.setSearchOpen);
  const setSearchQuery = useUiStore((state) => state.setSearchQuery);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 4000);
    return () => window.clearTimeout(timeout);
  }, [notice, setNotice]);

  const visibleError = error ?? lifecycleError;
  const dismissVisibleError = error
    ? dismissError
    : () => setLifecycleError(null);

  return (
    <AppShell>
      {visibleError && (
        <ServiceError
          message={visibleError}
          onRetry={error ? () => void refresh() : undefined}
          onDismiss={dismissVisibleError}
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
      <WorkspaceContent />
      <AppDialogs />
    </AppShell>
  );
}
