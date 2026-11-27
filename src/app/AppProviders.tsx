import {
  Info,
  Play,
  Plus,
  PowerIcon,
  RotateCw,
  Settings2,
  Square,
} from "lucide-react";
import { type ReactNode, useCallback } from "react";
import {
  type ContextMenuEntry,
  ContextMenuProvider,
} from "../components/ContextMenuProvider";

function defaultContextItems(): ContextMenuEntry[] {
  return [
    {
      id: "workspace-new",
      label: "New process",
      icon: <Plus size={14} />,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:new"));
      },
    },
    { type: "separator", id: "workspace-sep-services" },
    {
      id: "workspace-start-all",
      label: "Start all",
      icon: <Play size={14} />,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:start-all"));
      },
    },
    {
      id: "workspace-restart-all",
      label: "Restart all",
      icon: <RotateCw size={14} />,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:restart-all"));
      },
    },
    {
      id: "workspace-stop-all",
      label: "Stop all",
      icon: <Square size={14} />,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:stop-all"));
      },
    },
    { type: "separator", id: "workspace-sep-1" },
    {
      id: "workspace-startup",
      label: "Startup settings",
      icon: <Settings2 size={14} />,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:startup-settings"));
      },
    },
    {
      id: "workspace-about",
      label: "About",
      icon: <Info size={14} />,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:about"));
      },
    },
    { type: "separator", id: "workspace-sep-2" },
    {
      id: "workspace-quit",
      label: "Quit",
      icon: <PowerIcon size={14} />,
      danger: true,
      onSelect: () => {
        window.dispatchEvent(new CustomEvent("processhub:quit"));
      },
    },
  ];
}

interface AppProvidersProps {
  children: ReactNode;
}

export function AppProviders({ children }: AppProvidersProps) {
  const getDefaultItems = useCallback(() => defaultContextItems(), []);
  return (
    <ContextMenuProvider getDefaultItems={getDefaultItems}>
      {children}
    </ContextMenuProvider>
  );
}
