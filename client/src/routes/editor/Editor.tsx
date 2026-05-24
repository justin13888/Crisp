/**
 * The recipe authoring route at `/new`.
 *
 * Every guardrail in this form is driven by `RecipeConstraints` loaded from
 * `crisp-core` at mount time — the form does not render until constraints
 * resolve. Validation is recomputed on every change and gates the submit
 * button, so the user can never build a bundle that violates a schema rule.
 *
 * Localized fields are authored in a single language; the language tag is
 * picked from the curated list in `curated.ts`. Multi-language authoring is
 * out of scope for the creation form.
 */

import {
  type FormEvent,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Link, useNavigate } from "react-router-dom";
import { fromHex } from "../../lib/api";
import {
  formatVersion,
  nowSeconds,
  parseBundle,
  recipeConstraints,
  verifyBundle,
} from "../../lib/crisp";
import { saveBundle } from "../../lib/db";
import { useIdentity } from "../../lib/identity";
import type { RecipeConstraints } from "../../lib/types";
import { buildPayload } from "./buildPayload";
import { LANGUAGES, LICENSES } from "./curated";
import { EnumMultiSelect } from "./EnumMultiSelect";
import { EquipmentEditor } from "./EquipmentEditor";
import { ByteCountedInput, ByteCountedTextarea, humanizeCode } from "./fields";
import { IngredientEditor } from "./IngredientEditor";
import { StepEditor } from "./StepEditor";
import {
  emptyDraft,
  emptyEquipment,
  emptyIngredient,
  emptyStep,
  emptyTime,
  type IngredientDraft,
  type NutritionDraft,
  type RecipeDraft,
  type StepDraft,
  type TimeDraft,
} from "./state";
import { validateDraft } from "./validate";

