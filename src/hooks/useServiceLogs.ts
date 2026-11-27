import { useCallback, useEffect, useRef, useState } from "react";
import { processHubEvents } from "../lib/events";
import { processHub } from "../lib/processhub";
import type { LogEvent, RunHistory } from "../types";

const MAX_EVENTS_PER_SERVICE = 2_000;
const MAX_PENDING_EVENTS_PER_SERVICE = 4_000;
const MAX_BUFFER_BYTES_PER_SERVICE = 4 * 1024 * 1024;
const EVENT_OVERHEAD_BYTES = 96;
const FLUSH_INTERVAL_MS = 50;

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

export function useServiceLogs() {
  const [events, setEvents] = useState<Record<string, LogEvent[]>>({});
  const [runHistory, setRunHistory] = useState<Record<string, RunHistory[]>>(
    {},
  );
  const [dropped, setDropped] = useState<Record<string, number>>({});
  const [retentionDays, setRetentionDays] = useState(14);
  const pending = useRef<Record<string, LogEvent[]>>({});
  const pendingDrops = useRef<Record<string, number>>({});
  const changeRetention = useCallback(async (days: number) => {
    await processHub.setSetting("logRetentionDays", days);
    setRetentionDays(days);
  }, []);
  const load = useCallback(async (serviceId: string) => {
    const [savedEvents, savedRuns] = await Promise.all([
      processHub.getServiceLogs(serviceId),
      processHub.getServiceRunHistory(serviceId),
    ]);
    setEvents((current) => {
      const combined = [...savedEvents, ...(current[serviceId] ?? [])];
      const unique = new Map(
        combined.map((event) => [
          `${event.timestampMs}:${event.stream}:${event.text}`,
          event,
        ]),
      );
      return {
        ...current,
        [serviceId]: trimEvents(
          [...unique.values()].sort(
            (left, right) => left.timestampMs - right.timestampMs,
          ),
        ),
      };
    });
    setRunHistory((current) => ({ ...current, [serviceId]: savedRuns }));
    setDropped((current) => ({ ...current, [serviceId]: 0 }));
  }, []);

  useEffect(() => {
    void processHub
      .getSetting<number>("logRetentionDays")
      .then((days) => {
        if (days !== null && [7, 14, 30, 90].includes(days)) {
          setRetentionDays(days);
        }
      })
      .catch(() => {});
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let unlistenRuntime: (() => void) | undefined;
    const flush = () => {
      const batch = pending.current;
      const drops = pendingDrops.current;
      pending.current = {};
      pendingDrops.current = {};
      if (Object.keys(batch).length > 0) {
        setEvents((current) => {
          const next = { ...current };
          for (const [serviceId, incoming] of Object.entries(batch)) {
            next[serviceId] = trimEvents([
              ...(current[serviceId] ?? []),
              ...incoming,
            ]);
          }
          return next;
        });
      }
      if (Object.keys(drops).length > 0) {
        setDropped((current) => {
          const next = { ...current };
          for (const [serviceId, count] of Object.entries(drops)) {
            next[serviceId] = (current[serviceId] ?? 0) + count;
          }
          return next;
        });
      }
    };
    const interval = window.setInterval(flush, FLUSH_INTERVAL_MS);
    void processHubEvents
      .onLogBatch((events) => {
        for (const event of events) {
          const servicePending = pending.current[event.serviceId] ?? [];
          servicePending.push(event);
          const overflow = Math.max(
            0,
            servicePending.length - MAX_PENDING_EVENTS_PER_SERVICE,
          );
          if (overflow > 0) {
            servicePending.splice(0, overflow);
            pendingDrops.current[event.serviceId] =
              (pendingDrops.current[event.serviceId] ?? 0) + overflow;
          }
          pending.current[event.serviceId] = servicePending;
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // The desktop event bridge can be unavailable while the app is booting.
      });
    void processHubEvents
      .onRuntimeState((state) => {
        if (
          ["stopped", "exited", "failed", "waiting_to_restart"].includes(
            state.status,
          )
        ) {
          void load(state.serviceId).catch(() => {});
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlistenRuntime = stop;
      })
      .catch(() => {
        // The desktop event bridge can be unavailable while the app is booting.
      });
    return () => {
      disposed = true;
      window.clearInterval(interval);
      unlisten?.();
      unlistenRuntime?.();
    };
  }, [load]);

  function clear(serviceId: string) {
    delete pending.current[serviceId];
    delete pendingDrops.current[serviceId];
    setDropped((current) => ({ ...current, [serviceId]: 0 }));
    setEvents((current) => ({ ...current, [serviceId]: [] }));
  }

  return {
    events,
    runHistory,
    dropped,
    clear,
    load,
    retentionDays,
    changeRetention,
  };
}
