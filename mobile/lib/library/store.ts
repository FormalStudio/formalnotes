// The library on the phone: one SQLite table of entries (notes, folders, `.folder.json` files)
// keyed by the sync object id, so a rename or a move keeps the id. No React in here; views
// subscribe and read.

import { parseFolderView, writeFolderView, type FolderView } from './folder-view';
import {
  FOLDER_VIEW,
  contains,
  fileStemFor,
  join,
  nameOf,
  NOTE_EXT,
  parentOf,
  rebase,
  stemOf,
  titleOf,
  withHeading,
} from './model';
import { migrate, type Sql } from './sql';

export type Kind = 'note' | 'folder' | 'file';

export type Entry = {
  id: string;
  path: string;
  kind: Kind;
  text: string | null;
  hash: string | null;
  created: number;
  modified: number;
};

export type Note = { id: string; path: string; title: string; text: string; created: number; modified: number };

export type Folder = { id: string; path: string; name: string; created: number; modified: number };

export type TrashItem = { id: string; path: string; kind: Kind; title: string; trashed: number; batch: string };

export type Sort = 'modified' | 'created' | 'title';

type Listener = (local: boolean) => void;

export function toNote(e: Entry): Note {
  const text = e.text ?? '';
  return { id: e.id, path: e.path, title: titleOf(text, stemOf(e.path)), text, created: e.created, modified: e.modified };
}

function toFolder(e: Entry): Folder {
  return { id: e.id, path: e.path, name: nameOf(e.path), created: e.created, modified: e.modified };
}

const ENTRY = 'SELECT id, path, kind, text, hash, created, modified FROM entries';

export class Library {
  readonly sql: Sql;
  private readonly newId: () => string;
  private readonly now: () => number;
  private listeners = new Set<Listener>();
  /** Bumped on every change; views key their reads on it. */
  version = 0;

  constructor(sql: Sql, newId: () => string, now: () => number = Date.now) {
    this.sql = sql;
    this.newId = newId;
    this.now = now;
    migrate(sql);
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Tells views to read again; `local` changes are the user's and schedule a sync. */
  changed(local: boolean): void {
    this.version++;
    for (const l of this.listeners) l(local);
  }

  // Reads

  entry(id: string): Entry | null {
    return this.sql.get<Entry>(`${ENTRY} WHERE id = ?`, [id]);
  }

  byPath(path: string): Entry | null {
    return this.sql.get<Entry>(`${ENTRY} WHERE path = ?`, [path]);
  }

  all(): Entry[] {
    return this.sql.all<Entry>(ENTRY);
  }

  note(id: string): Note | null {
    const e = this.entry(id);
    return e && e.kind === 'note' ? toNote(e) : null;
  }

  folder(path: string): Folder | null {
    const e = this.byPath(path);
    return e && e.kind === 'folder' ? toFolder(e) : null;
  }

  hasFolder(path: string): boolean {
    return path === '' || this.folder(path) !== null;
  }

  /** Every direct child of `folder` of `kind`. */
  private children(folder: string, kind: Kind): Entry[] {
    if (folder === '') {
      return this.sql.all<Entry>(`${ENTRY} WHERE kind = ? AND instr(path, '/') = 0`, [kind]);
    }
    return this.sql
      .all<Entry>(`${ENTRY} WHERE kind = ? AND substr(path, 1, ?) = ?`, [kind, folder.length + 1, `${folder}/`])
      .filter((e) => !e.path.slice(folder.length + 1).includes('/'));
  }

  subfolders(folder: string, sort: Sort = 'modified'): Folder[] {
    const out = this.children(folder, 'folder').map(toFolder);
    if (sort === 'title') return out.sort((a, b) => a.name.localeCompare(b.name));
    const key = sort === 'created' ? 'created' : 'modified';
    return out.sort((a, b) => b[key] - a[key] || a.path.localeCompare(b.path));
  }

  notesIn(folder: string, sort: Sort = 'modified'): Note[] {
    const out = this.children(folder, 'note').map(toNote);
    if (sort === 'title') return out.sort((a, b) => a.title.localeCompare(b.title) || a.path.localeCompare(b.path));
    const key = sort === 'created' ? 'created' : 'modified';
    return out.sort((a, b) => b[key] - a[key] || a.path.localeCompare(b.path));
  }

  count(folder: string): number {
    return this.children(folder, 'folder').length + this.children(folder, 'note').length;
  }

  allFolders(): Folder[] {
    return this.sql
      .all<Entry>(`${ENTRY} WHERE kind = 'folder' ORDER BY path COLLATE NOCASE`)
      .map(toFolder);
  }

  allNotes(): Note[] {
    return this.sql.all<Entry>(`${ENTRY} WHERE kind = 'note'`).map(toNote);
  }

  folderView(folder: string): FolderView {
    return parseFolderView(this.byPath(join(folder, FOLDER_VIEW))?.text);
  }

  /** Notes whose title or text holds every word of `query`, title matches first. */
  search(query: string, limit = 100): Note[] {
    const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0) return [];
    const like = (w: string) => `%${w.replace(/[\\%_]/g, (c) => `\\${c}`)}%`;
    const where = words.map(() => `(path LIKE ? ESCAPE '\\' OR text LIKE ? ESCAPE '\\')`).join(' AND ');
    const params = words.flatMap((w) => [like(w), like(w)]);
    const hits = this.sql.all<Entry>(`${ENTRY} WHERE kind = 'note' AND ${where}`, params).map(toNote);
    const inTitle = (n: Note) => words.every((w) => n.title.toLowerCase().includes(w));
    return hits
      .sort((a, b) => Number(inTitle(b)) - Number(inTitle(a)) || b.modified - a.modified)
      .slice(0, limit);
  }

