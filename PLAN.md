# ProcessHub — Detailed Product & Engineering Plan
**Cross-platform process and service manager built with Tauri 2, React, TypeScript, and Rust**

- **Document type:** Product requirements + UX specification + technical architecture + phased implementation roadmap
- **Target platforms:** Windows, macOS, Linux
- **Initial audience:** Developers and technical users
- **Working product name:** ProcessHub
- **Status:** Initial Tauri scaffold created; implementation not yet verified
- **Version:** 1.1
- **Date:** 25 November 2026

---

## 1. Product vision

ProcessHub is a desktop application for creating, starting, stopping, restarting, and monitoring local processes and developer services from one graphical interface.

A user should be able to define a service once—for example, a Next.js development server, Laravel application, Redis instance, Mailpit, a custom script, or another executable—and then manage it without repeatedly opening terminals and remembering commands.

ProcessHub should feel like a lightweight local control center:

- Add a service with a name, icon, executable, arguments, working directory, and environment variables.
- Start, stop, restart, edit, duplicate, and delete services.
- Keep services running while the UI is minimized to the system tray.
- Optionally start selected services when the user logs into the operating system.
- View live stdout/stderr output and service status.
- Monitor process CPU and memory usage.
- Configure restart behaviour and basic health checks.
- Keep the interface responsive while many processes run.

### 1.1 Product principles

1. **Local-first:** no account, cloud backend, or network service is required.
2. **Safe by default:** commands are shown clearly; no automatic administrator/root elevation.
3. **Explicit execution:** executable, arguments, working directory, and environment are first-class configuration fields.
4. **Responsive:** process operations and log streaming must never block the UI.
5. **Predictable lifecycle:** the user can understand what happens when the app closes, the OS shuts down, or a child process exits.
6. **Cross-platform honesty:** platform-specific limitations are visible and documented rather than hidden.
7. **Useful without advanced setup:** a user can add and run a basic command in under a minute.

---

## 2. Confirmed product decisions

These decisions are the current planning baseline. Revisit them only if implementation reveals a strong reason.

| Area | Decision |
|---|---|
| Desktop framework | Tauri 2 |
| Project scaffold | `create-tauri-app` |
| Project directory / package name | `processhub` |
| Tauri application identifier | `com.aakashhq.processhub` |
| Frontend | React + TypeScript |
| Package manager | pnpm |
| UI template / flavor | React / TypeScript |
| Backend / process engine | Rust |
| Async runtime | Tokio |
| Local database | SQLite |
| Primary audience | Developers |
| Command support | Direct executables and optional shell commands |
| App close behaviour | Closing the main window should normally hide/minimize to tray; services continue while the app process remains alive |
| First-release logs | Included |
| First-release service groups | Excluded |
| First-release resource monitoring | Included |
| First-release startup integration | Included |
| Future architecture | Keep a separate supervisor daemon as a possible later phase, not an MVP requirement |

**Important lifecycle distinction:** “Close to tray” means the ProcessHub application remains running in the background. It does not mean services survive if the ProcessHub process itself is terminated, crashes, or the computer restarts. Independent supervision after the GUI exits is a later architectural enhancement.

---

## 3. Goals, non-goals, and success criteria

### 3.1 MVP goals

- Manage arbitrary user-configured local processes.
- Support direct executable + argument execution.
- Offer optional shell-command execution with a clearly labelled shell mode.
- Set a working directory for each service.
- Capture stdout and stderr separately, with a combined chronological view.
- Track process state, PID, start time, exit code, and runtime.
- Stop the managed process tree as reliably as possible on each OS.
- Persist service definitions and app settings in SQLite.
- Support tray behaviour and launch-at-login.
- Show basic CPU and memory metrics where available.
- Build installable packages for Windows, macOS, and Linux.

### 3.2 Not in the first release

- Running arbitrary services as system daemons or Windows Services.
- Root/admin process execution or privilege escalation.
- Docker/container orchestration.
- Remote server management.
- Cloud synchronization or user accounts.
- Full terminal emulator with interactive stdin, PTY, shell job control, and terminal escape rendering.
- Advanced cgroup CPU/memory enforcement.
- Automatic discovery of every service installed on the computer.
- Built-in package manager or runtime installer.
- Guaranteed process survival after ProcessHub itself exits or crashes.

### 3.3 MVP acceptance criteria

The MVP is ready for a public beta when:

- A user can create, edit, duplicate, disable, and delete a service.
- A service can be started, stopped, and restarted from the UI.
- A service's working directory and environment are applied correctly.
- Logs stream while the service runs and remain available after it stops, subject to retention settings.
- A service that exits unexpectedly is represented accurately.
- Stop handles descendants for the supported process-launch mode and OS implementation.
- A service can be configured to start when ProcessHub launches.
- ProcessHub can start at user login and open hidden/minimized to tray.
- The UI remains responsive with at least 20 lightweight test processes running.
- The app can be built and smoke-tested on all three target operating systems.
- No command is executed merely by opening or previewing its configuration.

---

## 4. Recommended technology stack

| Layer | Technology | Responsibility |
|---|---|---|
| Desktop shell | Tauri 2 | Native window, tray, app lifecycle, packaging, frontend/backend bridge |
| UI | React + TypeScript | Screens, forms, dashboards, interactions |
| Build tooling | Vite | Frontend development and production builds |
| Styling | Tailwind CSS | Layout, spacing, responsive design |
| Components | shadcn/ui or a consistent accessible component library | Dialogs, menus, inputs, tables, tooltips |
| Icons | Lucide | UI iconography |
| Backend | Rust | Configuration validation, process lifecycle, OS integration |
| Async runtime | Tokio | Non-blocking tasks, process I/O, timers, coordination |
| Persistence | SQLite | Service definitions, preferences, log metadata; legacy group schema retained for compatibility |
| Rust DB layer | SQLx or rusqlite | Database access and migrations; choose one and use consistently |
| Serialization | serde | Typed IPC and persisted configuration |
| System metrics | sysinfo (evaluate platform coverage) | CPU and memory measurements |
| Logging | tracing + tracing-subscriber | Structured application diagnostics |
| Frontend state | Zustand or Redux Toolkit | UI state; avoid duplicating backend runtime truth |
| Frontend data/cache | TanStack Query (optional) | Async request state and invalidation |
| Testing | Vitest, React Testing Library, Rust tests, Playwright/WebdriverIO where practical | Unit, integration, and UI tests |

### 4.1 Tauri 2 integration guidance

- Use Tauri commands for request/response operations such as create service, start service, stop service, and query status.
- Use Tauri events or a managed event channel for ongoing runtime updates and log chunks.
- Keep the process supervisor in Rust. Do not implement process spawning in the React frontend.
- Use narrowly scoped Tauri capabilities and permissions.
- Do not expose a generic unrestricted shell command API to the webview.
- Validate all arguments and paths in Rust, even if the UI already validates them.
- Treat frontend input as untrusted.
- Keep plugins limited to the capabilities actually required.
- Review the official Tauri 2 documentation and plugin compatibility before selecting plugins.

---

## 5. High-level architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                         ProcessHub UI                           │
│  Process workspace | Logs | Settings | Add/Edit             │
│                  React + TypeScript + Vite                    │
└──────────────────────────────┬───────────────────────────────┘
                               │ Tauri IPC
                Commands       │       Events / log stream
                               │
