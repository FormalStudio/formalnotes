// The phone's library: one SQLite database, opened once, read by views through useLibrary.

import { openDatabaseSync } from 'expo-sqlite';
import { useSyncExternalStore } from 'react';
import QuickCrypto from 'react-native-quick-crypto';

import { seedDemo } from '../library/demo';
import type { Sql, SqlValue } from '../library/sql';
import { Library } from '../library/store';

const db = openDatabaseSync('formalnotes.db');
db.execSync('PRAGMA journal_mode = WAL');

let depth = 0;

const sql: Sql = {
  run: (source: string, params: SqlValue[] = []) => void db.runSync(source, params),
  all: <T>(source: string, params: SqlValue[] = []) => db.getAllSync<T>(source, params),
  get: <T>(source: string, params: SqlValue[] = []) => db.getFirstSync<T>(source, params),
  tx: <T>(fn: () => T): T => {
    // Nested calls join the outer transaction.
    if (depth > 0) return fn();
    depth++;
    db.execSync('BEGIN');
    try {
      const out = fn();
      db.execSync('COMMIT');
      return out;
    } catch (e) {
      db.execSync('ROLLBACK');
      throw e;
    } finally {
      depth--;
    }
  },
};

export const newId = (): string => QuickCrypto.randomUUID();

export const library = new Library(sql, newId);

if (process.env.EXPO_PUBLIC_FORMALNOTES_DEMO === '1') seedDemo(library, newId);

const subscribe = (listener: () => void) => library.subscribe(listener);
const version = () => library.version;

/** Reads from the library, again whenever it changes. */
export function useLibrary<T>(read: (lib: Library) => T): T {
  'use no memo';
  useSyncExternalStore(subscribe, version);
  return read(library);
}
