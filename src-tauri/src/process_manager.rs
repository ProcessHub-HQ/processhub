use crate::models::{ActiveProcessRecord, LogEvent, RunHistory, RuntimeState, Service};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{mpsc, watch},
    time::timeout,
};

const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
const ROTATED_LOG_FILES: usize = 3;
const DEFAULT_LOG_RETENTION_DAYS: u64 = 14;

/// Resolves the user's full login-shell PATH on macOS/Linux.
///
/// When ProcessHub is launched as a bundled .app by double-clicking (or via
/// Login Items), macOS gives it a bare PATH of /usr/bin:/bin:/usr/sbin:/sbin.
/// That means user-installed tools like `mailpit`, `node`, `bun`, `brew`
/// binaries, etc. are not found.
///
/// We fix this by running the user's login shell once at startup with `-l -c
/// 'echo $PATH'` to get the same PATH the user sees in a terminal, then
/// storing it in the ProcessManager so every spawned service inherits it.
#[cfg(unix)]
async fn resolve_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let output = tokio::process::Command::new(&shell)
        .args(["-l", "-c", "echo $PATH"])
        .output()
        .await
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

#[derive(Clone, Default)]
pub struct ProcessManager {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    log_dir: Option<Arc<PathBuf>>,
    log_retention_days: Arc<AtomicU64>,
    /// Cached login-shell PATH, resolved once at startup on macOS/Linux.
    shell_path: Arc<Mutex<Option<String>>>,
}

struct Entry {
    state: RuntimeState,
    stop: watch::Sender<bool>,
    done: watch::Sender<bool>,
}

impl ProcessManager {
    pub fn with_log_dir(mut self, path: PathBuf) -> Self {
        self.log_dir = Some(Arc::new(path));
        if self.log_retention_days.load(Ordering::Relaxed) == 0 {
            self.set_log_retention_days(DEFAULT_LOG_RETENTION_DAYS);
        }
        self
    }

    pub fn with_log_retention_days(self, days: u64) -> Self {
        self.set_log_retention_days(days);
        self
    }

    pub fn set_log_retention_days(&self, days: u64) {
        self.log_retention_days
            .store(days.clamp(1, 90), Ordering::Relaxed);
    }

    /// Resolves and caches the login-shell PATH. Call once after construction.
    pub async fn init_shell_path(&self) {
        #[cfg(unix)]
        {
            let path = resolve_shell_path().await;
            if let Ok(mut guard) = self.shell_path.lock() {
                *guard = path;
            }
        }
    }

