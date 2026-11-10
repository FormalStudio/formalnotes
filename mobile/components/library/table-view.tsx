import { Link } from 'expo-router';
import type { ReactElement } from 'react';
import { FlatList, Pressable, Text, View, type RefreshControlProps } from 'react-native';

import { Icon } from '@/components/icon';
import { useNow } from '@/lib/app/clock';
import { useLibrary } from '@/lib/app/library';
import type { Column, FolderView } from '@/lib/library/folder-view';
import { relative } from '@/lib/library/model';
import { readProperties, type PropertyValue } from '@/lib/library/properties';
import type { Note } from '@/lib/library/store';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

type Row = { note: Note; values: Map<string, PropertyValue> };

function compare(a: Row, b: Row, column: string): number {
  if (column === 'title') return a.note.title.localeCompare(b.note.title);
  if (column === 'created' || column === 'modified') return a.note[column] - b.note[column];
  const x = a.values.get(column);
  const y = b.values.get(column);
  if (x === undefined || y === undefined) return x === undefined ? (y === undefined ? 0 : 1) : -1;
  if (typeof x === 'number' && typeof y === 'number') return x - y;
  return String(x).localeCompare(String(y));
}

function Chip({ column, value }: { column: Column; value: PropertyValue }) {
  if (column.type === 'checkbox') {
    return (
      <Icon
        sf={value === true ? 'checkmark.square.fill' : 'square'}
        md={value === true ? 'check_box' : 'check_box_outline_blank'}
        size={16}
        color={value === true ? colors.accent : colors.tertiary}
      />
    );
  }
  const values = Array.isArray(value) ? value : [String(value)];
  const tinted = column.type === 'select' || column.type === 'multi_select';
  return values.map((v) => (
    <View
      key={v}
      style={{
        paddingHorizontal: 8,
        paddingVertical: 2,
        borderRadius: 8,
        borderCurve: 'continuous',
        backgroundColor: tinted ? colors.fill : 'transparent',
      }}
    >
      <Text numberOfLines={1} style={{ fontSize: 13, color: tinted ? colors.text : colors.secondary, fontVariant: ['tabular-nums'] }}>
        {v}
      </Text>
    </View>
  ));
}

/** A table folder: one row per note, its title over the chips of the view's columns. */
export function TableView({
  folder,
  view,
  refreshControl,
}: {
  folder: string;
  view: FolderView;
  refreshControl?: ReactElement<RefreshControlProps>;
}) {
  const now = useNow();
  const notes = useLibrary((lib) => lib.notesIn(folder));
  const rows: Row[] = notes.map((note) => ({
    note,
    values: new Map(readProperties(note.text).map((p) => [p.name, p.value])),
  }));
  const sort = view.sort ?? { column: 'modified', descending: true };
  rows.sort((a, b) => (sort.descending ? -1 : 1) * compare(a, b, sort.column));

  return (
    <FlatList
      data={rows}
      keyExtractor={(r) => r.note.id}
      contentInsetAdjustmentBehavior="automatic"
      refreshControl={refreshControl}
      contentContainerStyle={{ paddingBottom: 32 }}
      ItemSeparatorComponent={() => <View style={{ height: 0.5, marginLeft: PAGE_PAD_X, backgroundColor: colors.border }} />}
      ListEmptyComponent={
        <Text style={{ paddingTop: 96, textAlign: 'center', fontSize: 15, color: colors.secondary }}>No notes in this table yet</Text>
      }
      renderItem={({ item }) => (
        <Link href={{ pathname: '/note/[id]', params: { id: item.note.id } }} asChild>
          <Pressable
            accessibilityRole="button"
            style={({ pressed }) => ({
              paddingHorizontal: PAGE_PAD_X,
              paddingVertical: 12,
              gap: 6,
              backgroundColor: pressed ? colors.fill : 'transparent',
            })}
          >
            <View style={{ flexDirection: 'row', alignItems: 'baseline', gap: 8 }}>
              <Text numberOfLines={1} style={{ flex: 1, fontSize: 17, fontWeight: '600', color: colors.text }}>
                {item.note.title}
              </Text>
              <Text style={{ fontSize: 13, color: colors.secondary }}>{relative(item.note.modified, now)}</Text>
            </View>
            {view.columns.some((c) => item.values.has(c.name)) ? (
              <View style={{ flexDirection: 'row', flexWrap: 'wrap', alignItems: 'center', gap: 6 }}>
                {view.columns.map((c) => {
                  const v = item.values.get(c.name);
                  return v === undefined || v === '' ? null : <Chip key={c.name} column={c} value={v} />;
                })}
              </View>
            ) : null}
          </Pressable>
        </Link>
      )}
    />
  );
}
