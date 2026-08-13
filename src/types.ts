export type MediaKind = "image" | "audio" | "video" | "document" | "model" | "unsupported";
export type Quality = "small" | "balanced" | "high";
export type EngineId =
  | "pandoc"
  | "typst"
  | "blender"
  | "whisper"
  | "whisper-model"
  | "ocr-detection"
  | "ocr-recognition"
  | "piper"
  | "piper-voice"
  | "imagemagick";

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
  /** Every file the job will write — more than one when the clip is split. */
  outputs: string[];
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
  outputs: string[];
  /** Every file added up, not just the first. */
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
  outputs?: string[];
  outputBytes?: number;
  /** Projected final size, from the backend's live byte counter. */
  estimatedBytes?: number | null;
  message?: string;
  edit: EditSpec;
}

/** Only kinds that can actually be converted get a target format. */
export type ConvertibleKind = Exclude<MediaKind, "unsupported">;
export type ViewMode = "list" | "grid";
export type Orientation = "keep" | "portrait" | "landscape" | "square";
/** How the frame is made to fit a new shape. Only used when reframing. */
export type Fit = "crop" | "pad" | "blur";

/** Mirrors `EditSpec` in src-tauri/src/edit.rs. */
export interface EditSpec {
  trimStart: number | null;
  trimEnd: number | null;
  mute: boolean;
  orientation: Orientation;
  fit: Fit;
  /** Cut points in seconds, inside the trimmed range. */
  splitPoints: number[];
}

export const NO_EDIT: EditSpec = {
  trimStart: null,
  trimEnd: null,
  mute: false,
  orientation: "keep",
  fit: "crop",
  splitPoints: [],
};

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
  speech: boolean;
  ocr: boolean;
  tts: boolean;
  extraImages: boolean;
}

export interface EngineStatus {
  id: EngineId;
  label: string;
  version: string;
  installed: boolean;
  downloadBytes: number;
  /** False when this engine has no build for the current platform. */
  available: boolean;
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
