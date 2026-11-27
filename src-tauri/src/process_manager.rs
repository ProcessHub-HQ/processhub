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
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufWriter},
    process::Command,
    sync::{mpsc, watch},
    time::{MissedTickBehavior, timeout},
};

#[cfg(test)]
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub(crate) const ROTATED_LOG_FILES: usize = 3;
const DEFAULT_LOG_RETENTION_DAYS: u64 = 14;
/// A run that stays up this long resets the restart attempt counter.
const RESTART_STABILITY_MS: u128 = 60_000;
/// Exponential backoff is capped so repeated failures do not hammer the host.
const RESTART_MAX_DELAY_MS: u64 = 30_000;
/// Live output is coalesced before it reaches the disk and the UI.
const LOG_FLUSH_INTERVAL_MS: u64 = 100;
const LOG_BATCH_MAX_EVENTS: usize = 256;
const LOG_BATCH_MAX_BYTES: usize = 64 * 1024;

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
    /// Consecutive automatic restart attempts per service.
    restart_attempts: Arc<Mutex<HashMap<String, u32>>>,
    /// Monotonic run identifier. A scheduled restart only proceeds while its
    /// generation is still the active one, which makes manual starts and stops
    /// cancel pending restarts without shared timers.
    next_generation: Arc<AtomicU64>,
    log_dir: Option<Arc<PathBuf>>,
    log_retention_days: Arc<AtomicU64>,
    /// Cached login-shell PATH, resolved once at startup on macOS/Linux.
    #[cfg(unix)]
    shell_path: Arc<Mutex<Option<String>>>,
}

struct Entry {
    state: RuntimeState,
    stop: watch::Sender<bool>,
    done: watch::Sender<bool>,
    generation: u64,
}

struct RunOutcome {
    status: String,
    restart_delay_ms: Option<u64>,
}

fn restart_backoff_ms(base_ms: u64, attempt: u32) -> u64 {
    let base = base_ms.max(100);
    let exponent = attempt.saturating_sub(1).min(10);
    let cap = RESTART_MAX_DELAY_MS.max(base);
    base.saturating_mul(1_u64 << exponent).min(cap)
}

fn initial_health(service: &Service) -> &'static str {
    if service.health_configured() {
        "starting"
    } else {
        "not_configured"
    }
}

