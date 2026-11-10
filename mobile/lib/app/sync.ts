// When the phone syncs: two seconds after a local change, when the app comes to the front, every
// 30 seconds while it is open, and on pull to refresh. Holds the session in the keychain.

import { fetch } from 'expo/fetch';
import { useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import QuickCrypto from 'react-native-quick-crypto';

import { haptic } from '../haptics';
import * as account from '../sync/account';
import { Api, ApiError, OfflineError, type Fetch } from '../sync/api';
import { SyncCrypto, fromBase64, toBase64, type CryptoBackend } from '../sync/crypto';
import { SyncEngine } from '../sync/engine';
import { library, newId } from './library';
import { secure } from './secure';

export type SyncStatus = 'idle' | 'syncing' | 'offline' | 'error';

export type SyncState = {
  loaded: boolean;
  email: string | null;
  status: SyncStatus;
  lastSync: number | null;
  message: string | null;
  /** The profile picture as a data URI, or null for the letter on its gradient. */
  avatar: string | null;
};

const SESSION = 'sync.session';
const LAST = 'sync.last';
const AVATAR_VERSION = 'avatar.version';
const AVATAR_JPEG = 'avatar.jpeg';
const AVATAR_AAD = 'avatar';
const dataUri = (jpeg: string) => `data:image/jpeg;base64,${jpeg}`;
const SOON = 2000;
const EVERY = 30000;

const backend = { subtle: QuickCrypto.subtle, getRandomValues: QuickCrypto.getRandomValues } as unknown as CryptoBackend;
export const syncCrypto = new SyncCrypto(backend);

type Stored = { email: string; token: string; vaultKey: string };

class SyncController {
  state: SyncState = { loaded: false, email: null, status: 'idle', lastSync: null, message: null, avatar: null };
  private listeners = new Set<() => void>();
  private api = new Api(fetch as unknown as Fetch);
  private vaultKey: Uint8Array | null = null;
  private soon: ReturnType<typeof setTimeout> | null = null;
  private every: ReturnType<typeof setInterval> | null = null;
  private running: Promise<void> | null = null;
  private again = false;
  private started = false;

  subscribe = (l: () => void) => {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  };

  private set(patch: Partial<SyncState>) {
    this.state = { ...this.state, ...patch };
    for (const l of this.listeners) l();
  }

  /** Loads the session and starts the timers. Once per process. */
  start(): void {
    if (this.started) return;
    this.started = true;
    library.subscribe((local) => {
      if (local) this.schedule(SOON);
    });
    AppState.addEventListener('change', (s) => {
      if (s === 'active') {
        this.schedule(0);
        this.tick();
      } else if (this.every) {
        clearInterval(this.every);
        this.every = null;
      }
    });
    void this.load();
  }

  private tick() {
    if (this.every) clearInterval(this.every);
    this.every = setInterval(() => this.schedule(0), EVERY);
  }

  private async load() {
    const raw = await secure.get(SESSION).catch(() => null);
    const last = library.meta(LAST);
    if (raw) {
      const s = JSON.parse(raw) as Stored;
      this.api.token = s.token;
      this.vaultKey = fromBase64(s.vaultKey);
      const jpeg = library.meta(AVATAR_JPEG);
      this.set({ email: s.email, avatar: jpeg ? dataUri(jpeg) : null });
    }
    this.set({ loaded: true, lastSync: last ? Number(last) : null });
    this.tick();
    this.schedule(0);
  }

  schedule(ms: number): void {
    if (!this.vaultKey) return;
    if (this.soon) clearTimeout(this.soon);
    this.soon = setTimeout(() => {
      this.soon = null;
      void this.syncNow();
    }, ms);
  }

  /** One sync, or the one already running; a request during it queues one more. */
  syncNow(): Promise<void> {
    if (!this.vaultKey) return Promise.resolve();
    if (this.running) {
      this.again = true;
      return this.running;
    }
    const key = this.vaultKey;
    this.running = (async () => {
      this.set({ status: 'syncing' });
      try {
        await new SyncEngine(library, this.api, syncCrypto, key, Date.now, newId).sync();
        const now = Date.now();
        library.setMeta(LAST, String(now));
        this.set({ status: 'idle', lastSync: now, message: null });
        await this.followAvatar(key);
      } catch (e) {
        if (e instanceof OfflineError) this.set({ status: 'offline', message: 'Offline. Changes sync when you are back.' });
        else if (e instanceof ApiError && e.status === 401) {
          await this.forget();
          this.set({ status: 'error', message: 'Your session ended. Sign in again to keep syncing.' });
        } else this.set({ status: 'error', message: e instanceof Error ? e.message : 'Sync failed' });
      } finally {
        this.running = null;
        if (this.again) {
          this.again = false;
          this.schedule(0);
        }
      }
    })();
    return this.running;
  }

  /** Keeps the picture this phone shows, and the version it came with, for the next launch. */
  private showAvatar(version: number | null, jpeg: string | null) {
    library.setMeta(AVATAR_VERSION, version === null ? null : String(version));
    library.setMeta(AVATAR_JPEG, jpeg);
    this.set({ avatar: jpeg ? dataUri(jpeg) : null });
  }

  /** Fetches the picture when the account reports a version other than the one shown. */
  private async followAvatar(key: Uint8Array) {
    const { avatar } = await this.api.account();
    const shown = library.meta(AVATAR_VERSION);
    if ((avatar === null ? null : String(avatar)) === (shown ?? null)) return;
    const blob = avatar === null ? null : await this.api.avatar();
    if (!blob) return this.showAvatar(null, null);
    this.showAvatar(avatar, toBase64(await syncCrypto.open(key, blob, AVATAR_AAD)));
  }

  /** `jpeg` is base64 of a square JPEG, 512 pixels across. */
  async setAvatar(jpeg: string) {
    if (!this.vaultKey) return;
    const version = await this.api.putAvatar(await syncCrypto.seal(this.vaultKey, fromBase64(jpeg), AVATAR_AAD));
    this.showAvatar(version, jpeg);
  }

  async removeAvatar() {
    await this.api.deleteAvatar();
    this.showAvatar(null, null);
  }

  private async keep(session: account.Session) {
    await secure.set(SESSION, JSON.stringify({ email: session.email, token: session.token, vaultKey: toBase64(session.vaultKey) }));
    this.api.token = session.token;
    this.vaultKey = session.vaultKey;
    this.set({ email: session.email, status: 'idle', message: null });
    void this.syncNow();
  }

  /** Drops the session and what this device knew about the server; the notes stay. */
  private async forget() {
    this.vaultKey = null;
    this.api.token = null;
    await secure.remove(SESSION).catch(() => {});
    library.sql.tx(() => {
      library.sql.run('DELETE FROM synced');
      library.setMeta('sync.cursor', null);
      library.setMeta(LAST, null);
      library.setMeta(AVATAR_VERSION, null);
      library.setMeta(AVATAR_JPEG, null);
    });
    this.set({ email: null, lastSync: null, status: 'idle', avatar: null });
  }

  async signUp(email: string, password: string) {
    await this.keep(await account.signUp(this.api, syncCrypto, email, password));
  }

  async signIn(email: string, password: string) {
    await this.keep(await account.signIn(this.api, syncCrypto, email, password));
  }

  async signOut() {
    await this.api.signOut().catch(() => {});
    await this.forget();
    this.set({ message: null });
  }

  async changePassword(current: string, next: string) {
    if (!this.vaultKey || !this.state.email) return;
    await account.changePassword(this.api, syncCrypto, this.state.email, current, next, this.vaultKey);
  }

  async deleteAccount(password: string) {
    if (!this.state.email) return;
    await account.deleteAccount(this.api, syncCrypto, this.state.email, password);
    await this.forget();
    this.set({ message: null });
  }
}

export const sync = new SyncController();

const read = () => sync.state;

export function useSync(): SyncState {
  return useSyncExternalStore(sync.subscribe, read);
}

/** The message to show for a failed account call. */
export function accountError(e: unknown): string {
  if (e instanceof OfflineError) return 'The server cannot be reached. Check your connection.';
  if (e instanceof ApiError) {
    if (e.code === 'invalid_email_or_password' || e.status === 401) return 'That email and password do not match.';
    if (e.code === 'wrong_password') return 'That password is wrong.';
    if (e.code === 'user_already_exists_use_another_email' || e.code.includes('already')) return 'An account with that email exists. Sign in instead.';
    if (e.code === 'quota') return 'Your account is full. Delete some notes to sync more.';
    if (e.code === 'too_large') return 'That is too large to sync.';
    if (e.code === 'password_too_short') return 'That password is too short.';
    if (e.code === 'rate_limited') return 'Too many attempts. Try again in a minute.';
    return e.message;
  }
  return e instanceof Error ? e.message : 'Something went wrong.';
}

/**
 * Runs an account call for a form: busy while it runs, its message when it fails. Lives outside
 * the components because the compiler cannot lower try/finally.
 */
export async function accountCall(
  run: () => Promise<void>,
  setBusy: (busy: boolean) => void,
  setError: (message: string | null) => void,
): Promise<boolean> {
  setBusy(true);
  setError(null);
  try {
    await run();
    haptic('success');
    return true;
  } catch (e) {
    setError(accountError(e));
    haptic('warning');
    return false;
  } finally {
    setBusy(false);
  }
}
