import { useServiceWorkerUpdate } from "../hooks/useServiceWorkerUpdate";
import { useTranslation } from "../i18n";

/** A non-blocking "update available — reload" bar. */
export function UpdatePrompt() {
  const { t } = useTranslation();
  const { updateAvailable, applyUpdate } = useServiceWorkerUpdate();
  if (!updateAvailable) return null;
  return (
    <div className="toast" role="status">
      <span>{t("update.ready")}</span>
      <button type="button" onClick={applyUpdate}>
        {t("update.reload")}
      </button>
    </div>
  );
}
