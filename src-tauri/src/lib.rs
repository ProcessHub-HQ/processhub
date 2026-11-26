mod database;
mod models;
mod process_manager;

use std::sync::Mutex;
use tauri::{Emitter, Manager, Runtime};

use database::Database;
use models::{CreateService, LogEvent, RunHistory, Service};
use process_manager::ProcessManager;

pub(crate) struct AppState {
    pub(crate) database: Mutex<Database>,
    pub(crate) processes: ProcessManager,
    pub(crate) session_id: String,
}

/// Clears bookkeeping for a process that already exited, or verifies and stops
/// a process left behind by a previous ProcessHub session.
async fn reconcile_active_process(state: &AppState, service_id: &str) -> Result<(), String> {
    let record = {
        let database = state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        database
            .get_active_process(service_id)
            .map_err(|error| error.to_string())?
    };
    let Some(record) = record else {
        return Ok(());
    };
    if record.manager_session_id == state.session_id {
        if state.processes.is_running(service_id) {
            return Ok(());
        }
        state
            .database
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?
            .delete_active_process(service_id)
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    match process_manager::recover_orphaned_process(&record).await? {
        process_manager::OrphanedProcessRecovery::Stopped
        | process_manager::OrphanedProcessRecovery::AlreadyExited => {
            state
                .database
                .lock()
                .map_err(|_| "Database lock poisoned".to_string())?
                .delete_active_process(service_id)
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        process_manager::OrphanedProcessRecovery::Unverifiable => Err(format!(
            "A process from a previous ProcessHub session (PID {}) could not be verified. It was left untouched; stop it manually before using this service.",
            record.pid
        )),
    }
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
    state: tauri::State<'_, AppState>,
) -> Result<Service, String> {
    let service = Service::from_create(input).map_err(|error| error.to_string())?;
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
        .ok_or_else(|| "Created service could not be loaded".into())
}

#[tauri::command]
fn update_service(service: Service, state: tauri::State<'_, AppState>) -> Result<Service, String> {
    service.validate().map_err(|error| error.to_string())?;
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
        .ok_or_else(|| "Updated service could not be loaded".into())
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
    state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .delete_service(&id)
        .map_err(|error| error.to_string())
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
fn duplicate_service(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<Service>, String> {
    state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .duplicate_service(&id)
        .map_err(|error| error.to_string())
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

#[tauri::command]
async fn start_auto_start_services(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let services = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .list_services()
        .map_err(|error| error.to_string())?;
    let mut started = Vec::new();
    let mut failures = Vec::new();
    for service in services {
        if !service.enabled || !service.auto_start || state.processes.is_running(&service.id) {
            continue;
        }
        if let Err(error) = reconcile_active_process(&state, &service.id).await {
            failures.push(format!("{}: {error}", service.name));
            continue;
        }
        match state.processes.start(service.clone(), app.clone()).await {
            Ok(()) => started.push(service.name),
            Err(error) => failures.push(format!("{}: {error}", service.name)),
        }
    }
    let report = serde_json::json!({ "started": started, "failures": failures });
    let _ = app.emit("startup-report", report.clone());
    Ok(report)
}

#[tauri::command]
async fn recover_orphaned_services(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let (records, names) = {
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
        (
            database
                .list_active_processes()
                .map_err(|error| error.to_string())?,
            names,
        )
    };
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
            state
                .database
                .lock()
                .map_err(|_| "Database lock poisoned".to_string())?
                .delete_active_process(&record.service_id)
                .map_err(|error| error.to_string())?;
            cleared.push(name);
            continue;
        }
        match process_manager::recover_orphaned_process(&record).await {
            Ok(process_manager::OrphanedProcessRecovery::Stopped)
            | Ok(process_manager::OrphanedProcessRecovery::AlreadyExited) => {
                state
                    .database
                    .lock()
                    .map_err(|_| "Database lock poisoned".to_string())?
                    .delete_active_process(&record.service_id)
                    .map_err(|error| error.to_string())?;
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
    let running = state
        .processes
        .states()
        .into_iter()
        .filter(|state| matches!(state.status.as_str(), "running" | "starting" | "stopping"))
        .map(|state| state.service_id)
        .collect::<Vec<_>>();
    let mut failures = Vec::new();
    for id in running {
        if let Err(error) = state.processes.stop(&id, &app).await {
            failures.push(format!("{id}: {error}"));
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

#[tauri::command]
fn export_configuration(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let database = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;
    serde_json::to_string_pretty(&serde_json::json!({
        "formatVersion": 1,
        "services": database.list_services().map_err(|error| error.to_string())?,
        "settings": database.list_settings().map_err(|error| error.to_string())?,
    }))
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_service(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    reconcile_active_process(&state, &id).await?;
    let service = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .get_service(&id)
        .map_err(|error| error.to_string())?
        .ok_or("Service not found")?;
    state.processes.start(service, app).await
}

#[tauri::command]
async fn get_service_logs(
    id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LogEvent>, String> {
    let exists = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .get_service(&id)
        .map_err(|error| error.to_string())?
        .is_some();
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
    reconcile_active_process(&state, &id).await?;
    let service = state
        .database
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?
        .get_service(&id)
        .map_err(|error| error.to_string())?
        .ok_or("Service not found")?;
    state.processes.stop(&id, &app).await?;
    state.processes.start(service, app).await
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
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = Database::open(data_dir.join("processhub.sqlite3"))?;
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
            app.manage(AppState {
                database: Mutex::new(database),
                processes: process_manager,
                session_id: uuid::Uuid::new_v4().to_string(),
            });
            // Window starts hidden (visible:false in tauri.conf.json).
            // On macOS, hide the Dock icon until the user opens the window.
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let show_item =
                tauri::menu::MenuItem::with_id(app, "show", "Open ProcessHub", true, None::<&str>)?;
            let start_item = tauri::menu::MenuItem::with_id(
                app,
                "start_auto",
                "Start Auto-start Services",
                true,
                None::<&str>,
            )?;
            let stop_item = tauri::menu::MenuItem::with_id(
                app,
                "stop_all",
                "Stop All Services",
                true,
                None::<&str>,
            )?;
            let quit_item = tauri::menu::MenuItem::with_id(
                app,
                "quit",
                "Quit ProcessHub…",
                true,
                None::<&str>,
            )?;
            let menu = tauri::menu::Menu::with_items(
                app,
                &[&show_item, &start_item, &stop_item, &quit_item],
            )?;
            let mut tray = tauri::tray::TrayIconBuilder::new()
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
                                        "running" | "starting" | "stopping"
                                    )
                                });
                        if has_running {
                            let _ = app.emit("quit-confirmation-required", ());
                            show_main_window(app);
                        } else {
                            app.exit(0);
                        }
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
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
            get_service,
            create_service,
            update_service,
            delete_service,
            import_service_icon,
            duplicate_service,
            get_app_setting,
            set_app_setting,
            start_auto_start_services,
            recover_orphaned_services,
            stop_all_services,
            quit_application,
            export_configuration,
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
                        matches!(state.status.as_str(), "running" | "starting" | "stopping")
                    });
                if has_running {
                    api.prevent_exit();
                    let _ = app.emit("quit-confirmation-required", ());
                    show_main_window(app);
                }
            }
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::models::EnvironmentVariable;

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

    #[tokio::test]
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
            group_id: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
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
    #[tokio::test]
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
            group_id: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
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
    #[tokio::test]
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
            group_id: None,
            enabled: true,
            auto_start: false,
            restart_policy: "never".into(),
            restart_delay_ms: 1000,
            max_restart_attempts: Some(5),
            priority: "normal".into(),
            stop_grace_period_ms: 3000,
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
                .find(|item| item.service_id == id)
                .unwrap()
                .status,
            "stopped"
        );
    }
}
