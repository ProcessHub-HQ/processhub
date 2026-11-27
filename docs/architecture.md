# Architecture decisions

## 2026-11-26 — Tray submenus and notifications

- **Decision:** Give each service a tray submenu with Start, Stop, Restart, Open details, and Open URL. Open URL is enabled only when output detection has produced a URL.
- **Decision:** Send OS notifications for final process failures and unhealthy transitions only while the main window is hidden. Restart retries and user stops do not notify.
- **Reason:** The tray is the primary surface when ProcessHub is closed to the dock, and failures otherwise go unnoticed.
- **Follow-up:** Add notification action buttons (restart/open logs) once platform support is verified.

## 2026-11-26 — Keychain-backed secret environment values

- **Decision:** Store secret environment values in the OS keychain through the `keyring` crate (macOS Keychain, Windows Credential Manager, Linux kernel keyring). SQLite stores only the secret flag and an empty value.
- **Decision:** Resolve secrets only at spawn time. A missing secret prevents the service from starting with a clear error. Redact exact secret values from process output before it is persisted or emitted. Exports drop secrets, and updating or deleting a service removes keychain entries that are no longer referenced.
- **Reason:** Secrets must never land in the local SQLite file, exported configuration, or log history, while remaining usable by child processes.
- **Follow-up:** Redact secrets that span log chunk boundaries and add a per-secret reveal flow in the UI.

## 2026-11-26 — Configuration import and export

- **Decision:** Export services, dependencies, and settings as a versioned JSON file. Import creates new IDs, remaps dependency references, and renames services that collide with existing names using an `(imported)` suffix.
- **Decision:** Drop machine-specific icon paths on import and clear working directories that do not exist on the target machine. Report both counts. Imported dependency edges are cycle-checked before anything is written; `startAtLogin` is not imported because it is OS state.
- **Reason:** Configuration should be movable between machines without overwriting local services or leaving dangling dependencies.
- **Follow-up:** Pack icons into the export bundle and offer a replace-existing conflict mode.

## 2026-11-26 — Service dependencies

- **Decision:** Persist dependency edges in the existing `service_dependencies` table. Reject unknown targets, self-dependencies, disabled dependencies, and cycles before saving or starting.
- **Decision:** Starting a service resolves its transitive dependencies and starts them in dependency order. Auto-start computes an order per auto-start target so one broken graph does not block unrelated services. A configured health check must reach `healthy` before a dependent starts.
- **Reason:** Development stacks usually need a database or API running before the app; dependency order is sufficient without a separate grouping layer.
- **Follow-up:** Dependency-aware stop order.

## 2026-11-26 — Detected service URLs

- **Decision:** Scan each output chunk for the first `http://` or `https://` URL, falling back to `localhost:` / `127.0.0.1:` / `0.0.0.0:` host-port pairs. Store one URL per run, reset it when the run ends, and expose an Open action through the opener plugin.
- **Reason:** Development servers print their address on startup; users should not have to copy it out of the log stream.
- **Follow-up:** Warn about port conflicts before starting and gate Open on health once readiness is known.

## 2026-11-26 — Health checks

- **Decision:** Add per-service `none` / `tcp` / `http` health checks with configurable interval, timeout, start period, and optional expected HTTP status. Report health separately from process state as `not_configured`, `starting`, `healthy`, or `unhealthy`.
- **Decision:** Stay in `starting` for the configured start period before surfacing `unhealthy`, so a booting server is not reported as broken. Clear health state when the run ends or a restart is scheduled.
- **Decision:** Implement HTTP checks as a minimal `GET` over `TcpStream` for `http://` URLs only. HTTPS returns an explicit validation error until TLS support is added.
- **Reason:** A running process is not necessarily ready, and restarting or opening a dev server before it is listening wastes time.
- **Follow-up:** Add HTTPS/TLS checks, output-based readiness, and use health to gate dependent service starts.

## 2026-11-26 — Log viewer rendering

- **Decision:** Parse ANSI SGR sequences into colored and styled spans (16-color, 256-color, and truecolor), collapse carriage-return progress redraws to their final state, and strip non-SGR control sequences.
- **Decision:** Memoize each log row on event identity, key rows by a stable per-event id, and let the browser skip off-screen rows with `content-visibility: auto`. Cap the in-memory buffer at 2,000 events or 4 MiB, whichever is smaller.
- **Reason:** Raw escape codes made framework output unreadable, and re-rendering thousands of rows every 50 ms caused avoidable jank on noisy services.
- **Follow-up:** Move to a measured virtual list if long wrapped lines still cost too much on low-end hardware.

## 2026-11-26 — Batched log ingestion and tail reads

- **Decision:** Give each running service one log writer task that keeps a 64 KiB buffered file handle and coalesces output for up to 100 ms, 256 events, or 64 KiB. Flush once per batch, rotate by closing the handle before renaming, and emit a single `service-log-batch` IPC event per batch instead of one event per line.
- **Decision:** Load saved logs backwards from the newest file in 64 KiB chunks until 2,000 events are collected, then fill from older rotations only when needed.
- **Reason:** Opening, stat-ing, writing, and flushing per event plus per-line IPC created a throughput ceiling; a full 256-slot channel could backpressure the child's stdout pipe. Scanning every rotated file made opening a large history slow.
- **Follow-up:** Virtualize the log viewer and add sequence numbers so merging live and saved events stops hashing text.