┌──────────────────────────────▼───────────────────────────────┐
│                       Tauri Application                       │
│  App lifecycle | Window/tray | Capability boundary | Events  │
├──────────────────────────────────────────────────────────────┤
│                     Rust Application State                    │
│  ServiceRepository | RuntimeRegistry | EventPublisher        │
├──────────────────────────────────────────────────────────────┤
│                    Process Supervisor Core                    │
│  Validation | Start/Stop/Restart | State machine | Policies    │
│  Process handles | Output readers | Exit watcher | Scheduler  │
├──────────────────────────────┬───────────────────────────────┤
│                              │                                │
▼                              ▼                                ▼
SQLite Repository        OS Process Adapter              Metrics Collector
Services                  Windows / macOS / Linux         CPU / RAM / PID
Settings / Log index     Tree termination                Sampling
```

### 5.1 Architectural boundaries

**Frontend responsibilities**
- Render service definitions and runtime state.
- Collect user input and display validation errors.
- Send typed requests to the Rust backend.
- Subscribe to runtime events and log events.
- Never decide that a process is running based only on a button click.

**Rust backend responsibilities**
- Validate configuration.
- Resolve execution strategy.
- Spawn and supervise processes.
- Capture output and exit status.
- Manage process-tree termination.
- Apply restart policies.
- Persist configuration and relevant runtime history.
- Publish authoritative state changes.

**Database responsibilities**
- Persist configuration and preferences.
- Persist log metadata and optionally log content.
- Never be treated as the source of truth for whether a process is currently alive.

**Runtime registry responsibilities**
- Keep in-memory process handles and current runtime state.
- Associate a service ID with its child process and supervisor tasks.
- Prevent conflicting operations on the same service.
- Reconcile stale runtime records after app startup.

---

## 6. Process execution and supervision design

This is the highest-risk and most important technical area. Build and test it before spending much time polishing the UI.

### 6.1 Execution modes

#### Mode A — Direct executable (default and recommended)

Example configuration:

```json
{
  "executable": "npm",
  "arguments": ["run", "dev"],
  "workingDirectory": "/projects/my-next-app"
}
```

The backend should invoke the executable directly with an argument vector. Do not concatenate values into a shell string.

Advantages:
- More predictable quoting.
- Less shell-injection risk.
- Easier validation.
- Better cross-platform behaviour.

#### Mode B — Shell command (optional advanced mode)

Example:

```text
cd /projects/my-next-app && npm run dev
```

The user explicitly chooses a shell mode. The backend selects the platform shell and passes the command using the appropriate argument form. The UI must show which shell is used.

Requirements:
- Clearly label this as shell execution.
- Do not silently switch direct execution into shell execution when a command fails.
- Document shell differences across PowerShell, cmd.exe, zsh, bash, and other shells.
- Avoid interpolating untrusted values into the shell command.
- Provide a preview of the exact command configuration before saving.
- Do not claim that shell syntax is portable across all operating systems.

### 6.2 Service lifecycle state machine

Use a backend-owned state machine. Suggested states:

```text
stopped
  │ start request
  ▼
starting
  │ child spawned successfully
  ▼
running
  │ stop request             │ child exits unexpectedly
  ▼                          ▼
stopping                   exited
  │ process tree ended       │ restart policy permits
  ▼                          ▼
stopped                    waiting_to_restart
                              │ delay elapsed
                              ▼
                           starting

