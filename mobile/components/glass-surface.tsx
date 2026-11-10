import { BlurView } from 'expo-blur';
import { GlassView } from 'expo-glass-effect';
import type { ReactNode } from 'react';
import { StyleSheet, type StyleProp, type ViewStyle } from 'react-native';

import { isLiquidGlass } from '@/lib/liquid-glass';
import { colors } from '@/lib/ui/colors';

type Props = { children?: ReactNode; style?: StyleProp<ViewStyle>; interactive?: boolean };

/** Liquid Glass on iOS 26 and later, the system chrome material before it. */
export function GlassSurface({ children, style, interactive }: Props) {
  if (isLiquidGlass()) {
    return (
      <GlassView glassEffectStyle="regular" isInteractive={interactive} style={style}>
        {children}
      </GlassView>
    );
  }
  return (
    <BlurView
      tint="systemChromeMaterial"
      intensity={100}
      style={[{ overflow: 'hidden', borderWidth: StyleSheet.hairlineWidth, borderColor: colors.border }, style]}
    >
      {children}
    </BlurView>
  );
}