    pub async fn load_logs(&self, service_id: &str) -> Result<Vec<LogEvent>, String> {
        if service_id.is_empty()
            || !service_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
        {
            return Err("Invalid service id".into());
        }
        let Some(directory) = self.log_dir.as_deref() else {
            return Ok(Vec::new());
        };
        let mut events = std::collections::VecDeque::with_capacity(2_000);
        for index in (0..=ROTATED_LOG_FILES).rev() {
            let path = if index == 0 {
                directory.join(format!("{service_id}.log"))
            } else {
                rotated_log_path(directory, service_id, index)
            };
            let file = match fs::File::open(path).await {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.to_string()),
            };
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            loop {
                line.clear();
                if reader
                    .read_until(b'\n', &mut line)
                    .await
                    .map_err(|error| error.to_string())?
                    == 0
                {
                    break;
                }
                if let Ok(event) = serde_json::from_slice::<LogEvent>(&line)
                    && event.service_id == service_id
                {
                    if events.len() == 2_000 {
                        events.pop_front();
                    }
                    events.push_back(event);
                }
            }
        }
        Ok(events.into())
    }

    pub async fn start<R: Runtime>(
        &self,
        service: Service,
        app: AppHandle<R>,
    ) -> Result<(), String> {
        log::info!("event=service_start_requested service_id={}", service.id);
        if let Err(error) = service.validate() {
            let state = RuntimeState {
                service_id: service.id.clone(),
                status: "failed".into(),
                pid: None,
                exit_code: None,
                started_at_ms: None,
                cpu_usage_percent: None,
                memory_usage_bytes: None,
                metrics_sampled_at_ms: None,
                error: Some(error.to_string()),
            };
            self.set_state(state, &app)?;
            return Err(error.to_string());
        }
        if !service.enabled {
            return Err("This service is disabled".into());
        }
        {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Process registry lock poisoned")?;
            if entries.get(&service.id).is_some_and(|entry| {
                matches!(
                    entry.state.status.as_str(),
                    "starting" | "running" | "stopping"
                )
            }) {
                return Ok(());
            }
            let (stop, _) = watch::channel(false);
            entries.insert(
                service.id.clone(),
                Entry {
                    state: RuntimeState {
                        service_id: service.id.clone(),
                        status: "starting".into(),
                        pid: None,
                        exit_code: None,
                        started_at_ms: Some(now_ms()),
                        cpu_usage_percent: None,
                        memory_usage_bytes: None,
                        metrics_sampled_at_ms: None,
                        error: None,
                    },
                    stop,
                    done: watch::channel(false).0,
                },
            );
        }
        self.emit_state(&service.id, &app);
        let mut command = Command::new(&service.executable);
        command
            .args(&service.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(directory) = &service.working_directory {
            command.current_dir(directory);
        }
        // Inject the login-shell PATH so that bundled-.app launches find the
        // same user-installed tools (Homebrew, nvm, etc.) as a terminal would.
        // We merge: shell_path : existing_PATH, deduplicating as we go.
        #[cfg(unix)]
        {
            let shell_path = self.shell_path.lock().ok().and_then(|g| g.clone());
            if let Some(ref sp) = shell_path {
                let existing = std::env::var("PATH").unwrap_or_default();
                // Collect entries, shell_path first, then existing, skip dupes.
                let mut seen = std::collections::HashSet::new();
                let merged: Vec<&str> = sp
                    .split(':')
                    .chain(existing.split(':'))
                    .filter(|s| !s.is_empty() && seen.insert(*s))
                    .collect();
                command.env("PATH", merged.join(":"));
            }
        }
        command.envs(
            service
                .environment
                .iter()
                .map(|item| (&item.key, &item.value)),
        );
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                log::error!(
                    "event=service_spawn_failed service_id={} error={}",
                    service.id,
                    error
                );
                let state = RuntimeState {
                    service_id: service.id.clone(),
                    status: "failed".into(),
                    pid: None,
                    exit_code: None,
                    started_at_ms: None,
                    cpu_usage_percent: None,
                    memory_usage_bytes: None,
                    metrics_sampled_at_ms: None,
                    error: Some(error.to_string()),
                };
                self.set_state(state, &app)?;
                return Err(error.to_string());
            }
        };
        let pid = child
            .id()
            .ok_or_else(|| "Spawned process has no PID".to_string())?;
        if let Some(state) = app.try_state::<crate::AppState>() {
            let save_result = match active_process_record(&service.id, pid, &state.session_id).await
            {
                Ok(Some(record)) => state
                    .database
                    .lock()
                    .map_err(|_| "Database lock poisoned".to_string())
                    .and_then(|mut database| {
                        database
                            .save_active_process(&record)
                            .map_err(|error| error.to_string())
                    }),
                Ok(None) => Ok(()),
                Err(error) => Err(error),
            };
            if let Err(error) = save_result {
                terminate_tree(pid, true).await;
                let _ = child.wait().await;
                self.set_state(
                    RuntimeState {
                        service_id: service.id.clone(),
                        status: "failed".into(),
                        pid: Some(pid),
                        exit_code: None,
                        started_at_ms: None,
                        cpu_usage_percent: None,
                        memory_usage_bytes: None,
                        metrics_sampled_at_ms: None,
                        error: Some(error.clone()),
                    },
                    &app,
                )?;
                return Err(format!("Could not record the running process: {error}"));
            }
        }
        let started_at_ms = now_ms();
        let (stop, stop_rx) = watch::channel(false);
        let (done, _) = watch::channel(false);
        {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Process registry lock poisoned")?;
            entries.insert(
                service.id.clone(),
                Entry {
                    state: RuntimeState {
                        service_id: service.id.clone(),
                        status: "running".into(),
                        pid: Some(pid),
                        exit_code: None,
                        started_at_ms: Some(started_at_ms),
                        cpu_usage_percent: None,
                        memory_usage_bytes: None,
                        metrics_sampled_at_ms: None,
                        error: None,
                    },
                    stop,
                    done: done.clone(),
                },
            );
        }
        self.emit_state(&service.id, &app);
        log::info!(
            "event=service_started service_id={} pid={}",
            service.id,
            pid
        );

        let (log_tx, mut log_rx) = mpsc::channel::<LogEvent>(256);
        let service_id = service.id.clone();
        let log_app = app.clone();
        let log_dir = self.log_dir.clone();
        let log_retention_days = self.log_retention_days.load(Ordering::Relaxed);
        tokio::spawn(async move {
            if let Some(directory) = log_dir.as_deref()
                && let Err(error) = cleanup_expired_logs(directory, log_retention_days).await
            {
                log::warn!("event=service_log_cleanup_failed error={}", error);
            }
            while let Some(log) = log_rx.recv().await {
                if let Some(directory) = log_dir.as_deref()
                    && let Err(error) = persist_log_event(directory, &log).await
                {
                    log::warn!(
                        "event=service_log_write_failed service_id={} error={}",
                        log.service_id,
                        error
                    );
                }
                let _ = log_app.emit("service-log", log);
            }
        });
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, service.id.clone(), "stdout", log_tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, service.id.clone(), "stderr", log_tx.clone());
        }

        let metrics_manager = self.clone();
        let metrics_app = app.clone();
        let metrics_service_id = service.id.clone();
        let mut metrics_done = done.subscribe();
        tokio::spawn(async move {
            let mut system = match System::new() {
                Ok(system) => system,
                Err(error) => {
                    log::warn!(
                        "event=process_metrics_unavailable service_id={} error={}",
                        metrics_service_id,
                        error
                    );
                    return;
                }
            };
            let process_pid = Pid::from_u32(pid);
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut samples = 0_u8;
            loop {
                tokio::select! {
                    _ = metrics_done.changed() => break,
                    _ = interval.tick() => {
                        system.refresh_processes_specifics(
                            ProcessesToUpdate::Some(&[process_pid]),
                            true,
                            ProcessRefreshKind::nothing().with_cpu().with_memory(),
                        );
                        if let Some(process) = system.process(process_pid) {
                            samples = samples.saturating_add(1);
                            metrics_manager.set_metrics(
                                &metrics_service_id,
                                (samples > 1).then_some(process.cpu_usage()),
                                Some(process.memory()),
                                &metrics_app,
                            );
                        }
                    }
                }
            }
        });

        let manager = self.clone();
        tokio::spawn(async move {
            let mut stop_rx = stop_rx;
            let (exit_code, user_stopped, wait_error) = tokio::select! {
                result = child.wait() => match result {
                    Ok(status) => (status.code(), false, None),
                    Err(error) => (None, false, Some(error.to_string())),
                },
                changed = stop_rx.changed() => {
                    let should_stop = changed.is_ok() && *stop_rx.borrow();
                    if should_stop {
                        terminate_tree(pid, false).await;
                        match timeout(Duration::from_millis(service.stop_grace_period_ms), async {
                            let result = child.wait().await;
                            wait_for_process_tree(pid).await;
                            result
                        }).await {
                            Ok(result) => match result {
                                Ok(status) => (status.code(), true, None),
                                Err(error) => (None, true, Some(error.to_string())),
                            },
                            Err(_) => {
                                terminate_tree(pid, true).await;
                                let result = child.wait().await;
                                let _ = timeout(Duration::from_secs(5), wait_for_process_tree(pid)).await;
                                match result {
                                    Ok(status) => (status.code(), true, None),
                                    Err(error) => (None, true, Some(error.to_string())),
                                }
                            }
                        }
                    } else {
                        let result = child.wait().await;
                        match result {
                            Ok(status) => (status.code(), false, None),
                            Err(error) => (None, false, Some(error.to_string())),
                        }
                    }
                }
            };
            drop(log_tx);
            let status = if wait_error.is_some() {
                "failed"
            } else if user_stopped {
                "stopped"
            } else {
                "exited"
            };
            let run = RunHistory {
                id: uuid::Uuid::new_v4().to_string(),
                service_id: service_id.clone(),
                started_at_ms,
                ended_at_ms: now_ms(),
                status: status.into(),
                exit_code,
                error: wait_error.clone(),
            };
            if let Some(app_state) = app.try_state::<crate::AppState>() {
                match app_state.database.lock() {
                    Ok(mut database) => {
                        if let Err(error) = database.insert_run_history(&run) {
                            log::error!(
                                "event=run_history_write_failed service_id={} error={}",
                                service_id,
                                error
                            );
                        }
                        if let Err(error) = database.delete_active_process(&service_id) {
                            log::error!(
                                "event=active_process_clear_failed service_id={} error={}",
                                service_id,
                                error
                            );
                        }
                    }
                    Err(_) => log::error!(
                        "event=run_history_write_failed service_id={} error=database_lock_poisoned",
                        service_id
                    ),
                }
            }
            let _ = manager.finish_state(&service_id, status, exit_code, wait_error, &app);
            log::info!(
                "event=service_process_exited service_id={} status={} exit_code={:?}",
                service_id,
                status,
                exit_code
            );
            done.send_replace(true);
        });
        Ok(())
    }

    pub async fn stop<R: Runtime>(&self, id: &str, app: &AppHandle<R>) -> Result<(), String> {
        log::info!("event=service_stop_requested service_id={id}");
        let (stop, mut done) = {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Process registry lock poisoned")?;
            let Some(entry) = entries.get_mut(id) else {
                return Ok(());
            };
            if !matches!(
                entry.state.status.as_str(),
                "starting" | "running" | "stopping"
            ) {
                return Ok(());
            }
            entry.state.status = "stopping".into();
            (entry.stop.clone(), entry.done.subscribe())
        };
        self.emit_state(id, app);
        let _ = stop.send(true);
        timeout(Duration::from_secs(15), async {
            while !*done.borrow() {
                done.changed()
                    .await
                    .map_err(|_| "Process monitor stopped unexpectedly".to_string())?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "Timed out waiting for process shutdown".to_string())??;
        log::info!("event=service_stopped service_id={id}");
        Ok(())
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.entries
            .lock()
            .ok()
            .and_then(|entries| {
                entries.get(id).map(|entry| {
                    entry.state.status == "running"
                        || entry.state.status == "starting"
                        || entry.state.status == "stopping"
                })
            })
            .unwrap_or(false)
    }

    pub fn states(&self) -> Vec<RuntimeState> {
        self.entries
            .lock()
            .map(|entries| entries.values().map(|entry| entry.state.clone()).collect())
            .unwrap_or_default()
    }

    fn set_state<R: Runtime>(&self, state: RuntimeState, app: &AppHandle<R>) -> Result<(), String> {
        let id = state.service_id.clone();
        self.entries
            .lock()
            .map_err(|_| "Process registry lock poisoned".to_string())?
            .entry(id.clone())
            .and_modify(|entry| entry.state = state.clone())
            .or_insert_with(|| {
                let (stop, _) = watch::channel(false);
                Entry {
                    state,
                    stop,
                    done: watch::channel(false).0,
                }
            });
        self.emit_state(&id, app);
        Ok(())
    }

    fn set_metrics<R: Runtime>(
        &self,
        id: &str,
        cpu_usage_percent: Option<f32>,
        memory_usage_bytes: Option<u64>,
        app: &AppHandle<R>,
    ) {
        let updated = if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(id) {
                entry.state.cpu_usage_percent = cpu_usage_percent;
                entry.state.memory_usage_bytes = memory_usage_bytes;
                entry.state.metrics_sampled_at_ms = Some(now_ms());
                true
            } else {
                false
            }
        } else {
            false
        };
        if updated {
            self.emit_state(id, app);
        }
    }

    fn finish_state<R: Runtime>(
        &self,
        id: &str,
        status: &str,
        exit_code: Option<i32>,
        error: Option<String>,
        app: &AppHandle<R>,
    ) -> Result<(), String> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "Process registry lock poisoned".to_string())?;
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| "Process state not found".to_string())?;
        entry.state.status = status.into();
        entry.state.exit_code = exit_code;
        entry.state.error = error;
        drop(entries);
        self.emit_state(id, app);
        Ok(())
    }

    fn emit_state<R: Runtime>(&self, id: &str, app: &AppHandle<R>) {
        if let Ok(entries) = self.entries.lock()
            && let Some(entry) = entries.get(id)
        {
            let _ = app.emit("service-state", &entry.state);
        }
    }
}

