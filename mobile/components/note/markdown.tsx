import type { ReactNode } from 'react';
import { Text, type TextStyle } from 'react-native';

import { colors } from '@/lib/ui/colors';
import { BODY_TEXT, MONO } from '@/lib/ui/metrics';

const HEADING: Record<number, TextStyle> = {
  1: { fontSize: 26, fontWeight: '700' },
  2: { fontSize: 22, fontWeight: '700' },
  3: { fontSize: 19, fontWeight: '600' },
};

const MARK: TextStyle = { color: colors.tertiary };

/** `**bold**` and `` `code` `` inside a line, with their marks kept and dimmed. */
function inline(line: string, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let at = 0;
  for (const m of line.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > at) out.push(line.slice(at, i));
    const s = m[0];
    if (s.startsWith('**')) {
      out.push(
        <Text key={`${key}-${i}`} style={{ fontWeight: '700' }}>
          <Text style={MARK}>**</Text>
          {s.slice(2, -2)}
          <Text style={MARK}>**</Text>
        </Text>,
      );
    } else {
      out.push(
        <Text key={`${key}-${i}`} style={{ fontFamily: MONO, fontSize: BODY_TEXT - 2, color: colors.secondary }}>
          {s}
        </Text>,
      );
    }
    at = i + s.length;
  }
  if (at < line.length) out.push(line.slice(at));
  return out;
}

/**
 * The body as styled spans for a text field: the characters stay exactly as typed (marks
 * included) so the caret and the saved text never drift; only their look changes.
 */
export function markdownSpans(body: string): ReactNode[] {
  const lines = body.split('\n');
  let code = false;
  return lines.map((line, n) => {
    const key = String(n);
    const end = n < lines.length - 1 ? '\n' : '';
    const t = line.trimStart();
    if (t.startsWith('```')) {
      code = !code;
      return (
        <Text key={key} style={{ fontFamily: MONO, fontSize: BODY_TEXT - 2, color: colors.tertiary }}>
          {line + end}
        </Text>
      );
    }
    if (code) {
      return (
        <Text key={key} style={{ fontFamily: MONO, fontSize: BODY_TEXT - 2, color: colors.secondary }}>
          {line + end}
        </Text>
      );
    }
    const heading = /^(#{1,6}) /.exec(line);
    if (heading) {
      const level = Math.min(heading[1].length, 3);
      return (
        <Text key={key} style={HEADING[level]}>
          <Text style={MARK}>{heading[0]}</Text>
          {line.slice(heading[0].length) + end}
        </Text>
      );
    }
    const task = /^(\s*[-*] \[([ xX])\] )(.*)$/.exec(line);
    if (task) {
      const done = task[2] !== ' ';
      return (
        <Text key={key}>
          <Text style={{ color: done ? colors.accent : colors.secondary }}>{task[1]}</Text>
          <Text style={done ? { color: colors.secondary, textDecorationLine: 'line-through' } : undefined}>{task[3]}</Text>
          {end}
        </Text>
      );
    }
    const bullet = /^(\s*(?:[-*]|\d+\.) )(.*)$/.exec(line);
    if (bullet) {
      return (
        <Text key={key}>
          <Text style={{ color: colors.secondary }}>{bullet[1]}</Text>
          {inline(bullet[2], key)}
          {end}
        </Text>
      );
    }
    if (t.startsWith('>')) {
      return (
        <Text key={key} style={{ color: colors.secondary, fontStyle: 'italic' }}>
          {line + end}
        </Text>
      );
    }
    return (
      <Text key={key}>
        {inline(line, key)}
        {end}
      </Text>
    );
  });
}
