import { defineConfig, type RsbuildConfig } from "@rsbuild/core";
import { pluginReact } from "@rsbuild/plugin-react";
import { pluginServiceWorker } from "./plugins/sw";

/**
 * The Content-Security-Policy the client runs under (issue #92). Scripts,
 * styles, workers, and the WASM engine come only from the app's own origin
 * (`'wasm-unsafe-eval'` lets `WebAssembly.instantiate` compile it); images
 * also from the `blob:`/`data:` URLs bundle images are shown through; and
 * network requests go to the origin, to `https:` Crisp servers the cook
 * adds, and to a server on this machine (the default `localhost:8080`).
 * Hosts serving `dist/` send it as a header with {@link HEADER_ONLY} added;
 * `docs/deployment.md` has the reference configuration.
 */
export const CONTENT_SECURITY_POLICY: readonly string[] = [
  "default-src 'self'",
  "script-src 'self' 'wasm-unsafe-eval'",
  "style-src 'self'",
  "img-src 'self' blob: data:",
  "font-src 'self'",
  "connect-src 'self' https: http://localhost:* http://127.0.0.1:*",
  "worker-src 'self'",
  "manifest-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'self'",
];

/** Directives a browser ignores in a `<meta>` policy; only a header carries them. */
export const HEADER_ONLY: readonly string[] = ["frame-ancestors 'none'"];

// The Crisp web client. All format logic lives in the `crisp-core` WASM
// module under `public/crisp-wasm/` (built by `just wasm`); this config only
// builds the thin React UI shell around it.
export default defineConfig(({ env }): RsbuildConfig => {
  // Production output carries the policy in a <meta> tag, so it holds on any
  // static host; `rsbuild preview` also sends the full header, as a host
  // should. The dev server stays unrestricted for HMR and its error overlay.
  const production = env === "production";
  return {
    plugins: [pluginReact(), pluginServiceWorker()],
    source: {
      entry: { index: "./src/index.tsx" },
    },
    html: {
      template: "./src/index.html",
      meta: production
        ? {
            csp: {
              "http-equiv": "Content-Security-Policy",
              content: CONTENT_SECURITY_POLICY.join("; "),
            },
          }
        : {},
    },
    output: {
      distPath: { root: "dist" },
      // The PWA manifest and icons are served verbatim from `public/`; the
      // service worker is generated into `dist/sw.js` by `plugins/sw.ts`.
    },
    server: {
      port: 3000,
      headers: production
        ? {
            "Content-Security-Policy": [
              ...CONTENT_SECURITY_POLICY,
              ...HEADER_ONLY,
            ].join("; "),
          }
        : {},
    },
  };
});
