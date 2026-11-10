// Sign-up, sign-in and the password steps of docs/protocol.md "Keys" and "API". The password never
// leaves this module: the server sees base64url(authKey) only.

import type { Api } from './api';
import type { SyncCrypto } from './crypto';

export type Session = { email: string; token: string; vaultKey: Uint8Array };

export async function signUp(api: Api, crypto: SyncCrypto, email: string, password: string): Promise<Session> {
  const keys = await crypto.deriveKeys(email, password);
  const token = await api.signUp(email, keys.authPassword);
  const vaultKey = crypto.random(32);
  await api.putKeys(await crypto.wrapVaultKey(keys.wrapKey, vaultKey));
  return { email: email.trim().toLowerCase(), token, vaultKey };
}

export async function signIn(api: Api, crypto: SyncCrypto, email: string, password: string): Promise<Session> {
  const keys = await crypto.deriveKeys(email, password);
  const token = await api.signIn(email, keys.authPassword);
  const wrapped = await api.keys();
  if (wrapped) {
    return { email: email.trim().toLowerCase(), token, vaultKey: await crypto.unwrapVaultKey(keys.wrapKey, wrapped) };
  }
  // Sign-up was cut short before the key was put.
  const vaultKey = crypto.random(32);
  await api.putKeys(await crypto.wrapVaultKey(keys.wrapKey, vaultKey));
  return { email: email.trim().toLowerCase(), token, vaultKey };
}

/** Re-wraps the vault key under the new password's wrap key. Other sessions end. */
export async function changePassword(
  api: Api,
  crypto: SyncCrypto,
  email: string,
  current: string,
  next: string,
  vaultKey: Uint8Array,
): Promise<void> {
  const [before, after] = [await crypto.deriveKeys(email, current), await crypto.deriveKeys(email, next)];
  await api.changePassword(before.authPassword, after.authPassword, await crypto.wrapVaultKey(after.wrapKey, vaultKey));
}

export async function deleteAccount(api: Api, crypto: SyncCrypto, email: string, password: string): Promise<void> {
  const keys = await crypto.deriveKeys(email, password);
  await api.deleteAccount(keys.authPassword);
}