fn unavailable_health(service: &Service) -> &'static str {
    if service.health_configured() {
        "unhealthy"
    } else {
        "not_configured"
    }
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
        let limit = 2_000;
        // Newest file first, then fill from older rotations only when needed.
        let mut events: Vec<LogEvent> = Vec::with_capacity(limit);
        for index in 0..=ROTATED_LOG_FILES {
            let remaining = limit.saturating_sub(events.len());
            if remaining == 0 {
                break;
            }
            let path = if index == 0 {
                directory.join(format!("{service_id}.log"))
            } else {
                rotated_log_path(directory, service_id, index)
            };
            let mut older = tail_log_events(&path, service_id, remaining).await?;
            if older.is_empty() {
                continue;
            }
            older.append(&mut events);
            events = older;
        }
        Ok(events)
    }

    pub async fn start<R: Runtime>(
        &self,
        service: Service,
        app: AppHandle<R>,
    ) -> Result<(), String> {
        self.start_internal(service, app, true, None).await
    }

    /// Starts a service. `reset_attempts` is true for user-initiated starts and
    /// false when the restart scheduler relaunches a service after a failure.
    /// `expected_generation` makes relaunches abort if the entry changed while
    /// the backoff timer was running.
    async fn start_internal<R: Runtime>(
        &self,
        service: Service,
        app: AppHandle<R>,
        reset_attempts: bool,
        expected_generation: Option<u64>,
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
                health: unavailable_health(&service).into(),
                health_checked_at_ms: None,
                health_latency_ms: None,
                health_error: None,
                detected_url: None,
                restart_at_ms: None,
                restart_attempt: None,
                error: Some(error.to_string()),
            };
            self.set_state(state, &app)?;
            return Err(error.to_string());
        }
        if !service.enabled {
            return Err("This service is disabled".into());
        }
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Process registry lock poisoned")?;
            if let Some(expected) = expected_generation {
                let still_waiting = entries.get(&service.id).is_some_and(|entry| {
                    entry.generation == expected && entry.state.status == "waiting_to_restart"
                });
                if !still_waiting {
                    return Ok(());
                }
            } else if entries.get(&service.id).is_some_and(|entry| {
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
                        health: initial_health(&service).into(),
                        health_checked_at_ms: None,
                        health_latency_ms: None,
                        health_error: None,
                        detected_url: None,
                        restart_at_ms: None,
                        restart_attempt: None,
                        error: None,
                    },
                    stop,
                    done: watch::channel(false).0,
                    generation,
                },
            );
        }
        if reset_attempts && let Ok(mut attempts) = self.restart_attempts.lock() {
            attempts.remove(&service.id);
        }
        self.emit_lifecycle_state(&service.id, &app);
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
        let secret_keys = service
            .environment
            .iter()
            .filter(|variable| variable.is_secret)
            .map(|variable| variable.key.clone())
            .collect::<Vec<_>>();
        let resolved_secrets = if secret_keys.is_empty() {
            Ok(Vec::new())
        } else {
            let service_id = service.id.clone();
            tokio::task::spawn_blocking(move || {
                let mut values = Vec::with_capacity(secret_keys.len());
                for key in secret_keys {
                    let value = crate::read_secret(&service_id, &key)?.ok_or_else(|| {
                        format!("Secret {key} is not set. Edit the service to set it.")
                    })?;
                    values.push((key, value));
                }
                Ok::<Vec<(String, String)>, String>(values)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result)
        };
        let resolved_secrets = match resolved_secrets {
            Ok(values) => values,
            Err(error) => {
                self.set_state(
                    RuntimeState {
                        service_id: service.id.clone(),
                        status: "failed".into(),
                        pid: None,
                        exit_code: None,
                        started_at_ms: None,
                        cpu_usage_percent: None,
                        memory_usage_bytes: None,
                        metrics_sampled_at_ms: None,
                        health: unavailable_health(&service).into(),
                        health_checked_at_ms: None,
                        health_latency_ms: None,
                        health_error: None,
                        detected_url: None,
                        restart_at_ms: None,
                        restart_attempt: None,
                        error: Some(error.clone()),
                    },
                    &app,
                )?;
                return Err(error);
            }
        };
        command.envs(
            service
                .environment
                .iter()
                .filter(|variable| !variable.is_secret)
                .map(|item| (&item.key, &item.value)),
        );
        let mut redactions = Vec::new();
        for (key, value) in &resolved_secrets {
            command.env(key, value);
            if !value.is_empty() {
                redactions.push(value.clone());
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        #[cfg(windows)]
        configure_windows_no_window(&mut command);

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
                    health: unavailable_health(&service).into(),
                    health_checked_at_ms: None,
                    health_latency_ms: None,
                    health_error: None,
                    detected_url: None,
                    restart_at_ms: None,
                    restart_attempt: None,
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
                        health: unavailable_health(&service).into(),
                        health_checked_at_ms: None,
                        health_latency_ms: None,
                        health_error: None,
                        detected_url: None,
                        restart_at_ms: None,
                        restart_attempt: None,
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
                        health: initial_health(&service).into(),
                        health_checked_at_ms: None,
                        health_latency_ms: None,
                        health_error: None,
                        detected_url: None,
                        restart_at_ms: None,
                        restart_attempt: None,
                        error: None,
                    },
                    stop,
                    done: done.clone(),
                    generation,
                },
            );
        }
        self.emit_lifecycle_state(&service.id, &app);
        log::info!(
            "event=service_started service_id={} pid={}",
            service.id,
            pid
        );

        let (log_tx, mut log_rx) = mpsc::channel::<LogEvent>(1024);
        let service_id = service.id.clone();
        let log_app = app.clone();
        let log_dir = self.log_dir.clone();
        let log_retention_days = self.log_retention_days.load(Ordering::Relaxed);
        let max_log_bytes = service.log_max_size_mb.clamp(1, 1024) * 1024 * 1024;
        let writer_service_id = service_id.clone();
        let log_service_id = service_id.clone();
        let log_manager = self.clone();
        let log_redactions = redactions;
        tokio::spawn(async move {
            if let Some(directory) = log_dir.as_deref()
                && let Err(error) = cleanup_expired_logs(directory, log_retention_days).await
            {
                log::warn!("event=service_log_cleanup_failed error={}", error);
            }
            let mut writer = log_dir.as_deref().map(|directory| {
                ServiceLogWriter::new(directory.to_path_buf(), writer_service_id, max_log_bytes)
            });
            let mut batch = Vec::with_capacity(LOG_BATCH_MAX_EVENTS);
            let mut batch_bytes = 0_usize;
            let mut ticker = tokio::time::interval(Duration::from_millis(LOG_FLUSH_INTERVAL_MS));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
            // The first tick is immediate; consume it so an empty batch is not flushed.
            ticker.tick().await;
            loop {
                tokio::select! {
                    incoming = log_rx.recv() => {
                        match incoming {
                            Some(mut event) => {
                                event.text =
                                    redact_secrets(&event.text, &log_redactions);
                                if let Some(url) = detect_service_url(&event.text) {
                                    log_manager.set_detected_url(
                                        &log_service_id,
                                        generation,
                                        url,
                                        &log_app,
                                    );
                                }
                                batch_bytes = batch_bytes.saturating_add(event.text.len() + 64);
                                batch.push(event);
                                if batch.len() >= LOG_BATCH_MAX_EVENTS
                                    || batch_bytes >= LOG_BATCH_MAX_BYTES
                                {
                                    flush_log_batch(writer.as_mut(), &mut batch, &mut batch_bytes, &log_app).await;
                                }
                            }
                            None => break,
                        }
                    }
                    _ = ticker.tick() => {
                        if !batch.is_empty() {
                            flush_log_batch(writer.as_mut(), &mut batch, &mut batch_bytes, &log_app).await;
                        }
                    }
                }
            }
            if !batch.is_empty() {
                flush_log_batch(writer.as_mut(), &mut batch, &mut batch_bytes, &log_app).await;
            }
            if let Some(writer) = writer.as_mut()
                && let Err(error) = writer.close().await
            {
                log::warn!(
                    "event=service_log_close_failed service_id={} error={}",
                    log_service_id,
                    error
                );
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
                                generation,
                                (samples > 1).then_some(process.cpu_usage()),
                                Some(process.memory()),
                                &metrics_app,
                            );
                        }
                    }
                }
            }
        });

        if service.health_configured() {
            let health_manager = self.clone();
            let health_app = app.clone();
            let health_service = service.clone();
            let mut health_done = done.subscribe();
            tokio::spawn(async move {
                let started = now_ms();
                let mut ticker = tokio::time::interval(Duration::from_millis(
                    health_service.health_interval_ms.max(500),
                ));
                ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        _ = health_done.changed() => break,
                        _ = ticker.tick() => {
                            let result = run_health_check(&health_service).await;
                            let elapsed = now_ms().saturating_sub(started);
                            let in_start_period = elapsed
                                < u128::from(health_service.health_start_period_ms);
                            let (health, error, latency) = match result {
                                Ok(latency) => ("healthy".to_string(), None, Some(latency)),
                                Err(_) if in_start_period => ("starting".to_string(), None, None),
                                Err(error) => ("unhealthy".to_string(), Some(error), None),
                            };
                            if !health_manager.set_health(
                                &health_service.id,
                                &health_service.name,
                                generation,
                                health,
                                error,
                                latency,
                                &health_app,
                            ) {
                                break;
                            }
                        }
                    }
                }
            });
        }

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
            let outcome = manager
                .finalize_run(
                    &service,
                    generation,
                    status,
                    exit_code,
                    wait_error.clone(),
                    started_at_ms,
                )
                .unwrap_or_else(|error| {
                    log::error!(
                        "event=service_finalize_failed service_id={} error={}",
                        service_id,
                        error
                    );
                    RunOutcome {
                        status: status.into(),
                        restart_delay_ms: None,
                    }
                });
            let run = RunHistory {
                id: uuid::Uuid::new_v4().to_string(),
                service_id: service_id.clone(),
                started_at_ms,
                ended_at_ms: now_ms(),
                status: outcome.status.clone(),
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
            manager.emit_lifecycle_state(&service_id, &app);
            let failed = outcome.status == "failed" || !matches!(exit_code, Some(0));
            if outcome.restart_delay_ms.is_none() && outcome.status != "stopped" && failed {
                let detail = wait_error.clone().unwrap_or_else(|| {
                    exit_code
                        .map(|code| format!("exited with code {code}"))
                        .unwrap_or_else(|| "process exited unexpectedly".into())
                });
                crate::notify_if_hidden(
                    &app,
                    "Service failed",
                    &format!("{}: {detail}", service.name),
                );
            }
            log::info!(
                "event=service_process_exited service_id={} status={} exit_code={:?}",
                service_id,
                outcome.status,
                exit_code
            );
            done.send_replace(true);
            if let Some(delay_ms) = outcome.restart_delay_ms {
                manager.schedule_restart(service, app, generation, delay_ms);
            }
        });
        Ok(())
    }

    pub async fn stop<R: Runtime>(&self, id: &str, app: &AppHandle<R>) -> Result<(), String> {
        log::info!("event=service_stop_requested service_id={id}");
        let pending_restart = {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Process registry lock poisoned")?;
            match entries.get_mut(id) {
                Some(entry) if entry.state.status == "waiting_to_restart" => {
                    entry.state.status = "stopped".into();
                    entry.state.restart_at_ms = None;
                    entry.state.restart_attempt = None;
                    Some(entry.stop.clone())
                }
                _ => None,
            }
        };
        if let Some(stop) = pending_restart {
            let _ = stop.send(true);
            if let Ok(mut attempts) = self.restart_attempts.lock() {
                attempts.remove(id);
            }
            self.emit_lifecycle_state(id, app);
            log::info!("event=service_restart_cancelled service_id={id}");
            return Ok(());
        }
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
        self.emit_lifecycle_state(id, app);
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
                    matches!(
                        entry.state.status.as_str(),
                        "running" | "starting" | "stopping" | "waiting_to_restart"
                    )
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

    /// Drops every trace of a deleted service from the in-memory registry.
    pub fn forget(&self, id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(id);
        }
        if let Ok(mut attempts) = self.restart_attempts.lock() {
            attempts.remove(id);
        }
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
                    generation: 0,
                }
            });
        self.emit_lifecycle_state(&id, app);
        Ok(())
    }

    fn set_metrics<R: Runtime>(
        &self,
        id: &str,
        generation: u64,
        cpu_usage_percent: Option<f32>,
        memory_usage_bytes: Option<u64>,
        app: &AppHandle<R>,
    ) {
        let updated = if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(id)
                && entry.generation == generation
                && entry.state.status == "running"
            {
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

    #[allow(clippy::too_many_arguments)]
    fn set_health<R: Runtime>(
        &self,
        id: &str,
        service_name: &str,
        generation: u64,
        health: String,
        error: Option<String>,
        latency_ms: Option<u64>,
        app: &AppHandle<R>,
    ) -> bool {
        let mut became_unhealthy = false;
        let notification_error = error.clone();
        let updated = if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(id)
                && entry.generation == generation
                && entry.state.status == "running"
            {
                became_unhealthy = health == "unhealthy" && entry.state.health != "unhealthy";
                entry.state.health = health;
                entry.state.health_checked_at_ms = Some(now_ms());
                entry.state.health_error = error;
                entry.state.health_latency_ms = latency_ms;
                true
            } else {
                false
            }
        } else {
            false
        };
        if updated {
            self.emit_state(id, app);
            if became_unhealthy {
                let detail = notification_error.unwrap_or_else(|| "Check failed".into());
                crate::notify_if_hidden(
                    app,
                    "Service unhealthy",
                    &format!("{service_name}: {detail}"),
                );
            }
        }
        updated
    }

    fn set_detected_url<R: Runtime>(
        &self,
        id: &str,
        generation: u64,
        url: String,
        app: &AppHandle<R>,
    ) {
        let updated = if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(id)
                && entry.generation == generation
                && entry.state.status == "running"
                && entry.state.detected_url.is_none()
            {
                entry.state.detected_url = Some(url);
                true
            } else {
                false
            }
        } else {
            false
        };
        if updated {
            self.emit_state(id, app);
            crate::refresh_tray_menu(app);
        }
    }

    /// Applies the final state of a completed run and decides whether the
    /// service should be relaunched. The restart decision is made while the
    /// registry lock is held so a concurrent stop always wins cleanly.
    fn finalize_run(
        &self,
        service: &Service,
        generation: u64,
        status: &str,
        exit_code: Option<i32>,
        error: Option<String>,
        started_at_ms: u128,
    ) -> Result<RunOutcome, String> {
        let outcome = {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| "Process registry lock poisoned".to_string())?;
            let Some(entry) = entries.get_mut(&service.id) else {
                return Ok(RunOutcome {
                    status: status.into(),
                    restart_delay_ms: None,
                });
            };
            if entry.generation != generation {
                return Ok(RunOutcome {
                    status: status.into(),
                    restart_delay_ms: None,
                });
            }
            let stop_requested = entry.state.status == "stopping";
            let run_status = if stop_requested { "stopped" } else { status };
            let mut attempts = self
                .restart_attempts
                .lock()
                .map_err(|_| "Restart tracker lock poisoned".to_string())?;
            if run_status == "stopped"
                || now_ms().saturating_sub(started_at_ms) >= RESTART_STABILITY_MS
            {
                attempts.remove(&service.id);
            }
            let failed = run_status == "failed" || !matches!(exit_code, Some(0));
            let policy_allows = !stop_requested
                && service.enabled
                && match service.restart_policy.as_str() {
                    "always" => true,
                    "on_failure" => failed,
                    _ => false,
                };
            // The tracked process is gone for every branch below.
            entry.state.pid = None;
            entry.state.started_at_ms = None;
            entry.state.cpu_usage_percent = None;
            entry.state.memory_usage_bytes = None;
            entry.state.metrics_sampled_at_ms = None;
            let mut restart_delay_ms = None;
            if policy_allows {
                let next_attempt = attempts.get(&service.id).copied().unwrap_or(0) + 1;
                let limit = service.max_restart_attempts;
                if limit.is_some_and(|max| next_attempt > max) {
                    let suffix = error
                        .as_deref()
                        .map(|error| format!(": {error}"))
                        .unwrap_or_default();
                    entry.state.status = "failed".into();
                    entry.state.exit_code = exit_code;
                    entry.state.error = Some(format!(
                        "Restart limit reached after {} attempts{suffix}",
                        limit.unwrap_or_default()
                    ));
                    entry.state.restart_at_ms = None;
                    entry.state.restart_attempt = None;
                    log::warn!(
                        "event=service_restart_limit_reached service_id={} attempts={}",
                        service.id,
                        limit.unwrap_or_default()
                    );
                } else {
                    attempts.insert(service.id.clone(), next_attempt);
                    let delay = restart_backoff_ms(service.restart_delay_ms, next_attempt);
                    entry.state.status = "waiting_to_restart".into();
                    entry.state.exit_code = exit_code;
                    entry.state.error = error;
                    entry.state.restart_at_ms = Some(now_ms().saturating_add(u128::from(delay)));
                    entry.state.restart_attempt = Some(next_attempt);
                    restart_delay_ms = Some(delay);
                    log::info!(
                        "event=service_restart_scheduled service_id={} attempt={} delay_ms={}",
                        service.id,
                        next_attempt,
                        delay
                    );
                }
            } else {
                entry.state.status = run_status.into();
                entry.state.exit_code = exit_code;
                entry.state.error = error;
                entry.state.restart_at_ms = None;
                entry.state.restart_attempt = None;
            }
            entry.state.health = if entry.state.status == "waiting_to_restart" {
                initial_health(service).into()
            } else {
                "not_configured".into()
            };
            entry.state.health_checked_at_ms = None;
            entry.state.health_latency_ms = None;
            entry.state.health_error = None;
            entry.state.detected_url = None;
            RunOutcome {
                status: run_status.into(),
                restart_delay_ms,
            }
        };
        Ok(outcome)
    }

    /// Waits out the backoff, then relaunches the service only if the entry is
    /// still the same generation and still waiting to restart.
    fn schedule_restart<R: Runtime>(
        &self,
        service: Service,
        app: AppHandle<R>,
        generation: u64,
        delay_ms: u64,
    ) {
        let manager = self.clone();
        tokio::spawn(async move {
            let stop_rx =
                manager.entries.lock().ok().and_then(|entries| {
                    entries.get(&service.id).map(|entry| entry.stop.subscribe())
                });
            let Some(mut stop_rx) = stop_rx else {
                return;
            };
            if *stop_rx.borrow() {
                return;
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        return;
                    }
                }
            }
            let id = service.id.clone();
            let still_waiting = manager
                .entries
                .lock()
                .ok()
                .and_then(|entries| {
                    entries.get(&id).map(|entry| {
                        entry.generation == generation && entry.state.status == "waiting_to_restart"
                    })
                })
                .unwrap_or(false);
            if !still_waiting {
                return;
            }
            let service = match app.try_state::<crate::AppState>() {
                Some(state) => match state.database.lock() {
                    Ok(database) => match database.get_service(&id) {
                        Ok(Some(service)) => service,
                        Ok(None) => return,
                        Err(error) => {
                            log::warn!(
                                "event=service_restart_reload_failed service_id={} error={}",
                                id,
                                error
                            );
                            service
                        }
                    },
                    Err(_) => service,
                },
                None => service,
            };
            if !service.enabled {
                if let Ok(mut entries) = manager.entries.lock()
                    && let Some(entry) = entries.get_mut(&id)
                    && entry.generation == generation
                    && entry.state.status == "waiting_to_restart"
                {
                    entry.state.status = "stopped".into();
                    entry.state.restart_at_ms = None;
                    entry.state.restart_attempt = None;
                }
                manager.emit_lifecycle_state(&id, &app);
                return;
            }
            if let Err(error) = manager
                .start_internal(service, app.clone(), false, Some(generation))
                .await
            {
                log::warn!(
                    "event=service_restart_failed service_id={} error={}",
                    id,
                    error
                );
            }
        });
    }

    fn emit_lifecycle_state<R: Runtime>(&self, id: &str, app: &AppHandle<R>) {
        self.emit_state(id, app);
        crate::refresh_tray_menu(app);
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

async fn run_health_check(service: &Service) -> Result<u64, String> {
    let limit = Duration::from_millis(service.health_timeout_ms.clamp(100, 60_000));
    let started = std::time::Instant::now();
    match service.health_check_type.as_str() {
        "tcp" => {
            let target = tcp_health_target(service.health_target.as_deref().unwrap_or_default())?;
            match timeout(limit, tokio::net::TcpStream::connect(&target)).await {
                Ok(Ok(_)) => Ok(started.elapsed().as_millis() as u64),
                Ok(Err(error)) => Err(format!("TCP check failed: {error}")),
                Err(_) => Err("TCP check timed out".to_string()),
            }
        }
        "http" => run_http_health_check(service, limit, started).await,
        _ => Ok(started.elapsed().as_millis() as u64),
    }
}

fn redact_secrets(text: &str, secrets: &[String]) -> String {
    let mut redacted = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "***");
        }
    }
    redacted
}

