# Development

## prereqs

- Node.js + pnpm
- Rust toolchain
- tauri system deps for your OS (see tauri docs)
- git on PATH (repo mode executes git via tauri-plugin-shell)

## Run

- Install deps: pnpm install
- Start app (tauri): pnpm tauri dev
- Start web-only dev server: pnpm dev

## Build

- Web build: pnpm build
- tauri build: pnpm tauri build

## Notes

- The app writes narrative metadata under .narrative/ when you open a repo.
