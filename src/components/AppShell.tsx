import { Plus, Settings } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "./Button";

interface AppShellProps {
  children: ReactNode;
  settingsOpen: boolean;
  onNew: () => void;
  onToggleSettings: () => void;
}

export function AppShell({
  children,
  settingsOpen,
  onNew,
  onToggleSettings,
}: AppShellProps) {
  return (
    <main className="desktop">
      <header className="app-bar">
        <div className="app-brand">
          <span className="app-brand-mark">
            <img className={"app-brand-image"} src={"/icon.svg"} alt={"Logo"} />
          </span>
          <span>ProcessHub</span>
        </div>
        <div className="app-bar-actions">
          <Button
            variant="new"
            icon={<Plus size={18} strokeWidth={1.9} />}
            onClick={onNew}
          >
            New
          </Button>
          <Button
            variant="icon"
            data-settings-trigger=""
            className={settingsOpen ? "active" : ""}
            aria-label="Settings"
            aria-expanded={settingsOpen}
            title="Settings"
            icon={<Settings size={17} strokeWidth={1.8} />}
            onClick={onToggleSettings}
          />
        </div>
      </header>
      <section className="workspace" aria-label="Process workspace">
        {children}
      </section>
    </main>
  );
}
