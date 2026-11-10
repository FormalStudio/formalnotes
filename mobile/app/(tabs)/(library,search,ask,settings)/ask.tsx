import { fetch } from 'expo/fetch';
import { router } from 'expo-router';
import { useRef, useState } from 'react';
import { Pressable, ScrollView, Text, TextInput, View, type ScrollViewInstance } from 'react-native';
import { KeyboardStickyView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { GlassSurface } from '@/components/glass-surface';
import { HeaderActions } from '@/components/header-actions';
import { Icon } from '@/components/icon';
import { markdownSpans } from '@/components/note/markdown';
import { NoteRow, RowSeparator } from '@/components/note-row';
import { ask, AskError, type StreamFetch } from '@/lib/ai/ask';
import { ai, useAi } from '@/lib/app/ai';
import { capture } from '@/lib/app/actions';
import { library, useLibrary } from '@/lib/app/library';
import { haptic } from '@/lib/haptics';
import { colors } from '@/lib/ui/colors';
import { BODY_LINE, BODY_TEXT, PAGE_PAD_X, TAB_BAR_CLEARANCE } from '@/lib/ui/metrics';

type Turn = {
  id: number;
  question: string;
  text: string;
  activity: string | null;
  cites: string[];
  state: 'running' | 'done' | 'stopped' | AskError;
};

function activityOf(name: string, input: Record<string, unknown>): string {
  if (name === 'search_notes') return `Searching for "${String(input.query ?? '')}"`;
  if (name === 'read_note') return `Reading ${library.note(String(input.id ?? ''))?.title ?? 'a note'}`;
  return 'Looking through your folders';
}

const ERRORS: Record<AskError['kind'], { title: string; action?: 'settings' | 'retry' }> = {
  no_key: { title: 'Ask needs a key from your model provider.', action: 'settings' },
  bad_key: { title: 'Your provider refused the key.', action: 'settings' },
  offline: { title: 'The model cannot be reached. Check your connection.', action: 'retry' },
  refused: { title: 'The model declined to answer this.' },
  rate_limited: { title: 'Your provider is limiting requests. Try again shortly.', action: 'retry' },
  failed: { title: 'The answer stopped partway.', action: 'retry' },
};

function Cites({ ids }: { ids: string[] }) {
  const notes = useLibrary((lib) => ids.flatMap((id) => lib.note(id) ?? []));
  if (notes.length === 0) return null;
  return (
    <View style={{ marginHorizontal: -PAGE_PAD_X }}>
      <Text style={{ paddingHorizontal: PAGE_PAD_X, paddingBottom: 2, fontSize: 13, fontWeight: '600', color: colors.secondary }}>
        From your notes
      </Text>
      {notes.map((n, i) => (
        <View key={n.id}>
          {i > 0 ? <RowSeparator /> : null}
          <NoteRow note={n} />
        </View>
      ))}
    </View>
  );
}

function ErrorCard({ error, onRetry }: { error: AskError; onRetry: () => void }) {
  const info = ERRORS[error.kind];
  return (
    <View style={{ gap: 8, padding: 14, borderRadius: 14, borderCurve: 'continuous', backgroundColor: colors.fill }}>
      <Text selectable style={{ fontSize: 15, color: colors.text }}>
        {info.title}
      </Text>
      {error.kind !== 'no_key' && error.message ? (
        <Text selectable style={{ fontSize: 13, color: colors.secondary }}>
          {error.message}
        </Text>
      ) : null}
      {info.action ? (
        <Pressable
          accessibilityRole="button"
          onPress={info.action === 'settings' ? () => router.navigate('/settings') : onRetry}
          style={({ pressed }) => ({ alignSelf: 'flex-start', paddingVertical: 4, opacity: pressed ? 0.5 : 1 })}
        >
          <Text style={{ fontSize: 15, fontWeight: '600', color: colors.accent }}>
            {info.action === 'settings' ? 'Open settings' : 'Try again'}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}

function Answer({ turn, onRetry }: { turn: Turn; onRetry: () => void }) {
  const status = turn.state === 'running' ? (turn.activity ?? 'Thinking') : turn.state === 'stopped' ? 'Stopped' : null;
  return (
    <View style={{ gap: 12 }}>
      <Text selectable style={{ fontSize: 22, lineHeight: 28, fontWeight: '700', color: colors.text }}>
        {turn.question}
      </Text>
      {turn.text !== '' ? (
        <Text selectable style={{ fontSize: BODY_TEXT, lineHeight: BODY_LINE, color: colors.text }}>
          {markdownSpans(turn.text)}
        </Text>
      ) : null}
      {status ? <Text style={{ fontSize: 15, color: colors.secondary }}>{status}</Text> : null}
      {turn.state instanceof AskError ? <ErrorCard error={turn.state} onRetry={onRetry} /> : null}
      <Cites ids={turn.cites} />
    </View>
  );
}

export default function Ask() {
  const insets = useSafeAreaInsets();
  const { settings, hasKey } = useAi();
  const [question, setQuestion] = useState('');
  const [turns, setTurns] = useState<Turn[]>([]);
  const running = turns.at(-1)?.state === 'running';
  const abort = useRef<AbortController | null>(null);
  const scroll = useRef<ScrollViewInstance>(null);
  const nextId = useRef(1);

  const update = (id: number, patch: (t: Turn) => Partial<Turn>) =>
    setTurns((all) => all.map((t) => (t.id === id ? { ...t, ...patch(t) } : t)));

  const run = async (q: string) => {
    const id = nextId.current++;
    setTurns((all) => [...all, { id, question: q, text: '', activity: null, cites: [], state: 'running' }]);
    const controller = new AbortController();
    abort.current = controller;
    try {
      await ask(q, {
        settings,
        key: await ai.key(),
        lib: library,
        fetch: fetch as unknown as StreamFetch,
        signal: controller.signal,
        emit: (e) => {
          if (e.type === 'text') update(id, (t) => ({ text: t.text + e.text, activity: null }));
          else if (e.type === 'tool') update(id, () => ({ activity: activityOf(e.name, e.input) }));
          else update(id, (t) => ({ cites: t.cites.includes(e.id) ? t.cites : [...t.cites, e.id] }));
        },
      });
      update(id, () => ({ state: 'done', activity: null }));
      haptic('success');
    } catch (e) {
      if (controller.signal.aborted) update(id, () => ({ state: 'stopped', activity: null }));
      else {
        update(id, () => ({ state: e instanceof AskError ? e : new AskError('failed', e instanceof Error ? e.message : ''), activity: null }));
        haptic('warning');
      }
    } finally {
      if (abort.current === controller) abort.current = null;
    }
  };

  const send = () => {
    const q = question.trim();
    if (q === '' || running) return;
    haptic('light');
    setQuestion('');
    void run(q);
  };

  return (
    <>
      <HeaderActions
        items={
          turns.length > 0 && !running
            ? [
                { kind: 'button', sf: 'square.and.pencil', md: 'edit_square', label: 'New note in Inbox', onPress: capture },
                { kind: 'button', sf: 'arrow.counterclockwise', md: 'refresh', label: 'Clear answers', onPress: () => setTurns([]) },
              ]
            : [{ kind: 'button', sf: 'square.and.pencil', md: 'edit_square', label: 'New note in Inbox', onPress: capture }]
        }
      />
      <ScrollView
        ref={scroll}
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="interactive"
        onContentSizeChange={() => {
          if (running) scroll.current?.scrollToEnd({ animated: false });
        }}
        contentContainerStyle={{ paddingHorizontal: PAGE_PAD_X, paddingTop: 8, paddingBottom: 140, gap: 32 }}
      >
        {turns.length === 0 ? (
          <View style={{ alignItems: 'center', gap: 8, paddingTop: 64, paddingHorizontal: 16 }}>
            <Icon sf="sparkles" md="auto_awesome" size={34} color={colors.tertiary} />
            <Text style={{ fontSize: 17, fontWeight: '600', color: colors.text }}>Ask your notes</Text>
            <Text style={{ fontSize: 15, color: colors.secondary, textAlign: 'center' }}>
              {hasKey
                ? `Answers come from ${settings.model}, which reads only the notes it needs.`
                : 'Add a key from Anthropic or an OpenAI-compatible provider in Settings to start.'}
            </Text>
            {hasKey ? null : (
              <Pressable accessibilityRole="button" onPress={() => router.navigate('/settings')} hitSlop={8}>
                <Text style={{ fontSize: 15, fontWeight: '600', color: colors.accent }}>Open settings</Text>
              </Pressable>
            )}
          </View>
        ) : (
          turns.map((t) => <Answer key={t.id} turn={t} onRetry={() => void run(t.question)} />)
        )}
      </ScrollView>
      <KeyboardStickyView offset={{ closed: 0, opened: insets.bottom + 56 }} style={{ position: 'absolute', left: 12, right: 12, bottom: insets.bottom + TAB_BAR_CLEARANCE }}>
        <GlassSurface
          interactive
          style={{ flexDirection: 'row', alignItems: 'flex-end', gap: 8, padding: 6, paddingLeft: 18, borderRadius: 26, borderCurve: 'continuous' }}
        >
          <TextInput
            accessibilityLabel="Question"
            value={question}
            onChangeText={setQuestion}
            placeholder="Ask about your notes"
            placeholderTextColor={colors.secondary}
            multiline
            submitBehavior="submit"
            returnKeyType="send"
            onSubmitEditing={send}
            style={{ flex: 1, maxHeight: 120, paddingVertical: 10, fontSize: 17, color: colors.text }}
          />
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={running ? 'Stop' : 'Ask'}
            disabled={!running && question.trim() === ''}
            onPress={running ? () => abort.current?.abort() : send}
            style={({ pressed }) => ({
              width: 40,
              height: 40,
              borderRadius: 20,
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: running || question.trim() !== '' ? colors.accent : colors.fill,
              opacity: pressed ? 0.7 : 1,
            })}
          >
            <Icon sf={running ? 'stop.fill' : 'arrow.up'} md={running ? 'stop' : 'arrow_upward'} size={18} color="white" />
          </Pressable>
        </GlassSurface>
      </KeyboardStickyView>
    </>
  );
}
