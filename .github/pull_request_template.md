## What this changes

## Slice / issue

## Checklist
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass
- [ ] Tested on Windows **and** Linux (or CI covers it)
- [ ] No network access outside `morph-updater` (`python3 scripts/offline_guard.py`)
- [ ] UI changes are reflected in the S2 component gallery
- [ ] New operations have conversion-matrix tests and never modify the source
