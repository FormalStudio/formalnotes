import { useEffect } from 'react';
import { Pressable, Text, View } from 'react-native';
import Animated, { useAnimatedStyle, useReducedMotion, useSharedValue, withSpring } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { GlassSurface } from '@/components/glass-surface';
import { dismissNotice, useNotice, type Notice } from '@/lib/app/notice';
import { colors } from '@/lib/ui/colors';
import { TAB_BAR_CLEARANCE } from '@/lib/ui/metrics';
import { LAYOUT } from '@/lib/ui/motion';

function Bar({ notice }: { notice: Notice }) {
  const reduced = useReducedMotion();
  const settle = useSharedValue(reduced ? 1 : 0);
  // Glass is drawn on its first layout pass and stays blank if it starts translucent, so the
  // notice arrives by moving and scaling, never by fading.
  const style = useAnimatedStyle(() => ({
    transform: [{ translateY: 8 * (1 - settle.get()) }, { scale: 0.96 + 0.04 * settle.get() }],
  }));
  useEffect(() => {
    if (!reduced) settle.set(withSpring(1, LAYOUT));
  }, [reduced, settle]);
  return (
    <Animated.View style={style}>
      <GlassSurface
        style={{
          flexDirection: 'row',
          alignItems: 'center',
          gap: 12,
          minHeight: 48,
          paddingLeft: 18,
          paddingRight: notice.action ? 6 : 18,
          borderRadius: 24,
          borderCurve: 'continuous',
        }}
      >
        <Text accessibilityLiveRegion="polite" numberOfLines={2} style={{ flex: 1, fontSize: 15, color: colors.text }}>
          {notice.message}
        </Text>
        {notice.action ? (
          <Pressable
            accessibilityRole="button"
            hitSlop={8}
            onPress={() => {
              notice.action?.run();
              dismissNotice();
            }}
            style={({ pressed }) => ({ paddingHorizontal: 14, paddingVertical: 10, opacity: pressed ? 0.5 : 1 })}
          >
            <Text style={{ fontSize: 15, fontWeight: '600', color: colors.accent }}>{notice.action.label}</Text>
          </Pressable>
        ) : null}
      </GlassSurface>
    </Animated.View>
  );
}

/** The notice with Undo, floating over the tab bar. */
export function Snackbar() {
  const notice = useNotice();
  const insets = useSafeAreaInsets();
  if (!notice) return null;
  return (
    <View pointerEvents="box-none" style={{ position: 'absolute', left: 12, right: 12, bottom: insets.bottom + TAB_BAR_CLEARANCE }}>
      <Bar key={notice.id} notice={notice} />
    </View>
  );
}
