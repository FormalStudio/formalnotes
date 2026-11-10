import { useEffect, useImperativeHandle, useLayoutEffect, useRef, type Ref } from 'react';
import { Pressable, TextInput, View, type TextInputInstance } from 'react-native';

import { Icon } from '@/components/icon';
import { inline, markdownSpans } from '@/components/note/markdown';
import { haptic } from '@/lib/haptics';
import {
  blocks,
  bold,
  caretAfter,
  continueList,
  locate,
  setKind,
  toggleTask,
  type Block,
  type Edit,
  type LineKind,
  type Selection,
} from '@/lib/library/editing';
import { colors } from '@/lib/ui/colors';
import { BODY_LINE, BODY_TEXT } from '@/lib/ui/metrics';

const TEXT = { fontSize: BODY_TEXT, lineHeight: BODY_LINE, color: colors.text, padding: 0 } as const;
const BOX = 22;
/** From the box to its task's text. */
const BOX_GAP = 10;
/** Text sits below the middle of its line box, by the leading the line height adds above it. */
const NUDGE = 3;
/** A space in the body font, for a nested task's indent. */
const SPACE = BODY_TEXT * 0.26;
/** Focus passing from one block to the next is not the end of typing. */
const BLUR_REST = 80;

export type BodyEditorHandle = { focus: () => void; kind: (kind: LineKind) => void; bold: () => void };

type Props = {
  ref: Ref<BodyEditorHandle>;
  body: string;
  autoFocus: boolean;
  onChange: (body: string) => void;
  onTyping: (typing: boolean) => void;
};

const at = (n: number): Selection => ({ start: n, end: n });
const shape = (list: Block[]) => list.map((b) => b.kind[1]).join('');

/**
 * The note's body. A task is a row of its own, a checkbox beside a field holding its text, so
 * the text wraps under itself; the lines between tasks share one field. The file's Markdown is
 * the only state: every edit rewrites the body and the rows are cut from it again.
 */
