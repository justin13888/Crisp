import { useServiceWorkerUpdate } from "../hooks/useServiceWorkerUpdate";

/** A non-blocking "update available — reload" bar. */
export function UpdatePrompt() {
  const { updateAvailable, applyUpdate } = useServiceWorkerUpdate();
  if (!updateAvailable) return null;
  return (
    <div className="toast" role="status">
      <span>A new version of Crisp is ready.</span>
      <button type="button" onClick={applyUpdate}>
        Reload
      </button>
    </div>
  );
}
