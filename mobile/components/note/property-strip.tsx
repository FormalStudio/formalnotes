import { Alert, Pressable, Text, View } from 'react-native';

import { Icon } from '@/components/icon';
import { haptic } from '@/lib/haptics';
import { askName } from '@/lib/app/prompt';
import type { Column } from '@/lib/library/folder-view';
import { formatValue, inferType, parseValue, readProperties, type PropertyType, type PropertyValue } from '@/lib/library/properties';
import { colors } from '@/lib/ui/colors';

type Row = { name: string; type: PropertyType; value: PropertyValue | undefined; options: string[] };

/** The folder's columns first, then the other properties the note has. */
function rows(props: string, columns: Column[]): Row[] {
  const have = readProperties(props);
  const out: Row[] = columns.map((c) => ({
    name: c.name,
    type: c.type,
    value: have.find((p) => p.name === c.name)?.value,
    options: c.options ?? [],
  }));
  for (const p of have) {
    if (!columns.some((c) => c.name === p.name)) out.push({ name: p.name, type: inferType(p.value), value: p.value, options: [] });
  }
  return out;
}

function shown(row: Row): string {
  if (row.value === undefined || row.value === '') return 'Empty';
  if (Array.isArray(row.value)) return row.value.join(', ');
  return String(row.value);
}

function choose(title: string, options: string[]): Promise<string | null> {
  return new Promise((resolve) => {
    Alert.alert(title, undefined, [
      ...options.map((o) => ({ text: o, onPress: () => resolve(o) })),
      { text: 'Clear', style: 'destructive' as const, onPress: () => resolve('') },
      { text: 'Cancel', style: 'cancel' as const, onPress: () => resolve(null) },
    ]);
  });
}

type Props = {
  /** The note's properties block, fences included, or empty. */
  props: string;
  columns: Column[];
  /** Sets one property to its written form; empty removes the line. */
  onSet: (name: string, formatted: string) => void;
};

/** The note's properties above its body, each edited by its type, and a way to add one. */
export function PropertyStrip({ props, columns, onSet }: Props) {
  const list = rows(props, columns);

  const edit = async (row: Row) => {
    haptic('selection');
    if (row.type === 'checkbox') {
      onSet(row.name, formatValue('checkbox', row.value !== true));
      return;
    }
    if (row.type === 'select' && row.options.length > 0) {
      const v = await choose(row.name, row.options);
      if (v !== null) onSet(row.name, formatValue('select', v));
      return;
    }
    const initial = row.value === undefined ? '' : Array.isArray(row.value) ? row.value.join(', ') : String(row.value);
    const v = await askName({ title: row.name, initial, confirm: 'Set', allowEmpty: true });
    if (v === null) return;
    const value = row.type === 'number' ? Number(v) : row.type === 'text' || row.type === 'url' ? v : parseValue(v);
    onSet(row.name, formatValue(row.type, row.type === 'number' && Number.isNaN(value) ? v : value));
  };

  const add = async () => {
    const name = await askName({ title: 'New property', initial: '', confirm: 'Next' });
    if (!name) return;
    const v = await askName({ title: name, initial: '', confirm: 'Add' });
    if (!v) return;
    const value = parseValue(v);
    onSet(name, formatValue(inferType(value), value));
  };

  return (
    <View style={{ gap: 2 }}>
      {list.map((row) => (
        <Pressable
          key={row.name}
          accessibilityRole="button"
          accessibilityLabel={`${row.name}, ${shown(row)}`}
          onPress={() => void edit(row)}
          style={({ pressed }) => ({
            flexDirection: 'row',
            alignItems: 'center',
            minHeight: 32,
            borderRadius: 8,
            borderCurve: 'continuous',
            backgroundColor: pressed ? colors.fill : 'transparent',
          })}
        >
          <Text numberOfLines={1} style={{ width: 110, fontSize: 15, color: colors.secondary }}>
            {row.name}
          </Text>
          {row.type === 'checkbox' ? (
            <Icon
              sf={row.value === true ? 'checkmark.square.fill' : 'square'}
              md={row.value === true ? 'check_box' : 'check_box_outline_blank'}
              size={18}
              color={row.value === true ? colors.accent : colors.tertiary}
            />
          ) : (
            <Text
              numberOfLines={1}
              style={{ flex: 1, fontSize: 15, color: row.value === undefined ? colors.tertiary : colors.text, fontVariant: ['tabular-nums'] }}
            >
              {shown(row)}
            </Text>
          )}
        </Pressable>
      ))}
      <Pressable
        accessibilityRole="button"
        onPress={() => void add()}
        hitSlop={6}
        style={({ pressed }) => ({ flexDirection: 'row', alignItems: 'center', gap: 6, minHeight: 32, opacity: pressed ? 0.5 : 1 })}
      >
        <Icon sf="plus" md="add" size={14} color={colors.tertiary} />
        <Text style={{ fontSize: 15, color: colors.tertiary }}>Add property</Text>
      </Pressable>
    </View>
  );
}
