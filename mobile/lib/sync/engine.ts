// "What a device does" from docs/protocol.md. Every entry in the library carries its object id, so
// a rename or a move is the same id at a new path. Per object the `synced` table keeps the path and
// the SHA-256 of the text this device last synced, and that object's seq.

import type { Entry, Kind, Library } from '../library/store';
import { join, nameOf, NOTE_EXT, parentOf, stemOf } from '../library/model';
import type { Api, PushChange, PushResult } from './api';
import type { ObjectPayload, SyncCrypto } from './crypto';

type Synced = { id: string; path: string; hash: string; seq: number; deleted: number };

/** The server's state of an object: its seq and, unless deleted, what it holds. */
type ServerState = { seq: number; payload: ObjectPayload | null };

export type SyncReport = { pulled: number; pushed: number; conflicts: number };

const PUSH_BATCH = 100;
const PUSH_ROUNDS = 5;
const CURSOR = 'sync.cursor';

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

/** `<name> (conflict YYYY-MM-DD HH.mm).md`, in local time. */
export function conflictName(stem: string, at: number): string {
  const d = new Date(at);
  const stamp = `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}.${pad2(d.getMinutes())}`;
  return `${stem} (conflict ${stamp})${NOTE_EXT}`;
}

export class SyncEngine {
  private report: SyncReport = { pulled: 0, pushed: 0, conflicts: 0 };
  private touched = false;

  constructor(
    private readonly lib: Library,
    private readonly api: Api,
    private readonly crypto: SyncCrypto,
    private readonly vaultKey: Uint8Array,
    private readonly now: () => number = Date.now,
    private readonly newId: () => string = () => globalThis.crypto.randomUUID(),
  ) {}

  /** One full sync: pull every page, then push what changed here until nothing is left. */
  async sync(): Promise<SyncReport> {
    this.report = { pulled: 0, pushed: 0, conflicts: 0 };
    this.touched = false;
    try {
      await this.pull();
      for (let round = 0; round < PUSH_ROUNDS; round++) {
        if (!(await this.push())) break;
      }
    } finally {
      if (this.touched) this.lib.changed(false);
    }
    return this.report;
  }

  // State

  private synced(id: string): Synced | null {
    return this.lib.sql.get<Synced>('SELECT id, path, hash, seq, deleted FROM synced WHERE id = ?', [id]);
  }

  private setSynced(id: string, path: string, hash: string, seq: number, deleted: boolean): void {
    this.lib.sql.run('INSERT OR REPLACE INTO synced (id, path, hash, seq, deleted) VALUES (?, ?, ?, ?, ?)', [
      id,
      path,
      hash,
      seq,
      deleted ? 1 : 0,
    ]);
  }

  private hash(text: string | null | undefined): Promise<string> {
    return this.crypto.sha256Hex(text ?? '');
  }

  private async fillHashes(): Promise<void> {
    const missing = this.lib.sql.all<{ id: string; text: string | null }>(
      'SELECT id, text FROM entries WHERE hash IS NULL',
    );
    const hashes = await Promise.all(missing.map((e) => this.hash(e.text)));
    this.lib.sql.tx(() => missing.forEach((e, i) => this.lib.setHash(e.id, hashes[i])));
  }

  /** Whether the entry differs from what this device last synced for its id. */
  private changedHere(e: Entry, s: Synced | null): boolean {
    return !s || s.deleted === 1 || e.path !== s.path || e.hash === null || e.hash !== s.hash;
  }

  // Pull

  private async pull(): Promise<void> {
    let cursor = Number(this.lib.meta(CURSOR) ?? '0');
    const deletedFolders: { id: string; seq: number }[] = [];
    for (;;) {
      await this.fillHashes();
      const page = await this.api.pull(cursor);
      // Decrypting is independent per change; applying is not, so it stays in server order.
      const opened = await Promise.all(
        page.changes.map(async (change) => {
          const payload = change.deleted || !change.blob ? null : await this.open(change.id, change.blob);
          return { payload, hash: payload ? await this.hash(payload.text) : '' };
        }),
      );
      for (const [i, change] of page.changes.entries()) {
        const { payload, hash } = opened[i];
        if (payload === undefined) continue;
        this.lib.sql.tx(() => {
          const folder = this.applyPulled(change.id, { seq: change.seq, payload }, hash);
          if (folder) deletedFolders.push({ id: change.id, seq: change.seq });
        });
        this.report.pulled++;
      }
      cursor = page.cursor;
      this.lib.setMeta(CURSOR, String(cursor));
      if (!page.more) break;
    }
    if (deletedFolders.length > 0) this.lib.sql.tx(() => this.deleteFolders(deletedFolders));
  }

