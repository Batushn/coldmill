import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { useI18n, type Translator } from "../i18n";
import { formatBytes, formatDuration } from "../lib/format";
import type { EditSpec, QueueFile } from "../types";
import { NO_COLOR } from "../types";
import { EditPanel } from "./EditPanel";
import { IconAlert, IconCheck, IconClose, IconFolder } from "./Icons";
import { Thumb } from "./Thumb";

interface Props {
  file: QueueFile;
  target?: string;
  poster: string | null | undefined;
  /** Pre-run size guess; the live projection on the file wins once it exists. */
  estimate?: number | null;
  /** How many triangles a 3D model will come out with. */
  expectedTriangles?: number | null;
  onRemove: (id: string) => void;
  onCancel: (jobId: string) => void;
  onEdit: (patch: Partial<EditSpec>) => void;
}

export function FileRow({
  file,
  target,
  poster,
  estimate,
  expectedTriangles,
  onRemove,
  onCancel,
  onEdit,
}: Props) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const active = file.status === "running" || file.status === "queued";
  // Anything with a timeline can be cut; a picture has no timeline but can
  // still be graded, so it gets the panel without the track.
  const editable =
    ((file.kind === "video" || file.kind === "audio") && (file.durationSecs ?? 0) > 0) ||
    file.kind === "image" ||
    file.kind === "model";
  const edited =
    file.edit.trimStart != null ||
    file.edit.trimEnd != null ||
    file.edit.mute ||
    file.edit.orientation !== "keep" ||
    file.edit.splitPoints.length > 0 ||
    JSON.stringify(file.edit.color) !== JSON.stringify(NO_COLOR) ||
    file.edit.mesh.pivot !== "keep";
  const percent = file.fraction == null ? null : Math.round(file.fraction * 100);
  const projected = file.estimatedBytes ?? estimate ?? null;

  return (
    <li className={`row is-${file.status}${open ? " is-open" : ""}`}>
      <div className="row-top">
        <Thumb file={file} poster={poster} />

      <div className="row-main">
        <div className="row-name" title={file.path}>
          {file.fileName}
          {target && file.kind !== "unsupported" && (
            <span className="row-target">→ {target.toUpperCase()}</span>
          )}
        </div>
        <div className="row-meta">{metaLine(file, projected, expectedTriangles, t)}</div>
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
        {editable && (
          <button
            type="button"
            className={`chip${edited ? " is-active" : ""}`}
            aria-expanded={open}
            onClick={() => setOpen((wasOpen) => !wasOpen)}
          >
            {t("edit.open")}
          </button>
        )}
        {file.status === "running" && (
          <span className="muted tabular">
            {percent == null ? (file.speed ?? t("status.working")) : `${percent}%`}
          </span>
        )}
        {file.status === "queued" && <span className="muted">{t("status.waiting")}</span>}
        {file.status === "done" && (
          <>
            <IconCheck className="ok" />
            <button
              type="button"
              className="iconbutton"
              title={t("action.showInFolder")}
              aria-label={t("action.showInFolder")}
              onClick={() => file.outputPath && revealItemInDir(file.outputPath)}
            >
              <IconFolder />
            </button>
          </>
        )}
        {file.status === "error" && (
          <span className="bad error-badge" title={file.message}>
            <IconAlert />
            {t("status.failed")}
          </span>
        )}
        {file.status === "cancelled" && <span className="muted">{t("status.cancelled")}</span>}
        {file.status === "unsupported" && (
          <span className="muted" title={file.reason ?? undefined}>
            {t("status.unsupported")}
          </span>
        )}

        {active ? (
          <button
            type="button"
            className="iconbutton"
            title={t("action.cancel")}
            aria-label={t("action.cancel")}
            onClick={() => file.jobId && onCancel(file.jobId)}
          >
            <IconClose />
          </button>
        ) : (
          <button
            type="button"
            className="iconbutton"
            title={t("action.remove")}
            aria-label={t("action.remove")}
            onClick={() => onRemove(file.id)}
          >
            <IconClose />
          </button>
        )}
        </div>
      </div>

      {open && editable && (
        <EditPanel file={file} disabled={active} onChange={onEdit} />
      )}
    </li>
  );
}

function metaLine(
  file: QueueFile,
  projected: number | null,
  expectedTriangles: number | null | undefined,
  t: Translator,
): string {
  // Engine messages are not translated: they come from ffmpeg and friends.
  if (file.status === "unsupported") return file.reason ?? t("status.unsupportedFile");
  if (file.status === "error") return file.message ?? t("status.conversionFailed");

  const parts = [formatBytes(file.sizeBytes)];
  const duration = formatDuration(file.durationSecs);
  if (duration) parts.push(duration);
  if (file.width && file.height) parts.push(`${file.width}×${file.height}`);
  if (file.triangles) {
    parts.push(t("meta.triangles", { count: file.triangles.toLocaleString() }));
    // A tilde: clustering cannot promise an exact count, and saying a precise
    // number we might miss would be worse than admitting the approximation.
    if (expectedTriangles != null && expectedTriangles !== file.triangles) {
      parts.push(
        `→ ~${t("meta.triangles", { count: expectedTriangles.toLocaleString() })}`,
      );
    }
  }

  if (file.status === "done" && file.outputBytes != null) {
    // A split writes several files; saying "→ 12 MB" alone reads as if only
    // one came out.
    const pieces = file.outputs?.length ?? 1;
    const size = formatBytes(file.outputBytes);
    parts.push(
      pieces > 1 ? `→ ${t("meta.pieces", { count: pieces })} · ${size}` : `→ ${size}`,
    );
  } else if (projected != null) {
    // Always a tilde: encoders are content-adaptive and this is a model, not
    // a measurement.
    parts.push(`→ ~${formatBytes(projected)}`);
  }
  return parts.join(" · ");
}
