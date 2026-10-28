import i18next, {
  type i18n as I18n,
  type Resource,
  type ThirdPartyModule,
} from "i18next";
import { createLogger } from "../lib/log";
import { NAMESPACES } from "./catalogue";
import { en } from "./en";
import { fr } from "./fr";
import { FALLBACK_LOCALE, type Locale, SUPPORTED_LOCALES } from "./locales";

const log = createLogger("i18n");

/** Every shipped catalogue, keyed by locale. */
export const RESOURCES = { en, fr } satisfies Record<Locale, unknown>;

/**
 * A ready i18next instance for one locale.
 *
 * Initialisation is synchronous (the catalogues are bundled), so `t()`
 * works as soon as this returns. A key the locale lacks resolves from
 * English, and a key English lacks renders as the key itself.
 */
export function createI18n(
  locale: Locale,
  resources: Resource = RESOURCES,
  plugins: readonly ThirdPartyModule[] = [],
): I18n {
  const instance = i18next.createInstance();
  for (const plugin of plugins) instance.use(plugin);
  void instance.init({
    lng: locale,
    fallbackLng: FALLBACK_LOCALE,
    supportedLngs: [...SUPPORTED_LOCALES],
    resources,
    ns: [...NAMESPACES],
    defaultNS: "common",
    initAsync: false,
    // React escapes rendered text already.
    interpolation: { escapeValue: false },
    returnNull: false,
  });
  instance.on("languageChanged", (lng: string) =>
    log.info("locale.changed", { locale: lng }),
  );
  log.debug("init", { locale, fallback: FALLBACK_LOCALE });
  return instance;
}
