import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  Advanced,
  DonePayload,
  EditSpec,
  EngineEvent,
  EngineProgress,
  ErrorPayload,
  Estimate,
  FileProbe,
  JobCreated,
  MediaKind,
  ProgressPayload,
  Quality,
  ScrubStrip,
  Settings,
  SetupState,
} from "../types";

export const probeFile = (path: string) => invoke<FileProbe>("probe_file", { path });

export const supportedTargets = () =>
  invoke<Partial<Record<MediaKind, string[]>>>("supported_targets");

export const maxConcurrency = () => invoke<number>("max_concurrency");

export interface ConvertItem {
  path: string;
  targetFormat: string;
  kind: MediaKind;
  durationSecs: number | null;
  edit: EditSpec;
}

export const convertFiles = (
  items: ConvertItem[],
  quality: Quality,
  advanced: Advanced,
  outputDir: string | null,
) => invoke<JobCreated[]>("convert_files", { request: { items, quality, advanced, outputDir } });

export const cancelJob = (jobId: string) => invoke<boolean>("cancel_job", { jobId });

export const cancelAll = () => invoke<string[]>("cancel_all");

export const thumbnail = (path: string, kind: MediaKind, durationSecs: number | null) =>
  invoke<string | null>("thumbnail", { path, kind, durationSecs });

export const scrubStrip = (path: string, durationSecs: number) =>
  invoke<ScrubStrip | null>("scrub_strip", { path, durationSecs });

// --- Modules ---------------------------------------------------------------

export const setupState = () => invoke<SetupState>("setup_state");

export const applySetup = (settings: Settings) =>
  invoke<SetupState>("apply_setup", { settings });

// --- Estimates -------------------------------------------------------------

export interface EstimateItem {
  path: string;
  kind: MediaKind;
  targetFormat: string;
  sizeBytes: number;
  durationSecs: number | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  triangles: number | null;
  edit: EditSpec;
}

export const estimateOutput = (items: EstimateItem[], quality: Quality, advanced: Advanced) =>
  invoke<Estimate[]>("estimate_output", { items, quality, advanced });

// --- Events ----------------------------------------------------------------

export const onProgress = (fn: (p: ProgressPayload) => void): Promise<UnlistenFn> =>
  listen<ProgressPayload>("convert:progress", (e) => fn(e.payload));

export const onDone = (fn: (p: DonePayload) => void): Promise<UnlistenFn> =>
  listen<DonePayload>("convert:done", (e) => fn(e.payload));

export const onError = (fn: (p: ErrorPayload) => void): Promise<UnlistenFn> =>
  listen<ErrorPayload>("convert:error", (e) => fn(e.payload));

export const onEngineProgress = (fn: (p: EngineProgress) => void): Promise<UnlistenFn> =>
  listen<EngineProgress>("engine:progress", (e) => fn(e.payload));

export const onEngineDone = (fn: (p: EngineEvent) => void): Promise<UnlistenFn> =>
  listen<EngineEvent>("engine:done", (e) => fn(e.payload));

export const onEngineError = (fn: (p: EngineEvent) => void): Promise<UnlistenFn> =>
  listen<EngineEvent>("engine:error", (e) => fn(e.payload));
