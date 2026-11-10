// An in-memory server for tests that answers the routes of docs/protocol.md with the rules of
// server/src/sync.ts and server/src/api.ts: optimistic seq per object, conflicts carry the
// server's state, an unknown id reads as deleted at seq 0, pages of at most 500.

import type { Fetch } from './api';

type StoredObject = { seq: number; deleted: boolean; blob: string | null };
type User = { id: string; email: string; password: string; wrapped: string | null; seq: number; objects: Map<string, StoredObject> };

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const PASSWORD = /^[A-Za-z0-9_-]{43}$/;

function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return Response.json(body, { status, headers });
}

function error(status: number, code: string, message = code): Response {
  return json({ error: code, message }, status);
}

export class FakeServer {
  readonly users = new Map<string, User>();
  private tokens = new Map<string, User>();
  private nextToken = 1;
  /** When set, the next push is applied and its answer is lost on the way back. */
  dropNextPushAnswer = false;
  requests: string[] = [];

  readonly fetch: Fetch = async (input, init) => {
    const url = new URL(input);
    const method = init?.method ?? 'GET';
    this.requests.push(`${method} ${url.pathname}`);
    const body = init?.body ? JSON.parse(String(init.body)) : undefined;
    const auth = new Headers(init?.headers).get('Authorization');
    const user = auth?.startsWith('Bearer ') ? this.tokens.get(auth.slice(7)) : undefined;
    const response = this.route(method, url, body, user);
    if (method === 'POST' && url.pathname === '/v1/sync' && this.dropNextPushAnswer) {
      this.dropNextPushAnswer = false;
      throw new TypeError('Network request failed');
    }
    return response;
  };

  private session(user: User): Response {
    const token = `token-${this.nextToken++}`;
    this.tokens.set(token, user);
    return json({ token, user: { email: user.email } }, 200, { 'set-auth-token': token });
  }

  private route(method: string, url: URL, body: any, user: User | undefined): Response {
    const path = url.pathname;
    if (path === '/health') return json({ ok: true });
    if (method === 'POST' && path === '/api/auth/sign-up/email') {
      if (!PASSWORD.test(body?.password ?? '')) return error(400, 'password_too_short');
      if (this.users.has(body.email.toLowerCase())) return error(422, 'user_already_exists');
      const created: User = {
        id: `user-${this.users.size + 1}`,
        email: body.email.toLowerCase(),
        password: body.password,
        wrapped: null,
        seq: 0,
        objects: new Map(),
      };
      this.users.set(created.email, created);
      return this.session(created);
    }
    if (method === 'POST' && path === '/api/auth/sign-in/email') {
      const found = this.users.get(String(body?.email).toLowerCase());
      if (!found || found.password !== body.password) return error(401, 'invalid_email_or_password');
      return this.session(found);
    }
    if (method === 'POST' && path === '/api/auth/sign-out') {
      for (const [t, u] of this.tokens) if (u === user) this.tokens.delete(t);
      return json({ success: true });
    }
    if (!path.startsWith('/v1/')) return error(404, 'not_found');
    if (!user) return error(401, 'unauthorized', 'Sign in again');

    if (method === 'GET' && path === '/v1/account') {
      const live = [...user.objects.values()].filter((o) => !o.deleted);
      return json({ email: user.email, objects: live.length, bytes: live.reduce((n, o) => n + (o.blob?.length ?? 0), 0) });
    }
    if (method === 'GET' && path === '/v1/keys') {
      return user.wrapped ? json({ wrappedVaultKey: user.wrapped }) : error(404, 'no_keys');
    }
    if (method === 'PUT' && path === '/v1/keys') {
      if (user.wrapped) return error(409, 'keys_exist');
      user.wrapped = body.wrappedVaultKey;
      return new Response(null, { status: 204 });
    }
    if (method === 'POST' && path === '/v1/account/password') {
      if (body.currentPassword !== user.password) return error(403, 'wrong_password');
      user.password = body.newPassword;
      user.wrapped = body.wrappedVaultKey;
      return new Response(null, { status: 204 });
    }
    if (method === 'DELETE' && path === '/v1/account') {
      if (body.password !== user.password) return error(403, 'wrong_password');
      this.users.delete(user.email);
      for (const [t, u] of this.tokens) if (u === user) this.tokens.delete(t);
      return new Response(null, { status: 204 });
    }
    if (method === 'GET' && path === '/v1/sync') {
      const since = Number(url.searchParams.get('since') ?? '0');
      const limit = Math.min(Number(url.searchParams.get('limit') ?? '500'), 500);
      const rows = [...user.objects.entries()]
        .filter(([, o]) => o.seq > since)
        .sort((a, b) => a[1].seq - b[1].seq)
        .slice(0, limit + 1);
      const page = rows.slice(0, limit);
      return json({
        changes: page.map(([id, o]) => ({ id, seq: o.seq, deleted: o.deleted, blob: o.blob })),
        cursor: page.at(-1)?.[1].seq ?? since,
        more: rows.length > limit,
      });
    }
    if (method === 'POST' && path === '/v1/sync') {
      const changes = body?.changes as { id: string; base: number; deleted: boolean; blob?: string | null }[];
      if (!Array.isArray(changes) || changes.length > 100) return error(400, 'bad_request');
      for (const c of changes) {
        if (!UUID_V4.test(c.id) || (!c.deleted && typeof c.blob !== 'string')) return error(400, 'bad_request');
        if (!c.deleted && atob(c.blob!).charCodeAt(0) !== 1) return error(400, 'bad_request');
      }
      const results = changes.map((c) => {
        const now = user.objects.get(c.id);
        if ((now?.seq ?? 0) !== c.base) {
          return now
            ? { id: c.id, status: 'conflict', seq: now.seq, deleted: now.deleted, blob: now.blob }
            : { id: c.id, status: 'conflict', seq: 0, deleted: true, blob: null };
        }
        user.seq += 1;
        user.objects.set(c.id, { seq: user.seq, deleted: c.deleted, blob: c.deleted ? null : c.blob! });
        return { id: c.id, status: 'ok', seq: user.seq };
      });
      return json({ results });
    }
    return error(404, 'not_found');
  }
}
