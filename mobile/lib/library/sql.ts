// The slice of SQLite the store and the sync engine use: expo-sqlite on device, bun:sqlite in tests.

export type SqlValue = string | number | null;

export interface Sql {
  run(sql: string, params?: SqlValue[]): void;
  all<T>(sql: string, params?: SqlValue[]): T[];
  get<T>(sql: string, params?: SqlValue[]): T | null;
  /** Runs `fn` in one transaction; a throw rolls it back. */
  tx<T>(fn: () => T): T;
}

export const SCHEMA = [
  `CREATE TABLE IF NOT EXISTS entries (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE COLLATE NOCASE,
    kind TEXT NOT NULL,
    text TEXT,
    hash TEXT,
    created INTEGER NOT NULL,
    modified INTEGER NOT NULL
  )`,
  `CREATE TABLE IF NOT EXISTS trash (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT,
    created INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    trashed INTEGER NOT NULL,
    batch TEXT NOT NULL
  )`,
  `CREATE TABLE IF NOT EXISTS synced (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    hash TEXT NOT NULL,
    seq INTEGER NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0
  )`,
  `CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)`,
  `CREATE INDEX IF NOT EXISTS trash_batch ON trash (batch)`,
];

export function migrate(sql: Sql): void {
  for (const statement of SCHEMA) sql.run(statement);
}
