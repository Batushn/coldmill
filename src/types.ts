export type MediaKind = "image" | "audio" | "video" | "document" | "model" | "unsupported";
export type Quality = "small" | "balanced" | "high";
export type EngineId = "pandoc" | "typst" | "blender";

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
  fps: number | null;
  triangles: number | null;
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
  estimatedBytes: number | null;
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
  /** Projected final size, from the backend's live byte counter. */
  estimatedBytes?: number | null;
  message?: string;
}

/** Only kinds that can actually be converted get a target format. */
export type ConvertibleKind = Exclude<MediaKind, "unsupported">;
export type ViewMode = "list" | "grid";

/** A row of video frames tiled into one image, slid under the cursor. */
export interface ScrubStrip {
  dataUri: string;
  frames: number;
}
export type TargetMap = Record<ConvertibleKind, string>;

// --- Modules ---------------------------------------------------------------

export interface Settings {
  setupDone: boolean;
  documents: boolean;
  models: boolean;
  blender: boolean;
}

export interface EngineStatus {
  id: EngineId;
  label: string;
  version: string;
  installed: boolean;
  downloadBytes: number;
}

export interface SetupState {
  settings: Settings;
  engines: EngineStatus[];
  libreoffice: string | null;
}

export interface EngineProgress {
  engineId: EngineId;
  label: string;
  received: number;
  total: number | null;
  phase: "download" | "extract";
}

export interface EngineEvent {
  engineId: EngineId;
  label: string;
  message: string | null;
}

/** Pre-run size guess. `bytes` is null when there is nothing honest to say. */
export interface Estimate {
  path: string;
  bytes: number | null;
}
