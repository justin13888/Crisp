import { useBlobUrl } from "../../hooks/useBlobUrl";
import type { Bundle, StepView } from "../../lib/crisp";
import { Icon } from "./Icon";
import { Paragraphs } from "./RichText";
import { StepTimer } from "./StepTimer";

/** Heat, temperature, and "makes" badges for a step. */
export function StepBadges({
  step,
  size = "sm",
}: {
  step: Pick<StepView, "heat" | "temperature" | "produces">;
  size?: "sm" | "lg";
}) {
  if (!step.heat && !step.temperature && !step.produces) return null;
  return (
    <div className={`step-badges step-badges--${size}`}>
      {step.temperature && (
        <span className="step-badge step-badge--temp">
          <Icon name="thermo" />
          {step.temperature}
        </span>
      )}
      {step.heat && (
        <span className="step-badge step-badge--heat">
          <Icon name="flame" />
          {step.heat}
        </span>
      )}
      {step.produces && (
        <span className="step-badge step-badge--produces">
          Makes {step.produces}
        </span>
      )}
    </div>
  );
}

/** The ingredients a step uses, with scaled amounts. */
export function StepIngredientsInline({ step }: { step: StepView }) {
  if (step.ingredients.length === 0) return null;
  return (
    <ul className="step-ingredients" aria-label="Uses">
      {step.ingredients.map((item, i) => (
        <li key={i}>
          {item.amount && <strong className="tabular">{item.amount}</strong>}{" "}
          {item.name}
          {item.preparation && (
            <span className="muted">, {item.preparation}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

/** An image from the open bundle (renders nothing until it is ready). */
export function BundleImage({
  bundle,
  id,
  alt,
  className,
}: {
  bundle: Bundle;
  id: string | null | undefined;
  alt: string;
  className?: string;
}) {
  const url = useBlobUrl(bundle, id);
  if (!url) return null;
  return <img className={className} src={url} alt={alt} decoding="async" />;
}

/** The recipe page's step list. */
export function StepList({
  steps,
  docId,
  title,
  bundle,
}: {
  steps: readonly StepView[];
  docId: string;
  title: string;
  bundle: Bundle;
}) {
  return (
    <ol className="step-list clean">
      {steps.map((step, i) => {
        const section =
          step.section && step.section !== steps[i - 1]?.section
            ? step.section
            : null;
        return (
          <li key={step.ref} className="step-item">
            {section && <h3 className="section-title">{section}</h3>}
            <div className="step">
              <span className="num" aria-hidden="true">
                {step.number}
              </span>
              <div className="step__body stack-sm">
                <span className="visually-hidden">Step {step.number}. </span>
                <StepBadges step={step} />
                <div className="step__text">
                  <Paragraphs paragraphs={step.text} />
                </div>
                <StepIngredientsInline step={step} />
                {step.note.length > 0 && (
                  <div className="callout">
                    <Paragraphs paragraphs={step.note} />
                  </div>
                )}
                {step.image && (
                  <BundleImage
                    bundle={bundle}
                    id={step.image}
                    alt={`Step ${step.number}`}
                    className="step__image"
                  />
                )}
                {step.timers.length > 0 && (
                  <div className="cluster step__timers">
                    {step.timers.map((timer, index) => (
                      <StepTimer
                        key={index}
                        target={{ docId, title, step, index, timer }}
                      />
                    ))}
                  </div>
                )}
              </div>
            </div>
          </li>
        );
      })}
    </ol>
  );
}
