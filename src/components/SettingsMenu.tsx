import {
  Download,
  Info,
  PowerIcon,
  RefreshCw,
  Settings,
  Settings2,
  Upload,
} from "lucide-react";
import { useEffect } from "react";
import { Button } from "./Button";

interface SettingsMenuProps {
  open: boolean;
  checkingUpdates: boolean;
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
      <Button
        variant="icon"
        data-settings-trigger=""
        className={open ? "active" : ""}
        aria-label="Settings"
        aria-expanded={open}
        title="Settings"
        icon={<Settings size={17} strokeWidth={1.8} />}
        onClick={() => onOpenChange(!open)}
      />
      {open && (
        <div
          className="context-menu settings-popover"
          data-settings-menu=""
          role="menu"
          aria-label="Settings"
        >
          <button type="button" role="menuitem" onClick={onStartupSettings}>
            <Settings2 size={14} />
            <span>Startup settings</span>
          </button>
          <button
            type="button"
            role="menuitem"
            aria-busy={checkingUpdates}
            disabled={checkingUpdates}
            onClick={onCheckUpdates}
          >
            <RefreshCw
              className={checkingUpdates ? "is-spinning" : undefined}
              size={14}
            />
            <span>
              {checkingUpdates ? "Checking for updates…" : "Check for updates…"}
            </span>
          </button>
          <div className="menu-rule" />
          <button type="button" role="menuitem" onClick={onExport}>
            <Download size={14} />
            <span>Export configuration…</span>
          </button>
          <button type="button" role="menuitem" onClick={onImport}>
            <Upload size={14} />
            <span>Import configuration…</span>
          </button>
          <div className="menu-rule" />
          <button type="button" role="menuitem" onClick={onAbout}>
            <Info size={14} />
            <span>About</span>
          </button>
          <div className="menu-rule" />
          <button
            className="danger-item"
            type="button"
            role="menuitem"
            onClick={onQuit}
          >
            <PowerIcon size={14} />
            <span>Quit</span>
          </button>
        </div>
      )}
    </>
  );
}
