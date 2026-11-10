import { createExecutionContext, waitOnExecutionContext } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { eq } from "drizzle-orm";
import { describe, expect, it } from "vitest";
import { createApp } from "../src/app";
import { encodeBase64 } from "../src/base64";
import { createDb, schema } from "../src/db";
import { MAX_AVATAR } from "../src/api";
import { MAX_BLOB, MAX_BYTES, MAX_OBJECTS } from "../src/sync";

const base = "https://notes.example";
const app = createApp();
const db = createDb(env.HYPERDRIVE.connectionString);

let ip = 0;

async function request(path: string, init: RequestInit & { token?: string; json?: unknown } = {}) {
	const headers = new Headers(init.headers);
	if (init.token) headers.set("Authorization", `Bearer ${init.token}`);
	if (!headers.has("CF-Connecting-IP")) headers.set("CF-Connecting-IP", `10.2.${Math.floor(ip / 250)}.${(ip++ % 250) + 1}`);
	let body = init.body;
	if (init.json !== undefined) {
		headers.set("Content-Type", "application/json");
		body = JSON.stringify(init.json);
	}
	const ctx = createExecutionContext();
	const response = await app.request(`${base}${path}`, { ...init, headers, body }, env, ctx);
	await waitOnExecutionContext(ctx);
	return response;
}

