import { fetch } from 'expo/fetch';
import { Link, router } from 'expo-router';
import { useRef, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, Text, TextInput, useColorScheme, View, type ScrollViewInstance } from 'react-native';
import { useKeyboardHandler, useReanimatedKeyboardAnimation } from 'react-native-keyboard-controller';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { GlassSurface } from '@/components/glass-surface';
import { HeaderActions } from '@/components/header-actions';
import { Icon } from '@/components/icon';
import { markdownSpans } from '@/components/note/markdown';
import { ask, AskError, type StreamFetch } from '@/lib/ai/ask';
import { ai, useAi } from '@/lib/app/ai';
import { capture } from '@/lib/app/actions';
import { library, useLibrary } from '@/lib/app/library';
import { haptic } from '@/lib/haptics';
import { colors } from '@/lib/ui/colors';
import { BODY_LINE, BODY_TEXT, PAGE_PAD_X } from '@/lib/ui/metrics';

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

const SUGGESTIONS = ['What did I write this week?', 'What is still open on my lists?', 'Summarize my newest note'];

/** The composer's height at one line, its gap to the tab bar or the keyboard, and the room the conversation leaves for it. */
const COMPOSER = 44;
const SUBMIT = 34;
const COMPOSER_GAP = 8;
const FOOTER_CLEARANCE = COMPOSER + COMPOSER_GAP + 24;
/** How far above the composer the conversation starts to dissolve into the page. */
const FADE_OVERHANG = 32;

function Cites({ ids }: { ids: string[] }) {
  const notes = useLibrary((lib) => ids.flatMap((id) => lib.note(id) ?? []));
  if (notes.length === 0) return null;
  return (
    <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
      {notes.map((n) => (
        <Link key={n.id} href={{ pathname: '/note/[id]', params: { id: n.id } }} asChild>
          <Pressable accessibilityRole="link" accessibilityLabel={`Open ${n.title}`} hitSlop={6}>
            {({ pressed }) => (
              <View
                style={{
                  flexDirection: 'row',
                  alignItems: 'center',
                  gap: 6,
                  height: 32,
                  paddingHorizontal: 12,
                  borderRadius: 16,
                  backgroundColor: colors.fill,
                  opacity: pressed ? 0.6 : 1,
                }}
              >
                <Icon sf="doc.text" md="description" size={13} color={colors.secondary} />
                <Text numberOfLines={1} style={{ maxWidth: 200, fontSize: 14, color: colors.text }}>
                  {n.title}
                </Text>
              </View>
            )}
          </Pressable>
        </Link>
      ))}
    </View>
  );
}

