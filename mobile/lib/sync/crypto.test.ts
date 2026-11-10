import { describe, expect, test } from 'bun:test';

import vectors from '../../../docs/sync-vectors.json';
import { SyncCrypto, fromHex, fromUtf8, toHex, utf8 } from './crypto';

const crypto = new SyncCrypto(globalThis.crypto);

describe('sync vectors', () => {
  test('keys from the password', async () => {
    const v = vectors.kdf;
    expect(SyncCrypto.salt(v.email)).toBe(v.salt);
    const keys = await crypto.deriveKeys(v.email, v.password, v.iterations);
    expect(toHex(keys.masterKey)).toBe(v.masterKey);
    expect(toHex(keys.authKey)).toBe(v.authKey);
    expect(keys.authPassword).toBe(v.authPassword);
    expect(toHex(keys.wrapKey)).toBe(v.wrapKey);
  });

  test('wrapped vault key', async () => {
    const v = vectors.vaultKey;
    const wrapKey = fromHex(vectors.kdf.wrapKey);
    const wrapped = await crypto.wrapVaultKey(wrapKey, fromHex(v.key), fromHex(v.nonce));
    expect(wrapped).toBe(v.wrappedVaultKey);
    expect(toHex(await crypto.unwrapVaultKey(wrapKey, v.wrappedVaultKey))).toBe(v.key);
  });

  test('object blob', async () => {
    const v = vectors.object;
    const padded = SyncCrypto.pad(v.plaintext);
    expect(padded.length).toBe(v.paddedLength);
    const payload = JSON.parse(v.plaintext);
    const blob = await crypto.sealObject(fromHex(v.key), v.id, payload, fromHex(v.nonce));
    expect(blob).toBe(v.blob);
    expect(await crypto.openObject(fromHex(v.key), v.id, v.blob)).toEqual(payload);
  });

  test('open refuses another version byte and another aad', async () => {
    const v = vectors.object;
    const bytes = Uint8Array.from(atob(v.blob), (c) => c.charCodeAt(0));
    bytes[0] = 2;
    const other = btoa(String.fromCharCode(...bytes));
    await expect(crypto.open(fromHex(v.key), other, v.id)).rejects.toThrow();
    await expect(crypto.open(fromHex(v.key), v.blob, 'another-id')).rejects.toThrow();
  });

  test('padding fills to the next 1024 with spaces', () => {
    const p = SyncCrypto.pad('x'.repeat(1025));
    expect(p.length).toBe(2048);
    expect(fromUtf8(p).trimEnd()).toBe('x'.repeat(1025));
    expect(SyncCrypto.pad('{}').length).toBe(1024);
    expect(utf8('é').length).toBe(2);
  });
});
