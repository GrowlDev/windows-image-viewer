# Windows Image Viewer

A fast, minimal image viewer for Windows, built for pixel art and ordinary photos.

- **Pixel-perfect:** enlarging uses nearest-neighbour filtering at whole-number zoom steps, and the image is aligned to *physical* screen pixels, so 100% means one image pixel per screen pixel even with Windows display scaling. Reducing uses mipmapped smoothing so photos don't shimmer.
- **Folder browsing:** use the arrow keys through the other images in the folder, in Explorer's name order (frame2 before frame10). Neighbouring images are decoded in the background so navigation is instant.
- **Edit in your editor:** opens the current file in the editor you choose in Settings. Paint.NET, Aseprite, Krita, GIMP, Photoshop, Affinity Photo, Pixelorama, Pyxel Edit and Paint are detected automatically, and any other program can be picked. When you save, the viewer picks up the change within a second and keeps your zoom.
- **Gallery:** a side panel with every image in the folder as a card. Click a card to view that image; card size is adjustable (slider or Ctrl+wheel). Small images are enlarged by whole pixels, so sprites stay crisp.
- **Animation:** GIF, APNG and animated WebP play automatically. You can pause, step frames and turn looping off.
- **Formats:** PNG, APNG, JPEG (EXIF orientation honoured), GIF, BMP, WebP, ICO, TIFF, TGA, QOI.
- **Always-visible file info:** name, dimensions, format, size, modified date, position in the folder, and the pixel and colour under the cursor.

No slideshow, printing, sharing, cloud or library features.

## Keys and mouse

| Action | Keys / mouse |
|---|---|
| Next / previous image | → / ←, Page Down / Page Up, mouse forward/back buttons |
| First / last image | Home / End |
| Zoom around the cursor | Mouse wheel (pinch on a touchpad) |
| Zoom in / out | + / − |
| Fit to window | 0, or the Fit button |
| 100% (actual pixels) | 1, or click the zoom percentage |
| Exact zoom 200-900% | 2-9 |
| Pan | Drag |
| Rotate right / left (view only, never saved) | Ctrl+. / Ctrl+, (or R / Shift+R) |
| Full screen | F11, Enter or double-click; Esc leaves |
| Play / pause animation | Space |
| Step animation frames | , and . |
| Loop on / off | L |
| Edit in your editor | E |
| Gallery on / off | G |
| Copy image | Ctrl+C |
| Move to Recycle Bin | Delete |
| Open a file | Ctrl+O, or drop one on the window |
| Toggle the info bar | I |
| Reload | F5 |
| Close | Esc |

The toolbar also has **Show in Explorer**, **Open with…** and **Settings** (image editor, info bar, looping, gallery).

## Build and install

Requires Rust 1.99 or newer.

```powershell
cargo build --release              # target\release\imageviewer.exe
cargo test
.\tools\install.ps1                # per-user install, registration, Start menu entry
.\tools\uninstall.ps1              # removes all of it
```

`install.ps1` copies the exe to `%LOCALAPPDATA%\Programs\Windows Image Viewer`, runs `imageviewer.exe --register` and opens **Settings → Apps → Default apps** at the viewer's page. Windows doesn't let apps make themselves the default, so you choose the file types there. Registration writes only to `HKEY_CURRENT_USER` (no admin needed), and `--unregister` removes it again. Windows Photos is never touched.

## Layout

| File | Responsibility |
|---|---|
| `src/main.rs` | Command line, window options |
| `src/app.rs` | The window: info bar, canvas, toolbar, input, animation |
| `src/view.rs` | Zoom/pan maths in physical pixels (unit-tested) |
| `src/loader.rs` | Background decode threads and the neighbour cache |
| `src/decode.rs` | File → frames, EXIF orientation, animation timing |
| `src/folder.rs` | Folder scan and Explorer sort order |
| `src/editor.rs` | Image editor detection, choice and launching |
| `src/gallery.rs`, `src/thumbs.rs` | Side gallery and its background thumbnail cache |
| `src/platform.rs` | Window cloaking and title bar colour, Explorer, Open with, Recycle Bin |
| `src/register.rs` | Default-apps registration under HKCU |
| `src/icons.rs`, `src/svg.rs` | Toolbar icons: [Lucide](https://lucide.dev) SVGs drawn by a small path renderer |
| `src/theme.rs` | Colours, fonts and widget style |
| `tools/make_icon.py` | Regenerates `assets/app.ico` and `assets/icon.png` from 16×16 pixel art |

Rendering is egui on OpenGL (glow). It's the egui backend that supports mipmaps, which keep reduced photos smooth.

## Test harness

```bash
IMAGEVIEWER_BACKGROUND=1 imageviewer.exe <image> --size 1000x560 --do actual,zoomin,rotate --capture shot.png
```

- `IMAGEVIEWER_BACKGROUND=1` opens the window off-screen and unfocused, with its settings kept in a temporary folder.
- `--do` runs comma-separated actions once each image has loaded: `next`, `prev`, `zoomin`, `zoomout`, `actual`, `fit`, `zoomN`, `rotate`, `pause`, `step`, `info`, `gallery`, `settings`, `fullscreen`, `wait`.
- `--capture` saves a screenshot and exits.
- `IMAGEVIEWER_TRACE=<file>` logs startup timings.
