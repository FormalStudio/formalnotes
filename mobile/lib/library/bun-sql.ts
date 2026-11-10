// bun:sqlite behind the store's Sql interface, for tests.

import { Database } from 'bun:sqlite';

import type { Sql, SqlValue } from './sql';

export function memorySql(): Sql {
  const db = new Database(':memory:');
  return {
    run: (sql: string, params: SqlValue[] = []) => void db.query(sql).run(...params),
    all: <T>(sql: string, params: SqlValue[] = []) => db.query(sql).all(...params) as T[],
    get: <T>(sql: string, params: SqlValue[] = []) => (db.query(sql).get(...params) as T | null) ?? null,
    tx: <T>(fn: () => T) => db.transaction(fn)(),
  };
}
