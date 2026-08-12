import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  DonePayload,
  ErrorPayload,
  FileProbe,
  JobCreated,
  MediaKind,
  ProgressPayload,
  Quality,
} from "../types";

export const probeFile = (path: string) => invoke<FileProbe>("probe_file", { path });

export const supportedTargets = () =>
  invoke<Record<"image" | "audio" | "video", string[]>>("supported_targets");

export const maxConcurrency = () => invoke<number>("max_concurrency");

export interface ConvertItem {
  path: string;
  targetFormat: string;
  kind: MediaKind;
  durationSecs: number | null;
}

export const convertFiles = (
  items: ConvertItem[],
  quality: Quality,
  outputDir: string | null,
) => invoke<JobCreated[]>("convert_files", { request: { items, quality, outputDir } });

export const cancelJob = (jobId: string) => invoke<boolean>("cancel_job", { jobId });

export const cancelAll = () => invoke<string[]>("cancel_all");

export const onProgress = (fn: (p: ProgressPayload) => void): Promise<UnlistenFn> =>
  listen<ProgressPayload>("convert:progress", (e) => fn(e.payload));

export const onDone = (fn: (p: DonePayload) => void): Promise<UnlistenFn> =>
  listen<DonePayload>("convert:done", (e) => fn(e.payload));

export const onError = (fn: (p: ErrorPayload) => void): Promise<UnlistenFn> =>
  listen<ErrorPayload>("convert:error", (e) => fn(e.payload));
