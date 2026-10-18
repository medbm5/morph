# Metamorphosis (`morph`) — Feature audit vs. Tangerine for Mac

Source: tangerineformac.com (home, `/tools`, `/press`), read on 18 Oct 2026 — Tangerine v1.4, Leap Studio, $29.99 one-time, Apple silicon, macOS 13+.
"In README" refers to the README as it stood before this revision (PDF ring, image tools, chaining, Windows hooks / X11 / Wayland fallback). Every row now maps to a slice in the revised README (slice numbers updated after S2 "Design system" was inserted).

Difficulty: **L** low · **M** medium · **H** high · **XH** very high / needs a compromise.

## 1. Interaction model

| Feature | What it does | In README? | Windows | Linux | Notes → slice |
|---|---|---|---|---|---|
| Shift ring (conversions) | Drag a file, tap Shift, a format wheel opens at the cursor | Yes | M | M (X11) / XH (Wayland) | Windows: `WH_MOUSE_LL` + `WH_KEYBOARD_LL`, overlay is an OLE `IDropTarget`. X11: XInput2 raw events + check that `XdndSelection` has an owner (confirms a real drag). → S3 |
| Command ring (file tools) | Tap a second key for per-type editing tools | Partial (tools listed, no separate ring) | M | M / XH | No Command key: default **Ctrl**. → S3 |
| Control ring (file actions) | Third key for AirDrop/Float/Copy/Move/Copy Path/Terminal | No | M | M / XH | Default **Alt**. Linux WMs use Alt+drag at button-press time only; tapping mid-drag is safe. → S3, S15 |
| Tap-and-release, wheel stays open | Key is tapped, not held; wheel stays until drop or cancel | Unclear | L | L | State machine in core, platform-agnostic. → S3 |
| Switch wheels mid-drag | Tap another key to switch ring | No | L | L | → S3 |
| Configurable keys | Change trigger keys in Settings | No | L | L | → S18 |
| Keyboard flow | Press Enter on a selected file → format menu; arrows, Enter, Esc | No | M | H | Windows: Explorer selection via `IShellWindows`. Linux: no generic API → file-manager integrations. → S16 |
| Touch: hold then drag | On touch screens, hold a file briefly before dragging | No | M | H | Windows pointer hooks. Linux touch varies by compositor. Low priority. → S16 |
| Context-aware rings | Only relevant formats/tools appear for the dragged type | Implied | L | L | Registry filters by MIME + magic bytes. → S1 |
| Output beside source, original untouched | New copy saved next to the original | Yes | L | L | Atomic write (temp + rename), never overwrite. → S1 |
| Progress card | "1 of 1 saved" | Yes | L | L | Becomes the HUD `✓ SAVED` toast / progress card. → S3 |
| Ring visual style | Tangerine's own wheel design | n/a | M | M–H | Metamorphosis uses its own holographic HUD design (S2); translucency on Linux needs a compositor, XShape fallback otherwise. → S2, S3 |
| Chaining | PDF → JPG → Cropped → With BG | Yes | M | M | → S8 |
| Multi-file selections | Rings act on a whole selection | Partial | M | M | → S1/S3 |
| Folders and mixed selections | File actions work on folders and mixed types | No | L | L | → S15 |
| Free conversion allowance | Conversions are metered (actions are not) | n/a | — | — | Not copied: Metamorphosis is free and unmetered. |

## 2. Conversions (275 paths)

