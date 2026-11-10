// Asking for a name: the system alert with a text field on iOS, the name sheet on Android.

import { router } from 'expo-router';
import { Alert } from 'react-native';

export type Ask = { title: string; initial: string; confirm: string; allowEmpty?: boolean; secure?: boolean };

/** Cancel is null; an empty answer is null too unless the question allows it. */
function answer(ask: Ask, value: string | null | undefined): string | null {
  if (value === null || value === undefined) return null;
  const v = value.trim();
  return v === '' && !ask.allowEmpty ? null : v;
}

let pending: { ask: Ask; resolve: (name: string | null) => void } | null = null;

export function askName(ask: Ask): Promise<string | null> {
  if (process.env.EXPO_OS === 'ios') {
    return new Promise((resolve) => {
      Alert.prompt(
        ask.title,
        undefined,
        [
          { text: 'Cancel', style: 'cancel', onPress: () => resolve(null) },
          { text: ask.confirm, isPreferred: true, onPress: (value?: string) => resolve(answer(ask, value)) },
        ],
        ask.secure ? 'secure-text' : 'plain-text',
        ask.initial,
      );
    });
  }
  pending?.resolve(null);
  return new Promise((resolve) => {
    pending = { ask, resolve };
    router.push('/name');
  });
}

/** The name sheet reads what it asks for, and answers once. */
export function pendingAsk(): Ask | null {
  return pending?.ask ?? null;
}

export function answerAsk(name: string | null): void {
  if (pending) pending.resolve(answer(pending.ask, name));
  pending = null;
}
