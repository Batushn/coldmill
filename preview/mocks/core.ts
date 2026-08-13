// Stand-in for the Tauri IPC bridge so the real UI can run in a plain browser.
// Used only by the screenshot harness — see preview/README.md.
import { emit } from "./event";
import { SCENE, type Scene } from "./scene";

interface ProbeSeed {
  fileName: string;
  sizeBytes: number;
  kind: string;
  durationSecs?: number;
  width?: number;
  height?: number;
  fps?: number;
  triangles?: number;
  reason?: string;
}

const SEEDS: ProbeSeed[] = [
  { fileName: "beach-sunset.mov", sizeBytes: 486_000_000, kind: "video", durationSecs: 94, width: 3840, height: 2160, fps: 30 },
  { fileName: "drone-pass.mp4", sizeBytes: 212_000_000, kind: "video", durationSecs: 41, width: 1920, height: 1080, fps: 60 },
  { fileName: "interview-raw.mkv", sizeBytes: 903_000_000, kind: "video", durationSecs: 612, width: 1920, height: 1080, fps: 25 },
  { fileName: "voice-memo.m4a", sizeBytes: 8_400_000, kind: "audio", durationSecs: 322 },
  { fileName: "podcast-ep12.wav", sizeBytes: 318_000_000, kind: "audio", durationSecs: 1804 },
  { fileName: "IMG_4821.HEIC", sizeBytes: 3_100_000, kind: "image", width: 4032, height: 3024 },
  { fileName: "screenshot.png", sizeBytes: 1_450_000, kind: "image", width: 2560, height: 1440 },
  { fileName: "scan-0043.tiff", sizeBytes: 22_800_000, kind: "image", width: 4960, height: 7016 },
  { fileName: "contract.docx", sizeBytes: 148_000, kind: "document" },
  { fileName: "notes.md", sizeBytes: 6_200, kind: "document" },
  { fileName: "bracket.stl", sizeBytes: 4_900_000, kind: "model", triangles: 98_412 },
  { fileName: "archive.7z", sizeBytes: 51_000_000, kind: "unsupported", reason: "application/x-7z-compressed is not a convertible file" },
];

const seedFor = (path: string) => SEEDS.find((seed) => path.endsWith(seed.fileName)) ?? SEEDS[0];

export const DEMO_PATHS = SEEDS.map((seed) => `C:\\Users\\you\\Footage\\${seed.fileName}`);

const jobs = new Map<string, { path: string; kind: string; durationSecs: number | null }>();

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  switch (command) {
    case "probe_file": {
      const path = args?.path as string;
      const seed = seedFor(path);
      return {
        path,
        fileName: seed.fileName,
        sizeBytes: seed.sizeBytes,
        kind: seed.kind,
        mime: null,
        extension: seed.fileName.split(".").pop()?.toLowerCase() ?? null,
        durationSecs: seed.durationSecs ?? null,
        width: seed.width ?? null,
        height: seed.height ?? null,
        fps: seed.fps ?? null,
        triangles: seed.triangles ?? null,
        reason: seed.reason ?? null,
      } as T;
    }

    case "supported_targets":
      return {
        video: ["mp4", "mkv", "webm", "mov", "avi", "gif"],
        audio: ["mp3", "m4a", "aac", "opus", "ogg", "flac", "wav"],
        image: ["jpg", "png", "webp", "avif", "tiff", "bmp", "gif"],
        document: ["pdf", "docx", "odt", "html", "md", "epub", "rtf", "tex", "rst", "txt"],
        model: ["glb", "obj", "stl"],
      } as T;

    case "max_concurrency":
      return 8 as T;

    case "estimate_output": {
      const items = (args?.items ?? []) as { path: string; kind: string }[];
      const ratio: Record<string, number> = {
        video: 0.11,
        audio: 0.09,
        image: 0.07,
        document: 0,
        model: 0,
      };
      // A forced bitrate is the one override the real estimator takes at its
      // word rather than modelling, so the mock does the same: a screenshot
      // showing the panel open should not show numbers that ignore it.
      const forced = (args?.advanced as { videoKbps?: number | null } | undefined)?.videoKbps;
      return items.map((item) => {
        const seed = seedFor(item.path);
        if (forced && item.kind === "video" && seed.durationSecs) {
          return { path: item.path, bytes: Math.round((forced * 1000 * seed.durationSecs) / 8) };
        }
        return {
          path: item.path,
          bytes: ratio[item.kind] ? Math.round(seed.sizeBytes * ratio[item.kind]) : null,
        };
      }) as T;
    }

    case "setup_state":
      return setupState() as T;

    case "apply_setup": {
      const settings = args?.settings as Record<string, boolean>;
      await fakeDownload(settings);
      return { ...setupState(), settings: { ...settings, setupDone: true } } as T;
    }

    case "convert_files": {
      const items = (args?.request as { items: { path: string; kind: string; durationSecs: number | null }[] }).items;
      const created = items.map((item, index) => {
        const jobId = `job-${index}`;
        jobs.set(jobId, item);
        return {
          jobId,
          path: item.path,
          outputPath: item.path.replace(/\.[^.]+$/, ".out"),
        };
      });
      queueMicrotask(runConversion);
      return created as T;
    }

    case "thumbnail": {
      // Stand-in artwork: the real one is a frame pulled by ffmpeg, which the
      // preview has no way to run.
      const seed = SEEDS.findIndex((s) => (args?.path as string).endsWith(s.fileName));
      const hue = 200 + seed * 17;
      return svgUri(`<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="hsl(${hue} 45% 32%)"/>
        <stop offset="1" stop-color="hsl(${hue + 30} 40% 18%)"/>
      </linearGradient></defs><rect width="320" height="180" fill="url(#g)"/>`, 320, 180) as T;
    }

    case "scrub_strip": {
      // Forty visibly different frames, so dragging across one actually moves.
      const bands = Array.from({ length: 40 }, (_, i) =>
        `<rect x="${i * 160}" y="0" width="160" height="90" fill="hsl(${i * 9} 45% ${22 + (i % 5) * 6}%)"/>
         <text x="${i * 160 + 80}" y="52" font-size="28" fill="#fff" text-anchor="middle" opacity="0.65">${i + 1}</text>`,
      ).join("");
      return { dataUri: svgUri(bands, 6400, 90), frames: 40 } as T;
    }

    case "cancel_job":
    case "cancel_all":
      return [] as T;

    default:
      throw new Error(`preview mock has no answer for ${command}`);
  }
}

