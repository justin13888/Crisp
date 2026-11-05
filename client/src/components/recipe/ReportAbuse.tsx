/**
 * The way to report a recipe to the active server's operator, from the
 * recipe view: the abuse contact the server advertises, if it advertises
 * one.
 */

import { Trans, useTranslation } from "../../i18n";
import { abuseHref } from "../../lib/moderation";
import { useServer } from "../../lib/servers";

/** "Report this recipe" for `docId`; nothing when the server names no contact. */
export function ReportAbuse({ docId }: { docId: string }) {
  const { t } = useTranslation("recipe");
  const server = useServer();
  const contact = server.info?.abuse_contact?.trim();
  if (!contact) return null;
  const href = abuseHref(contact, t("report.subject", { docId }));
  return (
    <p className="muted recipe-report" data-testid="report-abuse">
      {href ? (
        <Trans
          t={t}
          i18nKey="report.link"
          components={{
            // biome-ignore lint/a11y/useAnchorContent: <Trans> fills the link text.
            report: <a href={href} target="_blank" rel="noopener noreferrer" />,
          }}
        />
      ) : (
        t("report.text", { contact })
      )}
    </p>
  );
}
