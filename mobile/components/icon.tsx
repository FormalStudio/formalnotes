import { SymbolView, type AndroidSymbol, type SFSymbol } from 'expo-symbols';
import type { ColorValue, StyleProp, ViewStyle } from 'react-native';

type Props = {
  /** The SF Symbol on iOS. */
  sf: SFSymbol;
  /** The Material Symbol on Android. */
  md: AndroidSymbol;
  size?: number;
  color?: ColorValue;
  style?: StyleProp<ViewStyle>;
};

export function Icon({ sf, md, size = 20, color, style }: Props) {
  return <SymbolView name={{ ios: sf, android: md }} size={size} tintColor={color} style={[{ width: size, height: size }, style]} />;
}
