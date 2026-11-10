// The properties block at the top of a note (docs/protocol.md, "Properties"). Writing one property
// touches only its own line; every other byte of the block stays as it was.

import { frontMatter } from './model';

export type PropertyType = 'text' | 'select' | 'url' | 'number' | 'checkbox' | 'date' | 'multi_select';

export type PropertyValue = string | number | boolean | string[];

export type Property = { name: string; raw: string; value: PropertyValue };

const KEY = /^([^\s#\-[{"'][^:]*?):(?: (.*))?$/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const NUMBER = /^-?\d+(\.\d+)?$/;

function unquote(s: string): string | null {
  if (s.length >= 2 && s.startsWith('"') && s.endsWith('"')) {
    try {
      return JSON.parse(s) as string;
    } catch {
      return s.slice(1, -1);
    }
  }
  if (s.length >= 2 && s.startsWith("'") && s.endsWith("'")) return s.slice(1, -1).replaceAll("''", "'");
  return null;
}

function splitList(inner: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quote: string | null = null;
  for (let i = 0; i < inner.length; i++) {
    const c = inner[i];
    if (quote) {
      cur += c;
      if (c === '\\' && quote === '"' && i + 1 < inner.length) cur += inner[++i];
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
      cur += c;
    } else if (c === ',') {
      out.push(cur.trim());
      cur = '';
    } else cur += c;
  }
  if (cur.trim() !== '') out.push(cur.trim());
  return out.map((item) => unquote(item) ?? item);
}

export function parseValue(raw: string): PropertyValue {
  const s = raw.trim();
  const quoted = unquote(s);
  if (quoted !== null) return quoted;
  if (s.startsWith('[') && s.endsWith(']')) return splitList(s.slice(1, -1));
  if (s === 'true' || s === 'false') return s === 'true';
  if (NUMBER.test(s)) return Number(s);
  return s;
}

/** The properties a note's block holds, in order. Lines this app does not model are left out. */
export function readProperties(text: string): Property[] {
  const fm = frontMatter(text);
  if (!fm) return [];
  const out: Property[] = [];
  for (const line of fm.lines) {
    const m = KEY.exec(line);
    if (!m || m[2] === undefined || m[2].trim() === '') continue;
    out.push({ name: m[1].trim(), raw: m[2], value: parseValue(m[2]) });
  }
  return out;
}

/** The type a value reads as when no column says otherwise. */
export function inferType(value: PropertyValue): PropertyType {
  if (Array.isArray(value)) return 'multi_select';
  if (typeof value === 'boolean') return 'checkbox';
  if (typeof value === 'number') return 'number';
  if (DATE.test(value)) return 'date';
  if (/^https?:\/\//.test(value)) return 'url';
  return 'text';
}

function needsQuotes(s: string, inList: boolean): boolean {
  if (s === '') return true;
  if (s === 'true' || s === 'false' || NUMBER.test(s) || DATE.test(s)) return true;
  if (s !== s.trim()) return true;
  if (s.includes(': ') || s.endsWith(':')) return true;
  if (/^[["#-]/.test(s)) return true;
  return inList && (s.includes(',') || s.includes(']'));
}

function quote(s: string, inList: boolean): string {
  return needsQuotes(s, inList) ? JSON.stringify(s) : s;
}

/** A value as the block writes it for `type`; empty means the line goes. */
export function formatValue(type: PropertyType, value: PropertyValue): string {
  switch (type) {
    case 'number':
      return typeof value === 'number' && Number.isFinite(value) ? String(value) : String(value).trim();
    case 'checkbox':
      return value === true || value === 'true' ? 'true' : 'false';
    case 'date':
      return String(value).trim();
    case 'multi_select': {
      const list = Array.isArray(value) ? value : String(value).split(',').map((s) => s.trim());
      const kept = list.filter((s) => s !== '');
      return kept.length === 0 ? '' : `[${kept.map((s) => quote(s, true)).join(', ')}]`;
    }
    default: {
      const s = String(value);
      return s === '' ? '' : quote(s, false);
    }
  }
}

function isContinuation(line: string): boolean {
  return line.startsWith(' ') || line.startsWith('\t') || line.startsWith('- ');
}

/**
 * The text with property `name` set to `formatted` (already written by `formatValue`). An empty
 * value removes the line, and a block left with no lines goes with its fences.
 */
export function setProperty(text: string, name: string, formatted: string): string {
  const fm = frontMatter(text);
  if (!fm) {
    if (formatted === '') return text;
    return `---\n${name}: ${formatted}\n---\n${text}`;
  }
  const lines = [...fm.lines];
  const at = lines.findIndex((line) => KEY.exec(line)?.[1].trim() === name);
  if (at >= 0) {
    let end = at + 1;
    while (end < lines.length && isContinuation(lines[end])) end++;
    if (formatted === '') lines.splice(at, end - at);
    else lines.splice(at, end - at, `${name}: ${formatted}`);
  } else if (formatted !== '') {
    lines.push(`${name}: ${formatted}`);
  }
  const rest = text.slice(fm.end);
  if (lines.length === 0) return rest;
  const block = text.slice(0, fm.end);
  const nl = block.startsWith('---\r\n') ? '\r\n' : '\n';
  const trailing = block.endsWith('\n') ? nl : '';
  return `---${nl}${lines.join(nl)}${nl}---${trailing}${rest}`;
}
