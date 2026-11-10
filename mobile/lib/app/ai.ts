// The user's model: provider, model and base URL in the library's settings, the key in the keychain.

import { useSyncExternalStore } from 'react';

import { DEFAULTS, type AiSettings, type Provider } from '../ai/ask';
import { library } from './library';
import { secure } from './secure';

const SETTINGS = 'ai.settings';
const KEY = 'ai.key';

type AiState = { settings: AiSettings; hasKey: boolean; loaded: boolean };

let state: AiState = { settings: read(), hasKey: false, loaded: false };
const listeners = new Set<() => void>();

function read(): AiSettings {
  const raw = library.meta(SETTINGS);
  if (!raw) return DEFAULTS.anthropic;
  try {
    const s = JSON.parse(raw) as Partial<AiSettings>;
    const provider: Provider = s.provider === 'openai' ? 'openai' : 'anthropic';
    return { ...DEFAULTS[provider], ...s, provider };
  } catch {
    return DEFAULTS.anthropic;
  }
}

function set(patch: Partial<AiState>) {
  state = { ...state, ...patch };
  for (const l of listeners) l();
}

void secure
  .get(KEY)
  .then((k) => set({ hasKey: !!k, loaded: true }))
  .catch(() => set({ loaded: true }));

export const ai = {
  subscribe(l: () => void) {
    listeners.add(l);
    return () => listeners.delete(l);
  },
  get: () => state,
  key: () => secure.get(KEY),
  update(patch: Partial<AiSettings>) {
    const next = { ...state.settings, ...patch };
    library.setMeta(SETTINGS, JSON.stringify(next));
    set({ settings: next });
  },
  /** Switching provider puts back that provider's model and endpoint. */
  setProvider(provider: Provider) {
    if (provider === state.settings.provider) return;
    library.setMeta(SETTINGS, JSON.stringify(DEFAULTS[provider]));
    set({ settings: DEFAULTS[provider] });
  },
  async setKey(key: string) {
    const k = key.trim();
    if (k === '') await secure.remove(KEY);
    else await secure.set(KEY, k);
    set({ hasKey: k !== '' });
  },
};

export function useAi(): AiState {
  return useSyncExternalStore(ai.subscribe, ai.get);
}
