# Metamorphosis — build plan, commit by commit

Each slice from the README is broken into small commits, in order. Messages follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`.

**Scopes:** `repo` `ci` `core` `cli` `input` `platform` `hud` `app` `image` `pdf` `media` `office` `archive` `ml` `recipes` `pkg` `site`

**Rules**
- One commit = one reviewable change that builds and passes tests on both OSes.
- The last commit of a slice makes its "done when" pass; then tag the release.
- If a commit gets too big, split it — the list below is the minimum, not a limit.
- Use a branch per slice (`s03-ring-overlay`), merge to `main` with a PR when CI is green.

---

## S0 — Repo, CI and release pipeline
```
chore(repo): add README, FEATURE_AUDIT, design mockup and docs site
chore(repo): add MIT/Apache-2.0 licence, .gitignore, .editorconfig, rustfmt.toml
chore(repo): create cargo workspace with morph-core and morph-cli crates
feat(cli): add `morph --version` and `morph --help`
chore(app): scaffold Tauri 2 app "Metamorphosis" in app/
feat(app): run in background with tray icon and Quit menu
feat(app): add About screen with version and Stand stat card
ci: run fmt, clippy and tests on windows-latest and ubuntu-22.04
ci: add offline guard that fails on network crates outside the updater
ci(pkg): build MSI + NSIS on Windows and AppImage + .deb on Linux on tag
chore(repo): add CONTRIBUTING and issue/PR templates
```
**Tag:** `v0.0.1`

## S1 — Core, naming and image conversions (CLI first)
```
feat(core): define Operation trait, input kinds and parameter schema
feat(core): add operation registry filtered by file type
feat(core): detect file type by magic bytes with extension fallback
feat(core): add naming templates for convert and tool outputs
feat(core): number output names on collision instead of overwriting
feat(core): write outputs atomically via temp file and rename
feat(core): add job runner with progress events
feat(image): convert between JPG, PNG, WebP, TIFF, AVIF and BMP
feat(image): read SVG input via resvg
feat(image): combine images into one PDF
feat(image): read HEIC input as optional component (libheif)
feat(cli): add `morph convert --to <fmt> <files...>`
test(core): verify source hash unchanged after every operation
test(image): add conversion matrix tests with fixture images
```
**Tag:** `v0.1.0`

## S2 — Design system: HUD tokens, fonts and motion
```
feat(hud): add design tokens as CSS custom properties
feat(hud): bundle Chakra Petch and JetBrains Mono WOFF2 with OFL licences
feat(hud): add SVG ring component with segments, numbers and centre readout
feat(hud): add decorative rings, crosshair ticks and backing disc
feat(hud): add boot-up draw-in and flicker animation
feat(hud): add idle rotation and scan-line shimmer
feat(hud): add drop collapse, pulse and SAVED toast
feat(hud): add configurable accent with contrast check
feat(hud): add reduced-motion and forced-colours variants
feat(hud): add expanded-mode panels (target, action list, output preview)
feat(hud): add component gallery page with every state
test(hud): add visual snapshot tests in WebView2 and WebKitGTK
perf(hud): measure idle frame time and drop glow when over budget
```
**Tag:** `v0.2.0`

## S3 — HUD ring overlay and drag detection
```
feat(input): add morph-input-watch crate with DragWatcher trait
feat(input): detect drag and key taps on Windows with low-level hooks
feat(input): detect drag and key taps on X11 with XInput2
feat(input): confirm real file drags on X11 via XdndSelection owner
feat(input): add tap/switch/cancel state machine (Shift, Ctrl, Alt, Esc)
feat(platform): add transparent topmost overlay window on Windows
feat(platform): accept drops on the overlay via OLE IDropTarget
feat(platform): add ARGB overlay accepting XDND drops on X11
feat(platform): fall back to XShape circular window without compositor
feat(app): show ring under cursor within 50 ms of key tap
feat(app): build ring segments from registry for dragged files
feat(app): run job on drop and show SAVED toast and progress card
feat(app): add keys 1–7 and arrow navigation in the ring
feat(app): add "Show in folder" to the progress card
fix(input): ignore window drags that carry no files
test(app): add manual test checklist for Explorer, Nautilus, Dolphin
```
**Tag:** `v0.3.0` — first release worth showing people

## S4 — PDF ring
```
feat(pdf): load PDFs with PDFium and bundle the library per OS
feat(pdf): render all pages to PNG and JPG at 300 DPI
feat(pdf): save multi-page output into one folder
feat(pdf): extract text to TXT
feat(pdf): convert to Markdown with heading and list heuristics
feat(pdf): convert to DOCX with paragraphs and images
test(pdf): verify DOCX opens in LibreOffice in CI
```
**Tag:** `v0.4.0`

## S5 — Image tools I: Compress, Crop, Metadata
```
feat(image): compress to target size, percentage or preset
feat(image): resize while compressing
feat(image): crop with presets and exact W×H
feat(image): read and edit EXIF, XMP and IPTC fields
feat(image): strip all metadata from a selection
feat(app): add crop editor in expanded mode
feat(app): add metadata editor in expanded mode
test(image): verify no GPS tags remain with exiftool
```
**Tag:** `v0.5.0`

## S6 — Image tools II: Edit, Annotate, Add BG, Collage
```
feat(image): add adjustment pipeline (exposure, contrast, colour, detail)
feat(image): add dehaze, clarity, grain and noise reduction
feat(app): add photo editor with live WebGL preview
test(image): verify preview matches export within ΔE < 2
feat(app): add annotate tools (arrows, shapes, text, numbered callouts)
feat(app): add magnifier and freehand highlight
feat(image): render backgrounds (solid, gradient, image, blur)
feat(app): add background editor (padding, radius, shadow, size presets)
feat(image): render collages (grid, row, column, featured)
feat(app): add collage editor
```
**Tag:** `v0.6.0`

## S7 — Redact and local AI
```
feat(ml): add ONNX Runtime engine as optional component
feat(ml): download models with checksum verification
feat(image): redact areas with solid colour or blur
feat(ml): detect faces locally
feat(ml): detect text regions locally
feat(app): add redact editor with movable layered areas
feat(ml): remove backgrounds (optional)
feat(ml): add OCR for Copy text and searchable PDF (optional)
test(ml): run detection suite with network disabled
```
**Tag:** `v0.7.0`

## S8 — Chaining, saved recipes and undo
```
feat(core): add job graph for chained operations
feat(recipes): save a chain as a TOML recipe
feat(app): show saved recipes as ring segments
feat(cli): add `morph run <recipe> <files...>` and `morph chain`
feat(core): record job outputs for undo
feat(app): add Undo last job (outputs to Recycle Bin/Trash)
```
**Tag:** `v0.8.0`

## S9 — Media conversions (FFmpeg)
```
feat(media): bundle LGPL FFmpeg as a sidecar per OS
feat(media): convert between video formats and to GIF
feat(media): extract audio from video to MP3
feat(media): convert between audio formats
feat(media): compress video and audio to a target size
feat(media): use hardware encoders when available, else software
test(media): add conversion matrix tests
```
**Tag:** `v0.9.0`

## S10 — Video tools
```
feat(media): generate preview proxies and frame strips
feat(app): add video timeline with frame stepping and timecodes
feat(media): trim video
feat(media): crop video keeping audio
feat(media): change speed with pitch preservation
feat(media): join clips using first clip as canvas
feat(media): save selected frames as PNG
feat(media): split at points or into equal parts
feat(media): redact time-ranged regions
```
**Tag:** `v0.10.0`

## S11 — Audio tools
```
feat(app): add waveform view
feat(media): normalize with two-pass loudnorm and meters
feat(media): create waveform or still-image video from audio
feat(media): trim audio and auto-trim silence
feat(media): convert channels with per-channel gain
feat(media): bleep ranges with A/B playback
```
**Tag:** `v0.11.0`

## S12 — PDF tools
```
feat(pdf): merge PDFs in selection order
feat(app): reorder documents before merging
feat(app): add page organizer with thumbnails
feat(pdf): rotate, duplicate and delete pages
feat(pdf): compress PDFs by downsampling images
```
**Tag:** `v0.12.0`

## S13 — Documents, slides, spreadsheets, subtitles
```
feat(office): bundle pandoc sidecar
feat(office): convert DOCX, Markdown, TXT, EPUB and HTML; read RTF
feat(office): add LibreOffice as optional component
feat(app): show INSTALL COMPONENT on segments needing it
feat(office): convert slides to PDF and images
feat(office): convert PDF to PPTX (page images) and EPUB
feat(office): read and write XLSX, ODS, CSV, TSV; export PDF
feat(office): read PAGES files (best effort)
feat(office): convert between SRT and VTT; export TXT
feat(office): extract PDF tables to XLSX (experimental)
```
**Tag:** `v0.13.0`

## S14 — Archives
```
feat(archive): create ZIP, TAR and GZIP from a selection
feat(archive): extract ZIP, TAR, GZIP, 7z and RAR via libarchive
fix(archive): reject path-traversal entries
```
**Tag:** `v0.14.0`

## S15 — Actions ring
```
feat(platform): copy files to clipboard as files (Windows and Linux)
feat(app): add Copy path and Move to…
feat(platform): open folder in terminal (Windows Terminal, GNOME Terminal, Konsole…)
feat(platform): share via Windows share UI, KDE Connect or LocalSend
feat(app): add Float shelf with drag in and drag out
feat(app): support folders and mixed selections in Actions ring
feat(app): use clipboard image as a source
```
**Tag:** `v0.15.0`

## S16 — Keyboard, file-manager integration, touch
```
feat(platform): add Explorer context menu via IExplorerCommand
feat(platform): add Dolphin service menu
feat(platform): add Nautilus, Nemo and Thunar actions
feat(app): open ring in expanded mode from file-manager menus
feat(platform): add hotkey on Explorer selection
feat(app): announce segments to screen readers
feat(input): support hold-then-drag on Windows touch screens
```
**Tag:** `v0.16.0`

## S17 — Wayland mode
```
feat(platform): detect Wayland and skip input hooks
feat(platform): add HUD-styled drop dock
feat(input): summon dock via GlobalShortcuts portal
feat(platform): add layer-shell overlay on KDE and wlroots
feat(app): explain Wayland limits on first run
```
**Tag:** `v0.17.0`

## S18 — Settings, customization and distribution
```
feat(app): add settings in expanded mode (keys, accent, motion)
feat(app): add ring layout customization per file type
feat(app): add naming template and conflict policy settings
feat(app): add job history with Show in folder and Run again
feat(app): add autostart and portable mode
feat(app): add French translation
feat(pkg): sign Windows installers
feat(pkg): publish winget manifest with moniker morph
feat(pkg): add AppImage zsync updates
feat(pkg): publish Flathub manifest
feat(pkg): publish morph-bin to AUR
```
**Tag:** `v1.0.0` — first stable release

## S19 — Extensibility: recipes, watch folders, plugins
```
feat(recipes): load user TOML recipes wrapping local commands
feat(recipes): hot-reload recipes without restart
fix(recipes): block writes outside the output path
feat(app): show recipe stderr on failure
feat(recipes): add watch folders
docs: document the Operation trait for engine crates
```
**Tag:** `v1.1.0`

## S20 — Website
```
feat(site): replace placeholder GIFs and screenshots with real ones
feat(site): add og:image and real package IDs
ci(site): add Lighthouse accessibility check (≥ 95)
```
The site already deploys from S0; this slice finishes it.
