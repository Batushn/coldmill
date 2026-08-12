# Changelog

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

### Known limits

- macOS is not built yet
- Dropping a folder is not supported — files only
- Documents and 3D models have no size estimate; their output depends far too
  much on content
- Cancelling an in-process 3D conversion only takes effect when it finishes