async fn active_process_record(
    service_id: &str,
    pid: u32,
    manager_session_id: &str,
) -> Result<Option<ActiveProcessRecord>, String> {
    let process_pid = Pid::from_u32(pid);
    let mut system = System::new().map_err(|error| error.to_string())?;
    for _ in 0..10 {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[process_pid]),
            true,
            ProcessRefreshKind::everything(),
        );
        if system.process(process_pid).is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let Some(process) = system.process(process_pid) else {
        // The service exited before it could be recorded. There is nothing
        // left to recover, and the normal exit watcher still runs.
        return Ok(None);
    };
    let executable_path = process
        .exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();
    // The command is spawned with `process_group(0)`, so the child is its own
    // process-group leader. sysinfo's `group_id` is the user group, not the
    // process group, so the PID is the authoritative group id here.
    let process_group_id = pid;
    Ok(Some(ActiveProcessRecord {
        service_id: service_id.to_string(),
        manager_session_id: manager_session_id.to_string(),
        pid,
        process_group_id,
        process_start_time_s: process.start_time(),
        executable_path,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrphanedProcessRecovery {
    Stopped,
    AlreadyExited,
    Unverifiable,
}

/// Stops a process that was started by a previous ProcessHub session.
///
/// The record is only acted on when the live process still matches the recorded
/// PID, start time, and process group. If anything is ambiguous the process is
/// left untouched so recovery can never kill an unrelated process that reused a
/// PID.
#[cfg(unix)]
pub async fn recover_orphaned_process(
    record: &ActiveProcessRecord,
) -> Result<OrphanedProcessRecovery, String> {
    let Ok(group_id) = i32::try_from(record.process_group_id) else {
        return Ok(OrphanedProcessRecovery::Unverifiable);
    };
    // Managed processes are always process-group leaders, and we must never
    // signal our own process group.
    if record.pid <= 1
        || record.process_group_id <= 1
        || record.pid != record.process_group_id
        || group_id == unsafe { libc::getpgrp() }
    {
        return Ok(OrphanedProcessRecovery::Unverifiable);
    }

    let process_pid = Pid::from_u32(record.pid);
    let mut system = System::new().map_err(|error| error.to_string())?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[process_pid]),
        true,
        ProcessRefreshKind::everything(),
    );

    let group_alive = process_group_exists(group_id);
    match system.process(process_pid) {
        Some(process) => {
            let live_group = unsafe { libc::getpgid(record.pid as i32) };
            if process.start_time() != record.process_start_time_s || live_group != group_id {
                return Ok(OrphanedProcessRecovery::Unverifiable);
            }
        }
        None => {
            if !group_alive {
                return Ok(OrphanedProcessRecovery::AlreadyExited);
            }
            // The root exited but descendants are still in its group. Only stop
            // them when every remaining member started with or after the root,
            // which makes a reused process-group id implausible.
            if !group_is_plausible_orphan(&mut system, record) {
                return Ok(OrphanedProcessRecovery::Unverifiable);
            }
        }
    }

    signal_group(group_id, false);
    if !wait_for_orphan_group_to_exit(group_id, Duration::from_secs(3)).await {
        signal_group(group_id, true);
        if !wait_for_orphan_group_to_exit(group_id, Duration::from_secs(2)).await {
            return Err("Verified process group did not exit after SIGKILL".into());
        }
    }
    Ok(OrphanedProcessRecovery::Stopped)
}

#[cfg(windows)]
pub async fn recover_orphaned_process(
    record: &ActiveProcessRecord,
) -> Result<OrphanedProcessRecovery, String> {
    if record.pid <= 1 {
        return Ok(OrphanedProcessRecovery::Unverifiable);
    }
    let process_pid = Pid::from_u32(record.pid);
    let mut system = System::new().map_err(|error| error.to_string())?;
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[process_pid]),
        true,
        ProcessRefreshKind::everything(),
    );
    let Some(process) = system.process(process_pid) else {
        // Windows has no process group to inspect after the root exits. A
        // surviving descendant cannot be verified, so recovery accepts the
        // leak rather than risking taskkill /T on a reused PID.
        return Ok(OrphanedProcessRecovery::AlreadyExited);
    };
    if process.start_time() != record.process_start_time_s {
        return Ok(OrphanedProcessRecovery::Unverifiable);
    }
    terminate_tree(record.pid, true).await;
    if !wait_for_pid_to_exit(record.pid, Duration::from_secs(5)).await {
        return Err("Verified process tree did not exit after taskkill".into());
    }
    Ok(OrphanedProcessRecovery::Stopped)
}

