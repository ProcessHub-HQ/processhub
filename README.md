# ProcessHub

ProcessHub is a local desktop control center for processes and services. It uses Tauri 2, React, TypeScript, and Rust. Service definitions are stored in a local SQLite database; configured commands run directly without shell interpolation.

## Development

Requirements: Node.js, pnpm, and the stable Rust toolchain. On Linux, install the Tauri 2 WebKitGTK and GTK system dependencies described in the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
pnpm install
pnpm tauri dev
```

Useful checks:

```sh
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm lint
pnpm format:check
```

## Current implementation

- Rust process supervisor with direct executable and argument-vector execution.
- SQLite service, group, environment, dependency, and app-settings schema.
- Typed Tauri commands for service and group configuration, runtime state, and process control.
- stdout/stderr events with bounded buffering.
- Native image and directory pickers. Selected PNG/JPEG/WebP icons are validated and copied into app-managed storage before their path is saved.
- Runtime and app events are written through Tauri's log plugin to the app log directory; log messages exclude environment values and command arguments.
- Light-only, no-scroll app canvas with process icons, right-click management actions, New, Settings, and an About dialog. The window starts at 1120 × 760 and cannot be resized below 860 × 700.
- Non-secret environment values are supported. Secret values are rejected until OS-backed secure storage is added.

Process-tree cleanup currently uses POSIX process groups on macOS/Linux and `taskkill /T` with a force-kill fallback on Windows. Process manager tests cover a grandchild on POSIX; a Windows tree test runs in the CI matrix. The supervisor is owned by the ProcessHub app process, so services are not guaranteed to survive an app crash or quit.

See [PLAN.md](PLAN.md) for phase status and [docs/architecture.md](docs/architecture.md) for implementation decisions.
