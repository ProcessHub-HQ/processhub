import {
  IconButton,
  Menu,
  MenuItem,
  MenuSeparator,
} from "@processhub/design-system";
import {
  ArrowUpCircle,
  Download,
  Info,
  PowerIcon,
  RefreshCw,
  Settings,
  Settings2,
  Upload,
} from "lucide-react";
import { useEffect } from "react";
import { AppearanceSwitch } from "./AppearanceSwitch";

interface SettingsMenuProps {
  open: boolean;
  checkingUpdates: boolean;
  updateVersion: string | null;
  onOpenChange: (open: boolean) => void;
  onAbout: () => void;
  onCheckUpdates: () => void;
  onExport: () => void;
  onImport: () => void;
  onStartupSettings: () => void;
  onQuit: () => void;
}

export function SettingsMenu({
  open,
  checkingUpdates,
  updateVersion,
  onOpenChange,
  onAbout,
  onCheckUpdates,
  onExport,
  onImport,
  onStartupSettings,
  onQuit,
}: SettingsMenuProps) {
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (
        !(event.target instanceof Element) ||
        (!event.target.closest("[data-settings-menu]") &&
          !event.target.closest("[data-settings-trigger]"))
      ) {
        onOpenChange(false);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onOpenChange(false);
    };
    const onBlur = () => onOpenChange(false);

    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", onBlur);
    };
  }, [onOpenChange, open]);

  return (
    <>
      <IconButton
        size="sm"
        data-settings-trigger=""
        active={open}
        label="Settings"
        aria-expanded={open}
        title="Settings"
        onClick={() => onOpenChange(!open)}
      >
        <Settings size={17} strokeWidth={1.8} />
      </IconButton>
      {open && (
        <div className="settings-popover" data-settings-menu="">
          <div className="settings-appearance">
            <span className="settings-appearance__label">Appearance</span>
            <AppearanceSwitch />
          </div>
          <Menu className="settings-menu" aria-label="Settings">
            <MenuItem
              icon={<Settings2 size={14} />}
              onSelect={onStartupSettings}
            >
              Startup settings
            </MenuItem>
            <MenuItem
              icon={
                checkingUpdates ? (
                  <RefreshCw className="is-spinning" size={14} />
                ) : updateVersion ? (
                  <ArrowUpCircle size={14} />
                ) : (
                  <RefreshCw size={14} />
                )
              }
              aria-busy={checkingUpdates}
              disabled={checkingUpdates}
              onSelect={onCheckUpdates}
            >
              {checkingUpdates
                ? "Checking for updates…"
                : updateVersion
                  ? `Update to v${updateVersion}…`
                  : "Check for updates…"}
            </MenuItem>
            <MenuSeparator />
            <MenuItem icon={<Download size={14} />} onSelect={onExport}>
              Export configuration…
            </MenuItem>
            <MenuItem icon={<Upload size={14} />} onSelect={onImport}>
              Import configuration…
            </MenuItem>
            <MenuSeparator />
            <MenuItem icon={<Info size={14} />} onSelect={onAbout}>
              About
            </MenuItem>
            <MenuSeparator />
            <MenuItem danger icon={<PowerIcon size={14} />} onSelect={onQuit}>
              Quit
            </MenuItem>
          </Menu>
        </div>
      )}
    </>
  );
}