  trashItems(): TrashItem[] {
    return this.sql
      .all<Entry & { trashed: number; batch: string }>(
        `SELECT id, path, kind, text, created, modified, trashed, batch FROM trash
         WHERE id IN (SELECT id FROM trash t WHERE NOT EXISTS (
           SELECT 1 FROM trash p WHERE p.batch = t.batch AND length(p.path) < length(t.path)))
         ORDER BY trashed DESC`,
      )
      .map((e) => ({
        id: e.id,
        path: e.path,
        kind: e.kind,
        title: e.kind === 'note' ? titleOf(e.text ?? '', stemOf(e.path)) : nameOf(e.path),
        trashed: e.trashed,
        batch: e.batch,
      }));
  }

  // Free names, compared case-insensitively as the default macOS and Windows file systems do.

  taken(path: string, except?: string): boolean {
    const e = this.byPath(path);
    return e !== null && e.id !== except;
  }

  freeNotePath(folder: string, title: string, except?: string): string {
    const stem = fileStemFor(title);
    for (let n = 1; ; n++) {
      const path = join(folder, n === 1 ? `${stem}${NOTE_EXT}` : `${stem} ${n}${NOTE_EXT}`);
      if (!this.taken(path, except)) return path;
    }
  }

  freeFolderPath(folder: string, name: string, except?: string): string {
    const base = fileStemFor(name);
    for (let n = 1; ; n++) {
      const path = join(folder, n === 1 ? base : `${base} ${n}`);
      if (!this.taken(path, except)) return path;
    }
  }

  // Writes the user makes. Each one tells listeners it was local.

  createNote(folder: string, text = ''): string {
    const id = this.newId();
    this.sql.tx(() => {
      const at = this.ensureFolder(folder);
      this.insert({ id, path: this.freeNotePath(at, titleOf(text, 'Untitled')), kind: 'note', text });
    });
    this.changed(true);
    return id;
  }

  createFolder(parent: string, name = 'New folder'): string {
    const id = this.newId();
    this.sql.tx(() => {
      const at = this.ensureFolder(parent);
      this.insert({ id, path: this.freeFolderPath(at, name), kind: 'folder', text: null });
    });
    this.changed(true);
    return id;
  }

  /** The folder at `path`, made (with its parents) when missing. Returns the path. */
  ensureFolder(path: string): string {
    if (path === '') return '';
    const existing = this.byPath(path);
    if (existing?.kind === 'folder') return existing.path;
    const parent = this.ensureFolder(parentOf(path));
    const at = join(parent, nameOf(path));
    if (!this.byPath(at)) this.insert({ id: this.newId(), path: at, kind: 'folder', text: null });
    return at;
  }

  writeText(id: string, text: string): void {
    const e = this.entry(id);
    if (!e || e.text === text) return;
    this.sql.run('UPDATE entries SET text = ?, hash = NULL, modified = ? WHERE id = ?', [text, this.now(), id]);
    this.changed(true);
  }

  /** Sets a note's title: its heading becomes `title` and the file takes the title's name. */
  retitle(id: string, title: string): void {
    const e = this.entry(id);
    const t = title.trim();
    if (!e || e.kind !== 'note' || t === '') return;
    const text = withHeading(e.text ?? '', t);
    const path = this.freeNotePath(parentOf(e.path), t, id);
    this.sql.tx(() => {
      if (text !== e.text) {
        this.sql.run('UPDATE entries SET text = ?, hash = NULL, modified = ? WHERE id = ?', [text, this.now(), id]);
      }
      if (path !== e.path) this.movePath(e.path, path);
    });
    this.changed(true);
  }

  renameFolder(id: string, name: string): void {
    const e = this.entry(id);
    const n = name.trim();
    if (!e || e.kind !== 'folder' || n === '' || n === nameOf(e.path)) return;
    const to = this.freeFolderPath(parentOf(e.path), n, id);
    this.sql.tx(() => this.movePath(e.path, to));
    this.changed(true);
  }

  /** Moves a note or a folder into `folder` under its name or the next free one. */
  move(id: string, folder: string): void {
    const e = this.entry(id);
    if (!e || parentOf(e.path) === folder || !this.hasFolder(folder)) return;
    if (e.kind === 'folder' && contains(e.path, folder)) return;
    const to =
      e.kind === 'note' ? this.freeNotePath(folder, stemOf(e.path)) : this.freeFolderPath(folder, nameOf(e.path));
    this.sql.tx(() => this.movePath(e.path, to));
    this.changed(true);
  }

