import { create } from "zustand";
import { processHub } from "../lib/processhub";
import type { LogEvent, RunHistory } from "../types";

const MAX_EVENTS_PER_SERVICE = 2_000;
const MAX_PENDING_EVENTS_PER_SERVICE = 4_000;
const MAX_BUFFER_BYTES_PER_SERVICE = 4 * 1024 * 1024;
const EVENT_OVERHEAD_BYTES = 96;

let pending: Record<string, LogEvent[]> = {};
let pendingDrops: Record<string, number> = {};

function trimEvents(events: LogEvent[]): LogEvent[] {
  const bounded =
    events.length > MAX_EVENTS_PER_SERVICE
      ? events.slice(-MAX_EVENTS_PER_SERVICE)
      : events;
  let bytes = 0;
  let start = bounded.length;
  while (start > 0) {
    const size = bounded[start - 1].text.length + EVENT_OVERHEAD_BYTES;
    if (bytes + size > MAX_BUFFER_BYTES_PER_SERVICE && start < bounded.length) {
      break;
    }
    bytes += size;
    start -= 1;
  }
  return start === 0 ? bounded : bounded.slice(start);
}

interface LogsState {
  events: Record<string, LogEvent[]>;
  runHistory: Record<string, RunHistory[]>;
  dropped: Record<string, number>;
  retentionDays: number;
  enqueue: (incoming: LogEvent[]) => void;
  flush: () => void;
  load: (serviceId: string) => Promise<void>;
  clear: (serviceId: string) => void;
  changeRetention: (days: number) => Promise<void>;
  setRetentionDays: (days: number) => void;
}

export const useLogsStore = create<LogsState>((set) => ({
  events: {},
  runHistory: {},
  dropped: {},
  retentionDays: 14,

  enqueue: (incoming) => {
    for (const event of incoming) {
      const servicePending = pending[event.serviceId] ?? [];
      servicePending.push(event);
      const overflow = Math.max(
        0,
        servicePending.length - MAX_PENDING_EVENTS_PER_SERVICE,
      );
      if (overflow > 0) {
        servicePending.splice(0, overflow);
        pendingDrops[event.serviceId] =
          (pendingDrops[event.serviceId] ?? 0) + overflow;
      }
      pending[event.serviceId] = servicePending;
    }
  },

  flush: () => {
    const batch = pending;
    const drops = pendingDrops;
    pending = {};
    pendingDrops = {};
    if (Object.keys(batch).length > 0) {
      set((current) => {
        const next = { ...current.events };
        for (const [serviceId, incoming] of Object.entries(batch)) {
          next[serviceId] = trimEvents([
            ...(current.events[serviceId] ?? []),
            ...incoming,
          ]);
        }
        return { events: next };
      });
    }
    if (Object.keys(drops).length > 0) {
      set((current) => {
        const next = { ...current.dropped };
        for (const [serviceId, count] of Object.entries(drops)) {
          next[serviceId] = (current.dropped[serviceId] ?? 0) + count;
        }
        return { dropped: next };
      });
    }
  },

  load: async (serviceId) => {
    const [savedEvents, savedRuns] = await Promise.all([
      processHub.getServiceLogs(serviceId),
      processHub.getServiceRunHistory(serviceId),
    ]);
    set((current) => {
      const combined = [...savedEvents, ...(current.events[serviceId] ?? [])];
      const unique = new Map(
        combined.map((event) => [
          `${event.timestampMs}:${event.stream}:${event.text}`,
          event,
        ]),
      );
      return {
        events: {
          ...current.events,
          [serviceId]: trimEvents(
            [...unique.values()].sort(
              (left, right) => left.timestampMs - right.timestampMs,
            ),
          ),
        },
        runHistory: { ...current.runHistory, [serviceId]: savedRuns },
        dropped: { ...current.dropped, [serviceId]: 0 },
      };
    });
  },

  clear: (serviceId) => {
    delete pending[serviceId];
    delete pendingDrops[serviceId];
    set((current) => ({
      events: { ...current.events, [serviceId]: [] },
      dropped: { ...current.dropped, [serviceId]: 0 },
    }));
  },

  changeRetention: async (days) => {
    await processHub.setSetting("logRetentionDays", days);
    set({ retentionDays: days });
  },

  setRetentionDays: (days) => {
    if ([7, 14, 30, 90].includes(days)) set({ retentionDays: days });
  },
}));
