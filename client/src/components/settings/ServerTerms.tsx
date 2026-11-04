/**
 * The active server's terms of service, shown where signing in may register
 * an account; nothing when the server publishes no terms.
 */

import { termsHref } from "../../lib/moderation";
import { useServer } from "../../lib/servers";

/** The terms a first sign-in agrees to, as a link. */
export function ServerTerms() {
  const server = useServer();
  const href = termsHref(server.info?.terms_url);
  if (!href) return null;
  return (
    <p className="muted" data-testid="server-terms">
      Signing in for the first time creates an account on this server, under its{" "}
      <a href={href} target="_blank" rel="noopener noreferrer">
        terms of service
      </a>
      .
    </p>
  );
}
