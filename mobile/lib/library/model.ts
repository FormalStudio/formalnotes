// Paths, titles and previews, the same rules as the desktop's src/library/model.rs.

export const NOTE_EXT = '.md';
export const FOLDER_VIEW = '.folder.json';

export function parentOf(path: string): string {
  const i = path.lastIndexOf('/');
  return i < 0 ? '' : path.slice(0, i);
}

export function nameOf(path: string): string {
  const i = path.lastIndexOf('/');
  return i < 0 ? path : path.slice(i + 1);
}

export function stemOf(path: string): string {
  const name = nameOf(path);
  return name.endsWith(NOTE_EXT) ? name.slice(0, -NOTE_EXT.length) : name;
}

export function join(folder: string, name: string): string {
  return folder === '' ? name : `${folder}/${name}`;
}

/** Whether `path` is `folder` or lies anywhere under it. The root contains everything. */
export function contains(folder: string, path: string): boolean {
  return folder === '' || path === folder || path.startsWith(`${folder}/`);
}

/** `path` with its leading `from` swapped for `to`, when it lies under `from`. */
export function rebase(path: string, from: string, to: string): string | null {
  if (path === from) return to;
  return path.startsWith(`${from}/`) ? to + path.slice(from.length) : null;
}

/** The folders from the root down to `folder`, root excluded. */
export function ancestry(folder: string): string[] {
  if (folder === '') return [];
  const parts = folder.split('/');
  return parts.map((_, i) => parts.slice(0, i + 1).join('/'));
}

/** A path from the library is relative, `/`-separated, NFC, never absolute, never `..`. */
export function isSafePath(path: string): boolean {
  if (path === '' || path.startsWith('/') || path !== path.normalize('NFC')) return false;
  return path.split('/').every((part) => part !== '' && part !== '..' && part !== '.');
}

/** Dot directories (`.trash` and the like) are not part of the library. */
export function isHidden(path: string): boolean {
  const parts = path.split('/');
  const folders = parts.slice(0, -1);
  const last = parts[parts.length - 1];
  return folders.some((p) => p.startsWith('.')) || (last.startsWith('.') && last !== FOLDER_VIEW);
}

/** A file name for a title: separators and refused characters become spaces, leading dots go. */
export function fileStemFor(title: string): string {
  let cleaned = '';
  for (const c of title) {
    const code = c.codePointAt(0) ?? 0;
    const control = code < 0x20 || (code >= 0x7f && code < 0xa0);
    cleaned += '/\\:*?"<>|'.includes(c) || control ? ' ' : c;
  }
  cleaned = cleaned.split(/\s+/).filter(Boolean).join(' ');
  cleaned = cleaned.replace(/^\.+/, '').trim();
  cleaned = Array.from(cleaned).slice(0, 120).join('');
  return cleaned === '' ? 'Untitled' : cleaned;
}

/** The leading properties block, when the text starts with one: its full length and inner lines. */
export function frontMatter(text: string): { end: number; lines: string[] } | null {
  if (!text.startsWith('---\n') && !text.startsWith('---\r\n')) return null;
  const lines = text.split('\n');
  for (let i = 1; i < lines.length; i++) {
    if (lines[i].replace(/\r$/, '') === '---') {
      const end = lines.slice(0, i + 1).join('\n').length + (i + 1 < lines.length ? 1 : 0);
      return { end, lines: lines.slice(1, i).map((l) => l.replace(/\r$/, '')) };
    }
  }
  return null;
}

/** The text after the properties block. */
export function bodyOf(text: string): string {
  const fm = frontMatter(text);
  return fm ? text.slice(fm.end) : text;
}

/** A note's title: its first `# ` heading after the properties block, else its file name. */
export function titleOf(text: string, stem: string): string {
  for (const line of bodyOf(text).split('\n')) {
    if (line.startsWith('# ')) {
      const t = line.slice(2).trim();
      if (t !== '') return t;
    }
  }
  return stem;
}

export type Split = { heading: string | null; body: string };

