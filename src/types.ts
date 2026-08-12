export type MediaKind = "image" | "audio" | "video" | "unsupported";
export type Quality = "small" | "balanced" | "high";

/** Mirrors `FileProbe` in src-tauri/src/model.rs. */
export interface FileProbe {
  path: string;
  fileName: string;
  sizeBytes: number;
  kind: MediaKind;
  mime: string | null;
  extension: string | null;
  durationSecs: number | null;
  width: number | null;
  height: number | null;
  reason: string | null;
}

export interface JobCreated {
  jobId: string;
  path: string;
  outputPath: string;
}

export interface ProgressPayload {
  jobId: string;
  fraction: number | null;
  outBytes: number | null;
  speed: string | null;
}

export interface DonePayload {
  jobId: string;
  outputPath: string;
  outputBytes: number;
  elapsedMs: number;
}

export interface ErrorPayload {
  jobId: string;
  message: string;
  cancelled: boolean;
}

export type FileStatus =
  | "ready"
  | "queued"
  | "running"
  | "done"
  | "error"
  | "cancelled"
  | "unsupported";

export interface QueueFile extends FileProbe {
  /** Stable across re-adds: the absolute path is the identity. */
  id: string;
  status: FileStatus;
  jobId?: string;
  /** 0–1, or null while a still image is encoding (no timeline to measure). */
  fraction: number | null;
  speed: string | null;
  outputPath?: string;
  outputBytes?: number;
  message?: string;
}

export type TargetMap = Record<Exclude<MediaKind, "unsupported">, string>;
