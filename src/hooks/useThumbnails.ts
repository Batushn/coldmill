import { useEffect, useRef, useState } from "react";

import { thumbnail } from "../lib/ipc";
import type { QueueFile } from "../types";

/** Posters are cheap but not free, and a dropped folder can be hundreds of
 *  files. Three at a time keeps the queue responsive while they fill in. */
const CONCURRENCY = 3;

/**
 * Fetches one preview image per file, in the background.
 *
 * Rows appear immediately and gain their picture a moment later — the queue is
 * never blocked waiting for ffmpeg to draw something.
 */
export function useThumbnails(files: QueueFile[]) {
  const [posters, setPosters] = useState<Record<string, string | null>>({});
  const claimed = useRef(new Set<string>());
  const running = useRef(0);

  useEffect(() => {
    const pump = () => {
      while (running.current < CONCURRENCY) {
        const next = files.find(
          (file) => file.kind !== "unsupported" && !claimed.current.has(file.path),
        );
        if (!next) return;

        claimed.current.add(next.path);
        running.current += 1;

        thumbnail(next.path, next.kind, next.durationSecs)
          .then((poster) => setPosters((prev) => ({ ...prev, [next.path]: poster })))
          .catch(() => setPosters((prev) => ({ ...prev, [next.path]: null })))
          .finally(() => {
            running.current -= 1;
            pump();
          });
      }
    };
    pump();
  }, [files]);

  return posters;
}
