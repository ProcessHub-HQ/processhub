//! Read-only snapshot of the current user's system processes.
//!
//! One [`System`] is kept alive so sysinfo can derive CPU usage from the delta
//! between refreshes. The first snapshot refreshes twice because a freshly
//! created [`System`] has no previous CPU sample and every process would
//! otherwise report 0%.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::error_copy::friendly_error;

pub(crate) const LOAD_SYSTEM_PROCESSES_FALLBACK: &str =
    "The process list couldn't be loaded. Try again.";

const FIRST_SAMPLE_INTERVAL: Duration = Duration::from_millis(200);

static SYSTEM: Mutex<Option<System>> = Mutex::new(None);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProcess {
    pid: u32,
    name: String,
    path: Option<String>,
    cpu_percent: f64,
    memory_bytes: u64,
    parent_pid: Option<u32>,
    parent_name: Option<String>,
    run_time_seconds: u64,
}

fn refresh(system: &mut System) {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_user(UpdateKind::Always),
    );
}

pub fn list_system_processes() -> Result<Vec<SystemProcess>, String> {
    snapshot(FIRST_SAMPLE_INTERVAL)
}

fn snapshot(first_sample_interval: Duration) -> Result<Vec<SystemProcess>, String> {
    let mut guard = SYSTEM.lock().map_err(|error| {
        let raw = error.to_string();
        log::warn!("event=system_processes_lock_failed error={raw}");
        friendly_error(&raw, LOAD_SYSTEM_PROCESSES_FALLBACK)
    })?;
    if guard.is_none() {
        let mut system = System::new().map_err(|error| {
            let raw = error.to_string();
            log::warn!("event=system_processes_unavailable error={raw}");
            friendly_error(&raw, LOAD_SYSTEM_PROCESSES_FALLBACK)
        })?;
        refresh(&mut system);
        std::thread::sleep(first_sample_interval);
        refresh(&mut system);
        *guard = Some(system);
    } else if let Some(system) = guard.as_mut() {
        refresh(system);
    }
    let Some(system) = guard.as_ref() else {
        return Err(LOAD_SYSTEM_PROCESSES_FALLBACK.to_string());
    };

    // Built before filtering so a parent owned by another user (launchd, for
    // example) can still be named.
    let names = system
        .processes()
        .values()
        .map(|process| {
            (
                process.pid().as_u32(),
                process.name().to_string_lossy().into_owned(),
            )
        })
        .collect::<HashMap<_, _>>();

    let current_user = system
        .process(Pid::from_u32(std::process::id()))
        .and_then(|process| process.user_id())
        .cloned();

    let mut processes = system
        .processes()
        .values()
        .filter(|process| match &current_user {
            Some(user) => process.user_id() == Some(user),
            None => true,
        })
        .map(|process| {
            let parent_pid = process.parent().map(Pid::as_u32);
            SystemProcess {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                path: process
                    .exe()
                    .map(|path| path.to_string_lossy().into_owned()),
                cpu_percent: process.cpu_usage() as f64,
                memory_bytes: process.memory(),
                parent_pid,
                parent_name: parent_pid.and_then(|pid| names.get(&pid).cloned()),
                run_time_seconds: process.run_time(),
            }
        })
        .collect::<Vec<_>>();

    processes.sort_by(|left, right| {
        right
            .cpu_percent
            .partial_cmp(&left.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(processes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_includes_the_current_process() {
        let processes = snapshot(Duration::ZERO).expect("list system processes");
        let current_pid = std::process::id();
        assert!(
            processes.iter().any(|process| process.pid == current_pid),
            "current process {current_pid} is missing from the snapshot",
        );
    }
}
