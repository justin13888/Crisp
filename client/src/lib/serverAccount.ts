/**
 * What a signed-in account does with the server beyond publishing: signing
 * in with the key file, removing what it published, and deleting itself.
 * The protocol is crisp-wasm's; this module only orders the calls and
 * words the refusals for the cook.
 */

import { errorMessage, isCrispFailure, type Session } from "./crisp";

/** The failure `code` of an unpublish of another account's revisions. */
export const FORBIDDEN = "forbidden";
/** The failure `code` of an account deletion with a session too old. */
export const REAUTHENTICATION_REQUIRED = "reauthentication-required";

/** The server calls a key-file sign-in needs. */
export interface LoginServer {
  challenge(username: string): Promise<{ nonce: string; audience: string }>;
  login(nonce: string, signature: Uint8Array): Promise<Session>;
}

/** The key that answers a login challenge. */
export interface LoginSigner {
  signLogin(audience: string, nonce: string): Uint8Array;
}

/** Signs `username` in by answering a fresh challenge with `signer`. */
export async function logIn(
  server: LoginServer,
  signer: LoginSigner,
  username: string,
): Promise<Session> {
  const challenge = await server.challenge(username);
  const signature = signer.signLogin(challenge.audience, challenge.nonce);
  return server.login(challenge.nonce, signature);
}

/** The server calls deleting an account needs. */
export interface DeletionServer extends LoginServer {
  setToken(token?: string | null): void;
  deleteAccount(): Promise<void>;
}

/**
 * Deletes `username`'s account with a session made for the purpose: the
 * server accepts only a recent sign-in, so the key file signs in again
 * first and the deletion uses that new session.
 */
export async function deleteAccountFreshly(
  server: DeletionServer,
  signer: LoginSigner,
  username: string,
): Promise<void> {
  const session = await logIn(server, signer, username);
  server.setToken(session.token);
  await server.deleteAccount();
}

/** Why an unpublish failed, for the cook. */
export function unpublishFailureMessage(cause: unknown): string {
  if (isCrispFailure(cause)) {
    if (cause.code === FORBIDDEN) {
      return "another account published what is on the server, so only it can remove it.";
    }
    if (cause.status === 404) {
      return "the server no longer has it.";
    }
  }
  return errorMessage(cause);
}

/** Why an account deletion failed, for the cook. */
export function deleteAccountFailureMessage(cause: unknown): string {
  if (isCrispFailure(cause) && cause.code === REAUTHENTICATION_REQUIRED) {
    return "the server wants a newer sign-in. Try again; this device signs in afresh each time.";
  }
  return errorMessage(cause);
}
