import { sql } from "drizzle-orm";
import { createApp } from "./app";
import { createDb, schema } from "./db";

const app = createApp();

export default {
	fetch: app.fetch,
	async scheduled(_controller, env) {
		const db = createDb(env.HYPERDRIVE.connectionString);
		await db.delete(schema.session).where(sql`${schema.session.expiresAt} <= now()`);
		await db.delete(schema.verification).where(sql`${schema.verification.expiresAt} <= now()`);
	},
} satisfies ExportedHandler<Env>;
