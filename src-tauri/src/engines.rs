//! External engines for the optional modules.
//!
//! ffmpeg ships inside the installer; pandoc, typst and Blender do not — they
//! would quadruple the download for people who only convert video. So they are
//! fetched on demand into the app data directory, checksum-verified, and can be
//! removed again from the setup screen.
//!
//! Every download is pinned to a version and a SHA-256. Blender's hash comes
//! from the checksum manifest it publishes next to the archive; the GitHub
//! projects get their hash inlined, taken from the release asset digest.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

pub const EVENT_ENGINE_PROGRESS: &str = "engine:progress";
pub const EVENT_ENGINE_DONE: &str = "engine:done";
pub const EVENT_ENGINE_ERROR: &str = "engine:error";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineId {
    /// Document conversion workhorse.
    Pandoc,
    /// Pandoc's PDF engine. A LaTeX install would be an order of magnitude
    /// bigger for the same job.
    Typst,
    /// Optional 3D backend: the only one that opens .blend and writes FBX.
    Blender,
    /// Speech to text.
    Whisper,
    /// The weights Whisper listens with. Kept separate from the binary so a
    /// future model change does not re-download the engine, and so the
    /// progress bar can say which of the two it is fetching.
    WhisperModel,
    /// Finds where the words are in a picture.
    OcrDetection,
    /// Reads the words it found.
    OcrRecognition,
    /// Text to speech.
    Piper,
    /// The voice Piper speaks with.
    PiperVoice,
}

impl EngineId {
    pub const ALL: &'static [EngineId] = &[
        EngineId::Pandoc,
        EngineId::Typst,
        EngineId::Blender,
        EngineId::Whisper,
        EngineId::WhisperModel,
        EngineId::OcrDetection,
        EngineId::OcrRecognition,
        EngineId::Piper,
        EngineId::PiperVoice,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            EngineId::Pandoc => "pandoc",
            EngineId::Typst => "typst",
            EngineId::Blender => "blender",
            EngineId::Whisper => "whisper",
            EngineId::WhisperModel => "whisper-model",
            EngineId::OcrDetection => "ocr-detection",
            EngineId::OcrRecognition => "ocr-recognition",
            EngineId::Piper => "piper",
            EngineId::PiperVoice => "piper-voice",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EngineId::Pandoc => "Pandoc",
            EngineId::Typst => "Typst",
            EngineId::Blender => "Blender",
            EngineId::Whisper => "Whisper",
            EngineId::WhisperModel => "Whisper model",
            EngineId::OcrDetection => "OCR detection model",
            EngineId::OcrRecognition => "OCR recognition model",
            EngineId::Piper => "Piper",
            EngineId::PiperVoice => "Voice",
        }
    }

    pub fn version(self) -> &'static str {
        match self {
            EngineId::Pandoc => "3.10.2",
            EngineId::Typst => "0.15.1",
            EngineId::Blender => "4.5.9",
            EngineId::Whisper => "1.9.2",
            EngineId::WhisperModel => "base",
            EngineId::OcrDetection | EngineId::OcrRecognition => "2024-05",
            EngineId::Piper => "2023.11.14-2",
            EngineId::PiperVoice => "en_US-lessac-medium",
        }
    }
}