  /** Moves `from` (and, for a folder, everything under it) to `to`. */
  movePath(from: string, to: string): void {
    const under = this.sql.all<Entry>(`${ENTRY} WHERE path = ? OR substr(path, 1, ?) = ?`, [
      from,
      from.length + 1,
      `${from}/`,
    ]);
    // Through a placeholder, so a rename that only changes case passes the unique index.
    for (const e of under) this.sql.run('UPDATE entries SET path = ? WHERE id = ?', [`\u0001${e.id}`, e.id]);
    for (const e of under) {
      this.sql.run('UPDATE entries SET path = ? WHERE id = ?', [rebase(e.path, from, to) ?? e.path, e.id]);
    }
  }

  /** Moves an entry (and what is under it) to the trash. Returns the batch that undoes it. */
  trash(id: string): string | null {
    const batch = this.sql.tx(() => this.trashEntry(id));
    if (batch) this.changed(true);
    return batch;
  }

  trashEntry(id: string): string | null {
    const e = this.entry(id);
    if (!e) return null;
    const batch = this.newId();
    const now = this.now();
    const under =
      e.kind === 'folder'
        ? this.sql.all<Entry>(`${ENTRY} WHERE path = ? OR substr(path, 1, ?) = ?`, [
            e.path,
            e.path.length + 1,
            `${e.path}/`,
          ])
        : [e];
    for (const x of under) {
      this.sql.run(
        'INSERT OR REPLACE INTO trash (id, path, kind, text, created, modified, trashed, batch) VALUES (?, ?, ?, ?, ?, ?, ?, ?)',
        [x.id, x.path, x.kind, x.text, x.created, x.modified, now, batch],
      );
      this.sql.run('DELETE FROM entries WHERE id = ?', [x.id]);
    }
    return batch;
  }

  /** Puts a trashed batch back where it was, under free names when those are taken now. */
  restore(batch: string): void {
    const items = this.sql
      .all<Entry>('SELECT id, path, kind, text, NULL AS hash, created, modified FROM trash WHERE batch = ?', [batch])
      .sort((a, b) => a.path.length - b.path.length);
    if (items.length === 0) return;
    this.sql.tx(() => {
      const top = items[0];
      const folder = this.ensureFolder(parentOf(top.path));
      const to =
        top.kind === 'folder'
          ? this.freeFolderPath(folder, nameOf(top.path))
          : this.freeNotePath(folder, stemOf(top.path));
      for (const x of items) {
        const path = rebase(x.path, top.path, to) ?? x.path;
        this.sql.run(
          'INSERT OR REPLACE INTO entries (id, path, kind, text, hash, created, modified) VALUES (?, ?, ?, ?, NULL, ?, ?)',
          [x.id, path, x.kind, x.text, x.created, x.modified],
        );
      }
      this.sql.run('DELETE FROM trash WHERE batch = ?', [batch]);
    });
    this.changed(true);
  }

  emptyTrash(): void {
    this.sql.run('DELETE FROM trash');
    this.changed(false);
  }

  setFolderView(folder: string, view: FolderView): void {
    const path = join(folder, FOLDER_VIEW);
    const text = writeFolderView(view);
    const e = this.byPath(path);
    if (e) this.writeText(e.id, text);
    else {
      this.insert({ id: this.newId(), path, kind: 'file', text });
      this.changed(true);
    }
  }

  // Low level, for the sync engine. No notifications; the engine sends one when it is done.

  insert(e: Omit<Entry, 'hash' | 'created' | 'modified'> & { created?: number; modified?: number }): void {
    const now = this.now();
    this.sql.run(
      'INSERT INTO entries (id, path, kind, text, hash, created, modified) VALUES (?, ?, ?, ?, NULL, ?, ?)',
      [e.id, e.path, e.kind, e.text, e.created ?? now, e.modified ?? now],
    );
  }

  /** Writes an entry as given, replacing whatever has its id. */
  put(e: Omit<Entry, 'hash'>): void {
    this.sql.run(
      'INSERT OR REPLACE INTO entries (id, path, kind, text, hash, created, modified) VALUES (?, ?, ?, ?, NULL, ?, ?)',
      [e.id, e.path, e.kind, e.text, e.created, e.modified],
    );
  }

  setHash(id: string, hash: string): void {
    this.sql.run('UPDATE entries SET hash = ? WHERE id = ?', [hash, id]);
  }

  reId(from: string, to: string): void {
    this.sql.run('UPDATE entries SET id = ? WHERE id = ?', [to, from]);
  }

  meta(key: string): string | null {
    return this.sql.get<{ value: string }>('SELECT value FROM meta WHERE key = ?', [key])?.value ?? null;
  }

  setMeta(key: string, value: string | null): void {
    if (value === null) this.sql.run('DELETE FROM meta WHERE key = ?', [key]);
    else this.sql.run('INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)', [key, value]);
  }
}
