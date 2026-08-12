# Coldmill

**Offline batch file converter for audio, video and images.** Drag files in, pick a format per group, hit convert. No upload, no account, no settings screen.

Built with [Tauri v2](https://v2.tauri.app), React and a bundled [ffmpeg](https://ffmpeg.org) sidecar. Everything runs locally — files never leave the machine.

<!-- screenshot goes here -->

## Why

Convertio and friends are simple but upload your files to a server. HandBrake runs locally but greets you with fifty encoder knobs. Coldmill is the middle: local processing, three quality buttons, nothing else.

## Features

- Drop mixed files — they are grouped automatically into **video / audio / image**
- One target format per group, only formats that make sense for that type
- Three quality presets: **Small / Balanced / High**. No CRF, no bitrate, no codec pickers
- Parallel conversion, capped at your CPU core count
- Live per-file progress, cancel any job mid-flight
- Output next to the source file by default

### Supported targets

| Type  | Formats |
| ----- | ------- |
| Video | mp4, mkv, webm, mov, avi, gif |
| Audio | mp3, wav, flac, aac, ogg, m4a, opus |
| Image | jpg, png, webp, avif, tiff, bmp, gif |

Input format is detected from **magic bytes**, not the file extension — a `.txt` that is really a PNG converts fine, and a renamed archive is rejected up front.

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

### Project layout

```
src/                  React UI (single screen)
src-tauri/src/
  detect.rs           magic-byte media-type detection
  presets.rs          quality preset -> ffmpeg arguments (the only place to tune encoding)
  probe.rs            ffprobe metadata / duration
  ffmpeg.rs           sidecar spawn + progress parsing
  queue.rs            tokio Semaphore worker pool + cancellation registry
  commands.rs         Tauri commands exposed to the frontend
scripts/fetch-ffmpeg.sh
```

Want different encoding settings? Everything lives in [`src-tauri/src/presets.rs`](src-tauri/src/presets.rs).

## License

Coldmill is licensed under the [GNU GPL v3.0 or later](LICENSE).

It bundles GPL-licensed ffmpeg builds (which include x264/x265) as a separate sidecar process. ffmpeg is a separate project with its own license — see [ffmpeg.org/legal.html](https://ffmpeg.org/legal.html). Binaries are downloaded at build time from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) and are not distributed in this repository.
