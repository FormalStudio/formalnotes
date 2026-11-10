import { Stack } from 'expo-router';

import { isLiquidGlass } from '@/lib/liquid-glass';
import { colors } from '@/lib/ui/colors';

export const unstable_settings = {
  library: { anchor: 'index' },
  search: { anchor: 'search' },
  ask: { anchor: 'ask' },
  settings: { anchor: 'settings' },
};

const ROOTS: Record<string, { name: string; title: string }> = {
  library: { name: 'index', title: 'Notes' },
  search: { name: 'search', title: 'Search' },
  ask: { name: 'ask', title: 'Ask' },
  settings: { name: 'settings', title: 'Settings' },
};

export default function SharedLayout({ segment }: { segment: string }) {
  const root = ROOTS[segment.match(/\((.*)\)/)?.[1] ?? 'library'] ?? ROOTS.library;
  return (
    <Stack
      screenOptions={{
        headerTransparent: true,
        headerShadowVisible: false,
        headerLargeTitleShadowVisible: false,
        headerLargeStyle: { backgroundColor: 'transparent' },
        headerLargeTitleEnabled: true,
        // Below iOS 26 there is no scroll edge effect, so the transparent header needs a material.
        headerBlurEffect: isLiquidGlass() ? 'none' : 'systemChromeMaterial',
        headerBackButtonDisplayMode: 'minimal',
        // The desktop's white page: the cards' rings and the folders' plates are drawn against it.
        contentStyle: { backgroundColor: colors.page },
      }}
    >
      <Stack.Screen name={root.name} options={{ title: root.title }} />
      <Stack.Screen name="note/[id]" options={{ headerLargeTitleEnabled: false, title: '' }} />
      <Stack.Screen name="trash" options={{ title: 'Trash' }} />
    </Stack>
  );
}
