import { invoke } from "@tauri-apps/api/core";
import type {
  CreateService,
  ImportReport,
  LogEvent,
  RunHistory,
  RuntimeState,
  Service,
} from "../types";

export const processHub = {
  listServices: () => invoke<Service[]>("list_services"),
  createService: (input: CreateService) =>
    invoke<Service>("create_service", { input }),
  updateService: (service: Service) =>
    invoke<Service>("update_service", { service }),
  deleteService: (id: string) => invoke<boolean>("delete_service", { id }),
  importServiceIcon: (sourcePath: string) =>
    invoke<string>("import_service_icon", { sourcePath }),
  setServiceSecret: (serviceId: string, key: string, value: string) =>
    invoke<void>("set_service_secret", { serviceId, key, value }),
  deleteServiceSecret: (serviceId: string, key: string) =>
    invoke<void>("delete_service_secret", { serviceId, key }),
  getSetting: <T>(key: string) => invoke<T | null>("get_app_setting", { key }),
  setSetting: (key: string, value: unknown) =>
    invoke<void>("set_app_setting", { key, value }),
  startAutoStartServices: () =>
    invoke<{ started: string[]; failures: string[] }>(
      "start_auto_start_services",
    ),
  startAllServices: () =>
    invoke<{ started: string[]; failures: string[] }>("start_all_services"),
  restartAllServices: () =>
    invoke<{ started: string[]; failures: string[] }>("restart_all_services"),
  recoverOrphanedServices: () =>
    invoke<{
      cleared: string[];
      unverifiable: string[];
      failures: string[];
    }>("recover_orphaned_services"),
  stopAllServices: () => invoke<string[]>("stop_all_services"),
  quitApplication: () => invoke<void>("quit_application"),
  exportConfiguration: () => invoke<string>("export_configuration"),
  exportConfigurationTo: (path: string) =>
    invoke<void>("export_configuration_to", { path }),
  importConfigurationFrom: (path: string) =>
    invoke<ImportReport>("import_configuration_from", { path }),
  startService: (id: string) => invoke<void>("start_service", { id }),
  stopService: (id: string) => invoke<void>("stop_service", { id }),
  restartService: (id: string) => invoke<void>("restart_service", { id }),
  getRuntimeStates: () => invoke<RuntimeState[]>("get_runtime_states"),
  getServiceLogs: (id: string) =>
    invoke<LogEvent[]>("get_service_logs", { id }),
  getServiceRunHistory: (id: string) =>
    invoke<RunHistory[]>("get_service_run_history", { id }),
};
