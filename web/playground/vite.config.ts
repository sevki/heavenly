import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// `vite build` writes one module, app.js, plus the WebAssembly file, into the
// book (book/src/playground/), where playground.md loads it. `vite` serves
// index.html here for development.
export default defineConfig({
  plugins: [react()],
  base: "./",
  build: {
    outDir: "../../book/src/playground",
    emptyOutDir: false, // the coastlines (land-110m.json) live there too
    assetsDir: "",
    sourcemap: false,
    rollupOptions: {
      input: "src/main.tsx",
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "[name].js",
        assetFileNames: "[name][extname]",
      },
    },
  },
});
