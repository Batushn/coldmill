// Captures the README screenshots and the demo GIF from the real UI.
//
//   npm run capture
//
// Boots the preview server (real components, mocked Tauri bridge), drives it
// with headless Chrome and writes to docs/screenshots/. Needs Chrome or Edge
// installed — set CHROME_PATH if it lives somewhere unusual — and the ffmpeg
// sidecar for the GIF, so run scripts/fetch-ffmpeg.sh first.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import puppeteer from "puppeteer-core";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "docs", "screenshots");
const FRAMES = join(ROOT, "docs", ".frames");
const PORT = 1421;

// The app's default window, so a screenshot matches what people will see.
const VIEWPORT = { width: 920, height: 680 };

const SHOTS = [
  { name: "empty", scene: "empty" },
  { name: "queue", scene: "queue" },
  { name: "converting", scene: "running" },
  { name: "done", scene: "done" },
  { name: "setup", scene: "setup" },
  { name: "languages", scene: "languages" },
  { name: "update", scene: "update" },
  { name: "locale-turkish", scene: "queue", lang: "tr" },
  { name: "locale-arabic", scene: "queue", lang: "ar" },
];

const GIF = { scene: "demo", seconds: 13, fps: 8, width: 760 };

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

function findChrome() {
  const candidates = [
    process.env.CHROME_PATH,
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
    "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ].filter(Boolean);
  const found = candidates.find((path) => existsSync(path));
  if (!found) throw new Error("no Chrome found — set CHROME_PATH");
  return found;
}

function findFfmpeg() {
  const dir = join(ROOT, "src-tauri", "binaries");
  const name = readdirSync(dir).find((file) => file.startsWith("ffmpeg-"));
  if (!name) throw new Error("no ffmpeg sidecar — run scripts/fetch-ffmpeg.sh");
  return join(dir, name);
}

async function startPreview() {
  const server = spawn(
    process.platform === "win32" ? "npx.cmd" : "npx",
    ["vite", "--config", "vite.preview.config.ts"],
    { cwd: ROOT, stdio: "ignore", shell: process.platform === "win32" },
  );

  for (let attempt = 0; attempt < 60; attempt++) {
    try {
      const response = await fetch(`http://localhost:${PORT}/`);
      if (response.ok) return server;
    } catch {
      // not up yet
    }
    await sleep(500);
  }
  server.kill();
  throw new Error("preview server never came up");
}

const url = ({ scene, lang }) =>
  `http://localhost:${PORT}/?scene=${scene}${lang ? `&lang=${lang}` : ""}`;

/** Every scene flags itself ready once it has settled. */
const waitForScene = (page, timeout) =>
  page.waitForSelector("body[data-scene-ready]", { timeout });

async function main() {
  mkdirSync(OUT, { recursive: true });
  rmSync(FRAMES, { recursive: true, force: true });
  mkdirSync(FRAMES, { recursive: true });

  const server = await startPreview();
  const browser = await puppeteer.launch({
    executablePath: findChrome(),
    headless: "new",
    args: ["--force-color-profile=srgb", "--hide-scrollbars"],
  });

  try {
    for (const shot of SHOTS) {
      const page = await browser.newPage();
      await page.setViewport({ ...VIEWPORT, deviceScaleFactor: 2 });
      await page.goto(url(shot), { waitUntil: "networkidle0" });
      await waitForScene(page, 20_000);
      // One more beat so the last progress bar has painted.
      await sleep(400);
      await page.screenshot({ path: join(OUT, `${shot.name}.png`) });
      await page.close();
      console.log(`✓ ${shot.name}.png`);
    }

    // The GIF is a straight time-lapse of one uninterrupted run.
    const page = await browser.newPage();
    await page.setViewport({ ...VIEWPORT, deviceScaleFactor: 1 });
    await page.goto(url(GIF), { waitUntil: "networkidle0" });

    const total = GIF.seconds * GIF.fps;
    const interval = 1000 / GIF.fps;
    for (let frame = 0; frame < total; frame++) {
      const started = Date.now();
      const buffer = await page.screenshot({ type: "png" });
      writeFileSync(join(FRAMES, `f${String(frame).padStart(4, "0")}.png`), buffer);
      const spent = Date.now() - started;
      if (spent < interval) await sleep(interval - spent);
    }
    await page.close();
    console.log(`✓ ${total} frames`);
  } finally {
    await browser.close();
    server.kill();
  }

  const filter =
    `fps=${GIF.fps},scale=${GIF.width}:-1:flags=lanczos,split[a][b];` +
    `[a]palettegen=max_colors=128[p];[b][p]paletteuse=dither=bayer`;

  await new Promise((done, fail) => {
    const ffmpeg = spawn(
      findFfmpeg(),
      [
        "-y", "-hide_banner", "-loglevel", "error",
        "-framerate", String(GIF.fps),
        "-i", join(FRAMES, "f%04d.png"),
        "-vf", filter,
        "-loop", "0",
        join(OUT, "demo.gif"),
      ],
      { stdio: "inherit" },
    );
    ffmpeg.on("exit", (code) => (code === 0 ? done() : fail(new Error(`ffmpeg exited ${code}`))));
  });

  rmSync(FRAMES, { recursive: true, force: true });
  console.log("✓ demo.gif");
}

await main();
