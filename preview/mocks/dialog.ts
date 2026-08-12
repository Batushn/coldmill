import { DEMO_PATHS } from "./core";

/** No OS dialogs in a headless browser: "browse" just adds the demo set. */
export async function open(options?: { directory?: boolean }) {
  if (options?.directory) return "C:\\Users\\you\\Converted";
  return DEMO_PATHS;
}
