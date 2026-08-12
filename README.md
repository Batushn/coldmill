# Coldmill

**Offline batch file converter.** Drag files in, pick a format per group, hit convert. No upload, no account, no settings screen.

Built with [Tauri v2](https://v2.tauri.app), React and a bundled [ffmpeg](https://ffmpeg.org) sidecar. Everything runs locally — files never leave the machine.

<!-- screenshot goes here -->

## Why

Convertio and friends are simple but upload your files to a server. HandBrake runs locally but greets you with fifty encoder knobs. Coldmill is the middle: local processing, three quality buttons, nothing else.

## Features

- Drop mixed files — they are grouped automatically by type
- One target format per group, only formats that make sense for that type
- Three quality presets: **Small / Balanced / High**. No CRF, no bitrate, no codec pickers
- **Estimated output size** per file and for the batch, refined live while encoding
- Parallel conversion, capped at your CPU core count
- Live per-file progress, cancel any job mid-flight
- Output next to the source file by default

## Modules

The first launch asks what you actually convert. Media works out of the box; the other two fetch their engines only if you ask for them, and removing a module deletes them again.

| Module | Download | Converts |
| ------ | -------- | -------- |
| **Media** — always on | bundled | **Video** mp4, mkv, webm, mov, avi, gif · **Audio** mp3, wav, flac, aac, ogg, m4a, opus · **Image** jpg, png, webp, avif, tiff, bmp, gif |
| **Documents** | ~60 MB ([pandoc](https://pandoc.org) + [Typst](https://typst.app)) | docx, odt, md, html, epub, rtf, tex, rst, txt — and anything to PDF |
| **3D** | free | stl, obj, glb, gltf → stl, obj, glb |
| **3D + Blender** | ~400 MB ([Blender](https://blender.org)) | adds fbx, dae, ply and **.blend** |

Two things worth knowing:

- **PDF as an *input*** (and legacy `.doc` / `.xls` / `.ppt`) needs **LibreOffice**, which Coldmill looks for rather than installs — it is a system package and its download URL moves every release. The setup screen says whether it was found and links to the official download. Everything else in the document module works without it.
- The built-in 3D converter carries **geometry only** — no materials, no animation. Blender is the tier that keeps them.

Every engine download is pinned to a version and verified against a published SHA-256 before it is unpacked.

Input format is detected from **magic bytes**, not the file extension — a `.txt` that is really a PNG converts fine, and a renamed archive is rejected up front. Text formats that genuinely have no signature (obj, md, html) fall back to the extension, but only if the bytes really do look like text.

## Development

Requires [Node 18+](https://nodejs.org), [Rust](https://rustup.rs), and the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm install
./scripts/fetch-ffmpeg.sh   # downloads the ffmpeg/ffprobe sidecars (~80 MB)
npm run app:dev
```

Build a release bundle:

```bash
npm run app:build
```

Regenerate the app icons after changing the logo:

```bash
npm run icons
```

### Tests

```bash
cd src-tauri && cargo test
```

There is also a slower check that runs **every** quality preset through the real
ffmpeg sidecar — worth running whenever you touch `presets.rs`:

```bash
cd src-tauri && cargo test -- --ignored --nocapture
```

### Project layout

```
src/                  React UI (single screen + setup)
src-tauri/src/
  detect.rs           magic-byte type detection, with a text tier for obj/md/html
  job.rs              picks the backend for a job and reports every outcome alike
  presets.rs          quality preset -> ffmpeg arguments (the only place to tune encoding)
  estimate.rs         output size model, refined mid-run from real byte counts
  probe.rs            ffprobe metadata / duration
  ffmpeg.rs           sidecar spawn + progress parsing
  external.rs         runs pandoc / LibreOffice / Blender
  document.rs         which document engine handles which pair
  mesh.rs             built-in stl/obj/glb converter, and the Blender script
  engines.rs          pinned, checksum-verified engine downloads
  settings.rs         which modules are on, remembered between runs
  queue.rs            tokio Semaphore worker pool + cancellation registry
  commands.rs         Tauri commands exposed to the frontend
scripts/fetch-ffmpeg.sh
```

Want different encoding settings? Everything lives in [`src-tauri/src/presets.rs`](src-tauri/src/presets.rs). Adding a new engine is a row in [`engines.rs`](src-tauri/src/engines.rs) plus a backend that knows its arguments.

## Not in scope

Deliberately left out to keep the app a single uncluttered screen: codec
selection, GPU encoding, metadata editing, trimming and cropping, watch folders,
a settings screen, and localisation.

On the roadmap: macOS builds, dropping whole folders, and OCR for scanned PDFs.

## License

Coldmill is licensed under the [GNU GPL v3.0 or later](LICENSE).

It bundles GPL-licensed ffmpeg builds (which include x264/x265) as a separate sidecar process. ffmpeg is a separate project with its own license — see [ffmpeg.org/legal.html](https://ffmpeg.org/legal.html). Binaries are downloaded at build time from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) and are not distributed in this repository.
