// The client side of docs/protocol.md "Sync", in WebCrypto, against a running server.
//
//   bun scripts/roundtrip.ts [base URL]     (default https://notes.formalsnake.dev)
//
// It first reproduces docs/sync-vectors.json, then signs up a throwaway @example.com account, syncs two
// devices through it, changes the password and deletes the account. Any mismatch exits 1.

import { join } from "node:path";

const BASE = (process.argv[2] ?? "https://notes.formalsnake.dev").replace(/\/$/, "");
type Bytes = Uint8Array<ArrayBuffer>;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Encoding

const hex = (bytes: Bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
const fromHex = (text: string) => Uint8Array.from(text.match(/../g) ?? [], (pair) => parseInt(pair, 16));
const base64 = (bytes: Bytes) => Buffer.from(bytes).toString("base64");
const fromBase64 = (text: string): Bytes => new Uint8Array(Buffer.from(text, "base64"));
const base64url = (bytes: Bytes) => Buffer.from(bytes).toString("base64url");

// Keys: protocol "Keys", steps 1 to 3

interface Keys {
	masterKey: Bytes;
	authKey: Bytes;
	/** What the server receives as the password. */
	authPassword: string;
	wrapKey: Bytes;
}

async function deriveKeys(email: string, password: string, iterations = 600_000): Promise<Keys> {
	const salt = encoder.encode(`formalnotes:v1:${email.trim().toLowerCase()}`);
	const passwordKey = await crypto.subtle.importKey("raw", encoder.encode(password), "PBKDF2", false, ["deriveBits"]);
	const masterKey = new Uint8Array(await crypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt, iterations }, passwordKey, 256));
	const hkdfKey = await crypto.subtle.importKey("raw", masterKey, "HKDF", false, ["deriveBits"]);
	const hkdf = async (info: string) =>
		new Uint8Array(await crypto.subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt: new Uint8Array(0), info: encoder.encode(info) }, hkdfKey, 256));
	const authKey = await hkdf("formalnotes auth v1");
	return { masterKey, authKey, authPassword: base64url(authKey), wrapKey: await hkdf("formalnotes wrap v1") };
}

// seal and open: AES-256-GCM, laid out as 0x01 || nonce || ciphertext || tag

async function seal(key: Bytes, plaintext: Bytes, aad: string, nonce = crypto.getRandomValues(new Uint8Array(12))): Promise<Bytes> {
	const aesKey = await crypto.subtle.importKey("raw", key, "AES-GCM", false, ["encrypt"]);
	const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: encoder.encode(aad) }, aesKey, plaintext));
	return new Uint8Array([1, ...nonce, ...sealed]);
}

async function open(key: Bytes, blob: Bytes, aad: string): Promise<Bytes> {
	if (blob[0] !== 1) throw new Error(`unknown seal version ${blob[0]}`);
	const aesKey = await crypto.subtle.importKey("raw", key, "AES-GCM", false, ["decrypt"]);
	return new Uint8Array(await crypto.subtle.decrypt({ name: "AES-GCM", iv: blob.subarray(1, 13), additionalData: encoder.encode(aad) }, aesKey, blob.subarray(13)));
}

// Objects: the JSON padded with spaces to a multiple of 1024 bytes, sealed with aad = id

interface Content {
	v: 1;
	kind: "note" | "folder" | "file";
	path: string;
	text?: string;
	created: number;
	modified: number;
}

function pad(json: string): Bytes {
	const bytes = encoder.encode(json);
	const padded = new Uint8Array(Math.ceil(bytes.length / 1024) * 1024).fill(0x20);
	padded.set(bytes);
	return padded;
}

async function sealObject(vaultKey: Bytes, id: string, content: Content): Promise<string> {
	return base64(await seal(vaultKey, pad(JSON.stringify(content)), id));
}

async function openObject(vaultKey: Bytes, id: string, blob: string): Promise<Content> {
	return JSON.parse(decoder.decode(await open(vaultKey, fromBase64(blob), id)));
}

