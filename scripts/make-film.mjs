// Builds the product film from the screenshots the capture script produces.
//
//   npm run film            (run npm run capture first)
//
// Every frame of art direction lives in HTML and CSS here, laid out at
// 1920x1080 and photographed once per beat. ffmpeg only does what it is
// actually good at: a slow push on each still and a dissolve between them.
// Nothing is drawn with drawtext, so the typography is the same engine that
// renders the app.
//
// Writes docs/film.mp4.
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import puppeteer from "puppeteer-core";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SHOTS = join(ROOT, "docs", "screenshots");
const OUT = join(ROOT, "docs");
const WORK = join(ROOT, "docs", ".film");

const WIDTH = 1920;
const HEIGHT = 1080;
const FPS = 30;
/** Seconds a beat holds before the dissolve into the next one. */
const HOLD = 3.4;
const FADE = 0.7;

// Short, declarative, one idea each. A caption that needs reading twice is a
// caption the viewer misses entirely, because the next beat has already
// started.
const BEATS = [
  {
    kind: "title",
    title: "Coldmill",
    body: "Convert anything. On your own machine.",
  },
  {
    shot: "queue.png",
    title: "Drop anything in",
    body: "Audio, video, images, documents, 3D models — sorted by what they are, not what they are called.",
  },
  {
    shot: "converting.png",
    title: "All of it at once",
    body: "One job per processor core, each with its own progress and its own cancel.",
  },
  {
    shot: "edit.png",
    title: "Trim, split, reframe",
    body: "On the filmstrip, before anything is encoded.",
  },
  {
    shot: "colour.png",
    title: "Grade a picture",
    body: "Brightness, contrast, saturation and hue — on photos and video alike.",
  },
  {
    shot: "model.png",
    title: "Reduce a mesh",
    body: "Quality decides how many triangles survive, and the row says how many that will be.",
  },
  {
    shot: "advanced.png",
    title: "Or take the wheel",
    body: "Bitrate, CRF, frame rate, resolution — for anyone who wants them, hidden from everyone who does not.",
  },
  {
    shot: "languages.png",
    title: "Sixteen languages",
    body: "Right-to-left included, laid out properly rather than mirrored.",
  },
  {
    shot: "done.png",
    title: "Nothing leaves the machine",
    body: "No cloud, no account, no telemetry. It works with the network unplugged.",
  },
  {
    kind: "title",
    title: "Coldmill",
    body: "Free and open source · GPL-3.0 · github.com/Batushn/coldmill",
  },
];

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

const run = (program, args) =>
  new Promise((done, fail) => {
    const child = spawn(program, args, { stdio: ["ignore", "ignore", "pipe"] });
    let stderr = "";
    child.stderr.on("data", (chunk) => (stderr += chunk));
    child.on("exit", (code) =>
      code === 0 ? done() : fail(new Error(`${program} exited ${code}\n${stderr.slice(-1500)}`)),
    );
  });

/** The screenshot as a data URI, so the page needs no server to load it. */
function inlineShot(name) {
  const bytes = readFileSync(join(SHOTS, name));
  return `data:image/png;base64,${bytes.toString("base64")}`;
}

function page(beat) {
  const isTitle = beat.kind === "title";
  return `<!doctype html>
<meta charset="utf-8">
<style>
  /* The app's own palette, a shade darker so the window reads as lit. */
  :root {
    --bg: #0e0f12;
    --text: #e7e8ea;
    --muted: #8f949e;
    --accent: #4d7cff;
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  html, body {
    width: ${WIDTH}px; height: ${HEIGHT}px;
    background: var(--bg);
    color: var(--text);
    font-family: "Segoe UI", system-ui, -apple-system, sans-serif;
    overflow: hidden;
  }
  /* A wide, soft pool of light behind the subject. Without it a dark app on
     a dark ground looks like a mistake rather than a choice. */
  body::before {
    content: "";
    position: absolute; inset: -20%;
    background: radial-gradient(50% 45% at 50% 38%, #1b2338 0%, transparent 70%);
  }
  .stage {
    position: relative;
    height: 100%;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: ${isTitle ? "26px" : "30px"};
    padding: 48px;
  }
  .title {
    font-size: ${isTitle ? "108px" : "50px"};
    font-weight: ${isTitle ? "700" : "600"};
    letter-spacing: ${isTitle ? "-3px" : "-1.2px"};
    line-height: 1.05;
    text-align: center;
  }
  .body {
    font-size: ${isTitle ? "30px" : "23px"};
    color: var(--muted);
    text-align: center;
    max-width: 1180px;
    line-height: 1.45;
  }
  .shotwrap {
    /* Room for the shadow to fall without being clipped. */
    padding: 0 40px 30px;
  }
  .shot {
    display: block;
    /* Sized so the caption above and below it still fit inside 1080: the
       screenshots are taller than they are wide once scaled, and at any more
       than this the page overflows and the words are what get cut. */
    width: 980px;
    border-radius: 14px;
    border: 1px solid #2c2f35;
    box-shadow: 0 40px 90px rgb(0 0 0 / 65%), 0 8px 24px rgb(0 0 0 / 45%);
  }
  .mark {
    display: flex; align-items: center; gap: 22px;
  }
  /* The app icon, drawn rather than loaded: two chevrons, in becomes out. */
  .chev { width: 96px; height: 96px; }
</style>
<div class="stage">
${
  isTitle
    ? `  <div class="mark">
    <svg class="chev" viewBox="0 0 100 100" fill="none" stroke="var(--accent)" stroke-width="11" stroke-linecap="round" stroke-linejoin="round">
      <path d="M22 28 L46 50 L22 72" />
      <path d="M54 28 L78 50 L54 72" />
    </svg>
    <div class="title">${beat.title}</div>
  </div>
  <div class="body">${beat.body}</div>`
    : `  <div class="title">${beat.title}</div>
  <div class="shotwrap"><img class="shot" src="${inlineShot(beat.shot)}"></div>
  <div class="body">${beat.body}</div>`
}
</div>`;
}

