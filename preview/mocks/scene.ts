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
  | "grid"
  | "scrub"
  | "edit"
  | "update"
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

    case "update":
      dropDemoFiles();
      // The updater mock answers on its own; this just waits for the banner.
      await sleep(900);
      return ready();

    case "grid":
      dropDemoFiles();
      await sleep(900);
      return ready();

    case "scrub": {
      dropDemoFiles();
      await sleep(900);
      // Walk the pointer onto a video preview and part-way across it, which
      // is what makes the filmstrip load and slide.
      const thumb = document.querySelector<HTMLElement>(".row .thumb.is-scrubbable");
      if (thumb) {
        const box = thumb.getBoundingClientRect();
        const at = (x: number) => ({
          clientX: box.left + box.width * x,
          clientY: box.top + box.height / 2,
          bubbles: true,
        });
        thumb.dispatchEvent(new PointerEvent("pointerenter", at(0.1)));
        await sleep(500);
        thumb.dispatchEvent(new PointerEvent("pointermove", at(0.62)));
      }
      await sleep(400);
      return ready();
    }

    case "edit": {
      dropDemoFiles();
      await sleep(900);
      // Open the panel on the first video, then pull the start handle in.
      const chips = [...document.querySelectorAll<HTMLButtonElement>(".row .chip")];
      chips[0]?.click();
      await sleep(700);

      const handle = document.querySelector<HTMLElement>(".edittrack-handle.is-start");
      const track = document.querySelector<HTMLElement>(".edittrack");
      if (handle && track) {
        const box = track.getBoundingClientRect();
        handle.dispatchEvent(
          new PointerEvent("pointerdown", { bubbles: true, clientX: box.left }),
        );
        // The drag listeners are attached by an effect, so they do not exist
        // until React has re-rendered.
        await sleep(120);
        window.dispatchEvent(
          new PointerEvent("pointermove", {
            bubbles: true,
            clientX: box.left + box.width * 0.22,
            clientY: box.top + box.height / 2,
          }),
        );
        window.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
      }
      await sleep(500);
      return ready();
    }

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
