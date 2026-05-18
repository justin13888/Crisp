import { type FormEvent, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { fromHex } from "../lib/api";
import {
  formatVersion,
  nowSeconds,
  parseBundle,
  verifyBundle,
} from "../lib/crisp";
import { saveBundle } from "../lib/db";
import { useIdentity } from "../lib/identity";
import type {
  BundlePayload,
  Ingredient,
  Quantity,
  Recipe,
  Step,
} from "../lib/types";

/** A draft ingredient row. */
interface IngredientRow {
  name: string;
  amount: string;
  unit: string;
}

/** Converts a free-text amount into a quantity, or `undefined` if blank. */
function amountToQuantity(
  amount: string,
  unit: string,
  lang: string,
): Quantity | undefined {
  const trimmed = amount.trim();
  if (!trimmed) return undefined;
  const value = Number(trimmed);
  if (Number.isFinite(value)) {
    const decimals = trimmed.includes(".")
      ? (trimmed.split(".")[1]?.length ?? 0)
      : 0;
    const denominator = 10 ** decimals;
    const quantity: Quantity = {
      min: [Math.round(value * denominator), denominator],
    };
    if (unit.trim()) quantity.unit = unit.trim();
    return quantity;
  }
  return { text: { [lang]: trimmed } };
}

/** A 128-bit random birth nonce. */
function birthNonce(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(16));
}

