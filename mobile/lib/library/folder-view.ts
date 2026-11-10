// A folder's `.folder.json` (docs/protocol.md). Unknown keys survive a rewrite.

import type { PropertyType } from './properties';

export type Column = {
  name: string;
  type: PropertyType;
  options?: string[];
  width?: number;
  [key: string]: unknown;
};

export type FolderView = {
  view: 'cards' | 'table';
  columns: Column[];
  sort?: { column: string; descending: boolean };
  /** Every key of the file as read, so a rewrite keeps what this app does not know. */
  raw: Record<string, unknown>;
};

const TYPES: PropertyType[] = ['text', 'select', 'url', 'number', 'checkbox', 'date', 'multi_select'];

export const CARDS: FolderView = { view: 'cards', columns: [], raw: {} };

export function parseFolderView(text: string | null | undefined): FolderView {
  if (!text) return CARDS;
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    return CARDS;
  }
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return CARDS;
  const obj = raw as Record<string, unknown>;
  const columns = Array.isArray(obj.columns)
    ? obj.columns.filter(
        (c): c is Column =>
          !!c && typeof c === 'object' && typeof c.name === 'string' && TYPES.includes(c.type as PropertyType),
      )
    : [];
  const sort =
    obj.sort && typeof obj.sort === 'object'
      ? (obj.sort as { column?: unknown; descending?: unknown })
      : undefined;
  return {
    view: obj.view === 'table' ? 'table' : 'cards',
    columns,
    sort:
      sort && typeof sort.column === 'string'
        ? { column: sort.column, descending: sort.descending === true }
        : undefined,
    raw: obj,
  };
}

export function writeFolderView(view: FolderView): string {
  const out: Record<string, unknown> = { ...view.raw, v: 1, view: view.view, columns: view.columns };
  if (view.sort) out.sort = view.sort;
  return `${JSON.stringify(out, null, 2)}\n`;
}
