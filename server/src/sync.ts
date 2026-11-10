import { and, asc, eq, gt, inArray, sql } from "drizzle-orm";
import type { Context } from "hono";
import { z } from "zod";
import type { Variables } from "./api";
import { decodeBase64, encodeBase64, isSealed } from "./base64";
import { schema } from "./db";
import { error, json, parse } from "./http";

type Ctx = Context<{ Bindings: Env; Variables: Variables }>;

export const PAGE = 500;
export const MAX_CHANGES = 100;
export const MAX_BLOB = 1024 * 1024;
export const MAX_OBJECTS = 20_000;
export const MAX_BYTES = 200_000_000;

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

const pushInput = z.object({
	changes: z
		.array(
			z
				.object({
					id: z.string().regex(UUID_V4, "Expected a lowercase UUID v4"),
					base: z.number().int().min(0),
					deleted: z.boolean(),
					blob: z.string().nullish(),
				})
				.refine((change) => change.deleted || typeof change.blob === "string", "A change that is not deleted needs a blob"),
		)
		.max(MAX_CHANGES),
});

type Row = { id: string; seq: number; deleted: boolean; blob: Uint8Array | null; size: number };

function state(row: Row | undefined) {
	// An id the server never saw reads as deleted at seq 0, so the device pushes again with base 0.
	if (!row) return { seq: 0, deleted: true, blob: null };
	return { seq: row.seq, deleted: row.deleted, blob: row.blob ? encodeBase64(row.blob) : null };
}

class QuotaExceeded extends Error {}

export async function pull(c: Ctx) {
	const since = Number(c.req.query("since") ?? "0");
	const limit = Math.min(Number(c.req.query("limit") ?? PAGE), PAGE);
	if (!Number.isSafeInteger(since) || since < 0) return error(400, "bad_request", "since is a seq, 0 or more");
	if (!Number.isSafeInteger(limit) || limit < 1) return error(400, "bad_request", "limit is a number from 1 to 500");

	const rows = await c
		.get("db")
		.select({ id: schema.object.id, seq: schema.object.seq, deleted: schema.object.deleted, blob: schema.object.blob })
		.from(schema.object)
		.where(and(eq(schema.object.userId, c.get("userId")), gt(schema.object.seq, since)))
		.orderBy(asc(schema.object.seq))
		.limit(limit + 1);
	const page = rows.slice(0, limit);
	return json({
		changes: page.map((row) => ({ id: row.id, seq: row.seq, deleted: row.deleted, blob: row.blob ? encodeBase64(row.blob) : null })),
		cursor: page.at(-1)?.seq ?? since,
		more: rows.length > limit,
	});
}

export async function push(c: Ctx) {
	const input = await parse(c, pushInput);
	if (input instanceof Response) return input;

	const changes: { id: string; base: number; deleted: boolean; blob: Uint8Array | null }[] = [];
	for (const change of input.changes) {
		if (change.deleted) {
			changes.push({ ...change, blob: null });
			continue;
		}
		const blob = decodeBase64(change.blob ?? "");
		if (!blob || !isSealed(blob)) return error(400, "bad_request", "A blob is a sealed object in standard base64");
		if (blob.length > MAX_BLOB) return error(413, "too_large", "An object is at most 1 MiB");
		changes.push({ ...change, blob });
	}
	if (changes.length === 0) return json({ results: [] });

	const userId = c.get("userId");
	try {
		const results = await c.get("db").transaction(async (tx) => {
			await tx.insert(schema.counter).values({ userId }).onConflictDoNothing();
			// Every push of this user waits here, so seq values are handed out in commit order with no gaps.
			const [counter] = await tx.select().from(schema.counter).where(eq(schema.counter.userId, userId)).for("update");
			let { seq, objects, bytes } = counter!;

			const ids = [...new Set(changes.map((change) => change.id))];
			const existing = await tx
				.select({ id: schema.object.id, seq: schema.object.seq, deleted: schema.object.deleted, blob: schema.object.blob, size: schema.object.size })
				.from(schema.object)
				.where(and(eq(schema.object.userId, userId), inArray(schema.object.id, ids)));
			const current = new Map<string, Row>(existing.map((row) => [row.id, row]));
			const written = new Map<string, Row>();

			const results = changes.map((change) => {
				const now = current.get(change.id);
				if ((now?.seq ?? 0) !== change.base) return { id: change.id, status: "conflict" as const, ...state(now) };

				const size = change.blob?.length ?? 0;
				const objectDelta = (change.deleted ? 0 : 1) - (now && !now.deleted ? 1 : 0);
				const byteDelta = size - (now?.size ?? 0);
				objects += objectDelta;
				bytes += byteDelta;
				if ((objectDelta > 0 && objects > MAX_OBJECTS) || (byteDelta > 0 && bytes > MAX_BYTES)) throw new QuotaExceeded();

				seq += 1;
				const row = { id: change.id, seq, deleted: change.deleted, blob: change.blob, size };
				current.set(change.id, row);
				written.set(change.id, row);
				return { id: change.id, status: "ok" as const, seq };
			});

			if (written.size > 0) {
				await tx
					.insert(schema.object)
					.values([...written.values()].map((row) => ({ userId, ...row })))
					.onConflictDoUpdate({
						target: [schema.object.userId, schema.object.id],
						set: { seq: sql`excluded.seq`, deleted: sql`excluded.deleted`, blob: sql`excluded.blob`, size: sql`excluded.size` },
					});
				await tx.update(schema.counter).set({ seq, objects, bytes }).where(eq(schema.counter.userId, userId));
			}
			return results;
		});
		return json({ results });
	} catch (cause) {
		if (cause instanceof QuotaExceeded) return error(403, "quota", "This account is full: 20000 objects or 200 MB");
		throw cause;
	}
}
