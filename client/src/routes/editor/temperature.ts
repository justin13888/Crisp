/**
 * Reading one typed temperature bound in the editor. The engine decides what
 * counts as a temperature; the step's unit picker decides the unit.
 */

import type {
  ParsedTemperature,
  Rational,
  TemperatureUnit,
} from "../../generated/crisp";
import { TEMPERATURE_UNITS } from "./model";

/** The engine call {@link readDegrees} needs. */
export interface TemperatureEngine {
  /** Parses a typed temperature (`"180"`, `"180 °C"`); throws otherwise. */
  parseTemperature(text: string, lang: string): ParsedTemperature;
}

/** A read bound, or the message saying why `typed` was refused. */
export type DegreesResult = { value: Rational } | { problem: string };

/**
 * Reads `typed` (already trimmed, non-empty) as a single bound in `unit`.
 * A range is refused, and so is a typed unit that disagrees with `unit`:
 * saving "350 °F" as 350 °C would silently change the recipe.
 */
export function readDegrees(
  engine: TemperatureEngine,
  typed: string,
  unit: TemperatureUnit,
  lang: string,
): DegreesResult {
  let parsed: ParsedTemperature;
  try {
    parsed = engine.parseTemperature(typed, lang);
  } catch {
    return { problem: `"${typed}" is not a number.` };
  }
  if (parsed.unit !== undefined && parsed.unit !== unit) {
    const typedUnit = TEMPERATURE_UNITS[parsed.unit];
    return {
      problem: `"${typed}" is in ${typedUnit}, but the step is set to ${TEMPERATURE_UNITS[unit]} — change the unit or the number.`,
    };
  }
  if (parsed.max !== undefined) {
    return { problem: `"${typed}" is not a number.` };
  }
  return { value: parsed.min };
}
