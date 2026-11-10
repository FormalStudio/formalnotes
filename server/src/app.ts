import { Hono, type MiddlewareHandler } from "hono";
import { api, type Variables } from "./api";
import { createAuth } from "./auth";
import { createDb } from "./db";
import { error, json } from "./http";

type App = { Bindings: Env; Variables: Variables };

/** Better Auth answers `{ code, message }`; the protocol's errors are `{ error, message }`. */
async function protocolError(response: Response): Promise<Response> {
	const body = (await response.json().catch(() => ({}))) as { code?: string; message?: string };
	const code = body.code?.toLowerCase() ?? (response.status === 401 ? "unauthorized" : "bad_request");
	return error(response.status, code, body.message ?? response.statusText);
}

/**
 * Error names and Postgres codes only. A message can carry the failed query's parameters: an email, a token,
 * a blob.
 */
function describe(cause: unknown) {
	const chain = [];
	for (let at = cause; at instanceof Error && chain.length < 5; at = at.cause) {
		chain.push({ name: at.name, code: (at as { code?: unknown }).code });
	}
	return chain;
}

export function createApp() {
	const app = new Hono<App>();

	app.get("/", (c) => c.text("Formal Notes sync server\n"));
	app.get("/health", () => json({ ok: true }));

	const withDatabase: MiddlewareHandler<App> = async (c, next) => {
		const db = createDb(c.env.HYPERDRIVE.connectionString);
		c.set("db", db);
		c.set("auth", createAuth(db, c.env));
		await next();
	};
	app.use("/api/auth/*", withDatabase);
	app.use("/v1/*", withDatabase);

	const perIp = (limit: (env: Env) => RateLimit): MiddlewareHandler<App> => async (c, next) => {
		const { success } = await limit(c.env).limit({ key: c.req.header("CF-Connecting-IP") ?? "unknown" });
		if (!success) return error(429, "rate_limited", "Too many attempts, try again in a minute");
		await next();
	};

	const toAuth: MiddlewareHandler<App> = async (c) => {
		// Devices authenticate with bearer tokens only, and Better Auth refuses a cookie without an Origin header.
		const headers = new Headers(c.req.raw.headers);
		headers.delete("Cookie");
		const response = await c.get("auth").handler(new Request(c.req.raw, { headers }));
		if (response.status >= 500) console.error(JSON.stringify({ message: "auth", path: c.req.path, status: response.status }));
		return response.status >= 400 ? protocolError(response) : response;
	};

	// Only these three: Better Auth's other routes (password reset, change password, update user) would bypass the
	// vault key rules in /v1.
	app.post("/api/auth/sign-up/email", perIp((env) => env.SIGN_UP_LIMIT), toAuth);
	app.post("/api/auth/sign-in/email", perIp((env) => env.SIGN_IN_LIMIT), toAuth);
	app.post("/api/auth/sign-out", toAuth);

	app.route("/v1", api);

	app.notFound(() => error(404, "not_found", "Not found"));
	app.onError((cause, c) => {
		console.error(JSON.stringify({ message: "unhandled", method: c.req.method, path: c.req.path, error: describe(cause) }));
		return error(500, "server_error", "Something went wrong");
	});

	return app;
}
