/** Narrows an untrusted value to a plain (non-null, non-array) object. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
