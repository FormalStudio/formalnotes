// The user's model: provider, model and base URL in the library's settings, the key in the keychain.
// A computer's access code is kept apart from the provider key, so switching back loses neither.

import { useSyncExternalStore } from 'react';

import { DEFAULTS, type AiSettings, type Provider } from '../ai/ask';
import { library } from './library';
import { secure } from './secure';

const SETTINGS = 'ai.settings';
const keyOf = (provider: Provider) => (provider === 'computer' ? 'ai.code' : 'ai.key');

type AiState = { settings: AiSettings; hasKey: boolean; loaded: boolean };

let state: AiState = { settings: read(), hasKey: false, loaded: false };
const listeners = new Set<() => void>();

function read(): AiSettings {
  const raw = library.meta(SETTINGS);
  if (!raw) return DEFAULTS.anthropic;
  try {
    const s = JSON.parse(raw) as Partial<AiSettings>;
    const provider: Provider = s.provider === 'openai' || s.provider === 'computer' ? s.provider : 'anthropic';
    return { ...DEFAULTS[provider], ...s, provider };
  } catch {
    return DEFAULTS.anthropic;
  }
}

function set(patch: Partial<AiState>) {
  state = { ...state, ...patch };
  for (const l of listeners) l();
}

function loadKey() {
  const provider = state.settings.provider;
  void secure
    .get(keyOf(provider))
    .then((k) => {
      if (state.settings.provider === provider) set({ hasKey: !!k, loaded: true });
    })
    .catch(() => set({ loaded: true }));
}
loadKey();

export const ai = {
  subscribe(l: () => void) {
    listeners.add(l);
    return () => listeners.delete(l);
  },
  get: () => state,
  key: () => secure.get(keyOf(state.settings.provider)),
  update(patch: Partial<AiSettings>) {
    const next = { ...state.settings, ...patch };
    library.setMeta(SETTINGS, JSON.stringify(next));
    set({ settings: next });
  },
  /** Switching provider puts back that provider's model and endpoint. */
  setProvider(provider: Provider) {
    if (provider === state.settings.provider) return;
    library.setMeta(SETTINGS, JSON.stringify(DEFAULTS[provider]));
    set({ settings: DEFAULTS[provider], hasKey: false });
    loadKey();
  },
  async setKey(key: string) {
    const k = key.trim();
    const slot = keyOf(state.settings.provider);
    if (k === '') await secure.remove(slot);
    else await secure.set(slot, k);
    set({ hasKey: k !== '' });
  },
};

export function useAi(): AiState {
  return useSyncExternalStore(ai.subscribe, ai.get);
}