/** The recipe authoring form: build, sign, save, and optionally publish. */
export function Editor() {
  const { identity, username, session, server } = useIdentity();
  const navigate = useNavigate();

  const [title, setTitle] = useState("");
  const [language, setLanguage] = useState("en");
  const [license, setLicense] = useState("CC-BY-4.0");
  const [yieldAmount, setYieldAmount] = useState("4");
  const [yieldUnit, setYieldUnit] = useState("servings");
  const [ingredients, setIngredients] = useState<IngredientRow[]>([
    { name: "", amount: "", unit: "" },
  ]);
  const [steps, setSteps] = useState<string[]>([""]);
  const [tags, setTags] = useState("");
  const [publish, setPublish] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!identity) {
    return (
      <div className="card">
        <p>
          Unlock your identity before authoring a recipe — it must be signed.
        </p>
        <Link className="button" to="/identity">
          Go to Identity
        </Link>
      </div>
    );
  }

  const setIngredient = (index: number, patch: Partial<IngredientRow>) => {
    setIngredients((rows) =>
      rows.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    );
  };

  const buildPayload = async (): Promise<BundlePayload> => {
    const now = nowSeconds();
    const version = await formatVersion();
    const lang = language.trim() || "en";

    const draftIngredients: Ingredient[] = ingredients
      .filter((row) => row.name.trim())
      .map((row, index) => {
        const ingredient: Ingredient = {
          ref: index + 1,
          name: { [lang]: row.name.trim() },
        };
        const quantity = amountToQuantity(row.amount, row.unit, lang);
        if (quantity) ingredient.quantity = quantity;
        return ingredient;
      });

    const draftSteps: Step[] = steps
      .filter((text) => text.trim())
      .map((text, index) => ({
        ref: index + 1,
        text: { [lang]: [[{ t: text.trim() }]] },
      }));

    if (draftIngredients.length === 0)
      throw new Error("add at least one ingredient");
    if (draftSteps.length === 0) throw new Error("add at least one step");

    const recipe: Recipe = {
      identity: {
        author_fpr: new Uint8Array(32),
        title: title.trim(),
        language: lang,
        created_at: now,
        birth_nonce: birthNonce(),
      },
      id: "",
      revision: "",
      modified_at: now,
      title: { [lang]: title.trim() },
      yield: {
        amount: amountToQuantity(yieldAmount, "", lang) ?? { min: [1, 1] },
        unit_text: { [lang]: yieldUnit.trim() || "servings" },
      },
      ingredients: draftIngredients,
      steps: draftSteps,
    };
    const tagList = tags
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);
    if (tagList.length > 0) recipe.tags = tagList;

    return {
      v: version,
      min_v: version,
      kind: "recipe",
      doc_id: "",
      default_language: lang,
      languages: [lang],
      meta: {
        title: { [lang]: title.trim() },
        authors: [
          {
            uid: identity.userId,
            display_name: username ?? "Author",
            keyset: identity.publicKeyset,
          },
        ],
        license: license.trim() || "NONE",
        created_at: now,
        modified_at: now,
      },
      recipes: [recipe],
    };
  };

  const onSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      if (!title.trim()) throw new Error("a title is required");
      const payload = await buildPayload();
      let bytes = identity.buildBundle(payload, nowSeconds());
      let published = false;

      if (publish) {
        if (!session)
          throw new Error("sign in on the Identity page before publishing");
        const result = await server.publish(bytes);
        if (!result.ok) {
          setError(result.message);
          setBusy(false);
          return;
        }
        bytes = result.endorsed;
        published = true;
      }

      const platformKeys = published
        ? await server
            .serverInfo()
            .then((info) =>
              info.platform_keys.map((key) => ({
                ed25519: fromHex(key.ed25519),
                "ml-dsa-65": fromHex(key.ml_dsa_65),
              })),
            )
            .catch(() => [])
        : [];
      const report = await verifyBundle(bytes, platformKeys);
      const finalPayload = await parseBundle(bytes);
      await saveBundle({
        docId: finalPayload.doc_id,
        kind: "recipe",
        title: title.trim(),
        bytes,
        report,
        published,
        importedAt: Date.now(),
      });
      setStatus("Recipe saved.");
      navigate(`/recipe/${encodeURIComponent(finalPayload.doc_id)}`);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section>
      <h1>New recipe</h1>
      <form onSubmit={onSubmit}>
        <label>
          <span>Title</span>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            required
          />
        </label>
        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Language</span>
            <input
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
            />
          </label>
          <label style={{ flex: 1 }}>
            <span>License (SPDX)</span>
            <input
              value={license}
              onChange={(e) => setLicense(e.target.value)}
            />
          </label>
        </div>
        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Yield amount</span>
            <input
              value={yieldAmount}
              onChange={(e) => setYieldAmount(e.target.value)}
            />
          </label>
          <label style={{ flex: 1 }}>
            <span>Yield unit</span>
            <input
              value={yieldUnit}
              onChange={(e) => setYieldUnit(e.target.value)}
            />
          </label>
        </div>

        <h2>Ingredients</h2>
        {ingredients.map((row, index) => (
          <div className="row" key={index}>
            <input
              style={{ flex: 2 }}
              placeholder="Name"
              value={row.name}
              onChange={(e) => setIngredient(index, { name: e.target.value })}
            />
            <input
              style={{ flex: 1 }}
              placeholder="Amount"
              value={row.amount}
              onChange={(e) => setIngredient(index, { amount: e.target.value })}
            />
            <input
              style={{ flex: 1 }}
              placeholder="Unit"
              value={row.unit}
              onChange={(e) => setIngredient(index, { unit: e.target.value })}
            />
            <button
              type="button"
              className="secondary"
              onClick={() =>
                setIngredients((rows) => rows.filter((_, i) => i !== index))
              }
            >
              ✕
            </button>
          </div>
        ))}
        <button
          type="button"
          className="secondary"
          onClick={() =>
            setIngredients((rows) => [
              ...rows,
              { name: "", amount: "", unit: "" },
            ])
          }
        >
          Add ingredient
        </button>

        <h2>Steps</h2>
        {steps.map((text, index) => (
          <div className="row" key={index}>
            <textarea
              style={{ flex: 1 }}
              placeholder={`Step ${index + 1}`}
              value={text}
              onChange={(e) =>
                setSteps((rows) =>
                  rows.map((row, i) => (i === index ? e.target.value : row)),
                )
              }
            />
            <button
              type="button"
              className="secondary"
              onClick={() =>
                setSteps((rows) => rows.filter((_, i) => i !== index))
              }
            >
              ✕
            </button>
          </div>
        ))}
        <button
          type="button"
          className="secondary"
          onClick={() => setSteps((rows) => [...rows, ""])}
        >
          Add step
        </button>

        <label>
          <span>Tags (comma-separated)</span>
          <input value={tags} onChange={(e) => setTags(e.target.value)} />
        </label>

        <label className="row" style={{ alignItems: "center" }}>
          <input
            type="checkbox"
            style={{ width: "auto" }}
            checked={publish}
            onChange={(e) => setPublish(e.target.checked)}
          />
          <span style={{ margin: 0 }}>Publish to the server after signing</span>
        </label>

        {error && <div className="banner bad">{error}</div>}
        {status && (
          <div className="banner" style={{ background: "#e3f2e8" }}>
            {status}
          </div>
        )}

        <button type="submit" disabled={busy}>
          {busy ? "Working…" : "Sign & save recipe"}
        </button>
      </form>
    </section>
  );
}
