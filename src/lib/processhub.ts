import { invoke } from "@tauri-apps/api/core";
import type {
  CreateService,
  LogEvent,
  RunHistory,
  RuntimeState,
  Service,
} from "../types";

export const processHub = {
  listServices: () => invoke<Service[]>("list_services"),
  getService: (id: string) => invoke<Service | null>("get_service", { id }),
  createService: (input: CreateService) =>
    invoke<Service>("create_service", { input }),
  updateService: (service: Service) =>
    invoke<Service>("update_service", { service }),
  deleteService: (id: string) => invoke<boolean>("delete_service", { id }),
  importServiceIcon: (sourcePath: string) =>
    invoke<string>("import_service_icon", { sourcePath }),
  getSetting: <T>(key: string) => invoke<T | null>("get_app_setting", { key }),
  setSetting: (key: string, value: unknown) =>
    invoke<void>("set_app_setting", { key, value }),
  startAutoStartServices: () =>
    invoke<{ started: string[]; failures: string[] }>(
      "start_auto_start_services",
    ),
  recoverOrphanedServices: () =>
    invoke<{
      cleared: string[];
      unverifiable: string[];
      failures: string[];
    }>("recover_orphaned_services"),
  stopAllServices: () => invoke<string[]>("stop_all_services"),
  quitApplication: () => invoke<void>("quit_application"),
  exportConfiguration: () => invoke<string>("export_configuration"),
  startService: (id: string) => invoke<void>("start_service", { id }),
  stopService: (id: string) => invoke<void>("stop_service", { id }),
  restartService: (id: string) => invoke<void>("restart_service", { id }),
  getRuntimeStates: () => invoke<RuntimeState[]>("get_runtime_states"),
  getServiceLogs: (id: string) =>
    invoke<LogEvent[]>("get_service_logs", { id }),
  getServiceRunHistory: (id: string) =>
    invoke<RunHistory[]>("get_service_run_history", { id }),
};
