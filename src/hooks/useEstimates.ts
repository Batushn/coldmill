import { useEffect, useMemo, useRef, useState } from "react";

import { estimateOutput } from "../lib/ipc";
import type { Advanced, ConvertibleKind, QueueFile, Quality, TargetMap } from "../types";

/**
 * Pre-run output size guesses, keyed by path.
 *
 * Recomputed only when the *inputs* to the estimate change — not on every
 * progress tick, which would otherwise fire a round trip twice a second per
 * running job.
 */
export function useEstimates(
  files: QueueFile[],
  targets: TargetMap,
  quality: Quality,
  advanced: Advanced,
) {
  const [estimates, setEstimates] = useState<Record<string, number | null>>({});

  const latest = useRef(files);
  latest.current = files;

  const signature = useMemo(
    () =>
      files
        .filter((file) => file.kind !== "unsupported")
        .map(
          (file) =>
            `${file.path}>${targets[file.kind as ConvertibleKind]}>${JSON.stringify(file.edit)}`,
        )
        .join("|"),
    [files, targets],
  );

  useEffect(() => {
    const items = latest.current
      .filter((file) => file.kind !== "unsupported")
      .map((file) => ({
        path: file.path,
        kind: file.kind,
        targetFormat: targets[file.kind as ConvertibleKind],
        sizeBytes: file.sizeBytes,
        durationSecs: file.durationSecs,
        width: file.width,
        height: file.height,
        fps: file.fps,
        edit: file.edit,
      }));

    if (items.length === 0) {
      setEstimates({});
      return;
    }

    let stale = false;
    estimateOutput(items, quality, advanced)
      .then((rows) => {
        if (stale) return;
        setEstimates(Object.fromEntries(rows.map((row) => [row.path, row.bytes])));
      })
      .catch(() => {
        /* an estimate is a nicety, never an error worth showing */
      });

    return () => {
      stale = true;
    };
    // `targets` is read through the signature; including it would re-run on
    // every render of the parent.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    // Overrides change the encode, so they change the number under the row.
  }, [signature, quality, advanced]);

  return estimates;
}
