import { defineConfig } from "vite";

// Configurazione minima per Tauri: porta fissa, nessun clear dello schermo
// (così si vedono gli errori di Rust) e build per WebView2 (Chromium recente).
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "chrome110",
    minify: "esbuild",
    sourcemap: false,
  },
});
