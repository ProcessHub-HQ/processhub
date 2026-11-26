# Architecture decisions

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
