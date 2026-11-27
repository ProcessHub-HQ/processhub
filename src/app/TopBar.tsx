import { Plus } from "lucide-react";
import { Button } from "../components/Button";
import { SettingsMenu } from "../components/SettingsMenu";
import { useUiStore } from "../stores/uiStore";
import { useAppCommands } from "./useAppCommands";

export function TopBar() {
  const settingsOpen = useUiStore((state) => state.settingsOpen);
  const checkingForUpdates = useUiStore((state) => state.checkingForUpdates);
  const availableUpdate = useUiStore((state) => state.availableUpdate);
  const openUpdateDialog = useUiStore((state) => state.openUpdateDialog);
  const setSettingsOpen = useUiStore((state) => state.setSettingsOpen);
  const openEditor = useUiStore((state) => state.openEditor);
  const checkForUpdates = useUiStore((state) => state.checkForUpdates);
  const setStartupSettingsOpen = useUiStore(
    (state) => state.setStartupSettingsOpen,
  );
  const setAboutOpen = useUiStore((state) => state.setAboutOpen);
  const setQuitConfirmationOpen = useUiStore(
    (state) => state.setQuitConfirmationOpen,
  );
  const { exportConfiguration, importConfiguration } = useAppCommands();

  return (
    <header className="app-bar">
      <div className="app-bar-actions">
        <Button
          variant="new"
          icon={<Plus size={18} strokeWidth={1.9} />}
          onClick={() => openEditor()}
        >
          New
        </Button>
        <SettingsMenu
          open={settingsOpen}
          checkingUpdates={checkingForUpdates}
          updateVersion={availableUpdate?.version ?? null}
          onOpenChange={setSettingsOpen}
          onStartupSettings={() => {
            setSettingsOpen(false);
            setStartupSettingsOpen(true);
          }}
          onCheckUpdates={() => {
            if (availableUpdate) {
              setSettingsOpen(false);
              openUpdateDialog();
              return;
            }
            void checkForUpdates();
          }}
          onExport={() => {
            setSettingsOpen(false);
            void exportConfiguration();
          }}
          onImport={() => {
            setSettingsOpen(false);
            void importConfiguration();
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
      </div>
    </header>
  );
}
