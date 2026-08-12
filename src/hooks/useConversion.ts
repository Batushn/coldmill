import { useCallback, useEffect } from "react";

import { cancelAll, cancelJob, convertFiles, onDone, onError, onProgress } from "../lib/ipc";
import type { QueueFile, Quality, TargetMap } from "../types";

interface Options {
  patchByJob: (jobId: string, changes: Partial<QueueFile>) => void;
  attachJobs: (jobs: Awaited<ReturnType<typeof convertFiles>>) => void;
}

/** Wires the convert:* events into the queue and exposes start/cancel. */
export function useConversion({ patchByJob, attachJobs }: Options) {
  useEffect(() => {
    const subscriptions = [
      onProgress(({ jobId, fraction, speed }) =>
        patchByJob(jobId, { status: "running", fraction, speed }),
      ),
      onDone(({ jobId, outputPath, outputBytes }) =>
        patchByJob(jobId, {
          status: "done",
          fraction: 1,
          speed: null,
          outputPath,
          outputBytes,
        }),
      ),
      onError(({ jobId, message, cancelled }) =>
        patchByJob(jobId, {
          status: cancelled ? "cancelled" : "error",
          fraction: null,
          speed: null,
          message,
        }),
      ),
    ];
    return () => {
      subscriptions.forEach((pending) => pending.then((unlisten) => unlisten()));
    };
  }, [patchByJob]);

  const start = useCallback(
    async (files: QueueFile[], targets: TargetMap, quality: Quality, outputDir: string | null) => {
      const items = files
        .filter((file) => file.kind !== "unsupported")
        .map((file) => ({
          path: file.path,
          targetFormat: targets[file.kind as keyof TargetMap],
          kind: file.kind,
          durationSecs: file.durationSecs,
        }));
      if (items.length === 0) return;

      const jobs = await convertFiles(items, quality, outputDir);
      attachJobs(jobs);
    },
    [attachJobs],
  );

  return { start, cancel: cancelJob, cancelEverything: cancelAll };
}