function ErrorCard({ error, onRetry }: { error: AskError; onRetry: () => void }) {
  const info = ERRORS[error.kind];
  return (
    <View style={{ gap: 8, padding: 14, borderRadius: 16, borderCurve: 'continuous', backgroundColor: colors.fill }}>
      <Text selectable style={{ fontSize: 15, lineHeight: 20, color: colors.text }}>
        {info.title}
      </Text>
      {error.kind !== 'no_key' && error.message ? (
        <Text selectable style={{ fontSize: 13, lineHeight: 18, color: colors.secondary }}>
          {error.message}
        </Text>
      ) : null}
      {info.action ? (
        <Pressable
          accessibilityRole="button"
          hitSlop={8}
          onPress={info.action === 'settings' ? () => router.navigate('/settings') : onRetry}
          style={({ pressed }) => ({ alignSelf: 'flex-start', opacity: pressed ? 0.5 : 1 })}
        >
          <Text style={{ fontSize: 15, fontWeight: '600', color: colors.accent }}>
            {info.action === 'settings' ? 'Open Settings' : 'Try again'}
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}

/** One exchange: the question as a bubble on the right, the answer across the column under it. */
function Exchange({ turn, onRetry }: { turn: Turn; onRetry: () => void }) {
  const status = turn.state === 'running' ? (turn.activity ?? 'Thinking') : turn.state === 'stopped' ? 'Stopped' : null;
  return (
    <View style={{ gap: 12 }}>
      <View
        style={{
          alignSelf: 'flex-end',
          maxWidth: '82%',
          paddingHorizontal: 14,
          paddingVertical: 9,
          borderRadius: 20,
          borderCurve: 'continuous',
          backgroundColor: colors.fill,
        }}
      >
        <Text selectable style={{ fontSize: 16, lineHeight: 22, color: colors.text }}>
          {turn.question}
        </Text>
      </View>
      {turn.text !== '' ? (
        <Text selectable style={{ fontSize: BODY_TEXT, lineHeight: BODY_LINE, color: colors.text }}>
          {markdownSpans(turn.text, false)}
        </Text>
      ) : null}
      {status ? (
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
          {turn.state === 'running' ? <ActivityIndicator size="small" /> : null}
          <Text style={{ fontSize: 15, color: colors.secondary }}>{status}</Text>
        </View>
      ) : null}
      {turn.state instanceof AskError ? <ErrorCard error={turn.state} onRetry={onRetry} /> : null}
      <Cites ids={turn.cites} />
    </View>
  );
}

function Hero({ model, hasKey }: { model: string; hasKey: boolean }) {
  return (
    <View style={{ alignItems: 'center', gap: 8, paddingTop: 96, paddingHorizontal: 32 }}>
      <Icon sf="sparkles" md="auto_awesome" size={34} color={colors.tertiary} />
      <Text style={{ fontSize: 20, fontWeight: '700', color: colors.text }}>Ask your notes</Text>
      <Text style={{ fontSize: 15, lineHeight: 20, color: colors.secondary, textAlign: 'center' }}>
        {hasKey
          ? `Answers come from ${model}, which reads only the notes it needs.`
          : 'Add a key from Anthropic or an OpenAI-compatible provider to start.'}
      </Text>
      {hasKey ? null : (
        <Pressable
          accessibilityRole="button"
          onPress={() => router.navigate('/settings')}
          style={({ pressed }) => ({
            marginTop: 8,
            height: 44,
            paddingHorizontal: 20,
            borderRadius: 22,
            justifyContent: 'center',
            backgroundColor: colors.accent,
            opacity: pressed ? 0.75 : 1,
          })}
        >
          <Text style={{ fontSize: 16, fontWeight: '600', color: 'white' }}>Open Settings</Text>
        </Pressable>
      )}
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

  // The composer rests above the tab bar and only starts to move once the rising keyboard
  // reaches it, so it rides the keys instead of running ahead of them.
  const keyboard = useReanimatedKeyboardAnimation();
  const rest = insets.bottom;
  const lifted = useAnimatedStyle(() => ({ transform: [{ translateY: -Math.max(0, -keyboard.height.get() - rest) }] }));

  // Room behind the keyboard so the last answer can scroll clear of it. Set once the keyboard
  // has settled: it sits under the keys, so it has nothing to animate.
  const [keyboardSpace, setKeyboardSpace] = useState(0);
  const settled = (height: number) => {
    setKeyboardSpace(Math.max(0, height - rest));
    if (height > 0) requestAnimationFrame(() => scroll.current?.scrollToEnd({ animated: true }));
  };
  useKeyboardHandler(
    {
      onEnd: (e) => {
        'worklet';
        scheduleOnRN(settled, e.height);
      },
    },
    [rest],
  );

  const page = useColorScheme() === 'dark' ? '0, 0, 0' : '255, 255, 255';
  const canSend = question.trim() !== '' && !running;
  const empty = turns.length === 0;

  return (
    <>
      <HeaderActions
        items={
          !empty && !running
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
        keyboardShouldPersistTaps="handled"
        showsVerticalScrollIndicator={false}
        onContentSizeChange={() => {
          if (running) scroll.current?.scrollToEnd({ animated: false });
        }}
        contentContainerStyle={{ paddingHorizontal: PAGE_PAD_X, paddingTop: 8, paddingBottom: FOOTER_CLEARANCE, gap: 24 }}
      >
        {empty ? (
          <Hero model={settings.model} hasKey={hasKey} />
        ) : (
          turns.map((t) => <Exchange key={t.id} turn={t} onRetry={() => void run(t.question)} />)
        )}
        <View style={{ height: keyboardSpace }} />
      </ScrollView>
      <Animated.View
        // In a tab the bottom inset already ends at the tab bar, so the composer rests one gap
        // above the bar and, with the keyboard up, one gap above the keys.
        style={[{ position: 'absolute', left: 0, right: 0, bottom: rest, paddingBottom: COMPOSER_GAP, gap: 8 }, lifted]}
      >
        <View
          pointerEvents="none"
          style={{
            position: 'absolute',
            left: 0,
            right: 0,
            top: -FADE_OVERHANG,
            bottom: 0,
            experimental_backgroundImage: `linear-gradient(to bottom, rgba(${page}, 0) 0%, rgba(${page}, 0.8) 45%, rgba(${page}, 1) 100%)`,
          }}
        />
        {empty && hasKey ? (
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            keyboardShouldPersistTaps="handled"
            contentContainerStyle={{ paddingHorizontal: PAGE_PAD_X, gap: 8 }}
          >
            {SUGGESTIONS.map((s) => (
              <Pressable
                key={s}
                accessibilityRole="button"
                onPress={() => {
                  haptic('light');
                  void run(s);
                }}
                style={({ pressed }) => ({
                  height: 36,
                  paddingHorizontal: 14,
                  borderRadius: 18,
                  justifyContent: 'center',
                  backgroundColor: colors.fill,
                  opacity: pressed ? 0.6 : 1,
                })}
              >
                <Text style={{ fontSize: 14, color: colors.text }}>{s}</Text>
              </Pressable>
            ))}
          </ScrollView>
        ) : null}
        <View style={{ paddingHorizontal: PAGE_PAD_X }}>
          <GlassSurface interactive style={{ flexDirection: 'row', minHeight: COMPOSER, borderRadius: COMPOSER / 2, borderCurve: 'continuous' }}>
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
              style={{ flex: 1, maxHeight: 120, paddingLeft: 16, paddingRight: 8, paddingTop: 12, paddingBottom: 12, fontSize: 16, color: colors.text }}
            />
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={running ? 'Stop' : 'Ask'}
              disabled={!canSend && !running}
              hitSlop={{ top: 8, right: 8, bottom: 8, left: 0 }}
              onPress={running ? () => abort.current?.abort() : send}
              style={({ pressed }) => ({
                width: SUBMIT,
                height: SUBMIT,
                margin: (COMPOSER - SUBMIT) / 2,
                borderRadius: SUBMIT / 2,
                alignSelf: 'flex-end',
                alignItems: 'center',
                justifyContent: 'center',
                backgroundColor: canSend || running ? colors.accent : colors.fill,
                transform: [{ scale: pressed ? 0.94 : 1 }],
              })}
            >
              <Icon
                sf={running ? 'stop.fill' : 'arrow.up'}
                md={running ? 'stop' : 'arrow_upward'}
                size={running ? 12 : 15}
                color={canSend || running ? 'white' : colors.tertiary}
              />
            </Pressable>
          </GlassSurface>
        </View>
      </Animated.View>
    </>
  );
}
