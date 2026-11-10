import { drizzle } from "drizzle-orm/postgres-js";
import postgres from "postgres";
import * as schema from "./schema";

/**
 * A fresh client per request: Hyperdrive is the pool and recycles backend connections, so a client cached
 * across requests ends up holding a dead socket. `prepare: false` because prepared statements do not survive
 * the proxy.
 */
export function createDb(connectionString: string) {
	const sql = postgres(connectionString, { max: 5, connect_timeout: 10, prepare: false, fetch_types: false });
	return drizzle(sql, { schema });
}

export type Database = ReturnType<typeof createDb>;

export { schema };