| Category | Tangerine formats | In README? | Windows | Linux | Notes → slice |
|---|---|---|---|---|---|
| Images | JPG PNG WebP HEIC TIFF AVIF BMP; PDF export; SVG input | No (only PDF→image) | L–M | L–M | `image`, `ravif`, `resvg`. HEIC via libheif + libde265 (LGPL, HEVC patents: make it an optional component). → S1 |
| Video | MP4 MOV MKV WebM AVI WMV GIF; MP3 export | No | M | M | FFmpeg sidecar, LGPL build. Avoid bundling GPL/non-free encoders by default. → S9 |
| Audio | MP3 M4A WAV FLAC OGG Opus AIFF WMA | No | L | L | FFmpeg sidecar. → S9 |
| PDF → images | JPG PNG, all pages at 300 DPI | Yes | L | L | PDFium. → S4 |
| PDF → TXT / MD | Text and Markdown | TXT only | L / M | L / M | Markdown needs heading/list heuristics. → S4 |
| PDF → DOCX | Editable Word | Yes | H | H | Hardest conversion. v1 = paragraphs + images; tables later. → S4, S13 |
| PDF → PPTX / XLSX | Slides and spreadsheets | No | M / H | M / H | PPTX v1 = one page image per slide. XLSX = table extraction, experimental. → S13 |
| PDF → EPUB | E-book | No | M | M | Reflowed text + images. → S13 |
| PDF → PAGES | Apple Pages | No | XH | XH | Not feasible; offer ODT instead. |
| Documents | DOCX PAGES MD TXT EPUB; RTF/RTFD/clippings input; HTML export | No | M | M | Pandoc sidecar (separate process). PAGES input best-effort via LibreOffice's libetonyk. RTFD/clippings are macOS-only: skip. → S13 |
| Slides | PPT/PPTX → PDF JPG PNG; PDF → PPTX | No | M | M | LibreOffice headless, optional download. → S13 |
| Spreadsheets | XLSX ODS CSV TSV; PDF export; XLS input | No | L–M | L–M | `calamine` read, `rust_xlsxwriter` write, LibreOffice for PDF. → S13 |
| Subtitles | SRT VTT; TXT export | No | L | L | Pure Rust. → S13 |
| Archives | ZIP TAR GZIP; RAR input | No | L | L | `zip`, `tar`, `flate2`; RAR via libarchive (not unrar). Add 7z. → S14 |

## 3. File tools (Command ring)

| Tool | What it does | In README? | Windows | Linux | Notes → slice |
|---|---|---|---|---|---|
| Compress (any file) | Target size, % reduction or preset; resize while compressing | Images only | M | M | Images: mozjpeg/oxipng/webp. Video: two-pass target bitrate. PDF: image downsampling via lopdf. → S5, S9, S12 |
| Edit metadata (any file) | Inspect/change/remove; fields grouped by file/track/chapter; strip camera data | Images only | M | M | Images: EXIF/XMP/IPTC. Media: FFmpeg `-metadata`, chapters. Optional exiftool for exotic formats. → S5 |
| Edit photos | Exposure, color, detail, dehaze, clarity, grain, noise reduction, live preview | Yes | H | H | WebGL preview in the webview, same kernels in Rust for export. → S6 |
| Annotate photos | Arrows, shapes, text, numbered callouts, magnifiers, draw/highlight | Yes (partial) | M | M | Magnifier + freehand are the missing bits. → S6 |
| Add a background | Solid, gradient or image BG; blur, radius, shadow, aspect, spacing | Yes | L | L | Image-as-background and blur were missing. → S6 |
| Crop images | Exact pixels, preset/custom ratios | Yes | L | L | → S5 |
| Redact photos | Local face/text detection; movable layered areas; solid or blur | Partial (manual) | M | M | ONNX Runtime (`ort`) with a face detector + text detector, CPU. → S7 |
| Create a collage | Grid/row/column/featured; size, spacing, corners, BG | No | M | M | → S6 |
| Create a PDF from images | One page per image, mixed formats | No | L | L | → S4 |
| Trim video | Frame-step, precise timecodes | No | M | H | WebKitGTK playback depends on GStreamer plugins: use FFmpeg-generated proxies + frame strips on Linux. → S10 |
| Crop video | Exact dimensions, keep audio | No | M | M | → S10 |
| Change video speed | Pitch-preserving, works on silent clips | No | L | L | `setpts` + `atempo`. → S10 |
| Join videos | Arrange clips, mix MP4/MOV/MKV, first clip = canvas | No | M | M | Re-encode to first clip's canvas when streams differ. → S10 |
| Save video frames | Step frames, export full-res PNGs | No | M | M | → S10 |
| Split video | Custom points or equal parts, one folder | No | L | L | → S10 |
| Redact video | Time-ranged regions, solid or blur | No | H | H | Per-region `enable=between(t,a,b)` filters. → S10 |
| Normalize volume | Loudness range, true peak, before/after levels | No | L | L | Two-pass `loudnorm`. → S11 |
| Audio visualizer | MP4 from audio with waveform or still image; landscape/portrait/square | No | M | M | `showwaves`. → S11 |
| Trim audio | Waveform UI, trim leading/trailing silence | No | M | M | → S11 |
| Convert audio channels | Mono/stereo, per-channel control | No | L | L | `pan` filter. → S11 |
| Bleep audio | Ranges replaced by a bleep, A/B playback | No | M | M | → S11 |
| Merge PDFs | Selection order, reorder before merge | No | L | L | PDFium import pages / lopdf. → S12 |
| Organize PDF pages | Reorder, rotate, duplicate, delete visually | No | M | M | → S12 |

