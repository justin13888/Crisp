import { useEffect, useState } from "react";
import type { AccountUsageView } from "../../generated/crisp";
import { errorMessage } from "../../lib/crisp";
import { createLogger } from "../../lib/log";
import { useServer } from "../../lib/servers";
import { usageSummary } from "./settingsModel";

const log = createLogger("settings.usage");

/** What the signed-in account stores on the server, against its quotas. */
export function ServerUsage() {
  const server = useServer();
  const [usage, setUsage] = useState<AccountUsageView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const signedIn = server.status === "online" && server.username !== null;
  const read = server.usage;

  useEffect(() => {
    setUsage(null);
    setError(null);
    if (!signedIn) return;
    let current = true;
    read().then(
      (next) => {
        if (current) setUsage(next);
      },
      (cause: unknown) => {
        log.warn("usage.failed", { error: errorMessage(cause) });
        if (current) setError(errorMessage(cause));
      },
    );
    return () => {
      current = false;
    };
  }, [read, signedIn]);

  if (!signedIn) return null;
  if (error) {
    return (
      <p className="muted" data-testid="server-usage">
        Couldn’t read your storage on this server ({error}).
      </p>
    );
  }
  if (!usage) return null;
  const summary = usageSummary(usage);
  return (
    <div data-testid="server-usage">
      <dl className="facts">
        <div>
          <dt>Published storage</dt>
          <dd className="tabular">{summary.bytes}</dd>
        </div>
        <div>
          <dt>Published revisions</dt>
          <dd className="tabular">{summary.revisions}</dd>
        </div>
      </dl>
      {summary.nearlyFull && (
        <p className="banner warn" role="status">
          Your storage on this server is nearly full. Delete old revisions or
          documents you published there to make room.
        </p>
      )}
    </div>
  );
}
