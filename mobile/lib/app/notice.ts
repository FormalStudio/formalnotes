// The one notice that carries an action: "Moved to trash", with Undo. A new one replaces the last.

import { useSyncExternalStore } from 'react';

export type Notice = { id: number; message: string; action?: { label: string; run: () => void } };

const HOLD = 5000;
let current: Notice | null = null;
let next = 1;
let timer: ReturnType<typeof setTimeout> | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const l of listeners) l();
}

export function showNotice(message: string, action?: Notice['action']): void {
  current = { id: next++, message, action };
  if (timer) clearTimeout(timer);
  timer = setTimeout(dismissNotice, HOLD);
  emit();
}

export function dismissNotice(): void {
  if (timer) clearTimeout(timer);
  timer = null;
  current = null;
  emit();
}

export function useNotice(): Notice | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => current,
  );
}