## 4. File actions (Control ring)

| Action | What it does | In README? | Windows | Linux | Notes → slice |
|---|---|---|---|---|---|
| AirDrop | macOS share sheet | No | M | M | Becomes **Share**: Windows share UI (Nearby Share); Linux: KDE Connect / LocalSend if installed. → S15 |
| Float | Floating shelf pinned to a screen edge, drag out to other apps | No | M | M / H | Needs drag-out from a Tauri window. Wayland edge pinning needs layer-shell (not GNOME). → S15 |
| Copy | Files to clipboard, paste in the file manager | No | L | M | Windows `CF_HDROP`. Linux `text/uri-list` + `x-special/gnome-copied-files`. → S15 |
| Move | Pick a folder, move originals | No | L | L | → S15 |
| Copy Path | Full paths, one per line | No | L | L | → S15 |
| Terminal | Open folder in Terminal/iTerm2 | No | L | L | Windows Terminal → PowerShell fallback; Linux `$TERMINAL` → `x-terminal-emulator` → known list. → S15 |

## 5. Product claims

| Claim | In README? | Notes |
|---|---|---|
| Offline: files never uploaded | Implied | Make it an explicit, testable guarantee (no network permission in core; CI test). → S0 |
| No separate app window | Yes | Background process + tray. |
| Apple silicon only, macOS 13+ | n/a | Target Windows 10 22H2+/11 (x64, ARM64 later); Linux x86_64 + aarch64. |

---

# Improvements over the original (ranked by value ÷ effort)

| Rank | Improvement | Why it beats Tangerine | Effort | Slice |
|---|---|---|---|---|
| 1 | **CLI parity** (`morph convert/tool/chain`) | Every ring action is scriptable; Tangerine is GUI-only | L (already planned) | S1 |
| 2 | **User recipes in TOML** (wrap any command: `magick`, `ffmpeg`, `pandoc`, `yt-dlp`…) appearing as ring segments | Turns the ring into a launcher for any local tool; zero Rust needed to extend | L–M | S19 |
| 3 | **Saved chains** (one segment = PDF → JPG → Crop 1:1 → BG preset) | Tangerine chains interactively only | L | S8 |
| 4 | **File-manager integration** (Explorer context menu, Dolphin service menu, Nautilus/Nemo/Thunar actions) | Works everywhere drag detection can't, especially Wayland | M | S16/S17 |
| 5 | **Output naming templates + conflict policy** (`{stem} {Op}.{ext}`, subfolder, overwrite never/ask) | Teams with conventions; batch jobs | L | S1 |
| 6 | **Undo last job** (moves outputs to Recycle Bin/Trash) | Safe experimentation | L | S8 |
| 7 | **Job history panel** with "Show in folder" and "Run again" | Re-running a chain on new files | L–M | S18 |
| 8 | **Clipboard as source** (convert/compress the image on the clipboard) | Screenshots never touch disk | L | S15 |
| 9 | **Background removal + OCR** (local ONNX; searchable PDF, "Copy text") | High-demand local AI features, same runtime as Redact | M | S7+ |
| 10 | **Hardware encoding** (NVENC / QSV / AMF / VA-API) | Much faster video on Windows/Linux GPUs | M | S9 |
| 11 | **Watch folders** (apply a recipe to anything dropped into a folder) | Hands-free batch pipelines | M | S19 |
| 12 | **Per-type ring customization** (reorder, pin, hide, favorites) | Smaller, personal rings | L–M | S18 |
| 13 | **Portable mode** (Windows zip, config next to exe) | IT-restricted machines | L | S18 |
| 14 | **Localization** (FR, AR, ES first) + screen-reader-friendly keyboard ring | Accessibility; your own working languages | M | S18 |
| 15 | **Themeable HUD** (accent colour presets + custom, motion level, high-contrast variant) | Personal look without forking; accessible by default | L | S2/S18 |
| 16 | **Plugin crates** (Rust `Operation` trait, signed WASM later) | Community-maintained engines | H | S19 |

Rejected on purpose: cloud uploads/"send to service" (breaks the offline promise; possible as a user recipe), metering/allowance, PAGES export.
