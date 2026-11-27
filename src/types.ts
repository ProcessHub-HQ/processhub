export type ExecutionMode = "direct" | "shell";
export type RestartPolicy = "never" | "on_failure" | "always";
export type ProcessPriority = "low" | "normal" | "high";
export type HealthCheckType = "none" | "tcp" | "http";
export type HealthStatus =
  | "not_configured"
  | "starting"
  | "healthy"
  | "unhealthy";
export type RuntimeStatus =
  | "stopped"
  | "starting"
  | "running"
  | "stopping"
  | "exited"
  | "waiting_to_restart"
  | "failed"
  | "unknown";

export interface EnvironmentVariable {
  key: string;
  value: string;
  isSecret: boolean;
}

export interface SecretDraft {
  id: string;
  key: string;
  value: string;
  existing: boolean;
}

export interface Service {
  id: string;
  name: string;
  description: string | null;
  iconPath: string | null;
  executionMode: ExecutionMode;
  executable: string;
  arguments: string[];
  shellCommand: string | null;
  workingDirectory: string | null;
  environment: EnvironmentVariable[];
  enabled: boolean;
  autoStart: boolean;
  restartPolicy: RestartPolicy;
  restartDelayMs: number;
  maxRestartAttempts: number | null;
  priority: ProcessPriority;
  stopGracePeriodMs: number;
  logMaxSizeMb: number;
  healthCheckType: HealthCheckType;
  healthTarget: string | null;
  healthIntervalMs: number;
  healthTimeoutMs: number;
  healthExpectedStatus: number | null;
  healthStartPeriodMs: number;
  dependsOn: string[];
  createdAt: string;
  updatedAt: string;
  sortOrder: number;
}

export interface ImportReport {
  importedServices: number;
  droppedSecrets: number;
  renamedServices: number;
  droppedDependencies: number;
  droppedIcons: number;
  sanitizedDirectories: number;
  importedSettings: number;
}

export type CreateService = Pick<
  Service,
  | "name"
  | "iconPath"
  | "executable"
  | "arguments"
  | "workingDirectory"
  | "environment"
  | "description"
  | "enabled"
  | "autoStart"
  | "dependsOn"
  | "restartPolicy"
  | "restartDelayMs"
  | "maxRestartAttempts"
  | "priority"
  | "stopGracePeriodMs"
  | "logMaxSizeMb"
  | "healthCheckType"
  | "healthTarget"
  | "healthIntervalMs"
  | "healthTimeoutMs"
  | "healthExpectedStatus"
  | "healthStartPeriodMs"
>;

export interface RuntimeState {
  serviceId: string;
  status: RuntimeStatus;
  pid: number | null;
  exitCode: number | null;
  startedAtMs: number | null;
  cpuUsagePercent: number | null;
  memoryUsageBytes: number | null;
  treeCpuUsagePercent: number | null;
  treeMemoryBytes: number | null;
  processCount: number | null;
  metricsSampledAtMs: number | null;
  health: HealthStatus;
  healthCheckedAtMs: number | null;
  healthLatencyMs: number | null;
  healthError: string | null;
  detectedUrl: string | null;
  restartAtMs: number | null;
  restartAttempt: number | null;
  error: string | null;
}

export interface LogEvent {
  serviceId: string;
  stream: "stdout" | "stderr" | "internal";
  timestampMs: number;
  text: string;
}

export interface RunHistory {
  id: string;
  serviceId: string;
  startedAtMs: number;
  endedAtMs: number;
  status: RuntimeStatus;
  exitCode: number | null;
  error: string | null;
}

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  notes: string | null;
  date: string | null;
}

export interface AppSettings {
  theme: "light";
  startAtLogin: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  startAutoStartServices: boolean;
  logRetentionDays: number;
}