// Checks

let failed = false;

function check(name: string, ok: boolean, detail?: unknown) {
	console.log(`${ok ? "ok  " : "FAIL"} ${name}`);
	if (!ok) {
		failed = true;
		if (detail !== undefined) console.log(`     ${JSON.stringify(detail)}`);
	}
}

function same(name: string, actual: unknown, expected: unknown) {
	const ok = JSON.stringify(actual) === JSON.stringify(expected);
	check(name, ok, ok ? undefined : { actual, expected });
}

// Vectors

async function vectors() {
	const v = await Bun.file(join(import.meta.dir, "../../docs/sync-vectors.json")).json();
	const keys = await deriveKeys(v.kdf.email, v.kdf.password, v.kdf.iterations);
	same("vectors: masterKey", hex(keys.masterKey), v.kdf.masterKey);
	same("vectors: authKey", hex(keys.authKey), v.kdf.authKey);
	same("vectors: authPassword", keys.authPassword, v.kdf.authPassword);
	same("vectors: wrapKey", hex(keys.wrapKey), v.kdf.wrapKey);

	const wrapped = await seal(keys.wrapKey, fromHex(v.vaultKey.key), v.vaultKey.aad, fromHex(v.vaultKey.nonce));
	same("vectors: wrappedVaultKey", base64(wrapped), v.vaultKey.wrappedVaultKey);
	same("vectors: open wrappedVaultKey", hex(await open(keys.wrapKey, fromBase64(v.vaultKey.wrappedVaultKey), v.vaultKey.aad)), v.vaultKey.key);

	const padded = pad(v.object.plaintext);
	same("vectors: paddedLength", padded.length, v.object.paddedLength);
	same("vectors: object blob", base64(await seal(fromHex(v.object.key), padded, v.object.id, fromHex(v.object.nonce))), v.object.blob);
	const opened = decoder.decode(await open(fromHex(v.object.key), fromBase64(v.object.blob), v.object.id));
	same("vectors: open object", opened.trimEnd(), v.object.plaintext);
}

// The server

interface Change {
	id: string;
	seq: number;
	deleted: boolean;
	blob: string | null;
}

interface Result {
	id: string;
	status: "ok" | "conflict";
	seq: number;
	deleted?: boolean;
	blob?: string | null;
}

async function call(path: string, init: { method?: string; token?: string; body?: unknown } = {}) {
	const headers: Record<string, string> = {};
	if (init.token) headers.Authorization = `Bearer ${init.token}`;
	if (init.body !== undefined) headers["Content-Type"] = "application/json";
	const response = await fetch(`${BASE}${path}`, {
		method: init.method ?? "GET",
		headers,
		body: init.body === undefined ? undefined : JSON.stringify(init.body),
	});
	const text = await response.text();
	return { status: response.status, token: response.headers.get("set-auth-token"), body: text ? JSON.parse(text) : null };
}

async function signIn(email: string, keys: Keys) {
	return call("/api/auth/sign-in/email", { method: "POST", body: { email, password: keys.authPassword } });
}

async function push(token: string, changes: { id: string; base: number; deleted: boolean; blob: string | null }[]): Promise<Result[]> {
	const response = await call("/v1/sync", { method: "POST", token, body: { changes } });
	if (response.status !== 200) throw new Error(`push answered ${response.status} ${JSON.stringify(response.body)}`);
	return response.body.results;
}

/** Pulls until `more` is false, as a device does. */
async function pullAll(token: string, since: number, limit?: number) {
	const changes: Change[] = [];
	let cursor = since;
	let pages = 0;
	for (let more = true; more; pages++) {
		const response = await call(`/v1/sync?since=${cursor}${limit ? `&limit=${limit}` : ""}`, { token });
		if (response.status !== 200) throw new Error(`pull answered ${response.status} ${JSON.stringify(response.body)}`);
		changes.push(...response.body.changes);
		({ cursor, more } = response.body);
	}
	return { changes, cursor, pages };
}