async function main() {
  if (!existsSync(join(SHOTS, "queue.png"))) {
    throw new Error("no screenshots yet — run npm run capture first");
  }
  rmSync(WORK, { recursive: true, force: true });
  mkdirSync(WORK, { recursive: true });

  const browser = await puppeteer.launch({
    executablePath: findChrome(),
    headless: "new",
    args: ["--force-color-profile=srgb", "--hide-scrollbars"],
  });

  const cards = [];
  try {
    const tab = await browser.newPage();
    await tab.setViewport({ width: WIDTH, height: HEIGHT, deviceScaleFactor: 1 });
    for (const [index, beat] of BEATS.entries()) {
      const file = join(WORK, `card${String(index).padStart(2, "0")}.png`);
      await tab.setContent(page(beat), { waitUntil: "load" });
      // Fonts settle a frame late; a still photographed mid-swap is the one
      // frame anybody notices.
      await tab.evaluate(() => document.fonts.ready);
      await sleep(150);
      await tab.screenshot({ path: file });
      cards.push(file);
      console.log(`✓ card ${index + 1}/${BEATS.length} — ${beat.title}`);
    }
  } finally {
    await browser.close();
  }

  const ffmpeg = findFfmpeg();

  // Each card becomes a clip with a slow push. zoompan works per output
  // frame, so the duration is in frames, and the still is scaled up first:
  // zooming a 1920-wide source is what makes zoompan shimmer.
  const clipFrames = Math.round(HOLD * FPS);
  const clips = [];
  for (const [index, card] of cards.entries()) {
    const clip = join(WORK, `clip${String(index).padStart(2, "0")}.mp4`);
    const zoom = `min(1.06, 1 + on/${clipFrames * 18})`;
    await run(ffmpeg, [
      "-y", "-hide_banner", "-loglevel", "error",
      "-loop", "1", "-i", card,
      "-vf",
      // zoompan can only zoom in, never out: its output window is cut from
      // the input at scale z, so a 3840-wide source with z=1 shows the middle
      // quarter and nothing else — which is how the first cut lost every
      // caption. The card is enlarged, panned at its own size, and reduced
      // afterwards; the detour is what keeps the push from shimmering.
      `scale=${Math.round(WIDTH * 1.35)}:${Math.round(HEIGHT * 1.35)}:flags=lanczos,` +
        `zoompan=z='${zoom}':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':` +
        `d=${clipFrames}:s=${Math.round(WIDTH * 1.35)}x${Math.round(HEIGHT * 1.35)}:fps=${FPS},` +
        `scale=${WIDTH}:${HEIGHT}:flags=lanczos,format=yuv420p`,
      // The still is endless, so the cut has to come from the output side.
      "-frames:v", String(clipFrames),
      // Intermediate: quality that survives one more pass, encoded quickly.
      // Every dissolve re-encodes everything before it, so a slow preset here
      // is paid for nine times over.
      "-c:v", "libx264", "-preset", "veryfast", "-crf", "16",
      clip,
    ]);
    clips.push(clip);
    console.log(`✓ clip ${index + 1}/${cards.length}`);
  }

  // Dissolve the clips together, one pair at a time. A single filter_complex
  // with nine chained xfades is one typo away from unreadable, and this way a
  // failure says which join broke.
  let current = clips[0];
  for (let index = 1; index < clips.length; index++) {
    const merged = join(WORK, `merge${String(index).padStart(2, "0")}.mp4`);
    const offset = (HOLD - FADE) * index + (HOLD - FADE) * 0;
    await run(ffmpeg, [
      "-y", "-hide_banner", "-loglevel", "error",
      "-i", current, "-i", clips[index],
      "-filter_complex",
      `[0][1]xfade=transition=fade:duration=${FADE}:offset=${(
        HOLD * index -
        FADE * index
      ).toFixed(3)},format=yuv420p`,
      "-c:v", "libx264", "-preset", "veryfast", "-crf", "16",
      merged,
    ]);
    current = merged;
    void offset;
  }

  const film = join(OUT, "film.mp4");
  await run(ffmpeg, [
    "-y", "-hide_banner", "-loglevel", "error",
    "-i", current,
    // faststart so it begins playing while the rest arrives.
    "-c:v", "libx264", "-preset", "slow", "-crf", "19",
    "-pix_fmt", "yuv420p", "-movflags", "+faststart",
    film,
  ]);
  console.log("✓ docs/film.mp4");

  rmSync(WORK, { recursive: true, force: true });
}

await main();