Any operation may also transition to:
failed (configuration, spawn, monitoring, or unrecoverable operation error)
```

Suggested runtime states:
- `stopped`
- `starting`
- `running`
- `stopping`
- `exited`
- `waiting_to_restart`
- `failed`
- `unknown` (only for a temporary reconciliation condition)

Do not store transient runtime status as a permanent service configuration field.

### 6.3 Start operation

1. Receive `start_service(service_id)`.
2. Acquire a per-service operation lock.
3. Load the latest enabled service configuration.
4. Confirm the service is not already starting/running.
5. Validate executable, arguments, working directory, environment, and execution mode.
6. Resolve executable according to the documented PATH policy.
7. Create the child process with stdout/stderr piped.
8. Configure process-group or platform-specific containment.
9. Spawn the child.
10. Register its handle and PID in the runtime registry.
11. Set state to `running` only after successful spawn.
12. Start independent stdout and stderr reader tasks.
13. Start an exit watcher.
14. Publish a runtime-state event.
15. Release the operation lock when safe; ongoing monitoring is handled by owned tasks.

A process that spawns successfully but exits immediately should transition quickly to `exited` or `failed`, not remain visually “running.”

### 6.4 Stop operation

1. Acquire the per-service operation lock.
2. If already stopped, return an idempotent result.
3. Transition to `stopping`.
4. Request graceful termination of the managed process tree.
5. Wait for a configurable grace period.
6. If still alive, force terminate the managed process tree.
7. Wait for exit confirmation.
8. Stop monitoring tasks and finalize log streams.
9. Record exit information.
10. Transition to `stopped` for a user-requested stop.
11. Publish the final runtime event.

**Critical:** killing only the parent process is insufficient for commands such as `npm run dev`, which can spawn Node and other descendants. Implement and test process-tree containment separately on Windows, macOS, and Linux.

### 6.5 Platform-specific process-tree strategy

| OS | Initial strategy | Notes |
|---|---|---|
| Windows | Windows Job Objects | Assign child process to a Job Object; configure close/kill behaviour deliberately; test nested process creation and shutdown. |
| macOS | POSIX process groups / sessions | Spawn into a dedicated process group and signal the group; handle graceful signal and force termination. |
| Linux | POSIX process groups initially | Consider cgroups in a later advanced release; do not require elevated privileges for MVP. |

The exact implementation must be validated with real process-tree tests. Avoid promising perfect containment until those tests pass.

### 6.6 Concurrency and thread model

Do **not** create one dedicated OS thread per service.

Use Tokio for asynchronous coordination:
- One task to read stdout.
- One task to read stderr.
- One task to watch process exit.
- A metrics sampling task or shared sampling loop.
- A supervisor task for restart policy when needed.

Use bounded channels and backpressure-aware log handling. A noisy process must not create unbounded memory growth or freeze the UI.

Do not hold a global mutex while waiting for a process to exit. Use per-service locks and short critical sections.

### 6.7 Restart policies

MVP options:
- Never restart.
- Restart on failure.
- Always restart (including clean exit, if explicitly selected).

Configurable values:
- Initial restart delay.
- Maximum restart delay.
- Maximum attempts; optionally unlimited.
- Backoff strategy (fixed or exponential).
- Reset failure counter after a stable-running duration.
- Do not restart after an explicit user stop.
- Do not restart when the service is disabled.

Recommended default: **Never restart**, so a user's command does not unexpectedly loop. Offer “Restart on failure” as an explicit option.

Use exponential backoff with a cap for repeated failures. Avoid restart storms.

### 6.8 Process output and logs

Capture stdout and stderr independently, but render a combined timeline in the UI.

Each log event should include:
- Service ID.
- Stream (`stdout`, `stderr`, or internal).
- Timestamp.
- Text chunk or normalized line.
- Optional sequence number.

Requirements:
- Handle partial lines and large output chunks.
- Handle UTF-8 decoding safely; do not crash on invalid byte sequences.
- Preserve ordering as far as possible, while acknowledging stdout and stderr are separate streams.
- Use bounded buffers and log rotation.
- Provide pause/resume of UI rendering without pausing the underlying pipe readers.
- Support clear-view separately from delete-history.
- Avoid writing secrets from environment variables into logs.
- Do not store unlimited logs in SQLite.

Recommended first implementation: stream output to the UI and write rotated per-service log files under the application data directory. Store file metadata and retention information in SQLite. This is more suitable for larger logs than inserting every line into SQLite.

### 6.9 Metrics

MVP metrics:
- PID.
- Process state.
- CPU percentage (sampled).
- Memory usage.
- Start time and elapsed runtime.

Important details:
- Metrics may be unavailable or approximate on some OS versions.
- CPU percentage needs sampling over time; a single instantaneous reading is not meaningful.
- Decide whether to report only the root process or the whole managed process tree. Prefer process-tree totals where the platform implementation supports them reliably.
- Do not sample excessively. A 1–2 second interval is a reasonable initial experiment.
- Metrics collection must not delay process supervision.

### 6.10 Health checks

Keep health checks separate from process state.

Possible checks:
- Process exists.
- TCP port is accepting connections.
- HTTP endpoint returns an expected status range.
- Optional expected response text in a later phase.

A process can be alive but not ready. Show both:
- **Process:** Running
- **Health:** Starting / Healthy / Unhealthy / Not configured

For MVP, implement process state first and add simple TCP port checks only after core supervision is stable.

---

## 7. Data model and SQLite plan

Use UUIDs for persistent IDs. Store timestamps in UTC. Store paths as strings in a platform-aware way and avoid assuming `/` path separators.

### 7.1 `services`

| Column | Type | Notes |
|---|---|---|
| id | TEXT primary key | UUID |
| name | TEXT not null | Display name |
| description | TEXT nullable | Short description |
| icon_path | TEXT nullable | App-managed asset path |
| group_id | TEXT nullable | Foreign key to service_groups |
| execution_mode | TEXT not null | `direct` or `shell` |
| executable | TEXT not null | Executable or shell target |
| arguments_json | TEXT not null | JSON array of arguments |
| shell_command | TEXT nullable | Used only in shell mode |
| working_directory | TEXT nullable | Explicit cwd |
| enabled | INTEGER not null | Whether service is enabled |
| auto_start | INTEGER not null | Start when ProcessHub launches |
| start_minimized | INTEGER nullable | Optional future per-service setting |
| restart_policy | TEXT not null | `never`, `on_failure`, `always` |
| restart_delay_ms | INTEGER not null | Initial delay |
| max_restart_attempts | INTEGER nullable | Null can mean unlimited, if supported |
| priority | TEXT not null | `low`, `normal`, `high` |
| stop_grace_period_ms | INTEGER not null | Graceful shutdown wait |
| created_at | TEXT not null | UTC ISO-8601 |
| updated_at | TEXT not null | UTC ISO-8601 |
| sort_order | INTEGER not null | UI ordering |

### 7.2 `service_environment`

| Column | Type | Notes |
|---|---|---|
| id | TEXT primary key | UUID |
| service_id | TEXT not null | Foreign key |
| key | TEXT not null | Environment variable name |
| value | TEXT not null | Plain text only for non-secret values |
| is_secret | INTEGER not null | Secret marker; secret values should use OS secure storage |
| created_at | TEXT not null | UTC |

Do not store secrets in plaintext merely because the database is local. Evaluate Tauri's secure storage/keyring options for secret values. If secure storage is unavailable, warn the user and do not silently claim the value is encrypted.

### 7.3 Legacy `service_groups` compatibility table

This table is retained only to preserve existing databases. ProcessHub has no user-facing grouping feature and new services are not assigned to groups.

| Column | Type | Notes |
|---|---|---|
| id | TEXT primary key | UUID |
| name | TEXT not null | Group name |
| description | TEXT nullable | Optional |
| icon_path | TEXT nullable | Optional |
| sort_order | INTEGER not null | UI order |
| created_at | TEXT not null | UTC |
| updated_at | TEXT not null | UTC |

### 7.4 `service_dependencies`

| Column | Type | Notes |
|---|---|---|
| service_id | TEXT not null | Dependent service |
| depends_on_service_id | TEXT not null | Required service |
| dependency_type | TEXT not null | Initially `start_before` |
| primary key | (service_id, depends_on_service_id) | Prevent duplicates |


### 7.5 `app_settings`

| Column | Type | Notes |
|---|---|---|
| key | TEXT primary key | Setting name |
| value_json | TEXT not null | Typed JSON value |
| updated_at | TEXT not null | UTC |

Example settings:
- Theme.
- Start at login.
- Start minimized.
- Close button behaviour.
- Log retention days.
- Maximum log file size.
- Metrics sampling interval.
- Default shell.
- Default stop grace period.

### 7.6 Runtime data

Do not persist live process handles. On application startup:
- Initialize every service's runtime state as stopped/unknown.
- Verify whether any supported managed process can be reconciled.
- Do not assume a PID from an old session still belongs to the service; PIDs can be reused.
- Do not automatically kill a process merely because its PID matches a stale database record.

Persist historical run sessions only if useful:
- Service ID.
- PID.
- Started/ended timestamps.
- Exit code.
- Exit reason.
- Log file reference.

---

## 8. UX and visual design specification

### 8.1 Overall design direction

Create a focused developer utility, not a generic admin dashboard.

Visual characteristics:
- A light-only desktop app workspace with no sidebar, page header, or footer.
- Saved services appear as app items with a distinct icon, name, and readable runtime state.
- Right-click opens the actions for that item; keyboard users can open the same menu.
- A compact app bar contains the ProcessHub identity, New action, and Settings.
- The canvas stays fixed to the app viewport with no scrolling. The window enforces a minimum size that keeps the workspace usable.
- Restrained surfaces, consistent Lucide icons, visible keyboard focus, and clear loading/empty/error states.

Suggested navigation:
1. Overview
2. Services
4. Activity / Logs
5. Settings

### 8.2 Main application shell

**App bar**
- Left: ProcessHub logo and name.
- Right: a light New button and Settings gear.
- The Settings menu contains Startup settings and About. Startup settings controls launch-at-login, start-minimized, close-to-tray, and enabled-service startup; About shows the app icon, name, purpose, developer, and website.
- Light appearance only.

**Process workspace**
- App-like icon grid for saved services, using an icon and service name for every item.
- Single click selects an item; double click starts or stops it.
- Right-click opens Start/Stop, Restart, Edit, Duplicate, and Delete actions.
- Show a clear first-run empty state and error feedback without sample services.
- No “My processes” count/heading section, sidebar, page header, footer, or scrollbars.
- Enforce a minimum window size of 860 × 700 CSS pixels; use 1120 × 760 as the initial size.
- Keep key service actions visible without opening a detail page.

**System tray**
- Open ProcessHub.
- Start all auto-start services / start all enabled services (wording must be precise).
- Stop all services.
- Show running service list (if platform menu supports it).
- Quit ProcessHub and stop services.
- Quit ProcessHub and leave services running only if the architecture can truly support that; do not show this option in the MVP if it cannot be honoured.

### 8.3 Dashboard / Overview screen

Purpose: answer “What is running, and is anything wrong?” at a glance.

Sections:
- The process workspace and its saved app items.
- A manager health status in app error feedback.
- Quick Add process icon in the app bar.

Dashboard states:
- First-run empty state with a clear “Add your first service” action.
- Normal state with service summary.
- Warning state when one or more services failed.
- Loading state while initial configuration is loaded.
- Error state if database initialization fails.

### 8.4 Services list screen

Use the same app-item grid as the process workspace. Each item shows its icon, name, and process state. PID and full configuration belong in details or the context menu, not as always-visible table columns.

Interactions:
- Start or stop with a double click and through the right-click menu.
- Edit, duplicate, restart, and delete from the right-click menu.
- Confirm destructive deletion.

Use status text plus icon/colour; never rely on colour alone.

### 8.5 Service detail screen or drawer

Sections:
- Header: icon, name, status, primary action.
- Overview: PID, uptime, last exit code, auto-start.
- Configuration summary: executable, arguments, cwd.
- Resource usage.
- Live logs.
- Recent run history.
- Actions: Edit, Restart, Stop, Delete.

The detail view should make it easy to copy the command configuration and working directory.

### 8.6 Add/Edit service flow

Use a multi-section form or a short wizard. Do not overwhelm the user with every advanced option initially.

**Section 1 — Identity**
- Service name (required).
- Description (optional).
- Icon upload or icon picker.

**Section 2 — Command**
- Execution mode: Direct executable / Shell command.
- Executable.
- Arguments as individual entries, not a single ambiguous string.
- Working directory.
- “Browse” action for selecting a directory or executable where supported.
- Command preview.

**Section 3 — Environment**
- Key/value rows.
- Add/remove variable.
- Mask secret values.
- Explain environment inheritance and overrides.

**Section 4 — Behaviour**
- Start manually or when ProcessHub launches.
- Restart policy.
- Restart delay and maximum attempts.
- Stop grace period.
- Priority.
- Optional health check in a later phase.

**Section 5 — Review**
- Summary of what will run.
- Warnings for missing executable, invalid directory, shell mode, or unusual configuration.
- Save; do not automatically start unless the user explicitly chooses “Save and start.”

Validation:
- Name is required and unique within the app (case-insensitive recommended).
- Executable is required.
- Arguments must be preserved exactly as separate values.
- Working directory must exist when configured.
- Environment keys must be valid for the target OS.
- Reject invalid restart settings.
- Validate dependency relationships.
- Show backend validation errors beside the relevant field.

### 8.7 Logs screen

Requirements:
- Monospace text.
- Clear stdout/stderr distinction.
- Timestamp display.
- Auto-scroll toggle.
- Pause visual updates.
- Search within loaded logs.
- Copy selected text.
- Clear current view.
- Export log file.
- Indicate when older logs have been rotated or are no longer retained.

Avoid rendering every incoming chunk immediately in React. Batch UI updates (for example, 50–150 ms) and virtualize long log lists.

### 8.9 Settings screen

Sections:
- General: theme, language (future), confirm destructive actions.
- Startup: launch at login, start minimized, auto-start enabled services.
- Behaviour: close-to-tray, stop services on quit.
- Logs: location, retention, maximum size, export.
- Monitoring: sampling interval, metrics display.
- Shell: default shell and execution warning.
- Advanced: diagnostics, open data directory, reset configuration.
- About: version, licenses, update information (future).

### 8.10 Accessibility and interaction requirements

- Full keyboard navigation.
- Visible focus states.
- Labels for all form controls.
- Tooltips for icon-only buttons.
- Confirmation for deleting a service or stopping all services.
- Clear disabled/loading states during operations.
- Do not use colour as the only status indicator.
- Support reduced motion where possible.
- Keep destructive actions visually distinct from primary actions.

---

## 9. Frontend state and backend IPC contract

### 9.1 Source of truth

The Rust backend is authoritative for runtime state. The frontend may cache and render state, but it must reconcile with backend snapshots on startup and after reconnecting from tray.

### 9.2 Suggested command API

Configuration:
- `list_services`
- `get_service`
- `create_service`
- `update_service`
- `duplicate_service`
- `delete_service`
- `set_service_enabled`

Runtime:
- `start_service`
- `stop_service`
- `restart_service`
- `start_all_auto_start_services`
- `stop_all_services`
- `get_runtime_snapshot`
- `get_service_run_history`

Logs:
- `get_log_files`
- `read_log_range`
- `export_service_logs`
- `clear_service_logs`

Settings:
- `get_app_settings`
- `update_app_settings`
- `get_system_capabilities`
- `open_app_data_directory`

Use typed request and response structs. Avoid a single generic “execute arbitrary command” IPC endpoint.

### 9.3 Suggested events

- `service://state-changed`
- `service://log`
- `service://metrics`
- `service://restart-scheduled`
- `service://operation-error`
- `app://startup-complete`

