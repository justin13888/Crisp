/**
 * Pure helpers behind the settings screen: passphrase rules and strength,
 * id truncation, byte formatting, and server URLs.
 */

/** The shortest passphrase accepted. */
export const MIN_PASSPHRASE = 8;

/** How strong a passphrase looks. */
export interface PassphraseStrength {
  /** 0 (empty/too short) to 4 (strong). */
  score: 0 | 1 | 2 | 3 | 4;
  /** A word for the meter. */
  label: string;
  /** A tip to make it stronger, or a reassurance. */
  hint: string;
}

const COMMON = [
  "password",
  "passphrase",
  "12345678",
  "qwerty",
  "letmein",
  "iloveyou",
  "crisp",
  "recipe",
];

const LABELS = ["", "Weak", "Fair", "Good", "Strong"] as const;

/**
 * A rough strength estimate: length and variety, penalizing repeats and
 * well-known passwords. Advisory only — the minimum length is the rule.
 */
export function passphraseStrength(passphrase: string): PassphraseStrength {
  if (passphrase.length === 0) {
    return {
      score: 0,
      label: "",
      hint: `At least ${MIN_PASSPHRASE} characters.`,
    };
  }
  if (passphrase.length < MIN_PASSPHRASE) {
    const missing = MIN_PASSPHRASE - passphrase.length;
    return {
      score: 0,
      label: "Too short",
      hint: `${missing} more character${missing === 1 ? "" : "s"} needed.`,
    };
  }
  const lower = passphrase.toLowerCase();
  if (passphrase.length < 16 && COMMON.some((word) => lower.includes(word))) {
    return {
      score: 1,
      label: "Weak",
      hint: "Avoid common words like “password”.",
    };
  }
  if (/^(.)\1+$/.test(passphrase)) {
    return { score: 1, label: "Weak", hint: "Avoid repeating one character." };
  }
  const classes = [/[a-z]/, /[A-Z]/, /[0-9]/, /[^A-Za-z0-9]/].filter((re) =>
    re.test(passphrase),
  ).length;
  const words = passphrase.trim().split(/\s+/).length;
  let points = 1;
  if (passphrase.length >= 12) points++;
  if (passphrase.length >= 16) points++;
  if (passphrase.length >= 24) points++;
  if (classes >= 3) points++;
  if (words >= 4) points++;
  const score = Math.min(4, points) as 1 | 2 | 3 | 4;
  const hint =
    score >= 4
      ? "Great — now keep it somewhere safe."
      : "Longer is stronger: try four random words.";
  return { score, label: LABELS[score], hint };
}

/** Why a new passphrase is not acceptable yet, or `null` when it is. */
export function passphraseProblem(
  passphrase: string,
  confirm: string,
): string | null {
  if (passphrase.length < MIN_PASSPHRASE) {
    return `Use at least ${MIN_PASSPHRASE} characters.`;
  }
  if (passphrase !== confirm) return "The passphrases don’t match.";
  return null;
}

/** "usr1234…cdef" — the start and end of a long id. */
export function truncateId(id: string, head = 10, tail = 6): string {
  if (id.length <= head + tail + 1) return id;
  return `${id.slice(0, head)}…${id.slice(-tail)}`;
}

/** 1536 → "1.5 KB"; binary multiples, one decimal under 10. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = unit === 0 || value >= 10 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

/** Normalizes a typed server URL; `null` when it is not http(s). */
export function normalizeServerUrl(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed)
    ? trimmed
    : `https://${trimmed}`;
  try {
    const url = new URL(withScheme);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    if (!/^([a-z0-9-]+\.)*[a-z0-9-]+$|^\[[0-9a-f:.]+\]$/i.test(url.hostname)) {
      return null;
    }
    return `${url.origin}${url.pathname}`.replace(/\/+$/, "");
  } catch {
    return null;
  }
}

/**
 * A `0xMMmmpp` format version as `major.minor.patch` (spec §14), for a
 * server's `ServerInfoView`, which carries only the encoded numbers. The
 * engine's own versions come pre-written in `CoreInfo`.
 */
export function formatVersion(encoded: number): string {
  if (!Number.isInteger(encoded) || encoded < 0) return String(encoded);
  const major = Math.floor(encoded / 0x10000) & 0xff;
  const minor = (encoded >> 8) & 0xff;
  const patch = encoded & 0xff;
  return `${major}.${minor}.${patch}`;
}
