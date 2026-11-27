mod database;
mod dependencies;
mod models;
mod process_manager;

use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use database::Database;
use models::{CreateService, LogEvent, RunHistory, Service};
use process_manager::ProcessManager;

pub(crate) struct AppState {
    pub(crate) database: Mutex<Database>,
    pub(crate) processes: ProcessManager,
    pub(crate) session_id: String,
}

static UPDATE_INSTALLING: AtomicBool = AtomicBool::new(false);

struct UpdateInstallGuard;

impl Drop for UpdateInstallGuard {
    fn drop(&mut self) {
        UPDATE_INSTALLING.store(false, Ordering::SeqCst);
    }
}

/// Clears bookkeeping for a process that already exited, or verifies and stops
/// a process left behind by a previous ProcessHub session.
async fn reconcile_active_process(state: &AppState, service_id: &str) -> Result<(), String> {
    let record = tokio::task::block_in_place(|| {
        let database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        database
            .get_active_process(service_id)
            .map_err(|error| error.to_string())
    })?;
    let Some(record) = record else {
        return Ok(());
    };
    if record.manager_session_id == state.session_id {
        if state.processes.is_running(service_id) {
            return Ok(());
        }
        tokio::task::block_in_place(|| {
            state
                .database
                .lock()
                .map_err(|_| "Database lock poisoned".to_string())?
                .delete_active_process(service_id)
                .map_err(|error| error.to_string())
        })?;
        return Ok(());
    }
    match process_manager::recover_orphaned_process(&record).await? {
        process_manager::OrphanedProcessRecovery::Stopped
        | process_manager::OrphanedProcessRecovery::AlreadyExited => {
            tokio::task::block_in_place(|| {
                state
                    .database
                    .lock()
                    .map_err(|_| "Database lock poisoned".to_string())?
                    .delete_active_process(service_id)
                    .map_err(|error| error.to_string())
            })?;
            Ok(())
        }
        process_manager::OrphanedProcessRecovery::Unverifiable => Err(format!(
            "A process from a previous ProcessHub session (PID {}) could not be verified. It was left untouched; stop it manually before using this service.",
            record.pid
        )),
    }
}

type ServiceGraph = (Vec<Service>, Vec<(String, String)>);

fn service_graph(state: &AppState) -> Result<ServiceGraph, String> {
    tokio::task::block_in_place(|| {
        let database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        let services = database
            .list_services()
            .map_err(|error| error.to_string())?;
        let dependencies = database
            .list_dependencies()
            .map_err(|error| error.to_string())?;
        Ok((services, dependencies))
    })
}

fn validate_service_dependencies(state: &AppState, service: &Service) -> Result<(), String> {
    let (known, edges) = {
        let database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        let known = database
            .list_services()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|service| service.id)
            .collect::<HashSet<_>>();
        let edges = database
            .list_dependencies()
            .map_err(|error| error.to_string())?;
        (known, edges)
    };
    dependencies::validate_graph(&service.id, &service.depends_on, &known, &edges)
}

