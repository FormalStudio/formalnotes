// The minute that relative dates ("5 min ago") are measured from, ticking while anyone reads it.

import { useSyncExternalStore } from 'react';

let now = Date.now();
let timer: ReturnType<typeof setInterval> | null = null;
const listeners = new Set<() => void>();

function subscribe(l: () => void) {
  listeners.add(l);
  now = Date.now();
  timer ??= setInterval(() => {
    now = Date.now();
    for (const x of listeners) x();
  }, 60000);
  return () => {
    listeners.delete(l);
    if (listeners.size === 0 && timer) {
      clearInterval(timer);
      timer = null;
    }
  };
}

export function useNow(): number {
  return useSyncExternalStore(subscribe, () => now);
}
