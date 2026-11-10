import type { AndroidSymbol, SFSymbol } from 'expo-symbols';
import { Pressable, View } from 'react-native';
import { useReanimatedKeyboardAnimation } from 'react-native-keyboard-controller';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { GlassSurface } from '@/components/glass-surface';
import { Icon } from '@/components/icon';
import { haptic } from '@/lib/haptics';
import type { LineKind } from '@/lib/library/editing';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

export const FORMAT_BAR = 44;
const GAP = 8;
/** What the bar takes above the keys, for the scroll view to keep the caret clear of it. */
export const FORMAT_BAR_SPACE = FORMAT_BAR + GAP * 2;

type Props = { onKind: (kind: LineKind) => void; onBold: () => void; onDone: () => void };

function Key({ sf, md, label, onPress }: { sf: SFSymbol; md: AndroidSymbol; label: string; onPress: () => void }) {
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={label}
      onPress={() => {
        haptic('selection');
        onPress();
      }}
      style={({ pressed }) => ({ width: 48, height: FORMAT_BAR, alignItems: 'center', justifyContent: 'center', opacity: pressed ? 0.5 : 1 })}
    >
      <Icon sf={sf} md={md} size={20} color={colors.text} />
    </Pressable>
  );
}

/** The keys that write Markdown for you, riding just above the keyboard while the body is being typed in. */
export function FormatBar({ onKind, onBold, onDone }: Props) {
  const keyboard = useReanimatedKeyboardAnimation();
  const rest = useSafeAreaInsets().bottom;
  const lifted = useAnimatedStyle(() => ({ transform: [{ translateY: -Math.max(0, -keyboard.height.get() - rest) }] }));
  const pill = { flexDirection: 'row', height: FORMAT_BAR, borderRadius: FORMAT_BAR / 2, borderCurve: 'continuous' } as const;
  return (
    <Animated.View
      style={[
        { position: 'absolute', left: 0, right: 0, bottom: rest, paddingBottom: GAP, paddingHorizontal: PAGE_PAD_X },
        { flexDirection: 'row', justifyContent: 'space-between' },
        lifted,
      ]}
    >
      <GlassSurface interactive style={[pill, { paddingHorizontal: 4 }]}>
        <Key sf="checklist" md="checklist" label="Checklist" onPress={() => onKind('task')} />
        <Key sf="list.bullet" md="format_list_bulleted" label="List" onPress={() => onKind('bullet')} />
        <Key sf="textformat.size" md="format_size" label="Heading" onPress={() => onKind('heading')} />
        <Key sf="bold" md="format_bold" label="Bold" onPress={onBold} />
      </GlassSurface>
      <View>
        <GlassSurface interactive style={pill}>
          <Key sf="keyboard.chevron.compact.down" md="keyboard_hide" label="Hide keyboard" onPress={onDone} />
        </GlassSurface>
      </View>
    </Animated.View>
  );
}