Events should include stable service IDs, timestamps, and typed payloads. The frontend should subscribe once at the appropriate application boundary and unsubscribe cleanly.

### 9.4 Error model

Use structured errors, for example:
- `SERVICE_NOT_FOUND`
- `SERVICE_ALREADY_RUNNING`
- `SERVICE_ALREADY_STOPPED`
- `INVALID_CONFIGURATION`
- `EXECUTABLE_NOT_FOUND`
- `WORKING_DIRECTORY_NOT_FOUND`
- `SPAWN_FAILED`
- `PERMISSION_DENIED`
- `STOP_TIMEOUT`
- `PROCESS_TREE_TERMINATION_FAILED`
- `DATABASE_ERROR`
- `DEPENDENCY_CYCLE`
- `UNSUPPORTED_PLATFORM_FEATURE`

Display human-readable messages in the UI while retaining diagnostic details in application logs. Do not expose secrets or sensitive environment values in errors.

---

## 10. Security and safety requirements

Because ProcessHub executes commands, security is a core feature.

1. Treat every service configuration as executable code.
2. Never execute a service simply because a user opens its details or edits it.
3. Direct executable mode is the default.
4. Shell mode is explicit and visibly labelled.
5. Avoid unrestricted shell access from the webview.
6. Use Tauri capabilities with least privilege.
7. Validate paths and arguments in Rust.
8. Do not run as administrator/root by default.
9. Do not silently elevate privileges.
10. Never store secrets in plaintext logs.
11. Mask secret environment variables in the UI.
12. Do not include environment values in telemetry or crash reports.
13. Make log export a user-initiated action.
14. Protect local IPC if a daemon is added in the future.
15. Consider symlinks, path traversal, executable replacement, and working-directory changes.
16. Make “Stop all” and “Quit and stop services” explicit actions.
17. Do not kill unrelated processes based solely on a PID saved in an old session.

---

## 11. Project structure proposal

The exact structure can adapt to the blank Tauri template, but keep frontend and Rust concerns separated.

```text
processhub/
├── src/
│   ├── app/
│   │   ├── App.tsx
│   │   ├── router.tsx
│   │   └── providers.tsx
│   ├── components/
│   │   ├── layout/
│   │   ├── common/
│   │   ├── service/
│   │   ├── logs/
│   │   └── forms/
│   ├── features/
│   │   ├── dashboard/
│   │   ├── services/
│   │   ├── logs/
│   │   └── settings/
│   ├── hooks/
│   ├── lib/
│   │   ├── tauri.ts
│   │   ├── events.ts
│   │   └── validators.ts
│   ├── stores/
│   ├── types/
│   └── styles/
├── src-tauri/
│   ├── capabilities/
│   ├── migrations/
│   ├── icons/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── app_state.rs
│   │   ├── commands/
│   │   ├── database/
│   │   ├── models/
│   │   ├── process/
│   │   │   ├── mod.rs
│   │   │   ├── supervisor.rs
│   │   │   ├── lifecycle.rs
│   │   │   ├── output.rs
│   │   │   ├── restart.rs
│   │   │   ├── metrics.rs
│   │   │   └── platform/
│   │   │       ├── windows.rs
│   │   │       ├── macos.rs
│   │   │       └── linux.rs
│   │   ├── events/
│   │   ├── tray/
│   │   ├── settings/
│   │   └── errors.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── docs/
│   ├── architecture.md
│   ├── process-lifecycle.md
│   ├── security.md
│   └── testing.md
├── package.json
└── README.md
```

Do not create all these files as empty scaffolding on day one. Add modules as their phase begins.

---

## 11.1 Current repository baseline

The project has been scaffolded with the following choices:

```text
Project directory: processhub
App identifier:    com.aakashhq.processhub
Framework:         Tauri 2
Frontend:          React + TypeScript
Package manager:   pnpm
Scaffolder:        create-tauri-app
```

Expected first-run commands:

```bash
cd processhub
pnpm install
pnpm tauri dev
```

These commands are the next verification step, not a claim that the app has already been run successfully. Keep the generated scaffold's actual file names and scripts authoritative; template versions can change.

## 12. Phased implementation roadmap and progress tracking

### Progress tracking rules

- Mark a task `[x]` only when implementation and its relevant tests are complete.
- Mark `[~]` for work in progress.
- Leave `[ ]` for not started.
- Record platform-specific test results separately.
- A phase is complete when its implementation is delivered and locally actionable exit criteria pass. Checks deferred by explicit user direction must be recorded below as deferred, never described as passing.
- Keep a short decision log when implementation differs from this plan.

**Overall status:** 6 / 8 phases complete; Phases 0–5 implementation is complete
**Overall progress:** Phase 5 startup and tray features are implemented. Interactive tray/login smoke checks and Windows/Linux verification are explicitly deferred.

