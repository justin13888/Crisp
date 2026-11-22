import { useTranslation } from "../../i18n";
import { setDerivedMeasures, useDerivedMeasures } from "./derivedMeasures";

/** The reading card: how recipes are shown to this reader. */
export function ReadingCard() {
  const { t } = useTranslation("settings");
  const derived = useDerivedMeasures();
  return (
    <section className="card settings-card" aria-labelledby="reading-title">
      <h2 id="reading-title">{t("reading.title")}</h2>
      <label className="settings__row">
        <input
          type="checkbox"
          checked={derived}
          onChange={(event) => setDerivedMeasures(event.target.checked)}
        />{" "}
        {t("reading.derived")}
      </label>
      <p className="muted small">{t("reading.derivedHint")}</p>
    </section>
  );
}