#[cfg(not(any(unix, windows)))]
pub async fn recover_orphaned_process(
    _record: &ActiveProcessRecord,
) -> Result<OrphanedProcessRecovery, String> {
    Ok(OrphanedProcessRecovery::Unverifiable)
}

#[cfg(unix)]
fn process_group_exists(group_id: i32) -> bool {
    if unsafe { libc::kill(-group_id, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(unix)]
fn group_is_plausible_orphan(system: &mut System, record: &ActiveProcessRecord) -> bool {
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let Ok(group_id) = i32::try_from(record.process_group_id) else {
        return false;
    };
    let mut found_member = false;
    for process in system.processes().values() {
        let pid = process.pid().as_u32();
        if unsafe { libc::getpgid(pid as i32) } != group_id {
            continue;
        }
        found_member = true;
        // A process now owns the old leader PID, or a member predates the
        // recorded root. Either way the group id no longer identifies our tree.
        if pid == record.process_group_id || process.start_time() < record.process_start_time_s {
            return false;
        }
    }
    found_member
}

#[cfg(unix)]
fn signal_group(group_id: i32, force: bool) {
    unsafe {
        libc::kill(-group_id, if force { libc::SIGKILL } else { libc::SIGTERM });
    }
}

#[cfg(unix)]
async fn wait_for_orphan_group_to_exit(group_id: i32, duration: Duration) -> bool {
    timeout(duration, async {
        loop {
            if !process_group_exists(group_id) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .is_ok()
}

#[cfg(windows)]
async fn wait_for_pid_to_exit(pid: u32, duration: Duration) -> bool {
    timeout(duration, async {
        let process_pid = Pid::from_u32(pid);
        let Ok(mut system) = System::new() else {
            return;
        };
        loop {
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[process_pid]),
                true,
                ProcessRefreshKind::nothing(),
            );
            if system.process(process_pid).is_none() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .is_ok()
}

async fn persist_log_event(directory: &Path, event: &LogEvent) -> std::io::Result<()> {
    fs::create_dir_all(directory).await?;
    let path = directory.join(format!("{}.log", event.service_id));
    let mut line = serde_json::to_vec(event).map_err(std::io::Error::other)?;
    line.push(b'\n');
    let line_bytes = line.as_slice();
    let current_size = fs::metadata(&path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if current_size > 0 && current_size.saturating_add(line_bytes.len() as u64) > MAX_LOG_FILE_BYTES
    {
        for index in (1..=ROTATED_LOG_FILES).rev() {
            let source = if index == 1 {
                path.clone()
            } else {
                rotated_log_path(directory, &event.service_id, index - 1)
            };
            let destination = rotated_log_path(directory, &event.service_id, index);
            if index == ROTATED_LOG_FILES {
                let _ = fs::remove_file(&destination).await;
            }
            if fs::metadata(&source).await.is_ok() {
                fs::rename(source, destination).await?;
            }
        }
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line_bytes).await?;
    file.flush().await
}

fn rotated_log_path(directory: &Path, service_id: &str, index: usize) -> PathBuf {
    directory.join(format!("{service_id}.log.{index}"))
}

async fn cleanup_expired_logs(directory: &Path, retention_days: u64) -> std::io::Result<()> {
    let mut entries = match fs::read_dir(directory).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let retention = Duration::from_secs(retention_days * 24 * 60 * 60);
    let now = SystemTime::now();
    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.metadata().await?;
        let expired = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > retention);
        if expired && metadata.is_file() {
            fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

fn spawn_reader<R>(
    mut reader: R,
    service_id: String,
    stream: &'static str,
    tx: mpsc::Sender<LogEvent>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut buffer = [0_u8; 4096];
        let mut pending = Vec::with_capacity(16 * 1024);
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    pending.extend_from_slice(&buffer[..size]);
                    loop {
                        let line_end = pending
                            .iter()
                            .position(|byte| *byte == b'\n')
                            .map(|index| index + 1);
                        let cut = match line_end {
                            Some(end) if end <= 16 * 1024 => Some(end),
                            Some(_) | None if pending.len() >= 16 * 1024 => {
                                Some(utf8_prefix_len(&pending, 16 * 1024))
                            }
                            _ => None,
                        };
                        let Some(cut) = cut.filter(|cut| *cut > 0) else {
                            break;
                        };
                        let chunk: Vec<u8> = pending.drain(..cut).collect();
                        if send_log_chunk(&tx, &service_id, stream, &chunk)
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        }
        if !pending.is_empty() {
            let _ = send_log_chunk(&tx, &service_id, stream, &pending).await;
        }
    });
}

async fn send_log_chunk(
    tx: &mpsc::Sender<LogEvent>,
    service_id: &str,
    stream: &str,
    bytes: &[u8],
) -> Result<(), ()> {
    tx.send(LogEvent {
        service_id: service_id.to_string(),
        stream: stream.into(),
        timestamp_ms: now_ms(),
        text: String::from_utf8_lossy(bytes).into_owned(),
    })
    .await
    .map_err(|_| ())
}

fn utf8_prefix_len(bytes: &[u8], limit: usize) -> usize {
    match std::str::from_utf8(&bytes[..limit]) {
        Ok(_) => limit,
        Err(error) if error.error_len().is_none() && error.valid_up_to() > 0 => error.valid_up_to(),
        Err(_) => limit,
    }
}

#[cfg(unix)]
async fn terminate_tree(pid: u32, force: bool) {
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    // Each managed command is made a process-group leader, so a negative PID targets its descendants too.
    unsafe {
        libc::kill(-(pid as i32), signal);
    }
}

#[cfg(unix)]
async fn wait_for_process_tree(pid: u32) {
    while unsafe { libc::kill(-(pid as i32), 0) } == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(windows)]
async fn wait_for_process_tree(_pid: u32) {}

#[cfg(windows)]
async fn terminate_tree(pid: u32, force: bool) {
    let mut command = Command::new("taskkill");
    let pid = pid.to_string();
    command.args(["/PID", pid.as_str(), "/T"]);
    if force {
        command.arg("/F");
    }
    let _ = command.status().await;
}

#[cfg(not(any(unix, windows)))]
async fn terminate_tree(_pid: u32, _force: bool) {}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EnvironmentVariable;
    use tokio::io::AsyncWriteExt;

    fn test_service(executable: &str, arguments: &[&str]) -> Service {
        Service {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Process test".into(),
            description: None,
            icon_path: None,
            group_id: None,
            execution_mode: "direct".into(),
            executable: executable.into(),
            arguments: arguments.iter().map(|arg| (*arg).into()).collect(),
            shell_command: None,
            working_directory: None,
            environment: vec![],
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 100,
            max_restart_attempts: Some(1),
            priority: "normal".into(),
            stop_grace_period_ms: 500,
            created_at: String::new(),
            updated_at: String::new(),
            sort_order: 0,
        }
    }

    async fn wait_for_status(manager: &ProcessManager, id: &str, status: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if manager
                    .states()
                    .iter()
                    .any(|state| state.service_id == id && state.status == status)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("process state transition");
    }

    #[test]
    fn log_chunks_keep_valid_utf8_boundaries() {
        let text = "a".repeat(16 * 1024 - 2) + "🌱tail";
        let bytes = text.as_bytes();
        let prefix = utf8_prefix_len(bytes, 16 * 1024);
        assert!(std::str::from_utf8(&bytes[..prefix]).is_ok());
        assert!(prefix <= 16 * 1024);
    }

    #[tokio::test]
    async fn reader_splits_large_partial_output_without_losing_bytes() {
        let (mut writer, reader) = tokio::io::duplex(8 * 1024);
        let (tx, mut rx) = mpsc::channel(256);
        spawn_reader(reader, "test-service".into(), "stdout", tx);

        let input = format!(
            "partial line\n{}{}\nend",
            "line\n".repeat(10_000),
            "x".repeat(40 * 1024)
        );
        let expected = input.clone();
        let writer_task = tokio::spawn(async move {
            writer
                .write_all(input.as_bytes())
                .await
                .expect("write fixture");
            drop(writer);
        });

        let mut output = String::new();
        let mut chunks = 0;
        while let Some(event) = rx.recv().await {
            assert_eq!(event.service_id, "test-service");
            assert_eq!(event.stream, "stdout");
            assert!(event.text.len() <= 16 * 1024);
            output.push_str(&event.text);
            chunks += 1;
        }
        writer_task.await.expect("writer task");
        assert!(chunks > 1);
        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn persisted_logs_rotate_at_the_configured_file_limit() {
        let directory =
            std::env::temp_dir().join(format!("processhub-logs-{}", uuid::Uuid::new_v4()));
        let service_id = uuid::Uuid::new_v4().to_string();
        let text = "x".repeat(6 * 1024 * 1024);
        for _ in 0..2 {
            persist_log_event(
                &directory,
                &LogEvent {
                    service_id: service_id.clone(),
                    stream: "stdout".into(),
                    timestamp_ms: now_ms(),
                    text: text.clone(),
                },
            )
            .await
            .expect("persist large log event");
        }
        assert!(directory.join(format!("{service_id}.log.1")).exists());
        let active_size = fs::metadata(directory.join(format!("{service_id}.log")))
            .await
            .expect("active log metadata")
            .len();
        assert!(active_size < MAX_LOG_FILE_BYTES);
        fs::remove_dir_all(directory)
            .await
            .expect("remove log fixture");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn manager_starts_stops_and_reconciles_runtime_state() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let service = test_service("/bin/sh", &["-c", "exec sleep 30"]);
        let id = service.id.clone();

        manager
            .start(service.clone(), handle.clone())
            .await
            .expect("start process");
        assert!(manager.is_running(&id));
        let original_pid = manager
            .states()
            .iter()
            .find(|state| state.service_id == id)
            .unwrap()
            .pid;
        manager
            .start(service.clone(), handle.clone())
            .await
            .expect("idempotent start");
        assert_eq!(
            manager
                .states()
                .iter()
                .find(|state| state.service_id == id)
                .unwrap()
                .pid,
            original_pid
        );
        let (first_stop, second_stop) =
            tokio::join!(manager.stop(&id, &handle), manager.stop(&id, &handle));
        first_stop.expect("stop process");
        second_stop.expect("concurrent idempotent stop");
        wait_for_status(&manager, &id, "stopped").await;
        manager.stop(&id, &handle).await.expect("idempotent stop");
        manager
            .start(service, handle.clone())
            .await
            .expect("restart stopped process");
        assert!(manager.is_running(&id));
        manager
            .stop(&id, &handle)
            .await
            .expect("stop restarted process");
        wait_for_status(&manager, &id, "stopped").await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn manager_reports_nonzero_exit_and_missing_executable() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let exited = test_service("/bin/sh", &["-c", "exit 17"]);
        let exited_id = exited.id.clone();
        manager
            .start(exited, handle.clone())
            .await
            .expect("start short process");
        wait_for_status(&manager, &exited_id, "exited").await;
        assert_eq!(
            manager
                .states()
                .iter()
                .find(|state| state.service_id == exited_id)
                .unwrap()
                .exit_code,
            Some(17)
        );

        let missing = test_service("/definitely/not/a/processhub-command", &[]);
        let missing_id = missing.id.clone();
        assert!(manager.start(missing, handle).await.is_err());
        assert_eq!(
            manager
                .states()
                .iter()
                .find(|state| state.service_id == missing_id)
                .unwrap()
                .status,
            "failed"
        );
    }

    #[cfg(unix)]
    fn spawn_in_own_group(script: &str) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("/bin/sh");
        command.arg("-c").arg(script).process_group(0);
        command.spawn().expect("spawn fixture process")
    }

    #[cfg(unix)]
    async fn record_running_process(pid: u32) -> ActiveProcessRecord {
        active_process_record("service", pid, "previous-session")
            .await
            .expect("inspect fixture process")
            .expect("fixture process is running")
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn orphan_recovery_stops_a_verified_process_group() {
        let mut child = spawn_in_own_group("exec sleep 30");
        let pid = child.id();
        let record = record_running_process(pid).await;
        let waiter = tokio::spawn(async move { child.wait() });

        let outcome = recover_orphaned_process(&record)
            .await
            .expect("recover orphan");
        assert_eq!(outcome, OrphanedProcessRecovery::Stopped);
        let status = timeout(Duration::from_secs(5), waiter)
            .await
            .expect("fixture exits")
            .expect("join waiter")
            .expect("wait fixture");
        assert_eq!(status.code(), None, "SIGTERM should stop the fixture");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn orphan_recovery_leaves_a_mismatched_process_untouched() {
        let mut child = spawn_in_own_group("exec sleep 30");
        let pid = child.id();
        let mut record = record_running_process(pid).await;
        record.process_start_time_s = record.process_start_time_s.saturating_add(1);
        let waiter = tokio::spawn(async move { child.wait() });

        let outcome = recover_orphaned_process(&record)
            .await
            .expect("inspect orphan");
        assert_eq!(outcome, OrphanedProcessRecovery::Unverifiable);
        assert_eq!(unsafe { libc::kill(pid as i32, 0) }, 0);

        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
        timeout(Duration::from_secs(5), waiter)
            .await
            .expect("fixture exits")
            .expect("join waiter")
            .expect("wait fixture");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn orphan_recovery_stops_children_after_the_root_exits() {
        let marker =
            std::env::temp_dir().join(format!("processhub-orphan-{}.txt", uuid::Uuid::new_v4()));
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("sleep 30 & echo $! > \"$PROCESSHUB_CHILD_PID\"; sleep 0.5; exit 0")
            .env("PROCESSHUB_CHILD_PID", &marker)
            .process_group(0);
        let mut child = command.spawn().expect("spawn fixture process");
        let pid = child.id();
        let record = record_running_process(pid).await;
        child.wait().expect("wait for fixture root");
        let child_pid = timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(text) = std::fs::read_to_string(&marker)
                    && let Ok(pid) = text.trim().parse::<u32>()
                {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("read descendant pid");

        let outcome = recover_orphaned_process(&record)
            .await
            .expect("recover orphaned group");
        assert_eq!(outcome, OrphanedProcessRecovery::Stopped);
        let exited = timeout(Duration::from_secs(5), async {
            while unsafe { libc::kill(child_pid as i32, 0) } == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .is_ok();
        let _ = std::fs::remove_file(&marker);
        assert!(exited, "orphaned descendant survived recovery");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn orphan_recovery_rejects_unsafe_records() {
        let mut record = ActiveProcessRecord {
            service_id: "service".into(),
            manager_session_id: "previous-session".into(),
            pid: 1,
            process_group_id: 1,
            process_start_time_s: 1,
            executable_path: "/bin/sh".into(),
        };
        assert_eq!(
            recover_orphaned_process(&record)
                .await
                .expect("inspect unsafe record"),
            OrphanedProcessRecovery::Unverifiable
        );
        record.pid = 42;
        record.process_group_id = 43;
        assert_eq!(
            recover_orphaned_process(&record)
                .await
                .expect("inspect mismatched group"),
            OrphanedProcessRecovery::Unverifiable
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stopping_a_service_clears_its_active_process_record() {
        let app = tauri::test::mock_app();
        let service = test_service("/bin/sh", &["-c", "exec sleep 30"]);
        let id = service.id.clone();
        let mut database = crate::database::Database::open(":memory:").expect("open database");
        database.insert_service(&service).expect("save service");
        app.manage(crate::AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
        });

        let manager = app.state::<crate::AppState>().processes.clone();
        manager
            .start(service, app.handle().clone())
            .await
            .expect("start process");
        assert!(
            app.state::<crate::AppState>()
                .database
                .lock()
                .expect("lock database")
                .get_active_process(&id)
                .expect("load active process")
                .is_some()
        );

        manager.stop(&id, app.handle()).await.expect("stop process");
        assert!(
            app.state::<crate::AppState>()
                .database
                .lock()
                .expect("lock database")
                .get_active_process(&id)
                .expect("load active process")
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn manager_persists_completed_run_history() {
        let app = tauri::test::mock_app();
        let service = test_service("/bin/sh", &["-c", "exit 7"]);
        let id = service.id.clone();
        let mut database = crate::database::Database::open(":memory:").expect("open database");
        database.insert_service(&service).expect("save service");
        app.manage(crate::AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
        });

        let manager = app.state::<crate::AppState>().processes.clone();
        manager
            .start(service, app.handle().clone())
            .await
            .expect("start process");
        wait_for_status(&manager, &id, "exited").await;

        let state = app.state::<crate::AppState>();
        let database = state.database.lock().expect("lock database");
        let history = database.list_run_history(&id).expect("load run history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "exited");
        assert_eq!(history[0].exit_code, Some(7));
        assert!(
            database
                .get_active_process(&id)
                .expect("load active process")
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn manager_applies_working_directory_and_environment() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service(
            "/bin/sh",
            &[
                "-c",
                "test \"$PROCESSHUB_TEST_VALUE\" = yes && test \"$PWD\" = \"$EXPECTED_WORKDIR\"",
            ],
        );
        let directory = std::env::current_dir().expect("current directory");
        service.working_directory = Some(directory.to_string_lossy().into_owned());
        service.environment = vec![
            EnvironmentVariable {
                key: "PROCESSHUB_TEST_VALUE".into(),
                value: "yes".into(),
                is_secret: false,
            },
            EnvironmentVariable {
                key: "EXPECTED_WORKDIR".into(),
                value: directory.to_string_lossy().into_owned(),
                is_secret: false,
            },
        ];
        let id = service.id.clone();
        manager
            .start(service, handle)
            .await
            .expect("start configured process");
        wait_for_status(&manager, &id, "exited").await;
        assert_eq!(
            manager
                .states()
                .iter()
                .find(|state| state.service_id == id)
                .unwrap()
                .exit_code,
            Some(0)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn manager_terminates_a_grandchild_in_the_process_group() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let marker =
            std::env::temp_dir().join(format!("processhub-child-{}.txt", uuid::Uuid::new_v4()));
        let script = "(sleep 0.4; echo survived > \"$PROCESSHUB_CHILD_MARKER\") & wait";
        let mut service = test_service("/bin/sh", &["-c", script]);
        service.environment.push(EnvironmentVariable {
            key: "PROCESSHUB_CHILD_MARKER".into(),
            value: marker.to_string_lossy().into_owned(),
            is_secret: false,
        });
        let id = service.id.clone();
        manager
            .start(service, handle.clone())
            .await
            .expect("start process tree");

        manager.stop(&id, &handle).await.expect("stop process tree");
        wait_for_status(&manager, &id, "stopped").await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let child_survived = marker.exists();
        let _ = std::fs::remove_file(marker);
        assert!(
            !child_survived,
            "grandchild survived process-group shutdown"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn manager_forces_shutdown_after_grace_period() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service(
            "/bin/sh",
            &["-c", "trap '' TERM; while :; do sleep 1; done"],
        );
        service.stop_grace_period_ms = 100;
        let id = service.id.clone();
        manager
            .start(service, handle.clone())
            .await
            .expect("start signal-resistant process");

        tokio::time::timeout(Duration::from_secs(4), manager.stop(&id, &handle))
            .await
            .expect("force shutdown completes")
            .expect("stop process");
        wait_for_status(&manager, &id, "stopped").await;
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn manager_terminates_a_grandchild_on_windows() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let pid_file =
            std::env::temp_dir().join(format!("processhub-child-{}.pid", uuid::Uuid::new_v4()));
        let script = "$child = Start-Process -FilePath $env:ComSpec -ArgumentList '/c','ping -n 30 127.0.0.1' -PassThru; [IO.File]::WriteAllText($env:PROCESSHUB_CHILD_PID_FILE, [string]$child.Id); Wait-Process -Id $child.Id";
        let mut service = test_service(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", script],
        );
        service.environment.push(EnvironmentVariable {
            key: "PROCESSHUB_CHILD_PID_FILE".into(),
            value: pid_file.to_string_lossy().into_owned(),
            is_secret: false,
        });
        let id = service.id.clone();
        manager
            .start(service, handle.clone())
            .await
            .expect("start process tree");
        let child_pid = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Ok(pid) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = pid.trim().parse::<u32>() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("grandchild pid fixture");

        manager.stop(&id, &handle).await.expect("stop process tree");
        wait_for_status(&manager, &id, "stopped").await;
        let probe = format!(
            "if (Get-Process -Id {child_pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"
        );
        let still_running = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &probe])
            .status()
            .expect("query grandchild process")
            .success();
        let _ = std::fs::remove_file(pid_file);
        assert!(!still_running, "grandchild survived taskkill /T shutdown");
    }
}
