import * as Haptics from 'expo-haptics';

export type HapticStyle = 'selection' | 'light' | 'medium' | 'success' | 'warning';

/** Every tap's haptic goes through here: one per user action, fired with the visual change. */
export function haptic(style: HapticStyle = 'light'): void {
  if (process.env.EXPO_OS === 'android') {
    const kind =
      style === 'selection' || style === 'light'
        ? Haptics.AndroidHaptics.Clock_Tick
        : style === 'warning'
          ? Haptics.AndroidHaptics.Reject
          : Haptics.AndroidHaptics.Confirm;
    Haptics.performAndroidHapticsAsync(kind).catch(() => {});
    return;
  }
  if (style === 'selection') Haptics.selectionAsync().catch(() => {});
  else if (style === 'success') Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success).catch(() => {});
  else if (style === 'warning') Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning).catch(() => {});
  else
    Haptics.impactAsync(style === 'medium' ? Haptics.ImpactFeedbackStyle.Medium : Haptics.ImpactFeedbackStyle.Light).catch(
      () => {},
    );
}
