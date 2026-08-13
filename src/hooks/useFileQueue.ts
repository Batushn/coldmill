import { useCallback, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { probeFile } from "../lib/ipc";
import { NO_EDIT, type EditSpec, type FileStatus, type JobCreated, type MediaKind, type QueueFile } from "../types";

const basename = (path: string) => path.split(/[\\/]/).pop() ?? path;

const FINISHED: FileStatus[] = ["done", "error", "cancelled"];

/** Owns the file list: adding, probing, removing, and per-job updates. */
export function useFileQueue() {
  const [files, setFiles] = useState<QueueFile[]>([]);
  const [scanning, setScanning] = useState(0);
  // The absolute path is the identity, so dropping the same file twice is a
  // no-op instead of a duplicate row.
  const known = useRef(new Set<string>());
  // convert_files starts encoding before it returns, so events can arrive
  // before the rows know their job id. Park those and replay them on bind —
  // otherwise a fast job (a small image) finishes into the void and its row
  // sits at "waiting" forever.
  const early = useRef(new Map<string, Partial<QueueFile>>());

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
    // The dialog title comes from the OS shell, so it stays untranslated here
    // rather than threading the translator through the queue.
    const picked = await open({ multiple: true, title: "Coldmill" });
    if (!picked) return;
    await addPaths(Array.isArray(picked) ? picked : [picked]);
  }, [addPaths]);

  /// Re-inspects files after the modules change: what was "unsupported"
  /// yesterday may be convertible now that its engine is installed.
  const reprobe = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    const probed = await Promise.all(paths.map(probeOne));
    setFiles((prev) =>
      prev.map((file) => probed.find((fresh) => fresh.path === file.path) ?? file),
    );
  }, []);

  /** Trim, split, mute or re-frame one file. Anything already converted is
   *  queued again, since the result no longer matches what was asked for. */
  const setEdit = useCallback((id: string, patch: Partial<EditSpec>) => {
    setFiles((prev) =>
      prev.map((file) => {
        if (file.id !== id) return file;
        const finished = FINISHED.includes(file.status);
        return {
          ...file,
          edit: { ...file.edit, ...patch },
          ...(finished ? { status: "ready" as const, fraction: null, message: undefined } : {}),
        };
      }),
    );
  }, []);

  const remove = useCallback((id: string) => {
    known.current.delete(id);
    setFiles((prev) => prev.filter((file) => file.id !== id));
  }, []);

  const clear = useCallback(() => {
    known.current.clear();
    early.current.clear();
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
          outputs: undefined,
          outputBytes: undefined,
        };
      }),
    );
  }, []);

  const patchByJob = useCallback((jobId: string, changes: Partial<QueueFile>) => {
    setFiles((prev) => {
      if (!prev.some((file) => file.jobId === jobId)) {
        // Job ids are UUIDs, so a parked patch can never hit the wrong row.
        early.current.set(jobId, { ...early.current.get(jobId), ...changes });
        return prev;
      }
      return prev.map((file) => (file.jobId === jobId ? { ...file, ...changes } : file));
    });
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
          outputs: job.outputs,
          status: "queued",
          fraction: null,
          speed: null,
          message: undefined,
          // Anything that already happened for this job wins over "queued".
          ...early.current.get(job.jobId),
        };
      }),
    );
  }, []);

  return {
    files,
    scanning,
    addPaths,
    pickFiles,
    reprobe,
    setEdit,
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
      edit: NO_EDIT,
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
      fps: null,
      triangles: null,
      reason: String(error),
      status: "unsupported",
      fraction: null,
      speed: null,
      edit: NO_EDIT,
    };
  }
}
