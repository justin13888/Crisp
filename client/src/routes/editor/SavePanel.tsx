/**
 * "Save to library": unlock the identity if needed, seal the draft into a
 * signed file, store it, and optionally publish it to the server.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import type { BundlePayload, Issue } from "../../generated/crisp";
import { useHotkeys } from "../../hooks/useHotkeys";
import {
  errorMessage,
  isCrispFailure,
  nowSeconds,
  type Signer,
} from "../../lib/crisp";
import { nextPaint, useIdentity } from "../../lib/identity";
import { addFile } from "../../lib/library";
import { createLogger } from "../../lib/log";
import { useServer } from "../../lib/servers";

const log = createLogger("editor.save");

/** What the editor hands the save panel. */
export interface SavePanelProps {
  /**
   * Builds and checks the draft. Returns `null` (after showing the
   * problems) when it would not seal.
   */
  prepare(): BundlePayload | null;
  /** The file being revised, if any (`seal`'s `previous`). */
  previous: Uint8Array | null;
  /** Shows issues `seal` reported. */
  onIssues(issues: Issue[]): void;
  /** Called once the file is in the library (drops the autosave). */
  onSaved(docId: string): Promise<void>;
}

type Phase = "idle" | "unlock" | "unlocking" | "saving" | "publishing";