## 2026-11-26 — Automatic restart policy

- **Decision:** Honor the persisted `never` / `on_failure` / `always` restart policies. Relaunch `on_failure` only for non-zero or signal exits, and `always` for every exit that was not requested by the user.
- **Decision:** Use exponential backoff starting at `restartDelayMs` and capped at 30 seconds. Stop after `maxRestartAttempts` automatic restarts; an empty limit means unlimited. Reset the attempt counter after 60 seconds of stable uptime.
- **Decision:** Represent the backoff window as a first-class `waiting_to_restart` runtime state with `restartAtMs` and `restartAttempt`. Explicit stop, disable, delete, quit, or a manual start cancels a pending restart. Manual starts reset the attempt budget.
- **Reason:** The settings were stored and shown in the form but had no runtime effect, so a crashed development server stayed down until the user noticed.
- **Follow-up:** Gate restarts on health checks once health monitoring lands, and expose restart counts in the service details view.

## 2026-11-25 — Tauri application identity

- **Decision:** Use `ProcessHub` as the display name and `com.aakashhq.processhub` as the stable bundle identifier.
- **Reason:** Keep the generated project identity aligned with the product plan.

## 2026-11-25 — Direct process execution

- **Decision:** Execute the configured executable and argument vector directly. Shell mode is rejected until it is implemented explicitly.
- **Reason:** Direct execution preserves argument boundaries and avoids silently interpreting user input as shell syntax.

## 2026-11-25 — Process tree containment

- **Decision:** Create a dedicated POSIX process group on macOS/Linux and signal the group for graceful and forced shutdown. On Windows, use `taskkill /T` with a force-kill fallback.
- **Reason:** This gives the first implementation a process-tree strategy on each target family without adding elevated permissions.
- **Follow-up:** Validate child/grandchild cleanup on all three operating systems. Windows Job Objects remain the preferred stronger containment mechanism if tests show gaps.

## 2026-11-26 — Services across app restarts

- **Decision:** Persist each managed process as `(pid, process group id, process start time, executable path, manager session id)` before the service is reported as running. Clear the record when the process exits or is stopped.
- **Decision:** A graceful quit stops every tracked service tree before exiting. If ProcessHub crashes or is force-killed, the next launch verifies each recorded process by PID, start time, and process group (Unix) or PID and start time (Windows), then stops verified trees with SIGTERM/SIGKILL on Unix and `taskkill /T /F` on Windows.
- **Decision:** If identity cannot be verified, leave the process untouched, keep its record, and refuse to auto-start, start, restart, or delete that service until the conflict is resolved. Never signal a PID or process group that does not match the recorded identity.
- **Decision:** On Unix, if the recorded root exited but descendants remain in its process group, stop the group only when every remaining member started at or after the recorded root; otherwise report it as unverifiable.
- **Decision:** Enforce a single running ProcessHub instance. A second launch focuses the existing window instead of running orphan recovery against services owned by the first instance.
- **Reason:** Closing the app should not leave duplicate services behind, while crash recovery must never kill an unrelated process that reused a PID.
- **Follow-up:** On Windows, descendants that outlive the root cannot be verified without Job Objects. Adopt Job Objects before treating Windows crash recovery as equivalent to Unix.

## 2026-11-25 — SQLite access

- **Decision:** Use `rusqlite` with bundled SQLite and a versioned `user_version` migration.
- **Reason:** The database is local and operations are short; a synchronous repository behind a mutex keeps the boundary simple. Process I/O is asynchronous and does not hold the database lock.

## 2026-11-25 — Secret environment values

- **Status:** Superseded on 2026-11-26 by keychain-backed storage.
- **Decision:** Persist only non-secret environment values. Reject `isSecret: true` until secure OS-backed storage is available.
- **Reason:** A local SQLite file is not encrypted secret storage.

## 2026-11-25 — UI components and checks

- **Decision:** Use a small set of focused in-house React components and hooks, styled with the accepted light app canvas. Use Biome for TypeScript/CSS lint and formatting; use rustfmt and Clippy for Rust.
- **Reason:** Keep the current app design, avoid adding a larger UI framework, and make component responsibilities explicit.

## 2026-11-25 — User-selected icons and logs

- **Decision:** Import supported image files into `$APPDATA/icons` and only expose that directory through Tauri's asset protocol. Write app lifecycle and process events to the app log directory through `tauri-plugin-log`.
- **Reason:** Icon references remain valid after moving/deleting the source file; diagnostics remain local and do not include service arguments or environment values.

## 2026-11-25 — Process output and runtime observability

- **Decision:** Persist each service's stdout/stderr as newline-delimited JSON in app data; rotate at 10 MiB, retain three rotated files, and clean by the selected 7/14/30/90-day policy when a service starts.
- **Decision:** Keep at most 2,000 loaded log events per service and batch frontend updates every 50 ms with a 4,000-event pending cap.
- **Decision:** Sample CPU and memory once per second for the root process only. Display both as estimates; child processes are excluded.
- **Decision:** Persist the latest 20 completed runs per service in SQLite and remove those rows when the service is deleted.
- **Reason:** Keep diagnostics useful across app restarts while bounding disk and UI memory costs. Root-only sampling avoids implying process-tree metrics that are not collected.
- **Follow-up:** Verify noisy-output UI responsiveness and metric accuracy interactively on each desktop platform.
