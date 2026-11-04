import { describe, expect, test } from "bun:test";
import {
  formatBytes,
  MIN_PASSPHRASE,
  normalizeServerUrl,
  passphraseProblem,
  passphraseStrength,
  truncateId,
  usageSummary,
} from "./settingsModel";

describe("usageSummary", () => {
  const usage = (bytes: number, bundles: number) => ({
    bytes_used: bytes,
    bytes_limit: 256 * 1024 * 1024,
    bundles_used: bundles,
    bundles_limit: 5000,
  });

  test("reads used against the limit", () => {
    const summary = usageSummary(usage(12 * 1024 * 1024, 14));
    expect(summary.bytes).toBe("12 MB of 256 MB");
    expect(summary.revisions).toBe("14 of 5000");
    expect(summary.nearlyFull).toBe(false);
  });

  test("warns once either quota is nine-tenths used", () => {
    expect(usageSummary(usage(240 * 1024 * 1024, 0)).nearlyFull).toBe(true);
    expect(usageSummary(usage(0, 4500)).nearlyFull).toBe(true);
    expect(usageSummary(usage(0, 4499)).nearlyFull).toBe(false);
  });

  test("a zero limit counts as full", () => {
    expect(
      usageSummary({
        bytes_used: 0,
        bytes_limit: 0,
        bundles_used: 0,
        bundles_limit: 1,
      }).nearlyFull,
    ).toBe(true);
  });
});

describe("passphraseStrength", () => {
  test("empty and short passphrases score zero with a hint", () => {
    expect(passphraseStrength("").score).toBe(0);
    const short = passphraseStrength("abc");
    expect(short).toMatchObject({ score: 0, label: "Too short" });
    expect(short.hint).toBe("5 more characters needed.");
    expect(passphraseStrength("abcdefg").hint).toBe("1 more character needed.");
  });

  test("common words and repeats are weak", () => {
    expect(passphraseStrength("password1").score).toBe(1);
    expect(passphraseStrength("aaaaaaaaaaaa").score).toBe(1);
  });

  test("length and variety raise the score", () => {
    expect(passphraseStrength("abcdefgh").score).toBe(1);
    expect(passphraseStrength("abcdefghijkl").score).toBe(2);
    expect(passphraseStrength("Abcdefghijk1").score).toBe(3);
    const strong = passphraseStrength("correct horse battery staple");
    expect(strong).toMatchObject({ score: 4, label: "Strong" });
  });

  test("scores are monotonic in the meter range", () => {
    for (const p of ["", "a", "abcdefgh", "Tr0ub4dor&3xyz", "x".repeat(40)]) {
      const { score } = passphraseStrength(p);
      expect(score).toBeGreaterThanOrEqual(0);
      expect(score).toBeLessThanOrEqual(4);
    }
  });
});

describe("passphraseProblem", () => {
  test("enforces the minimum and the confirmation", () => {
    expect(passphraseProblem("short", "short")).toContain(
      String(MIN_PASSPHRASE),
    );
    expect(passphraseProblem("long enough", "long enougH")).toBe(
      "The passphrases don’t match.",
    );
    expect(passphraseProblem("long enough", "long enough")).toBeNull();
  });
});

describe("formatting", () => {
  test("truncates long ids only", () => {
    expect(truncateId("usr123")).toBe("usr123");
    expect(truncateId("usrABCDEFGHIJKLMNOPQRSTUVWXYZ")).toBe(
      "usrABCDEFG…UVWXYZ",
    );
  });

  test("formats byte counts", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(50 * 1024 * 1024)).toBe("50 MB");
    expect(formatBytes(Number.NaN)).toBe("—");
  });

  test("normalizes server URLs", () => {
    expect(normalizeServerUrl("http://localhost:8080/")).toBe(
      "http://localhost:8080",
    );
    expect(normalizeServerUrl("recipes.example.com")).toBe(
      "https://recipes.example.com",
    );
    expect(normalizeServerUrl("https://x.org/api/")).toBe("https://x.org/api");
    expect(normalizeServerUrl("ftp://x.org")).toBeNull();
    expect(normalizeServerUrl("not a url at all")).toBeNull();
    expect(normalizeServerUrl("http://[::1]:8080")).toBe("http://[::1]:8080");
    expect(normalizeServerUrl("  ")).toBeNull();
  });
});