/// Finds the first usable local URL in a chunk of process output.
fn detect_service_url(text: &str) -> Option<String> {
    let lowered = text.to_ascii_lowercase();
    let mut start = None;
    for scheme in ["http://", "https://"] {
        if let Some(index) = lowered.find(scheme) {
            start = Some(start.map_or(index, |current: usize| current.min(index)));
        }
    }
    if let Some(index) = start {
        let candidate = &text[index..];
        let end = candidate
            .find(|character: char| {
                character.is_whitespace()
                    || character.is_control()
                    || matches!(character, '"' | '\'' | '<' | '>' | ')' | ']' | '}' | '`')
            })
            .unwrap_or(candidate.len());
        let candidate = candidate[..end].trim_end_matches([',', '.', ';', '!', '?']);
        if let Some((_, rest)) = candidate.split_once("://")
            && !rest.is_empty()
            && !rest.starts_with('/')
            && candidate.len() <= 2048
        {
            return Some(candidate.to_string());
        }
    }
    for host in ["localhost:", "127.0.0.1:", "0.0.0.0:"] {
        let Some(index) = lowered.find(host) else {
            continue;
        };
        let candidate = &text[index..];
        let end = candidate
            .find(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '.' | ':' | '[' | ']'))
            })
            .unwrap_or(candidate.len());
        let candidate = &candidate[..end];
        let port = candidate.rsplit(':').next().unwrap_or_default();
        if port.parse::<u16>().is_ok_and(|port| port > 0) {
            return Some(format!("http://{candidate}"));
        }
    }
    None
}

