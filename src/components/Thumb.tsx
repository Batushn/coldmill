import { useRef, useState } from "react";

import { formatDuration } from "../lib/format";
import { scrubStrip } from "../lib/ipc";
import type { QueueFile, ScrubStrip } from "../types";
import { KindIcon } from "./Icons";

interface Props {
  file: QueueFile;
  poster: string | null | undefined;
}

/**
 * The preview image, and — for video — a scrubber.
 *
 * Moving the pointer across a video walks its whole length, the way a
 * timeline does. The filmstrip behind that is one tiled image fetched on
 * first hover: cheap to slide, and never built for files nobody looks at.
 */
export function Thumb({ file, poster }: Props) {
  const [strip, setStrip] = useState<ScrubStrip | null>(null);
  const [fraction, setFraction] = useState<number | null>(null);
  const requested = useRef(false);
  const box = useRef<HTMLDivElement>(null);

  const duration = file.durationSecs ?? 0;
  const scrubbable = file.kind === "video" && duration > 0;

  const onEnter = () => {
    if (!scrubbable || requested.current) return;
    requested.current = true;
    scrubStrip(file.path, duration)
      .then(setStrip)
      .catch(() => {
        /* the poster alone is a fine outcome */
      });
  };

  const onMove = (event: React.PointerEvent) => {
    if (!scrubbable) return;
    const rect = box.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return;
    setFraction(Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width)));
  };

  // Quoted: a data URI may legally contain parentheses, which would otherwise
  // close the CSS url() early.
  // The strip is `frames` images side by side, so showing frame N means
  // blowing the background up to frames×100% and sliding it along.
  const scrubStyle =
    strip && fraction != null
      ? {
          backgroundImage: `url("${strip.dataUri}")`,
          backgroundSize: `${strip.frames * 100}% 100%`,
          backgroundPosition: `${(Math.round(fraction * (strip.frames - 1)) / (strip.frames - 1)) * 100}% 0`,
        }
      : poster
        ? { backgroundImage: `url("${poster}")` }
        : undefined;

  return (
    <div
      ref={box}
      className={`thumb is-${file.kind}${scrubbable ? " is-scrubbable" : ""}`}
      style={scrubStyle}
      onPointerEnter={onEnter}
      onPointerMove={onMove}
      onPointerLeave={() => setFraction(null)}
    >
      {!poster && <KindIcon kind={file.kind} className="thumb-fallback" />}
      {scrubbable && fraction != null && (
        <span className="thumb-time tabular">{formatDuration(fraction * duration)}</span>
      )}
    </div>
  );
}
