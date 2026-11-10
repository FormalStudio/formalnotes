import { beforeEach, describe, expect, test } from 'bun:test';

import { memorySql } from '../library/bun-sql';
import { Library } from '../library/store';
import { Api } from './api';
import { SyncCrypto } from './crypto';
import { SyncEngine } from './engine';
import { FakeServer } from './fake-server';

const crypto = new SyncCrypto(globalThis.crypto);
const vaultKey = crypto.random(32);
const uuid = () => globalThis.crypto.randomUUID();

let server: FakeServer;
let clock: number;

async function account(): Promise<string> {
  const api = new Api(server.fetch);
  const token = await api.signUp('ada@example.com', 'a'.repeat(43));
  await api.putKeys(await crypto.wrapVaultKey(crypto.random(32), vaultKey));
  return token;
}

function device(token: string) {
  const lib = new Library(memorySql(), uuid, () => clock);
  const api = new Api(server.fetch, 'https://notes.test', token);
  const engine = new SyncEngine(lib, api, crypto, vaultKey, () => clock, uuid);
  return { lib, api, sync: () => engine.sync() };
}

/** Every note and folder by path, with note texts. */
function snapshot(lib: Library): Record<string, string | null> {
  return Object.fromEntries(
    lib
      .all()
      .sort((a, b) => a.path.localeCompare(b.path))
      .map((e) => [e.path, e.text]),
  );
}

beforeEach(() => {
  server = new FakeServer();
  clock = Date.UTC(2026, 10, 10, 9, 30);
});

