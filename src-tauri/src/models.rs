use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariable {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateService {
    pub name: String,
    #[serde(default)]
    pub icon_path: Option<String>,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    #[serde(default)]
    pub environment: Vec<EnvironmentVariable>,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_restart_policy")]
    pub restart_policy: String,
    #[serde(default = "default_restart_delay_ms")]
    pub restart_delay_ms: u64,
    #[serde(default = "default_max_restart_attempts")]
    pub max_restart_attempts: Option<u32>,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_stop_grace_period_ms")]
    pub stop_grace_period_ms: u64,
    #[serde(default = "default_log_max_size_mb")]
    pub log_max_size_mb: u64,
    #[serde(default = "default_health_check_type")]
    pub health_check_type: String,
    #[serde(default)]
    pub health_target: Option<String>,
    #[serde(default = "default_health_interval_ms")]
    pub health_interval_ms: u64,
    #[serde(default = "default_health_timeout_ms")]
    pub health_timeout_ms: u64,
    #[serde(default)]
    pub health_expected_status: Option<u16>,
    #[serde(default = "default_health_start_period_ms")]
    pub health_start_period_ms: u64,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_path: Option<String>,
    pub execution_mode: String,
    pub executable: String,
    pub arguments: Vec<String>,
    pub shell_command: Option<String>,
    pub working_directory: Option<String>,
    pub environment: Vec<EnvironmentVariable>,
    pub enabled: bool,
    pub auto_start: bool,
    pub restart_policy: String,
    pub restart_delay_ms: u64,
    pub max_restart_attempts: Option<u32>,
    pub priority: String,
    pub stop_grace_period_ms: u64,
    #[serde(default = "default_log_max_size_mb")]
    pub log_max_size_mb: u64,
    #[serde(default = "default_health_check_type")]
    pub health_check_type: String,
    #[serde(default)]
    pub health_target: Option<String>,
    #[serde(default = "default_health_interval_ms")]
    pub health_interval_ms: u64,
    #[serde(default = "default_health_timeout_ms")]
    pub health_timeout_ms: u64,
    #[serde(default)]
    pub health_expected_status: Option<u16>,
    #[serde(default = "default_health_start_period_ms")]
    pub health_start_period_ms: u64,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub sort_order: i64,
}

impl Service {
    pub fn from_create(input: CreateService) -> Result<Self, &'static str> {
        let service = Self {
            id: Uuid::new_v4().to_string(),
            name: input.name,
            description: input.description,
            icon_path: input.icon_path,
            execution_mode: "direct".into(),
            executable: input.executable,
            arguments: input.arguments,
            shell_command: None,
            working_directory: input.working_directory,
            environment: input.environment,
            enabled: input.enabled,
            auto_start: input.auto_start,
            restart_policy: input.restart_policy,
            restart_delay_ms: input.restart_delay_ms,
            max_restart_attempts: input.max_restart_attempts,
            priority: input.priority,
            stop_grace_period_ms: input.stop_grace_period_ms,
            log_max_size_mb: input.log_max_size_mb,
            health_check_type: input.health_check_type,
            health_target: input.health_target,
            health_interval_ms: input.health_interval_ms,
            health_timeout_ms: input.health_timeout_ms,
            health_expected_status: input.health_expected_status,
            health_start_period_ms: input.health_start_period_ms,
            depends_on: input.depends_on,
            created_at: String::new(),
            updated_at: String::new(),
            sort_order: 0,
        };
        service.validate()?;
        Ok(service)
    }

    pub fn health_configured(&self) -> bool {
        self.health_check_type != "none"
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.trim().is_empty() || self.name.trim().len() > 120 {
            return Err("Name must contain 1 to 120 characters");
        }
        if self.executable.trim().is_empty() {
            return Err("Executable is required");
        }
        if self.executable.contains('\0')
            || self
                .arguments
                .iter()
                .any(|argument| argument.contains('\0'))
        {
            return Err("Executable and arguments cannot contain NUL");
        }
        if self.execution_mode != "direct" {
            return Err("Only direct executable mode is currently supported");
        }
        if self.depends_on.iter().any(|id| id == &self.id) {
            return Err("A service cannot depend on itself");
        }
        if !matches!(
            self.restart_policy.as_str(),
            "never" | "on_failure" | "always"
        ) {
            return Err("Restart policy must be never, on_failure, or always");
        }
        if !matches!(self.priority.as_str(), "low" | "normal" | "high") {
            return Err("Priority must be low, normal, or high");
        }
        if !(1..=1024).contains(&self.log_max_size_mb) {
            return Err("Max log size must be between 1 and 1024 MB");
        }
        if !matches!(self.health_check_type.as_str(), "none" | "tcp" | "http") {
            return Err("Health check type must be none, tcp, or http");
        }
        if !(500..=300_000).contains(&self.health_interval_ms) {
            return Err("Health interval must be between 500 and 300000 ms");
        }
        if !(100..=60_000).contains(&self.health_timeout_ms) {
            return Err("Health timeout must be between 100 and 60000 ms");
        }
        if self.health_start_period_ms > 600_000 {
            return Err("Health start period must be at most 600000 ms");
        }
        if let Some(status) = self.health_expected_status
            && !(100..=599).contains(&status)
        {
            return Err("Expected HTTP status must be between 100 and 599");
        }
        if self.health_check_type != "none" {
            let target = self
                .health_target
                .as_deref()
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .ok_or("Health check target is required")?;
            if self.health_check_type == "http" {
                if target.starts_with("https://") {
                    return Err("HTTPS health checks are not supported yet; use http://");
                }
                if !target.starts_with("http://") {
                    return Err("HTTP health checks must use an http:// URL");
                }
                let authority = target
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or_default();
                if authority.is_empty() {
                    return Err("HTTP health check URL must include a host");
                }
            } else {
                let valid_port =
                    |port: &str| port.parse::<u16>().map(|port| port > 0).unwrap_or(false);
                let valid = valid_port(target)
                    || target
                        .rsplit_once(':')
                        .is_some_and(|(host, port)| !host.is_empty() && valid_port(port));
                if !valid {
                    return Err("TCP health check target must be a port or host:port");
                }
            }
        }
        if let Some(directory) = &self.working_directory
            && !std::path::Path::new(directory).is_dir()
        {
            return Err("Working directory does not exist or is not a directory");
        }
        for variable in &self.environment {
            if variable.key.is_empty() || variable.key.contains('=') || variable.key.contains('\0')
            {
                return Err(
                    "Environment variable names must be non-empty and cannot contain '=' or NUL",
                );
            }
            if variable.value.contains('\0') {
                return Err("Environment variable values cannot contain NUL");
            }
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

fn default_restart_policy() -> String {
    "never".into()
}
fn default_restart_delay_ms() -> u64 {
    1000
}
fn default_max_restart_attempts() -> Option<u32> {
    Some(5)
}
fn default_priority() -> String {
    "normal".into()
}
fn default_stop_grace_period_ms() -> u64 {
    3000
}
fn default_log_max_size_mb() -> u64 {
    10
}
fn default_health_check_type() -> String {
    "none".into()
}
fn default_health_interval_ms() -> u64 {
    5000
}
fn default_health_timeout_ms() -> u64 {
    2000
}
fn default_health_start_period_ms() -> u64 {
    5000
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    pub service_id: String,
    pub status: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub started_at_ms: Option<u128>,
    pub cpu_usage_percent: Option<f32>,
    pub memory_usage_bytes: Option<u64>,
    pub metrics_sampled_at_ms: Option<u128>,
    /// `not_configured`, `starting`, `healthy`, or `unhealthy`.
    pub health: String,
    pub health_checked_at_ms: Option<u128>,
    pub health_latency_ms: Option<u64>,
    pub health_error: Option<String>,
    /// First local URL detected in this run's output, if any.
    pub detected_url: Option<String>,
    /// Milliseconds since the Unix epoch when a pending restart will fire.
    pub restart_at_ms: Option<u128>,
    /// One-based restart attempt currently scheduled or in progress.
    pub restart_attempt: Option<u32>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub service_id: String,
    pub stream: String,
    pub timestamp_ms: u128,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunHistory {
    pub id: String,
    pub service_id: String,
    pub started_at_ms: u128,
    pub ended_at_ms: u128,
    pub status: String,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ActiveProcessRecord {
    pub service_id: String,
    pub manager_session_id: String,
    pub pid: u32,
    pub process_group_id: u32,
    pub process_start_time_s: u64,
    pub executable_path: String,
}

#[allow(dead_code)]
pub type AppSettings = HashMap<String, serde_json::Value>;

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> CreateService {
        CreateService {
            name: "Local API".into(),
            icon_path: None,
            executable: "node".into(),
            arguments: vec!["server.js".into()],
            working_directory: None,
            environment: vec![],
            description: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
            log_max_size_mb: 10,
            health_check_type: "none".into(),
            health_target: None,
            health_interval_ms: 5000,
            health_timeout_ms: 2000,
            health_expected_status: None,
            health_start_period_ms: 5000,
            depends_on: vec![],
        }
    }

    #[test]
    fn accepts_secret_environment_values_and_rejects_shell_mode() {
        let mut secret = input();
        secret.environment.push(EnvironmentVariable {
            key: "TOKEN".into(),
            value: "private".into(),
            is_secret: true,
        });
        assert!(Service::from_create(secret).is_ok());

        let mut service = Service::from_create(input()).unwrap();
        service.execution_mode = "shell".into();
        assert!(
            service
                .validate()
                .unwrap_err()
                .contains("direct executable")
        );
    }

    #[test]
    fn rejects_invalid_directory_and_nul_arguments() {
        let mut directory = input();
        directory.working_directory = Some("/path/that/does/not/exist/processhub".into());
        assert!(Service::from_create(directory).is_err());

        let mut nul = input();
        nul.arguments.push("invalid\0argument".into());
        assert!(Service::from_create(nul).unwrap_err().contains("NUL"));
    }

    #[test]
    fn validates_health_check_targets() {
        let mut missing = input();
        missing.health_check_type = "tcp".into();
        assert!(
            Service::from_create(missing)
                .unwrap_err()
                .contains("target")
        );

        let mut invalid_tcp = input();
        invalid_tcp.health_check_type = "tcp".into();
        invalid_tcp.health_target = Some("not-a-port".into());
        assert!(
            Service::from_create(invalid_tcp)
                .unwrap_err()
                .contains("port or host:port")
        );

        let mut tcp = input();
        tcp.health_check_type = "tcp".into();
        tcp.health_target = Some("3000".into());
        assert!(Service::from_create(tcp).is_ok());

        let mut https = input();
        https.health_check_type = "http".into();
        https.health_target = Some("https://localhost:3000/health".into());
        assert!(
            Service::from_create(https)
                .unwrap_err()
                .contains("HTTPS health checks are not supported")
        );

        let mut http = input();
        http.health_check_type = "http".into();
        http.health_target = Some("http://localhost:3000/health".into());
        http.health_expected_status = Some(204);
        assert!(Service::from_create(http).is_ok());

        let mut bad_status = input();
        bad_status.health_check_type = "http".into();
        bad_status.health_target = Some("http://localhost:3000".into());
        bad_status.health_expected_status = Some(99);
        assert!(
            Service::from_create(bad_status)
                .unwrap_err()
                .contains("Expected HTTP status")
        );
    }
}
