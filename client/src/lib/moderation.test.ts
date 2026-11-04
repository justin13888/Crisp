import { describe, expect, test } from "bun:test";
import { abuseHref, termsHref } from "./moderation";

describe("termsHref", () => {
  test("links an http(s) URL and nothing else", () => {
    expect(termsHref("https://crisp.example/terms")).toBe(
      "https://crisp.example/terms",
    );
    expect(termsHref(" http://crisp.example/t ")).toBe(
      "http://crisp.example/t",
    );
    expect(termsHref(undefined)).toBeNull();
    expect(termsHref("")).toBeNull();
    expect(termsHref("javascript:alert(1)")).toBeNull();
    expect(termsHref("crisp.example/terms")).toBeNull();
  });
});

describe("abuseHref", () => {
  test("names the document in an e-mail's subject", () => {
    expect(abuseHref("mailto:abuse@crisp.example", "rcp1a")).toBe(
      "mailto:abuse@crisp.example?subject=Report%3A%20rcp1a",
    );
    expect(abuseHref("abuse@crisp.example", "rcp1a")).toBe(
      "mailto:abuse@crisp.example?subject=Report%3A%20rcp1a",
    );
    expect(abuseHref("mailto:abuse@crisp.example?cc=x@y.z", "rcp1a")).toBe(
      "mailto:abuse@crisp.example?cc=x@y.z&subject=Report%3A%20rcp1a",
    );
  });

  test("links a web address as it is", () => {
    expect(abuseHref("https://crisp.example/report", "rcp1a")).toBe(
      "https://crisp.example/report",
    );
  });

  test("links nothing else", () => {
    expect(abuseHref(undefined, "rcp1a")).toBeNull();
    expect(abuseHref("  ", "rcp1a")).toBeNull();
    expect(abuseHref("javascript:alert(1)", "rcp1a")).toBeNull();
    expect(abuseHref("call +1 555 0100", "rcp1a")).toBeNull();
  });
});
