import { Link } from 'expo-router';
import { Pressable, Text, View } from 'react-native';

import { Icon } from '@/components/icon';
import { bodyOf, nameOf, parentOf, split } from '@/lib/library/model';
import type { Note } from '@/lib/library/store';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

const ICON_TILE = 36;
const ROW_GAP = 12;

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
      <Pressable accessibilityRole="button">
        {/* The row's layout sits on this view: a link replaces its child's own style. */}
        {({ pressed }) => (
          <View
            style={{
              flexDirection: 'row',
              alignItems: 'center',
              gap: ROW_GAP,
              paddingHorizontal: PAGE_PAD_X,
              paddingVertical: 10,
              backgroundColor: pressed ? colors.fill : 'transparent',
            }}
          >
            <View
              style={{
                width: ICON_TILE,
                height: ICON_TILE,
                borderRadius: 10,
                borderCurve: 'continuous',
                alignItems: 'center',
                justifyContent: 'center',
                backgroundColor: colors.fill,
              }}
            >
              <Icon sf="doc.text" md="description" size={18} color={colors.secondary} />
            </View>
            <View style={{ flex: 1, gap: 2 }}>
              <Text numberOfLines={1} style={{ fontSize: 17, fontWeight: '600', color: colors.text }}>
                {note.title}
              </Text>
              {line ? (
                <Text numberOfLines={1} style={{ fontSize: 15, color: colors.secondary }}>
                  {line}
                </Text>
              ) : null}
            </View>
            <Text numberOfLines={1} style={{ maxWidth: 110, fontSize: 13, color: colors.tertiary }}>
              {detail ?? (folder === '' ? 'Notes' : nameOf(folder))}
            </Text>
          </View>
        )}
      </Pressable>
    </Link>
  );
}

export function RowSeparator() {
  return <View style={{ height: 0.5, marginLeft: PAGE_PAD_X + ICON_TILE + ROW_GAP, backgroundColor: colors.border }} />;
}
