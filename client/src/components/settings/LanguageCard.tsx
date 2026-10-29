import { useState } from "react";
import {
  LOCALE_NAMES,
  type Locale,
  SUPPORTED_LOCALES,
  setLocale,
  storedLocale,
  useTranslation,
} from "../../i18n";

/** The `<select>` value that means "follow the system's languages". */
const SYSTEM = "system";

/** The language card: which language the app's interface uses. */
export function LanguageCard() {
  const { t } = useTranslation("settings");
  const [choice, setChoice] = useState<Locale | null>(storedLocale);

  const change = (value: string) => {
    const locale = SUPPORTED_LOCALES.find((l) => l === value) ?? null;
    setChoice(locale);
    setLocale(locale);
  };

  return (
    <section className="card settings-card" aria-labelledby="language-title">
      <h2 id="language-title">{t("language.title")}</h2>
      <label className="settings__row">
        <span className="identity__label">{t("language.label")}</span>
        <select
          value={choice ?? SYSTEM}
          onChange={(event) => change(event.target.value)}
        >
          <option value={SYSTEM}>{t("language.system")}</option>
          {SUPPORTED_LOCALES.map((locale) => (
            <option key={locale} value={locale} lang={locale}>
              {LOCALE_NAMES[locale]}
            </option>
          ))}
        </select>
      </label>
      <p className="muted small">{t("language.hint")}</p>
    </section>
  );
}
