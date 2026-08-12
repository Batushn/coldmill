import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { formatBytes, formatDuration } from "../lib/format";
import type { QueueFile } from "../types";
import { IconAlert, IconCheck, IconClose, IconFolder, KindIcon } from "./Icons";

interface Props {
  file: QueueFile;
  target?: string;
  /** Pre-run size guess; the live projection on the file wins once it exists. */
  estimate?: number | null;
  onRemove: (id: string) => void;
  onCancel: (jobId: string) => void;
}

export function FileRow({ file, target, estimate, onRemove, onCancel }: Props) {
  const active = file.status === "running" || file.status === "queued";
  const percent = file.fraction == null ? null : Math.round(file.fraction * 100);
  const projected = file.estimatedBytes ?? estimate ?? null;

  return (
    <li className={`row is-${file.status}`}>
      <KindIcon kind={file.kind} className="row-icon" />

      <div className="row-main">
        <div className="row-name" title={file.path}>
          {file.fileName}
          {target && file.kind !== "unsupported" && (
            <span className="row-target">→ {target.toUpperCase()}</span>
          )}
        </div>
        <div className="row-meta">{metaLine(file, projected)}</div>
        {active && (
          <div className={`bar${percent == null ? " is-indeterminate" : ""}`}>
            <div
              className="bar-fill"
              style={percent == null ? undefined : { width: `${percent}%` }}
            />
          </div>
        )}
      </div>

      <div className="row-status">
        {file.status === "running" && (
          <span className="muted tabular">
            {percent == null ? (file.speed ?? "working") : `${percent}%`}
          </span>
        )}
        {file.status === "queued" && <span className="muted">waiting</span>}
        {file.status === "done" && (
          <>
            <IconCheck className="ok" />
            <button
              type="button"
              className="iconbutton"
              title="Show in folder"
              aria-label="Show in folder"
              onClick={() => file.outputPath && revealItemInDir(file.outputPath)}
            >
              <IconFolder />
            </button>
          </>
        )}
        {file.status === "error" && (
          <span className="bad error-badge" title={file.message}>
            <IconAlert />
            failed
          </span>
        )}
        {file.status === "cancelled" && <span className="muted">cancelled</span>}
        {file.status === "unsupported" && (
          <span className="muted" title={file.reason ?? undefined}>
            unsupported
          </span>
        )}

        {active ? (
          <button
            type="button"
            className="iconbutton"
            title="Cancel"
            aria-label="Cancel"
            onClick={() => file.jobId && onCancel(file.jobId)}
          >
            <IconClose />
          </button>
        ) : (
          <button
            type="button"
            className="iconbutton"
            title="Remove"
            aria-label="Remove"
            onClick={() => onRemove(file.id)}
          >
            <IconClose />
          </button>
        )}
      </div>
    </li>
  );
}

function metaLine(file: QueueFile, projected: number | null): string {
  if (file.status === "unsupported") return file.reason ?? "Unsupported file";
  if (file.status === "error") return file.message ?? "Conversion failed";

  const parts = [formatBytes(file.sizeBytes)];
  const duration = formatDuration(file.durationSecs);
  if (duration) parts.push(duration);
  if (file.width && file.height) parts.push(`${file.width}×${file.height}`);
  if (file.triangles) parts.push(`${file.triangles.toLocaleString()} triangles`);

  if (file.status === "done" && file.outputBytes != null) {
    parts.push(`→ ${formatBytes(file.outputBytes)}`);
  } else if (projected != null) {
    // Always a tilde: encoders are content-adaptive and this is a model, not
    // a measurement.
    parts.push(`→ ~${formatBytes(projected)}`);
  }
  return parts.join(" · ");
}