function setupState() {
  const configured: Scene[] = ["setup-configured"];
  return {
    settings: {
      setupDone: SCENE !== "setup",
      documents: configured.includes(SCENE),
      models: configured.includes(SCENE),
      blender: false,
      speech: false,
      ocr: false,
      tts: false,
      extraImages: false,
    },
    engines: [
      { id: "pandoc", label: "Pandoc", version: "3.10.2", installed: false, available: true, downloadBytes: 41_600_000 },
      { id: "typst", label: "Typst", version: "0.15.1", installed: false, available: true, downloadBytes: 22_400_000 },
      { id: "blender", label: "Blender", version: "4.5.9", installed: false, available: true, downloadBytes: 399_051_129 },
      { id: "whisper", label: "Whisper", version: "1.9.2", installed: false, available: true, downloadBytes: 8_200_000 },
      { id: "whisper-model", label: "Whisper model", version: "base", installed: false, available: true, downloadBytes: 147_951_465 },
      { id: "ocr-detection", label: "OCR detection model", version: "2024-05", installed: false, available: true, downloadBytes: 2_510_284 },
      { id: "ocr-recognition", label: "OCR recognition model", version: "2024-05", installed: false, available: true, downloadBytes: 9_716_568 },
      { id: "piper", label: "Piper", version: "2023.11.14-2", installed: false, available: true, downloadBytes: 22_400_000 },
      { id: "piper-voice", label: "Voice", version: "en_US-lessac-medium", installed: false, available: true, downloadBytes: 63_201_294 },
      { id: "imagemagick", label: "ImageMagick", version: "7.1.2-29", installed: false, available: true, downloadBytes: 11_682_401 },
    ],
    libreoffice: null,
  };
}

/** Enough of a download to make the setup progress bar look like itself. */
async function fakeDownload(settings: Record<string, boolean>) {
  if (!settings.documents) return;
  const total = 41_600_000;
  for (let received = 0; received <= total; received += total / 12) {
    emit("engine:progress", {
      engineId: "pandoc",
      label: "Pandoc",
      received: Math.round(received),
      total,
      phase: "download",
    });
    await sleep(90);
  }
  emit("engine:done", { engineId: "pandoc", label: "Pandoc", message: null });
}

/** Drives every queued job to completion, staggered so the rows do not move
 *  in lockstep. */
async function runConversion() {
  const entries = [...jobs.entries()];
  await Promise.all(
    entries.map(async ([jobId, item], index) => {
      await sleep(index * 220);
      const steps = 14 + (index % 5) * 3;
      for (let step = 1; step <= steps; step++) {
        const fraction = step / steps;
        emit("convert:progress", {
          jobId,
          fraction: item.kind === "image" || item.kind === "document" ? null : fraction,
          outBytes: Math.round(seedFor(item.path).sizeBytes * 0.11 * fraction),
          speed: `${(1.4 + (index % 4) * 0.6).toFixed(1)}x`,
          estimatedBytes: Math.round(seedFor(item.path).sizeBytes * 0.11),
        });
        await sleep(150);
      }
      emit("convert:done", {
        jobId,
        outputPath: item.path.replace(/\.[^.]+$/, ".out"),
        outputBytes: Math.round(seedFor(item.path).sizeBytes * 0.11),
        elapsedMs: 4200,
      });
    }),
  );
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const svgUri = (body: string, width: number, height: number) =>
  `data:image/svg+xml;utf8,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">${body}</svg>`,
  )}`;