async fn wait_for_service_ready(state: &AppState, service: &Service) -> Result<(), String> {
    if !service.health_configured() {
        return Ok(());
    }
    let timeout_ms = service
        .health_start_period_ms
        .saturating_add(service.health_timeout_ms)
        .saturating_add(service.health_interval_ms.saturating_mul(2))
        .min(60_000);
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let runtime = state
            .processes
            .states()
            .into_iter()
            .find(|runtime| runtime.service_id == service.id);
        match runtime {
            Some(runtime) => match runtime.status.as_str() {
                "running" => match runtime.health.as_str() {
                    "healthy" => return Ok(()),
                    "unhealthy" => {
                        return Err(runtime
                            .health_error
                            .unwrap_or_else(|| format!("{} is unhealthy", service.name)));
                    }
                    _ => {}
                },
                "failed" | "exited" | "stopped" => {
                    return Err(runtime.error.unwrap_or_else(|| {
                        format!("{} exited before becoming ready", service.name)
                    }));
                }
                _ => {}
            },
            None => return Err(format!("{} is no longer running", service.name)),
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Timed out waiting for {} to become healthy",
                service.name
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

const KEYRING_SERVICE: &str = "com.aakashhq.processhub";

fn secret_account(service_id: &str, key: &str) -> String {
    format!("{service_id}:{key}")
}

pub(crate) fn read_secret(service_id: &str, key: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &secret_account(service_id, key))
        .map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn store_secret(service_id: &str, key: &str, value: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &secret_account(service_id, key))
        .map_err(|error| error.to_string())?;
    entry.set_password(value).map_err(|error| error.to_string())
}

fn delete_secret(service_id: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &secret_account(service_id, key))
        .map_err(|error| error.to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn valid_secret_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[tauri::command]
fn set_service_secret(
    service_id: String,
    key: String,
    value: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if !valid_secret_key(&key) {
        return Err("Secret names must use letters, numbers, and underscores".into());
    }
    if value.is_empty() {
        return Err("Secret value cannot be empty".into());
    }
    let mut database = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    if database
        .get_service(&service_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("Service not found".into());
    }
    store_secret(&service_id, &key, &value)?;
    database
        .ensure_secret_key(&service_id, &key)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn delete_service_secret(service_id: String, key: String) -> Result<(), String> {
    delete_secret(&service_id, &key)
}

#[tauri::command]
fn list_services(state: tauri::State<'_, AppState>) -> Result<Vec<Service>, String> {
    state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .list_services()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_service(id: String, state: tauri::State<'_, AppState>) -> Result<Option<Service>, String> {
    state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .get_service(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_service(
    input: CreateService,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Service, String> {
    let service = Service::from_create(input).map_err(|error| error.to_string())?;
    validate_service_dependencies(&state, &service)?;
    let created = {
        let mut database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        database
            .insert_service(&service)
            .map_err(|error| error.to_string())?;
        database
            .get_service(&service.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Created service could not be loaded".to_string())?
    };
    refresh_tray_menu(&app);
    Ok(created)
}

#[tauri::command]
fn update_service(
    service: Service,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Service, String> {
    service.validate().map_err(|error| error.to_string())?;
    validate_service_dependencies(&state, &service)?;
    let previous = {
        let database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        database
            .get_service(&service.id)
            .map_err(|error| error.to_string())?
    };
    let updated = {
        let mut database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        database
            .update_service(&service)
            .map_err(|error| error.to_string())?;
        database
            .get_service(&service.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Updated service could not be loaded".to_string())?
    };
    let previous_icon = previous
        .as_ref()
        .and_then(|existing| existing.icon_path.clone());
    if previous_icon.as_deref() != service.icon_path.as_deref()
        && let Some(previous_icon) = previous_icon
        && let Ok(data_dir) = app.path().app_data_dir()
    {
        remove_managed_icon(&previous_icon, &data_dir);
    }
    if let Some(previous) = previous {
        let retained = service
            .environment
            .iter()
            .filter(|variable| variable.is_secret)
            .map(|variable| variable.key.as_str())
            .collect::<HashSet<_>>();
        for variable in previous
            .environment
            .iter()
            .filter(|variable| variable.is_secret)
        {
            if !retained.contains(variable.key.as_str()) {
                let _ = delete_secret(&service.id, &variable.key);
            }
        }
    }
    refresh_tray_menu(&app);
    Ok(updated)
}

#[tauri::command]
async fn delete_service(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    delete_service_from_state(id, &app, &state).await
}

async fn delete_service_from_state<R: Runtime>(
    id: String,
    app: &tauri::AppHandle<R>,
    state: &AppState,
) -> Result<bool, String> {
    reconcile_active_process(state, &id).await?;
    if state.processes.is_running(&id) {
        state.processes.stop(&id, app).await?;
    }
    let (deleted, service) = tokio::task::block_in_place(|| {
        let mut database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        let service = database
            .get_service(&id)
            .map_err(|error| error.to_string())?;
        let deleted = database
            .delete_service(&id)
            .map_err(|error| error.to_string())?;
        Ok::<_, String>((deleted, service))
    })?;
    if deleted && let Some(service) = service.as_ref() {
        for variable in service
            .environment
            .iter()
            .filter(|variable| variable.is_secret)
        {
            let _ = delete_secret(&service.id, &variable.key);
        }
        match app.path().app_data_dir() {
            Ok(data_dir) => remove_service_files(service, &data_dir),
            Err(error) => log::warn!(
                "event=service_file_cleanup_skipped service_id={} error={}",
                id,
                error
            ),
        }
    }
    state.processes.forget(&id);
    refresh_tray_menu(app);
    Ok(deleted)
}

/// Removes app-managed artifacts for a deleted service. Paths outside the
/// managed icon and log directories are never touched.
fn remove_service_files(service: &Service, data_dir: &std::path::Path) {
    if let Some(icon_path) = &service.icon_path {
        remove_managed_icon(icon_path, data_dir);
    }
    if !is_valid_service_id(&service.id) {
        return;
    }
    let log_dir = data_dir.join("service-logs");
    let mut paths = vec![log_dir.join(format!("{}.log", service.id))];
    for index in 1..=process_manager::ROTATED_LOG_FILES {
        paths.push(log_dir.join(format!("{}.log.{index}", service.id)));
    }
    for path in paths {
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!(
                "event=service_log_cleanup_failed service_id={} error={}",
                service.id,
                error
            );
        }
    }
}

fn remove_managed_icon(icon_path: &str, data_dir: &std::path::Path) {
    let icon = std::path::Path::new(icon_path);
    if !icon.starts_with(data_dir.join("icons")) {
        return;
    }
    if let Err(error) = std::fs::remove_file(icon)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        log::warn!(
            "event=service_icon_cleanup_failed path={} error={}",
            icon_path,
            error
        );
    }
}

/// Removes icons that no service references, such as files imported into a
/// form that was never saved.
fn cleanup_unreferenced_icons(
    icon_dir: &std::path::Path,
    referenced: &HashSet<std::path::PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(icon_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && !referenced.contains(&path) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn is_valid_service_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

#[tauri::command]
fn import_service_icon(source_path: String, app: tauri::AppHandle) -> Result<String, String> {
    let source = std::path::PathBuf::from(source_path);
    let icon_directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("icons");
    copy_icon_to_directory(&source, &icon_directory).map(|path| path.to_string_lossy().into_owned())
}

fn copy_icon_to_directory(
    source: &std::path::Path,
    icon_directory: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let extension = source
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .filter(|extension| matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp"))
        .ok_or("Choose a PNG, JPEG, or WebP image")?;
    let metadata = std::fs::metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err("Icon must be a file smaller than 8 MB".into());
    }
    let signature = std::fs::read(source).map_err(|error| error.to_string())?;
    let valid_signature = match extension.as_str() {
        "png" => signature.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => signature.starts_with(&[0xff, 0xd8, 0xff]),
        "webp" => {
            signature.len() >= 12 && &signature[..4] == b"RIFF" && &signature[8..12] == b"WEBP"
        }
        _ => false,
    };
    if !valid_signature {
        return Err("The selected file does not match its image type".into());
    }
    std::fs::create_dir_all(icon_directory).map_err(|error| error.to_string())?;
    let destination = icon_directory.join(format!("{}.{}", uuid::Uuid::new_v4(), extension));
    std::fs::copy(source, &destination).map_err(|error| error.to_string())?;
    Ok(destination)
}

#[tauri::command]
fn get_app_setting(
    key: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .get_setting(&key)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_app_setting(
    key: String,
    value: serde_json::Value,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let retention_days = if key == "logRetentionDays" {
        Some(
            value
                .as_u64()
                .filter(|days| [7, 14, 30, 90].contains(days))
                .ok_or("Log retention must be 7, 14, 30, or 90 days")?,
        )
    } else {
        None
    };
    if key == "startAtLogin" {
        let enabled = value.as_bool().ok_or("Launch at login must be a boolean")?;
        use tauri_plugin_autostart::ManagerExt;
        let manager = app.autolaunch();
        if enabled {
            manager.enable().map_err(|error| error.to_string())?;
        } else {
            manager.disable().map_err(|error| error.to_string())?;
        }
    }
    state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .set_setting(&key, &value)
        .map_err(|error| error.to_string())?;
    if let Some(days) = retention_days {
        state.processes.set_log_retention_days(days);
    }
    Ok(())
}

async fn start_targets<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    targets: Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let (services, edges) = match service_graph(state) {
        Ok(graph) => graph,
        Err(error) => return (Vec::new(), vec![error]),
    };
    let by_id = services
        .iter()
        .cloned()
        .map(|service| (service.id.clone(), service))
        .collect::<HashMap<_, _>>();
    let mut started = Vec::new();
    let mut failures = Vec::new();
    let mut failed = HashSet::new();
    let mut order = Vec::new();
    let mut scheduled = HashSet::new();
    for target in &targets {
        match dependencies::start_order(std::slice::from_ref(target), &services, &edges) {
            Ok(list) => {
                for service_id in list {
                    if scheduled.insert(service_id.clone()) {
                        order.push(service_id);
                    }
                }
            }
            Err(error) => {
                let name = by_id
                    .get(target)
                    .map(|service| service.name.as_str())
                    .unwrap_or(target.as_str());
                failures.push(format!("{name}: {error}"));
            }
        }
    }
    for service_id in order {
        let Some(service) = by_id.get(&service_id).cloned() else {
            continue;
        };
        if state.processes.is_running(&service_id) {
            continue;
        }
        if let Some(dependency) = service
            .depends_on
            .iter()
            .find(|dependency| failed.contains(*dependency))
        {
            failures.push(format!(
                "{}: dependency {dependency} failed to start",
                service.name
            ));
            failed.insert(service.id.clone());
            continue;
        }
        if let Err(error) = reconcile_active_process(state, &service_id).await {
            failures.push(format!("{}: {error}", service.name));
            failed.insert(service.id.clone());
            continue;
        }
        match state.processes.start(service.clone(), app.clone()).await {
            Ok(()) => match wait_for_service_ready(state, &service).await {
                Ok(()) => started.push(service.name),
                Err(error) => {
                    failures.push(format!("{}: {error}", service.name));
                    failed.insert(service.id.clone());
                }
            },
            Err(error) => {
                failures.push(format!("{}: {error}", service.name));
                failed.insert(service.id.clone());
            }
        }
    }
    (started, failures)
}

fn startup_report(started: Vec<String>, failures: Vec<String>) -> serde_json::Value {
    serde_json::json!({ "started": started, "failures": failures })
}

#[tauri::command]
async fn start_auto_start_services(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let (services, _) = service_graph(&state)?;
    let targets = services
        .iter()
        .filter(|service| service.enabled && service.auto_start)
        .map(|service| service.id.clone())
        .collect::<Vec<_>>();
    let (started, failures) = start_targets(&app, &state, targets).await;
    let report = startup_report(started, failures);
    let _ = app.emit("startup-report", report.clone());
    Ok(report)
}

#[tauri::command]
async fn start_all_services(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let (services, _) = service_graph(&state)?;
    let targets = services
        .iter()
        .filter(|service| service.enabled)
        .map(|service| service.id.clone())
        .collect::<Vec<_>>();
    let (started, failures) = start_targets(&app, &state, targets).await;
    let report = startup_report(started, failures);
    let _ = app.emit("startup-report", report.clone());
    Ok(report)
}

#[tauri::command]
async fn restart_all_services(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let mut failures = stop_all_from_state(&app, &state).await?;
    let (services, _) = service_graph(&state)?;
    let targets = services
        .iter()
        .filter(|service| service.enabled)
        .map(|service| service.id.clone())
        .collect::<Vec<_>>();
    let (started, start_failures) = start_targets(&app, &state, targets).await;
    failures.extend(start_failures);
    let report = startup_report(started, failures);
    let _ = app.emit("startup-report", report.clone());
    Ok(report)
}

#[tauri::command]
async fn recover_orphaned_services(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let (records, names) = tokio::task::block_in_place(|| {
        let database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        let names = database
            .list_services()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|service| (service.id, service.name))
            .collect::<std::collections::HashMap<_, _>>();
        Ok::<_, String>((
            database
                .list_active_processes()
                .map_err(|error| error.to_string())?,
            names,
        ))
    })?;
    let mut cleared = Vec::new();
    let mut unverifiable = Vec::new();
    let mut failures = Vec::new();
    for record in records {
        let name = names
            .get(&record.service_id)
            .cloned()
            .unwrap_or_else(|| record.service_id.clone());
        if record.manager_session_id == state.session_id {
            if state.processes.is_running(&record.service_id) {
                continue;
            }
            tokio::task::block_in_place(|| {
                state
                    .database
                    .lock()
                    .map_err(|_| "Database lock poisoned".to_string())?
                    .delete_active_process(&record.service_id)
                    .map_err(|error| error.to_string())
            })?;
            cleared.push(name);
            continue;
        }
        match process_manager::recover_orphaned_process(&record).await {
            Ok(process_manager::OrphanedProcessRecovery::Stopped)
            | Ok(process_manager::OrphanedProcessRecovery::AlreadyExited) => {
                tokio::task::block_in_place(|| {
                    state
                        .database
                        .lock()
                        .map_err(|_| "Database lock poisoned".to_string())?
                        .delete_active_process(&record.service_id)
                        .map_err(|error| error.to_string())
                })?;
                cleared.push(name);
            }
            Ok(process_manager::OrphanedProcessRecovery::Unverifiable) => unverifiable.push(name),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    Ok(serde_json::json!({
        "cleared": cleared,
        "unverifiable": unverifiable,
        "failures": failures,
    }))
}

#[tauri::command]
async fn stop_all_services(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>, String> {
    stop_all_from_state(&app, &state).await
}

async fn stop_all_from_state<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
) -> Result<Vec<String>, String> {
    let ids = state
        .processes
        .states()
        .into_iter()
        .filter(|state| {
            matches!(
                state.status.as_str(),
                "running" | "starting" | "stopping" | "waiting_to_restart"
            )
        })
        .map(|state| state.service_id)
        .collect::<Vec<_>>();
    stop_process_ids(app, state, ids).await
}

async fn stop_process_ids<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let running = state
        .processes
        .states()
        .into_iter()
        .filter(|state| {
            ids.contains(&state.service_id)
                && matches!(
                    state.status.as_str(),
                    "running" | "starting" | "stopping" | "waiting_to_restart"
                )
        })
        .map(|state| state.service_id)
        .collect::<Vec<_>>();
    let manager = state.processes.clone();
    let mut tasks = tokio::task::JoinSet::new();
    for id in running {
        let manager = manager.clone();
        let app = app.clone();
        tasks.spawn(async move {
            let result = manager.stop(&id, &app).await;
            (id, result)
        });
    }
    let mut failures = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((id, Err(error))) => failures.push(format!("{id}: {error}")),
            Ok((_, Ok(()))) => {}
            Err(error) => failures.push(format!("stop task failed: {error}")),
        }
    }
    Ok(failures)
}

#[tauri::command]
async fn quit_application(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let failures = stop_all_services(app.clone(), state).await?;
    if !failures.is_empty() {
        return Err(format!(
            "Could not stop all services: {}",
            failures.join("; ")
        ));
    }
    app.exit(0);
    Ok(())
}

const CONFIGURATION_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigurationFile {
    format_version: u32,
    #[serde(default)]
    services: Vec<Service>,
    #[serde(default)]
    settings: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReport {
    imported_services: usize,
    renamed_services: usize,
    dropped_dependencies: usize,
    dropped_icons: usize,
    dropped_secrets: usize,
    sanitized_directories: usize,
    imported_settings: usize,
}

fn configuration_json(state: &AppState) -> Result<String, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    serde_json::to_string_pretty(&ConfigurationFile {
        format_version: CONFIGURATION_FORMAT_VERSION,
        services: database
            .list_services()
            .map_err(|error| error.to_string())?,
        settings: database
            .list_settings()
            .map_err(|error| error.to_string())?,
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_configuration(state: tauri::State<'_, AppState>) -> Result<String, String> {
    configuration_json(&state)
}

#[tauri::command]
fn export_configuration_to(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let json = configuration_json(&state)?;
    std::fs::write(path, json).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_configuration_from(
    path: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ImportReport, String> {
    let contents = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let configuration: ConfigurationFile = serde_json::from_str(&contents)
        .map_err(|error| format!("Invalid configuration file: {error}"))?;
    if configuration.format_version != CONFIGURATION_FORMAT_VERSION {
        return Err(format!(
            "Unsupported configuration format version {}",
            configuration.format_version
        ));
    }
    let report = import_configuration(configuration, &state)?;
    refresh_tray_menu(&app);
    Ok(report)
}

fn unique_import_name(base: &str, used: &mut HashSet<String>) -> (String, bool) {
    let base = base.trim();
    let lowercase = base.to_lowercase();
    if !used.contains(&lowercase) {
        used.insert(lowercase);
        return (base.to_string(), false);
    }
    let mut index = 1_u32;
    loop {
        let suffix = if index == 1 {
            " (imported)".to_string()
        } else {
            format!(" (imported {index})")
        };
        let keep = 120_usize.saturating_sub(suffix.chars().count());
        let candidate = format!("{}{suffix}", base.chars().take(keep).collect::<String>());
        let lowercase = candidate.to_lowercase();
        if !used.contains(&lowercase) {
            used.insert(lowercase);
            return (candidate, true);
        }
        index += 1;
    }
}

fn import_configuration(
    configuration: ConfigurationFile,
    state: &AppState,
) -> Result<ImportReport, String> {
    let mut report = ImportReport {
        imported_services: 0,
        renamed_services: 0,
        dropped_dependencies: 0,
        dropped_icons: 0,
        dropped_secrets: 0,
        sanitized_directories: 0,
        imported_settings: 0,
    };
    let mut database = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    let existing_services = database
        .list_services()
        .map_err(|error| error.to_string())?;
    let existing_ids = existing_services
        .iter()
        .map(|service| service.id.clone())
        .collect::<HashSet<_>>();
    let mut used_names = existing_services
        .iter()
        .map(|service| service.name.to_lowercase())
        .collect::<HashSet<_>>();

    let mut id_map = HashMap::new();
    let mut planned = Vec::with_capacity(configuration.services.len());
    for mut service in configuration.services {
        let old_id = service.id.clone();
        let (name, renamed) = unique_import_name(&service.name, &mut used_names);
        if renamed {
            report.renamed_services += 1;
        }
        service.id = uuid::Uuid::new_v4().to_string();
        service.name = name;
        if service.icon_path.take().is_some() {
            report.dropped_icons += 1;
        }
        let secret_count = service
            .environment
            .iter()
            .filter(|variable| variable.is_secret)
            .count();
        if secret_count > 0 {
            report.dropped_secrets += secret_count;
            service.environment.retain(|variable| !variable.is_secret);
        }
        if let Some(directory) = &service.working_directory
            && !std::path::Path::new(directory).is_dir()
        {
            service.working_directory = None;
            report.sanitized_directories += 1;
        }
        let dependencies = std::mem::take(&mut service.depends_on);
        service.created_at = String::new();
        service.updated_at = String::new();
        id_map.insert(old_id, service.id.clone());
        planned.push((service, dependencies));
    }

    for (service, _) in &planned {
        service
            .validate()
            .map_err(|error| format!("Imported service {} is invalid: {error}", service.name))?;
    }

    let mut known_ids = existing_ids.clone();
    known_ids.extend(planned.iter().map(|(service, _)| service.id.clone()));
    let mut proposed_edges = database
        .list_dependencies()
        .map_err(|error| error.to_string())?;
    let mut mapped_dependencies = Vec::with_capacity(planned.len());
    for (service, dependencies) in &planned {
        let mut mapped = dependencies
            .iter()
            .filter_map(|dependency| {
                let mapped = id_map.get(dependency).cloned().or_else(|| {
                    existing_ids
                        .contains(dependency)
                        .then(|| dependency.clone())
                });
                if mapped.is_none() {
                    report.dropped_dependencies += 1;
                }
                mapped
            })
            .collect::<Vec<_>>();
        mapped.sort();
        mapped.dedup();
        debug_assert!(
            mapped
                .iter()
                .all(|dependency| known_ids.contains(dependency))
        );
        for dependency in &mapped {
            proposed_edges.push((service.id.clone(), dependency.clone()));
        }
        mapped_dependencies.push(mapped);
    }
    dependencies::validate_edges(&proposed_edges)?;

    for (service, _) in &planned {
        database
            .insert_service(service)
            .map_err(|error| error.to_string())?;
        report.imported_services += 1;
    }
    for ((service, _), dependencies) in planned.iter().zip(mapped_dependencies) {
        if dependencies.is_empty() {
            continue;
        }
        let mut updated = service.clone();
        updated.depends_on = dependencies;
        database
            .update_service(&updated)
            .map_err(|error| error.to_string())?;
    }

    for (key, value) in configuration.settings {
        if key == "startAtLogin" {
            continue;
        }
        database
            .set_setting(&key, &value)
            .map_err(|error| error.to_string())?;
        report.imported_settings += 1;
    }
    if let Some(days) = database
        .get_setting("logRetentionDays")
        .ok()
        .flatten()
        .and_then(|value| value.as_u64())
    {
        state.processes.set_log_retention_days(days);
    }
    Ok(report)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    current_version: String,
    notes: Option<String>,
    date: Option<String>,
}

#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = tokio::time::timeout(Duration::from_secs(2), updater.check())
        .await
        .map_err(|_| "Update check timed out.".to_string())?
        .map_err(|error| error.to_string())?;
    Ok(update.map(|update| UpdateInfo {
        version: update.version,
        current_version: update.current_version,
        notes: update.body,
        date: update.date.map(|date| date.to_string()),
    }))
}

#[tauri::command]
async fn install_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    if UPDATE_INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("An update is already being installed.".into());
    }
    let _guard = UpdateInstallGuard;

    let updater = app.updater().map_err(|error| error.to_string())?;
    let Some(update) = tokio::time::timeout(Duration::from_secs(2), updater.check())
        .await
        .map_err(|_| "Update check timed out.".to_string())?
        .map_err(|error| error.to_string())?
    else {
        return Err("No update is available.".into());
    };

    let _ = app.emit(
        "update-status",
        serde_json::json!({ "status": "downloading" }),
    );
    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    let bytes = update
        .download(
            move |chunk_length, content_length| {
                downloaded = downloaded.saturating_add(chunk_length as u64);
                let _ = progress_app.emit(
                    "update-progress",
                    serde_json::json!({
                        "downloaded": downloaded,
                        "total": content_length,
                    }),
                );
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())?;

    let _ = app.emit(
        "update-status",
        serde_json::json!({ "status": "installing" }),
    );
    let failures = stop_all_services(app.clone(), state).await?;
    if !failures.is_empty() {
        return Err(format!(
            "Could not stop all services before updating: {}",
            failures.join("; ")
        ));
    }
    update.install(&bytes).map_err(|error| error.to_string())?;

    let _ = app.emit(
        "update-status",
        serde_json::json!({ "status": "restarting" }),
    );
    app.restart();
}

#[tauri::command]
async fn start_service(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    start_service_from_state(id, &app, &state).await
}

async fn start_service_from_state<R: Runtime>(
    id: String,
    app: &tauri::AppHandle<R>,
    state: &AppState,
) -> Result<(), String> {
    let (services, edges) = service_graph(state)?;
    let order = dependencies::start_order(std::slice::from_ref(&id), &services, &edges)?;
    let by_id = services
        .into_iter()
        .map(|service| (service.id.clone(), service))
        .collect::<HashMap<_, _>>();
    for service_id in order {
        if state.processes.is_running(&service_id) {
            continue;
        }
        reconcile_active_process(state, &service_id).await?;
        let service = by_id.get(&service_id).cloned().ok_or("Service not found")?;
        state.processes.start(service.clone(), app.clone()).await?;
        wait_for_service_ready(state, &service).await?;
    }
    Ok(())
}

#[tauri::command]
async fn get_service_logs(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LogEvent>, String> {
    let exists = tokio::task::block_in_place(|| {
        state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?
            .get_service(&id)
            .map_err(|error| error.to_string())
            .map(|service| service.is_some())
    })?;
    if !exists {
        return Err("Service not found".into());
    }
    state.processes.load_logs(&id).await
}

#[tauri::command]
fn get_service_run_history(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RunHistory>, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    if database
        .get_service(&id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("Service not found".into());
    }
    database
        .list_run_history(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn stop_service(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.processes.stop(&id, &app).await
}

#[tauri::command]
async fn restart_service(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let (services, edges) = service_graph(&state)?;
    let order = dependencies::start_order(std::slice::from_ref(&id), &services, &edges)?;
    let by_id = services
        .into_iter()
        .map(|service| (service.id.clone(), service))
        .collect::<HashMap<_, _>>();
    reconcile_active_process(&state, &id).await?;
    state.processes.stop(&id, &app).await?;
    for service_id in order {
        if state.processes.is_running(&service_id) {
            continue;
        }
        reconcile_active_process(&state, &service_id).await?;
        let service = by_id.get(&service_id).cloned().ok_or("Service not found")?;
        state.processes.start(service.clone(), app.clone()).await?;
        wait_for_service_ready(&state, &service).await?;
    }
    Ok(())
}

#[tauri::command]
fn get_runtime_states(state: tauri::State<'_, AppState>) -> Vec<models::RuntimeState> {
    state.processes.states()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second instance must never run orphan recovery against services
        // owned by the instance that is already running.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::LogDir { file_name: None },
                )])
                .build(),
        )
        .setup(|app| {
            #[cfg(not(target_os = "macos"))]
            if let Some(window) = app.get_webview_window("main") {
                // macOS keeps the native title bar as a transparent overlay so
                // traffic lights and window corners stay native. Other
                // platforms use the custom Mac-style controls in the app bar.
                let _ = window.set_decorations(false);
            }
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = Database::open(data_dir.join("processhub.sqlite3"))?;
            if let Ok(services) = database.list_services() {
                let referenced = services
                    .into_iter()
                    .filter_map(|service| service.icon_path)
                    .map(std::path::PathBuf::from)
                    .collect::<HashSet<_>>();
                cleanup_unreferenced_icons(&data_dir.join("icons"), &referenced);
            }
            let retention_days = database
                .get_setting("logRetentionDays")?
                .and_then(|value| value.as_u64())
                .filter(|days| [7, 14, 30, 90].contains(days))
                .unwrap_or(14);
            log::info!("event=app_initialized");
            let process_manager = ProcessManager::default()
                .with_log_dir(data_dir.join("service-logs"))
                .with_log_retention_days(retention_days);
            // Resolve the login-shell PATH once at startup so spawned services
            // find user-installed tools (Homebrew, nvm, etc.) even when the
            // app is launched as a bundled .app outside a terminal.
            let pm_for_path = process_manager.clone();
            tauri::async_runtime::spawn(async move {
                pm_for_path.init_shell_path().await;
            });
            let start_minimized = database
                .get_setting("startMinimized")
                .ok()
                .flatten()
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            app.manage(AppState {
                database: Mutex::new(database),
                processes: process_manager,
                session_id: uuid::Uuid::new_v4().to_string(),
            });
            if start_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let menu = build_tray_menu(app.handle())?;
            let mut tray = tauri::tray::TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("ProcessHub")
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "start_auto" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = handle.state::<AppState>();
                            let _ = start_auto_start_services(handle.clone(), state).await;
                        });
                    }
                    "stop_all" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = handle.state::<AppState>();
                            let failures = stop_all_services(handle.clone(), state)
                                .await
                                .unwrap_or_else(|error| vec![error]);
                            let _ = handle.emit(
                                "tray-action-report",
                                serde_json::json!({"action":"stopAll", "failures":failures}),
                            );
                        });
                    }
                    "quit" => {
                        let has_running =
                            app.state::<AppState>()
                                .processes
                                .states()
                                .iter()
                                .any(|state| {
                                    matches!(
                                        state.status.as_str(),
                                        "running" | "starting" | "stopping" | "waiting_to_restart"
                                    )
                                });
                        if has_running {
                            let _ = app.emit("quit-confirmation-required", ());
                            show_main_window(app);
                        } else {
                            app.exit(0);
                        }
                    }
                    id if id.starts_with("service:") => {
                        let mut parts = id.splitn(3, ':');
                        let _ = parts.next();
                        let Some(service_id) = parts.next().map(str::to_string) else {
                            return;
                        };
                        let Some(action) = parts.next().map(str::to_string) else {
                            return;
                        };
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = handle.state::<AppState>();
                            match action.as_str() {
                                "start" => {
                                    let _ =
                                        start_service_from_state(service_id, &handle, &state).await;
                                }
                                "stop" => {
                                    let _ = state.processes.stop(&service_id, &handle).await;
                                }
                                "restart" => {
                                    let _ =
                                        restart_service(service_id, handle.clone(), state).await;
                                }
                                "details" => {
                                    show_main_window(&handle);
                                    let _ = handle.emit("open-service-details", &service_id);
                                }
                                "open" => {
                                    let url = state
                                        .processes
                                        .states()
                                        .into_iter()
                                        .find(|item| item.service_id == service_id)
                                        .and_then(|item| item.detected_url);
                                    if let Some(url) = url {
                                        let _ = handle.emit("tray-open-url", url);
                                    }
                                }
                                _ => {}
                            }
                        });
                    }
                    _ => {}
                });
            tray = tray
                .icon(tauri::include_image!("icons/tray-icon.png"))
                .icon_as_template(true);
            tray.build(app)?;

            let handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                let main_window = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let close_to_tray = handle
                            .state::<AppState>()
                            .database
                            .lock()
                            .ok()
                            .and_then(|database| database.get_setting("closeToTray").ok().flatten())
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true);
                        if close_to_tray {
                            api.prevent_close();
                            let _ = main_window.hide();
                            #[cfg(target_os = "macos")]
                            let _ = handle.set_dock_visibility(false);
                        } else {
                            api.prevent_close();
                            let _ = handle.emit("quit-confirmation-required", ());
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_services,
            check_for_update,
            install_update,
            set_service_secret,
            delete_service_secret,
            get_service,
            create_service,
            update_service,
            delete_service,
            import_service_icon,
            get_app_setting,
            set_app_setting,
            start_auto_start_services,
            start_all_services,
            restart_all_services,
            recover_orphaned_services,
            stop_all_services,
            quit_application,
            export_configuration,
            export_configuration_to,
            import_configuration_from,
            start_service,
            get_service_logs,
            get_service_run_history,
            stop_service,
            restart_service,
            get_runtime_states
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let has_running = app
                    .state::<AppState>()
                    .processes
                    .states()
                    .iter()
                    .any(|state| {
                        matches!(
                            state.status.as_str(),
                            "running" | "starting" | "stopping" | "waiting_to_restart"
                        )
                    });
                if has_running {
                    api.prevent_exit();
                    let _ = app.emit("quit-confirmation-required", ());
                    show_main_window(app);
                }
            }
        });
}

