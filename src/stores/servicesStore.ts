import { create } from "zustand";
import { friendlyError } from "../lib/errors";
import { processHub } from "../lib/processhub";
import type {
  CreateService,
  EnvironmentVariable,
  RuntimeState,
  SecretDraft,
  Service,
} from "../types";

export interface ServiceDraft {
  id?: string;
  name: string;
  description: string;
  iconPath: string | null;
  dependsOn: string[];
  executable: string;
  argumentsText: string;
  workingDirectory: string;
  environmentText: string;
  secrets: SecretDraft[];
  restartPolicy: Service["restartPolicy"];
  restartDelayMs: number;
  maxRestartAttempts: number | null;
  stopGracePeriodMs: number;
  logMaxSizeMb: number;
  healthCheckType: Service["healthCheckType"];
  healthTarget: string;
  healthIntervalMs: number;
  healthTimeoutMs: number;
  healthExpectedStatus: number | null;
  healthStartPeriodMs: number;
  enabled: boolean;
  autoStart: boolean;
}

interface ServicesState {
  services: Service[];
  runtime: Record<string, RuntimeState>;
  loading: boolean;
  busy: string | null;
  error: string | null;
  refresh: () => Promise<void>;
  setRuntimeState: (state: RuntimeState) => void;
  runAction: (
    service: Service,
    action: "start" | "stop" | "restart",
  ) => Promise<void>;
  saveService: (draft: ServiceDraft) => Promise<boolean>;
  deleteService: (service: Service) => Promise<boolean>;
  dismissError: () => void;
}

function parseEnvironment(text: string): EnvironmentVariable[] {
  return text
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => {
      const separator = line.indexOf("=");
      return {
        key: line.slice(0, separator).trim(),
        value: line.slice(separator + 1),
        isSecret: false,
      };
    });
}

