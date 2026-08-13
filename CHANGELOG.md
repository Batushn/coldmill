# Changelog

## Unreleased

- **Speech to text.** An optional module turns video and audio into `.txt`,
  `.srt` or `.vtt`. Whisper only listens to 16 kHz mono, so a transcription is
  a pipeline: ffmpeg prepares the audio, Whisper reads it. The language is
  detected rather than assumed. ~150 MB, downloaded only if you ask for it.

- **Previews.** Every file gets a thumbnail — a frame for video, the picture
  itself for images, a waveform for audio. Hovering a video scrubs through it
  like a timeline: the filmstrip behind that is one tiled image, built on first
  hover and cached, so files nobody looks at cost nothing. Drawn by the ffmpeg
  already in the installer, so this adds no download.
- **List and grid views**, remembered between runs.
- **Trimming, splitting, muting and re-framing** for video and audio, laid over
  the filmstrip rather than in timecode boxes. A split runs as several trims of
  the same source into numbered files, so one row is still one job with one
  progress bar. Trims feed the size estimate.
- A bug report button in the footer.
- Fixed: the language menu closed the instant you touched its scrollbar. A
  capture-phase scroll listener saw the scrolling of the menu itself and
  treated it as a click elsewhere.

## 0.1.0 — first release

The first public build. Windows installer and Linux `.deb` / `.AppImage`.

### Converting

- Drop mixed files; they are grouped by type, each group gets one target format
- Three quality presets — Small, Balanced, High — mapped to real encoder
  settings in one file, with no codec knobs in the interface
- Parallel conversion capped at the CPU core count, one ffmpeg process per job
- Live per-file progress, per-job cancel, and cancel-all
- Estimated output size before a job starts, replaced by a live projection from
  ffmpeg's own byte counter once it is running
- Output goes next to the source file unless you pick a folder, which is then
  remembered

### Formats

- **Media** (bundled): mp4, mkv, webm, mov, avi, gif · mp3, wav, flac, aac, ogg,
  m4a, opus · jpg, png, webp, avif, tiff, bmp, gif
- **Documents** (optional, ~60 MB): docx, odt, md, html, epub, rtf, tex, rst,
  txt, and anything to PDF. PDF *input* and legacy `.doc` / `.xls` / `.ppt`
  additionally need LibreOffice, which is detected rather than installed.
- **3D** (optional, free): stl, obj, glb, gltf → stl, obj, glb, converted
  in-process. Adding Blender (~400 MB) unlocks fbx, dae, ply and `.blend`.

Input type comes from magic bytes, not the extension. Engine downloads are
version-pinned and SHA-256 verified before they are unpacked.

### Interface

- Single screen, dark, one accent colour
- First-run module picker, reachable again from the footer; turning a module
  off removes its engines
- 16 languages with OS detection, and right-to-left layout for Arabic

### Updates

Checks once at startup and shows a single line if a newer release exists.
Nothing downloads or restarts without a click, and a failed or offline check
stays silent. Releases are signed and the signature is verified before
anything is installed. The Windows installer and the Linux AppImage update in
place; the `.deb` stays with your package manager.

### Known limits

- macOS is not built yet
- Dropping a folder is not supported — files only
- Documents and 3D models have no size estimate; their output depends far too
  much on content
- Cancelling an in-process 3D conversion only takes effect when it finishes