enum Checksum {
    /// SHA-256 of the archive itself.
    Inline(&'static str),
    /// A published `<file>  <sha256>` manifest listing many archives.
    Manifest { url: String, file_name: String },
}

/// Each platform's registry only reaches for one of these — Windows ships
/// zips, Linux tarballs — so on either target the other variant is genuinely
/// unconstructed. That is not a bug worth a warning.
#[allow(dead_code)]
enum Archive {
    Zip,
    /// Unpacked with the system `tar`, which already knows gzip and xz.
    Tar,
    /// Not an archive: a single file that only has to be put in place. Model
    /// weights arrive this way.
    Raw,
}

struct Asset {
    url: String,
    checksum: Checksum,
    archive: Archive,
    /// Executable path relative to the engine's install directory.
    exe_rel: PathBuf,
    approx_bytes: u64,
    /// A small second file the first one is useless without. Piper reads its
    /// voice configuration from a JSON file it expects to find beside the
    /// weights, and says nothing at all when it is missing.
    companion: Option<Companion>,
}

struct Companion {
    url: &'static str,
    file_name: &'static str,
    sha256: &'static str,
}

#[cfg(target_os = "windows")]
fn asset(id: EngineId) -> Asset {
    match id {
        EngineId::Pandoc => Asset {
            url: "https://github.com/jgm/pandoc/releases/download/3.10.2/pandoc-3.10.2-windows-x86_64.zip".into(),
            checksum: Checksum::Inline(
                "52487faaa63f8cef5363d5a771097da001228d61c6f44f32ed41b27a98c0278c",
            ),
            archive: Archive::Zip,
            exe_rel: PathBuf::from("pandoc-3.10.2/pandoc.exe"),
            approx_bytes: 41_600_000,
            companion: None,
        },
        EngineId::Typst => Asset {
            url: "https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-pc-windows-msvc.zip".into(),
            checksum: Checksum::Inline(
                "19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736",
            ),
            archive: Archive::Zip,
            exe_rel: PathBuf::from("typst-x86_64-pc-windows-msvc/typst.exe"),
            approx_bytes: 22_400_000,
            companion: None,
        },
        EngineId::Blender => Asset {
            url: "https://download.blender.org/release/Blender4.5/blender-4.5.9-windows-x64.zip"
                .into(),
            checksum: Checksum::Manifest {
                url: "https://download.blender.org/release/Blender4.5/blender-4.5.9.sha256".into(),
                file_name: "blender-4.5.9-windows-x64.zip".into(),
            },
            archive: Archive::Zip,
            exe_rel: PathBuf::from("blender-4.5.9-windows-x64/blender.exe"),
            approx_bytes: 399_051_129,
            companion: None,
        },
        EngineId::Whisper => Asset {
            url: "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-x64.zip".into(),
            checksum: Checksum::Inline(
                "49dcc16de826f20bd53d44f947a1ae49dfa81f86cad67a64d80820cb192d674a",
            ),
            archive: Archive::Zip,
            exe_rel: PathBuf::from("Release/whisper-cli.exe"),
            approx_bytes: 8_200_000,
            companion: None,
        },
        EngineId::WhisperModel => Asset {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin".into(),
            checksum: Checksum::Inline(
                "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("ggml-base.bin"),
            approx_bytes: 147_951_465,
            companion: None,
        },
        EngineId::OcrDetection => Asset {
            url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten".into(),
            checksum: Checksum::Inline(
                "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("text-detection.rten"),
            approx_bytes: 2_510_284,
            companion: None,
        },
        EngineId::OcrRecognition => Asset {
            url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten".into(),
            checksum: Checksum::Inline(
                "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("text-recognition.rten"),
            approx_bytes: 9_716_568,
            companion: None,
        },
        EngineId::Piper => Asset {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip".into(),
            checksum: Checksum::Inline(
                "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea",
            ),
            archive: Archive::Zip,
            exe_rel: PathBuf::from("piper/piper.exe"),
            approx_bytes: 22_400_000,
            companion: None,
        },
        EngineId::PiperVoice => Asset {
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx".into(),
            checksum: Checksum::Inline(
                "5efe09e69902187827af646e1a6e9d269dee769f9877d17b16b1b46eeaaf019f",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("en_US-lessac-medium.onnx"),
            approx_bytes: 63_201_294,
            companion: Some(Companion {
                url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json",
                file_name: "en_US-lessac-medium.onnx.json",
                sha256: "efe19c417bed055f2d69908248c6ba650fa135bc868b0e6abb3da181dab690a0",
            }),
        },
    }
}

#[cfg(not(target_os = "windows"))]
fn asset(id: EngineId) -> Asset {
    match id {
        EngineId::Pandoc => Asset {
            url: "https://github.com/jgm/pandoc/releases/download/3.10.2/pandoc-3.10.2-linux-amd64.tar.gz".into(),
            checksum: Checksum::Inline(
                "c7edd535941c48be6a362081a748272837de81ae11777202d9c341d3d8261c9a",
            ),
            archive: Archive::Tar,
            exe_rel: PathBuf::from("pandoc-3.10.2/bin/pandoc"),
            approx_bytes: 34_900_000,
            companion: None,
        },
        EngineId::Typst => Asset {
            url: "https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-unknown-linux-musl.tar.xz".into(),
            checksum: Checksum::Inline(
                "a6d077d0a95eed5a2eba715b2dae06be954f624ccbf85758a03f389ded33118c",
            ),
            archive: Archive::Tar,
            exe_rel: PathBuf::from("typst-x86_64-unknown-linux-musl/typst"),
            approx_bytes: 17_500_000,
            companion: None,
        },
        EngineId::Blender => Asset {
            url: "https://download.blender.org/release/Blender4.5/blender-4.5.9-linux-x64.tar.xz"
                .into(),
            checksum: Checksum::Manifest {
                url: "https://download.blender.org/release/Blender4.5/blender-4.5.9.sha256".into(),
                file_name: "blender-4.5.9-linux-x64.tar.xz".into(),
            },
            archive: Archive::Tar,
            exe_rel: PathBuf::from("blender-4.5.9-linux-x64/blender"),
            approx_bytes: 377_929_956,
            companion: None,
        },
        EngineId::Whisper => Asset {
            url: "https://github.com/ggml-org/whisper.cpp/releases/download/v1.9.2/whisper-bin-ubuntu-x64.tar.gz".into(),
            checksum: Checksum::Inline(
                "46811a3ecf584307480a220b9ef5ff81b7b22dc41577cbc274ce3afc61f753b1",
            ),
            archive: Archive::Tar,
            exe_rel: PathBuf::from("whisper-bin-ubuntu-x64/whisper-cli"),
            approx_bytes: 9_500_000,
            companion: None,
        },
        EngineId::WhisperModel => Asset {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin".into(),
            checksum: Checksum::Inline(
                "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("ggml-base.bin"),
            approx_bytes: 147_951_465,
            companion: None,
        },
        EngineId::OcrDetection => Asset {
            url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten".into(),
            checksum: Checksum::Inline(
                "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("text-detection.rten"),
            approx_bytes: 2_510_284,
            companion: None,
        },
        EngineId::OcrRecognition => Asset {
            url: "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten".into(),
            checksum: Checksum::Inline(
                "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("text-recognition.rten"),
            approx_bytes: 9_716_568,
            companion: None,
        },
        EngineId::Piper => Asset {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz".into(),
            checksum: Checksum::Inline(
                "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992",
            ),
            archive: Archive::Tar,
            exe_rel: PathBuf::from("piper/piper"),
            approx_bytes: 26_400_000,
            companion: None,
        },
        EngineId::PiperVoice => Asset {
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx".into(),
            checksum: Checksum::Inline(
                "5efe09e69902187827af646e1a6e9d269dee769f9877d17b16b1b46eeaaf019f",
            ),
            archive: Archive::Raw,
            exe_rel: PathBuf::from("en_US-lessac-medium.onnx"),
            approx_bytes: 63_201_294,
            companion: Some(Companion {
                url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json",
                file_name: "en_US-lessac-medium.onnx.json",
                sha256: "efe19c417bed055f2d69908248c6ba650fa135bc868b0e6abb3da181dab690a0",
            }),
        },
    }
}

/// Reported to the setup screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub id: EngineId,
    pub label: &'static str,
    pub version: &'static str,
    pub installed: bool,
    pub download_bytes: u64,
}

pub fn status(app: &AppHandle, id: EngineId) -> EngineStatus {
    EngineStatus {
        id,
        label: id.label(),
        version: id.version(),
        installed: executable(app, id).is_some(),
        download_bytes: asset(id).approx_bytes,
    }
}

fn install_dir(app: &AppHandle, id: EngineId) -> Option<PathBuf> {
    Some(
        app.path()
            .app_data_dir()
            .ok()?
            .join("engines")
            .join(format!("{}-{}", id.slug(), id.version())),
    )
}

/// Absolute path of an installed engine's executable, or `None`.
pub fn executable(app: &AppHandle, id: EngineId) -> Option<PathBuf> {
    let exe = install_dir(app, id)?.join(asset(id).exe_rel);
    exe.is_file().then_some(exe)
}

pub fn remove(app: &AppHandle, id: EngineId) -> Result<(), String> {
    let dir = install_dir(app, id).ok_or("no app data directory")?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("could not remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EngineProgress {
    engine_id: EngineId,
    label: &'static str,
    /// Bytes written so far, and the total when the server declares one.
    received: u64,
    total: Option<u64>,
    /// `download` or `extract` — the UI switches its wording on this.
    phase: &'static str,
}

/// Downloads, verifies and unpacks one engine. Safe to call when it is already
/// installed: it returns immediately.
pub async fn install(app: &AppHandle, id: EngineId) -> Result<(), String> {
    if executable(app, id).is_some() {
        return Ok(());
    }
    let asset = asset(id);
    let dir = install_dir(app, id).ok_or("no app data directory")?;

    // A previous attempt may have left a partial tree behind.
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let expected = match &asset.checksum {
        Checksum::Inline(hash) => (*hash).to_string(),
        Checksum::Manifest { url, file_name } => fetch_manifest_hash(url, file_name).await?,
    };

    let archive_path = dir.join("download.tmp");
    let actual = download(app, id, &asset, &archive_path).await?;
    if !actual.eq_ignore_ascii_case(&expected) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!(
            "{} checksum mismatch — expected {expected}, got {actual}",
            id.label()
        ));
    }

    emit_progress(
        app,
        id,
        asset.approx_bytes,
        Some(asset.approx_bytes),
        "extract",
    );
    let target = dir.clone();
    let path = archive_path.clone();
    let result = match asset.archive {
        Archive::Zip => tokio::task::spawn_blocking(move || unzip(&path, &target))
            .await
            .map_err(|e| e.to_string())?,
        Archive::Tar => untar(&archive_path, &dir),
        // Already the file we wanted; it only needs its real name.
        Archive::Raw => std::fs::rename(&archive_path, dir.join(&asset.exe_rel))
            .map_err(|e| format!("could not place the download: {e}")),
    };
    if !matches!(asset.archive, Archive::Raw) {
        let _ = std::fs::remove_file(&archive_path);
    }
    result?;

    let exe = dir.join(&asset.exe_rel);
    if !exe.is_file() {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!(
            "{} archive did not contain {}",
            id.label(),
            asset.exe_rel.display()
        ));
    }
    make_executable(&exe)?;

    if let Some(companion) = &asset.companion {
        let beside = dir.join(companion.file_name);
        let bytes = reqwest::get(companion.url)
            .await
            .map_err(|e| format!("could not fetch {}: {e}", companion.file_name))?
            .error_for_status()
            .map_err(|e| format!("could not fetch {}: {e}", companion.file_name))?
            .bytes()
            .await
            .map_err(|e| format!("could not read {}: {e}", companion.file_name))?;

        let actual = format!("{:x}", Sha256::digest(&bytes));
        if !actual.eq_ignore_ascii_case(companion.sha256) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(format!("{} checksum mismatch", companion.file_name));
        }
        std::fs::write(&beside, &bytes)
            .map_err(|e| format!("could not save {}: {e}", companion.file_name))?;
    }

    Ok(())
}

async fn fetch_manifest_hash(url: &str, file_name: &str) -> Result<String, String> {
    let body = reqwest::get(url)
        .await
        .map_err(|e| format!("could not fetch checksums: {e}"))?
        .error_for_status()
        .map_err(|e| format!("could not fetch checksums: {e}"))?
        .text()
        .await
        .map_err(|e| format!("could not read checksums: {e}"))?;

    body.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            // Manifests write "hash  name" or "hash *name".
            let name = name.trim().trim_start_matches('*');
            (name == file_name).then(|| hash.trim().to_string())
        })
        .next()
        .ok_or_else(|| format!("{file_name} is not listed in the checksum manifest"))
}

/// Streams the archive to disk, hashing as it goes, and returns the hash.
async fn download(
    app: &AppHandle,
    id: EngineId,
    asset: &Asset,
    target: &Path,
) -> Result<String, String> {
    let response = reqwest::get(&asset.url)
        .await
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?;

    let total = response.content_length().or(Some(asset.approx_bytes));
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|e| format!("could not write to {}: {e}", target.display()))?;

    let mut hasher = Sha256::new();
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("could not write to disk: {e}"))?;
        received += chunk.len() as u64;

