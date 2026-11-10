import { router, Stack, useLocalSearchParams } from 'expo-router';
import { useEffect, useEffectEvent, useRef, useState } from 'react';
import { AppState, Share, Text, TextInput, View, type TextInputInstance } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';

import { HeaderActions } from '@/components/header-actions';
import { markdownSpans } from '@/components/note/markdown';
import { PropertyStrip } from '@/components/note/property-strip';
import { trash, type Focus } from '@/lib/app/actions';
import { useNow } from '@/lib/app/clock';
import { library, useLibrary } from '@/lib/app/library';
import { editorParts, joinParts, nameOf, parentOf, relative } from '@/lib/library/model';
import { setProperty } from '@/lib/library/properties';
import type { Note } from '@/lib/library/store';
import { colors } from '@/lib/ui/colors';
import { BODY_LINE, BODY_TEXT, EDITOR_TITLE } from '@/lib/ui/metrics';

/** Typing rests this long before a save, and never goes longer than MAX_WAIT unsaved. */
const REST = 500;
const MAX_WAIT = 2000;

export default function NoteRoute() {
  const { id, focus } = useLocalSearchParams<{ id: string; focus?: Focus }>();
  const note = useLibrary((lib) => lib.note(id));
  if (!note) {
    return (
      <>
        <Stack.Screen options={{ title: '' }} />
        <Text style={{ paddingTop: 160, textAlign: 'center', fontSize: 15, color: colors.secondary }}>This note is gone</Text>
      </>
    );
  }
  return <Editor key={note.id} note={note} focus={focus} />;
}

function Editor({ note, focus }: { note: Note; focus?: Focus }) {
  const now = useNow();
  const folder = parentOf(note.path);
  const columns = useLibrary((lib) => lib.folderView(folder).columns);
  const first = editorParts(note.text);
  const [seen, setSeen] = useState(note.text);
  const [props, setProps] = useState(first.props);
  const [title, setTitle] = useState(first.heading ?? note.title);
  // A note whose file has no heading keeps it that way until its title is edited.
  const [heading, setHeading] = useState(first.heading !== null);
  const [body, setBody] = useState(first.body);

  // The text changed outside this editor (a sync, a rename): show what is there now.
  if (note.text !== seen) {
    const p = editorParts(note.text);
    setSeen(note.text);
    setProps(p.props);
    setTitle(p.heading ?? note.title);
    setHeading(p.heading !== null);
    setBody(p.body);
  }

  const bodyRef = useRef<TextInputInstance>(null);
  const pending = useRef<string | null>(null);
  const rest = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cap = useRef<ReturnType<typeof setTimeout> | null>(null);
  const titleEdited = useRef(false);

  const flush = () => {
    if (rest.current) clearTimeout(rest.current);
    if (cap.current) clearTimeout(cap.current);
    rest.current = cap.current = null;
    const text = pending.current;
    pending.current = null;
    if (text === null) return;
    setSeen(text);
    library.writeText(note.id, text);
  };

  const queue = (text: string) => {
    pending.current = text;
    if (rest.current) clearTimeout(rest.current);
    rest.current = setTimeout(flush, REST);
    if (!cap.current) cap.current = setTimeout(flush, MAX_WAIT);
  };

  /** Saves, then gives the file its title's name. */
  const commitTitle = () => {
    flush();
    const t = title.trim();
    if (titleEdited.current && t !== '') library.retitle(note.id, t);
    titleEdited.current = false;
  };

  const leave = useEffectEvent(commitTitle);

  useEffect(() => {
    const sub = AppState.addEventListener('change', (s) => {
      if (s !== 'active') leave();
    });
    return () => {
      sub.remove();
      leave();
      // A note left without a word in it goes to the trash, as the desktop does.
      const left = library.note(note.id);
      if (left && left.text.trim() === '') library.trash(note.id);
    };
  }, [note.id]);

  const onTitle = (t: string) => {
    titleEdited.current = true;
    setTitle(t);
    setHeading(true);
    queue(joinParts(props, t.trim() === '' ? null : t, body));
  };

  const onBody = (b: string) => {
    setBody(b);
    queue(joinParts(props, heading ? title : null, b));
  };

  const onProperty = (name: string, formatted: string) => {
    const next = setProperty(props, name, formatted);
    setProps(next);
    pending.current = joinParts(next, heading ? title : null, body);
    flush();
  };

  const caption = [relative(note.modified, now), folder === '' ? 'Notes' : nameOf(folder)].join(' · ');

  return (
    <>
      <Stack.Screen options={{ title: '' }} />
      <HeaderActions
        items={[
          {
            kind: 'menu',
            sf: 'ellipsis',
            md: 'more_vert',
            label: 'More',
            groups: [
              [
                {
                  title: 'Move to',
                  sf: 'folder',
                  md: 'drive_file_move',
                  onPress: () => {
                    commitTitle();
                    router.push({ pathname: '/move', params: { id: note.id } });
                  },
                },
                {
                  title: 'Share',
                  sf: 'square.and.arrow.up',
                  md: 'share',
                  onPress: () => {
                    flush();
                    void Share.share({ message: library.note(note.id)?.text ?? '' });
                  },
                },
              ],
              [
                {
                  title: 'Delete',
                  sf: 'trash',
                  md: 'delete',
                  destructive: true,
                  onPress: () => {
                    flush();
                    trash(note.id);
                    router.back();
                  },
                },
              ],
            ],
          },
        ]}
      />
      <KeyboardAwareScrollView
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="interactive"
        bottomOffset={24}
        contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 4, paddingBottom: 120, gap: 14 }}
      >
        <View style={{ gap: 4 }}>
          <TextInput
            accessibilityLabel="Title"
            value={title}
            onChangeText={onTitle}
            onBlur={commitTitle}
            placeholder="Title"
            placeholderTextColor={colors.tertiary}
            autoFocus={focus === 'title'}
            multiline
            scrollEnabled={false}
            submitBehavior="submit"
            returnKeyType="next"
            onSubmitEditing={() => bodyRef.current?.focus()}
            style={{ fontSize: EDITOR_TITLE, lineHeight: EDITOR_TITLE * 1.2, fontWeight: '700', color: colors.text, padding: 0 }}
          />
          <Text style={{ fontSize: 13, color: colors.secondary }}>{caption}</Text>
        </View>
        <PropertyStrip props={props} columns={columns} onSet={onProperty} />
        <TextInput
          ref={bodyRef}
          accessibilityLabel="Body"
          onChangeText={onBody}
          placeholder="Start writing"
          placeholderTextColor={colors.tertiary}
          autoFocus={focus === 'body'}
          multiline
          scrollEnabled={false}
          textAlignVertical="top"
          style={{ fontSize: BODY_TEXT, lineHeight: BODY_LINE, color: colors.text, padding: 0, minHeight: 240 }}
        >
          {markdownSpans(body)}
        </TextInput>
      </KeyboardAwareScrollView>
    </>
  );
}
