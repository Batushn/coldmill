import { DEMO_PATHS } from "./core";
import { emit } from "./event";

export type Scene =
  | "empty"
  | "queue"
  | "running"
  | "done"
  | "setup"
  | "setup-configured"
  | "languages"
  | "demo";

const params = new URLSearchParams(location.search);

export const SCENE = (params.get("scene") ?? "queue") as Scene;
export const LOCALE = params.get("lang");

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Clicks Convert by role, not by label — the label is translated. */
const clickConvert = () => {
  document.querySelector<HTMLButtonElement>("footer .primary")?.click();
};

const dropDemoFiles = () => emit("preview:dragdrop", { type: "drop", paths: DEMO_PATHS });

/** Marks the page as settled so the capture script knows when to shoot. */
const ready = () => document.body.setAttribute("data-scene-ready", "1");

/**
 * Drives the UI into the state a screenshot should show. The app itself is
 * untouched — everything here goes through the same events and clicks a user
 * would produce.
 */
export async function runScene() {
  // React has to mount and subscribe before an event has anywhere to land.
  await sleep(400);

  switch (SCENE) {
    case "empty":
    case "setup":
    case "setup-configured":
      return ready();

    case "queue":
      dropDemoFiles();
      await sleep(700);
      return ready();

    case "languages":
      dropDemoFiles();
      await sleep(700);
      // Opens the language menu, which is a real element now rather than a
      // platform-drawn popup — so it lands in the screenshot.
      document.querySelector<HTMLButtonElement>(".langpicker .dropdown-trigger")?.click();
      await sleep(300);
      return ready();

    case "running":
      dropDemoFiles();
      await sleep(700);
      clickConvert();
      await sleep(1600);
      return ready();

    case "done":
      dropDemoFiles();
      await sleep(700);
      clickConvert();
      // Long enough for every staggered job to report done.
      await sleep(9000);
      return ready();

    case "demo": {
      // The recorded sequence: land on the empty state, drop a batch,
      // convert, finish.
      await sleep(1200);
      dropDemoFiles();
      await sleep(2000);
      clickConvert();
      await sleep(9000);
      return ready();
    }
  }
}
