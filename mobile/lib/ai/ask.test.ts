import { describe, expect, test } from 'bun:test';

import { memorySql } from '../library/bun-sql';
import { Library } from '../library/store';
import { DEFAULTS, ask, AskError, type AskEvent } from './ask';

function sse(events: unknown[], named = true): Response {
  const body = events
    .map((e) => (named ? `event: ${(e as { type: string }).type}\ndata: ${JSON.stringify(e)}\n\n` : `data: ${typeof e === 'string' ? e : JSON.stringify(e)}\n\n`))
    .join('');
  const bytes = new TextEncoder().encode(body);
  // Split mid-event so the parser has to join chunks.
  const stream = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(bytes.slice(0, 37));
      c.enqueue(bytes.slice(37));
      c.close();
    },
  });
  return new Response(stream, { status: 200 });
}

function library() {
  let n = 0;
  const lib = new Library(memorySql(), () => `note-${++n}`);
  lib.createNote('', '# Packing list\n\n- Passport\n- Rain jacket\n');
  return lib;
}

describe('ask', () => {
  test('anthropic: runs the tool loop, streams text, cites what it read', async () => {
    const lib = library();
    const bodies: any[] = [];
    const rounds = [
      sse([
        { type: 'message_start', message: {} },
        { type: 'content_block_start', index: 0, content_block: { type: 'thinking', thinking: '' } },
        { type: 'content_block_delta', index: 0, delta: { type: 'signature_delta', signature: 'sig' } },
        { type: 'content_block_stop', index: 0 },
        { type: 'content_block_start', index: 1, content_block: { type: 'tool_use', id: 'tu1', name: 'read_note', input: {} } },
        { type: 'content_block_delta', index: 1, delta: { type: 'input_json_delta', partial_json: '{"id":' } },
        { type: 'content_block_delta', index: 1, delta: { type: 'input_json_delta', partial_json: '"note-1"}' } },
        { type: 'content_block_stop', index: 1 },
        { type: 'message_delta', delta: { stop_reason: 'tool_use' } },
        { type: 'message_stop' },
      ]),
      sse([
        { type: 'content_block_start', index: 0, content_block: { type: 'text', text: '' } },
        { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: 'Your passport, ' } },
        { type: 'content_block_delta', index: 0, delta: { type: 'text_delta', text: 'from Packing list.' } },
        { type: 'message_delta', delta: { stop_reason: 'end_turn' } },
      ]),
    ];
    const events: AskEvent[] = [];
    await ask('What do I pack?', {
      settings: DEFAULTS.anthropic,
      key: 'sk-test',
      lib,
      emit: (e) => events.push(e),
      fetch: async (_url, init) => {
        bodies.push(JSON.parse(String(init.body)));
        return rounds.shift()!;
      },
    });
    expect(events.filter((e) => e.type === 'text').map((e) => (e as { text: string }).text).join('')).toBe(
      'Your passport, from Packing list.',
    );
    expect(events).toContainEqual({ type: 'cite', id: 'note-1' });
    const second = bodies[1];
    expect(second.messages[1].content[0]).toEqual({ type: 'thinking', thinking: '', signature: 'sig' });
    expect(second.messages[2].content[0].tool_use_id).toBe('tu1');
    expect(second.messages[2].content[0].content).toContain('Rain jacket');
    expect(second.temperature).toBeUndefined();
  });

  test('openai-compatible: tool calls arrive in pieces', async () => {
    const lib = library();
    const rounds = [
      sse(
        [
          { choices: [{ delta: { tool_calls: [{ index: 0, id: 'c1', function: { name: 'search_notes', arguments: '{"que' } }] } }] },
          { choices: [{ delta: { tool_calls: [{ index: 0, function: { arguments: 'ry":"passport"}' } }] }, finish_reason: 'tool_calls' }] },
          '[DONE]',
        ],
        false,
      ),
      sse([{ choices: [{ delta: { content: 'In Packing list.' }, finish_reason: 'stop' }] }, '[DONE]'], false),
    ];
    const bodies: any[] = [];
    const events: AskEvent[] = [];
    await ask('passport?', {
      settings: DEFAULTS.openai,
      key: 'sk-test',
      lib,
      emit: (e) => events.push(e),
      fetch: async (_url, init) => {
        bodies.push(JSON.parse(String(init.body)));
        return rounds.shift()!;
      },
    });
    expect(bodies[1].messages.at(-1)).toMatchObject({ role: 'tool', tool_call_id: 'c1' });
    expect(events.at(-1)).toEqual({ type: 'text', text: 'In Packing list.' });
  });

  test('errors name what went wrong', async () => {
    const lib = library();
    const base = { settings: DEFAULTS.anthropic, lib, emit: () => {} };
    await expect(ask('x', { ...base, key: null, fetch: async () => new Response() })).rejects.toMatchObject({ kind: 'no_key' });
    await expect(
      ask('x', { ...base, key: 'bad', fetch: async () => new Response('{"error":{"message":"invalid x-api-key"}}', { status: 401 }) }),
    ).rejects.toMatchObject({ kind: 'bad_key' });
    await expect(
      ask('x', {
        ...base,
        key: 'k',
        fetch: async () => {
          throw new TypeError('Network request failed');
        },
      }),
    ).rejects.toBeInstanceOf(AskError);
  });
});