| Phase | Name | Priority | Status |
|---|---|---|---|
| 0 | Project foundation and decisions | P0 | Complete |
| 1 | Process engine spike | P0 | Complete (cross-platform runs deferred) |
| 2 | Service configuration and persistence | P0 | Complete |
| 3 | Core service management UI | P0 | Complete (interactive smoke checks deferred) |
| 4 | Logs and runtime observability | P0 | Complete (interactive and cross-platform checks deferred) |
| 5 | Startup and tray | P1 | Complete (interactive and cross-platform checks deferred) |
| 6 | Reliability, security and cross-platform hardening | P0 | Not started |
| 7 | Packaging, release and documentation | P1 | Not started |

#### Deferred verification

- Windows and Linux process-tree test runs. macOS process-tree tests pass locally; the platform fixtures and CI matrix remain in place for a later verification pass.
- Interactive Next.js/Mailpit start-stop smoke tests and main-window close/reopen. The desktop app launches and SQLite persistence is covered by close/reopen tests.
- Interactive noisy-output UI stress check and CPU/memory metric verification on Windows/Linux. Bounded backend/frontend buffers and macOS Rust tests are covered; no GUI stress or cross-platform metric pass is claimed.
- Interactive tray open/close, stop-all, quit-confirmation, start-minimized, and login-launch checks. The implementation is in place, but GUI behavior has not been exercised in this verification pass; Linux and Windows verification remains deferred.

---

### Phase 0 — Project foundation and decisions

**Objective:** Verify the generated `create-tauri-app` scaffold, then turn it into a reproducible development foundation.

**Current scaffold selection (confirmed):**

- Project name / directory: `processhub`
- Tauri identifier: `com.aakashhq.processhub`
- Frontend template: React
- Frontend language: TypeScript
- Package manager: pnpm
- Desktop framework: Tauri 2

**Scaffold status:** The template generation and dependency installation completed. The production frontend build, Biome checks, Rust tests, formatting, and Clippy pass locally. The desktop app launched on macOS using temporary Vite port 1422 because the default port was occupied; interactive workflow checks remain deferred. Do not recreate the project unless the generated scaffold is broken.

#### Tasks

- [x] Generate the project using `create-tauri-app`.
- [x] Select the `processhub` project name.
- [x] Set identifier to `com.aakashhq.processhub`.
- [x] Select React with TypeScript.
- [x] Select pnpm as the package manager.
- [x] Run `pnpm install` and confirm dependency installation.
- [x] Run `pnpm tauri dev` and confirm the desktop app launches (used temporary port 1422 because port 1420 was occupied).
- [x] Run the production frontend build and confirm it succeeds (`pnpm build`).
- [x] Confirm app identifier, product name, and bundle metadata in the generated Tauri configuration.
- [x] Confirm Tauri 2 version and Rust toolchain.
- [x] Confirm the generated React + TypeScript + Vite setup.
- [x] Add TypeScript formatting and lint tooling (Biome).
- [x] Add Rust formatting and lint checks (rustfmt and Clippy in CI).
- [x] Establish the small, project-owned component set requested for this app instead of adding a separate UI framework; preserve the accepted design.
- [x] Add Lucide icons for process types and actions.
- [x] Use a single process workspace instead of route/sidebar navigation.
- [x] Create the initial app shell with app bar and process canvas.
- [x] Create the icon-grid app space with a Settings → About menu, New button, and context actions.
- [x] Extract focused Button, Input/Textarea, Card, Modal, ServiceIcon, service card, context menu, settings menu, About, and service form components.
- [x] Add an AppShell for the app bar and workspace layout, plus a focused ServiceError component.
- [x] Replace the native delete confirmation with an app-styled confirmation modal.
- [x] Stop a running service as part of an explicitly confirmed delete before removing its configuration.
- [x] Add `useServiceManager` and `useContextMenu` hooks for service state/operations and context-menu behavior.
- [x] Keep the app light-only, including the `AppSettings` type.
- [x] Add typed frontend models for Service, RuntimeState, and AppSettings.
- [x] Add a central typed Tauri IPC wrapper.
- [x] Add a central event subscription module.
- [x] Configure application data directory handling.
- [x] Add application-level local logging for app startup and process lifecycle events.
- [x] Add CI for formatting, linting, build, Rust tests, and Clippy on macOS, Windows, and Linux.
- [x] Add `docs/architecture.md` and record initial decisions.

#### Deliverables

- A light, no-scroll ProcessHub app canvas with a service icon grid and About dialog.
- Consistent app-bar and service-item styling.
- Typed frontend/backend integration boundary.
- Reproducible development commands.

#### Exit criteria

- [x] App starts in development mode.
- [x] Production frontend build succeeds (`pnpm build`).
- [x] Rust tests, formatting, and Clippy plus frontend lint, format, and type/build checks pass locally.
- [x] No application feature depends on hard-coded mock state.
- [x] A new developer can follow the README to run the project.

---

### Phase 1 — Process engine spike

**Objective:** Prove that Rust can start, monitor, capture output from, and stop a process tree before building the full product around it.

This phase should be deliberately small and technical. A CLI or temporary Tauri test screen is acceptable.

#### Tasks

- [x] Create a minimal Rust process manager module and typed process configuration.
- [x] Spawn direct executables with configured working directories and environment variables.
- [x] Capture stdout/stderr through bounded chunks; test partial lines, UTF-8 chunk boundaries, and output larger than the chunk size.
- [x] Detect exits and exit codes; test non-zero exit and executable-not-found errors.
- [x] Validate invalid working directories before spawn and test immediate exits.
- [x] Implement idempotent start/stop, concurrent stop waiters, graceful termination, and force termination after a timeout.
- [x] Create and terminate a child/grandchild fixture on the current macOS host.
- [x] Add a Windows process-tree fixture to CI; defer executing the Windows job.
- [x] Add POSIX process-tree coverage to CI; defer executing the Linux job.
- [x] Stress the bounded reader with output larger than its buffer.
- [x] Test a process that exits with a non-zero code.
- [x] Startup uses synchronous OS spawn before yielding; cancellation cannot interrupt the spawn after the backend command begins, and the monitor owns the child once created.
- [x] Document platform limitations and chosen containment mechanism.

**Phase 1 validation note:** 13 Rust unit tests pass on the current macOS host, including repeated start/stop, force shutdown, large/frequent output, and grandchild termination checks. Windows and Linux runs are deferred by user direction; no results are claimed. Windows uses `taskkill /T`; consider Job Objects if a later Windows run exposes a cleanup gap.

#### Deliverables

- Working Rust process supervisor prototype.
- Process lifecycle state machine.
- Platform-specific process-tree implementation or documented blocker.
- Test fixtures for child/grandchild process scenarios.

#### Exit criteria

- [x] Direct executable can be started and stopped reliably on the current host.
- [x] Working directory and environment are correct.
- [x] stdout/stderr reader path is bounded and tested with large output.
- [x] Exit state is detected reliably on the current host.
- [x] POSIX process-group and Windows `taskkill /T` strategies and platform fixtures are implemented; Linux/Windows execution is deferred.
- [x] No global lock is held while waiting for process exit.
- [x] Log buffering is bounded by a 256-event channel and 16 KiB chunks.

**Phase 1 status:** Implementation and local macOS validation are complete. Cross-platform execution is deferred and tracked above; it is not represented as a pass.

---

### Phase 2 — Service configuration and persistence

**Objective:** Build durable service definitions and the local data layer.

#### Tasks

- [x] Integrate bundled SQLite, database initialization, and a `user_version` migration.
- [x] Add services, legacy groups, environment, dependency, and app settings tables.
- [x] Add the database repository and service/settings CRUD operations.
- [x] Persist enabled/disabled, ordering, and environment fields.
- [x] Copy validated PNG/JPEG/WebP icons to app-managed storage and restrict asset-protocol access to the icons directory.
- [x] Validate names, executable, working directory, arguments, environment variables, and unsupported shell mode.
- [x] Test clean migration, CRUD, constraints, environment cascade, settings, and persistence across reopen.
- [x] Add JSON configuration export format.
- [x] Define secret storage approach before persisting secret values: reject secret values until secure storage exists.

