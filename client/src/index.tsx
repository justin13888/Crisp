import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { registerServiceWorker } from "./lib/swRegistration";
import "./index.css";

// Register the generated service worker right away so the precache starts
// filling during the first visit. Production only: the dev server has no
// `sw.js`, and a worker would cache stale dev bundles. Failure is non-fatal.
if (import.meta.env.PROD) {
  void registerServiceWorker();
}

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