/** The leading `# ` heading, shown as the page title, and the rest. */
export function split(text: string): Split {
  const trimmed = text.replace(/^[\r\n]+/, '');
  const nl = trimmed.indexOf('\n');
  const first = nl < 0 ? trimmed : trimmed.slice(0, nl);
  const rest = nl < 0 ? '' : trimmed.slice(nl + 1);
  const line = first.replace(/\r$/, '');
  if (line.startsWith('# ')) {
    return { heading: line.slice(2).trim(), body: rest.replace(/^[\r\n]+/, '') };
  }
  return { heading: null, body: text };
}

export function compose(heading: string | null, body: string): string {
  if (heading === null) return body;
  return body === '' ? `# ${heading}\n` : `# ${heading}\n\n${body}`;
}

/** A note's text in the editor's three parts: properties block, title heading, body. */
export function editorParts(text: string): { props: string; heading: string | null; body: string } {
  const fm = frontMatter(text);
  const props = fm ? text.slice(0, fm.end) : '';
  const { heading, body } = split(text.slice(props.length));
  return { props, heading, body };
}

export function joinParts(props: string, heading: string | null, body: string): string {
  return props + compose(heading, body);
}

/** The text with its leading heading set to `title`, after the properties block. */
export function withHeading(text: string, title: string): string {
  const { props, body } = editorParts(text);
  return joinParts(props, title, body);
}

export type PreviewLine =
  | { kind: 'heading'; text: string }
  | { kind: 'bullet'; text: string }
  | { kind: 'task'; done: boolean; text: string }
  | { kind: 'quote'; text: string }
  | { kind: 'code'; text: string }
  | { kind: 'text'; text: string };

/** The first `max` blocks of a note's body (title and properties left out), for a card. */
export function preview(text: string, max: number): PreviewLine[] {
  const body = split(bodyOf(text)).body;
  const strip = (s: string) => s.replaceAll('**', '').replaceAll('`', '').trim();
  const out: PreviewLine[] = [];
  let inCode = false;
  for (const raw of body.split('\n')) {
    if (out.length >= max) break;
    const line = raw.trimEnd();
    if (line.trimStart().startsWith('```')) {
      inCode = !inCode;
      continue;
    }
    if (inCode) {
      out.push({ kind: 'code', text: line });
      continue;
    }
    const t = line.trimStart();
    if (t === '' || /^[-*_]+$/.test(t)) continue;
    const task = /^[-*] \[([ x])\] (.*)$/.exec(t);
    if (task) out.push({ kind: 'task', done: task[1] === 'x', text: strip(task[2]) });
    else if (t.startsWith('- ') || t.startsWith('* ')) out.push({ kind: 'bullet', text: strip(t.slice(2)) });
    else if (t.startsWith('#')) out.push({ kind: 'heading', text: strip(t.replace(/^#+/, '')) });
    else if (t.startsWith('>')) out.push({ kind: 'quote', text: strip(t.slice(1)) });
    else out.push({ kind: 'text', text: strip(t) });
  }
  return out;
}

/** A key per preview line from its content, counting repeats, so equal lines stay distinct. */
export function lineKeys(lines: PreviewLine[]): string[] {
  const seen = new Map<string, number>();
  return lines.map((l) => {
    const base = `${l.kind}:${l.text}`;
    const n = (seen.get(base) ?? 0) + 1;
    seen.set(base, n);
    return `${base}#${n}`;
  });
}

/** "Just now", "5 min ago", "3 h ago", "Yesterday", the weekday, then the date. */
export function relative(then: number, now: number): string {
  const minutes = Math.max(0, Math.floor((now - then) / 60000));
  const day = (t: number) => {
    const d = new Date(t);
    return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 86400000;
  };
  const days = day(now) - day(then);
  const date = new Date(then);
  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes} min ago`;
  if (days === 0) return `${Math.floor(minutes / 60)} h ago`;
  if (days === 1) return 'Yesterday';
  if (days >= 2 && days < 7) return date.toLocaleDateString('en-US', { weekday: 'long' });
  if (date.getFullYear() === new Date(now).getFullYear()) {
    return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
  }
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}

export function items(n: number): string {
  return n === 1 ? '1 item' : `${n} items`;
}
