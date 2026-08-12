import { useCallback, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { probeFile } from "../lib/ipc";
import type { FileStatus, JobCreated, MediaKind, QueueFile } from "../types";

const basename = (path: string) => path.split(/[\\/]/).pop() ?? path;

const FINISHED: FileStatus[] = ["done", "error", "cancelled"];

/** Owns the file list: adding, probing, removing, and per-job updates. */
export function useFileQueue() {
  const [files, setFiles] = useState<QueueFile[]>([]);
  const [scanning, setScanning] = useState(0);
  // The absolute path is the identity, so dropping the same file twice is a
  // no-op instead of a duplicate row.
  const known = useRef(new Set<string>());

  const addPaths = useCallback(async (paths: string[]) => {
    const fresh = paths.filter((path) => !known.current.has(path));
    if (fresh.length === 0) return;
    fresh.forEach((path) => known.current.add(path));

    setScanning((n) => n + fresh.length);
    const probed = await Promise.all(fresh.map(probeOne));
    setFiles((prev) => [...prev, ...probed]);
    setScanning((n) => Math.max(0, n - fresh.length));
  }, []);

  const pickFiles = useCallback(async () => {
    const picked = await open({ multiple: true, title: "Add files" });
    if (!picked) return;
    await addPaths(Array.isArray(picked) ? picked : [picked]);
  }, [addPaths]);

  const remove = useCallback((id: string) => {
    known.current.delete(id);
    setFiles((prev) => prev.filter((file) => file.id !== id));
  }, []);

  const clear = useCallback(() => {
    known.current.clear();
    setFiles([]);
  }, []);

  /**
   * Puts finished rows back into the queue. Called when the target format or
   * quality changes, so pressing Convert again re-encodes with the new choice
   * instead of silently skipping everything that already ran.
   */
  const resetFinished = useCallback((kind?: MediaKind) => {
    setFiles((prev) =>
      prev.map((file) => {
        const finished = FINISHED.includes(file.status);
        if (!finished || (kind && file.kind !== kind)) return file;
        return {
          ...file,
          status: "ready",
          fraction: null,
          speed: null,
          message: undefined,
          jobId: undefined,
          outputPath: undefined,
          outputBytes: undefined,
        };
      }),
    );
  }, []);

  const patchByJob = useCallback((jobId: string, changes: Partial<QueueFile>) => {
    setFiles((prev) =>
      prev.map((file) => (file.jobId === jobId ? { ...file, ...changes } : file)),
    );
  }, []);

  /** Binds the ids returned by `convert_files` to their rows. */
  const attachJobs = useCallback((jobs: JobCreated[]) => {
    const byPath = new Map(jobs.map((job) => [job.path, job]));
    setFiles((prev) =>
      prev.map((file) => {
        const job = byPath.get(file.path);
        if (!job) return file;
        return {
          ...file,
          jobId: job.jobId,
          outputPath: job.outputPath,
          status: "queued",
          fraction: null,
          speed: null,
          message: undefined,
        };
      }),
    );
  }, []);

  return {
    files,
    scanning,
    addPaths,
    pickFiles,
    remove,
    clear,
    resetFinished,
    patchByJob,
    attachJobs,
  };
}

async function probeOne(path: string): Promise<QueueFile> {
  try {
    const probe = await probeFile(path);
    return {
      ...probe,
      id: path,
      status: probe.kind === "unsupported" ? "unsupported" : "ready",
      fraction: null,
      speed: null,
    };
  } catch (error) {
    return {
      id: path,
      path,
      fileName: basename(path),
      sizeBytes: 0,
      kind: "unsupported",
      mime: null,
      extension: null,
      durationSecs: null,
      width: null,
      height: null,
      reason: String(error),
      status: "unsupported",
      fraction: null,
      speed: null,
    };
  }
}
