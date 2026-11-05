/**
 * The active server's terms of service, shown where signing in may register
 * an account; nothing when the server publishes no terms.
 */

import { Trans, useTranslation } from "../../i18n";
import { termsHref } from "../../lib/moderation";
import { useServer } from "../../lib/servers";

/** The terms a first sign-in agrees to, as a link. */
export function ServerTerms() {
  const { t } = useTranslation("settings");
  const server = useServer();
  const href = termsHref(server.info?.terms_url);
  if (!href) return null;
  return (
    <p className="muted" data-testid="server-terms">
      <Trans
        t={t}
        i18nKey="server.terms"
        components={{
          // biome-ignore lint/a11y/useAnchorContent: <Trans> fills the link text.
          terms: <a href={href} target="_blank" rel="noopener noreferrer" />,
        }}
      />
    </p>
  );
}
