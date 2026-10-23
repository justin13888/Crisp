import "./library.css";
import "./settings.css";
import { IdentityCard } from "../components/settings/IdentityCard";
import { ServerCard } from "../components/settings/ServerCard";
import { StorageCard } from "../components/settings/StorageCard";

/** Identity, server connection, and storage. */
export function Settings() {
  return (
    <div className="settings">
      <h1>Settings</h1>
      <IdentityCard />
      <ServerCard />
      <StorageCard />
    </div>
  );
}
