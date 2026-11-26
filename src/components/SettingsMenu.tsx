import { Info, PowerIcon, Settings2 } from "lucide-react";

interface SettingsMenuProps {
  onAbout: () => void;
  onStartupSettings: () => void;
  onQuit: () => void;
}

export function SettingsMenu({
  onAbout,
  onStartupSettings,
  onQuit,
}: SettingsMenuProps) {
  return (
    <div className="settings-popover" role="dialog" aria-label="Settings">
      <button
        className="about-menu-item"
        type="button"
        onClick={onStartupSettings}
      >
        <Settings2 size={15} />
        <span>Startup settings</span>
      </button>
      <button className="about-menu-item" type="button" onClick={onAbout}>
        <Info size={15} />
        <span>About</span>
      </button>
      <button className="about-menu-item" type="button" onClick={onQuit}>
        <PowerIcon size={15} />
        <span>Quit</span>
      </button>
    </div>
  );
}
