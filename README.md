# Metamorphosis

**Drag a file, tap a key, drop it on the ring.** Metamorphosis converts and edits files from a holographic ring that opens under your cursor mid-drag — on Windows and Linux, fully offline.

The original is never modified. Every result is saved as a new file next to the source.

Short name: **`morph`** — used for the CLI, binaries, crates, package IDs and the repo slug.

```sh
morph convert --to jpg file.pdf
```

> Metamorphosis is an independent open-source project inspired by the drag-to-convert interaction of Tangerine for Mac. It is not affiliated with Tangerine or Leap Studio and uses none of their name, logo, icons, art or code. The name is a nod to Philip Glass's album *Metamorphosis*, in the tradition of JoJo's Bizarre Adventure Stands named after songs and albums; it is not affiliated with either.

[Website](https://YOUR-GITHUB-USER.github.io/morph/) · [Build plan](BUILD_PLAN.md) · [Feature audit](FEATURE_AUDIT.md) · [Releases](https://github.com/YOUR-GITHUB-USER/morph/releases)

---

## Stand stats

```
┌──────────────────────────────────────────────────────────────┐
│  「METAMORPHOSIS」                         User: you         │
├──────────────────────────────────────────────────────────────┤
│  Power                  B                                    │
│  Speed                  A                                    │
│  Range                  C   only what the user drags         │
│  Durability             A   originals are never destroyed    │
│  Precision              A                                    │
│  Development potential  A                                    │
└──────────────────────────────────────────────────────────────┘
```

| Power | Speed | Range | Durability | Precision | Development potential |
|:---:|:---:|:---:|:---:|:---:|:---:|
| **B** | **A** | **C** — only what the user drags | **A** — originals are never destroyed | **A** | **A** |

---

## How it works

1. Start dragging one or more files (or a folder) in your file manager.
2. Tap a key — you don't need to hold it:
   | Key (default) | Ring | Contents |
   |---|---|---|
   | `Shift` | **Convert** | Output formats valid for the dragged type |
   | `Ctrl` | **Tools** | Compress, metadata, crop, trim, merge… for that type |
   | `Alt` | **Actions** | Share, Float, Copy, Move, Copy path, Terminal |
3. Keep dragging. The ring stays open until you drop or press `Esc`. Tap another trigger key to switch rings, or press `1`–`7` to pick a segment.
4. Drop on a segment. The ring collapses into a pulse and confirms: `✓ SAVED · Page 001 Annotated.jpg`.
5. Drag the new file again to chain (PDF → JPG → Cropped → With BG), or save the chain as a recipe.

**Output naming (default):** conversions keep the name and change the extension (`Page 001.pdf` → `Page 001.jpg`); tools append the operation (`Page 001 Cropped.jpg`, `Page 001 With BG.png`). Collisions get a number (`Page 001 Cropped 2.jpg`). The template is configurable.

Keys are configurable. Keyboard-only and file-manager entry points exist where drag detection isn't possible (see [Wayland](#wayland)).

## Principles

- **Offline, always.** The core has no network code. CI fails if a network call is introduced outside the updater. Fonts and assets are bundled.
- **Non-destructive.** Write to a temp file, then atomic rename to a fresh name. Never overwrite; never touch the source.
- **CLI parity.** Anything the ring can do, `morph` can do from a shell.
- **Each slice ships on Windows and Linux.** A slice isn't done until both pass its "done when".

## UI design — holographic HUD ring

The ring is a sci-fi holographic interface (FUI — think JARVIS in *Iron Man*, *Minority Report*, *Oblivion*). It must stay readable on any wallpaper, feel instant, and never block the drop.

Reference mockup: `design/ring-mockup.html` (image ring, 7 segments, expanded mode with side panels).

### Anatomy (sizes for the 600 px reference; the ring scales with display DPI)

| Layer (back → front) | Spec |
|---|---|
| Backing disc | `r 262`, fill `rgba(4,10,18,0.72)` — keeps the ring legible on light and busy wallpapers |
| Outer tick ring | `r 282`, 1 px, `--hud-line` at 45 %, dash `2 8`, rotates **CW 40 s** |
| Data ring | `r 268`, 2 px, `--hud-line` at 70 %, dash `120 40 12 40 60 80`, rotates **CCW 22 s** |
| Sweep arc | `r 248`, 1.5 px, `--hud-accent` at 60 %, dash `30 400`, rotates **CW 9 s** |
| Guide ring | `r 240`, 1 px, `--hud-line` at 25 %, static |
| Segments | Annular sectors `r 122–232`, gap ≈ 2°, stroke 1.5 px `--hud-line` at 65 %, fill `--hud-line` at 7 %, glow `drop-shadow(0 0 6px cyan 55 %)` |
| Segment labels | Chakra Petch 600, 14 px, letter-spacing 2 px, uppercase, `--hud-text` |
| Segment numbers | JetBrains Mono 9 px, `01`–`07` just outside each segment, `--hud-muted`; also keyboard shortcuts `1`–`7` |
| Centre disc | `r 108`, fill `rgba(4,10,18,0.9)`, 1 px `--hud-line` at 55 %, inner dotted ring `r 98` dash `1 5` rotating CCW 22 s |
| Centre readout | `ACTION` (Mono 10 px, muted) · hovered action (Chakra Petch 700, 24 px, `--hud-accent`) · `RELEASE TO APPLY` (Mono 10 px) · `1.1 MB · JPG` (Mono 10 px, muted) |
| Crosshair ticks | Four 20 px lines at N/E/S/W outside the ring |

**Hovered segment:** stroke `--hud-accent`, fill `--hud-accent` at 28 %, label `#FFFFFF`.

### Design tokens

| Token | Default | Notes |
|---|---|---|
| `--hud-line` | `#4FE3FF` | Cyan — outlines, ticks, structure |
| `--hud-accent` | `#FFB347` | Tangerine amber — hover, sweep arc, centre label, states. **User-configurable** (presets: amber `#FFB347`, cyan `#4FE3FF`, pink `#FF5EA8`, lime `#9CFF6E`, or any hex) |
| `--hud-text` | `#BDF4FF` | Labels and readouts |
| `--hud-muted` | `#6FB6C6` | Captions, numbers, secondary readouts |
| `--hud-backdrop` | `rgba(4,10,18,0.72)` | Disc behind the ring |
| `--hud-core` | `rgba(4,10,18,0.90)` | Centre disc and panels |
| `--hud-grid` | `#04070C` + 40 px cyan grid at 5 % | Expanded-mode and editor background only |
| `--font-label` | Chakra Petch 400/500/600/700 | Labels, headings |
| `--font-readout` | JetBrains Mono 400/500 | Numbers, file info, status |

Fonts are **bundled as WOFF2** in the app (both SIL OFL 1.1; licences shipped in `app/ui/hud/fonts/`). No font is ever fetched from the network.

The accent must keep ≥ 4.5:1 contrast for the centre label against `--hud-core`; the settings picker warns when a custom colour falls below that.

### Motion

| Moment | Spec |
|---|---|
| Boot-up (key tap) | Ring layers draw in via `stroke-dashoffset` over **240 ms**, staggered 30 ms outer → inner; one flicker (opacity 1 → 0.4 → 1 over 80 ms) at 120 ms. Segments are hit-testable from frame 0 — the animation never delays a drop. |
| Idle | Decorative rings rotate (40 s CW, 22 s CCW, 9 s CW sweep); a 120 px scan-line shimmer (cyan 5 %) passes over the disc every 6 s. |
| Hover | Fill/stroke transition 150 ms; centre label swaps instantly (no fade — it must be readable mid-drag). |
| Ring switch | Old segments scale to 0.9 and fade (100 ms), new ones draw in (160 ms). |
| Drop | Ring collapses to a 6 px dot at the centre (180 ms, ease-in), then one accent pulse ring expands and fades (400 ms). |
| Confirmation | Toast at the drop point: `✓ SAVED · Page 001 Annotated.jpg` (Chakra Petch + Mono), auto-hides after 1.6 s; becomes a progress card for long jobs (`2 of 5 saved`). |
| Cancel (`Esc`) | Ring fades out 120 ms, no pulse. |

**Reduced motion:** when the OS asks for reduced motion — Windows `UISettings.AnimationsEnabled` / `SPI_GETCLIENTAREAANIMATION`, GNOME `enable-animations`, KDE `AnimationDurationFactor = 0` — or the in-app setting is on: no rotation, no flicker, no scan-line, no collapse; the ring appears and disappears with a 100 ms fade. The platform value is passed into the webview explicitly, because WebKitGTK doesn't always map it to `prefers-reduced-motion`.

### Implementation

- **SVG + CSS animations** in the Tauri webview (WebView2 on Windows, WebKitGTK on Linux). Only `transform` and `opacity` animate; glow uses one `drop-shadow` on the segment group.
- **WebGL** only if real bloom is needed. Performance gate: if the overlay misses 60 fps (frame time > 16.7 ms on the reference low-end laptop), drop the glow filter before anything else.
- **Show latency budget:** ≤ 50 ms from key tap to first ring frame. The overlay window is created hidden at startup and reused.
- **Transparency on Linux** needs a compositor. On X11 without one (e.g. bare i3/Openbox), the overlay falls back to a circular window via the XShape extension with an opaque backdrop. Windows uses a layered, click-through-except-ring window.
- **High contrast:** Windows High Contrast / forced colours switch the ring to system colours, no glow.
- **Accessibility:** segments are focusable and labelled; `1`–`7` and arrow keys work; screen readers announce `Annotate, 4 of 7`.

### Expanded mode (editors and settings)

Same style, full panel on the `--hud-grid` background with corner brackets:

- **Left — `TARGET ACQUIRED`:** file thumbnail, `FILE`, `TYPE`, `SIZE`, `DIM`, `STATE` (`ARMED` in accent).
- **Centre:** the ring, or the active editor canvas (crop, annotate, background…).
- **Right — `ACTIONS · KEYS 1–7`:** the action list as buttons, plus `OUTPUT → Page 001 Annotated.jpg`, `SOURCE UNTOUCHED · SAVED BESIDE ORIGINAL` and the live status line.
- **Header:** blinking accent dot, `METAMORPHOSIS`, `// STAND ACTIVE · v0.x`.

## Architecture

```
crates/
  morph-core          Operation registry, job graph (chains), naming, atomic output, progress events
  morph-engines       image · pdf (PDFium) · media (FFmpeg sidecar) · office (LibreOffice/pandoc sidecars)
                      · archive · ml (ONNX Runtime: faces, text, background)
  morph-cli           `morph` binary — same registry as the app
  morph-input-watch   Drag + trigger-key detection
    ├─ windows        WH_MOUSE_LL / WH_KEYBOARD_LL
    ├─ x11            XInput2 raw events on root (no grabs), XdndSelection owner check
    └─ wayland        xdg-desktop-portal GlobalShortcuts (dock summon only)
  morph-platform      Overlay window, clipboard, share, terminal, file-manager integration
    ├─ windows        Layered topmost overlay as OLE IDropTarget, IShellWindows, IExplorerCommand
    ├─ x11            ARGB override-redirect overlay as XDND target; XShape fallback without compositor
    └─ wayland        Drop dock; layer-shell overlay where supported
app/                  Tauri 2 shell "Metamorphosis": tray, HUD ring overlay, editors, settings, About
  ui/hud/             Design tokens, SVG ring component, motion, bundled fonts
design/               Mockups (ring-mockup.html) and token source
docs/                 GitHub Pages site (static, no build step)
recipes/              Built-in TOML recipes (also the format users extend)
```

**Operation model.** Each operation declares input kinds (MIME + magic bytes), output kind, a parameter schema (rendered as the tool UI and as CLI flags) and whether it needs an editor. Rings are built at drop time from the registry, filtered by the dragged selection. A ring shows up to 7 segments (keys `1`–`7`); more items go into a `MORE` segment that opens a second page.

**Engines and licences**

| Engine | Used for | Licence note |
|---|---|---|
| `image`, `ravif`, `resvg`, `oxipng`, mozjpeg, `webp` | Images | Permissive |
| libheif + libde265 | HEIC input | LGPL; HEVC patents → optional component |
| PDFium (`pdfium-render`) | PDF render/text/merge | BSD/Apache |
| `lopdf` | PDF structure edits, compression | MIT |
| FFmpeg (sidecar process) | Audio/video | LGPL build only by default |
| LibreOffice headless (optional download) | Slides, office ↔ PDF, PAGES input | MPL |
| pandoc (sidecar) | DOCX/MD/EPUB/HTML/TXT | GPL, separate process |
| `calamine`, `rust_xlsxwriter` | Spreadsheets | MIT/Apache |
| `zip`, `tar`, `flate2`, libarchive | Archives, RAR input | Permissive / BSD |
| ONNX Runtime (`ort`) + small models | Faces, text, background removal | Check each model's licence |
| Chakra Petch, JetBrains Mono | UI fonts (bundled) | SIL OFL 1.1 |

Heavy engines (LibreOffice, ML models, HEIC) are **optional components** downloaded on first use, so the base install stays small.

## Platform support

| | Windows 10 22H2+ / 11 | Linux X11 | Linux Wayland |
|---|---|---|---|
| Drag + key → ring at cursor | ✅ | ✅ (translucent with a compositor, shaped without) | ⚠️ see below |
| File-manager menu entry | ✅ Explorer | ✅ Dolphin, Nautilus, Nemo, Thunar | ✅ same |
| Keyboard ring on selected files | ✅ | ⚠️ via file-manager integration | ⚠️ same |
| Drop dock / Float shelf | ✅ | ✅ | ✅ (edge-pinning: KDE, wlroots) |

### Wayland

Wayland deliberately prevents apps from reading global keyboard input, seeing the cursor position outside their own windows, or placing windows at arbitrary coordinates. Metamorphosis therefore can't open the ring under your cursor mid-drag in another app. Instead:

- **Drop dock:** a small always-available target. Drag a file onto it; the ring opens there.
- **Global shortcut** via the `GlobalShortcuts` portal (KDE Plasma, GNOME 48+, others vary) to summon the dock.
- **Layer-shell overlay** on KDE and wlroots compositors (Sway, Hyprland) for a full-screen drop surface.
- **File-manager menu entries** work everywhere.
- Running an X11 session gives the full experience.

## Build slices

Each slice is shippable on both platforms. Order is the planned build order. The commit-by-commit breakdown of every slice is in [BUILD_PLAN.md](BUILD_PLAN.md).

### S0 — Repo, CI and release pipeline
Cargo workspace (`morph-core`, `morph-cli`, `morph-input-watch`, …), Tauri 2 app skeleton titled "Metamorphosis", tray icon, About screen with the Stand stat card, `morph --version`. CI on `windows-latest` and `ubuntu-22.04`: fmt, clippy, tests, offline guard. Tag → MSI + NSIS (Windows), AppImage + .deb (Linux) as release assets.
**Done when:** pushing a `v0.0.x` tag produces installable artifacts for both OSes that start, show a tray icon, open the About screen and quit cleanly; `morph --version` prints the same version.

### S1 — Core, naming and image conversions (CLI first)
Operation registry, type detection, job runner with progress events, atomic outputs, naming templates (convert: `{stem}.{ext}`; tools: `{stem} {Op}.{ext}`) with collision numbering. Image conversions: JPG, PNG, WebP, TIFF, AVIF, BMP; SVG input; image → PDF; HEIC input as optional component.
**Done when:** `morph convert --to png photo.heic` and `morph convert --to webp *.png` produce correct files on both OSes; the source hash is unchanged; a second run creates `photo 2.png`.

### S2 — Design system: HUD tokens, fonts and motion *(new)*
`app/ui/hud/`: design tokens as CSS custom properties (the table above), configurable accent with contrast check, bundled Chakra Petch + JetBrains Mono (WOFF2 + OFL licences), the SVG ring component (backing disc, decorative rings, segments, numbers, centre readout, crosshair ticks), motion specs as named CSS animations (boot, idle rotation, scan-line, hover, switch, collapse + pulse, toast), reduced-motion and forced-colours variants, and the expanded-mode panel components (`TARGET ACQUIRED`, action list, output preview, header). A standalone `design/ring-mockup.html` and a component gallery page render every state.
**Done when:** the gallery renders identically (visual snapshot diff < 1 %) in WebView2 and WebKitGTK; the idle ring holds 60 fps on the reference low-end laptop on both OSes; with reduced motion on (Windows setting and GNOME `enable-animations=false`) nothing rotates or flickers; an offline build shows the correct fonts.

### S3 — HUD ring overlay and drag detection
`morph-input-watch` (Windows hooks; X11 XInput2 + XdndSelection check) drives the S2 ring in a transparent overlay: three rings with tap-to-open, switch rings mid-drag, `1`–`7` and arrows, `Esc` cancels; the ring accepts the drop and runs S1 jobs, then collapses with the pulse and the `✓ SAVED` toast / progress card ("Show in folder"). Multi-file selections. XShape fallback on X11 without a compositor.
**Done when:** on Windows 11 (Explorer) and an X11 session (Nautilus and Dolphin, with and without a compositor), dragging a JPG, tapping `Shift` and dropping on PNG saves `photo.png` beside it in under 1 s; the first ring frame appears ≤ 50 ms after the key tap; tapping `Ctrl` mid-drag switches rings; dragging a window (not a file) never shows the ring.

### S4 — PDF ring
PDFium: PDF → JPG/PNG (all pages, 300 DPI, `Page 001.jpg`… in one folder for multi-page), TXT, Markdown (headings/lists heuristics), DOCX v1 (paragraphs + images); multiple images → one PDF.
**Done when:** a 10-page PDF converts to 10 PNGs at 300 DPI and to a DOCX that opens in Word and LibreOffice with readable flowing text, on both OSes.

### S5 — Image tools I: Compress, Crop, Metadata
Compress by target size, percentage or preset, with optional resize; batch. Crop with presets (1:1, 4:5, 16:9, custom) and W×H fields. Metadata viewer/editor (camera, location, author), strip-all on a selection. Editors use S2 expanded mode.
**Done when:** compressing a 6 MB JPG to "under 1 MB" yields ≤ 1 MB; a crop to 1080×1080 is pixel-exact; "Remove metadata" leaves no GPS tags (verified with exiftool in CI).

### S6 — Image tools II: Edit, Annotate, Add BG, Collage
Edit: exposure, contrast, color, detail, dehaze, clarity, grain, noise reduction with live WebGL preview and identical Rust export. Annotate: arrows, shapes, text, numbered callouts, magnifier, freehand/highlight. Add BG: solid, gradient, swatches, image-as-background, blur, padding, radius, shadow, size presets. Collage: grid/row/column/featured, size, spacing, corners, background.
**Done when:** each editor opens from the Tools ring in expanded mode, preview matches export within ΔE < 2, and output is saved as `{stem} Edited|Annotated|With BG|Collage.{ext}` on both OSes.

### S7 — Redact and local AI
Manual redaction rectangles (movable, layered, solid or blur), plus local face and text detection via ONNX Runtime (CPU). Optional: background removal and OCR (Copy text, searchable PDF).
**Done when:** on a test photo set, auto-detect finds ≥ 90 % of faces/text regions; a network-disabled CI run passes; models download as an optional component with checksum verification.

### S8 — Chaining, saved recipes and undo
Re-drag outputs to chain; "Save as recipe" stores the chain as TOML and adds it as a ring segment. Undo last job (outputs → Recycle Bin/Trash).
**Done when:** a saved recipe "PDF → JPG → Crop 1:1 → BG preset" runs from one segment and from `morph run <recipe> file.pdf` with identical output on both OSes; Undo removes exactly that job's outputs.

### S9 — Media conversions (FFmpeg)
Video: MP4, MOV, MKV, WebM, AVI, WMV, GIF; video → MP3. Audio: MP3, M4A, WAV, FLAC, OGG, Opus, AIFF, WMA. Compress video/audio by target size. Hardware encoding where available (NVENC/QSV/AMF/VA-API) with software fallback.
**Done when:** the conversion matrix test suite passes on both OSes using the bundled LGPL FFmpeg; a 100 MB MP4 compressed to "25 MB" lands within ±5 %.

### S10 — Video tools
Trim (frame step, timecodes), Crop (exact size, keep audio), Speed (pitch-preserving), Join (reorder, mixed containers, first clip as canvas), Save frames (full-res PNG), Split (points or equal parts into one folder), Redact (time-ranged regions, solid/blur). Linux preview uses FFmpeg proxies so it doesn't depend on GStreamer plugins.
**Done when:** each tool produces the expected duration/frames (±1 frame) on both OSes, and the trim editor scrubs smoothly on a stock Ubuntu install.

### S11 — Audio tools
Normalize (two-pass loudnorm, LRA and true-peak controls, before/after meters), Visualizer (waveform or still image → MP4, landscape/portrait/square), Trim (waveform, auto-trim silence), Channels (mono/stereo, per-channel gain), Bleep (ranges, A/B playback).
**Done when:** normalizing hits the target LUFS within ±0.5 LU and every tool's output plays in VLC on both OSes.

### S12 — PDF tools
Merge (selection order, reorder before merging), Organize pages (reorder, rotate, duplicate, delete with thumbnails), Compress PDF (image downsampling, object streams).
**Done when:** merging 3 PDFs preserves page count and links; organize round-trips without losing text selectability.

### S13 — Documents, slides, spreadsheets, subtitles
Pandoc: DOCX ↔ Markdown ↔ TXT ↔ EPUB, HTML export, RTF input. LibreOffice (optional): PPT/PPTX → PDF/JPG/PNG, office → PDF, PAGES input (best effort). PDF → PPTX (page images), PDF → EPUB, PDF → XLSX (experimental). Spreadsheets: XLSX/ODS/CSV/TSV, XLS input, PDF export. Subtitles: SRT ↔ VTT, TXT export.
**Done when:** the format-matrix test suite passes on both OSes with and without LibreOffice installed (segments needing it show `INSTALL COMPONENT` instead of failing).

### S14 — Archives
Create ZIP/TAR/GZIP from a selection; extract ZIP/TAR/GZIP/7z/RAR (libarchive) into a sibling folder.
**Done when:** round-trip tests pass, and extraction rejects path-traversal (`../`) entries.

### S15 — Actions ring
Share (Windows share UI; Linux KDE Connect/LocalSend when installed), Float shelf (drag in, pin to edge, drag out — HUD-styled), Copy to clipboard as files, Move to…, Copy path (one per line), Open in terminal. Works on folders and mixed selections. Clipboard-as-source for images.
**Done when:** each action works from Explorer, Nautilus and Dolphin; Copy pastes as real files in the native file manager; Terminal opens in the right folder on Windows Terminal, GNOME Terminal and Konsole.

### S16 — Keyboard, file-manager integration, touch
Explorer context menu (IExplorerCommand, Windows 11 top-level menu), Dolphin service menu, Nautilus script/extension, Nemo action, Thunar custom action — each opens the ring in expanded mode. Keyboard ring: arrows, Enter, Esc, `1`–`7`. Windows: hotkey on the Explorer selection. Touch: hold-then-drag on Windows.
**Done when:** every ring operation is reachable without a mouse from Explorer and from Dolphin/Nautilus; a screen reader announces segment names (NVDA, Orca).

### S17 — Wayland mode
Drop dock (HUD-styled), GlobalShortcuts portal, layer-shell overlay where supported, first-run explanation of what's limited and why.
**Done when:** on GNOME Wayland and KDE Plasma Wayland, a user can convert a file via the dock and via the file-manager menu, and the app never crashes or hangs trying to hook input.

### S18 — Settings, customization and distribution
Settings (expanded mode): trigger keys, accent colour (presets + custom with contrast check), motion (full / reduced / follow system), ring layout (reorder, pin, hide per type), naming template, conflict policy, job history, autostart, language (EN/FR first), portable mode (Windows). Distribution: signed MSI, winget manifest (moniker `morph`), AppImage with zsync updates, .deb, Flathub, AUR (`morph-bin`).
**Done when:** `winget install morph`, `flatpak install flathub io.github.YOUR-GITHUB-USER.morph` and `yay -S morph-bin` all install a working build; settings (including accent) survive upgrades.

### S19 — Extensibility: recipes, watch folders, plugins
User recipes in TOML wrapping any local command (inputs, outputs, parameters → UI), watch folders that apply a recipe, documented `Operation` trait for engine crates.

```toml
# ~/.config/morph/recipes/thumbnail.toml   (Windows: %APPDATA%\morph\recipes\)
name    = "Thumbnail 512"
ring    = "tools"
accepts = ["image/*"]
output  = "{stem} Thumb.webp"
run     = ["magick", "{input}", "-thumbnail", "512x512", "{output}"]
```

**Done when:** a recipe file dropped in the folder appears in the right ring without restart on both OSes, failures show the command's stderr, and recipes cannot write outside the output path.

### S20 — Website
`docs/` static site deployed by GitHub Actions to GitHub Pages: hero with a live HUD ring demo, features, how it works, install per OS, Wayland limits, FAQ.
**Done when:** the Pages workflow deploys on push to `main`, Lighthouse accessibility ≥ 95, and the page works without JavaScript except the ring demo and copy buttons.

## CLI

```sh
morph convert --to jpg file.pdf
morph convert --to png --dpi 300 report.pdf
morph tool compress --target 1MB photo.jpg
morph tool crop --ratio 1:1 photo.jpg
morph chain report.pdf "convert:jpg" "crop:1:1" "bg:preset=soft"
morph run thumbnail *.png
morph list --for clip.mov
```

## Contributing

Open an issue before large changes. New engines implement `Operation` in `morph-engines` with a conversion-matrix test. UI changes must update the S2 component gallery. Every PR must pass on Windows and Linux CI.

## Licence

TBD (MIT or Apache-2.0 for Metamorphosis code; bundled components keep their own licences — see the engines table).
