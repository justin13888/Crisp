import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";
import { pluginServiceWorker } from "./plugins/sw";

// The Crisp web client. All format logic lives in the `crisp-core` WASM
// module under `public/crisp-wasm/` (built by `just wasm`); this config only
// builds the thin React UI shell around it.
export default defineConfig({
  plugins: [pluginReact(), pluginServiceWorker()],
  source: {
    entry: { index: "./src/index.tsx" },
  },
  html: {
    template: "./src/index.html",
  },
  output: {
    distPath: { root: "dist" },
    // The PWA manifest and icons are served verbatim from `public/`; the
    // service worker is generated into `dist/sw.js` by `plugins/sw.ts`.
  },
  server: {
    port: 3000,
  },
});