fn status_dot(status: &str) -> &'static str {
    match status {
        "running" => "●",
        "failed" => "✕",
        "waiting_to_restart" => "◷",
        "starting" | "stopping" => "◐",
        _ => "○",
    }
}

fn format_tray_status(status: &str) -> String {
    let mut chars = status.replace('_', " ").chars().collect::<Vec<_>>();
    if let Some(first) = chars.first_mut() {
        *first = first.to_ascii_uppercase();
    }
    chars.into_iter().collect()
}

fn build_tray_menu<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let state = app.state::<AppState>();
    let services = state
        .database
        .lock()
        .ok()
        .and_then(|database| database.list_services().ok())
        .unwrap_or_default();
    let runtime_by_id: std::collections::HashMap<String, models::RuntimeState> = state
        .processes
        .states()
        .into_iter()
        .map(|item| (item.service_id.clone(), item))
        .collect();

    let menu = tauri::menu::Menu::new(app)?;
    let show_item =
        tauri::menu::MenuItem::with_id(app, "show", "Open ProcessHub", true, None::<&str>)?;
    menu.append(&show_item)?;
    menu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;

    if services.is_empty() {
        let empty_item = tauri::menu::MenuItem::with_id(
            app,
            "no_services",
            "No services configured",
            false,
            None::<&str>,
        )?;
        menu.append(&empty_item)?;
    } else {
        for service in services {
            let runtime = runtime_by_id.get(&service.id);
            let status = runtime
                .map(|item| item.status.as_str())
                .unwrap_or("stopped");
            let active = matches!(
                status,
                "running" | "starting" | "stopping" | "waiting_to_restart"
            );
            let dot = status_dot(status);
            let label = match runtime.and_then(|item| item.pid) {
                Some(pid) => format!(
                    "{dot} {} — {} (PID {pid})",
                    service.name,
                    format_tray_status(status),
                ),
                None => format!("{dot} {} — {}", service.name, format_tray_status(status)),
            };
            let submenu = tauri::menu::Submenu::with_id(
                app,
                format!("service-menu:{}", service.id),
                label,
                true,
            )?;
            let start = tauri::menu::MenuItem::with_id(
                app,
                format!("service:{}:start", service.id),
                "Start",
                !active,
                None::<&str>,
            )?;
            let stop = tauri::menu::MenuItem::with_id(
                app,
                format!("service:{}:stop", service.id),
                "Stop",
                active,
                None::<&str>,
            )?;
            let restart = tauri::menu::MenuItem::with_id(
                app,
                format!("service:{}:restart", service.id),
                "Restart",
                true,
                None::<&str>,
            )?;
            let details = tauri::menu::MenuItem::with_id(
                app,
                format!("service:{}:details", service.id),
                "Open details",
                true,
                None::<&str>,
            )?;
            let has_url = runtime
                .and_then(|item| item.detected_url.as_ref())
                .is_some();
            let open = tauri::menu::MenuItem::with_id(
                app,
                format!("service:{}:open", service.id),
                "Open URL",
                has_url,
                None::<&str>,
            )?;
            submenu.append(&start)?;
            submenu.append(&stop)?;
            submenu.append(&restart)?;
            submenu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
            submenu.append(&details)?;
            submenu.append(&open)?;
            menu.append(&submenu)?;
        }
    }

    menu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
    let start_item = tauri::menu::MenuItem::with_id(
        app,
        "start_auto",
        "Start Auto-start Services",
        true,
        None::<&str>,
    )?;
    let stop_item =
        tauri::menu::MenuItem::with_id(app, "stop_all", "Stop All Services", true, None::<&str>)?;
    let quit_item =
        tauri::menu::MenuItem::with_id(app, "quit", "Quit ProcessHub…", true, None::<&str>)?;
    menu.append(&start_item)?;
    menu.append(&stop_item)?;
    menu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit_item)?;
    Ok(menu)
}

