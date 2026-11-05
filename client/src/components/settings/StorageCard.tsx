import { useEffect, useState } from "react";
import packageJson from "../../../package.json";
import { useEngine } from "../../hooks/useEngine";
import { useTranslation } from "../../i18n";
import { createLogger } from "../../lib/log";
import { requestPersistence } from "../../lib/store";
import { formatBytes } from "./settingsModel";

const log = createLogger("settings.storage");

/** Where the format specification is published. */
export const SPEC_URL = "https://github.com/justin13888/crisp/tree/master/spec";

/** The storage usage the browser reports. */
interface Usage {
  usage: number;
  quota: number;
}

/** The storage & app card: durability, usage, and versions. */
export function StorageCard() {
  const { t } = useTranslation("settings");
  const { engine, error: engineError } = useEngine();
  const [persisted, setPersisted] = useState<boolean | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [asking, setAsking] = useState(false);

  useEffect(() => {
    const storage = globalThis.navigator?.storage;
    if (storage?.persisted) {
      void storage.persisted().then(setPersisted, () => setPersisted(false));
    } else {
      setPersisted(false);
    }
    if (storage?.estimate) {
      void storage.estimate().then(
        (estimate) =>
          setUsage({
            usage: estimate.usage ?? 0,
            quota: estimate.quota ?? 0,
          }),
        (error: unknown) =>
          log.warn("estimate.failed", { error: String(error) }),
      );
    }
  }, []);

  const ask = async () => {
    setAsking(true);
    const granted = await requestPersistence();
    setPersisted(granted);
    setAsking(false);
  };

  const core = engine?.coreInfo();
  const percent =
    usage && usage.quota > 0
      ? Math.min(100, (usage.usage / usage.quota) * 100)
      : 0;

  return (
    <section className="card settings-card" aria-labelledby="storage-title">
      <h2 id="storage-title">{t("storage.title")}</h2>

      <div className="spread settings__row">
        <div>
          <div className="identity__label">{t("storage.keepLibrary")}</div>
          <p className="muted small">
            {persisted ? t("storage.persisted") : t("storage.notPersisted")}
          </p>
        </div>
        {persisted ? (
          <span className="badge ok">{t("storage.persistent")}</span>
        ) : (
          <button
            type="button"
            className="secondary"
            onClick={() => void ask()}
            disabled={asking || persisted === null}
          >
            {t("storage.keep")}
          </button>
        )}
      </div>

      <div className="settings__row">
        <div className="identity__label">{t("storage.spaceUsed")}</div>
        {usage ? (
          <>
            <div className="meter" aria-hidden="true">
              <span style={{ width: `${Math.max(percent, 1)}%` }} />
            </div>
            <p className="muted small tabular">
              {t("storage.usage", {
                used: formatBytes(usage.usage),
                quota: formatBytes(usage.quota),
              })}
            </p>
          </>
        ) : (
          <p className="muted small">{t("storage.notReported")}</p>
        )}
      </div>

      <dl className="facts">
        <div>
          <dt>{t("storage.app")}</dt>
          <dd className="tabular">{packageJson.version}</dd>
        </div>
        <div>
          <dt>{t("storage.core")}</dt>
          <dd className="tabular">
            {core
              ? core.core_version
              : engineError
                ? t("storage.unavailable")
                : "…"}
          </dd>
        </div>
        <div>
          <dt>{t("storage.format")}</dt>
          <dd className="tabular">
            {core
              ? t("version.reads", {
                  version: core.format_version_text,
                  min: core.min_reader_version_text,
                })
              : "…"}
          </dd>
        </div>
      </dl>
      <p>
        <a href={SPEC_URL} target="_blank" rel="noreferrer">
          {t("storage.spec")}
        </a>
      </p>
    </section>
  );
}
