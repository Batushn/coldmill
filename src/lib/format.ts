import type { MediaKind } from "../types";

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

export function formatDuration(seconds: number | null): string | null {
  if (seconds == null) return null;
  const total = Math.round(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

const LABELS: Record<MediaKind, [string, string]> = {
  video: ["video", "videos"],
  audio: ["audio file", "audio files"],
  image: ["image", "images"],
  unsupported: ["unsupported file", "unsupported files"],
};

export const kindLabel = (kind: MediaKind, count: number) =>
  `${count} ${LABELS[kind][count === 1 ? 0 : 1]}`;

/** Trailing part of a path, for the "output folder" line. */
export function shortenPath(path: string, segments = 2): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  if (parts.length <= segments) return path;
  return `…${path.includes("\\") ? "\\" : "/"}${parts.slice(-segments).join(path.includes("\\") ? "\\" : "/")}`;
}