  /** Undefined for a blob this device cannot read, which it leaves alone. */
  private async open(id: string, blob: string): Promise<ObjectPayload | undefined> {
    try {
      return await this.crypto.openObject(this.vaultKey, id, blob);
    } catch {
      return undefined;
    }
  }

  /** Applies one pulled change. Returns true for a folder delete, which waits for the page's end. */
  private applyPulled(id: string, server: ServerState, hash: string): boolean {
    const s = this.synced(id);
    if (s && server.seq <= s.seq) return false;
    const e = this.lib.entry(id);
    this.touched = true;
    if (!s) {
      if (server.payload) this.place(id, server.payload, server.seq, hash);
      else if (e) this.resolve(id, server, hash);
      return false;
    }
    const unchangedHere = e ? !this.changedHere(e, s) : s.deleted === 1;
    if (!unchangedHere) {
      this.resolve(id, server, hash);
      return false;
    }
    if (!server.payload) {
      if (e?.kind === 'folder') return true;
      if (e) this.lib.trashEntry(id);
      this.setSynced(id, s.path, s.hash, server.seq, true);
      return false;
    }
    this.write(id, server.payload);
    this.setSynced(id, server.payload.path, hash, server.seq, false);
    return false;
  }

  /**
   * A remote folder delete. An emptied folder goes to the trash; one still holding notes kept
   * here is pushed again, as a server delete against a local edit is.
   */
  private deleteFolders(folders: { id: string; seq: number }[]): void {
    for (const { id, seq } of folders.sort((a, b) => this.depth(b.id) - this.depth(a.id))) {
      const e = this.lib.entry(id);
      const s = this.synced(id);
      if (!e || !s) continue;
      if (this.lib.count(e.path) === 0) this.lib.trashEntry(id);
      this.setSynced(id, s.path, s.hash, seq, true);
    }
  }

  private depth(id: string): number {
    return this.lib.entry(id)?.path.split('/').length ?? 0;
  }

  /**
   * An object this device has never seen. A local file at its path with the same kind and text
   * takes its id (first sync adoption); any other file at that path came second and moves aside.
   */
  private place(id: string, p: ObjectPayload, seq: number, hash: string): void {
    const occupant = this.lib.byPath(p.path);
    if (occupant && !this.synced(occupant.id)) {
      if (occupant.kind === p.kind && (occupant.text ?? '') === (p.text ?? '')) {
        this.lib.reId(occupant.id, id);
        this.lib.setHash(id, hash);
        this.setSynced(id, occupant.path, hash, seq, false);
        return;
      }
      this.lib.movePath(occupant.path, this.freeBeside(occupant.path, occupant.kind));
    }
    this.write(id, p);
    this.setSynced(id, p.path, hash, seq, false);
  }

  /**
   * Writes the server's version into the entry with `id`. When another tracked entry holds the
   * path, this one arrived second: it lands at `<name> 2` and that rename is pushed.
   */
  private write(id: string, p: ObjectPayload): void {
    const current = this.lib.entry(id);
    let path = p.path;
    const occupant = this.lib.byPath(path);
    if (occupant && occupant.id !== id) {
      if (this.synced(occupant.id)) path = this.freeBeside(path, p.kind);
      else this.lib.movePath(occupant.path, this.freeBeside(occupant.path, occupant.kind));
    }
    this.lib.ensureFolder(parentOf(path));
    if (current && current.path !== path) {
      if (current.kind === 'folder' && this.lib.byPath(path) === null) this.lib.movePath(current.path, path);
    }
    this.lib.put({
      id,
      path,
      kind: p.kind,
      text: p.kind === 'folder' ? null : (p.text ?? ''),
      created: p.created,
      modified: p.modified,
    });
  }

  private freeBeside(path: string, kind: Kind): string {
    const folder = parentOf(path);
    return kind === 'folder'
      ? this.lib.freeFolderPath(folder, nameOf(path))
      : this.lib.freeNotePath(folder, stemOf(path));
  }

  // Conflicts

