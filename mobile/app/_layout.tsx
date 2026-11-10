import { Stack } from 'expo-router';
import { DarkTheme, DefaultTheme, ThemeProvider } from 'expo-router/react-navigation';
import { useEffect } from 'react';
import { useColorScheme } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';

import { Snackbar } from '@/components/snackbar';
import { sync } from '@/lib/app/sync';

const SHEET = {
  presentation: 'formSheet',
  sheetGrabberVisible: true,
  sheetAllowedDetents: [0.6, 1] as number[],
  headerShown: true,
  headerTransparent: true,
  headerShadowVisible: false,
  contentStyle: { backgroundColor: 'transparent' },
} as const;

export default function RootLayout() {
  const scheme = useColorScheme();
  useEffect(() => sync.start(), []);
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <KeyboardProvider>
        <ThemeProvider value={scheme === 'dark' ? DarkTheme : DefaultTheme}>
          <Stack screenOptions={{ headerShown: false }}>
            <Stack.Screen name="(tabs)" />
            <Stack.Screen name="account" options={{ ...SHEET, sheetAllowedDetents: [1], title: '' }} />
            <Stack.Screen name="password" options={{ ...SHEET, sheetAllowedDetents: [1], title: 'Change password' }} />
            <Stack.Screen name="delete-account" options={{ ...SHEET, title: 'Delete account' }} />
            <Stack.Screen name="move" options={{ ...SHEET, title: 'Move to' }} />
            <Stack.Screen name="name" options={{ ...SHEET, sheetAllowedDetents: [0.35], title: '' }} />
            <Stack.Screen name="capture" options={{ animation: 'none' }} />
          </Stack>
          <Snackbar />
        </ThemeProvider>
      </KeyboardProvider>
    </GestureHandlerRootView>
  );
}
