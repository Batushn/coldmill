# Changelog

## 0.3.0

- **The app is called Coldmill.** The old name was already carried by four
  other file converters, one of them an iOS app shipping the same pitch this
  has. A cold rolling mill reshapes metal without heat: raw material in,
  finished stock out, in batches, and nothing has to get hot to do it — which
  is the whole of this program, given the work happens on your own machine.
  The bundle identifier moved with the name, so this installs alongside the
  old application rather than upgrading it.

- **Splitting produces, and shows, every piece.** Cutting a clip in three
  wrote three files but only ever reported one, so the row claimed a single
  output at a single file's size. Both the row and the finished event now
  carry the whole list. Pressing "Split" without moving the playhead also did
  nothing at all — the cut landed on the trim edge and was silently dropped —
  so the button is now disabled unless the cut would survive, the piece count
  reflects the cuts that will, and a marker can be clicked to take one back.

- **Choose how a reframe fills the gap.** Changing a clip's shape only ever
  cropped it. Black bars and a blurred copy of the frame join it, and neither
  scales the picture: the canvas grows around it instead, so filling never
  costs any detail where cropping always costs the edges.

- **An advanced panel**, under the quality presets and behind a click. Video
  bitrate, CRF, encoder speed, frame rate, a height cap, audio bitrate, sample
  rate and channels. They override the preset rather than replacing it, so a
  single field can be set without describing a whole encode, and an untouched
  panel is byte-for-byte the encode it always was. The closed button carries a
  count of what is set, and the size estimates follow the overrides.

- **Colour adjustments on pictures**, and on video too: brightness, contrast,
  saturation and hue. An untouched file picks up no filter at all, so nothing
  pays a re-encode for a grading nobody asked for.

- **.ico output**, needing nothing installed. The picture is fitted inside a
  square and the rest padded with transparency, so a wide photo keeps all of
  itself, and a source already under 256 px is left at its own size.

- **A preview for 3D models.** Every other kind of file had a picture in its
  row; a model had a grey rectangle. It is rendered in-process — no GPU, no
  new dependency — from geometry the queue already parses to count triangles.

- **Quality reduces a mesh, and the origin can be moved.** Small keeps a
  quarter of the triangles, Balanced three fifths, and High passes the mesh
  through untouched. Where Blender is installed it does the reduction with a
  real decimate modifier. The origin can go to the middle of the model or the
  middle of its base, and each row says what it will come out with.

- **Fixed: speech, text-from-pictures and read-aloud could not be switched
  on.** All three reported "no build for this platform" on every platform,
  including ones where their engines were ready to install. The engine ids
  crossing into the setup screen were spelled without their hyphens, so the
  lookups found nothing and an absent engine counts as unavailable. Every id
  is now named outright and a test pins it against the list the screen asks
  for.

- Those three modules now sit under an **Extras** heading at the foot of the
  setup screen, one line each. They are not what this program is for.

- **An Arch package.** `packaging/aur` wraps the released AppImage and works
  with `makepkg -si` whether or not it is ever published to the AUR.

- **macOS build**, for Apple Silicon, unsigned for now. Media conversion, the
  built-in 3D converter and reading text from pictures all work; the modules
  whose engines have no verified macOS build are greyed out rather than
  offering a download that would fail. CI builds and runs the full test suite
  on Apple hardware, including every quality preset through the macOS ffmpeg.

## 0.2.0

- **More image formats.** An optional ImageMagick module adds vector files
  (svg, eps, ai), camera raw (cr2, nef, arw, dng and friends) and heic. It only
  takes the inputs ffmpeg genuinely cannot open — the bundled build already
  reads JPEG XL, PSD, DDS, EXR, DPX and TGA, and those keep their existing,
  faster path. ~12 MB.

- **Read aloud.** An optional module turns a text or markdown file into spoken
  audio. Piper takes its text on standard input rather than as an argument, so
  this is the one engine driven by a pipe; it writes WAV, and anything else is
  a second pass through ffmpeg. Markdown punctuation is stripped first, so the
  asterisks are not read out. ~85 MB, English voice.

- **Text from pictures.** An optional module reads the words out of
  screenshots, photos and scans into `.txt` or `.md`. Tesseract is used when
  the machine already has it; otherwise a Rust OCR engine compiled into the
  app does the reading, so the feature works on Linux too — Tesseract
  publishes no Linux binary. Only the models are downloaded, about 12 MB.

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