  /** The rules of docs/protocol.md "Conflict", with the server's current state in hand. */
  private resolve(id: string, server: ServerState, serverHash: string): void {
    const e = this.lib.entry(id);
    const s = this.synced(id);
    const p = server.payload;
    if (p && e && e.kind === p.kind && e.path === p.path && (e.text ?? '') === (p.text ?? '')) {
      this.lib.setHash(id, serverHash);
      this.setSynced(id, p.path, serverHash, server.seq, false);
      return;
    }
    this.report.conflicts++;
    if (!p && !e) {
      this.setSynced(id, s?.path ?? '', s?.hash ?? '', server.seq, true);
      return;
    }
    if (!p && e) {
      // Server deleted, local edited: pushed again on top of the server's seq.
      this.setSynced(id, s?.path ?? e.path, s?.hash ?? '', server.seq, true);
      return;
    }
    if (p && !e) {
      // Local deleted, server edited: the server's version comes back.
      this.write(id, p);
      this.lib.setHash(id, serverHash);
      this.setSynced(id, p.path, serverHash, server.seq, false);
      return;
    }
    if (!p || !e) return;
    const localText = e.text ?? '';
    this.write(id, p);
    this.lib.setHash(id, serverHash);
    this.setSynced(id, p.path, serverHash, server.seq, false);
    if (e.kind === 'note' && p.kind === 'note' && localText !== (p.text ?? '')) {
      const folder = this.lib.ensureFolder(parentOf(p.path));
      let path = join(folder, conflictName(stemOf(p.path), this.now()));
      if (this.lib.byPath(path)) path = this.lib.freeNotePath(folder, stemOf(path));
      this.lib.insert({ id: this.newId(), path, kind: 'note', text: localText, created: e.created, modified: this.now() });
    }
  }

  // Push

  /** Pushes every local change in batches. Returns whether anything was pushed. */
  private async push(): Promise<boolean> {
    await this.fillHashes();
    const entries = this.lib.all().sort((a, b) => kindOrder(a.kind) - kindOrder(b.kind) || a.path.length - b.path.length);
    const changed = entries.flatMap((e) => {
      const s = this.synced(e.id);
      return this.changedHere(e, s) ? [{ e, s }] : [];
    });
    const pending: { change: PushChange; path: string; hash: string }[] = await Promise.all(
      changed.map(async ({ e, s }) => {
        const payload: ObjectPayload = {
          v: 1,
          kind: e.kind,
          path: e.path,
          ...(e.kind === 'folder' ? {} : { text: e.text ?? '' }),
          created: e.created,
          modified: e.modified,
        };
        const blob = await this.crypto.sealObject(this.vaultKey, e.id, payload);
        return { change: { id: e.id, base: s?.seq ?? 0, deleted: false, blob }, path: e.path, hash: e.hash ?? '' };
      }),
    );
    const gone = this.lib.sql.all<Synced>(
      'SELECT id, path, hash, seq, deleted FROM synced WHERE deleted = 0 AND id NOT IN (SELECT id FROM entries)',
    );
    for (const s of gone) pending.push({ change: { id: s.id, base: s.seq, deleted: true, blob: null }, path: s.path, hash: s.hash });
    if (pending.length === 0) return false;

    // Batches go one after another: the server applies them in order and folders lead.
    for (let i = 0; i < pending.length; i += PUSH_BATCH) {
      const batch = pending.slice(i, i + PUSH_BATCH);
      const results = await this.api.push(batch.map((b) => b.change));
      const conflicts = await Promise.all(results.map((r) => this.openConflict(r)));
      for (const [j, result] of results.entries()) {
        const sent = batch[j];
        if (!sent || sent.change.id !== result.id) continue;
        this.settle(sent, result, conflicts[j]);
      }
    }
    return true;
  }

  /** The server's side of a conflict, decrypted; undefined when there is none or it is unreadable. */
  private async openConflict(result: PushResult): Promise<{ payload: ObjectPayload | null; hash: string } | undefined> {
    if (result.status === 'ok') return undefined;
    const payload = result.deleted || !result.blob ? null : await this.open(result.id, result.blob);
    if (payload === undefined) return undefined;
    return { payload, hash: payload ? await this.hash(payload.text) : '' };
  }

  private settle(
    sent: { change: PushChange; path: string; hash: string },
    result: PushResult,
    server: { payload: ObjectPayload | null; hash: string } | undefined,
  ): void {
    const { id } = sent.change;
    if (result.status === 'ok') {
      this.setSynced(id, sent.path, sent.hash, result.seq, sent.change.deleted);
      this.report.pushed++;
      return;
    }
    if (!server) return;
    this.touched = true;
    this.lib.sql.tx(() => this.resolve(id, { seq: result.seq, payload: server.payload }, server.hash));
  }
}

function kindOrder(kind: Kind): number {
  return kind === 'folder' ? 0 : kind === 'note' ? 1 : 2;
}
