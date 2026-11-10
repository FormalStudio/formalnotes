import type { ReactNode } from 'react';
import { StyleSheet, View, type StyleProp, type ViewStyle } from 'react-native';

import { colors } from '@/lib/ui/colors';

type Props = { children?: ReactNode; style?: StyleProp<ViewStyle>; interactive?: boolean };

/** Material separates floating surfaces by tone and shadow rather than blur. */
export function GlassSurface({ children, style }: Props) {
  return (
    <View
      style={[
        {
          overflow: 'hidden',
          backgroundColor: colors.paper,
          borderWidth: StyleSheet.hairlineWidth,
          borderColor: colors.border,
          boxShadow: '0 2px 6px rgba(0, 0, 0, 0.18)',
        },
        style,
      ]}
    >
      {children}
    </View>
  );
}