        // One event per megabyte: enough for a smooth bar, quiet enough not to
        // flood the webview on a 400 MB download.
        if received - last_emit >= 1_048_576 {
            last_emit = received;
            emit_progress(app, id, received, total, "download");
        }
    }
    file.flush().await.map_err(|e| e.to_string())?;

    Ok(format!("{:x}", hasher.finalize()))
}

fn emit_progress(
    app: &AppHandle,
    id: EngineId,
    received: u64,
    total: Option<u64>,
    phase: &'static str,
) {
    let _ = app.emit(
        EVENT_ENGINE_PROGRESS,
        EngineProgress {
            engine_id: id,
            label: id.label(),
            received,
            total,
            phase,
        },
    );
}

fn unzip(archive: &Path, target: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("corrupt archive: {e}"))?;

    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        // `enclosed_name` rejects paths that would escape the target directory.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let out = target.join(relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut sink = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut sink).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

/// Tar archives only appear in the Linux registry, where `tar` is always there
/// and already knows how to handle gzip and xz.
fn untar(archive: &Path, target: &Path) -> Result<(), String> {
    let output = std::process::Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(target)
        .output()
        .map_err(|e| format!("could not run tar: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "tar failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// LibreOffice is a system install, not something we download: it is huge, it
/// wants to register file associations, and its download URL moves with every
/// release. We look for it instead, and the UI offers to open the official
/// download page when it is missing.
pub fn find_libreoffice() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\LibreOffice\program\soffice.exe",
            r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        ]
    } else {
        &[
            "/usr/bin/soffice",
            "/usr/lib/libreoffice/program/soffice",
            "/usr/local/bin/soffice",
            "/snap/bin/libreoffice",
            "/var/lib/flatpak/exports/bin/org.libreoffice.LibreOffice",
        ]
    };

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.is_file() {
            return Some(path);
        }
    }

    // Fall back to PATH for portable and package-manager installs.
    let exe = if cfg!(windows) {
        "soffice.exe"
    } else {
        "soffice"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(exe))
            .find(|path| path.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every download has to be nailed down one way or the other: either the
    /// URL names a version, or the hash is written here — so a file swapped
    /// out from under a versionless URL fails the install rather than being
    /// run. The OCR models are served from an unversioned address, which is
    /// exactly the case the second half of this covers.
    #[test]
    fn nothing_is_fetched_without_being_pinned() {
        for id in EngineId::ALL {
            let asset = asset(*id);
            assert!(
                asset.url.starts_with("https://"),
                "{id:?} url must be https"
            );
            assert!(
                asset.url.contains(id.version()) || matches!(asset.checksum, Checksum::Inline(_)),
                "{id:?} has neither a versioned URL nor an inline hash"
            );
            assert!(asset.approx_bytes > 1_000_000);
        }
    }

    #[test]
    fn manifest_lines_are_parsed() {
        let manifest =
            "abc123  blender-4.5.9-linux-x64.tar.xz\ndef456  blender-4.5.9-windows-x64.zip\n";
        let found = manifest
            .lines()
            .filter_map(|line| {
                let (hash, name) = line.split_once(char::is_whitespace)?;
                (name.trim().trim_start_matches('*') == "blender-4.5.9-windows-x64.zip")
                    .then(|| hash.to_string())
            })
            .next();
        assert_eq!(found.as_deref(), Some("def456"));
    }
}
