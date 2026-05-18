import { defineConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";

// The Crisp web client. All format logic lives in the `crisp-core` WASM
// module under `public/crisp-wasm/` (built by `just wasm`); this config only
// builds the thin React UI shell around it.
export default defineConfig({
  plugins: [pluginReact()],
  source: {
    entry: { index: "./src/index.tsx" },
  },
  html: {
    template: "./src/index.html",
  },
  output: {
    distPath: { root: "dist" },
    // The service worker and PWA manifest are served verbatim from `public/`.
  },
  server: {
    port: 3000,
  },
});
