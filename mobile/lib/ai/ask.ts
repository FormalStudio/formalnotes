// Ask: a question answered by the user's own model through a tool loop over the local library.
// Anthropic Messages or an OpenAI-compatible chat completions endpoint, both streamed.

import type { Library } from '../library/store';
import { readSse } from './sse';
import { SYSTEM_PROMPT, TOOLS, runTool } from './tools';

export type Provider = 'anthropic' | 'openai';

export type AiSettings = { provider: Provider; model: string; baseUrl: string };

export const DEFAULTS: Record<Provider, AiSettings> = {
  anthropic: { provider: 'anthropic', model: 'claude-opus-5-5', baseUrl: 'https://api.anthropic.com' },
  openai: { provider: 'openai', model: 'gpt-5', baseUrl: 'https://api.openai.com/v1' },
};

export type AskErrorKind = 'no_key' | 'bad_key' | 'offline' | 'refused' | 'rate_limited' | 'failed';

export class AskError extends Error {
  constructor(
    readonly kind: AskErrorKind,
    message: string,
  ) {
    super(message);
  }
}

export type AskEvent =
  | { type: 'text'; text: string }
  | { type: 'tool'; name: string; input: Record<string, unknown> }
  | { type: 'cite'; id: string };

export type StreamFetch = (url: string, init: RequestInit) => Promise<Response>;

const MAX_ROUNDS = 10;
const MAX_TOKENS = 16000;
const ANTHROPIC_VERSION = '2026-10-21';
/** Models that take server-side fallbacks on a refusal. */
const FALLBACK_MODELS = new Set(['claude-opus-5-5', 'claude-opus-5', 'claude-fable-5-1', 'claude-sonnet-5-5']);

type Ctx = {
  settings: AiSettings;
  key: string;
  lib: Library;
  fetch: StreamFetch;
  emit: (e: AskEvent) => void;
  signal?: AbortSignal;
};

async function post(ctx: Ctx, url: string, headers: Record<string, string>, body: unknown): Promise<Response> {
  let response: Response;
  try {
    response = await ctx.fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream', ...headers },
      body: JSON.stringify(body),
      signal: ctx.signal,
    });
  } catch (e) {
    if (ctx.signal?.aborted) throw e;
    throw new AskError('offline', 'The model could not be reached.');
  }
  if (response.ok && response.body) return response;
  const text = await response.text().catch(() => '');
  let message = text;
  try {
    const parsed = JSON.parse(text) as { error?: { message?: string } | string };
    message = typeof parsed.error === 'string' ? parsed.error : (parsed.error?.message ?? text);
  } catch {}
  if (response.status === 401 || response.status === 403) throw new AskError('bad_key', message || 'The key was refused.');
  if (response.status === 429) throw new AskError('rate_limited', message || 'Too many requests.');
  throw new AskError('failed', message || `HTTP ${response.status}`);
}

function runAndCite(ctx: Ctx, name: string, input: Record<string, unknown>): { content: string; isError: boolean } {
  ctx.emit({ type: 'tool', name, input });
  const out = runTool(ctx.lib, name, input);
  if (out.read) ctx.emit({ type: 'cite', id: out.read });
  return out;
}

function parseInput(json: string): Record<string, unknown> | null {
  if (json.trim() === '') return {};
  try {
    const v = JSON.parse(json) as unknown;
    return v && typeof v === 'object' && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
  } catch {
    return null;
  }
}

// Anthropic

type Block = { type: string; [key: string]: unknown };