export function BodyEditor({ ref, body, autoFocus, onChange, onTyping }: Props) {
  const list = blocks(body);
  const inputs = useRef<(TextInputInstance | null)[]>([]);
  /** The caret or selection as places in the whole body. */
  const caret = useRef<Selection>(at(0));
  /** Where the caret goes once the rows for an edit are on screen. */
  const want = useRef<Selection | null>(null);
  /** An edit of ours is going in: what the fields report meanwhile is from before it. */
  const settling = useRef(false);
  const replaced = useRef<string | null>(null);
  const rest = useRef<ReturnType<typeof setTimeout> | null>(null);
  /**
   * Each field's own selection, by row. A field tapped where its caret already was reports
   * nothing, so the caret is read from here when a field takes focus.
   */
  const held = useRef<Record<number, Selection>>({});
  const focused = useRef<TextInputInstance | null>(null);

  const place = (sel: Selection) => {
    const a = locate(list, sel.start);
    const b = locate(list, sel.end);
    const input = inputs.current[a.index];
    const gone = focused.current !== null && !inputs.current.includes(focused.current);
    input?.focus();
    requestAnimationFrame(() => {
      // The field that had focus left the screen with this edit and took the keyboard's focus
      // with it; the first request came before that, so it is made again.
      if (gone) {
        input?.blur();
        input?.focus();
      }
      const end = b.index === a.index ? b.at : a.at;
      input?.setSelection(a.at, end);
      held.current[a.index] = { start: a.at, end };
      settling.current = false;
    });
  };

  useLayoutEffect(() => {
    const sel = want.current;
    want.current = null;
    if (sel) place(sel);
  });

  useEffect(() => {
    if (autoFocus) place(at(body.length));
    return () => {
      if (rest.current) clearTimeout(rest.current);
    };
    // Only as the editor opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** Puts an edit of ours in the body, with the caret where the edit left it. */
  const apply = (edit: Edit) => {
    caret.current = edit.selection;
    settling.current = true;
    if (edit.text === body) {
      place(edit.selection);
      return;
    }
    want.current = edit.selection;
    onChange(edit.text);
  };

  useImperativeHandle(ref, () => ({
    focus: () => place(at(0)),
    kind: (kind) => apply(setKind(body, caret.current, kind)),
    bold: () => apply(bold(body, caret.current)),
  }));

  const splice = (b: Block, to: string) => {
    const length = (b.kind === 'task' ? b.marker.length : 0) + b.text.length;
    return body.slice(0, b.start) + to + body.slice(b.start + length);
  };

  /** A field can report one keystroke twice; the second report would undo the edit made for the first. */
  const repeated = (typed: string) => settling.current && typed === replaced.current;

  const typedText = (b: Block, typed: string) => {
    if (repeated(typed)) return;
    const { start, end } = caret.current;
    const carried = start === end ? continueList(b.text, typed, start - b.start) : null;
    const next = splice(b, carried?.text ?? typed);
    if (!carried && shape(blocks(next)) === shape(list)) {
      onChange(next);
      return;
    }
    replaced.current = typed;
    apply({ text: next, selection: at(b.start + (carried?.selection.start ?? caretAfter(b.text, typed))) });
  };

  const typedTask = (b: Block & { kind: 'task' }, typed: string) => {
    if (repeated(typed)) return;
    const next = splice(b, b.marker + typed);
    // Only a paste brings a return here: the return key goes to `entered`.
    if (!typed.includes('\n')) {
      onChange(next);
      return;
    }
    replaced.current = typed;
    apply({ text: next, selection: at(b.start + b.marker.length + caretAfter(b.text, typed)) });
  };

  /** The return key in a task: the next task, or the end of the list from an empty one. */
  const entered = () => {
    const { start, end } = caret.current;
    const prev = body.slice(0, start) + body.slice(end);
    const next = `${prev.slice(0, start)}\n${prev.slice(start)}`;
    apply(continueList(prev, next, start) ?? { text: next, selection: at(start + 1) });
  };

  /** A backspace at the start of a row: a task loses its box, a line joins the task above it. */
  const backspace = (b: Block) => {
    const lead = b.start + (b.kind === 'task' ? b.marker.length : 0);
    const { start, end } = caret.current;
    if (start !== end || start !== lead) return;
    if (b.kind === 'task') {
      const indent = b.marker.length - b.marker.trimStart().length;
      apply({ text: splice(b, b.marker.slice(0, indent) + b.text), selection: at(b.start + indent) });
    } else if (b.start > 0) {
      apply({ text: body.slice(0, b.start - 1) + body.slice(b.start), selection: at(b.start - 1) });
    }
  };

  const field = (b: Block, index: number) => {
    const lead = b.start + (b.kind === 'task' ? b.marker.length : 0);
    return {
      ref: (input: TextInputInstance | null) => {
        inputs.current[index] = input;
      },
      multiline: true,
      scrollEnabled: false,
      textAlignVertical: 'top' as const,
      onSelectionChange: (e: { nativeEvent: { selection: Selection } }) => {
        const s = e.nativeEvent.selection;
        if (settling.current) return;
        held.current[index] = s;
        caret.current = { start: lead + s.start, end: lead + s.end };
      },
      onKeyPress: (e: { nativeEvent: { key: string } }) => {
        if (e.nativeEvent.key === 'Backspace') backspace(b);
      },
      onFocus: () => {
        if (rest.current) clearTimeout(rest.current);
        rest.current = null;
        focused.current = inputs.current[index];
        const s = held.current[index];
        if (s && !settling.current) caret.current = { start: lead + s.start, end: lead + s.end };
        onTyping(true);
      },
      onBlur: () => {
        if (rest.current) clearTimeout(rest.current);
        rest.current = setTimeout(() => onTyping(false), BLUR_REST);
      },
    };
  };

  const last = list[list.length - 1];
  return (
    <Pressable accessible={false} onPress={() => place(at(body.length))} style={{ minHeight: 240 }}>
      {list.map((b, index) =>
        b.kind === 'text' ? (
          <TextInput
            key={`text${index}`}
            {...field(b, index)}
            accessibilityLabel="Body"
            onChangeText={(typed) => typedText(b, typed)}
            placeholder={body === '' ? 'Start writing' : undefined}
            placeholderTextColor={colors.tertiary}
            style={TEXT}
          >
            {markdownSpans(b.text)}
          </TextInput>
        ) : (
          <View key={`task${index}`} style={{ flexDirection: 'row', paddingLeft: (b.marker.length - b.marker.trimStart().length) * SPACE }}>
            <Pressable
              accessibilityRole="checkbox"
              accessibilityState={{ checked: b.done }}
              accessibilityLabel={b.text}
              hitSlop={{ top: 2, bottom: 2, left: 16, right: BOX_GAP / 2 }}
              onPress={() => {
                haptic('light');
                onChange(toggleTask(body, b.line));
              }}
              style={({ pressed }) => ({
                width: BOX,
                height: BOX,
                marginTop: (BODY_LINE - BOX) / 2 + NUDGE,
                marginRight: BOX_GAP,
                borderRadius: BOX / 2,
                borderWidth: b.done ? 0 : 1.5,
                borderColor: colors.tertiary,
                backgroundColor: b.done ? colors.accent : 'transparent',
                alignItems: 'center',
                justifyContent: 'center',
                transform: [{ scale: pressed ? 0.9 : 1 }],
              })}
            >
              {b.done ? <Icon sf="checkmark" md="check" size={11} color="white" /> : null}
            </Pressable>
            <TextInput
              {...field(b, index)}
              accessibilityLabel="Task"
              onChangeText={(typed) => typedTask(b, typed)}
              // Return is ours to answer: a field that took it would show a line its row does not have.
              submitBehavior="submit"
              onSubmitEditing={entered}
              style={[TEXT, { flex: 1 }, b.done ? { color: colors.secondary, textDecorationLine: 'line-through' } : null]}
            >
              {inline(b.text, String(index), true)}
            </TextInput>
          </View>
        ),
      )}
      {/* Room to tap under a note that ends in a task. */}
      {last.kind === 'task' ? <View style={{ height: BODY_LINE }} /> : null}
    </Pressable>
  );
}
