// Semantic colours: UIKit's on iOS, Material 3 dynamic colour on Android. They follow light and
// dark on their own. The desktop roles they stand for are in the names.

import { Color } from 'expo-router';
import { Platform, type ColorValue } from 'react-native';

const pick = (ios: ColorValue, android: ColorValue, web: string): ColorValue =>
  Platform.select({ ios, android, default: web }) as ColorValue;

export const colors = {
  page: pick(Color.ios.systemBackground, Color.android.dynamic.surface, '#ffffff'),
  /** A note card's page and a folder's papers. */
  raised: pick(Color.ios.secondarySystemGroupedBackground, Color.android.dynamic.surfaceContainerLowest, '#ffffff'),
  /** A folder's back plate and tab. */
  plate: pick(Color.ios.systemGray6, Color.android.dynamic.surfaceContainer, '#f2f2f7'),
  paper: pick(Color.ios.tertiarySystemBackground, Color.android.dynamic.surfaceContainerHigh, '#ffffff'),
  /** The ring a note card's page sits in. */
  frame: pick(Color.ios.systemGray5, Color.android.dynamic.surfaceContainerHigh, '#e5e5ea'),
  border: pick(Color.ios.separator, Color.android.dynamic.outlineVariant, '#c6c6c8'),
  fill: pick(Color.ios.tertiarySystemFill, Color.android.dynamic.surfaceContainerHighest, '#7676801f'),
  text: pick(Color.ios.label, Color.android.dynamic.onSurface, '#000000'),
  secondary: pick(Color.ios.secondaryLabel, Color.android.dynamic.onSurfaceVariant, '#3c3c4399'),
  tertiary: pick(Color.ios.tertiaryLabel, Color.android.dynamic.outline, '#3c3c434d'),
  accent: pick(Color.ios.systemBlue, Color.android.dynamic.primary, '#007aff'),
  danger: pick(Color.ios.systemRed, Color.android.dynamic.error, '#ff3b30'),
  grouped: pick(Color.ios.systemGroupedBackground, Color.android.dynamic.surfaceContainerLow, '#f2f2f7'),
};
