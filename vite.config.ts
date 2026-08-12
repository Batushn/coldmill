import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives the dev server; the fixed port and strictPort are required so the
// Rust side can point the webview at a known URL.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
    sourcemap: false,
  },
});
