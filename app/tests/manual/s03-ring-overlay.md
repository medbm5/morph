# S3 manual test checklist — HUD ring overlay and drag detection

Run on **Windows 11 (Explorer)** and an **X11 session** with **Nautilus** and **Dolphin**, once **with** a compositor and once **without** (e.g. Openbox or i3 with no picom).

Prepare a folder with `photo.jpg`, `scan.png`, `report.pdf` and a subfolder.

| # | Step | Expected | Win 11 | Nautilus | Dolphin | No compositor |
|---|---|---|---|---|---|---|
| 1 | Start dragging `photo.jpg`, tap `Shift` | Ring opens centred under the cursor; first frame ≤ 50 ms (`ring first frame after N ms` in the log) | | | | |
| 2 | Keep dragging over the segments | Hovered segment turns amber, centre label follows instantly | | | | |
| 3 | Drop on **PNG** | Ring collapses with a pulse; `✓ SAVED · photo.png`; `photo.png` exists beside the original within 1 s | | | | |
| 4 | Repeat 1–3 | `photo 2.png` is created; `photo.jpg` unchanged (hash) | | | | |
| 5 | Mid-drag tap `Ctrl` | Ring switches to **Tools** (old segments shrink/fade, new ones draw in) | | | | |
| 6 | Mid-drag tap `Alt` | **Actions** ring; the focused window's menu bar does not activate | | | | |
| 7 | With the ring open press `3` | Segment 03 locks; dropping anywhere on the ring applies it | | | | |
| 8 | Press arrows, then `Enter` | Hot segment moves; `Enter` applies to the selection | | | | |
| 9 | Press `Esc` | Ring fades out (120 ms, no pulse); drag can continue; tap again reopens | | | | |
| 10 | Release outside the ring | Ring closes within ~0.5 s; nothing is saved | | | | |
| 11 | Drag a **window title bar**, tap `Shift` | Ring never appears | | | | |
| 12 | Select text in an editor and drag it, tap `Shift` | Ring never appears (or closes immediately: no files) | | | | |
| 13 | Drag 3 PNGs, tap `Shift`, drop on **PDF** | One PDF with 3 pages; progress card then `✓ SAVED` with **Show in folder** | | | | |
| 14 | Drag 5 JPGs, drop on **WebP** | Progress card `2 OF 5 SAVED`; **Show in folder** opens the folder with the file selected | | | | |
| 15 | Turn on reduced motion (Windows: Settings → Accessibility → Visual effects → Animation effects off; GNOME: `gsettings set org.gnome.desktop.interface enable-animations false`) | Nothing rotates or flickers; ring fades in/out in 100 ms | | | | |
| 16 | Windows High Contrast on | Ring uses system colours, no glow | | | | |
| 17 | Ring near a screen edge | Ring stays fully on screen | | | | |
| 18 | Drop on a segment that opens an editor | Expanded window opens with the file as `TARGET ACQUIRED` | | | | |
| 19 | No compositor (X11) | Ring is a circular window with an opaque backdrop; drops still work | — | | | |

Record the show latency from the log for each environment:

| Environment | Latency (ms) |
|---|---|
| Windows 11 | |
| GNOME X11 | |
| KDE X11 | |
| Openbox (no compositor) | |
