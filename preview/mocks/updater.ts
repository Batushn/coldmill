import { SCENE } from "./scene";

type ProgressEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished"; data: Record<string, never> };

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Only the `update` scene pretends a new version exists. */
export async function check() {
  if (SCENE !== "update") return null;
  return {
    version: "0.2.0",
    async downloadAndInstall(onEvent: (event: ProgressEvent) => void) {
      const total = 18_400_000;
      onEvent({ event: "Started", data: { contentLength: total } });
      for (let sent = 0; sent < total; sent += total / 20) {
        onEvent({ event: "Progress", data: { chunkLength: total / 20 } });
        await sleep(120);
      }
      onEvent({ event: "Finished", data: {} });
    },
  };
}

export type Update = NonNullable<Awaited<ReturnType<typeof check>>>;
