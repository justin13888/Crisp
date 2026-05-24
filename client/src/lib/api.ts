/**
 * The Crisp server API client.
 *
 * Wraps the `/api/v1` HTTP surface: crypto-bound auth (the passphrase never
 * leaves the device — only signatures do), bundle publication and retrieval,
 * search, and version negotiation. All format/crypto work is delegated to a
 * [`CrispIdentity`] from `lib/crisp.ts`.
 */

import type { CrispIdentity } from "./crisp";
import type { ComplianceReport } from "./types";

/** Hex-encodes bytes. */
export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Hex-decodes a string into bytes. */
export function fromHex(hex: string): Uint8Array {
  const clean = hex.trim();
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** The server's advertised version and key information. */
export interface ServerInfo {
  /** Supported API versions. */
  api_versions: string[];
  /** The supported format-version window. */
  format_version: {
    current: number;
    min_supported: number;
    max_supported: number;
  };
  /** The platform endorsement keys, hex-encoded. */
  platform_keys: { usr_id: string; ed25519: string; ml_dsa_65: string }[];
  /** The server's `crisp-core` build version. */
  core_version: string;
}

/** A search/browse result row. */
export interface RecipeSummary {
  /** The owning bundle's document id. */
  doc_id: string;
  /** The recipe id. */
  recipe_id: string;
  /** The recipe revision id. */
  recipe_revision: string;
  /** The recipe title. */
  title: string;
  /** The recipe language. */
  language: string;
  /** An optional summary. */
  summary: string | null;
  /** Keyword tags. */
  tags: string[];
  /** Cuisine tags. */
  cuisine: string[];
  /** Diet classifications. */
  diet: string[];
  /** When last modified (epoch seconds). */
  modified_at: number;
}

/** An authenticated session. */
export interface Session {
  /** The authenticated `usr…` id. */
  usrId: string;
  /** The bearer token. */
  token: string;
  /** When the token expires (epoch seconds). */
  expiresAt: number;
}

/** The outcome of a publish attempt. */
export type PublishResult =
  | { ok: true; endorsed: Uint8Array }
  | { ok: false; report: ComplianceReport | null; message: string };

/** Faceted search parameters. */
export interface SearchParams {
  /** Free-text query. */
  q?: string;
  /** Required cuisine facets. */
  cuisine?: string[];
  /** Required diet facets. */
  diet?: string[];
  /** Allergens to exclude. */
  allergenFree?: string[];
  /** A language filter. */
  language?: string;
}

/** A typed client for one Crisp server. */
export class ServerClient {
  /** The server base URL, without a trailing slash. */
  readonly baseUrl: string;
  #token: string | null = null;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  /** Sets (or clears) the bearer token used for authenticated requests. */
  setToken(token: string | null): void {
    this.#token = token;
  }

  /** Builds request headers, including auth when a token is set. */
  #headers(extra?: Record<string, string>): Record<string, string> {
    const headers: Record<string, string> = { ...extra };
    if (this.#token) headers["authorization"] = `Bearer ${this.#token}`;
    return headers;
  }

  /** Fetches the server's version and key advertisement. */
  async serverInfo(): Promise<ServerInfo> {
    const response = await fetch(`${this.baseUrl}/api/v1/server-info`);
    if (!response.ok) throw new Error(`server-info failed: ${response.status}`);
    return (await response.json()) as ServerInfo;
  }

  /** Registers a crypto-bound account from a self-signed profile; returns the
   * derived `usr…` id. The username and display name are carried inside the
   * profile, signed by the identity's key, so the server never trusts an
   * unsigned name. */
  async register(
    identity: CrispIdentity,
    username: string,
    displayName: string,
  ): Promise<string> {
    const profile = identity.signProfile(username, displayName);
    const response = await fetch(`${this.baseUrl}/api/v1/auth/register`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ profile: toHex(profile) }),
    });
    if (!response.ok) throw new Error(await errorMessage(response));
    const body = (await response.json()) as { usr_id: string };
    return body.usr_id;
  }

  /** Fetches an account's `signed-profile` bytes, or `null` if unknown. */
  async accountProfile(usrId: string): Promise<Uint8Array | null> {
    const response = await fetch(
      `${this.baseUrl}/api/v1/accounts/${encodeURIComponent(usrId)}`,
    );
    if (response.status === 404) return null;
    if (!response.ok) {
      throw new Error(`account fetch failed: ${response.status}`);
    }
    return new Uint8Array(await response.arrayBuffer());
  }

  /** Performs the signed-challenge login handshake and stores the token. */
  async login(username: string, identity: CrispIdentity): Promise<Session> {
    const challengeResponse = await fetch(
      `${this.baseUrl}/api/v1/auth/challenge`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ username }),
      },
    );
    if (!challengeResponse.ok)
      throw new Error(await errorMessage(challengeResponse));
    const challenge = (await challengeResponse.json()) as {
      nonce: string;
      message: string;
    };

    const signature = identity.signMessage(
      new TextEncoder().encode(challenge.message),
    );
    const loginResponse = await fetch(`${this.baseUrl}/api/v1/auth/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        nonce: challenge.nonce,
        signature: toHex(signature),
      }),
    });
    if (!loginResponse.ok) throw new Error(await errorMessage(loginResponse));
    const body = (await loginResponse.json()) as {
      usr_id: string;
      token: string;
      expires_at: number;
    };
    this.#token = body.token;
    return {
      usrId: body.usr_id,
      token: body.token,
      expiresAt: body.expires_at,
    };
  }

  /** Ends the current session. */
  async logout(): Promise<void> {
    if (!this.#token) return;
    await fetch(`${this.baseUrl}/api/v1/auth/logout`, {
      method: "POST",
      headers: this.#headers(),
    });
    this.#token = null;
  }

  /** Publishes an author-signed bundle; the server endorses and stores it. */
  async publish(bytes: Uint8Array): Promise<PublishResult> {
    const response = await fetch(`${this.baseUrl}/api/v1/bundles`, {
      method: "POST",
      headers: this.#headers({ "content-type": "application/crisp+cbor" }),
      // A `Uint8Array` is a valid request body; the wrapping `Blob` keeps the
      // bound buffer type explicit for TypeScript's DOM `BodyInit`.
      body: new Blob([bytes as BlobPart]),
    });
    if (response.ok) {
      return {
        ok: true,
        endorsed: new Uint8Array(await response.arrayBuffer()),
      };
    }
    const body = (await response.json().catch(() => null)) as {
      message?: string;
      report?: ComplianceReport;
    } | null;
    return {
      ok: false,
      report: body?.report ?? null,
      message: body?.message ?? `publish failed: ${response.status}`,
    };
  }

  /** Fetches the latest endorsed bundle for a document id. */
  async fetchBundle(docId: string): Promise<Uint8Array | null> {
    const response = await fetch(
      `${this.baseUrl}/api/v1/bundles/${encodeURIComponent(docId)}`,
    );
    if (response.status === 404) return null;
    if (!response.ok) throw new Error(`fetch failed: ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  }

  /** Lists indexed recipes, newest first. */
  async browse(): Promise<RecipeSummary[]> {
    const response = await fetch(`${this.baseUrl}/api/v1/recipes`);
    if (!response.ok) throw new Error(`browse failed: ${response.status}`);
    const body = (await response.json()) as { recipes: RecipeSummary[] };
    return body.recipes;
  }

  /** Runs a full-text and faceted recipe search. */
  async search(params: SearchParams): Promise<RecipeSummary[]> {
    const query = new URLSearchParams();
    if (params.q) query.set("q", params.q);
    if (params.language) query.set("language", params.language);
    for (const c of params.cuisine ?? []) query.append("cuisine", c);
    for (const d of params.diet ?? []) query.append("diet", d);
    for (const a of params.allergenFree ?? []) query.append("allergen_free", a);
    const response = await fetch(
      `${this.baseUrl}/api/v1/search?${query.toString()}`,
    );
    if (!response.ok) throw new Error(`search failed: ${response.status}`);
    const body = (await response.json()) as { recipes: RecipeSummary[] };
    return body.recipes;
  }
}

/** Extracts a human-readable message from a failed JSON response. */
async function errorMessage(response: Response): Promise<string> {
  const body = (await response.json().catch(() => null)) as {
    message?: string;
  } | null;
  return body?.message ?? `request failed: ${response.status}`;
}