pub(crate) fn refresh_tray_menu<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    if let Ok(menu) = build_tray_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn show_main_window<R: Runtime>(app: &tauri::AppHandle<R>) {
    #[cfg(target_os = "macos")]
    let _ = app.set_dock_visibility(true);

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub(crate) fn notify_if_hidden<R: Runtime>(app: &tauri::AppHandle<R>, title: &str, body: &str) {
    let visible = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if visible {
        return;
    }
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        log::warn!("event=notification_failed error={}", error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::models::EnvironmentVariable;

    fn sample_service(name: &str) -> Service {
        Service::from_create(CreateService {
            name: name.into(),
            icon_path: None,
            executable: "/bin/echo".into(),
            arguments: vec![],
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
        })
        .expect("create sample service")
    }

    #[test]
    fn imports_configuration_with_remapped_names_and_dependencies() {
        let mut database = Database::open(":memory:").expect("open database");
        database
            .insert_service(&sample_service("API"))
            .expect("insert existing service");

        let dependency = sample_service("Database");
        let mut api = sample_service("API");
        api.depends_on = vec![dependency.id.clone()];
        let configuration = ConfigurationFile {
            format_version: 1,
            services: vec![dependency, api],
            settings: HashMap::from([
                ("logRetentionDays".to_string(), serde_json::json!(7)),
                ("startAtLogin".to_string(), serde_json::json!(false)),
            ]),
        };
        let state = AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id: "test-session".into(),
        };

        let report = import_configuration(configuration, &state).expect("import configuration");
        assert_eq!(report.imported_services, 2);
        assert_eq!(report.renamed_services, 1);
        assert_eq!(report.imported_settings, 1);

        let services = state
            .database
            .lock()
            .unwrap()
            .list_services()
            .expect("list services");
        assert_eq!(services.len(), 3);
        let imported_api = services
            .iter()
            .find(|service| service.name == "API (imported)")
            .expect("renamed API");
        let imported_database = services
            .iter()
            .find(|service| service.name == "Database")
            .expect("imported database");
        assert_eq!(imported_api.depends_on, vec![imported_database.id.clone()]);
        assert_eq!(
            state
                .database
                .lock()
                .unwrap()
                .get_setting("logRetentionDays")
                .unwrap(),
            Some(serde_json::json!(7))
        );
    }

    #[test]
    fn imported_icons_are_copied_into_app_managed_storage() {
        let root = std::env::temp_dir().join(format!("processhub-icons-{}", uuid::Uuid::new_v4()));
        let source = root.join("source").join("custom.PNG");
        let managed = root.join("app-data").join("icons");
        std::fs::create_dir_all(source.parent().unwrap()).expect("create source directory");
        let png_signature = b"\x89PNG\r\n\x1a\nfixture";
        std::fs::write(&source, png_signature).expect("write fixture");

        let imported = copy_icon_to_directory(&source, &managed).expect("import icon");
        assert_ne!(imported, source);
        assert_eq!(
            imported.extension().and_then(std::ffi::OsStr::to_str),
            Some("png")
        );
        assert_eq!(std::fs::read(&imported).unwrap(), png_signature);

        let invalid = root.join("bad.svg");
        std::fs::write(&invalid, b"<svg/>").expect("write invalid fixture");
        assert!(copy_icon_to_directory(&invalid, &managed).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn stale_same_session_records_are_cleared_without_signalling() {
        let app = tauri::test::mock_app();
        let service = Service::from_create(CreateService {
            name: "Stale record".into(),
            icon_path: None,
            executable: "/bin/sh".into(),
            arguments: vec![],
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
        })
        .expect("create service");
        let mut database = Database::open(":memory:").expect("open database");
        database.insert_service(&service).expect("save service");
        let session_id = uuid::Uuid::new_v4().to_string();
        database
            .save_active_process(&models::ActiveProcessRecord {
                service_id: service.id.clone(),
                manager_session_id: session_id.clone(),
                pid: 99_999_999,
                process_group_id: 99_999_999,
                process_start_time_s: 1,
                executable_path: "/bin/sh".into(),
            })
            .expect("save stale record");
        app.manage(AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id,
        });

        let report = recover_orphaned_services(app.state::<AppState>())
            .await
            .expect("recover orphans");
        assert_eq!(report["cleared"], serde_json::json!([service.name]));
        assert!(
            app.state::<AppState>()
                .database
                .lock()
                .unwrap()
                .get_active_process(&service.id)
                .unwrap()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn unverifiable_previous_session_records_are_retained() {
        let app = tauri::test::mock_app();
        let service = Service::from_create(CreateService {
            name: "Unverifiable record".into(),
            icon_path: None,
            executable: "/bin/sh".into(),
            arguments: vec![],
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
        })
        .expect("create service");
        let mut database = Database::open(":memory:").expect("open database");
        database.insert_service(&service).expect("save service");
        database
            .save_active_process(&models::ActiveProcessRecord {
                service_id: service.id.clone(),
                manager_session_id: "previous-session".into(),
                pid: 1,
                process_group_id: 1,
                process_start_time_s: 1,
                executable_path: "/bin/sh".into(),
            })
            .expect("save unsafe record");
        app.manage(AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
        });

        let report = recover_orphaned_services(app.state::<AppState>())
            .await
            .expect("inspect orphans");
        assert_eq!(report["unverifiable"], serde_json::json!([service.name]));
        assert!(
            app.state::<AppState>()
                .database
                .lock()
                .unwrap()
                .get_active_process(&service.id)
                .unwrap()
                .is_some()
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn deleting_a_running_service_stops_it_then_removes_its_configuration() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let database = Database::open(":memory:").expect("open database");
        let service = Service::from_create(CreateService {
            name: "Delete test".into(),
            icon_path: None,
            executable: "/bin/sh".into(),
            arguments: vec!["-c".into(), "exec sleep 30".into()],
            working_directory: None,
            environment: vec![EnvironmentVariable {
                key: "MODE".into(),
                value: "test".into(),
                is_secret: false,
            }],
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
        })
        .expect("create service");
        let id = service.id.clone();
        let mut database = database;
        database.insert_service(&service).expect("save service");
        app.manage(AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
        });

        app.state::<AppState>()
            .processes
            .start(service, handle.clone())
            .await
            .expect("start service");
        let app_state = app.state::<AppState>();
        let deleted = delete_service_from_state(id.clone(), &handle, &app_state)
            .await
            .expect("delete service");
        assert!(deleted);
        let state = app.state::<AppState>();
        assert!(
            state
                .database
                .lock()
                .unwrap()
                .get_service(&id)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            state
                .processes
                .states()
                .iter()
                .filter(|item| item.service_id == id)
                .count(),
            0,
            "deleted service is removed from the runtime registry"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn starting_a_service_starts_its_dependencies_first() {
        let app = tauri::test::mock_app();
        let handle = app.handle().clone();
        let mut database = Database::open(":memory:").expect("open database");
        let dependency = Service::from_create(CreateService {
            name: "Dependency".into(),
            icon_path: None,
            executable: "/bin/sh".into(),
            arguments: vec!["-c".into(), "exec sleep 30".into()],
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
        })
        .expect("create dependency");
        let mut dependent = Service::from_create(CreateService {
            name: "Dependent".into(),
            icon_path: None,
            executable: "/bin/sh".into(),
            arguments: vec!["-c".into(), "exec sleep 30".into()],
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
        })
        .expect("create dependent");
        dependent.depends_on = vec![dependency.id.clone()];
        database
            .insert_service(&dependency)
            .expect("insert dependency");
        database
            .insert_service(&dependent)
            .expect("insert dependent");
        app.manage(AppState {
            database: Mutex::new(database),
            processes: ProcessManager::default(),
            session_id: uuid::Uuid::new_v4().to_string(),
        });

        let app_state = app.state::<AppState>();
        start_service_from_state(dependent.id.clone(), &handle, &app_state)
            .await
            .expect("start dependency tree");
        let state = app.state::<AppState>();
        assert!(state.processes.is_running(&dependency.id));
        assert!(state.processes.is_running(&dependent.id));

        state
            .processes
            .stop(&dependent.id, &handle)
            .await
            .expect("stop dependent");
        state
            .processes
            .stop(&dependency.id, &handle)
            .await
            .expect("stop dependency");
    }

    #[test]
    fn startup_cleanup_removes_only_unreferenced_icons() {
        let root = std::env::temp_dir().join(format!("processhub-icons-{}", uuid::Uuid::new_v4()));
        let icons = root.join("icons");
        std::fs::create_dir_all(&icons).expect("create icons");
        let keep = icons.join("keep.png");
        let orphan = icons.join("orphan.png");
        std::fs::write(&keep, b"keep").expect("write kept icon");
        std::fs::write(&orphan, b"orphan").expect("write orphan icon");
        cleanup_unreferenced_icons(&icons, &HashSet::from([keep.clone()]));
        assert!(keep.exists());
        assert!(!orphan.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deleting_a_service_removes_only_managed_files() {
        let root =
            std::env::temp_dir().join(format!("processhub-cleanup-{}", uuid::Uuid::new_v4()));
        let data = root.join("app-data");
        let icons = data.join("icons");
        let logs = data.join("service-logs");
        std::fs::create_dir_all(&icons).expect("create icons");
        std::fs::create_dir_all(&logs).expect("create logs");
        let icon = icons.join("service.png");
        std::fs::write(&icon, b"icon").expect("write icon");
        let outside = root.join("outside.png");
        std::fs::write(&outside, b"outside").expect("write outside file");

        let service = Service::from_create(CreateService {
            name: "Cleanup test".into(),
            icon_path: Some(icon.to_string_lossy().into_owned()),
            executable: "/bin/echo".into(),
            arguments: vec![],
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
        })
        .expect("create service");
        let active_log = logs.join(format!("{}.log", service.id));
        let rotated_log = logs.join(format!("{}.log.2", service.id));
        std::fs::write(&active_log, b"log").expect("write active log");
        std::fs::write(&rotated_log, b"log").expect("write rotated log");

        remove_service_files(&service, &data);
        assert!(!icon.exists(), "managed icon is removed");
        assert!(!active_log.exists(), "active log is removed");
        assert!(!rotated_log.exists(), "rotated log is removed");
        assert!(outside.exists(), "unmanaged files are untouched");

        let mut unmanaged_icon = service;
        unmanaged_icon.icon_path = Some(outside.to_string_lossy().into_owned());
        remove_service_files(&unmanaged_icon, &data);
        assert!(outside.exists(), "icons outside app data are untouched");

        let _ = std::fs::remove_dir_all(root);
    }
}