async function askAnthropic(ctx: Ctx, question: string): Promise<void> {
  const messages: { role: 'user' | 'assistant'; content: unknown }[] = [{ role: 'user', content: question }];
  const tools = TOOLS.map((t) => ({
    name: t.name,
    description: t.description,
    input_schema: t.parameters,
    eager_input_streaming: true,
  }));
  const base = ctx.settings.baseUrl.replace(/\/+$/, '');
  const fallbacks = FALLBACK_MODELS.has(ctx.settings.model) && base === DEFAULTS.anthropic.baseUrl;
  const headers: Record<string, string> = { 'x-api-key': ctx.key, 'anthropic-version': ANTHROPIC_VERSION };
  if (fallbacks) headers['anthropic-beta'] = 'server-side-fallback-2026-07-01';

  for (let round = 0; round < MAX_ROUNDS; round++) {
    const response = await post(ctx, `${base}/v1/messages`, headers, {
      model: ctx.settings.model,
      max_tokens: MAX_TOKENS,
      stream: true,
      system: SYSTEM_PROMPT,
      tools,
      messages,
      output_config: { effort: 'low' },
      ...(fallbacks ? { fallbacks: 'default' } : {}),
    });
    const blocks: Block[] = [];
    const partial: string[] = [];
    let stop = '';
    for await (const { data } of readSse(response.body!)) {
      const e = JSON.parse(data) as Record<string, any>;
      if (e.type === 'content_block_start') {
        blocks[e.index] = { ...e.content_block };
        partial[e.index] = '';
      } else if (e.type === 'content_block_delta') {
        const b = blocks[e.index];
        const d = e.delta;
        if (d.type === 'text_delta') {
          b.text = String(b.text ?? '') + d.text;
          ctx.emit({ type: 'text', text: d.text });
        } else if (d.type === 'input_json_delta') partial[e.index] += d.partial_json;
        else if (d.type === 'thinking_delta') b.thinking = String(b.thinking ?? '') + d.thinking;
        else if (d.type === 'signature_delta') b.signature = d.signature;
      } else if (e.type === 'message_delta') {
        stop = e.delta?.stop_reason ?? stop;
      } else if (e.type === 'error') {
        throw new AskError(e.error?.type === 'overloaded_error' ? 'rate_limited' : 'failed', e.error?.message ?? 'Stream failed');
      }
    }
    if (stop === 'refusal') throw new AskError('refused', 'The model declined to answer this.');

    // After a fallback, model-internal blocks before the last boundary are not echoed back.
    const boundary = blocks.map((b) => b?.type).lastIndexOf('fallback');
    const results: Block[] = [];
    const content: Block[] = [];
    for (const [i, b] of blocks.entries()) {
      if (!b || b.type === 'fallback') continue;
      if (i < boundary && ['thinking', 'redacted_thinking', 'tool_use'].includes(b.type)) continue;
      if (b.type === 'tool_use') {
        const input = parseInput(partial[i] ?? '');
        b.input = input ?? {};
        const out =
          input === null || stop === 'max_tokens'
            ? { content: 'INVALID_JSON: the tool input did not arrive whole. Call the tool again.', isError: true }
            : runAndCite(ctx, String(b.name), input);
        results.push({ type: 'tool_result', tool_use_id: b.id, content: out.content, is_error: out.isError });
      }
      content.push(b);
    }
    messages.push({ role: 'assistant', content });
    if (stop !== 'tool_use' || results.length === 0) return;
    messages.push({ role: 'user', content: results });
  }
}

// OpenAI-compatible

async function askOpenAI(ctx: Ctx, question: string): Promise<void> {
  const messages: Record<string, unknown>[] = [
    { role: 'system', content: SYSTEM_PROMPT },
    { role: 'user', content: question },
  ];
  const tools = TOOLS.map((t) => ({
    type: 'function',
    function: { name: t.name, description: t.description, parameters: t.parameters },
  }));
  const base = ctx.settings.baseUrl.replace(/\/+$/, '');
  for (let round = 0; round < MAX_ROUNDS; round++) {
    const response = await post(ctx, `${base}/chat/completions`, { Authorization: `Bearer ${ctx.key}` }, {
      model: ctx.settings.model,
      stream: true,
      messages,
      tools,
    });
    let text = '';
    let finish = '';
    const calls: { id: string; name: string; args: string }[] = [];
    for await (const { data } of readSse(response.body!)) {
      if (data === '[DONE]') break;
      const chunk = JSON.parse(data) as Record<string, any>;
      if (chunk.error) throw new AskError('failed', chunk.error.message ?? 'Stream failed');
      const choice = chunk.choices?.[0];
      if (!choice) continue;
      const delta = choice.delta ?? {};
      if (typeof delta.content === 'string' && delta.content !== '') {
        text += delta.content;
        ctx.emit({ type: 'text', text: delta.content });
      }
      for (const tc of delta.tool_calls ?? []) {
        const at = (calls[tc.index ?? 0] ??= { id: '', name: '', args: '' });
        if (tc.id) at.id = tc.id;
        if (tc.function?.name) at.name += tc.function.name;
        if (tc.function?.arguments) at.args += tc.function.arguments;
      }
      if (choice.finish_reason) finish = choice.finish_reason;
    }
    if (finish === 'content_filter') throw new AskError('refused', 'The model declined to answer this.');
    const used = calls.filter(Boolean);
    messages.push({
      role: 'assistant',
      content: text === '' ? null : text,
      ...(used.length > 0
        ? { tool_calls: used.map((c) => ({ id: c.id, type: 'function', function: { name: c.name, arguments: c.args } })) }
        : {}),
    });
    if (used.length === 0) return;
    for (const c of used) {
      const input = parseInput(c.args);
      const out =
        input === null
          ? { content: 'INVALID_JSON: the arguments did not parse. Call the tool again.', isError: true }
          : runAndCite(ctx, c.name, input);
      messages.push({ role: 'tool', tool_call_id: c.id, content: out.content });
    }
  }
}

export async function ask(
  question: string,
  opts: { settings: AiSettings; key: string | null; lib: Library; fetch: StreamFetch; emit: (e: AskEvent) => void; signal?: AbortSignal },
): Promise<void> {
  if (!opts.key) throw new AskError('no_key', 'Add a key in Settings to ask your notes.');
  const ctx: Ctx = { ...opts, key: opts.key };
  if (opts.settings.provider === 'anthropic') await askAnthropic(ctx, question);
  else await askOpenAI(ctx, question);
}
