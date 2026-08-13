import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { useI18n } from "../i18n";
import { formatBytes } from "../lib/format";
import type { QueueFile } from "../types";
import { IconAlert, IconCheck, IconClose, IconFolder } from "./Icons";
import { Thumb } from "./Thumb";

interface Props {
  file: QueueFile;
  target?: string;
  poster: string | null | undefined;
  estimate?: number | null;
  onRemove: (id: string) => void;
  onCancel: (jobId: string) => void;
}

/** The grid tile: the same information as a row, arranged around the picture
 *  instead of beside it. */
export function FileCard({ file, target, poster, estimate, onRemove, onCancel }: Props) {
  const { t } = useI18n();
  const active = file.status === "running" || file.status === "queued";
  const percent = file.fraction == null ? null : Math.round(file.fraction * 100);
  const projected = file.estimatedBytes ?? estimate ?? null;

  return (
    <li className={`card is-${file.status}`}>
      <div className="card-media">
        <Thumb file={file} poster={poster} />

        <div className="card-overlay">
          {file.status === "done" && (
            <button
              type="button"
              className="iconbutton"
              title={t("action.showInFolder")}
              aria-label={t("action.showInFolder")}
              onClick={() => file.outputPath && revealItemInDir(file.outputPath)}
            >
              <IconFolder />
            </button>
          )}
          <button
            type="button"
            className="iconbutton"
            title={active ? t("action.cancel") : t("action.remove")}
            aria-label={active ? t("action.cancel") : t("action.remove")}
            onClick={() => (active ? file.jobId && onCancel(file.jobId) : onRemove(file.id))}
          >
            <IconClose />
          </button>
        </div>

        {file.status === "done" && <IconCheck className="card-badge ok" />}
        {file.status === "error" && (
          <span className="card-badge bad" title={file.message}>
            <IconAlert />
          </span>
        )}

        {active && (
          <div className={`bar card-bar${percent == null ? " is-indeterminate" : ""}`}>
            <div
              className="bar-fill"
              style={percent == null ? undefined : { width: `${percent}%` }}
            />
          </div>
        )}
      </div>

      <div className="card-name" title={file.path}>
        {file.fileName}
      </div>
      <div className="card-meta">
        {file.status === "unsupported"
          ? (file.reason ?? t("status.unsupportedFile"))
          : [
              formatBytes(file.sizeBytes),
              target && `→ ${target.toUpperCase()}`,
              file.status === "done" && file.outputBytes != null
                ? formatBytes(file.outputBytes)
                : projected != null
                  ? `~${formatBytes(projected)}`
                  : null,
            ]
              .filter(Boolean)
              .join(" · ")}
      </div>
    </li>
  );
}
