import { useEffect, useState } from "react";
import packageJson from "../../../package.json";
import { useEngine } from "../../hooks/useEngine";
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
      <h2 id="storage-title">Storage &amp; app</h2>

      <div className="spread settings__row">
        <div>
          <div className="identity__label">Keep my library</div>
          <p className="muted small">
            {persisted
              ? "Stored persistently — the browser won’t clear it to free space."
              : "Ask the browser not to clear your recipes when space runs low."}
          </p>
        </div>
        {persisted ? (
          <span className="badge ok">Persistent</span>
        ) : (
          <button
            type="button"
            className="secondary"
            onClick={() => void ask()}
            disabled={asking || persisted === null}
          >
            Keep
          </button>
        )}
      </div>

      <div className="settings__row">
        <div className="identity__label">Space used</div>
        {usage ? (
          <>
            <div className="meter" aria-hidden="true">
              <span style={{ width: `${Math.max(percent, 1)}%` }} />
            </div>
            <p className="muted small tabular">
              {formatBytes(usage.usage)} of {formatBytes(usage.quota)} available
            </p>
          </>
        ) : (
          <p className="muted small">Not reported by this browser.</p>
        )}
      </div>

      <dl className="facts">
        <div>
          <dt>App</dt>
          <dd className="tabular">{packageJson.version}</dd>
        </div>
        <div>
          <dt>Crisp core</dt>
          <dd className="tabular">
            {core ? core.core_version : engineError ? "unavailable" : "…"}
          </dd>
        </div>
        <div>
          <dt>Format</dt>
          <dd className="tabular">
            {core
              ? `${core.format_version_text} (reads ${core.min_reader_version_text}+)`
              : "…"}
          </dd>
        </div>
      </dl>
      <p>
        <a href={SPEC_URL} target="_blank" rel="noreferrer">
          Read the Crisp format specification
        </a>
      </p>
    </section>
  );
}
