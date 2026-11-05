import "./library.css";
import "./settings.css";
import { IdentityCard } from "../components/settings/IdentityCard";
import { LanguageCard } from "../components/settings/LanguageCard";
import { ServerCard } from "../components/settings/ServerCard";
import { StorageCard } from "../components/settings/StorageCard";
import { useTranslation } from "../i18n";

/** Identity, server connection, language, and storage. */
export function Settings() {
  const { t } = useTranslation("settings");
  return (
    <div className="settings">
      <h1>{t("title")}</h1>
      <IdentityCard />
      <ServerCard />
      <LanguageCard />
      <StorageCard />
    </div>
  );
}
