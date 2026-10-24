//! ffprobe wrapper. Used for two things: knowing the total duration up front
//! (without it there is no percentage to report) and confirming what streams a
//! container really holds.

use std::path::Path;

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Default, Clone)]
pub struct MediaInfo {
    pub duration_secs: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub has_video: bool,
    pub has_audio: bool,
    /// The picture carries transparency.
    pub has_alpha: bool,
}

pub async fn inspect(app: &AppHandle, path: &str) -> Result<MediaInfo, String> {
    // A HEIC photo is a grid of 512 px tiles, each its own stream; only the
    // group knows the size of the picture they make. The flag is only asked
    // for where grids exist, and dropped again if this ffprobe predates it —
    // the macOS build is 6.1, which refuses the whole run over an unknown
    // option.
    let gridded = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| matches!(e.as_str(), "heic" | "heif" | "avif"));
    let mut output = run(app, path, gridded).await?;
    if gridded && !output.status.success() {
        output = run(app, path, false).await?;
    }

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("could not parse ffprobe output: {e}"))?;

    let mut info = MediaInfo {
        duration_secs: json
            .get("format")
            .and_then(|f| f.get("duration"))
            .and_then(parse_secs),
        ..Default::default()
    };

    if let Some(streams) = json.get("streams").and_then(Value::as_array) {
        for stream in streams {
            match stream.get("codec_type").and_then(Value::as_str) {
                Some("video") => {
                    // Cover art is carried as a video stream; it is not one.
                    let is_cover = stream
                        .get("disposition")
                        .and_then(|d| d.get("attached_pic"))
                        .and_then(Value::as_u64)
                        == Some(1);
                    if !is_cover {
                        info.has_video = true;
                        info.width = stream
                            .get("width")
                            .and_then(Value::as_u64)
                            .map(|v| v as u32);
                        info.height = stream
                            .get("height")
                            .and_then(Value::as_u64)
                            .map(|v| v as u32);
                        info.fps = stream
                            .get("avg_frame_rate")
                            .or_else(|| stream.get("r_frame_rate"))
                            .and_then(Value::as_str)
                            .and_then(parse_frame_rate);
                        info.has_alpha = stream
                            .get("pix_fmt")
                            .and_then(Value::as_str)
                            .is_some_and(has_alpha);
                    }
                }
                Some("audio") => info.has_audio = true,
                _ => {}
            }

            // Some containers (MKV, OGG) only carry the duration per stream.
            if info.duration_secs.is_none() {
                info.duration_secs = stream.get("duration").and_then(parse_secs);
            }
        }
    }

    if let Some((width, height)) = tile_grid_size(&json) {
        info.width = Some(width);
        info.height = Some(height);
    }

    Ok(info)
}

async fn run(
    app: &AppHandle,
    path: &str,
    stream_groups: bool,
) -> Result<tauri_plugin_shell::process::Output, String> {
    let mut args = vec![
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ];
    if stream_groups {
        args.push("-show_stream_groups");
    }
    args.push(path);
    app.shell()
        .sidecar("ffprobe")
        .map_err(|e| format!("ffprobe sidecar missing: {e}"))?
        .args(args)
        .output()
        .await
        .map_err(|e| format!("ffprobe failed to start: {e}"))
}

fn tile_grid_size(json: &Value) -> Option<(u32, u32)> {
    let group = json
        .get("stream_groups")?
        .as_array()?
        .iter()
        .find(|group| group.get("type").and_then(Value::as_str) == Some("Tile Grid"))?;
    let component = group.get("components")?.as_array()?.first()?;
    let dimension = |key: &str| {
        component
            .get(key)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .map(|value| value as u32)
    };
    Some((dimension("width")?, dimension("height")?))
}

/// Pixel formats with an alpha plane. `pal8` is included because a palette
/// can mark an entry transparent, and GIFs and small PNGs routinely do.
fn has_alpha(pix_fmt: &str) -> bool {
    pix_fmt == "pal8"
        || pix_fmt.starts_with("yuva")
        || pix_fmt.starts_with("gbrap")
        || pix_fmt.starts_with("ya")
        || pix_fmt.starts_with("argb")
        || pix_fmt.starts_with("abgr")
        || pix_fmt.contains("rgba")
        || pix_fmt.contains("bgra")
}

/// ffprobe reports frame rates as a rational string: `30000/1001`, `25/1`, or
/// `0/0` when it has no idea.
fn parse_frame_rate(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    (denominator > 0.0 && numerator > 0.0).then(|| numerator / denominator)
}

fn parse_secs(value: &Value) -> Option<f64> {
    let secs = match value {
        Value::String(s) => s.parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    // Stills report a nominal frame duration; treat that as "no duration".
    (secs.is_finite() && secs > 0.1).then_some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rational_frame_rates_are_parsed() {
        assert_eq!(parse_frame_rate("25/1"), Some(25.0));
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.97).abs() < 0.01);
        // ffprobe's "no idea" answer for stills and some containers.
        assert_eq!(parse_frame_rate("0/0"), None);
        assert_eq!(parse_frame_rate("nonsense"), None);
    }

    #[test]
    fn alpha_is_read_from_the_pixel_format() {
        for format in [
            "rgba", "ya8", "yuva420p", "gbrap", "pal8", "rgba64be", "bgra",
        ] {
            assert!(has_alpha(format), "{format}");
        }
        for format in ["rgb24", "yuv420p", "yuvj444p", "gray", "gbrp10le"] {
            assert!(!has_alpha(format), "{format}");
        }
    }

    #[test]
    fn a_heic_grid_reports_the_whole_picture() {
        let json: Value = serde_json::from_str(
            r#"{"stream_groups": [{"type": "Tile Grid",
                "components": [{"width": 4032, "height": 3024}]}]}"#,
        )
        .unwrap();
        assert_eq!(tile_grid_size(&json), Some((4032, 3024)));
        assert_eq!(tile_grid_size(&serde_json::json!({})), None);
    }
}
