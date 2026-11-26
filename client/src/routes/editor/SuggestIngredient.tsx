/**
 * "Suggest a missing ingredient": sends what an ingredient name proposes
 * to the ingredient index through the signed-in server account, or says
 * that suggestions need one.
 */
import { type MouseEvent, useState } from "react";
import { useTranslation } from "../../i18n";
import type { FiledSuggestion, IngredientSuggestion } from "../../lib/crisp";
import { createLogger } from "../../lib/log";
import { useServer } from "../../lib/servers";
import { suggestionFailureMessage } from "./suggest";

const log = createLogger("editor");

/** Where one suggestion stands. */
type SuggestState =
  | { step: "idle" }
  | { step: "signedOut" }
  | { step: "sending" }
  | { step: "sent"; filed: FiledSuggestion }
  | { step: "failed"; message: string };

/** The action and its outcome, over the app's server connection. */
export function SuggestIngredient({
  suggestion,
}: {
  /** What to suggest. */
  suggestion: IngredientSuggestion;
}) {
  const { t } = useTranslation("editorLists");
  const server = useServer();
  const [state, setState] = useState<SuggestState>({ step: "idle" });

  const send = async () => {
    if (!server.username) {
      setState({ step: "signedOut" });
      return;
    }
    setState({ step: "sending" });
    const fields = { kind: suggestion.kind, locale: suggestion.locale };
    try {
      const client = await server.client();
      const filed = await client.suggestIngredient(suggestion);
      log.info("editor.ingredient_suggested", { ...fields, id: filed.id });
      setState({ step: "sent", filed });
    } catch (cause: unknown) {
      const message = suggestionFailureMessage(cause);
      log.warn("editor.ingredient_suggestion_failed", { ...fields, message });
      setState({ step: "failed", message });
    }
  };
  // Keeps focus in the name's group across the press (see IngredientLink).
  const keepFocus = (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault();
  };

  if (state.step === "sent") {
    return (
      <p className="muted hint ingredient-suggest" role="status">
        {t("link.suggest.sent")}
      </p>
    );
  }
  const label =
    suggestion.kind === "name"
      ? t("link.suggest.name", { name: suggestion.text })
      : t("link.suggest.missing");
  return (
    <div className="ingredient-suggest">
      <button
        type="button"
        className="secondary"
        disabled={state.step === "sending"}
        onMouseDown={keepFocus}
        onClick={() => void send()}
      >
        {state.step === "sending" ? t("link.suggest.sending") : label}
      </button>
      {state.step === "signedOut" ? (
        <p className="muted hint" role="status">
          {t("link.suggest.signedOut")}
        </p>
      ) : null}
      {state.step === "failed" ? (
        <p className="bad hint" role="alert">
          {state.message}
        </p>
      ) : null}
    </div>
  );
}