describe('sync engine', () => {
  test('create, edit, move and delete reach the other device', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);

    const work = a.lib.createFolder('', 'Work');
    const id = a.lib.createNote('Work', '# Plan\n\nShip it.\n');
    await a.sync();
    await b.sync();
    expect(snapshot(b.lib)).toEqual({ Work: null, 'Work/Plan.md': '# Plan\n\nShip it.\n' });
    expect(b.lib.entry(id)?.path).toBe('Work/Plan.md');

    a.lib.writeText(id, '# Plan\n\nShip it on Friday.\n');
    await a.sync();
    await b.sync();
    expect(b.lib.note(id)?.text).toBe('# Plan\n\nShip it on Friday.\n');

    b.lib.move(id, '');
    await b.sync();
    await a.sync();
    expect(a.lib.entry(id)?.path).toBe('Plan.md');

    a.lib.trash(id);
    a.lib.trash(work);
    await a.sync();
    await b.sync();
    expect(snapshot(b.lib)).toEqual({});
    expect(b.lib.trashItems().map((t) => t.path).sort()).toEqual(['Plan.md', 'Work']);
  });

  test('renaming a folder moves its notes on the other device', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    const folder = a.lib.createFolder('', 'Trips');
    a.lib.createNote('Trips', '# Lisbon\n');
    await a.sync();
    await b.sync();
    a.lib.renameFolder(folder, 'Travel');
    await a.sync();
    await b.sync();
    expect(snapshot(b.lib)).toEqual({ Travel: null, 'Travel/Lisbon.md': '# Lisbon\n' });
    // Nothing left to push on either side.
    const before = server.requests.length;
    await b.sync();
    expect(server.requests.slice(before)).toEqual(['GET /v1/sync']);
  });

  test('both edited: the server keeps the file and the local text becomes a conflict note', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    const id = a.lib.createNote('', '# Groceries\n\n- milk\n');
    await a.sync();
    await b.sync();

    a.lib.writeText(id, '# Groceries\n\n- milk\n- bread\n');
    b.lib.writeText(id, '# Groceries\n\n- milk\n- eggs\n');
    await a.sync();
    await b.sync();
    await a.sync();

    const expected = {
      'Groceries.md': '# Groceries\n\n- milk\n- bread\n',
      'Groceries (conflict 2026-11-10 09.30).md': '# Groceries\n\n- milk\n- eggs\n',
    };
    const local = new Date(clock);
    const stamp = `2026-11-10 ${String(local.getHours()).padStart(2, '0')}.${String(local.getMinutes()).padStart(2, '0')}`;
    const withStamp = Object.fromEntries(
      Object.entries(expected).map(([k, v]) => [k.replace('2026-11-10 09.30', stamp), v]),
    );
    expect(snapshot(b.lib)).toEqual(withStamp);
    expect(snapshot(a.lib)).toEqual(withStamp);
  });

  test('a push answered conflict for both edits settles the same way', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    const id = a.lib.createNote('', '# Idea\n');
    await a.sync();
    await b.sync();
    a.lib.writeText(id, '# Idea\n\nA.\n');
    await a.sync();
    b.lib.writeText(id, '# Idea\n\nB.\n');
    // b pushes against a seq the server has moved past.
    await b.sync();
    expect(b.lib.note(id)?.text).toBe('# Idea\n\nA.\n');
    expect(b.lib.allNotes().filter((n) => n.path.includes('(conflict')).map((n) => n.text)).toEqual(['# Idea\n\nB.\n']);
  });

  test('server deleted, local edited: the edit is pushed again', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    const id = a.lib.createNote('', '# Keep me\n');
    await a.sync();
    await b.sync();
    a.lib.trash(id);
    b.lib.writeText(id, '# Keep me\n\nStill needed.\n');
    await a.sync();
    await b.sync();
    await a.sync();
    expect(b.lib.note(id)?.text).toBe('# Keep me\n\nStill needed.\n');
    expect(a.lib.note(id)?.text).toBe('# Keep me\n\nStill needed.\n');
  });

  test('local deleted, server edited: the server version comes back', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    const id = a.lib.createNote('', '# Draft\n');
    await a.sync();
    await b.sync();
    a.lib.writeText(id, '# Draft\n\nEdited on a.\n');
    b.lib.trash(id);
    await a.sync();
    await b.sync();
    expect(b.lib.note(id)?.text).toBe('# Draft\n\nEdited on a.\n');
    await a.sync();
    expect(a.lib.note(id)?.text).toBe('# Draft\n\nEdited on a.\n');
  });

  test('same path under two ids: the second to arrive becomes "<name> 2"', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    a.lib.createNote('', 'from a');
    b.lib.createNote('', 'from b');
    await a.sync();
    await b.sync();
    await a.sync();
    const expected = { 'Untitled.md': 'from a', 'Untitled 2.md': 'from b' };
    expect(snapshot(a.lib)).toEqual(expected);
    expect(snapshot(b.lib)).toEqual(expected);
  });

  test('first sync adopts the ids of matching files and pushes the rest', async () => {
    const token = await account();
    const a = device(token);
    a.lib.createFolder('', 'Journal');
    a.lib.createNote('Journal', '# Monday\n');
    await a.sync();
    const pushedBefore = server.users.get('ada@example.com')!.seq;

    const b = device(token);
    b.lib.createFolder('', 'Journal');
    b.lib.createNote('Journal', '# Monday\n');
    b.lib.createNote('Journal', '# Tuesday\n');
    await b.sync();
    const ids = [...server.users.get('ada@example.com')!.objects.keys()];
    expect(ids.length).toBe(3);
    expect(server.users.get('ada@example.com')!.seq).toBe(pushedBefore + 1);
    expect(b.lib.all().every((e) => ids.includes(e.id))).toBe(true);
  });

  test('a push whose answer was lost settles on the next pull', async () => {
    const token = await account();
    const a = device(token);
    const id = a.lib.createNote('', '# Lost\n');
    server.dropNextPushAnswer = true;
    await expect(a.sync()).rejects.toThrow();
    await a.sync();
    const objects = server.users.get('ada@example.com')!.objects;
    expect(objects.size).toBe(1);
    expect(objects.get(id)?.seq).toBe(1);
    expect(snapshot(a.lib)).toEqual({ 'Lost.md': '# Lost\n' });
    const before = server.requests.length;
    await a.sync();
    expect(server.requests.slice(before)).toEqual(['GET /v1/sync']);
  });

  test('pulls page through more than one batch and pushes in batches of 100', async () => {
    const token = await account();
    const a = device(token);
    for (let i = 0; i < 620; i++) a.lib.createNote('', `# Note ${i}\n`);
    await a.sync();
    expect(server.requests.filter((r) => r === 'POST /v1/sync').length).toBe(7);
    const b = device(token);
    await b.sync();
    expect(server.requests.filter((r) => r === 'GET /v1/sync').length).toBe(1 + 2);
    expect(b.lib.allNotes().length).toBe(620);
    expect(b.lib.meta('sync.cursor')).toBe('620');
  });

  test('a .folder.json syncs as a file object', async () => {
    const token = await account();
    const a = device(token);
    const b = device(token);
    a.lib.createFolder('', 'Tasks');
    a.lib.setFolderView('Tasks', { view: 'table', columns: [{ name: 'status', type: 'select', options: ['Todo', 'Done'] }], raw: { extra: 1 } });
    await a.sync();
    await b.sync();
    const view = b.lib.folderView('Tasks');
    expect(view.view).toBe('table');
    expect(view.columns[0].name).toBe('status');
    expect(view.raw.extra).toBe(1);
  });
});
