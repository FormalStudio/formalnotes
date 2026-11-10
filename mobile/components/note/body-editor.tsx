import { useState, type Ref } from 'react';
import { Pressable, Text, TextInput, View, type TextInputInstance } from 'react-native';

import { Icon } from '@/components/icon';
import { markdownSpans } from '@/components/note/markdown';
import { haptic } from '@/lib/haptics';
import { lineTops, tasks, type Selection } from '@/lib/library/editing';
import { colors } from '@/lib/ui/colors';
import { BODY_LINE, BODY_TEXT, PAGE_PAD_X } from '@/lib/ui/metrics';

const TEXT = { fontSize: BODY_TEXT, lineHeight: BODY_LINE, color: colors.text, padding: 0 } as const;
const BOX = 22;
/** Text sits below the middle of its line box, by the leading the line height adds above it. */
const NUDGE = 3;
/** A space in the body font, for a nested task's indent. */
const SPACE = BODY_TEXT * 0.26;

type Props = {
  ref: Ref<TextInputInstance>;
  body: string;
  autoFocus: boolean;
  onChangeText: (body: string) => void;
  onSelection: (selection: Selection) => void;
  onToggle: (line: number) => void;
  onFocus: () => void;
  onBlur: () => void;
};

/** The note's body: one text field, with a checkbox to tap drawn over each task's marker. */
export function BodyEditor({ ref, body, autoFocus, onChangeText, onSelection, onToggle, onFocus, onBlur }: Props) {
  const [tops, setTops] = useState<{ y: number; height: number }[]>([]);
  const spans = markdownSpans(body);
  return (
    <View>
      <TextInput
        ref={ref}
        accessibilityLabel="Body"
        onChangeText={onChangeText}
        onSelectionChange={(e) => onSelection(e.nativeEvent.selection)}
        onFocus={onFocus}
        onBlur={onBlur}
        placeholder="Start writing"
        placeholderTextColor={colors.tertiary}
        autoFocus={autoFocus}
        multiline
        scrollEnabled={false}
        textAlignVertical="top"
        style={[TEXT, { minHeight: 240 }]}
      >
        {spans}
      </TextInput>
      {/* A field does not report where its lines sit, so the same text is laid out unseen to find each task's line. */}
      <View pointerEvents="none" aria-hidden style={{ position: 'absolute', top: 0, left: 0, right: 0, opacity: 0 }}>
        <Text style={TEXT} onTextLayout={(e) => setTops(lineTops(e.nativeEvent.lines))}>
          {spans}
        </Text>
      </View>
      {tasks(body).map((t) => {
        const at = tops[t.line];
        if (!at) return null;
        return (
          <Pressable
            key={t.line}
            accessibilityRole="checkbox"
            accessibilityState={{ checked: t.done }}
            accessibilityLabel={t.text}
            hitSlop={{ top: 3, bottom: 3, left: PAGE_PAD_X, right: 6 }}
            onPress={() => {
              haptic('light');
              onToggle(t.line);
            }}
            style={({ pressed }) => ({
              position: 'absolute',
              top: at.y + (at.height - BOX) / 2 + NUDGE,
              left: t.indent * SPACE,
              width: BOX,
              height: BOX,
              borderRadius: BOX / 2,
              borderWidth: t.done ? 0 : 1.5,
              borderColor: colors.tertiary,
              backgroundColor: t.done ? colors.accent : 'transparent',
              alignItems: 'center',
              justifyContent: 'center',
              transform: [{ scale: pressed ? 0.9 : 1 }],
            })}
          >
            {t.done ? <Icon sf="checkmark" md="check" size={11} color="white" /> : null}
          </Pressable>
        );
      })}
    </View>
  );
}