/** The save controls at the end of the editor. */
export function SavePanel({
  prepare,
  previous,
  onIssues,
  onSaved,
}: SavePanelProps) {
  const identity = useIdentity();
  const server = useServer();
  const navigate = useNavigate();
  const [phase, setPhase] = useState<Phase>("idle");
  const [passphrase, setPassphrase] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [alsoPublish, setAlsoPublish] = useState(false);
  const [publishFailed, setPublishFailed] = useState<{
    docId: string;
    bytes: Uint8Array;
  } | null>(null);
  const resumeAfterUnlock = useRef(false);
  const passphraseRef = useRef<HTMLInputElement>(null);

  const publish = useCallback(
    async (docId: string, bytes: Uint8Array) => {
      setPhase("publishing");
      setError(null);
      try {
        const endorsed = await server.publish(bytes);
        await addFile(endorsed);
        log.info("published", { docId });
        setPublishFailed(null);
        navigate(`/recipe/${encodeURIComponent(docId)}`);
      } catch (cause: unknown) {
        log.warn("publish.failed", { docId, error: errorMessage(cause) });
        setPublishFailed({ docId, bytes });
        setError(
          `Saved to your library, but publishing failed: ${errorMessage(cause)}`,
        );
        setPhase("idle");
      }
    },
    [navigate, server],
  );

  const seal = useCallback(
    async (signer: Signer) => {
      const draft = prepare();
      if (!draft) {
        setPhase("idle");
        return;
      }
      setPhase("saving");
      setError(null);
      await nextPaint();
      const started = performance.now();
      let bytes: Uint8Array;
      try {
        bytes = signer.seal(
          draft,
          identity.displayName ?? "",
          nowSeconds(),
          previous,
        );
      } catch (cause: unknown) {
        log.warn("seal.failed", { error: errorMessage(cause) });
        if (isCrispFailure(cause) && cause.issues?.length)
          onIssues(cause.issues);
        setError(`Could not save: ${errorMessage(cause)}`);
        setPhase("idle");
        return;
      }
      try {
        const { entry } = await addFile(bytes);
        const docId = entry.record.docId;
        log.info("saved", {
          docId,
          revision: entry.record.revision,
          revising: previous !== null,
          bytes: bytes.length,
          ms: Math.round(performance.now() - started),
        });
        await onSaved(docId);
        if (alsoPublish && server.username) {
          await publish(docId, bytes);
        } else {
          navigate(`/recipe/${encodeURIComponent(docId)}`);
        }
      } catch (cause: unknown) {
        log.error("store.failed", { error: errorMessage(cause) });
        setError(`Could not store the recipe: ${errorMessage(cause)}`);
        setPhase("idle");
      }
    },
    [
      alsoPublish,
      identity.displayName,
      navigate,
      onIssues,
      onSaved,
      prepare,
      previous,
      publish,
      server.username,
    ],
  );

  const save = useCallback(() => {
    if (phase === "saving" || phase === "publishing" || phase === "unlocking")
      return;
    if (!prepare()) return;
    if (identity.signer) {
      void seal(identity.signer);
    } else if (identity.status === "locked") {
      setPhase("unlock");
      setError(null);
    }
  }, [identity.signer, identity.status, phase, prepare, seal]);

  // Finish a save that was waiting for the identity to unlock.
  useEffect(() => {
    if (identity.signer && resumeAfterUnlock.current) {
      resumeAfterUnlock.current = false;
      void seal(identity.signer);
    }
  }, [identity.signer, seal]);

  useEffect(() => {
    if (phase === "unlock") passphraseRef.current?.focus();
  }, [phase]);

  useHotkeys({ "mod+s": save }, { allowInInputs: true });

  const unlock = async () => {
    setPhase("unlocking");
    setError(null);
    try {
      resumeAfterUnlock.current = true;
      await identity.unlock(passphrase);
      setPassphrase("");
    } catch (cause: unknown) {
      resumeAfterUnlock.current = false;
      log.info("unlock.failed", { error: errorMessage(cause) });
      setError("That passphrase did not unlock your identity.");
      setPhase("unlock");
    }
  };

  const busy =
    phase === "saving" || phase === "publishing" || phase === "unlocking";

  return (
    <section
      id="sec-save"
      className="editor-section save-panel"
      aria-labelledby="h-save"
    >
      <h2 id="h-save">Save</h2>
      {identity.status === "none" ? (
        <p className="banner warn">
          To save recipes you need an identity — it signs your recipes.{" "}
          <Link to="/settings">Create one in Settings</Link>. Your draft is kept
          meanwhile.
        </p>
      ) : null}
      {server.username ? (
        <label className="check" htmlFor="f-also-publish">
          <input
            id="f-also-publish"
            type="checkbox"
            checked={alsoPublish}
            onChange={(e) => setAlsoPublish(e.currentTarget.checked)}
          />
          <span>Also publish to {new URL(server.url).host}</span>
        </label>
      ) : null}
      {phase === "unlock" || phase === "unlocking" ? (
        <form
          className="unlock stack-sm"
          onSubmit={(e) => {
            e.preventDefault();
            void unlock();
          }}
        >
          <label htmlFor="f-passphrase">
            <span>Passphrase to unlock your identity</span>
          </label>
          <input
            id="f-passphrase"
            ref={passphraseRef}
            type="password"
            autoComplete="current-password"
            value={passphrase}
            onChange={(e) => setPassphrase(e.currentTarget.value)}
            disabled={phase === "unlocking"}
          />
          <div className="cluster">
            <button
              type="submit"
              disabled={!passphrase || phase === "unlocking"}
            >
              {phase === "unlocking" ? (
                <>
                  <span className="spinner" aria-hidden="true" /> Unlocking…
                </>
              ) : (
                "Unlock and save"
              )}
            </button>
            <button
              type="button"
              className="ghost"
              disabled={phase === "unlocking"}
              onClick={() => setPhase("idle")}
            >
              Cancel
            </button>
          </div>
        </form>
      ) : null}
      {error ? (
        <p className="banner bad" role="alert">
          {error}
        </p>
      ) : null}
      <div className="cluster">
        {publishFailed ? (
          <>
            <button
              type="button"
              className="lg"
              disabled={busy}
              onClick={() =>
                void publish(publishFailed.docId, publishFailed.bytes)
              }
            >
              Retry publishing
            </button>
            <Link
              className="button secondary lg"
              to={`/recipe/${encodeURIComponent(publishFailed.docId)}`}
            >
              Open the saved recipe
            </Link>
          </>
        ) : phase === "unlock" || phase === "unlocking" ? null : (
          <button
            type="button"
            className="lg"
            onClick={save}
            disabled={
              busy ||
              identity.status === "none" ||
              identity.status === "loading"
            }
          >
            {phase === "saving" || phase === "publishing" ? (
              <>
                <span className="spinner" aria-hidden="true" />
                {phase === "saving" ? "Saving…" : "Publishing…"}
              </>
            ) : (
              "Save to library"
            )}
          </button>
        )}
      </div>
    </section>
  );
}
