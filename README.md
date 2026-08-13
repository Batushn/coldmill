<div align="center">

# Coldmill

**Offline batch file converter.** Drag files in, pick a format per group, hit convert.
No upload, no account, no settings screen.

[![License: GPL v3](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS-lightgrey.svg)](#development)
[![Languages](https://img.shields.io/badge/languages-16-brightgreen.svg)](#languages)
[![Vibe coded](https://img.shields.io/badge/vibe--coded-100%25-8957e5.svg)](#how-this-was-built)
[![Sponsor](https://img.shields.io/badge/sponsor-%E2%99%A5-db61a2.svg)](https://github.com/sponsors/batushn)

![Coldmill converting a mixed batch](docs/screenshots/demo.gif)

**[Watch the tour](docs/film.mp4)** · **[Download](https://github.com/Batushn/coldmill/releases/latest)** · **[coldmill on the web](https://batushn.github.io/coldmill/)**

</div>

Built with [Tauri v2](https://v2.tauri.app), React and a bundled [ffmpeg](https://ffmpeg.org) sidecar. Everything runs locally — files never leave the machine.

|  |  |
| --- | --- |
| ![Queue](docs/screenshots/queue.png) | ![Trimming a video](docs/screenshots/edit.png) |
| A mixed batch, grouped by type, one format each | Trim, split and re-frame on the filmstrip itself |

More of the interface — grid view, hover-scrubbing, colour adjustments, the advanced panel, 3D models and the right-to-left layouts — is on the [website](https://batushn.github.io/coldmill/#gallery).

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
- **16 languages**, picked up from your system on first run
- **Previews** for every file, and hover-scrubbing across a video the way a timeline works
- **List or grid** view, whichever suits the batch
- **Trim, split, mute and re-frame** video and audio, on the filmstrip itself

## Updates

Coldmill checks for a new release once at startup. If there is one, a single
line appears at the top — nothing downloads, and nothing restarts, until you
press the button. Dismissing it is one click and it stays quiet until the next
launch.

![Update banner](docs/screenshots/update.png)

Updates are signed: the app verifies the signature before it installs anything,
so a tampered file is rejected rather than run. Windows (`.exe`) and the Linux
`.AppImage` update in place; the `.deb` does not, because your package manager
owns that install.

## Languages

English · 中文 · Español · हिन्दी · العربية · Português · Русский · 日本語 · Deutsch · Français · 한국어 · Italiano · Türkçe · Tiếng Việt · Bahasa Indonesia · Polski

The language follows your OS on first launch and can be changed from the footer. Arabic switches the whole interface to right-to-left.

|  |  |
| --- | --- |
| ![Turkish](docs/screenshots/locale-turkish.png) | ![Arabic, right to left](docs/screenshots/locale-arabic.png) |

## Editing

Video and audio rows open a panel over the same filmstrip the row scrubs with.
Drag the ends to trim, click to place the playhead and split there, mute the
audio, or re-frame between horizontal, vertical and square — the source is
centre-cropped to the new shape at its original resolution.

A split is not a special case in the queue: it is several trims of the same
file, run one after another into numbered outputs (`clip-1.mp4`, `clip-2.mp4`),
and the row still shows one progress bar. Trimming also feeds the size
estimate, so cutting a ten-minute clip to thirty seconds updates the number
under the row.

## macOS

The `.dmg` is built for Apple Silicon and is **not signed by Apple** yet.

macOS will tell you the app is **"damaged and can't be opened"** and offer to
move it to the Trash. It is not damaged — that is simply the message Gatekeeper
shows for anything it cannot verify, and it looks identical to real corruption.
Drag Coldmill to Applications first, then:

```bash
xattr -dr com.apple.quarantine /Applications/Coldmill.app
```

The command has to run after the app is copied out of the disk image: the
mounted `.dmg` is read-only. System Settings → "Open Anyway" does not clear
this particular message, which is why the command is the documented route.

Converting media works as it does everywhere; so do the built-in 3D converter
and reading text from pictures, because those are compiled into the app. The
document, transcription, read-aloud and extra-image modules are greyed out on
macOS for now — their engines either publish no macOS build (whisper.cpp) or
publish one whose archive layout has not been checked against a real Mac, and
an unverified path is how an install fails at the last step instead of the
first.

CI builds and tests on Apple hardware every push, including running all sixty
quality presets through the macOS ffmpeg — so what is claimed here is measured
there rather than assumed.

## Modules

The first launch asks what you actually convert. Media works out of the box; the other two fetch their engines only if you ask for them, and removing a module deletes them again.

| Module | Download | Converts |
| ------ | -------- | -------- |
| **Media** — always on | bundled | **Video** mp4, mkv, webm, mov, avi, gif · **Audio** mp3, wav, flac, aac, ogg, m4a, opus · **Image** jpg, png, webp, avif, tiff, bmp, gif |
| **Documents** | ~60 MB ([pandoc](https://pandoc.org) + [Typst](https://typst.app)) | docx, odt, md, html, epub, rtf, tex, rst, txt — and anything to PDF |
| **More image formats** | ~12 MB ([ImageMagick](https://imagemagick.org)) | svg, eps, ai, camera raw and heic → the usual image formats |
| **Speech to text** | ~150 MB ([whisper.cpp](https://github.com/ggml-org/whisper.cpp) + base model) | video and audio → txt, srt, vtt, in any language |
| **Text from pictures** | ~12 MB ([ocrs](https://github.com/robertknight/ocrs) models) | screenshots, photos and scans → txt, md |
| **Read aloud** | ~85 MB ([Piper](https://github.com/rhasspy/piper) + an English voice) | txt, md → mp3, wav, m4a, opus, ogg, flac |
| **3D** | free | stl, obj, glb, gltf → stl, obj, glb |
| **3D + Blender** | ~400 MB ([Blender](https://blender.org)) | adds fbx, dae, ply and **.blend** |

Two things worth knowing:

- **PDF as an *input*** (and legacy `.doc` / `.xls` / `.ppt`) needs **LibreOffice**, which Coldmill looks for rather than installs — it is a system package and its download URL moves every release. The setup screen says whether it was found and links to the official download. Everything else in the document module works without it.
- **ImageMagick only takes what ffmpeg cannot.** The bundled ffmpeg already
  reads JPEG XL, PSD, DDS, EXR, DPX and TGA, so it keeps those; the extra
  module covers vectors, which ffmpeg has no rasteriser for, and camera raw,
  which it does not decode at all.
- **Reading aloud takes plain text only.** Voicing a `.docx` is deliberately
  two steps — convert it to `.txt` first, which this app already does. Chaining
  them silently would hide which half went wrong when a document reads badly.
  The voice is English; others exist and are each a separate download.
- **OCR prefers Tesseract** when the machine already has it, since it reads
  awkward scans better. It cannot be the only option, though: Tesseract ships
  a Windows installer and nothing at all for Linux, so the built-in engine —
  a Rust library, twelve megabytes of models, no platform binary — is what
  makes the feature work everywhere.
- **Transcription is two passes**: ffmpeg reduces the audio to the 16 kHz mono
  Whisper insists on, then Whisper reads it. Language is detected rather than
  assumed, so it is not an English-only feature.
- The built-in 3D converter carries **geometry only** — no materials, no animation. Blender is the tier that keeps them.

Every engine download is pinned to a version and verified against a published SHA-256 before it is unpacked.

Input format is detected from **magic bytes**, not the file extension — a `.txt` that is really a PNG converts fine, and a renamed archive is rejected up front. Text formats that genuinely have no signature (obj, md, html) fall back to the extension, but only if the bytes really do look like text.

## Arch Linux

The `.deb` is no use here; the AppImage is:

```bash
chmod +x Coldmill_0.3.0_amd64.AppImage
./Coldmill_0.2.0_amd64.AppImage
```

Arch stopped installing FUSE 2 by default, so a `libfuse.so.2` error means
`sudo pacman -S fuse2` — or run it once with `--appimage-extract-and-run`,
which needs no FUSE at all.

There is a PKGBUILD in [packaging/aur](packaging/aur), which works with
`makepkg -si` whether or not it is ever published to the AUR.

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

### Screenshots

The images above are captured from the real UI: `preview/` boots the actual
components in a plain browser with the Tauri bridge swapped for a scripted
mock, and headless Chrome drives it.

```bash
npm run preview   # http://localhost:1421/?scene=queue&lang=tr
npm run capture   # rewrites docs/screenshots/
npm run film      # rebuilds docs/film.mp4 from those shots
```

### Releasing

Bump the version in `package.json`, `src-tauri/tauri.conf.json` and
`src-tauri/Cargo.toml`, then tag and push; the workflow builds both platforms
and drafts a release:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

The build signs the installers with `TAURI_SIGNING_PRIVATE_KEY`, a repository
secret holding a minisign key. Its public half lives in `tauri.conf.json` and
is what installed copies check against, so **losing the private key means
existing installs can never be updated again** — back it up somewhere real.

### Translations

New strings go into `src/i18n/locales/en.json` first. `npm run check:locales`
fails if any of the other fifteen files drifts, and it runs in CI.

Plurals use `Intl.PluralRules`, so a locale only supplies the forms its
language actually has — `_one` / `_other` for English, `_few` and `_many` for
Russian and Polish, `_two` for Arabic.

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

## How this was built

**Coldmill is vibe coded.** The whole thing — the Rust backend, the React UI,
the ffmpeg preset tables, the engine downloader, all sixteen translations, the
CI workflows, the screenshot harness, and this README — was written by
[Claude](https://claude.com/claude-code) from conversational prompts, with a
human steering the product decisions rather than the keystrokes.

That is not a disclaimer of quality, but it is a fact you should weigh:

- What is **verified**: the Rust test suite, `clippy -D warnings`, every quality
  preset actually run through the bundled ffmpeg, the mesh round-trips checked
  against a real glTF parser, the pinned engine downloads checked against their
  published checksums, and a release bundle that builds on Windows.
- What is **not**: long-run behaviour on large real-world batches, the Blender
  and LibreOffice paths under every version, and the translations, which are
  machine-written and unreviewed by native speakers.

Corrections are welcome — especially to the translations, which are the part
most likely to read slightly off. Open an issue or a PR.

## Support

If Coldmill saved you a trip to a sketchy upload-your-file website, you can
[sponsor the project ♥](https://github.com/sponsors/batushn). It is entirely
optional; the app has no paid tier and never will.

## License

Coldmill is licensed under the [GNU GPL v3.0 or later](LICENSE).

It bundles GPL-licensed ffmpeg builds (which include x264/x265) as a separate sidecar process. ffmpeg is a separate project with its own license — see [ffmpeg.org/legal.html](https://ffmpeg.org/legal.html). Binaries are downloaded at build time from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) and are not distributed in this repository.
