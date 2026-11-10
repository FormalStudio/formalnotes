import { Link } from 'expo-router';
import { Pressable, Text, View } from 'react-native';

import { Icon } from '@/components/icon';
import { bodyOf, nameOf, parentOf, split } from '@/lib/library/model';
import type { Note } from '@/lib/library/store';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

/** The first line of the body that holds `query`'s first word, or the body's first line. */
function snippet(note: Note, query: string): string {
  const lines = split(bodyOf(note.text))
    .body.split('\n')
    .map((l) => l.replace(/^\s*(?:[-*] \[[ xX]\] |[-*>] |#+ )/, '').trim())
    .filter((l) => l !== '' && !l.startsWith('```'));
  const word = query.trim().toLowerCase().split(/\s+/)[0] ?? '';
  return (word ? lines.find((l) => l.toLowerCase().includes(word)) : undefined) ?? lines[0] ?? '';
}

/** A note as a list row: title, the line that matched, and its folder. */
export function NoteRow({ note, query = '', detail }: { note: Note; query?: string; detail?: string }) {
  const folder = parentOf(note.path);
  const line = snippet(note, query);
  return (
    <Link href={{ pathname: '/note/[id]', params: { id: note.id } }} asChild>
      <Pressable
        accessibilityRole="button"
        style={({ pressed }) => ({
          flexDirection: 'row',
          gap: 12,
          paddingHorizontal: PAGE_PAD_X,
          paddingVertical: 10,
          backgroundColor: pressed ? colors.fill : 'transparent',
        })}
      >
        <Icon sf="doc.text" md="description" size={20} color={colors.secondary} style={{ marginTop: 2 }} />
        <View style={{ flex: 1, gap: 2 }}>
          <Text numberOfLines={1} style={{ fontSize: 17, fontWeight: '600', color: colors.text }}>
            {note.title}
          </Text>
          {line ? (
            <Text numberOfLines={2} style={{ fontSize: 15, color: colors.secondary }}>
              {line}
            </Text>
          ) : null}
          <Text numberOfLines={1} style={{ fontSize: 13, color: colors.tertiary }}>
            {[folder === '' ? 'Notes' : nameOf(folder), detail].filter(Boolean).join(' · ')}
          </Text>
        </View>
      </Pressable>
    </Link>
  );
}

export function RowSeparator() {
  return <View style={{ height: 0.5, marginLeft: PAGE_PAD_X + 32, backgroundColor: colors.border }} />;
}