/** What a device sends as the password: base64url of a 32-byte key, 43 characters. */
function password(): string {
	return encodeBase64(crypto.getRandomValues(new Uint8Array(32))).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** Bytes shaped like `seal` output. The server never opens them, so random content is enough. */
function sealed(length: number): string {
	const bytes = new Uint8Array(length);
	for (let at = 0; at < length; at += 65536) crypto.getRandomValues(bytes.subarray(at, at + 65536));
	bytes[0] = 1;
	return encodeBase64(bytes);
}

const wrappedKey = () => sealed(61);

interface User {
	id: string;
	email: string;
	password: string;
	token: string;
}

async function signUp(): Promise<User> {
	const email = `${crypto.randomUUID()}@example.com`;
	const pw = password();
	const response = await request("/api/auth/sign-up/email", { method: "POST", json: { name: email.split("@")[0], email, password: pw } });
	expect(response.status).toBe(200);
	const token = response.headers.get("set-auth-token")!;
	const [row] = await db.select({ id: schema.user.id }).from(schema.user).where(eq(schema.user.email, email));
	return { id: row!.id, email, password: pw, token };
}

async function signIn(email: string, pw: string) {
	return request("/api/auth/sign-in/email", { method: "POST", json: { email, password: pw } });
}

type Result = { id: string; status: "ok" | "conflict"; seq: number; deleted?: boolean; blob?: string | null };
type Pulled = { changes: { id: string; seq: number; deleted: boolean; blob: string | null }[]; cursor: number; more: boolean };

async function push(user: User, changes: { id: string; base: number; deleted?: boolean; blob?: string | null }[]) {
	const response = await request("/v1/sync", {
		method: "POST",
		token: user.token,
		json: { changes: changes.map((change) => ({ deleted: false, ...change })) },
	});
	return response;
}

async function pushOk(user: User, changes: Parameters<typeof push>[1]): Promise<Result[]> {
	const response = await push(user, changes);
	expect(response.status).toBe(200);
	return ((await response.json()) as { results: Result[] }).results;
}

async function pull(user: User, since = 0, limit?: number): Promise<Pulled> {
	const response = await request(`/v1/sync?since=${since}${limit ? `&limit=${limit}` : ""}`, { token: user.token });
	expect(response.status).toBe(200);
	return (await response.json()) as Pulled;
}

describe("service", () => {
	it("answers one plain line at the root and health without auth", async () => {
		const root = await request("/");
		expect(root.headers.get("Content-Type")).toMatch(/^text\/plain/);
		expect((await root.text()).trim().split("\n")).toHaveLength(1);
		expect(await (await request("/health")).json()).toEqual({ ok: true });
	});

	it("offers only the three account routes of Better Auth", async () => {
		const user = await signUp();
		for (const path of ["/api/auth/change-password", "/api/auth/request-password-reset", "/api/auth/update-user", "/api/auth/delete-user"]) {
			expect((await request(path, { method: "POST", token: user.token, json: {} })).status).toBe(404);
		}
		expect((await request("/api/auth/get-session", { token: user.token })).status).toBe(404);
	});
});

describe("accounts", () => {
	it("signs up and signs in with the derived password, and the token works on /v1", async () => {
		const user = await signUp();
		expect(user.token).toBeTruthy();
		const account = await request("/v1/account", { token: user.token });
		expect(await account.json()).toEqual({ email: user.email, objects: 0, bytes: 0, avatar: null });

		const again = await signIn(user.email, user.password);
		expect(again.status).toBe(200);
		expect((await request("/v1/account", { token: again.headers.get("set-auth-token")! })).status).toBe(200);
	});

	it("refuses a wrong password and anything that is not a derived key, in the protocol's error shape", async () => {
		const user = await signUp();
		const wrong = await signIn(user.email, password());
		expect(wrong.status).toBe(401);
		expect(await wrong.json()).toMatchObject({ error: expect.any(String), message: expect.any(String) });

		const short = await request("/api/auth/sign-up/email", { method: "POST", json: { name: "x", email: `${crypto.randomUUID()}@example.com`, password: "hunter2hunter2" } });
		expect(short.status).toBe(400);
		expect(await short.json()).toMatchObject({ error: "password_too_short" });
	});

	it("keeps no address or user agent with a session", async () => {
		const user = await signUp();
		const rows = await db.select().from(schema.session).where(eq(schema.session.userId, user.id));
		expect(rows.length).toBeGreaterThan(0);
		for (const row of rows) expect([row.ipAddress, row.userAgent]).toEqual([null, null]);
	});

	it("ends the session on sign-out", async () => {
		const user = await signUp();
		expect((await request("/api/auth/sign-out", { method: "POST", token: user.token })).status).toBe(200);
		expect((await request("/v1/account", { token: user.token })).status).toBe(401);
	});

	it("rate limits sign-in and sign-up per address", async () => {
		const headers = { "CF-Connecting-IP": "10.9.9.9" };
		const statuses = [];
		for (let i = 0; i < 12; i++) {
			statuses.push((await request("/api/auth/sign-in/email", { method: "POST", headers, json: { email: "nobody@example.com", password: password() } })).status);
		}
		expect(statuses.at(-1)).toBe(429);
		const signUps = [];
		for (let i = 0; i < 12; i++) {
			signUps.push((await request("/api/auth/sign-up/email", { method: "POST", headers, json: { name: "x", email: `${crypto.randomUUID()}@example.com`, password: password() } })).status);
		}
		expect(signUps.at(-1)).toBe(429);
	});
});

describe("auth guard", () => {
	const routes: [string, string][] = [
		["GET", "/v1/account"],
		["GET", "/v1/keys"],
		["PUT", "/v1/keys"],
		["POST", "/v1/account/password"],
		["DELETE", "/v1/account"],
		["GET", "/v1/avatar"],
		["PUT", "/v1/avatar"],
		["DELETE", "/v1/avatar"],
		["GET", "/v1/sync?since=0"],
		["POST", "/v1/sync"],
	];

	it("answers 401 unauthorized on every /v1 route without a valid session", async () => {
		for (const [method, path] of routes) {
			for (const token of [undefined, "not-a-token"]) {
				const response = await request(path, { method, token, json: method === "GET" ? undefined : {} });
				expect(response.status, `${method} ${path}`).toBe(401);
				expect(await response.json()).toMatchObject({ error: "unauthorized" });
			}
		}
	});
});

describe("keys", () => {
	it("answers no_keys, takes the wrapped key once, then refuses a second", async () => {
		const user = await signUp();
		const missing = await request("/v1/keys", { token: user.token });
		expect(missing.status).toBe(404);
		expect(await missing.json()).toMatchObject({ error: "no_keys" });

		const key = wrappedKey();
		expect((await request("/v1/keys", { method: "PUT", token: user.token, json: { wrappedVaultKey: key } })).status).toBe(204);
		const again = await request("/v1/keys", { method: "PUT", token: user.token, json: { wrappedVaultKey: wrappedKey() } });
		expect(again.status).toBe(409);
		expect(await again.json()).toMatchObject({ error: "keys_exist" });
		expect(await (await request("/v1/keys", { token: user.token })).json()).toEqual({ wrappedVaultKey: key });
	});

	it("refuses a key that is not a sealed 32-byte key", async () => {
		const user = await signUp();
		for (const wrappedVaultKey of [sealed(60), "not base64!", encodeBase64(new Uint8Array(61))]) {
			expect((await request("/v1/keys", { method: "PUT", token: user.token, json: { wrappedVaultKey } })).status).toBe(400);
		}
	});
});

describe("sync", () => {
	it("pushes new objects, pulls them back, and answers a stale base with the server's state", async () => {
		const user = await signUp();
		const a = crypto.randomUUID();
		const b = crypto.randomUUID();
		const blobA = sealed(1024);
		const blobB = sealed(2048);
		expect(await pushOk(user, [{ id: a, base: 0, blob: blobA }, { id: b, base: 0, blob: blobB }])).toEqual([
			{ id: a, status: "ok", seq: 1 },
			{ id: b, status: "ok", seq: 2 },
		]);
		expect(await pull(user)).toEqual({
			changes: [
				{ id: a, seq: 1, deleted: false, blob: blobA },
				{ id: b, seq: 2, deleted: false, blob: blobB },
			],
			cursor: 2,
			more: false,
		});

		const edited = sealed(1024);
		expect(await pushOk(user, [{ id: a, base: 1, blob: edited }])).toEqual([{ id: a, status: "ok", seq: 3 }]);
		expect(await pushOk(user, [{ id: a, base: 1, blob: sealed(1024) }])).toEqual([{ id: a, status: "conflict", seq: 3, deleted: false, blob: edited }]);
		// Creating an id that already exists is a conflict too.
		expect(await pushOk(user, [{ id: b, base: 0, blob: sealed(1024) }])).toEqual([{ id: b, status: "conflict", seq: 2, deleted: false, blob: blobB }]);

		const since2 = await pull(user, 2);
		expect(since2).toEqual({ changes: [{ id: a, seq: 3, deleted: false, blob: edited }], cursor: 3, more: false });
		expect(await pull(user, 3)).toEqual({ changes: [], cursor: 3, more: false });
		expect(await (await request("/v1/account", { token: user.token })).json()).toEqual({ email: user.email, objects: 2, bytes: 3072, avatar: null });
	});

	it("keeps the taken changes of a request when others in it conflict", async () => {
		const user = await signUp();
		const a = crypto.randomUUID();
		const b = crypto.randomUUID();
		const c = crypto.randomUUID();
		await pushOk(user, [{ id: a, base: 0, blob: sealed(100) }]);
		const results = await pushOk(user, [
			{ id: b, base: 0, blob: sealed(100) },
			{ id: a, base: 7, blob: sealed(100) },
			{ id: c, base: 4, blob: sealed(100) },
			{ id: a, base: 1, blob: sealed(100) },
		]);
		expect(results.map((result) => [result.id, result.status, result.seq])).toEqual([
			[b, "ok", 2],
			[a, "conflict", 1],
			// An id the server never saw reads as deleted at seq 0.
			[c, "conflict", 0],
			[a, "ok", 3],
		]);
		expect(results[2]).toMatchObject({ deleted: true, blob: null });
		expect((await pull(user)).changes.map((change) => [change.id, change.seq])).toEqual([
			[b, 2],
			[a, 3],
		]);
	});

	it("keeps a deleted object as a tombstone with no blob", async () => {
		const user = await signUp();
		const a = crypto.randomUUID();
		await pushOk(user, [{ id: a, base: 0, blob: sealed(500) }]);
		expect(await pushOk(user, [{ id: a, base: 1, deleted: true }])).toEqual([{ id: a, status: "ok", seq: 2 }]);
		expect(await pull(user)).toEqual({ changes: [{ id: a, seq: 2, deleted: true, blob: null }], cursor: 2, more: false });
		expect(await (await request("/v1/account", { token: user.token })).json()).toMatchObject({ objects: 0, bytes: 0 });
		expect(await pushOk(user, [{ id: a, base: 1, blob: sealed(500) }])).toEqual([{ id: a, status: "conflict", seq: 2, deleted: true, blob: null }]);
		// Restoring over the tombstone takes its seq as the base.
		expect(await pushOk(user, [{ id: a, base: 2, blob: sealed(500) }])).toEqual([{ id: a, status: "ok", seq: 3 }]);
	});

	it("pages with limit and more, and caps the page at 500", async () => {
		const user = await signUp();
		const ids = Array.from({ length: 120 }, () => crypto.randomUUID());
		await pushOk(user, ids.slice(0, 100).map((id) => ({ id, base: 0, blob: sealed(40) })));
		await pushOk(user, ids.slice(100).map((id) => ({ id, base: 0, blob: sealed(40) })));

		const seen: string[] = [];
		let cursor = 0;
		let pages = 0;
		for (let more = true; more; pages++) {
			const page = await pull(user, cursor, 50);
			seen.push(...page.changes.map((change) => change.id));
			expect(page.cursor).toBe(page.changes.at(-1)?.seq ?? cursor);
			({ cursor, more } = page);
		}
		expect(pages).toBe(3);
		expect(seen).toEqual(ids);
		expect(cursor).toBe(120);

		expect((await pull(user, 0, 10_000)).changes).toHaveLength(120);
		for (const query of ["since=-1", "since=abc", "since=0&limit=0", "since=1.5"]) {
			expect((await request(`/v1/sync?${query}`, { token: user.token })).status, query).toBe(400);
		}
	});

	it("never shows one user's objects to another", async () => {
		const ada = await signUp();
		const bob = await signUp();
		const id = crypto.randomUUID();
		const blob = sealed(64);
		await pushOk(ada, [{ id, base: 0, blob }]);
		expect(await pull(bob)).toEqual({ changes: [], cursor: 0, more: false });
		// The same id is a different object under another account, and pushing it touches nothing of Ada's.
		expect(await pushOk(bob, [{ id, base: 0, blob: sealed(64) }])).toEqual([{ id, status: "ok", seq: 1 }]);
		expect(await pushOk(bob, [{ id, base: 1, deleted: true }])).toEqual([{ id, status: "ok", seq: 2 }]);
		expect(await pull(ada)).toEqual({ changes: [{ id, seq: 1, deleted: false, blob }], cursor: 1, more: false });
	});

	it("gives concurrent pushes distinct, gapless, ordered seqs", async () => {
		const user = await signUp();
		const batches = Array.from({ length: 8 }, () => Array.from({ length: 5 }, () => ({ id: crypto.randomUUID(), base: 0, blob: sealed(64) })));
		const results = await Promise.all(batches.map((batch) => pushOk(user, batch)));
		const seqs = results.flat().map((result) => result.seq);
		expect(results.flat().every((result) => result.status === "ok")).toBe(true);
		expect([...seqs].sort((x, y) => x - y)).toEqual(Array.from({ length: 40 }, (_, i) => i + 1));
		for (const batch of results) {
			const own = batch.map((result) => result.seq);
			expect(own).toEqual(Array.from({ length: own.length }, (_, i) => own[0]! + i));
		}
		expect((await pull(user)).changes.map((change) => change.seq)).toEqual(Array.from({ length: 40 }, (_, i) => i + 1));
	});

	it("refuses an object over 1 MiB with too_large and takes nothing from that request", async () => {
		const user = await signUp();
		const small = crypto.randomUUID();
		const response = await push(user, [
			{ id: small, base: 0, blob: sealed(40) },
			{ id: crypto.randomUUID(), base: 0, blob: sealed(MAX_BLOB + 1) },
		]);
		expect(response.status).toBe(413);
		expect(await response.json()).toMatchObject({ error: "too_large" });
		expect((await pull(user)).changes).toEqual([]);
		expect(await pushOk(user, [{ id: small, base: 0, blob: sealed(MAX_BLOB) }])).toEqual([{ id: small, status: "ok", seq: 1 }]);
	});

	it("refuses malformed pushes", async () => {
		const user = await signUp();
		const tooMany = Array.from({ length: 101 }, () => ({ id: crypto.randomUUID(), base: 0, blob: sealed(30) }));
		const bad = [
			tooMany,
			[{ id: crypto.randomUUID().toUpperCase(), base: 0, blob: sealed(30) }],
			[{ id: crypto.randomUUID(), base: -1, blob: sealed(30) }],
			[{ id: crypto.randomUUID(), base: 0, blob: null }],
			[{ id: crypto.randomUUID(), base: 0, blob: "AAAA" }],
			[{ id: crypto.randomUUID(), base: 0, blob: "not base64" }],
			[{ id: crypto.randomUUID(), base: 0, blob: `${sealed(30).slice(0, -4)}A=AA` }],
		];
		for (const changes of bad) expect((await push(user, changes)).status).toBe(400);
	});

	it("enforces the account quota on objects and bytes", async () => {
		const user = await signUp();
		const a = crypto.randomUUID();
		await pushOk(user, [{ id: a, base: 0, blob: sealed(100) }]);

		await db.update(schema.counter).set({ objects: MAX_OBJECTS }).where(eq(schema.counter.userId, user.id));
		const full = await push(user, [{ id: crypto.randomUUID(), base: 0, blob: sealed(100) }]);
		expect(full.status).toBe(403);
		expect(await full.json()).toMatchObject({ error: "quota" });
		// Editing or deleting still works on a full account.
		expect(await pushOk(user, [{ id: a, base: 1, blob: sealed(90) }])).toEqual([{ id: a, status: "ok", seq: 2 }]);

		await db.update(schema.counter).set({ objects: 1, bytes: MAX_BYTES - 50 }).where(eq(schema.counter.userId, user.id));
		const heavy = await push(user, [
			{ id: crypto.randomUUID(), base: 0, blob: sealed(40) },
			{ id: crypto.randomUUID(), base: 0, blob: sealed(40) },
		]);
		expect(heavy.status).toBe(403);
		expect(await heavy.json()).toMatchObject({ error: "quota" });
		expect((await pull(user, 2)).changes).toEqual([]);
	});

	it("rate limits sync per user", async () => {
		const user = await signUp();
		const statuses = [];
		for (let i = 0; i < 125; i++) statuses.push((await request("/v1/sync?since=0", { token: user.token })).status);
		expect(statuses.slice(0, 100).every((status) => status === 200)).toBe(true);
		expect(statuses.at(-1)).toBe(429);
	});
});

describe("profile picture", () => {
	it("keeps one sealed picture per account and reports its version", async () => {
		const user = await signUp();
		const other = await signUp();
		expect(await (await request("/v1/account", { token: user.token })).json()).toMatchObject({ avatar: null });
		expect(await (await request("/v1/avatar", { token: user.token })).json()).toMatchObject({ error: "no_avatar" });

		const first = sealed(4096);
		const put = await request("/v1/avatar", { method: "PUT", token: user.token, json: { blob: first } });
		expect(put.status).toBe(200);
		const { avatar } = (await put.json()) as { avatar: number };
		expect(await (await request("/v1/account", { token: user.token })).json()).toMatchObject({ avatar });
		expect(await (await request("/v1/avatar", { token: user.token })).json()).toEqual({ avatar, blob: first });
		expect((await request("/v1/avatar", { token: other.token })).status).toBe(404);

		const second = sealed(2048);
		await request("/v1/avatar", { method: "PUT", token: user.token, json: { blob: second } });
		expect(await (await request("/v1/avatar", { token: user.token })).json()).toMatchObject({ blob: second });

		expect((await request("/v1/avatar", { method: "DELETE", token: user.token })).status).toBe(204);
		expect((await request("/v1/avatar", { token: user.token })).status).toBe(404);
		expect(await (await request("/v1/account", { token: user.token })).json()).toMatchObject({ avatar: null });
	});

	it("refuses a picture that is not sealed or is over the limit", async () => {
		const user = await signUp();
		const notSealed = await request("/v1/avatar", { method: "PUT", token: user.token, json: { blob: encodeBase64(new Uint8Array(64)) } });
		expect(notSealed.status).toBe(400);
		const big = await request("/v1/avatar", { method: "PUT", token: user.token, json: { blob: sealed(MAX_AVATAR + 1) } });
		expect(big.status).toBe(413);
		expect(await big.json()).toMatchObject({ error: "too_large" });
		expect((await request("/v1/avatar", { method: "PUT", token: user.token, json: { blob: sealed(MAX_AVATAR) } })).status).toBe(200);
	});
});

describe("password change", () => {
	it("re-wraps the vault key and ends every other session", async () => {
		const user = await signUp();
		const other = (await signIn(user.email, user.password)).headers.get("set-auth-token")!;
		await request("/v1/keys", { method: "PUT", token: user.token, json: { wrappedVaultKey: wrappedKey() } });
		const newPassword = password();
		const rewrapped = wrappedKey();

		const wrong = await request("/v1/account/password", { method: "POST", token: user.token, json: { currentPassword: password(), newPassword, wrappedVaultKey: rewrapped } });
		expect(wrong.status).toBe(403);
		expect(await wrong.json()).toMatchObject({ error: "wrong_password" });

		const changed = await request("/v1/account/password", { method: "POST", token: user.token, json: { currentPassword: user.password, newPassword, wrappedVaultKey: rewrapped } });
		expect(changed.status).toBe(204);
		expect((await request("/v1/account", { token: other })).status).toBe(401);
		expect(await (await request("/v1/keys", { token: user.token })).json()).toEqual({ wrappedVaultKey: rewrapped });

		expect((await signIn(user.email, user.password)).status).toBe(401);
		expect((await signIn(user.email, newPassword)).status).toBe(200);
	});
});

describe("account deletion", () => {
	it("needs the password, then removes every row the user owns", async () => {
		const user = await signUp();
		const bystander = await signUp();
		await request("/v1/keys", { method: "PUT", token: user.token, json: { wrappedVaultKey: wrappedKey() } });
		await pushOk(user, [{ id: crypto.randomUUID(), base: 0, blob: sealed(100) }]);
		await request("/v1/avatar", { method: "PUT", token: user.token, json: { blob: sealed(100) } });
		await pushOk(bystander, [{ id: crypto.randomUUID(), base: 0, blob: sealed(100) }]);

		const wrong = await request("/v1/account", { method: "DELETE", token: user.token, json: { password: password() } });
		expect(wrong.status).toBe(403);

		expect((await request("/v1/account", { method: "DELETE", token: user.token, json: { password: user.password } })).status).toBe(204);
		expect((await request("/v1/account", { token: user.token })).status).toBe(401);
		expect((await signIn(user.email, user.password)).status).toBe(401);

		for (const table of [schema.session, schema.account, schema.vaultKey, schema.counter, schema.object, schema.avatar]) {
			expect(await db.select().from(table).where(eq(table.userId, user.id))).toEqual([]);
		}
		expect(await db.select().from(schema.user).where(eq(schema.user.id, user.id))).toEqual([]);
		expect((await pull(bystander)).changes).toHaveLength(1);
	});
});
