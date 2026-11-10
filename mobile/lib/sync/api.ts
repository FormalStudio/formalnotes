// The server's HTTP API (docs/protocol.md "API"). `fetch` is passed in so tests can answer it.

export const SERVER_URL = 'https://notes.formalsnake.dev';

export type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

/** The request never reached the server, or its answer never came back. */
export class OfflineError extends Error {}

export type PulledChange = { id: string; seq: number; deleted: boolean; blob: string | null };
export type Pull = { changes: PulledChange[]; cursor: number; more: boolean };
export type PushChange = { id: string; base: number; deleted: boolean; blob: string | null };
export type PushResult =
  | { id: string; status: 'ok'; seq: number }
  | { id: string; status: 'conflict'; seq: number; deleted: boolean; blob: string | null };

export class Api {
  constructor(
    private readonly fetch: Fetch,
    readonly base = SERVER_URL,
    public token: string | null = null,
  ) {}

  private async request(method: string, path: string, body?: unknown): Promise<Response> {
    const headers: Record<string, string> = { Accept: 'application/json' };
    if (body !== undefined) headers['Content-Type'] = 'application/json';
    if (this.token) headers.Authorization = `Bearer ${this.token}`;
    let response: Response;
    try {
      response = await this.fetch(`${this.base}${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch (cause) {
      throw new OfflineError(cause instanceof Error ? cause.message : 'offline');
    }
    if (!response.ok) {
      const err = (await response.json().catch(() => ({}))) as { error?: string; message?: string };
      throw new ApiError(response.status, err.error ?? 'server_error', err.message ?? `HTTP ${response.status}`);
    }
    return response;
  }

  private async json<T>(method: string, path: string, body?: unknown): Promise<T> {
    return (await (await this.request(method, path, body)).json()) as T;
  }

  /** Signs up or in; the session token comes back in `set-auth-token`. */
  private async session(path: string, body: unknown): Promise<string> {
    const response = await this.request('POST', path, body);
    const token = response.headers.get('set-auth-token');
    if (!token) throw new ApiError(response.status, 'no_token', 'The server sent no session token');
    this.token = token;
    return token;
  }

  signUp(email: string, password: string): Promise<string> {
    const name = email.trim().split('@')[0];
    return this.session('/api/auth/sign-up/email', { name, email: email.trim(), password });
  }

  signIn(email: string, password: string): Promise<string> {
    return this.session('/api/auth/sign-in/email', { email: email.trim(), password });
  }

  async signOut(): Promise<void> {
    await this.request('POST', '/api/auth/sign-out', {});
  }

  account(): Promise<{ email: string; objects: number; bytes: number }> {
    return this.json('GET', '/v1/account');
  }

  /** The wrapped vault key, or null when sign-up was cut short before it was put. */
  async keys(): Promise<string | null> {
    try {
      return (await this.json<{ wrappedVaultKey: string }>('GET', '/v1/keys')).wrappedVaultKey;
    } catch (e) {
      if (e instanceof ApiError && e.code === 'no_keys') return null;
      throw e;
    }
  }

  async putKeys(wrappedVaultKey: string): Promise<void> {
    await this.request('PUT', '/v1/keys', { wrappedVaultKey });
  }

  async changePassword(currentPassword: string, newPassword: string, wrappedVaultKey: string): Promise<void> {
    await this.request('POST', '/v1/account/password', { currentPassword, newPassword, wrappedVaultKey });
  }

  async deleteAccount(password: string): Promise<void> {
    await this.request('DELETE', '/v1/account', { password });
  }

  pull(since: number, limit = 500): Promise<Pull> {
    return this.json('GET', `/v1/sync?since=${since}&limit=${limit}`);
  }

  async push(changes: PushChange[]): Promise<PushResult[]> {
    return (await this.json<{ results: PushResult[] }>('POST', '/v1/sync', { changes })).results;
  }

  async health(): Promise<boolean> {
    try {
      return ((await this.json<{ ok?: boolean }>('GET', '/health')).ok ?? false) === true;
    } catch {
      return false;
    }
  }
}
