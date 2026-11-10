// The three tools the model answers with, run against the library on the phone.

import type { Library } from '../library/store';

export type ToolSpec = { name: string; description: string; parameters: Record<string, unknown> };

export const TOOLS: ToolSpec[] = [
  {
    name: 'search_notes',
    description:
      'Full-text search over the notes. Returns up to 20 notes with id, path, title and a snippet around the match. Search with a few distinctive words; try other words if nothing matches.',
    parameters: {
      type: 'object',
      properties: { query: { type: 'string', description: 'Words that should all appear in the note' } },
      required: ['query'],
      additionalProperties: false,
    },
  },
  {
    name: 'read_note',
    description: 'The full Markdown text of one note, by the id search_notes returned.',
    parameters: {
      type: 'object',
      properties: { id: { type: 'string', description: 'The note id' } },
      required: ['id'],
      additionalProperties: false,
    },
  },
  {
    name: 'list_folders',
    description: 'Every folder in the library with how many notes it holds, and the notes at the top level.',
    parameters: { type: 'object', properties: {}, additionalProperties: false },
  },
];

export const SYSTEM_PROMPT = `You answer questions about the user's notes, a library of Markdown files on their phone.
Find the notes that answer the question with the tools, read them, then answer briefly in plain sentences.
Say which notes the answer comes from by their titles. When the notes do not hold the answer, say so; do not make one up.`;

function snippet(text: string, words: string[]): string {
  const lower = text.toLowerCase();
  const at = words.map((w) => lower.indexOf(w)).find((i) => i >= 0) ?? 0;
  const start = Math.max(0, at - 80);
  return (start > 0 ? '…' : '') + text.slice(start, start + 240).replace(/\s+/g, ' ').trim();
}

export type ToolOutcome = { content: string; isError: boolean; read?: string; found?: string[] };

export function runTool(lib: Library, name: string, input: unknown): ToolOutcome {
  const args = (input && typeof input === 'object' ? input : {}) as Record<string, unknown>;
  if (name === 'search_notes') {
    if (typeof args.query !== 'string' || args.query.trim() === '') {
      return { content: 'query must be a non-empty string', isError: true };
    }
    const words = args.query.toLowerCase().split(/\s+/).filter(Boolean);
    let hits = lib.search(args.query, 20);
    // A question phrased in full words rarely matches every word; fall back to any of them.
    if (hits.length === 0 && words.length > 1) {
      const seen = new Set<string>();
      hits = words
        .flatMap((w) => lib.search(w, 10))
        .filter((n) => !seen.has(n.id) && !!seen.add(n.id))
        .slice(0, 20);
    }
    const out = hits.map((n) => ({ id: n.id, path: n.path, title: n.title, snippet: snippet(n.text, words) }));
    return { content: JSON.stringify(out), isError: false, found: hits.map((n) => n.id) };
  }
  if (name === 'read_note') {
    const note = typeof args.id === 'string' ? lib.note(args.id) : null;
    if (!note) return { content: 'No note has that id. Use an id from search_notes.', isError: true };
    return {
      content: JSON.stringify({ id: note.id, path: note.path, title: note.title, text: note.text }),
      isError: false,
      read: note.id,
    };
  }
  if (name === 'list_folders') {
    const folders = lib.allFolders().map((f) => ({ path: f.path, notes: lib.notesIn(f.path).length }));
    const top = lib.notesIn('').map((n) => ({ id: n.id, title: n.title }));
    return { content: JSON.stringify({ folders, topLevelNotes: top }), isError: false };
  }
  return { content: `Unknown tool ${name}`, isError: true };
}
