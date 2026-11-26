import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { LogEvent, RuntimeState } from "../types";

export const processHubEvents = {
  onRuntimeState: (
    handler: (state: RuntimeState) => void,
  ): Promise<UnlistenFn> =>
    listen<RuntimeState>("service-state", ({ payload }) => handler(payload)),
  onLog: (handler: (event: LogEvent) => void): Promise<UnlistenFn> =>
    listen<LogEvent>("service-log", ({ payload }) => handler(payload)),
};
