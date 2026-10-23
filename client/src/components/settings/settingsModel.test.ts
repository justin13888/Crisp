import { describe, expect, test } from "bun:test";
import {
  formatBytes,
  formatVersion,
  MIN_PASSPHRASE,
  normalizeServerUrl,
  passphraseProblem,
  passphraseStrength,
  truncateId,
} from "./settingsModel";

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

describe("formatVersion", () => {
  test("decodes 0xMMmmpp", () => {
    expect(formatVersion(0x010000)).toBe("1.0.0");
    expect(formatVersion(0x010203)).toBe("1.2.3");
    expect(formatVersion(-1)).toBe("-1");
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
