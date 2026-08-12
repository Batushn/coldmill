//! ffprobe wrapper. Used for two things: knowing the total duration up front
//! (without it there is no percentage to report) and confirming what streams a
//! container really holds.

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
}

pub async fn inspect(app: &AppHandle, path: &str) -> Result<MediaInfo, String> {
    let output = app
        .shell()
        .sidecar("ffprobe")
        .map_err(|e| format!("ffprobe sidecar missing: {e}"))?
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path,
        ])
        .output()
        .await
        .map_err(|e| format!("ffprobe failed to start: {e}"))?;

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

    Ok(info)
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
}
