import { Info, PowerIcon, Settings2 } from "lucide-react";
import { useEffect } from "react";

interface SettingsMenuProps {
  onAbout: () => void;
  onStartupSettings: () => void;
  onQuit: () => void;
  onClose: () => void;
}

export function SettingsMenu({
  onAbout,
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
