<div align="center">
  <img src="src-tauri/icons/128x128.png" width="96" height="96" alt="ProcessHub icon" />

  <h1>ProcessHub</h1>

  <p>
    <strong>A local-first desktop control center for your development processes and services.</strong>
  </p>

  <p>
    <a href="https://github.com/akashpoudelnp/processhub/actions/workflows/ci.yml">
      <img src="https://github.com/akashpoudelnp/processhub/actions/workflows/ci.yml/badge.svg" alt="CI" />
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
- **Single-Instance Enforcement** — Launching a second ProcessHub window focuses the existing instance instead of spawning duplicate supervisors.
- **Live Logs & Run History** — Streams `stdout` and `stderr` with bounded UI buffering (up to 2,000 events per service, batched every 50 ms), persists newline-delimited JSON logs with 10 MiB rotation (3 rotated files) and configurable retention (`7` / `14` / `30` / `90` days), and keeps the latest 20 completed runs per service in SQLite.
- **Resource Monitoring** — Samples root-process CPU and memory estimates every second.
- **System Tray & Autostart** — Lives cleanly in the system tray, supports launching at OS login, and can auto-start selected services.
- **Custom Service Icons & Export** — Import PNG, JPEG, or WebP icons into app-managed storage (`$APPDATA/icons`), duplicate services, search/filter, and export configurations.

## Architecture & Safety

ProcessHub is built to be predictable, local-only, and safe by default. Key design decisions are documented in [docs/architecture.md](docs/architecture.md):

| Area | Decision |
|---|---|
| **Execution model** | Direct executable + argument vector execution; shell mode is rejected until explicitly implemented. |
| **Local storage** | Embedded SQLite via `rusqlite` with versioned `user_version` migrations; async process I/O never holds the DB lock. |
| **Environment secrets** | Only non-secret environment values are persisted; `isSecret: true` is rejected until OS-backed secure storage is added. |
| **App diagnostics** | Lifecycle and process events are written locally via `tauri-plugin-log`, excluding service arguments and environment values. |
| **UI architecture** | Focused in-house React components and hooks on a light, no-scroll desktop canvas (`1120 × 760` default, `860 × 700` minimum). |

See [PLAN.md](PLAN.md) for the full engineering roadmap and phase status.

## Development

### Prerequisites

- **Node.js** (v22+ recommended) and **pnpm** (v10+)
- **Rust** stable toolchain (`rustfmt` and `clippy` components)
- **Linux only:** Tauri 2 system dependencies (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libxdo-dev`, `libssl-dev`) — see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

### Run Locally

```sh
git clone https://github.com/akashpoudelnp/processhub.git
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
| **Backend** | Rust (2024 edition), Tokio, `rusqlite` (bundled SQLite), `sysinfo` |
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
- **Secret environment variables** are intentionally rejected until OS keychain integration is implemented.