export const useServicesStore = create<ServicesState>((set, get) => ({
  services: [],
  runtime: {},
  loading: true,
  busy: null,
  error: null,

  refresh: async () => {
    set({ error: null });
    try {
      const [services, states] = await Promise.all([
        processHub.listServices(),
        processHub.getRuntimeStates(),
      ]);
      set({
        services,
        runtime: Object.fromEntries(
          states.map((state) => [state.serviceId, state]),
        ),
      });
    } catch (cause) {
      set({
        error: friendlyError(
          cause,
          "ProcessHub couldn't load your services. Try again.",
        ),
      });
    } finally {
      set({ loading: false });
    }
  },

  setRuntimeState: (state) =>
    set((current) => ({
      runtime: { ...current.runtime, [state.serviceId]: state },
    })),

  runAction: async (service, action) => {
    set({ busy: service.id, error: null });
    try {
      if (action === "start") await processHub.startService(service.id);
      if (action === "stop") await processHub.stopService(service.id);
      if (action === "restart") await processHub.restartService(service.id);
    } catch (cause) {
      const fallback =
        action === "start"
          ? "The service couldn't start. Check the executable, arguments, and working directory."
          : action === "stop"
            ? "The service couldn't stop. Try again."
            : "The service couldn't restart. Try again.";
      set({ error: friendlyError(cause, fallback) });
    } finally {
      set({ busy: null });
    }
  },

  saveService: async (draft) => {
    const argumentsList = draft.argumentsText
      .split("\n")
      .filter((argument) => argument.length > 0);
    const plainEnvironment = parseEnvironment(draft.environmentText);
    if (
      plainEnvironment.some(
        (item) => !item.key || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(item.key),
      )
    ) {
      set({
        error:
          "Environment variables must use KEY=VALUE format with a valid key",
      });
      return false;
    }

    const secrets = draft.secrets.map((secret) => ({
      ...secret,
      key: secret.key.trim(),
    }));
    if (
      secrets.some(
        (secret) =>
          !/^[A-Za-z_][A-Za-z0-9_]*$/.test(secret.key) ||
          (!secret.existing && secret.value.length === 0),
      )
    ) {
      set({ error: "Secrets need a valid name and a value." });
      return false;
    }

    const allKeys = [
      ...plainEnvironment.map((item) => item.key),
      ...secrets.map((secret) => secret.key),
    ];
    if (new Set(allKeys).size !== allKeys.length) {
      set({ error: "Environment and secret names must be unique." });
      return false;
    }

    const environment: EnvironmentVariable[] = [
      ...plainEnvironment,
      ...secrets.map((secret) => ({
        key: secret.key,
        value: "",
        isSecret: true,
      })),
    ];
    const secretsToWrite = secrets.filter((secret) => secret.value.length > 0);

    set({ busy: draft.id ?? "new", error: null });
    try {
      let serviceId = draft.id;
      if (draft.id) {
        const service = get().services.find((item) => item.id === draft.id);
        if (!service) throw new Error("This service is no longer available");
        await processHub.updateService({
          ...service,
          name: draft.name.trim(),
          description: draft.description.trim() || null,
          iconPath: draft.iconPath,
          dependsOn: draft.dependsOn,
          executable: draft.executable.trim(),
          arguments: argumentsList,
          workingDirectory: draft.workingDirectory.trim() || null,
          environment,
          restartPolicy: draft.restartPolicy,
          restartDelayMs: draft.restartDelayMs,
          maxRestartAttempts: draft.maxRestartAttempts,
          stopGracePeriodMs: draft.stopGracePeriodMs,
          logMaxSizeMb: draft.logMaxSizeMb,
          healthCheckType: draft.healthCheckType,
          healthTarget: draft.healthTarget.trim() || null,
          healthIntervalMs: draft.healthIntervalMs,
          healthTimeoutMs: draft.healthTimeoutMs,
          healthExpectedStatus: draft.healthExpectedStatus,
          healthStartPeriodMs: draft.healthStartPeriodMs,
          enabled: draft.enabled,
          autoStart: draft.autoStart,
        });
      } else {
        const input: CreateService = {
          name: draft.name.trim(),
          iconPath: draft.iconPath,
          executable: draft.executable.trim(),
          arguments: argumentsList,
          workingDirectory: draft.workingDirectory.trim() || null,
          environment,
          description: draft.description.trim() || null,
          enabled: draft.enabled,
          autoStart: draft.autoStart,
          dependsOn: draft.dependsOn,
          restartPolicy: draft.restartPolicy,
          restartDelayMs: draft.restartDelayMs,
          maxRestartAttempts: draft.maxRestartAttempts,
          priority: "normal",
          stopGracePeriodMs: draft.stopGracePeriodMs,
          logMaxSizeMb: draft.logMaxSizeMb,
          healthCheckType: draft.healthCheckType,
          healthTarget: draft.healthTarget.trim() || null,
          healthIntervalMs: draft.healthIntervalMs,
          healthTimeoutMs: draft.healthTimeoutMs,
          healthExpectedStatus: draft.healthExpectedStatus,
          healthStartPeriodMs: draft.healthStartPeriodMs,
        };
        const created = await processHub.createService(input);
        serviceId = created.id;
      }
      if (serviceId) {
        for (const secret of secretsToWrite) {
          await processHub.setServiceSecret(
            serviceId,
            secret.key,
            secret.value,
          );
        }
      }
      await get().refresh();
      return true;
    } catch (cause) {
      set({
        error: friendlyError(
          cause,
          "ProcessHub couldn't save this service. Try again.",
        ),
      });
      return false;
    } finally {
      set({ busy: null });
    }
  },

  deleteService: async (service) => {
    set({ error: null });
    try {
      await processHub.deleteService(service.id);
      await get().refresh();
      return true;
    } catch (cause) {
      set({
        error: friendlyError(
          cause,
          "ProcessHub couldn't delete this service. Try again.",
        ),
      });
      return false;
    }
  },

  dismissError: () => set({ error: null }),
}));
