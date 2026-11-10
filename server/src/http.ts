import type { Context } from "hono";
import { z } from "zod";

export function json(body: unknown, status = 200): Response {
	return Response.json(body, { status, headers: { "Cache-Control": "no-store" } });
}

export function error(status: number, code: string, message: string): Response {
	return json({ error: code, message }, status);
}

export function noContent(): Response {
	return new Response(null, { status: 204, headers: { "Cache-Control": "no-store" } });
}

/** The request's JSON body checked against `shape`, or the 400 to answer with. */
export async function parse<T extends z.ZodType>(c: Context, shape: T): Promise<z.infer<T> | Response> {
	let body: unknown;
	try {
		body = await c.req.json();
	} catch {
		return error(400, "bad_request", "Expected a JSON body");
	}
	const result = shape.safeParse(body);
	if (!result.success) return error(400, "bad_request", z.prettifyError(result.error));
	return result.data;
}
