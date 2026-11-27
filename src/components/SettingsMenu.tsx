import {
  Download,
  Info,
  PowerIcon,
  RefreshCw,
  Settings2,
  Upload,
} from "lucide-react";
import { useEffect } from "react";

interface SettingsMenuProps {
  onAbout: () => void;
  onCheckUpdates: () => void;
  onExport: () => void;
  onImport: () => void;
  onStartupSettings: () => void;
  onQuit: () => void;
  onClose: () => void;
}

export function SettingsMenu({
  onAbout,
  onCheckUpdates,
  onExport,
  onImport,
  onStartupSettings,
  onQuit,
  onClose,
}: SettingsMenuProps) {
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (
        !(event.target instanceof Element) ||
        (!event.target.closest("[data-settings-menu]") &&
          !event.target.closest("[data-settings-trigger]"))
      ) {
        onClose();
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const onBlur = () => onClose();

    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", onBlur);
    };
  }, [onClose]);

  return (
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
      <button type="button" role="menuitem" onClick={onCheckUpdates}>
        <RefreshCw size={14} />
        <span>Check for updates…</span>
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
  );
}