**Phase 2 validation note:** Local tests cover clean migration, CRUD, cascading environment cleanup, duplicate-name constraints, settings, and persistence across reopen. Legacy group tables and nullable service columns remain in existing databases without a user-facing API; grouping is out of product scope. Icon-copy tests verify managed copies and reject unsupported extensions. Configuration import/restore is outside this phase's current task list and remains a future item.

#### Deliverables

- Versioned SQLite database.
- Typed Rust models and repository.
- Configuration CRUD operations.
- Persistent app settings.

#### Exit criteria

- [x] Services remain after database close/reopen tests.
- [x] Migrations work from a clean database.
- [x] Invalid data is rejected with useful errors.
- [x] Icon files are copied into app-managed storage rather than relying on temporary picker paths.
- [x] Secrets are rejected instead of stored as plaintext.

**Phase 2 status:** Exit criteria pass in the local Rust test suite. The desktop app uses the same app-data SQLite file across launches; GUI reopen verification is tracked under Phase 0/3.

---

### Phase 3 — Core service management UI

**Objective:** Deliver the main user workflow: add a service and control it from the application.

#### Tasks

- [x] Build single-screen process workspace.
- [x] Build icon-grid service list with service-specific status indicators.
- [x] Extract reusable UI components with focused responsibilities and preserve the accepted light app design.
- [x] Extract AppShell and ServiceError components for app layout and service operation feedback.
- [x] Use an in-app delete confirmation; confirmed deletion stops a running service before removing its saved configuration.
- [x] Move process management and context-menu state into focused hooks.
- [x] Build Add and Edit Service forms.
- [x] Build icon picker/upload with app-managed storage.
- [x] Build executable and one-argument-per-line editor.
- [x] Build working-directory picker.
- [x] Show direct mode and disable unsupported shell mode.
- [x] Build argv preview, environment editor, and advanced behavior section.
- [x] Add frontend/backend validation and error feedback.
- [x] Wire create/update/delete and start/stop/restart commands.
- [x] Add loading, disabled, delete-confirmation, and empty states.
- [x] Add keyboard search/filtering and per-item move-to-first/last custom ordering without changing the requested top bar.
- [x] Reconcile UI state with backend runtime snapshots.

#### Deliverables

- Functional service list.
- Add/edit/delete service flows.
- Start/stop/restart controls.
- Persistent configuration.

#### Exit criteria

- [x] The form supports an npm executable, `run dev` arguments, and a project directory.
- [x] The form supports an executable such as Mailpit with separate arguments.
- [x] Start/stop/restart are implemented and tested with process fixtures; defer the Next.js and Mailpit interactive smoke runs.
- [x] The UI displays backend-confirmed runtime state.
- [x] Database persistence is covered by reopen tests; defer main-window close/reopen interaction checks.
- [x] Process operations run asynchronously and expose loading states.

**Phase 3 status:** Components, forms, native pickers, runtime actions, keyboard search, and ordering are implemented. Restart policy controls remain disabled; per-service auto-start is enabled as part of Phase 5. Desktop launch succeeds; Next.js/Mailpit and main-window interaction checks are deferred as recorded above.

---

### Phase 4 — Logs and runtime observability

**Objective:** Make running processes understandable and debuggable.

#### Tasks

- [x] Implement stdout reader task.
- [x] Implement stderr reader task.
- [x] Normalize output into typed log events.
- [x] Add timestamps and stream labels.
- [x] Persist per-service logs as newline-delimited JSON in app data.
- [x] Rotate service logs at 10 MiB, keeping three rotated files alongside the active log.
- [x] Remove log files older than the configured retention period when a service starts.
- [x] Expose 7, 14, 30, and 90-day retention choices in the log viewer and persist the selection.
- [x] Add bounded backend log channel and chunk buffers.
- [x] Batch frontend log state updates every 50 ms with a bounded pending queue and visible overflow notice.
- [x] Build per-service detail modal with live logs, runtime metrics, and recent runs.
- [x] Build live per-service log viewer in a modal.
- [x] Add auto-scroll toggle.
- [x] Add pause/resume visual updates.
- [x] Add search in loaded logs.
- [x] Add copy selected/all filtered visible logs and export.
- [x] Add clear-view control for the bounded in-memory buffer.
- [x] Show PID, uptime, and last exit status/code when available.
- [x] Persist bounded per-service log files with rotation and configurable retention cleanup.
- [x] Load saved log history into the viewer when opening a service's log modal.
- [x] Sample root-process CPU and memory every second with sysinfo and label values as estimates.

**Phase 4 progress note:** The service context menu opens a per-service detail modal and loads up to the latest 2,000 persisted events plus up to 20 saved runs. The frontend keeps at most 2,000 events per service and batches rendering every 50 ms, with a 4,000-event pending cap and an explicit overflow notice. Backend output continues through the bounded event channel and is persisted as newline-delimited JSON under app data. Each active log rotates at 10 MiB, keeps three rotated files, and removes files older than the selected 7, 14, 30, or 90-day retention period when a service starts. Search, pause/resume, follow toggle, row selection, copy, export, and clearing the current buffer are implemented; clearing the view leaves persisted files intact. CPU and memory are sampled once per second for the root PID only and shown as estimates. Automated output and rotation checks pass; interactive UI stress and Windows/Linux metric verification are deferred by user direction.
- [x] Add export logs from the current viewer buffer.
- [x] Keep “Clear view” scoped to the in-memory viewer buffer; persisted log files remain intact.
- [x] Add PID and uptime display.
- [x] Integrate CPU and memory collection into runtime state.
- [x] Define CPU/memory as root-process-only metrics; child processes are excluded.
- [x] Use a one-second metrics sampling interval.
- [x] Persist and display the latest 20 run records per service.
- [x] Add the latest runtime status and exit code to the viewer header.
- [x] Stress-test high-volume output and log rotation with bounded readers and storage; defer interactive long-running soak checks.

#### Deliverables

- Live logs.
- Persistent, bounded log files.
- Runtime metadata.
- Basic resource monitoring.

#### Exit criteria

- [x] stdout and stderr are visible.
- [ ] Large-output UI responsiveness confirmed interactively (deferred by user direction; bounded batching and buffers are implemented).
- [x] Log storage is bounded by rotation and the configured retention period.
- [x] CPU and memory are labelled as root-process estimates.
- [x] A stopped process retains its last-run information for the current app session.
- [x] Log export produces a readable text file.

---

### Phase 5 — Startup and tray

**Objective:** Make ProcessHub convenient for daily development workflows.

#### Tasks

**Startup**
- [x] Add per-service auto-start setting.
- [x] Add global “start enabled auto-start services” setting.
- [x] Define startup order by saved service order, then name.
- [x] Add startup progress and failure summary in the workspace.
- [x] Ensure a failed service does not block unrelated services.

**Tray and app lifecycle**
- [x] Add system tray icon and menu.
- [x] Add Open/Show action.
- [x] Add close-to-tray behaviour.
- [x] Add explicit Quit action.
- [x] Add Quit confirmation when services are running.
- [x] Add Stop all services action.
- [x] Add launch-at-login integration.
- [x] Add start minimized option.
- [ ] Test tray behaviour on each OS and desktop environment (interactive checks deferred; Windows/Linux deferred by user direction).
- [x] Handle app shutdown by stopping services according to the selected policy.
- [x] Ensure services are not accidentally orphaned without a clear user-facing explanation.

#### Deliverables

- Auto-start on ProcessHub launch.
- Tray integration.
- Launch at user login.

#### Exit criteria

- [x] App can hide to tray while services continue running (interactive verification deferred).
- [x] Login startup integration is available through Startup settings (platform smoke checks deferred).
- [x] Quit behaviour explicitly stops managed services; interactive verification deferred.
- [x] Tray menu actions match their implemented behavior.

---