/** The recipe authoring form: build, sign, save, and optionally publish. */
export function Editor() {
  const { identity, username, session, server } = useIdentity();
  const navigate = useNavigate();

  const [constraints, setConstraints] = useState<RecipeConstraints | null>(
    null,
  );
  const [draft, setDraft] = useState<RecipeDraft>(() => emptyDraft());
  const [touched, setTouched] = useState<Set<string>>(() => new Set());
  const [submitAttempted, setSubmitAttempted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    recipeConstraints()
      .then((c) => {
        if (!cancelled) setConstraints(c);
      })
      .catch((e: unknown) => {
        if (!cancelled)
          setError(
            e instanceof Error
              ? `Could not load form constraints: ${e.message}`
              : "Could not load form constraints.",
          );
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const errors = useMemo(
    () => (constraints ? validateDraft(draft, constraints) : {}),
    [draft, constraints],
  );

  // Show errors only after a field is touched OR after a submit attempt.
  const visibleTouched = useMemo(() => {
    if (!submitAttempted) return touched;
    return new Set(Object.keys(errors));
  }, [touched, submitAttempted, errors]);

  const touch = useCallback(
    (path: string) => setTouched((prev) => new Set(prev).add(path)),
    [],
  );

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

  if (!constraints) {
    return (
      <section>
        <h1>New recipe</h1>
        {error ? (
          <div className="banner bad">{error}</div>
        ) : (
          <p className="muted">Loading…</p>
        )}
      </section>
    );
  }

  const patch = (next: Partial<RecipeDraft>) =>
    setDraft((prev) => ({ ...prev, ...next }));

  // ---- Per-list helpers ----------------------------------------------------
  const setIngredient = (index: number, p: Partial<IngredientDraft>) =>
    setDraft((prev) => ({
      ...prev,
      ingredients: prev.ingredients.map((row, i) =>
        i === index ? { ...row, ...p } : row,
      ),
    }));
  const removeIngredient = (index: number) =>
    setDraft((prev) => ({
      ...prev,
      ingredients: prev.ingredients.filter((_, i) => i !== index),
      // Drop stale references on every step.
      steps: prev.steps.map((s) => ({
        ...s,
        ingredientIds: s.ingredientIds.filter(
          (id) => id !== prev.ingredients[index]?.id,
        ),
      })),
    }));

  const setStep = (index: number, p: Partial<StepDraft>) =>
    setDraft((prev) => ({
      ...prev,
      steps: prev.steps.map((row, i) => (i === index ? { ...row, ...p } : row)),
    }));
  const removeStep = (index: number) =>
    setDraft((prev) => ({
      ...prev,
      steps: prev.steps.filter((_, i) => i !== index),
    }));

  const setEquipment = (
    index: number,
    p: Partial<{ name: string; note: string }>,
  ) =>
    setDraft((prev) => ({
      ...prev,
      equipment: prev.equipment.map((row, i) =>
        i === index ? { ...row, ...p } : row,
      ),
    }));
  const removeEquipment = (index: number) =>
    setDraft((prev) => ({
      ...prev,
      equipment: prev.equipment.filter((_, i) => i !== index),
      steps: prev.steps.map((s) => ({
        ...s,
        equipmentIds: s.equipmentIds.filter(
          (id) => id !== prev.equipment[index]?.id,
        ),
      })),
    }));

  const setTime = (index: number, p: Partial<TimeDraft>) =>
    setDraft((prev) => ({
      ...prev,
      times: prev.times.map((row, i) => (i === index ? { ...row, ...p } : row)),
    }));

  const setGroupName = (index: number, name: string) =>
    setDraft((prev) => ({
      ...prev,
      ingredientGroups: prev.ingredientGroups.map((g, i) =>
        i === index ? name : g,
      ),
    }));
  const removeGroup = (index: number) =>
    setDraft((prev) => ({
      ...prev,
      ingredientGroups: prev.ingredientGroups.filter((_, i) => i !== index),
      // Shift / clear ingredients whose group was deleted or moved.
      ingredients: prev.ingredients.map((row) => {
        if (row.group === null) return row;
        if (row.group === index) return { ...row, group: null };
        if (row.group > index) return { ...row, group: row.group - 1 };
        return row;
      }),
    }));

  const setNutrition = (p: Partial<NutritionDraft>) =>
    setDraft((prev) => ({ ...prev, nutrition: { ...prev.nutrition, ...p } }));
  const setNutritionValue = (
    code: string,
    p: { amount?: string; unit?: string },
  ) =>
    setDraft((prev) => ({
      ...prev,
      nutrition: {
        ...prev.nutrition,
        values: {
          ...prev.nutrition.values,
          [code]: {
            amount: prev.nutrition.values[code]?.amount ?? "",
            unit: prev.nutrition.values[code]?.unit ?? "",
            ...p,
          },
        },
      },
    }));

  // ---- Submit --------------------------------------------------------------
  const onSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setSubmitAttempted(true);
    if (Object.keys(errors).length > 0) {
      setError("Some fields need attention.");
      return;
    }
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const payload = buildPayload(draft, {
        identity,
        username,
        formatVersion: await formatVersion(),
        now: nowSeconds(),
      });
      let bytes = identity.buildBundle(payload, nowSeconds());
      let published = false;
      if (draft.publish) {
        if (!session)
          throw new Error("Sign in on the Identity page before publishing.");
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
        title: draft.title.trim(),
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

  const limits = constraints.limits;
  const ingredientLimitHit = draft.ingredients.length >= limits.max_ingredients;
  const stepLimitHit = draft.steps.length >= limits.max_steps;
  const equipmentLimitHit = draft.equipment.length >= limits.max_equipment;
  const groupLimitHit =
    draft.ingredientGroups.length >= limits.max_ingredient_groups;
  const submitDisabled = busy || Object.keys(errors).length > 0;

  return (
    <section>
      <h1>New recipe</h1>
      <form onSubmit={onSubmit} noValidate>
        {/* -------- Basics -------------------------------------------------- */}
        <label>
          <span>Title</span>
          <ByteCountedInput
            value={draft.title}
            onChange={(v) => patch({ title: v })}
            maxBytes={limits.max_title_bytes}
            required
            error={errors["title"]}
            touched={visibleTouched.has("title")}
            onBlur={() => touch("title")}
          />
        </label>

        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Language</span>
            <select
              value={draft.language}
              onChange={(e) => patch({ language: e.target.value })}
              onBlur={() => touch("language")}
            >
              {LANGUAGES.map((l) => (
                <option key={l.tag} value={l.tag}>
                  {l.label} ({l.tag})
                </option>
              ))}
            </select>
            {visibleTouched.has("language") && errors["language"] && (
              <div style={{ color: "var(--bad)", fontSize: "0.78rem" }}>
                {errors["language"]}
              </div>
            )}
          </label>
          <label style={{ flex: 1 }}>
            <span>License (SPDX)</span>
            <select
              value={draft.license}
              onChange={(e) => patch({ license: e.target.value })}
              onBlur={() => touch("license")}
            >
              {LICENSES.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.label}
                </option>
              ))}
            </select>
            {visibleTouched.has("license") && errors["license"] && (
              <div style={{ color: "var(--bad)", fontSize: "0.78rem" }}>
                {errors["license"]}
              </div>
            )}
          </label>
        </div>

        <div className="row">
          <label style={{ flex: 1 }}>
            <span>Yield amount</span>
            <ByteCountedInput
              value={draft.yieldAmount}
              onChange={(v) => patch({ yieldAmount: v })}
              maxBytes={limits.max_text_bytes}
            />
          </label>
          <label style={{ flex: 1 }}>
            <span>Yield unit</span>
            <ByteCountedInput
              value={draft.yieldUnit}
              onChange={(v) => patch({ yieldUnit: v })}
              maxBytes={limits.max_text_bytes}
              error={errors["yield"]}
              touched={visibleTouched.has("yield")}
              onBlur={() => touch("yield")}
            />
          </label>
        </div>

        <label>
          <span>Summary (optional)</span>
          <ByteCountedTextarea
            value={draft.summary}
            onChange={(v) => patch({ summary: v })}
            maxBytes={limits.max_text_bytes}
            error={errors["summary"]}
            touched={visibleTouched.has("summary")}
            onBlur={() => touch("summary")}
          />
        </label>

        {/* -------- Ingredient groups -------------------------------------- */}
        <h2>Ingredient groups</h2>
        <p className="muted" style={{ fontSize: "0.85rem" }}>
          Optional. Group ingredients (e.g. "Dough", "Filling") so each
          ingredient can reference one.
        </p>
        {draft.ingredientGroups.map((name, i) => (
          <div className="row" key={i}>
            <ByteCountedInput
              style={{ flex: 1 }}
              value={name}
              onChange={(v) => setGroupName(i, v)}
              maxBytes={limits.max_text_bytes}
              placeholder={`Group ${i + 1}`}
              error={errors[`group.${i}`]}
              touched={visibleTouched.has(`group.${i}`)}
              onBlur={() => touch(`group.${i}`)}
            />
            <button
              type="button"
              className="secondary"
              onClick={() => removeGroup(i)}
            >
              ✕
            </button>
          </div>
        ))}
        <button
          type="button"
          className="secondary"
          disabled={groupLimitHit}
          title={
            groupLimitHit
              ? `Limit reached (${limits.max_ingredient_groups})`
              : undefined
          }
          onClick={() =>
            patch({ ingredientGroups: [...draft.ingredientGroups, ""] })
          }
        >
          Add group
        </button>

        {/* -------- Ingredients -------------------------------------------- */}
        <h2>
          Ingredients{" "}
          <span className="muted" style={{ fontSize: "0.85rem" }}>
            ({draft.ingredients.length} / {limits.max_ingredients})
          </span>
        </h2>
        {visibleTouched.has("ingredients") && errors["ingredients"] && (
          <div className="banner bad">{errors["ingredients"]}</div>
        )}
        {draft.ingredients.map((row, i) => (
          <IngredientEditor
            key={row.id}
            index={i}
            row={row}
            onChange={(p) => setIngredient(i, p)}
            onRemove={() => removeIngredient(i)}
            canRemove={draft.ingredients.length > 1}
            ingredientGroupNames={draft.ingredientGroups}
            constraints={constraints}
            errors={errors}
            touched={visibleTouched}
            touch={touch}
          />
        ))}
        <button
          type="button"
          className="secondary"
          disabled={ingredientLimitHit}
          title={
            ingredientLimitHit
              ? `Limit reached (${limits.max_ingredients})`
              : undefined
          }
          onClick={() => {
            touch("ingredients");
            patch({ ingredients: [...draft.ingredients, emptyIngredient()] });
          }}
        >
          Add ingredient
        </button>

        {/* -------- Equipment ---------------------------------------------- */}
        <h2>
          Equipment{" "}
          <span className="muted" style={{ fontSize: "0.85rem" }}>
            ({draft.equipment.length} / {limits.max_equipment})
          </span>
        </h2>
        {draft.equipment.map((row, i) => (
          <EquipmentEditor
            key={row.id}
            index={i}
            row={row}
            onChange={(p) => setEquipment(i, p)}
            onRemove={() => removeEquipment(i)}
            constraints={constraints}
            errors={errors}
            touched={visibleTouched}
            touch={touch}
          />
        ))}
        <button
          type="button"
          className="secondary"
          disabled={equipmentLimitHit}
          title={
            equipmentLimitHit
              ? `Limit reached (${limits.max_equipment})`
              : undefined
          }
          onClick={() =>
            patch({ equipment: [...draft.equipment, emptyEquipment()] })
          }
        >
          Add equipment
        </button>

        {/* -------- Steps -------------------------------------------------- */}
        <h2>
          Steps{" "}
          <span className="muted" style={{ fontSize: "0.85rem" }}>
            ({draft.steps.length} / {limits.max_steps})
          </span>
        </h2>
        {visibleTouched.has("steps") && errors["steps"] && (
          <div className="banner bad">{errors["steps"]}</div>
        )}
        {draft.steps.map((row, i) => (
          <StepEditor
            key={row.id}
            index={i}
            row={row}
            onChange={(p) => setStep(i, p)}
            onRemove={() => removeStep(i)}
            canRemove={draft.steps.length > 1}
            ingredients={draft.ingredients}
            equipment={draft.equipment}
            constraints={constraints}
            errors={errors}
            touched={visibleTouched}
            touch={touch}
          />
        ))}
        <button
          type="button"
          className="secondary"
          disabled={stepLimitHit}
          title={
            stepLimitHit ? `Limit reached (${limits.max_steps})` : undefined
          }
          onClick={() => {
            touch("steps");
            patch({ steps: [...draft.steps, emptyStep()] });
          }}
        >
          Add step
        </button>

        {/* -------- Times (named-phase durations) -------------------------- */}
        <h2>Phase durations</h2>
        <p className="muted" style={{ fontSize: "0.85rem" }}>
          Optional. Named durations alongside the recipe (e.g. prep, bake).
        </p>
        {draft.times.map((t, i) => (
          <div className="row" key={t.id}>
            <ByteCountedInput
              style={{ flex: 1 }}
              value={t.phase}
              onChange={(v) => setTime(i, { phase: v })}
              maxBytes={limits.max_text_bytes}
              placeholder="Phase (e.g. prep)"
              error={errors[`times.${i}.phase`]}
              touched={visibleTouched.has(`times.${i}.phase`)}
              onBlur={() => touch(`times.${i}.phase`)}
            />
            <ByteCountedInput
              style={{ flex: 1 }}
              value={t.amount}
              onChange={(v) => setTime(i, { amount: v })}
              maxBytes={limits.max_text_bytes}
              placeholder="Amount (e.g. 30)"
              error={errors[`times.${i}.amount`]}
              touched={visibleTouched.has(`times.${i}.amount`)}
              onBlur={() => touch(`times.${i}.amount`)}
            />
            <ByteCountedInput
              style={{ flex: 1 }}
              value={t.unit}
              onChange={(v) => setTime(i, { unit: v })}
              maxBytes={limits.unit_code_max_bytes}
              placeholder="Unit (e.g. min)"
              error={errors[`times.${i}.unit`]}
              touched={visibleTouched.has(`times.${i}.unit`)}
              onBlur={() => touch(`times.${i}.unit`)}
            />
            <button
              type="button"
              className="secondary"
              onClick={() =>
                patch({ times: draft.times.filter((_, j) => j !== i) })
              }
            >
              ✕
            </button>
          </div>
        ))}
        <button
          type="button"
          className="secondary"
          onClick={() => patch({ times: [...draft.times, emptyTime()] })}
        >
          Add phase
        </button>

        {/* -------- Classification ----------------------------------------- */}
        <h2>Classification</h2>
        <label>
          <span>Tags (comma-separated, up to {limits.max_tags})</span>
          <ByteCountedInput
            value={draft.tags}
            onChange={(v) => patch({ tags: v })}
            maxBytes={limits.max_text_bytes}
            placeholder="quick, weeknight"
            error={errors["tags"]}
            touched={visibleTouched.has("tags")}
            onBlur={() => touch("tags")}
          />
        </label>

        <label>
          <span>Cuisine (comma-separated, up to {limits.max_cuisine})</span>
          <ByteCountedInput
            value={draft.cuisine}
            onChange={(v) => patch({ cuisine: v })}
            maxBytes={limits.max_text_bytes}
            placeholder="italian, mediterranean"
            error={errors["cuisine"]}
            touched={visibleTouched.has("cuisine")}
            onBlur={() => touch("cuisine")}
          />
        </label>

        <fieldset
          style={{
            border: "none",
            padding: 0,
            margin: "0.6rem 0",
            color: "var(--muted)",
          }}
        >
          <legend>Course</legend>
          <EnumMultiSelect
            options={constraints.enums.course}
            selected={draft.course}
            onChange={(next) => patch({ course: next })}
            maxSelected={limits.max_course}
            idPrefix="course"
          />
        </fieldset>

        <fieldset
          style={{
            border: "none",
            padding: 0,
            margin: "0.6rem 0",
            color: "var(--muted)",
          }}
        >
          <legend>Diet</legend>
          <EnumMultiSelect
            options={constraints.enums.diet}
            selected={draft.diet}
            onChange={(next) => patch({ diet: next })}
            maxSelected={limits.max_diet}
            idPrefix="diet"
          />
        </fieldset>

        <fieldset
          style={{
            border: "none",
            padding: 0,
            margin: "0.6rem 0",
            color: "var(--muted)",
          }}
        >
          <legend>Declared allergens</legend>
          <EnumMultiSelect
            options={constraints.enums.allergens}
            selected={draft.allergens}
            onChange={(next) => patch({ allergens: next })}
            maxSelected={limits.max_allergens}
            idPrefix="allergen"
          />
        </fieldset>

        {/* -------- Nutrition ---------------------------------------------- */}
        <details>
          <summary>
            <h2 style={{ display: "inline", fontSize: "1.1rem" }}>
              Nutrition (optional)
            </h2>
          </summary>
          <div className="row">
            <label style={{ flex: 1 }}>
              <span>Basis amount (e.g. 1 serving, 100 g)</span>
              <ByteCountedInput
                value={draft.nutrition.basisAmount}
                onChange={(v) => setNutrition({ basisAmount: v })}
                maxBytes={limits.max_text_bytes}
                error={errors["nutrition.basisAmount"]}
                touched={visibleTouched.has("nutrition.basisAmount")}
                onBlur={() => touch("nutrition.basisAmount")}
              />
            </label>
            <label style={{ flex: 1 }}>
              <span>Basis unit</span>
              <ByteCountedInput
                value={draft.nutrition.basisUnit}
                onChange={(v) => setNutrition({ basisUnit: v })}
                maxBytes={limits.unit_code_max_bytes}
                error={errors["nutrition.basisUnit"]}
                touched={visibleTouched.has("nutrition.basisUnit")}
                onBlur={() => touch("nutrition.basisUnit")}
              />
            </label>
          </div>
          {constraints.enums.nutrients.map((code) => {
            const v = draft.nutrition.values[code] ?? { amount: "", unit: "" };
            return (
              <div className="row" key={code}>
                <label style={{ flex: 1 }}>
                  <span>{humanizeCode(code)}</span>
                  <ByteCountedInput
                    value={v.amount}
                    onChange={(val) => setNutritionValue(code, { amount: val })}
                    maxBytes={limits.max_text_bytes}
                    error={errors[`nutrition.values.${code}`]}
                    touched={visibleTouched.has(`nutrition.values.${code}`)}
                    onBlur={() => touch(`nutrition.values.${code}`)}
                  />
                </label>
                <label style={{ flex: 1 }}>
                  <span>Unit</span>
                  <ByteCountedInput
                    value={v.unit}
                    onChange={(val) => setNutritionValue(code, { unit: val })}
                    maxBytes={limits.unit_code_max_bytes}
                  />
                </label>
              </div>
            );
          })}
        </details>

        {/* -------- Notes / attribution / version -------------------------- */}
        <h2>Notes</h2>
        <label>
          <span>Notes</span>
          <ByteCountedTextarea
            value={draft.notes}
            onChange={(v) => patch({ notes: v })}
            maxBytes={limits.max_text_bytes}
            error={errors["notes"]}
            touched={visibleTouched.has("notes")}
            onBlur={() => touch("notes")}
          />
        </label>
        <label>
          <span>Attribution</span>
          <ByteCountedTextarea
            value={draft.attribution}
            onChange={(v) => patch({ attribution: v })}
            maxBytes={limits.max_text_bytes}
            error={errors["attribution"]}
            touched={visibleTouched.has("attribution")}
            onBlur={() => touch("attribution")}
          />
        </label>

        <details>
          <summary className="muted">Advanced</summary>
          <label>
            <span>Version (optional monotonic counter)</span>
            <input
              type="number"
              min={0}
              value={draft.version}
              onChange={(e) => patch({ version: e.target.value })}
              onBlur={() => touch("version")}
            />
            {visibleTouched.has("version") && errors["version"] && (
              <div style={{ color: "var(--bad)", fontSize: "0.78rem" }}>
                {errors["version"]}
              </div>
            )}
          </label>
        </details>

        {/* -------- Submit ------------------------------------------------- */}
        <label className="row" style={{ alignItems: "center" }}>
          <input
            type="checkbox"
            style={{ width: "auto" }}
            checked={draft.publish}
            onChange={(e) => patch({ publish: e.target.checked })}
          />
          <span style={{ margin: 0 }}>Publish to the server after signing</span>
        </label>

        {error && <div className="banner bad">{error}</div>}
        {status && (
          <div className="banner" style={{ background: "#e3f2e8" }}>
            {status}
          </div>
        )}

        <button type="submit" disabled={submitDisabled}>
          {busy ? "Working…" : "Sign & save recipe"}
        </button>
        {submitAttempted && Object.keys(errors).length > 0 && (
          <p className="muted" style={{ fontSize: "0.85rem" }}>
            Resolve the highlighted fields before submitting.
          </p>
        )}
      </form>
    </section>
  );
}
