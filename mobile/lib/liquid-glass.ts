import { isLiquidGlassAvailable } from 'expo-glass-effect';

let cached: boolean | undefined;

/** Whether the OS draws Liquid Glass (iOS 26 and later). Every fallback branch asks this. */
export function isLiquidGlass(): boolean {
  if (cached === undefined) cached = process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable();
  return cached;
}
