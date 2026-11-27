import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  LogEvent,
  RuntimeState,
  UpdatePhase,
  UpdateProgress,
} from "../types";

export const processHubEvents = {
  onRuntimeState: (
    handler: (state: RuntimeState) => void,
  ): Promise<UnlistenFn> =>
    listen<RuntimeState>("service-state", ({ payload }) => handler(payload)),
  onLogBatch: (handler: (events: LogEvent[]) => void): Promise<UnlistenFn> =>
    listen<LogEvent[]>("service-log-batch", ({ payload }) => handler(payload)),
  onUpdateProgress: (
    handler: (progress: UpdateProgress) => void,
  ): Promise<UnlistenFn> =>
    listen<UpdateProgress>("update-progress", ({ payload }) =>
      handler(payload),
    ),
  onUpdateStatus: (
    handler: (status: UpdatePhase) => void,
  ): Promise<UnlistenFn> =>
    listen<{ status: UpdatePhase }>("update-status", ({ payload }) =>
      handler(payload.status),
    ),
};
