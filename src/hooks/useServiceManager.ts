import { useCallback, useEffect, useState } from "react";
import { processHubEvents } from "../lib/events";
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

export function useServiceManager() {
  const [services, setServices] = useState<Service[]>([]);
  const [runtime, setRuntime] = useState<Record<string, RuntimeState>>({});
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const [savedServices, states] = await Promise.all([
        processHub.listServices(),
        processHub.getRuntimeStates(),
      ]);
      setServices(savedServices);
      setRuntime(
        Object.fromEntries(states.map((state) => [state.serviceId, state])),
      );
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void processHubEvents
      .onRuntimeState((state) => {
        setRuntime((current) => ({ ...current, [state.serviceId]: state }));
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((cause: unknown) => {
        if (!disposed) setError(String(cause));
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  const runAction = useCallback(
    async (service: Service, action: "start" | "stop" | "restart") => {
      setBusy(service.id);
      setError(null);
      try {
        if (action === "start") await processHub.startService(service.id);
        if (action === "stop") await processHub.stopService(service.id);
        if (action === "restart") await processHub.restartService(service.id);
      } catch (cause) {
        setError(String(cause));
      } finally {
        setBusy(null);
      }
    },
    [],
  );

  const saveService = useCallback(
    async (draft: ServiceDraft): Promise<boolean> => {
      const argumentsList = draft.argumentsText
        .split("\n")
        .filter((argument) => argument.length > 0);
      const plainEnvironment: EnvironmentVariable[] = draft.environmentText
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
      if (
        plainEnvironment.some(
          (item) => !item.key || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(item.key),
        )
      ) {
        setError(
          "Environment variables must use KEY=VALUE format with a valid key",
        );
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
        setError("Secrets need a valid name and a value.");
        return false;
      }
      const allKeys = [
        ...plainEnvironment.map((item) => item.key),
        ...secrets.map((secret) => secret.key),
      ];
      if (new Set(allKeys).size !== allKeys.length) {
        setError("Environment and secret names must be unique.");
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
      const secretsToWrite = secrets.filter(
        (secret) => secret.value.length > 0,
      );
      setBusy(draft.id ?? "new");
      setError(null);
      try {
        let serviceId = draft.id;
        if (draft.id) {
          const service = services.find((item) => item.id === draft.id);
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
        await refresh();
        return true;
      } catch (cause) {
        setError(String(cause));
        return false;
      } finally {
        setBusy(null);
      }
    },
    [refresh, services],
  );

  const deleteService = useCallback(
    async (service: Service) => {
      setError(null);
      try {
        await processHub.deleteService(service.id);
        await refresh();
        return true;
      } catch (cause) {
        setError(String(cause));
        return false;
      }
    },
    [refresh],
  );

  return {
    services,
    runtime,
    loading,
    busy,
    error,
    refresh,
    runAction,
    saveService,
    deleteService,
    dismissError: () => setError(null),
  };
}
