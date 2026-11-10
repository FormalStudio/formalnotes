// This app's sync engine against a running server.
//
//   bun scripts/live-check.ts [base URL]                     two devices through a throwaway account
//   bun scripts/live-check.ts --pull <email> [base URL]      what an account holds, as the phone sees it
//
// The password for --pull comes from FORMALNOTES_PASSWORD, so it stays out of shell history.

import { memorySql } from '../lib/library/bun-sql';
import { Library } from '../lib/library/store';
import * as account from '../lib/sync/account';
import { Api, SERVER_URL, type Fetch } from '../lib/sync/api';
import { SyncCrypto } from '../lib/sync/crypto';
import { SyncEngine } from '../lib/sync/engine';

const crypto = new SyncCrypto(globalThis.crypto);
const uuid = () => globalThis.crypto.randomUUID();
const args = process.argv.slice(2);
const pullAt = args.indexOf('--pull');
const base = (pullAt >= 0 ? args.filter((_, i) => i !== pullAt && i !== pullAt + 1) : args)[0] ?? SERVER_URL;

function device(session: account.Session) {
  const lib = new Library(memorySql(), uuid);
  const api = new Api(fetch as unknown as Fetch, base, session.token);
  const engine = new SyncEngine(lib, api, crypto, session.vaultKey, Date.now, uuid);
  return { lib, api, sync: () => engine.sync() };
}

function check(ok: boolean, what: string) {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
  if (!ok) process.exitCode = 1;
}

if (pullAt >= 0) {
  const email = args[pullAt + 1];
  const password = process.env.FORMALNOTES_PASSWORD;
  if (!email || !password) throw new Error('usage: FORMALNOTES_PASSWORD=... bun scripts/live-check.ts --pull <email>');
  const phone = device(await account.signIn(new Api(fetch as unknown as Fetch, base), crypto, email, password));
  const report = await phone.sync();
  console.log(`pulled ${report.pulled}`);
  for (const e of phone.lib.all().sort((a, b) => a.path.localeCompare(b.path))) console.log(`${e.kind.padEnd(6)} ${e.path}`);
} else {
  const email = `mobile-check-${Date.now()}@example.com`;
  const password = 'correct horse battery staple';
  const a = device(await account.signUp(new Api(fetch as unknown as Fetch, base), crypto, email, password));
  const first = a.lib.createNote('Inbox', '# Packing list\n\n- Passport\n');
  a.lib.createNote('Projects/Launch', '---\nstatus: Doing\n---\n# Plan\n\nShip it.\n');
  check((await a.sync()).pushed >= 4, 'device A pushes two notes and their folders');

  const b = device(await account.signIn(new Api(fetch as unknown as Fetch, base), crypto, email, password));
  await b.sync();
  check(b.lib.note(first)?.text === '# Packing list\n\n- Passport\n', 'device B pulls the note byte for byte');
  check(b.lib.byPath('Projects/Launch/Plan.md') !== null, 'device B has the nested note at its path');

  b.lib.writeText(first, '# Packing list\n\n- Passport\n- Charger\n');
  await b.sync();
  await a.sync();
  check(a.lib.note(first)?.text.includes('Charger') === true, 'an edit on B reaches A');

  await account.deleteAccount(b.api, crypto, email, password);
  const gone = await account.signIn(new Api(fetch as unknown as Fetch, base), crypto, email, password).then(
    () => false,
    () => true,
  );
  check(gone, 'the account is deleted');
}
