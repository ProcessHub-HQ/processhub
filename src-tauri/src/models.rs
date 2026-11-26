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
    pub group_id: Option<String>,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_path: Option<String>,
    pub group_id: Option<String>,
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
            group_id: input.group_id,
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
            created_at: String::new(),
            updated_at: String::new(),
            sort_order: 0,
        };
        service.validate()?;
        Ok(service)
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
        if !matches!(
            self.restart_policy.as_str(),
            "never" | "on_failure" | "always"
        ) {
            return Err("Restart policy must be never, on_failure, or always");
        }
        if !matches!(self.priority.as_str(), "low" | "normal" | "high") {
            return Err("Priority must be low, normal, or high");
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
            if variable.is_secret {
                return Err("Secret values cannot be stored until secure storage is implemented");
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
            group_id: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
        }
    }

    #[test]
    fn rejects_secret_environment_values_and_shell_mode() {
        let mut secret = input();
        secret.environment.push(EnvironmentVariable {
            key: "TOKEN".into(),
            value: "private".into(),
            is_secret: true,
        });
        assert!(
            Service::from_create(secret)
                .unwrap_err()
                .contains("secure storage")
        );

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
}
