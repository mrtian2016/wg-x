# Repository Guidelines

## Project Structure & Module Organization
Core React UI lives in `src/`, split into `components/` (reusable UI), `pages/` (route-level views like `TunnelManagementView`), `hooks/`, `utils/`, and global styles in `styles/`. Static assets stay in `public/`; generated builds land in `dist/`. Desktop-specific code resides in `src-tauri/`; `src-tauri/src/` contains the Rust services (tunnel lifecycle, daemon IPC) and `tauri.conf.json` controls bundle metadata. Automation scripts belong in `scripts/`, while reference screenshots should be added under `screens/` to keep PR documentation consistent.

## Build, Test, and Development Commands
Run `npm install` before first use; the project expects Node 18+. Use `npm run dev` for the Vite web preview and `npm run tauri dev` when you need the full desktop shell and WireGuard daemon bindings. `npm run build` compiles the web assets; ship-ready binaries are produced with `npm run tauri build`. Invoke `npm run version:update` to sync the desktop and web version numbers after a release cut.

## Coding Style & Naming Conventions
Follow the existing 2-space indentation and double-quote strings in React files. Name React components with `PascalCase` and hooks or utilities with `camelCase`. Keep stateful logic in custom hooks under `hooks/` and isolate network or updater helpers in `utils/`. Rust code in `src-tauri` should compile cleanly with `cargo fmt` and `cargo clippy`; prefer module-per-feature organization (see `tunnel.rs`, `daemon.rs`). CSS lives in plain files—co-locate new styles in `styles/` and import at the entry component.

## Testing Guidelines
Automated tests are not yet wired in; accompany features with manual verification notes covering tunnel creation, daemon lifecycle, and updater flows. If you introduce unit tests, colocate them alongside components as `*.test.jsx` using Vitest + React Testing Library, and wire a `test` script in `package.json`. Desktop changes must be exercised with `npm run tauri dev`; include logs from `src-tauri` when debugging daemon regressions.

## Commit & Pull Request Guidelines
Commit messages follow conventional commits with optional scopes (`refactor(TunnelManagementView): …`). Use English or Simplified Chinese summary lines, but keep the imperative mood. Pull requests should link related issues, describe UI or daemon impacts, and attach updated screenshots from `screens/` when visual changes occur. Note any manual test steps and mention whether version bumps or service restarts are required.