async function server() {
	const health = await call("/health");
	same("GET /health", [health.status, health.body], [200, { ok: true }]);

	const email = `roundtrip-${crypto.randomUUID()}@example.com`;
	const password = `roundtrip ${crypto.randomUUID()}`;
	let keys = await deriveKeys(email, password);
	let token: string | null = null;

	try {
		// Sign-up is two calls: the account, then the wrapped vault key.
		const signUp = await call("/api/auth/sign-up/email", { method: "POST", body: { name: email.split("@")[0], email, password: keys.authPassword } });
		token = signUp.token;
		check("sign-up answers a session token", signUp.status === 200 && Boolean(token), signUp.status);
		if (!token) return;

		const noKeys = await call("/v1/keys", { token });
		same("GET /v1/keys before keys: 404 no_keys", [noKeys.status, noKeys.body?.error], [404, "no_keys"]);
		const vaultKey = crypto.getRandomValues(new Uint8Array(32));
		const wrappedVaultKey = base64(await seal(keys.wrapKey, vaultKey, "vault-key"));
		same("PUT /v1/keys: 204", (await call("/v1/keys", { method: "PUT", token, body: { wrappedVaultKey } })).status, 204);
		const again = await call("/v1/keys", { method: "PUT", token, body: { wrappedVaultKey } });
		same("PUT /v1/keys again: 409 keys_exist", [again.status, again.body?.error], [409, "keys_exist"]);

		// A second device signs in and unwraps the vault key.
		const second = await signIn(email, keys);
		const token2 = second.token;
		check("sign-in answers a session token", second.status === 200 && Boolean(token2), second.status);
		if (!token2) return;
		const fetched = await call("/v1/keys", { token: token2 });
		same("second device unwraps the vault key", hex(await open(keys.wrapKey, fromBase64(fetched.body.wrappedVaultKey), "vault-key")), hex(vaultKey));

		// Device 1 creates a note and a folder.
		const now = Date.now();
		const note: Content = { v: 1, kind: "note", path: "Roundtrip/Plan.md", text: "# Plan\n\nCafé ☕\n", created: now, modified: now };
		const folder: Content = { v: 1, kind: "folder", path: "Roundtrip", created: now, modified: now };
		const noteId = crypto.randomUUID();
		const folderId = crypto.randomUUID();
		const created = await push(token, [
			{ id: noteId, base: 0, deleted: false, blob: await sealObject(vaultKey, noteId, note) },
			{ id: folderId, base: 0, deleted: false, blob: await sealObject(vaultKey, folderId, folder) },
		]);
		same("push two new objects", created.map((r) => [r.status, r.seq]), [["ok", 1], ["ok", 2]]);

		// Device 2 pulls everything and decrypts it.
		const first = await pullAll(token2, 0);
		same("pull since 0: cursor", first.cursor, 2);
		const opened = await Promise.all(first.changes.map((change) => openObject(vaultKey, change.id, change.blob!)));
		same("pull since 0: decrypted objects", opened, [note, folder]);

		// Device 2 edits the note; device 1 pushes a stale edit and gets a conflict with device 2's version.
		const edited = { ...note, text: "# Plan\n\nEdited on the second device\n", modified: now + 1000 };
		const edit = await push(token2, [{ id: noteId, base: 1, deleted: false, blob: await sealObject(vaultKey, noteId, edited) }]);
		same("second device edits the note", edit.map((r) => [r.status, r.seq]), [["ok", 3]]);
		const stale = await push(token, [{ id: noteId, base: 1, deleted: false, blob: await sealObject(vaultKey, noteId, { ...note, text: "stale" }) }]);
		same("stale push: conflict at the server's seq", [stale[0]?.status, stale[0]?.seq, stale[0]?.deleted], ["conflict", 3, false]);
		same("conflict carries the server's version", await openObject(vaultKey, noteId, stale[0]!.blob!), edited);

		// Partial acceptance: one taken, one conflict, in one request.
		const fresh = crypto.randomUUID();
		const mixed = await push(token, [
			{ id: fresh, base: 0, deleted: false, blob: await sealObject(vaultKey, fresh, { ...note, path: "Roundtrip/Fresh.md" }) },
			{ id: noteId, base: 1, deleted: false, blob: await sealObject(vaultKey, noteId, note) },
		]);
		same("partial acceptance", mixed.map((r) => [r.status, r.seq]), [["ok", 4], ["conflict", 3]]);

		// Device 1 deletes the folder: a tombstone with no blob.
		const removed = await push(token, [{ id: folderId, base: 2, deleted: true, blob: null }]);
		same("delete the folder", removed.map((r) => [r.status, r.seq]), [["ok", 5]]);
		const after = await pullAll(token2, first.cursor);
		same("pull since 2", after.changes.map((c) => [c.id, c.seq, c.deleted, c.blob === null]), [
			[noteId, 3, false, false],
			[fresh, 4, false, false],
			[folderId, 5, true, true],
		]);

		// Two devices push at once: distinct, gapless seqs in commit order.
		const batch = async (device: string) =>
			Promise.all(
				Array.from({ length: 5 }, async (_, i) => {
					const id = crypto.randomUUID();
					return { id, base: 0, deleted: false, blob: await sealObject(vaultKey, id, { ...note, path: `Roundtrip/${device} ${i}.md` }) };
				}),
			);
		const [one, two] = await Promise.all([batch("one"), batch("two")]);
		const both = await Promise.all([push(token, one), push(token2, two)]);
		const seqs = both.flat().map((r) => r.seq).sort((a, b) => a - b);
		same("concurrent pushes: seqs 6 to 15", seqs, Array.from({ length: 10 }, (_, i) => i + 6));
		const paged = await pullAll(token, after.cursor, 3);
		same("pull with limit 3 pages until more is false", [paged.pages, paged.cursor, paged.changes.map((c) => c.seq)], [4, 15, seqs]);

		const account = await call("/v1/account", { token });
		same("GET /v1/account counts live objects", [account.body?.email, account.body?.objects], [email, 12]);

		// Changing the password re-wraps the vault key and ends the other session.
		const newPassword = `roundtrip ${crypto.randomUUID()}`;
		const newKeys = await deriveKeys(email, newPassword);
		const change = await call("/v1/account/password", {
			method: "POST",
			token,
			body: { currentPassword: keys.authPassword, newPassword: newKeys.authPassword, wrappedVaultKey: base64(await seal(newKeys.wrapKey, vaultKey, "vault-key")) },
		});
		same("change the password: 204", change.status, 204);
		same("the other session ends", (await call("/v1/account", { token: token2 })).status, 401);
		same("the old password no longer signs in", (await signIn(email, keys)).status, 401);
		keys = newKeys;
		const third = await signIn(email, keys);
		same("the new password signs in", third.status, 200);
		const rewrapped = await call("/v1/keys", { token: third.token! });
		same("the new wrap key opens the same vault key", hex(await open(keys.wrapKey, fromBase64(rewrapped.body.wrappedVaultKey), "vault-key")), hex(vaultKey));
		same("the pulled note still opens", await openObject(vaultKey, noteId, (await pullAll(third.token!, 2)).changes[0]!.blob!), edited);
	} finally {
		if (token) {
			const deleted = await call("/v1/account", { method: "DELETE", token, body: { password: keys.authPassword } });
			same("delete the account: 204", deleted.status, 204);
			same("the session is gone", (await call("/v1/account", { token })).status, 401);
			same("the account no longer signs in", (await signIn(email, keys)).status, 401);
		}
	}
}

try {
	await vectors();
	console.log(`server ${BASE}`);
	await server();
} catch (cause) {
	check("unexpected error", false, String(cause));
}
process.exit(failed ? 1 : 0);
