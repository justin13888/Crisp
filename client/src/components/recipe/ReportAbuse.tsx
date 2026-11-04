/**
 * The way to report a recipe to the active server's operator, from the
 * recipe view: the abuse contact the server advertises, if it advertises
 * one.
 */

import { abuseHref } from "../../lib/moderation";
import { useServer } from "../../lib/servers";

/** "Report this recipe" for `docId`; nothing when the server names no contact. */
export function ReportAbuse({ docId }: { docId: string }) {
  const server = useServer();
  const contact = server.info?.abuse_contact?.trim();
  if (!contact) return null;
  const href = abuseHref(contact, docId);
  return (
    <p className="muted recipe-report" data-testid="report-abuse">
      Something wrong with this recipe?{" "}
      {href ? (
        <a href={href} target="_blank" rel="noopener noreferrer">
          Report it to the server operator
        </a>
      ) : (
        <>Report it to the server operator: {contact}</>
      )}
    </p>
  );
}
