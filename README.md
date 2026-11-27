<div align="center">
  <img src="src-tauri/icons/128x128.png" width="96" height="96" alt="ProcessHub icon" />

  <h1>ProcessHub</h1>

  <p>
    <strong>A local-first desktop control center for your development processes and services.</strong>
  </p>

  <p>
    <a href="https://github.com/ProcessHub-HQ/processhub/actions/workflows/ci.yml">
      <img src="https://github.com/ProcessHub-HQ/processhub/actions/workflows/ci.yml/badge.svg" alt="CI" />
    </a>
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black" alt="React 19" />
    <img src="https://img.shields.io/badge/Rust-2024-000000?logo=rust&logoColor=white" alt="Rust" />
    <img src="https://img.shields.io/badge/Platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platforms" />
  </p>

  <p>
    <a href="#features">Features</a> ·
    <a href="#architecture--safety">Architecture &amp; Safety</a> ·
    <a href="#development">Development</a> ·
    <a href="#tech-stack">Tech Stack</a> ·
    <a href="#platform-notes">Platform Notes</a>
  </p>
</div>

---

**ProcessHub** is a lightweight, cross-platform desktop application for creating, starting, stopping, restarting, and monitoring local processes and developer services from one interface.

Define a service once—such as a Next.js dev server, Laravel app, Redis instance, Mailpit, background worker, or custom script—and manage its lifecycle, logs, and resource usage without juggling terminal tabs.

## Features

- **Direct Process Execution** — Runs configured executables and argument vectors directly with a dedicated working directory and environment variables—no silent shell interpolation.
- **Reliable Process-Tree Shutdown** — Isolates each service in a dedicated POSIX process group on macOS/Linux and uses `taskkill /T` (with force-kill fallback) on Windows so child processes don't linger.
- **Crash & Restart Recovery** — Persists tracked process identities (`pid`, process group ID, start time, executable path, and session ID) in SQLite. Gracefully stops services on quit, and verifies process identity after a crash before cleaning up orphaned trees—never signaling an unrelated reused PID.
- **Restart Policies** — Restart on failure or always, with exponential backoff capped at 30 seconds, a configurable attempt limit, a stable-uptime reset, and a visible countdown while a restart is pending. Explicit stops and manual starts cancel or reset the schedule.
- **Single-Instance Enforcement** — Launching a second ProcessHub window focuses the existing instance instead of spawning duplicate supervisors.
- **Live Logs & Run History** — Streams `stdout` and `stderr` with a batched 100 ms disk pipeline, ANSI-colored rendering (16/256/truecolor), bounded UI buffering (up to 2,000 events or 4 MiB per service, batched every 50 ms), persisted newline-delimited JSON logs with 10 MiB rotation (3 rotated files) and configurable retention (`7` / `14` / `30` / `90` days), and keeps the latest 20 completed runs per service in SQLite.
- **Resource Monitoring** — Samples root-process CPU and memory estimates every second.
- **Secret Environment Values** — Secrets are stored in the OS keychain, redacted from logs, excluded from exports, and required before a service starts.
- **Desktop Notifications** — Failed services and unhealthy checks notify when the main window is hidden; tray submenus offer per-service start, stop, restart, details, and Open URL.
- **Health Checks** — TCP-port or HTTP readiness with a start-period grace window, configurable interval/timeout, and an optional expected status. Process state and health are reported separately.
- **Detected URLs** — Finds the first local URL or `host:port` in service output and offers a one-click Open action, reset per run.
- **Dependencies** — Mark services as "Start after" another service; starts resolve dependencies in order and wait for health when configured, with cycle-safe validation.
- **Configuration Backup** — Export services, dependencies, and settings to JSON; import with automatic name de-duplication, dependency remapping, and missing-directory/icon/secret cleanup reporting.
- **Bulk Actions** — Start all, restart all, and stop all from the workspace context menu, respecting dependency order and health gates.
- **System Tray & Autostart** — Lives cleanly in the system tray, supports launching at OS login, and can auto-start selected services.
- **Custom Service Icons & Export** — Import PNG, JPEG, or WebP icons into app-managed storage (`$APPDATA/icons`), duplicate services, search/filter, and export configurations.

## Architecture & Safety

ProcessHub is built to be predictable, local-only, and safe by default. Key design decisions are documented in [docs/architecture.md](docs/architecture.md):

| Area | Decision |
|---|---|
| **Execution model** | Direct executable + argument vector execution; shell mode is rejected until explicitly implemented. |
| **Local storage** | Embedded SQLite via `rusqlite` with versioned `user_version` migrations; async process I/O never holds the DB lock. |
| **Environment secrets** | Secret values live in the OS keychain; SQLite, exports, and logs never contain them. Output is redacted and a missing secret blocks startup. |
| **App diagnostics** | Lifecycle and process events are written locally via `tauri-plugin-log`, excluding service arguments and environment values. |
| **UI architecture** | Focused in-house React components and hooks on a dark, no-scroll desktop canvas (`800 × 500` fixed window). |

See [PLAN.md](PLAN.md) for the full engineering roadmap and phase status.

## Development

### Prerequisites

- **Node.js** (v22+ recommended) and **pnpm** (v10+)
- **Rust** stable toolchain (`rustfmt` and `clippy` components)
- **Linux only:** Tauri 2 system dependencies (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`, `libssl-dev`) — see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

### Run Locally

```sh
git clone https://github.com/ProcessHub-HQ/processhub.git
cd processhub
pnpm install
pnpm tauri dev
```

### Build

```sh
pnpm tauri build
```

### Checks & Formatting

Run the same checks enforced by [CI](.github/workflows/ci.yml):

```sh
pnpm lint
pnpm format:check
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## Tech Stack

| Layer | Technologies |
|---|---|
| **Desktop Shell** | [Tauri 2](https://v2.tauri.app/) (`single-instance`, `tray-icon`, `autostart`, `dialog`, `log`, `opener`) |
| **Frontend** | React 19, TypeScript, Vite, Lucide Icons |
| **Backend** | Rust (2024 edition), Tokio, `rusqlite` (bundled SQLite), `sysinfo`, `keyring` |
| **Tooling** | Biome (TS/CSS lint & format), `rustfmt`, `clippy`, GitHub Actions |

## Project Structure

```text
processhub/
├── src/                # React + TypeScript UI, hooks, and Tauri bindings
├── src-tauri/          # Rust backend, process supervisor, SQLite repository, icons
├── docs/               # Architecture decision records
├── scripts/            # Asset & icon generation scripts
└── PLAN.md             # Product & engineering plan
```

## Platform Notes

- **CPU & Memory metrics** are sampled once per second for the root process only and displayed as estimates; child processes are excluded.
- **Windows crash recovery** currently uses `taskkill /T /F` with PID + start-time verification; descendants that outlive the root cannot be verified without Windows Job Objects (tracked as a follow-up in [docs/architecture.md](docs/architecture.md)).
- **Secret environment variables** use the OS keychain (Keychain / Credential Manager / Secret Service). Removing a secret deletes its keychain entry.

## License

ProcessHub is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
