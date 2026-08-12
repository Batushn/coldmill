import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const mock = (name: string) =>
  fileURLToPath(new URL(`./preview/mocks/${name}.ts`, import.meta.url));

/**
 * Serves the real UI in a plain browser by swapping the Tauri bridge for a
 * scripted mock. Used to capture the README screenshots and the demo GIF —
 * the components themselves are imported unmodified, so what you see is what
 * the app renders.
 */
export default defineConfig({
  root: fileURLToPath(new URL("./preview", import.meta.url)),
  plugins: [react()],
  clearScreen: false,
  resolve: {
    alias: {
      "@tauri-apps/api/core": mock("core"),
      "@tauri-apps/api/event": mock("event"),
      "@tauri-apps/api/webview": mock("webview"),
      "@tauri-apps/plugin-dialog": mock("dialog"),
      "@tauri-apps/plugin-opener": mock("opener"),
    },
  },
  server: { port: 1421, strictPort: true },
});
