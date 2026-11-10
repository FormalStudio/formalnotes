// Every crypto step of docs/protocol.md "Keys" and "Objects", over WebCrypto. On device the subtle
// implementation is react-native-quick-crypto's (native PBKDF2), under bun test the runtime's own.

export interface CryptoBackend {
  subtle: SubtleCrypto;
  getRandomValues(array: Uint8Array<ArrayBuffer>): unknown;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export const utf8 = (s: string): Uint8Array<ArrayBuffer> => encoder.encode(s) as Uint8Array<ArrayBuffer>;
export const fromUtf8 = (b: Uint8Array): string => decoder.decode(b);

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

export function toBase64(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    out += B64[(n >> 18) & 63] + B64[(n >> 12) & 63];
    out += i + 1 < bytes.length ? B64[(n >> 6) & 63] : '=';
    out += i + 2 < bytes.length ? B64[n & 63] : '=';
  }
  return out;
}

export function fromBase64(s: string): Uint8Array<ArrayBuffer> {
  const clean = s.replace(/-/g, '+').replace(/_/g, '/').replace(/=+$/, '');
  const out = new Uint8Array(Math.floor((clean.length * 3) / 4));
  let bits = 0;
  let acc = 0;
  let o = 0;
  for (const c of clean) {
    const v = B64.indexOf(c);
    if (v < 0) throw new Error('bad base64');
    acc = (acc << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[o++] = (acc >> bits) & 0xff;
    }
  }
  return out;
}

export const toBase64Url = (bytes: Uint8Array): string =>
  toBase64(bytes).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

export const toHex = (bytes: Uint8Array): string =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');

export function fromHex(hex: string): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

export const PBKDF2_ITERATIONS = 600000;
const SEAL_VERSION = 0x01;
const PAD = 1024;

export type Keys = { masterKey: Uint8Array; authKey: Uint8Array; authPassword: string; wrapKey: Uint8Array };

export class SyncCrypto {
  constructor(private readonly backend: CryptoBackend) {}

  random(n: number): Uint8Array<ArrayBuffer> {
    const out = new Uint8Array(n);
    this.backend.getRandomValues(out);
    return out;
  }

  async sha256Hex(text: string): Promise<string> {
    return toHex(new Uint8Array(await this.backend.subtle.digest('SHA-256', utf8(text))));
  }

  /** The salt is the trimmed, lowercased email behind a fixed prefix. */
  static salt(email: string): string {
    return `formalnotes:v1:${email.trim().toLowerCase()}`;
  }

  async deriveKeys(email: string, password: string, iterations = PBKDF2_ITERATIONS): Promise<Keys> {
    const { subtle } = this.backend;
    const base = await subtle.importKey('raw', utf8(password), 'PBKDF2', false, ['deriveBits']);
    const masterKey = new Uint8Array(
      await subtle.deriveBits(
        { name: 'PBKDF2', hash: 'SHA-256', salt: utf8(SyncCrypto.salt(email)), iterations },
        base,
        256,
      ),
    );
    const [authKey, wrapKey] = await Promise.all([
      this.hkdf(masterKey, 'formalnotes auth v1'),
      this.hkdf(masterKey, 'formalnotes wrap v1'),
    ]);
    return { masterKey, authKey, authPassword: toBase64Url(authKey), wrapKey };
  }

  private async hkdf(ikm: Uint8Array<ArrayBuffer>, info: string): Promise<Uint8Array<ArrayBuffer>> {
    const { subtle } = this.backend;
    const key = await subtle.importKey('raw', ikm, 'HKDF', false, ['deriveBits']);
    const bits = await subtle.deriveBits(
      { name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array(0), info: utf8(info) },
      key,
      256,
    );
    return new Uint8Array(bits);
  }

  /** AES-256-GCM, laid out as `0x01 || nonce || ciphertext || tag`, in standard base64. */
  async seal(key: Uint8Array, plaintext: Uint8Array, aad: string, nonce: Uint8Array = this.random(12)): Promise<string> {
    const k = await this.backend.subtle.importKey('raw', key as Uint8Array<ArrayBuffer>, 'AES-GCM', false, ['encrypt']);
    const sealed = new Uint8Array(
      await this.backend.subtle.encrypt(
        { name: 'AES-GCM', iv: nonce as Uint8Array<ArrayBuffer>, additionalData: utf8(aad), tagLength: 128 },
        k,
        plaintext as Uint8Array<ArrayBuffer>,
      ),
    );
    const out = new Uint8Array(1 + nonce.length + sealed.length);
    out[0] = SEAL_VERSION;
    out.set(nonce, 1);
    out.set(sealed, 1 + nonce.length);
    return toBase64(out);
  }

  async open(key: Uint8Array, blob: string, aad: string): Promise<Uint8Array> {
    const bytes = fromBase64(blob);
    if (bytes.length < 1 + 12 + 16 || bytes[0] !== SEAL_VERSION) throw new Error('unknown blob version');
    const k = await this.backend.subtle.importKey('raw', key as Uint8Array<ArrayBuffer>, 'AES-GCM', false, ['decrypt']);
    const plain = await this.backend.subtle.decrypt(
      { name: 'AES-GCM', iv: bytes.slice(1, 13), additionalData: utf8(aad), tagLength: 128 },
      k,
      bytes.slice(13),
    );
    return new Uint8Array(plain);
  }

  wrapVaultKey(wrapKey: Uint8Array, vaultKey: Uint8Array, nonce?: Uint8Array): Promise<string> {
    return this.seal(wrapKey, vaultKey, 'vault-key', nonce);
  }

  async unwrapVaultKey(wrapKey: Uint8Array, wrapped: string): Promise<Uint8Array> {
    const key = await this.open(wrapKey, wrapped, 'vault-key');
    if (key.length !== 32) throw new Error('vault key is not 32 bytes');
    return key;
  }

  /** The object's JSON in UTF-8, padded with spaces to the next multiple of 1024 bytes. */
  static pad(json: string): Uint8Array {
    const bytes = utf8(json);
    const length = Math.ceil(Math.max(bytes.length, 1) / PAD) * PAD;
    const out = new Uint8Array(length).fill(0x20);
    out.set(bytes);
    return out;
  }

  sealObject(vaultKey: Uint8Array, id: string, payload: ObjectPayload, nonce?: Uint8Array): Promise<string> {
    return this.seal(vaultKey, SyncCrypto.pad(JSON.stringify(payload)), id, nonce);
  }

  async openObject(vaultKey: Uint8Array, id: string, blob: string): Promise<ObjectPayload> {
    const raw = JSON.parse(fromUtf8(await this.open(vaultKey, blob, id))) as Record<string, unknown>;
    const kind = raw.kind;
    if ((kind !== 'note' && kind !== 'folder' && kind !== 'file') || typeof raw.path !== 'string') {
      throw new Error('object without a kind or a path');
    }
    return {
      v: 1,
      kind,
      path: raw.path,
      ...(typeof raw.text === 'string' ? { text: raw.text } : {}),
      created: typeof raw.created === 'number' ? raw.created : 0,
      modified: typeof raw.modified === 'number' ? raw.modified : 0,
    };
  }
}

export type ObjectPayload = {
  v: 1;
  kind: 'note' | 'folder' | 'file';
  path: string;
  text?: string;
  created: number;
  modified: number;
};
