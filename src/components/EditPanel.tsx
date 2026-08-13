import { useEffect, useMemo, useRef, useState } from "react";

import { useI18n } from "../i18n";
import { formatDuration } from "../lib/format";
import { scrubStrip } from "../lib/ipc";
import type { EditSpec, Orientation, QueueFile, ScrubStrip } from "../types";
import { IconClose } from "./Icons";

const ORIENTATIONS: Orientation[] = ["keep", "portrait", "landscape", "square"];

interface Props {
  file: QueueFile;
  disabled: boolean;
  onChange: (edit: Partial<EditSpec>) => void;
}

/**
 * Trim, split, mute and re-frame — laid over the same filmstrip the row
 * already scrubs with, so the cut is made where you can see it rather than by
 * typing timecodes into boxes.
 */
/**
 * How far apart two cuts must be before a file is worth writing between them.
 * Must match `MIN_SEGMENT_SECS` in src-tauri/src/edit.rs, or the panel promises
 * a piece count the backend will not produce.
 */
const MIN_SEGMENT_SECS = 0.05;

export function EditPanel({ file, disabled, onChange }: Props) {
  const { t } = useI18n();
  const [strip, setStrip] = useState<ScrubStrip | null>(null);
  const [playhead, setPlayhead] = useState(0);
  const [dragging, setDragging] = useState<"start" | "end" | null>(null);
  const track = useRef<HTMLDivElement>(null);

  const duration = file.durationSecs ?? 0;
  const edit = file.edit;
  const start = edit.trimStart ?? 0;
  const end = edit.trimEnd ?? duration;
  const isVideo = file.kind === "video";

  /**
   * The cuts that will actually survive `segments()`. Showing the raw list
   * would let the panel count pieces that never get written — which is how
   * splitting into three came to yield one file.
   */
  const usableCuts = useMemo(() => {
    const kept: number[] = [];
    for (const point of [...edit.splitPoints].sort((a, b) => a - b)) {
      if (point <= start + MIN_SEGMENT_SECS) continue;
      if (point >= end - MIN_SEGMENT_SECS) continue;
      const previous = kept[kept.length - 1];
      if (previous !== undefined && point - previous < MIN_SEGMENT_SECS) continue;
      kept.push(point);
    }
    return kept;
  }, [edit.splitPoints, start, end]);

  const pending = playhead * duration;
  // Refuse a cut that lands on a trim edge or on top of one already there:
  // silently dropping it later is what made the button feel dead.
  const canSplit =
    duration > 0 &&
    pending > start + MIN_SEGMENT_SECS &&
    pending < end - MIN_SEGMENT_SECS &&
    !usableCuts.some((point) => Math.abs(point - pending) < MIN_SEGMENT_SECS);

  useEffect(() => {
    if (!isVideo || duration <= 0) return;
    let stale = false;
    scrubStrip(file.path, duration)
      .then((found) => !stale && setStrip(found))
      .catch(() => {
        /* the track works without pictures, it is just less useful */
      });
    return () => {
      stale = true;
    };
  }, [file.path, duration, isVideo]);

  const fractionAt = (clientX: number) => {
    const box = track.current?.getBoundingClientRect();
    if (!box || box.width === 0) return 0;
    return Math.min(1, Math.max(0, (clientX - box.left) / box.width));
  };

  // Dragging is tracked on the window: releasing outside the track should still
  // end the drag, and the pointer regularly outruns a 6px handle.
  useEffect(() => {
    if (!dragging) return;

    const onMove = (event: PointerEvent) => {
      const seconds = fractionAt(event.clientX) * duration;
      if (dragging === "start") {
        onChange({ trimStart: Math.min(seconds, end - 0.2) });
      } else {
        onChange({ trimEnd: Math.max(seconds, start + 0.2) });
      }
    };
    const onUp = () => setDragging(null);

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [dragging, duration, end, start, onChange]);

  if (duration <= 0) return null;

  const percent = (seconds: number) => `${(seconds / duration) * 100}%`;
  const kept = end - start;

  return (
    <div className="editpanel">
      <div
        ref={track}
        className="edittrack"
        style={strip ? { backgroundImage: `url("${strip.dataUri}")` } : undefined}
        onPointerDown={(event) => setPlayhead(fractionAt(event.clientX))}
      >
        {/* Everything outside the trim is dimmed rather than hidden, so you can
            still see what you are cutting away. */}
        <div className="edittrack-off" style={{ left: 0, width: percent(start) }} />
        <div className="edittrack-off" style={{ left: percent(end), right: 0 }} />

        <div
          className="edittrack-handle is-start"
          style={{ left: percent(start) }}
          role="slider"
          tabIndex={0}
          aria-label={t("edit.trimStart")}
          aria-valuenow={Math.round(start)}
          aria-valuemin={0}
          aria-valuemax={Math.round(duration)}
          onPointerDown={(event) => {
            event.stopPropagation();
            if (!disabled) setDragging("start");
          }}
        />
        <div
          className="edittrack-handle is-end"
          style={{ left: percent(end) }}
          role="slider"
          tabIndex={0}
          aria-label={t("edit.trimEnd")}
          aria-valuenow={Math.round(end)}
          aria-valuemin={0}
          aria-valuemax={Math.round(duration)}
          onPointerDown={(event) => {
            event.stopPropagation();
            if (!disabled) setDragging("end");
          }}
        />

        {usableCuts.map((point) => (
          <button
            type="button"
            key={point}
            className="edittrack-split"
            style={{ left: percent(point) }}
            disabled={disabled}
            title={t("edit.removeSplit")}
            aria-label={t("edit.removeSplit")}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={() =>
              onChange({ splitPoints: usableCuts.filter((kept) => kept !== point) })
            }
          />
        ))}

        <div className="edittrack-playhead" style={{ left: `${playhead * 100}%` }} />
      </div>

      <div className="editrow">
        <span className="muted tabular">
          {formatDuration(start)} – {formatDuration(end)}
          {usableCuts.length > 0 && ` · ${t.plural("edit.pieces", usableCuts.length + 1)}`}
        </span>
        <span className="tabular">{formatDuration(kept)}</span>

        <span className="spacer" />

        <button
          type="button"
          className="chip"
          disabled={disabled || !canSplit}
          title={canSplit ? undefined : t("edit.splitHint")}
          onClick={() => onChange({ splitPoints: [...usableCuts, pending].sort((a, b) => a - b) })}
        >
          {t("edit.split")}
        </button>

        {isVideo && (
          <button
            type="button"
            className={`chip${edit.mute ? " is-active" : ""}`}
            disabled={disabled}
            aria-pressed={edit.mute}
            onClick={() => onChange({ mute: !edit.mute })}
          >
            {t("edit.mute")}
          </button>
        )}

        {isVideo && (
          <div className="segmented is-compact">
            {ORIENTATIONS.map((orientation) => (
              <button
                key={orientation}
                type="button"
                disabled={disabled}
                aria-pressed={edit.orientation === orientation}
                className={edit.orientation === orientation ? "is-active" : undefined}
                title={t(`edit.${orientation}`)}
                onClick={() => onChange({ orientation })}
              >
                {t(`edit.${orientation}Short`)}
              </button>
            ))}
          </div>
        )}

        <button
          type="button"
          className="iconbutton"
          disabled={disabled}
          title={t("edit.reset")}
          aria-label={t("edit.reset")}
          onClick={() =>
            onChange({
              trimStart: null,
              trimEnd: null,
              mute: false,
              orientation: "keep",
              splitPoints: [],
            })
          }
        >
          <IconClose />
        </button>
      </div>
    </div>
  );
}