### Phase 6 — Reliability, security and cross-platform hardening

**Objective:** Turn a working developer tool into a trustworthy application.

#### Reliability tasks

- [ ] Add per-service operation locks.
- [ ] Add concurrency/race-condition tests.
- [ ] Add crash-safe database writes.
- [ ] Add stale runtime reconciliation.
- [ ] Add bounded event queues.
- [ ] Add restart policy backoff.
- [ ] Ensure explicit user stop suppresses auto-restart.
- [x] Ensure disabled services do not auto-start.
- [ ] Add application shutdown timeout handling.
- [ ] Add structured diagnostics.
- [ ] Add recoverable error screens.
- [ ] Test database corruption/recovery strategy.
- [ ] Test disk-full behaviour for logs.
- [ ] Test app upgrade with existing user data.

#### Security tasks

- [ ] Review Tauri capabilities and permissions.
- [ ] Ensure no unrestricted frontend shell API exists.
- [ ] Validate all service input in Rust.
- [ ] Ensure direct mode uses argument vectors.
- [ ] Clearly mark shell mode.
- [ ] Review environment secret handling.
- [ ] Mask secret values in UI.
- [ ] Prevent secrets from entering logs.
- [ ] Review file picker and icon path handling.
- [ ] Review symlink/path traversal cases.
- [ ] Do not elevate privileges automatically.
- [ ] Add safe defaults for stop and restart behaviour.
- [ ] Review local data file permissions.

#### Cross-platform test matrix

| Test area | Windows | macOS | Linux |
|---|---:|---:|---:|
| App launches | [ ] | [ ] | [ ] |
| Create/edit/delete service | [ ] | [ ] | [ ] |
| Direct executable | [ ] | [ ] | [ ] |
| Shell mode | [ ] | [ ] | [ ] |
| Working directory | [ ] | [ ] | [ ] |
| Environment variables | [ ] | [ ] | [ ] |
| stdout/stderr | [ ] | [ ] | [ ] |
| Parent + child cleanup | [ ] | [ ] | [ ] |
| Restart policy | [ ] | [ ] | [ ] |
| Tray behaviour | [ ] | [ ] | [ ] |
| Launch at login | [ ] | [ ] | [ ] |
| Installer/uninstaller | [ ] | [ ] | [ ] |
| Upgrade preserves data | [ ] | [ ] | [ ] |

#### Performance targets to validate

These are initial engineering targets, not guaranteed measurements:
- UI remains responsive with 20–30 lightweight managed services.
- Idle monitoring overhead remains low.
- Log memory usage stays bounded under sustained output.
- Metrics sampling does not materially affect process performance.
- Starting/stopping one service does not block unrelated service operations.
- Database queries remain responsive with a large service list and run history.

#### Exit criteria

- [ ] Critical lifecycle tests pass on all supported OSes.
- [ ] Process-tree termination behaviour is documented and verified.
- [ ] Security review has no unresolved critical issue.
- [ ] No known unbounded log/event buffer remains.
- [ ] App shutdown and crash scenarios have defined behaviour.
- [ ] A clean install and upgrade test pass on each platform.

---

### Phase 7 — Packaging, release and documentation

**Objective:** Produce installable builds and prepare a usable beta release.

#### Tasks

- [ ] Configure application icons for all platforms.
- [ ] Configure bundle metadata and versioning.
- [ ] Build Windows installer.
- [ ] Build macOS application bundle and installer/distribution format.
- [ ] Build Linux AppImage and/or distribution packages supported by the release strategy.
- [ ] Test install and uninstall.
- [ ] Test launch after installation.
- [ ] Test data directory persistence after upgrade.
- [ ] Configure code signing where available.
- [ ] Configure macOS notarization if distributing outside local development.
- [ ] Document platform security prompts.
- [ ] Write user quick-start guide.
- [ ] Write service configuration guide.
- [ ] Write troubleshooting guide.
- [ ] Write known limitations.
- [ ] Add license notices for dependencies.
- [ ] Add release notes.
- [ ] Create issue/feedback template.
- [ ] Tag beta release.

#### Release deliverables

- Windows build.
- macOS build.
- Linux build(s).
- README and installation guide.
- User guide and known limitations.
- Release notes and version number.

#### Exit criteria

- [ ] A new user can install and run the app without developer tooling.
- [ ] App launches successfully after installation.
- [ ] Existing configuration survives an in-place upgrade.
- [ ] Uninstall behaviour is documented, including whether user data is retained.
- [ ] All release artifacts are tested on their target OS.

---

## 13. Test strategy

### 13.1 Process fixture programs

Create small test programs/scripts for:
- Long-running process with no output.
- Process that prints every second.
- Process that writes to stdout and stderr.
- Process that exits successfully.
- Process that exits with a non-zero code.
- Process that spawns a child and grandchild.
- Process that ignores graceful termination.
- Process that emits very large output.
- Process that requires a specific working directory.
- Process that reads environment variables.
- Process that starts slowly.
- Process that exits immediately.

### 13.2 Test layers

**Rust unit tests**
- State transitions.
- Configuration validation.
- Restart backoff calculations.
- Dependency graph cycle detection.
- Argument handling.
- Error mapping.

**Rust integration tests**
- Spawn and monitor.
- Output capture.
- Exit watcher.
- Stop and force-stop.
- Process-tree cleanup.
- Concurrent service operations.
- Database migrations and repositories.

**Frontend tests**
- Add/edit form validation.
- Service list rendering.
- Runtime state updates.
- Error and loading states.
- Logs rendering and batching.
- Confirmation dialogs.

**End-to-end tests**
- Add a service.
- Start it.
- Observe running state.
- View logs.
- Stop it.
- Restart it.
- Close to tray and reopen.
- Relaunch app and verify configuration persistence.

### 13.3 Manual release checklist

- [ ] Test from a clean user profile.
- [ ] Test paths containing spaces.
- [ ] Test non-ASCII paths and service names.
- [ ] Test missing executable.
- [ ] Test unavailable working directory.
- [ ] Test environment variable overrides.
- [ ] Test a service that fails immediately.
- [ ] Test a service that produces continuous logs.
- [ ] Test stop while starting.
- [ ] Test restart while stopping.
- [ ] Test Quit with running services.
- [ ] Test OS shutdown/restart behaviour.
- [ ] Test database upgrade.
- [ ] Test log rotation and disk-full handling.

---

## 14. Suggested MVP scope and build order

Do not attempt every feature at once. The best implementation order is:

1. Prove process spawning, output capture, exit detection, and process-tree termination.
2. Persist service configuration.
3. Build a basic service list and add/edit form.
4. Connect start/stop/restart.
5. Add logs.
6. Add metrics.
7. Add tray and login startup.
8. Harden and package.

### MVP feature classification

| Feature | MVP | Later |
|---|:---:|:---:|
| Direct executable + args | Yes | |
| Working directory | Yes | |
| Environment variables | Yes | |
| Shell mode | Basic | More shell options |
| Add/edit/delete services | Yes | |
| Icons | Yes | |
| Start/stop/restart | Yes | |
| Process-tree termination | Yes, platform-tested | Stronger containment |
| Live stdout/stderr | Yes | |
| Log rotation/export | Basic | Advanced search |
| CPU/RAM | Basic | Tree aggregation/advanced limits |
| Tray | Yes | Rich tray status menu |
| Launch at login | Yes | |
| Auto-restart | Basic | Advanced policy editor |
| TCP/HTTP health checks | Basic TCP optional | Advanced checks |
| Independent daemon | | Yes |
| Remote services | | Yes |
| Docker integration | | Yes |
| Resource limits | | Yes |

---

## 15. Future architecture: independent supervisor daemon

This is a valuable future direction, but should not delay the first usable release.

### Why add it?

In the MVP, the process supervisor lives inside the ProcessHub application process. If ProcessHub exits unexpectedly, its managed process handles and monitoring tasks disappear. Depending on the OS and process setup, child processes may continue running without supervision.

