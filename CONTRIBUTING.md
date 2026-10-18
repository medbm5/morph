# Contributing to Metamorphosis

Thanks for helping. A few ground rules keep the project small, offline and safe.

## Before you start

- Open an issue before large changes, so we can agree on the approach.
- Read the [README](README.md) principles: **offline, non-destructive, CLI parity, both OSes**.
- Work happens in slices; see [BUILD_PLAN.md](BUILD_PLAN.md) for the commit-by-commit plan.

## Setup

| Need | Windows | Linux |
|---|---|---|
| Rust (stable) | `winget install Rustlang.Rustup` | `curl https://sh.rustup.rs -sSf \| sh` |
| C/C++ toolchain | VS Build Tools with *Desktop development with C++* | `build-essential` |
| Node.js ≥ 20 (Tauri CLI only) | `winget install OpenJS.NodeJS.LTS` | distro package or nvm |
| System libs | WebView2 (preinstalled on Windows 11) | `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libx11-dev libxi-dev` |

```sh
cargo test --workspace          # core, engines, CLI
cargo run -p morph-cli -- --help
cd app && npm ci && npx tauri dev   # the desktop app
```

## Commits and PRs

- [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`, scopes listed in BUILD_PLAN.md.
- One commit = one reviewable change that builds and passes tests.
- Every PR must pass CI on Windows **and** Linux.

## Adding an operation

1. Implement `morph_core::Operation` in the right `morph-engines` module.
2. Declare accepted input kinds, the output kind and the parameter schema — the ring UI and CLI flags are generated from it.
3. Write the output through the `OutputSink` you're given (temp file + atomic rename). Never open the source for writing.
4. Add a conversion-matrix test with fixture files, and check the source hash is unchanged.

## UI changes

The HUD lives in `app/ui/hud/`. Any change must update the component gallery (`app/ui/gallery.html`) and respect reduced-motion and forced-colours.

## Network

Metamorphosis has no network code outside `crates/morph-updater`. `scripts/offline_guard.py` runs in CI and fails otherwise.
