import { Text, View } from 'react-native';

import { Icon } from '@/components/icon';
import { lineKeys, preview, type PreviewLine } from '@/lib/library/model';
import { colors } from '@/lib/ui/colors';
import { MONO, PREVIEW_HEADING, PREVIEW_LINE, PREVIEW_LINES, PREVIEW_TEXT, TITLE_LINE, TITLE_TEXT } from '@/lib/ui/metrics';

/** A note's miniature: its title and first lines, as a card's page shows it. */
export function NotePage({ title, text, padding }: { title: string; text: string; padding: number }) {
  const lines = preview(text, PREVIEW_LINES);
  return (
    <View style={{ flex: 1, padding, gap: 2 }}>
      <Text
        numberOfLines={2}
        style={{ fontSize: TITLE_TEXT, lineHeight: TITLE_LINE, fontWeight: '700', color: colors.text, marginBottom: 6 }}
      >
        {title}
      </Text>
      {lines.length === 0 ? (
        <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center' }}>
          <Text style={{ fontSize: 12, color: colors.secondary }}>Blank note</Text>
        </View>
      ) : (
        lineKeys(lines).map((key, i) => <Line key={key} line={lines[i]} />)
      )}
    </View>
  );
}

export function Line({ line }: { line: PreviewLine }) {
  const small = { fontSize: PREVIEW_TEXT, lineHeight: PREVIEW_LINE, color: colors.text } as const;
  switch (line.kind) {
    case 'heading':
      return (
        <Text numberOfLines={1} style={{ fontSize: PREVIEW_HEADING, lineHeight: PREVIEW_LINE + 2, fontWeight: '600', color: colors.text }}>
          {line.text}
        </Text>
      );
    case 'bullet':
      return (
        <Text numberOfLines={1} style={small}>
          {'•  '}
          {line.text}
        </Text>
      );
    case 'task':
      return (
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 4, height: PREVIEW_LINE }}>
          <Icon
            sf={line.done ? 'checkmark.square.fill' : 'square'}
            md={line.done ? 'check_box' : 'check_box_outline_blank'}
            size={PREVIEW_TEXT + 1}
            color={line.done ? colors.secondary : colors.text}
          />
          <Text numberOfLines={1} style={[small, { flex: 1, color: line.done ? colors.secondary : colors.text }]}>
            {line.text}
          </Text>
        </View>
      );
    case 'quote':
      return (
        <View style={{ flexDirection: 'row', gap: 4, height: PREVIEW_LINE }}>
          <View style={{ width: 1.5, backgroundColor: colors.tertiary }} />
          <Text numberOfLines={1} style={[small, { flex: 1, color: colors.secondary }]}>
            {line.text}
          </Text>
        </View>
      );
    case 'code':
      return (
        <Text numberOfLines={1} style={[small, { color: colors.secondary, fontFamily: MONO }]}>
          {line.text}
        </Text>
      );
    default:
      return (
        <Text numberOfLines={1} style={small}>
          {line.text}
        </Text>
      );
  }
}
