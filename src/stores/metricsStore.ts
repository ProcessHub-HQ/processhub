import { create } from "zustand";
import type { RuntimeState } from "../types";

const MAX_SAMPLES = 90;

export interface MetricSample {
  t: number;
  cpu: number;
  memory: number;
  processes: number;
}

interface MetricsState {
  history: Record<string, MetricSample[]>;
  record: (state: RuntimeState) => void;
}

export const useMetricsStore = create<MetricsState>((set) => ({
  history: {},
  record: (state) => {
    const sampledAt = state.metricsSampledAtMs ?? Date.now();
    const cpu = state.treeCpuUsagePercent ?? state.cpuUsagePercent ?? 0;
    const memory = state.treeMemoryBytes ?? state.memoryUsageBytes ?? 0;
    const processes = state.processCount ?? (state.pid ? 1 : 0);

    set((current) => {
      const samples = current.history[state.serviceId] ?? [];
      const last = samples[samples.length - 1];
      // Runtime updates can arrive more often than the backend samples metrics.
      if (last && sampledAt - last.t < 750) return current;

      return {
        history: {
          ...current.history,
          [state.serviceId]: [
            ...samples,
            { t: sampledAt, cpu, memory, processes },
          ].slice(-MAX_SAMPLES),
        },
      };
    });
  },
}));
