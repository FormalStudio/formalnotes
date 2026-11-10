import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { PGlite } from "@electric-sql/pglite";
import { PGLiteSocketServer } from "@electric-sql/pglite-socket";
import type { TestProject } from "vitest/node";

/** An in-process Postgres with the migrations applied, reachable over TCP so the Worker's Hyperdrive binding can use it. */
export default async function setup(project: TestProject) {
	const db = await PGlite.create();
	const dir = join(import.meta.dirname, "../drizzle");
	for (const file of (await readdir(dir)).filter((name) => name.endsWith(".sql")).sort()) {
		await db.exec(await readFile(join(dir, file), "utf8"));
	}
	// Every request opens its own client, as behind Hyperdrive, and a test keeps its sockets until the run ends.
	const server = new PGLiteSocketServer({ db, port: 0, host: "127.0.0.1", maxConnections: 1000 });
	keepQueriesWhole(server, db);
	await server.start();
	project.provide("databaseUrl", `postgresql://postgres:postgres@${server.getServerConn()}/postgres`);
	return async () => {
		await server.stop();
		await db.close();
	};
}

interface Queued {
	handlerId: number;
	message: Uint8Array;
	onData: (data: Uint8Array) => void;
	resolve: (bytes: number) => void;
	reject: (cause: unknown) => void;
}

interface QueryQueue {
	queue: Queued[];
	processing: boolean;
	lastHandlerId: number | null;
	processQueue: () => Promise<void>;
}

/** Parse, Bind, Describe, Execute, Close and Flush: a query in the extended protocol, open until its Sync. */
const EXTENDED = new Set([0x50, 0x42, 0x44, 0x45, 0x43, 0x48]);

/**
 * PGlite is one backend, and pglite-socket keeps a transaction on one connection but interleaves the other
 * messages of concurrent connections one by one. Two clients' Parse and Bind then share the unnamed statement
 * and read each other's rows. This queue also keeps a connection's Parse to Sync run whole, so concurrent
 * requests behave as they do on a real server.
 */
function keepQueriesWhole(server: PGLiteSocketServer, db: PGlite) {
	const queue = (server as unknown as { queryQueue: QueryQueue }).queryQueue;
	let open: number | null = null;
	queue.processQueue = async function (this: QueryQueue) {
		if (this.processing) return;
		this.processing = true;
		try {
			while (this.queue.length > 0) {
				const pinned = db.isInTransaction() ? this.lastHandlerId : open;
				const index = pinned === null ? 0 : this.queue.findIndex((item) => item.handlerId === pinned);
				if (index === -1) break;
				const [item] = this.queue.splice(index, 1) as [Queued];
				let bytes = 0;
				try {
					await db.runExclusive(() =>
						db.execProtocolRawStream(item.message, {
							onRawData: (data) => {
								bytes += data.length;
								item.onData(data);
							},
						}),
					);
				} catch (cause) {
					open = null;
					item.reject(cause);
					continue;
				}
				open = EXTENDED.has(item.message[0]!) ? item.handlerId : null;
				this.lastHandlerId = item.handlerId;
				item.resolve(bytes);
			}
		} finally {
			this.processing = false;
		}
	};
}

declare module "vitest" {
	export interface ProvidedContext {
		databaseUrl: string;
	}
}