fn tcp_health_target(target: &str) -> Result<String, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("TCP health check target is empty".into());
    }
    let valid_port = |port: &str| port.parse::<u16>().map(|port| port > 0).unwrap_or(false);
    if valid_port(target) {
        return Ok(format!("127.0.0.1:{target}"));
    }
    let Some((host, port)) = target.rsplit_once(':') else {
        return Err("TCP health check target must be a port or host:port".into());
    };
    if host.is_empty() || !valid_port(port) {
        return Err("TCP health check target must be a port or host:port".into());
    }
    Ok(format!("{host}:{port}"))
}

async fn run_http_health_check(
    service: &Service,
    limit: Duration,
    started: std::time::Instant,
) -> Result<u64, String> {
    let target = service.health_target.as_deref().unwrap_or_default();
    let rest = target
        .strip_prefix("http://")
        .ok_or("HTTP health checks must use an http:// URL")?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err("HTTP health check URL must include a host".into());
    }
    let connect = if authority.contains(':') {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let expected = service.health_expected_status;
    let response = timeout(limit, async {
        let mut stream = tokio::net::TcpStream::connect(&connect).await?;
        let request = format!(
            "GET {path} HTTP/1.0\r\nHost: {authority}\r\nUser-Agent: ProcessHub\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await?;
        let mut data = Vec::with_capacity(1024);
        let mut buffer = [0_u8; 1024];
        loop {
            let read = stream.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            if data.len() >= 8192 || data.windows(2).any(|window| window == b"\r\n") {
                break;
            }
        }
        Ok::<Vec<u8>, std::io::Error>(data)
    })
    .await
    .map_err(|_| "HTTP check timed out".to_string())?
    .map_err(|error| format!("HTTP check failed: {error}"))?;
    let status_line = response
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let status_line = String::from_utf8_lossy(status_line);
    let code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or("Malformed HTTP response")?;
    let healthy = expected.map_or((200..400).contains(&code), |expected| expected == code);
    if !healthy {
        return Err(match expected {
            Some(expected) => format!("HTTP {code}, expected {expected}"),
            None => format!("HTTP {code}"),
        });
    }
    Ok(started.elapsed().as_millis() as u64)
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

struct ServiceLogWriter {
    directory: PathBuf,
    service_id: String,
    max_file_bytes: u64,
    file: Option<BufWriter<fs::File>>,
    current_size: u64,
}

impl ServiceLogWriter {
    fn new(directory: PathBuf, service_id: String, max_file_bytes: u64) -> Self {
        Self {
            directory,
            service_id,
            max_file_bytes,
            file: None,
            current_size: 0,
        }
    }

    async fn write_batch(&mut self, events: &[LogEvent]) -> std::io::Result<()> {
        fs::create_dir_all(&self.directory).await?;
        for event in events {
            let mut line = serde_json::to_vec(event).map_err(std::io::Error::other)?;
            line.push(b'\n');
            if self.current_size > 0
                && self.current_size.saturating_add(line.len() as u64) > self.max_file_bytes
            {
                self.rotate().await?;
            }
            let writer = self.writer().await?;
            writer.write_all(&line).await?;
            self.current_size = self.current_size.saturating_add(line.len() as u64);
        }
        if let Some(writer) = self.file.as_mut() {
            writer.flush().await?;
        }
        Ok(())
    }

    async fn writer(&mut self) -> std::io::Result<&mut BufWriter<fs::File>> {
        if self.file.is_none() {
            let path = self.directory.join(format!("{}.log", self.service_id));
            let file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await?;
            self.current_size = fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);
            self.file = Some(BufWriter::with_capacity(64 * 1024, file));
        }
        Ok(self.file.as_mut().expect("log writer is initialized above"))
    }

    async fn close(&mut self) -> std::io::Result<()> {
        if let Some(mut writer) = self.file.take() {
            let flush_result = writer.flush().await;
            let file = writer.into_inner();
            let std_file = file.into_std().await;
            drop(std_file);
            flush_result?;
        }
        Ok(())
    }

    async fn rotate(&mut self) -> std::io::Result<()> {
        self.close().await?;
        let active = self.directory.join(format!("{}.log", self.service_id));
        for index in (1..=ROTATED_LOG_FILES).rev() {
            let source = if index == 1 {
                active.clone()
            } else {
                rotated_log_path(&self.directory, &self.service_id, index - 1)
            };
            let destination = rotated_log_path(&self.directory, &self.service_id, index);
            if index == ROTATED_LOG_FILES {
                let _ = fs::remove_file(&destination).await;
            }
            if fs::metadata(&source).await.is_ok() {
                fs::rename(source, destination).await?;
            }
        }
        self.current_size = 0;
        Ok(())
    }
}

async fn flush_log_batch<R: Runtime>(
    writer: Option<&mut ServiceLogWriter>,
    batch: &mut Vec<LogEvent>,
    batch_bytes: &mut usize,
    app: &AppHandle<R>,
) {
    if let Some(writer) = writer
        && let Err(error) = writer.write_batch(batch).await
    {
        let service_id = batch
            .first()
            .map(|event| event.service_id.as_str())
            .unwrap_or("unknown");
        log::warn!(
            "event=service_log_write_failed service_id={} error={}",
            service_id,
            error
        );
    }
    let _ = app.emit("service-log-batch", batch.as_slice());
    batch.clear();
    *batch_bytes = 0;
}

fn rotated_log_path(directory: &Path, service_id: &str, index: usize) -> PathBuf {
    directory.join(format!("{service_id}.log.{index}"))
}

/// Reads at most `limit` events from the end of a log file without scanning the
/// whole file. Events are returned oldest-first, matching the active log order.
async fn tail_log_events(
    path: &Path,
    service_id: &str,
    limit: usize,
) -> Result<Vec<LogEvent>, String> {
    const CHUNK_BYTES: u64 = 64 * 1024;
    let mut file = match fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut position = file
        .metadata()
        .await
        .map_err(|error| error.to_string())?
        .len();
    let mut carry = Vec::new();
    let mut newest_first = Vec::with_capacity(limit);
    while position > 0 && newest_first.len() < limit {
        let read_size = CHUNK_BYTES.min(position);
        position -= read_size;
        file.seek(std::io::SeekFrom::Start(position))
            .await
            .map_err(|error| error.to_string())?;
        let mut chunk = vec![0_u8; read_size as usize];
        file.read_exact(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        chunk.extend_from_slice(&carry);
        let mut end = chunk.len();
        while newest_first.len() < limit {
            let Some(newline) = chunk[..end].iter().rposition(|byte| *byte == b'\n') else {
                break;
            };
            let line = &chunk[newline + 1..end];
            end = newline;
            if line.is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_slice::<LogEvent>(line)
                && event.service_id == service_id
            {
                newest_first.push(event);
            }
        }
        carry = chunk[..end].to_vec();
    }
    // The first line of the file has no preceding newline, so it stays in the
    // carry buffer when the scan reaches byte zero.
    if position == 0
        && !carry.is_empty()
        && newest_first.len() < limit
        && let Ok(event) = serde_json::from_slice::<LogEvent>(&carry)
        && event.service_id == service_id
    {
        newest_first.push(event);
    }
    newest_first.reverse();
    Ok(newest_first)
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
fn configure_windows_no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
}

#[cfg(windows)]
async fn wait_for_process_tree(_pid: u32) {}

#[cfg(windows)]
async fn terminate_tree(pid: u32, force: bool) {
    let mut command = Command::new("taskkill");
    let pid = pid.to_string();
    command
        .args(["/PID", pid.as_str(), "/T"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if force {
        command.arg("/F");
    }
    configure_windows_no_window(&mut command);
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
            log_max_size_mb: 10,
            health_check_type: "none".into(),
            health_target: None,
            health_interval_ms: 5000,
            health_timeout_ms: 2000,
            health_expected_status: None,
            health_start_period_ms: 5000,
            depends_on: vec![],
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

    async fn wait_for_health(manager: &ProcessManager, id: &str, health: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if manager
                    .states()
                    .iter()
                    .any(|state| state.service_id == id && state.health == health)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("health state transition");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn refuses_to_start_when_a_secret_is_missing() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/echo", &[]);
        service.environment.push(EnvironmentVariable {
            key: "PROCESSHUB_MISSING_SECRET".into(),
            value: String::new(),
            is_secret: true,
        });
        let id = service.id.clone();
        let error = manager
            .start(service, handle)
            .await
            .expect_err("missing secret blocks startup");
        assert!(error.contains("is not set"), "unexpected error: {error}");
        wait_for_status(&manager, &id, "failed").await;
    }

    #[test]
    fn redacts_secret_values_from_process_output() {
        let secrets = vec!["super-secret".to_string(), String::new()];
        assert_eq!(
            redact_secrets("token=super-secret and super-secret", &secrets),
            "token=*** and ***"
        );
        assert_eq!(
            redact_secrets("nothing to hide", &secrets),
            "nothing to hide"
        );
    }

    #[test]
    fn detects_local_urls_from_process_output() {
        assert_eq!(
            detect_service_url("ready at http://localhost:3000"),
            Some("http://localhost:3000".into())
        );
        assert_eq!(
            detect_service_url("Server listening on localhost:5173"),
            Some("http://localhost:5173".into())
        );
        assert_eq!(
            detect_service_url("\u{1b}[32mhttp://127.0.0.1:8000/docs\u{1b}[0m"),
            Some("http://127.0.0.1:8000/docs".into())
        );
        assert_eq!(
            detect_service_url("visit http://localhost:3000, then continue"),
            Some("http://localhost:3000".into())
        );
        assert_eq!(detect_service_url("no url in this line"), None);
        assert_eq!(detect_service_url("http://"), None);
        assert_eq!(detect_service_url("localhost:not-a-port"), None);
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
        let mut writer =
            ServiceLogWriter::new(directory.clone(), service_id.clone(), MAX_LOG_FILE_BYTES);
        for _ in 0..2 {
            writer
                .write_batch(&[LogEvent {
                    service_id: service_id.clone(),
                    stream: "stdout".into(),
                    timestamp_ms: now_ms(),
                    text: text.clone(),
                }])
                .await
                .expect("persist large log event");
        }
        writer.close().await.expect("close log writer");
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

    #[tokio::test]
    async fn log_writer_persists_a_batch_in_order() {
        let directory =
            std::env::temp_dir().join(format!("processhub-logs-{}", uuid::Uuid::new_v4()));
        let service_id = uuid::Uuid::new_v4().to_string();
        let mut writer =
            ServiceLogWriter::new(directory.clone(), service_id.clone(), MAX_LOG_FILE_BYTES);
        let events = (0..100)
            .map(|index| LogEvent {
                service_id: service_id.clone(),
                stream: if index % 2 == 0 { "stdout" } else { "stderr" }.into(),
                timestamp_ms: index,
                text: format!("line {index}\n"),
            })
            .collect::<Vec<_>>();
        writer.write_batch(&events).await.expect("persist batch");
        writer.close().await.expect("close writer");

        let contents = fs::read_to_string(directory.join(format!("{service_id}.log")))
            .await
            .expect("read log fixture");
        let lines = contents.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 100);
        for (index, line) in lines.iter().enumerate() {
            let parsed: LogEvent = serde_json::from_str(line).expect("parse log line");
            assert_eq!(parsed.timestamp_ms, index as u128);
            assert_eq!(parsed.text, format!("line {index}\n"));
        }
        fs::remove_dir_all(directory)
            .await
            .expect("remove log fixture");
    }

    #[tokio::test]
    async fn load_logs_reads_only_the_newest_tail_across_rotations() {
        let directory =
            std::env::temp_dir().join(format!("processhub-tail-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).await.expect("create logs");
        let service_id = uuid::Uuid::new_v4().to_string();
        let mut global = 0_u128;
        for index in [3_usize, 2, 1, 0] {
            let path = if index == 0 {
                directory.join(format!("{service_id}.log"))
            } else {
                directory.join(format!("{service_id}.log.{index}"))
            };
            let mut contents = String::new();
            for _ in 0..700 {
                let event = LogEvent {
                    service_id: service_id.clone(),
                    stream: "stdout".into(),
                    timestamp_ms: global,
                    text: format!("line {global}\n"),
                };
                contents.push_str(&serde_json::to_string(&event).unwrap());
                contents.push('\n');
                global += 1;
            }
            fs::write(&path, contents).await.expect("write log file");
        }

        let manager = ProcessManager::default().with_log_dir(directory.clone());
        let events = manager.load_logs(&service_id).await.expect("load logs");
        assert_eq!(events.len(), 2_000);
        assert_eq!(events[0].timestamp_ms, 800);
        assert_eq!(events[1_999].timestamp_ms, 2_799);
        assert!(
            events
                .windows(2)
                .all(|pair| pair[0].timestamp_ms < pair[1].timestamp_ms)
        );

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

    #[test]
    fn restart_backoff_doubles_and_is_capped() {
        assert_eq!(restart_backoff_ms(1000, 1), 1000);
        assert_eq!(restart_backoff_ms(1000, 2), 2000);
        assert_eq!(restart_backoff_ms(1000, 3), 4000);
        assert_eq!(restart_backoff_ms(1000, 10), RESTART_MAX_DELAY_MS);
        assert_eq!(restart_backoff_ms(0, 1), 100);
        assert_eq!(restart_backoff_ms(60_000, 2), 60_000);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn on_failure_restarts_until_the_attempt_limit_is_reached() {
        let app = tauri::test::mock_app();
        let mut service = test_service("/bin/sh", &["-c", "exit 3"]);
        service.restart_policy = "on_failure".into();
        service.restart_delay_ms = 50;
        service.max_restart_attempts = Some(2);
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
            .expect("start failing process");

        wait_for_status(&manager, &id, "failed").await;
        let state = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("runtime state");
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Restart limit")),
            "unexpected error: {:?}",
            state.error
        );
        assert_eq!(state.restart_attempt, None);
        assert_eq!(state.exit_code, Some(3));

        let history = app
            .state::<crate::AppState>()
            .database
            .lock()
            .expect("lock database")
            .list_run_history(&id)
            .expect("list history");
        assert_eq!(history.len(), 3, "initial run plus two restart attempts");
        assert!(history.iter().all(|run| run.exit_code == Some(3)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn on_failure_does_not_restart_a_clean_exit() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/sh", &["-c", "exit 0"]);
        service.restart_policy = "on_failure".into();
        service.restart_delay_ms = 50;
        service.max_restart_attempts = Some(2);
        let id = service.id.clone();

        manager
            .start(service, handle)
            .await
            .expect("start clean process");
        wait_for_status(&manager, &id, "exited").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let state = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("runtime state");
        assert_eq!(state.status, "exited");
        assert_eq!(state.restart_attempt, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stopping_a_service_cancels_a_scheduled_restart() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/sh", &["-c", "exit 3"]);
        service.restart_policy = "always".into();
        service.restart_delay_ms = 500;
        service.max_restart_attempts = Some(5);
        let id = service.id.clone();

        manager
            .start(service, handle.clone())
            .await
            .expect("start failing process");
        wait_for_status(&manager, &id, "waiting_to_restart").await;
        let waiting = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("waiting state");
        assert_eq!(waiting.restart_attempt, Some(1));
        assert!(waiting.restart_at_ms.is_some());

        manager.stop(&id, &handle).await.expect("cancel restart");
        wait_for_status(&manager, &id, "stopped").await;
        tokio::time::sleep(Duration::from_millis(900)).await;
        let state = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("runtime state");
        assert_eq!(state.status, "stopped");
        assert_eq!(state.restart_attempt, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn always_restarts_after_a_clean_exit() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/sh", &["-c", "exit 0"]);
        service.restart_policy = "always".into();
        service.restart_delay_ms = 50;
        service.max_restart_attempts = Some(1);
        let id = service.id.clone();

        manager
            .start(service, handle)
            .await
            .expect("start clean process");
        wait_for_status(&manager, &id, "failed").await;
        let state = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("runtime state");
        assert!(
            state
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Restart limit"))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tcp_health_check_reports_healthy() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind health listener");
        let port = listener.local_addr().expect("listener address").port();
        let acceptor = tokio::spawn(async move { while listener.accept().await.is_ok() {} });

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/sh", &["-c", "exec sleep 30"]);
        service.health_check_type = "tcp".into();
        service.health_target = Some(port.to_string());
        service.health_interval_ms = 500;
        service.health_start_period_ms = 0;
        let id = service.id.clone();

        manager
            .start(service, handle.clone())
            .await
            .expect("start health-checked process");
        wait_for_health(&manager, &id, "healthy").await;
        let state = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("runtime state");
        assert!(state.health_latency_ms.is_some());

        manager.stop(&id, &handle).await.expect("stop process");
        acceptor.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn http_health_check_honors_the_expected_status() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind health listener");
        let port = listener.local_addr().expect("listener address").port();
        let http = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut request = [0_u8; 1024];
                    let _ = stream.read(&mut request).await;
                    let _ = stream
                        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                        .await;
                });
            }
        });

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/sh", &["-c", "exec sleep 30"]);
        service.health_check_type = "http".into();
        service.health_target = Some(format!("http://127.0.0.1:{port}/health"));
        service.health_expected_status = Some(204);
        service.health_interval_ms = 500;
        service.health_start_period_ms = 0;
        let id = service.id.clone();

        manager
            .start(service, handle.clone())
            .await
            .expect("start health-checked process");
        wait_for_health(&manager, &id, "healthy").await;

        manager.stop(&id, &handle).await.expect("stop process");
        http.abort();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn detects_a_url_from_running_process_output() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let service = test_service(
            "/bin/sh",
            &["-c", "echo 'ready at http://localhost:4321'; exec sleep 30"],
        );
        let id = service.id.clone();

        manager
            .start(service, handle.clone())
            .await
            .expect("start process");
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if manager.states().iter().any(|state| {
                    state.service_id == id
                        && state.detected_url.as_deref() == Some("http://localhost:4321")
                }) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("detected URL");

        manager.stop(&id, &handle).await.expect("stop process");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_health_check_reports_unhealthy_after_the_start_period() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("reserve a free port");
        let port = listener.local_addr().expect("listener address").port();
        drop(listener);

        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let manager = ProcessManager::default();
        let mut service = test_service("/bin/sh", &["-c", "exec sleep 30"]);
        service.health_check_type = "tcp".into();
        service.health_target = Some(port.to_string());
        service.health_interval_ms = 500;
        service.health_timeout_ms = 200;
        service.health_start_period_ms = 0;
        let id = service.id.clone();

        manager
            .start(service, handle.clone())
            .await
            .expect("start health-checked process");
        wait_for_health(&manager, &id, "unhealthy").await;
        let state = manager
            .states()
            .into_iter()
            .find(|state| state.service_id == id)
            .expect("runtime state");
        assert!(
            state
                .health_error
                .as_deref()
                .is_some_and(|error| error.contains("TCP check failed"))
        );

        manager.stop(&id, &handle).await.expect("stop process");
    }
}
