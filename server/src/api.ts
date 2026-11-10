import { and, eq, ne } from "drizzle-orm";
import { Hono, type Context } from "hono";
import { z } from "zod";
import { PASSWORD_LENGTH, type Auth } from "./auth";
import { decodeBase64, encodeBase64, isSealed } from "./base64";
import { schema, type Database } from "./db";
import { error, json, noContent, parse } from "./http";
import { pull, push } from "./sync";

export type Variables = { db: Database; auth: Auth; userId: string; sessionId: string };
type Ctx = Context<{ Bindings: Env; Variables: Variables }>;

/** `seal(wrapKey, vaultKey)`: version byte, nonce, 32 key bytes, tag. */
const WRAPPED_KEY_LENGTH = 1 + 12 + 32 + 16;

const password = z.string().regex(new RegExp(`^[A-Za-z0-9_-]{${PASSWORD_LENGTH}}$`), "Expected base64url(authKey)");
const wrappedVaultKey = z
	.string()
	.transform((text) => decodeBase64(text))
	.refine((bytes): bytes is Uint8Array => bytes !== null && bytes.length === WRAPPED_KEY_LENGTH && isSealed(bytes), "Expected a sealed vault key");

const keysInput = z.object({ wrappedVaultKey });
const passwordInput = z.object({ currentPassword: password, newPassword: password, wrappedVaultKey });
const deleteInput = z.object({ password });

async function checkPassword(c: Ctx, candidate: string): Promise<boolean> {
	const [row] = await c
		.get("db")
		.select({ hash: schema.account.password })
		.from(schema.account)
		.where(and(eq(schema.account.userId, c.get("userId")), eq(schema.account.providerId, "credential")));
	if (!row?.hash) return false;
	const context = await c.get("auth").$context;
	return context.password.verify({ hash: row.hash, password: candidate });
}

export const api = new Hono<{ Bindings: Env; Variables: Variables }>();

api.use(async (c, next) => {
	const found = await c.get("auth").api.getSession({ headers: c.req.raw.headers });
	if (!found) return error(401, "unauthorized", "Sign in again");
	c.set("userId", found.user.id);
	c.set("sessionId", found.session.id);
	await next();
});

api.get("/account", async (c) => {
	const userId = c.get("userId");
	const [row] = await c
		.get("db")
		.select({ email: schema.user.email, objects: schema.counter.objects, bytes: schema.counter.bytes })
		.from(schema.user)
		.leftJoin(schema.counter, eq(schema.counter.userId, schema.user.id))
		.where(eq(schema.user.id, userId));
	if (!row) return error(401, "unauthorized", "Sign in again");
	return json({ email: row.email, objects: row.objects ?? 0, bytes: row.bytes ?? 0 });
});

api.get("/keys", async (c) => {
	const [row] = await c.get("db").select({ wrapped: schema.vaultKey.wrapped }).from(schema.vaultKey).where(eq(schema.vaultKey.userId, c.get("userId")));
	if (!row) return error(404, "no_keys", "This account has no vault key yet");
	return json({ wrappedVaultKey: encodeBase64(row.wrapped) });
});

api.put("/keys", async (c) => {
	const input = await parse(c, keysInput);
	if (input instanceof Response) return input;
	const inserted = await c
		.get("db")
		.insert(schema.vaultKey)
		.values({ userId: c.get("userId"), wrapped: input.wrappedVaultKey })
		.onConflictDoNothing()
		.returning({ userId: schema.vaultKey.userId });
	if (inserted.length === 0) return error(409, "keys_exist", "This account already has a vault key");
	return noContent();
});

api.post("/account/password", async (c) => {
	const input = await parse(c, passwordInput);
	if (input instanceof Response) return input;
	if (!(await checkPassword(c, input.currentPassword))) return error(403, "wrong_password", "The current password is wrong");
	const hash = await (await c.get("auth").$context).password.hash(input.newPassword);
	const userId = c.get("userId");
	await c.get("db").transaction(async (tx) => {
		const now = new Date();
		await tx
			.update(schema.account)
			.set({ password: hash, updatedAt: now })
			.where(and(eq(schema.account.userId, userId), eq(schema.account.providerId, "credential")));
		await tx
			.insert(schema.vaultKey)
			.values({ userId, wrapped: input.wrappedVaultKey, updatedAt: now })
			.onConflictDoUpdate({ target: schema.vaultKey.userId, set: { wrapped: input.wrappedVaultKey, updatedAt: now } });
		await tx.delete(schema.session).where(and(eq(schema.session.userId, userId), ne(schema.session.id, c.get("sessionId"))));
	});
	return noContent();
});

api.delete("/account", async (c) => {
	const input = await parse(c, deleteInput);
	if (input instanceof Response) return input;
	if (!(await checkPassword(c, input.password))) return error(403, "wrong_password", "The password is wrong");
	// Sessions, keys, the counter and every object go with the user through ON DELETE CASCADE.
	await c.get("db").delete(schema.user).where(eq(schema.user.id, c.get("userId")));
	return noContent();
});

api.use("/sync", async (c, next) => {
	const { success } = await c.env.SYNC_LIMIT.limit({ key: c.get("userId") });
	if (!success) return error(429, "rate_limited", "Too many sync requests, try again in a minute");
	await next();
});

api.get("/sync", pull);
api.post("/sync", push);
