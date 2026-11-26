export type ExecutionMode = "direct" | "shell";
export type RestartPolicy = "never" | "on_failure" | "always";
export type ProcessPriority = "low" | "normal" | "high";
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
  createdAt: string;
  updatedAt: string;
  sortOrder: number;
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
  | "restartPolicy"
  | "restartDelayMs"
  | "maxRestartAttempts"
  | "priority"
  | "stopGracePeriodMs"
>;

export interface RuntimeState {
  serviceId: string;
  status: RuntimeStatus;
  pid: number | null;
  exitCode: number | null;
  startedAtMs: number | null;
  cpuUsagePercent: number | null;
  memoryUsageBytes: number | null;
  metricsSampledAtMs: number | null;
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

export interface AppSettings {
  theme: "light";
  startAtLogin: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  startAutoStartServices: boolean;
  logRetentionDays: number;
}
