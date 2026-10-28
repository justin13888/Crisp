/** A step card's timers and its heat and temperature controls. */
import type {
  HeatLevel,
  TemperatureTarget,
  TemperatureUnit,
} from "../../generated/crisp";
import { useTranslation } from "../../i18n";
import { updateStep } from "./actions";
import { useEditor } from "./context";
import {
  AmountField,
  CheckField,
  IconButton,
  SelectField,
  TextField,
} from "./fields";
import {
  blankTimer,
  fieldId,
  HEAT_LEVELS,
  LIMITS,
  type StepForm,
  TEMPERATURE_TARGETS,
  TEMPERATURE_UNITS,
} from "./model";

/** A step's timers. */
export function Timers({ step }: { step: StepForm }) {
  const { t } = useTranslation("editorLists");
  const { update, focusLater } = useEditor();
  const set = (timers: StepForm["timers"]) =>
    update((f) => updateStep(f, step.ref, { timers }));
  return (
    <div className="stack-sm">
      {step.timers.map((timer, j) => (
        <div className="timer-row" key={j}>
          <AmountField
            kind="duration"
            id={fieldId.timer(step.ref, j, "duration")}
            label={t("timer.duration", { number: j + 1 })}
            placeholder={t("timer.durationPlaceholder")}
            value={timer.duration}
            onChange={(duration) =>
              set(step.timers.map((x, k) => (k === j ? { ...x, duration } : x)))
            }
            className="field-amount"
          />
          <TextField
            id={fieldId.timer(step.ref, j, "label")}
            label={t("timer.label", { number: j + 1 })}
            placeholder={t("timer.labelPlaceholder")}
            value={timer.label}
            onChange={(label) =>
              set(step.timers.map((x, k) => (k === j ? { ...x, label } : x)))
            }
          />
          <CheckField
            id={`f-step-${step.ref}-timer-${j}-passive`}
            label={t("timer.passive")}
            checked={timer.passive}
            onChange={(passive) =>
              set(step.timers.map((x, k) => (k === j ? { ...x, passive } : x)))
            }
          />
          <IconButton
            label={t("timer.remove", { number: j + 1 })}
            onClick={() => set(step.timers.filter((_, k) => k !== j))}
          >
            ✕
          </IconButton>
        </div>
      ))}
      {step.timers.length < LIMITS.timers ? (
        <button
          type="button"
          className="secondary"
          onClick={() => {
            set([...step.timers, blankTimer()]);
            focusLater(fieldId.timer(step.ref, step.timers.length, "duration"));
          }}
        >
          {t("timer.add")}
        </button>
      ) : null}
    </div>
  );
}

/** Catalogue keys naming each heat level. */
const HEAT_LABELS = {
  low: "heat.level.low",
  "medium-low": "heat.level.mediumLow",
  medium: "heat.level.medium",
  "medium-high": "heat.level.mediumHigh",
  high: "heat.level.high",
} as const satisfies Record<HeatLevel, string>;

/** Catalogue keys naming what each temperature applies to. */
const TARGET_LABELS = {
  oven: "temp.target.oven",
  internal: "temp.target.internal",
  oil: "temp.target.oil",
  pan: "temp.target.pan",
  water: "temp.target.water",
} as const satisfies Record<TemperatureTarget, string>;

/** Heat level and temperature. */
export function Heat({ step }: { step: StepForm }) {
  const { t } = useTranslation("editorLists");
  const { update } = useEditor();
  const patch = (change: Partial<StepForm>) =>
    update((f) => updateStep(f, step.ref, change));
  const units = (Object.keys(TEMPERATURE_UNITS) as TemperatureUnit[])
    .filter((u) => u !== "K" || step.tempUnit === "K")
    .map((u) => ({ value: u, label: TEMPERATURE_UNITS[u] }));
  const heatOptions: { value: HeatLevel | ""; label: string }[] = [
    { value: "", label: "—" },
    ...HEAT_LEVELS.map((h) => ({ value: h, label: t(HEAT_LABELS[h]) })),
  ];
  const targetOptions: { value: TemperatureTarget | ""; label: string }[] = [
    { value: "", label: "—" },
    ...TEMPERATURE_TARGETS.map((x) => ({
      value: x,
      label: t(TARGET_LABELS[x]),
    })),
  ];
  return (
    <div className="stack-sm">
      <SelectField
        id={fieldId.step(step.ref, "heat")}
        label={t("heat.label")}
        value={step.heat}
        options={heatOptions}
        onChange={(heat) => patch({ heat })}
      />
      <div className="temp-row">
        <TextField
          id={fieldId.step(step.ref, "temp-min")}
          label={t("temp.min")}
          placeholder="180"
          inputMode="decimal"
          value={step.tempMin}
          onChange={(tempMin) => patch({ tempMin })}
        />
        <TextField
          id={fieldId.step(step.ref, "temp-max")}
          label={t("temp.max")}
          placeholder="200"
          inputMode="decimal"
          value={step.tempMax}
          onChange={(tempMax) => patch({ tempMax })}
        />
        <SelectField
          id={`f-step-${step.ref}-temp-unit`}
          label={t("temp.unit")}
          value={step.tempUnit}
          options={units}
          onChange={(tempUnit) => patch({ tempUnit })}
        />
        <SelectField
          id={fieldId.step(step.ref, "temp-target")}
          label={t("temp.target")}
          value={step.tempTarget}
          options={targetOptions}
          onChange={(tempTarget) => patch({ tempTarget })}
        />
      </div>
    </div>
  );
}