A future daemon would separate:
- `processhub-ui`: desktop interface.
- `processhub-supervisor`: background service manager.

The UI would communicate with the supervisor over a local IPC channel. The daemon would own process handles, logs, restart policies, and runtime state.

### Future daemon requirements

- Per-user installation and lifecycle.
- Authenticated local IPC.
- Strict local socket/pipe permissions.
- Versioned IPC protocol.
- Daemon health checks.
- UI reconnect and state reconciliation.
- Safe daemon upgrade and restart.
- Explicit user choice about service survival.
- Platform-specific installation and uninstall handling.

Do not install a privileged system-wide daemon by default. Start with a per-user background process and evaluate OS service integration only if a real use case requires it.

---

## 16. Open technical decisions to resolve during implementation

Record each decision in `docs/architecture.md` with date, options considered, decision, and reason.

| Decision | Recommended starting point |
|---|---|
| SQLite library | Evaluate SQLx vs rusqlite; choose based on async and migration needs |
| Process containment | Windows Job Objects; POSIX process groups on macOS/Linux |
| Shell mode | Explicit opt-in, direct mode default |
| Log storage | Rotated files + SQLite metadata |
| State management | Backend authoritative; frontend subscribes to events |
| Metrics | sysinfo spike, then verify process-tree accuracy |
| Secrets | OS keyring/secure storage; never silently plaintext |
| Startup plugin | Tauri 2 autostart plugin after lifecycle behaviour is tested |
| Tray | Tauri 2 tray APIs; verify platform-specific behaviour |
| UI components | Choose one accessible component system and keep it consistent |

---

## 17. Definition of done for every feature

A feature is not complete merely because its UI exists.

For each feature:
- [ ] User flow is implemented.
- [ ] Backend validation exists where relevant.
- [ ] Loading, empty, success, and failure states exist.
- [ ] Errors are actionable and do not expose secrets.
- [ ] Relevant unit/integration/UI tests exist.
- [ ] Keyboard accessibility is checked.
- [ ] Windows/macOS/Linux differences are documented.
- [ ] No unbounded background task, event queue, or buffer is introduced.
- [ ] Documentation is updated.
- [ ] Progress tracker is updated.

---

## 18. Progress tracker template

Copy this section into the project issue tracker or update it during development.

### Current sprint

- **Sprint:** Startup and tray
- **Goal:** Add tray lifecycle and login startup support
- **Owner:** —
- **Start date:** 2026-11-25
- **Target date:** —

### Phase status

| Phase | Status | Completion | Notes |
|---|---|---:|---|
| 0 — Foundation | Complete | 100% | Build, lint, formatting, Rust tests/Clippy, CI configuration, app-data setup, and an approved development-window launch are verified locally. |
| 1 — Process engine | Complete | 100% | 13 Rust tests pass on macOS, including child/grandchild cleanup; Windows and Linux runs are deferred by user direction. |
| 2 — Persistence | Complete | 100% | Migration, CRUD, constraints, app settings, persistence/reopen, and app-data icon copy are covered locally. Legacy group columns remain for database compatibility. |
| 3 — Core UI | Complete | 100% | AppShell, focused components/hooks, service forms, native pickers, confirmed deletion, search, ordering, and process actions are in place; interactive example smoke checks are deferred by user direction. |
| 4 — Logs/metrics | Complete | 100% | Rotating persistent logs, configurable retention, batched live viewer, root-process metrics, and the latest 20 persisted runs are implemented. Interactive UI stress and Windows/Linux checks are deferred by user direction. |
| 5 — Startup/tray | Complete | 100% | Ordered enabled-service startup, failure reporting, tray controls, close-to-tray, explicit stop-and-quit, start minimized, and login integration are implemented. Interactive checks and Linux/Windows verification are deferred. Grouping remains out of scope. |
| 6 — Hardening | Not started | 0% | |
| 7 — Release | Not started | 0% | |

### Decision log

| Date | Decision | Reason | Follow-up |
|---|---|---|---|
| 2026-11-25 | Tauri 2 + React + TypeScript + Rust | Cross-platform desktop UI with native process supervision | Validate process lifecycle on all OSes |
| 2026-11-25 | Tokio async supervision | Avoid dedicated monitoring thread per service | Verify bounded task and channel behaviour |
| 2026-11-25 | Direct executable mode as default | Safer and more predictable than shell concatenation | Implement explicit shell mode separately |
| 2026-11-25 | Close to tray for normal window close | Keep services alive during normal desktop use | Define explicit Quit behaviour |
| 2026-11-25 | `rusqlite` with bundled SQLite | Local short transactions fit a synchronous repository; process I/O remains async | Verify clean migration and CRUD |
| 2026-11-25 | POSIX process groups; Windows `taskkill /T` fallback | Initial no-elevation tree-stop approach | Validate child trees on all target OSes; consider Windows Job Objects |
| 2026-11-25 | Light-only fixed app canvas | Process icons and names live in one workspace; right-click manages each item; top bar shows logo/name, New, and Settings | Verify no scrolling and the 860 × 700 minimum / 1120 × 760 initial window |

### Blockers and risks

| Risk | Impact | Mitigation | Status |
|---|---|---|---|
| Child process cleanup differs by OS | Critical | Test process groups and Windows tree termination with child/grandchild fixtures | Open |
| Shell semantics differ by OS | Medium | Direct mode default; explicit shell mode | Open |
| Noisy logs overwhelm UI or memory | High | Bounded channels, batching, file rotation | Open |
| PID reuse causes incorrect reconciliation | High | Never trust stale PID alone | Open |
| Tray/login behaviour differs by desktop environment | Medium | Test on real target OS environments | Open |
| Secrets leak into logs or database | High | Secure storage and redaction | Open |

---

## 19. First implementation session: scaffold history

The `create-tauri-app` project has already been generated. Continue inside that project; do not run the scaffolding wizard again.

### 19.1 Verify the generated app

From the directory containing `processhub`:

```bash
cd processhub
pnpm install
pnpm tauri dev
```

Then verify a production build using the script exposed by the generated `package.json` (commonly `pnpm tauri build`). If a command differs in the generated template, use the scripts and instructions in that project's `package.json` and README as the source of truth.

The scaffold checklist is historical and now covered by Phases 0–3 above. The project was launched on an alternate port because the default 1420 port was already occupied. No commit was created.

### 19.2 Begin implementation

Implementation progress and remaining verification are tracked in the phase table above. The approved UI direction is a single light app canvas without sidebar navigation.

---

## 20. Reference documentation

Use official documentation as the implementation source of truth. Check current versions and platform support before adding dependencies.

- Tauri 2: https://v2.tauri.app/
- Tauri process/plugin ecosystem: https://v2.tauri.app/plugin/
- Tauri autostart plugin: https://v2.tauri.app/plugin/autostart/
- Tauri global shortcut plugin: https://v2.tauri.app/plugin/global-shortcut/
- Tokio: https://tokio.rs/
- Tokio process API: https://docs.rs/tokio/latest/tokio/process/
- Rust standard process API: https://doc.rust-lang.org/std/process/
- SQLx: https://docs.rs/sqlx/latest/sqlx/
- SQLite: https://www.sqlite.org/docs.html
- sysinfo: https://docs.rs/sysinfo/latest/sysinfo/
- Windows Job Objects: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
- Apple developer documentation: https://developer.apple.com/documentation/
- Linux process documentation: https://man7.org/linux/man-pages/

---

## Final recommendation

Build ProcessHub as a **Tauri 2 desktop application with React/TypeScript UI and a Rust/Tokio process supervisor**.

Prioritize correctness of process lifecycle and process-tree termination over visual polish. The first meaningful milestone is a reliable process engine; the workspace, tray, icons, metrics, and advanced controls should be layered on top of that foundation.

The MVP should be a local developer process manager. The independent background daemon should remain a planned architectural evolution, not a requirement that delays the first release.
