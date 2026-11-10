// The edits the note editor makes for the writer, as plain text operations: ticking a task,
// carrying a list onto the next line, turning lines into tasks, bullets or headings, and bold.

export type Selection = { start: number; end: number };
export type Edit = { text: string; selection: Selection };
export type LineKind = 'task' | 'bullet' | 'heading';

const TASK = /^(\s*)[-*] \[([ xX])\] (.*)$/;
const MARKER = /^(\s*)([-*] \[[ xX]\] |[-*] |\d+\. |#{1,6} )?/;
const ITEM = /^(\s*)(?:([-*]) \[[ xX]\] |([-*]) |(\d+)\. )/;

export type Task = { line: number; done: boolean; indent: number; text: string };

/** The task lines of a body, by line number, outside code fences. */
export function tasks(body: string): Task[] {
  const out: Task[] = [];
  let code = false;
  body.split('\n').forEach((text, line) => {
    if (text.trimStart().startsWith('```')) code = !code;
    else if (!code) {
      const m = TASK.exec(text);
      if (m) out.push({ line, done: m[2] !== ' ', indent: m[1].length, text: m[3] });
    }
  });
  return out;
}

/** Ticks or unticks the task on a line. The text keeps its length, so a caret stays put. */
export function toggleTask(body: string, line: number): string {
  const lines = body.split('\n');
  const m = TASK.exec(lines[line] ?? '');
  if (!m) return body;
  const at = m[0].length - m[3].length - 3;
  lines[line] = lines[line].slice(0, at) + (m[2] === ' ' ? 'x' : ' ') + lines[line].slice(at + 1);
  return lines.join('\n');
}

/**
 * What a return typed at `caret` becomes when it ends a list item: the next item's marker, or,
 * on an item with nothing in it, the end of the list. Null when `next` is not `prev` with one
 * return typed at the caret, or the line is no list item.
 */
export function continueList(prev: string, next: string, caret: number): Edit | null {
  if (next !== `${prev.slice(0, caret)}\n${prev.slice(caret)}`) return null;
  const lineStart = prev.lastIndexOf('\n', caret - 1) + 1;
  const nl = prev.indexOf('\n', caret);
  const lineEnd = nl === -1 ? prev.length : nl;
  const m = ITEM.exec(prev.slice(lineStart, lineEnd));
  if (!m || caret - lineStart < m[0].length) return null;
  if (caret === lineEnd && lineEnd - lineStart === m[0].length) {
    return { text: prev.slice(0, lineStart) + prev.slice(caret), selection: { start: lineStart, end: lineStart } };
  }
  const marker = m[2] ? `${m[2]} [ ] ` : m[3] ? `${m[3]} ` : `${Number(m[4]) + 1}. `;
  const insert = `\n${m[1]}${marker}`;
  const at = caret + insert.length;
  return { text: prev.slice(0, caret) + insert + prev.slice(caret), selection: { start: at, end: at } };
}

/**
 * Turns the selected lines into `kind`, or back into plain lines when they all are that kind
 * already. A heading steps from second level to third and then off; a note's title is its first.
 */
export function setKind(body: string, sel: Selection, kind: LineKind): Edit {
  const lines = body.split('\n');
  const starts: number[] = [];
  let offset = 0;
  for (const l of lines) {
    starts.push(offset);
    offset += l.length + 1;
  }
  const lineAt = (pos: number) => {
    let n = 0;
    while (n + 1 < lines.length && starts[n + 1] <= pos) n++;
    return n;
  };
  const first = lineAt(sel.start);
  const last = lineAt(sel.end);
  const parts = lines.map((l) => {
    const m = MARKER.exec(l) ?? ['', '', undefined];
    return { indent: m[1] ?? '', marker: m[2] ?? '', rest: l.slice((m[0] ?? '').length) };
  });
  const picked = parts
    .slice(first, last + 1)
    .filter((p) => first === last || p.marker !== '' || p.rest.trim() !== '');
  const is = (marker: string) =>
    kind === 'task' ? /\[[ xX]\] $/.test(marker) : kind === 'bullet' ? /^[-*] $/.test(marker) : marker.startsWith('#');

  let marker: string;
  if (kind === 'heading') {
    const now = picked[0]?.marker ?? '';
    marker = now === '## ' ? '### ' : now.startsWith('#') ? '' : '## ';
  } else if (picked.every((p) => is(p.marker))) marker = '';
  else marker = kind === 'task' ? '- [ ] ' : '- ';

  let start = sel.start;
  let end = sel.end;
  let shift = 0;
  for (let n = first; n <= last; n++) {
    const p = parts[n];
    if (!picked.includes(p)) continue;
    // A task that is already one keeps its tick when its neighbours become tasks.
    const to = marker !== '' && kind !== 'heading' && is(p.marker) ? p.marker : marker;
    const indent = kind === 'heading' && to !== '' ? '' : p.indent;
    const before = p.indent.length + p.marker.length;
    const after = indent.length + to.length;
    const move = (pos: number) => {
      const col = pos - starts[n];
      if (col < 0 || col > lines[n].length) return null;
      return starts[n] + shift + (col <= before ? after : col + after - before);
    };
    start = (n === first ? move(sel.start) : null) ?? start;
    end = (n === last ? move(sel.end) : null) ?? end;
    lines[n] = indent + to + p.rest;
    shift += after - before;
  }
  return { text: lines.join('\n'), selection: { start, end } };
}

/** Wraps the selection in `**`, or unwraps it; with nothing selected, leaves the caret between a pair. */
export function bold(body: string, sel: Selection): Edit {
  const { start, end } = sel;
  if (body.slice(start - 2, start) === '**' && body.slice(end, end + 2) === '**') {
    return {
      text: body.slice(0, start - 2) + body.slice(start, end) + body.slice(end + 2),
      selection: { start: start - 2, end: end - 2 },
    };
  }
  return {
    text: `${body.slice(0, start)}**${body.slice(start, end)}**${body.slice(end)}`,
    selection: { start: start + 2, end: end + 2 },
  };
}

export type Block =
  | { kind: 'text'; start: number; text: string }
  | { kind: 'task'; start: number; line: number; marker: string; done: boolean; text: string };

/**
 * The body as the editor lays it out: each task on its own, the lines between them together.
 * `start` is where the block begins in the body. There is always at least one block.
 */
export function blocks(body: string): Block[] {
  const out: Block[] = [];
  const found = new Map(tasks(body).map((t) => [t.line, t]));
  let offset = 0;
  let open: { start: number; lines: string[] } | null = null;
  const close = () => {
    if (open) out.push({ kind: 'text', start: open.start, text: open.lines.join('\n') });
    open = null;
  };
  body.split('\n').forEach((text, line) => {
    const task = found.get(line);
    if (task) {
      close();
      out.push({ kind: 'task', start: offset, line, marker: text.slice(0, text.length - task.text.length), done: task.done, text: task.text });
    } else if (open) open.lines.push(text);
    else open = { start: offset, lines: [text] };
    offset += text.length + 1;
  });
  close();
  return out;
}

/** The block a place in the body falls in, and where in that block's own text. */
export function locate(list: Block[], offset: number): { index: number; at: number } {
  let index = 0;
  while (index + 1 < list.length && list[index + 1].start <= offset) index++;
  const b = list[index];
  const lead = b.kind === 'task' ? b.marker.length : 0;
  return { index, at: Math.min(Math.max(offset - b.start - lead, 0), b.text.length) };
}

/** Where the caret rests after typing turned `prev` into `next`: at the end of what changed. */
export function caretAfter(prev: string, next: string): number {
  let tail = 0;
  const most = Math.min(prev.length, next.length);
  while (tail < most && prev[prev.length - 1 - tail] === next[next.length - 1 - tail]) tail++;
  return next.length - tail;
}
