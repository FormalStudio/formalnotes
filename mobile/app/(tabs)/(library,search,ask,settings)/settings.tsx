import { FieldGroup, Host, ListItem, Picker, Text } from '@expo/ui';
import Constants from 'expo-constants';
import { router } from 'expo-router';
import { Alert } from 'react-native';

import { ai, useAi } from '@/lib/app/ai';
import { useNow } from '@/lib/app/clock';
import { askName } from '@/lib/app/prompt';
import { sync, useSync, type SyncState } from '@/lib/app/sync';
import { DEFAULTS, type Provider } from '@/lib/ai/ask';
import { haptic } from '@/lib/haptics';
import { relative } from '@/lib/library/model';

function syncLine(s: SyncState, now: number): string {
  if (s.status === 'syncing') return 'Syncing';
  if (s.message) return s.message;
  return s.lastSync ? `Synced ${relative(s.lastSync, now).toLowerCase()}` : 'Not synced yet';
}

function signOut() {
  Alert.alert('Sign out?', 'Your notes stay on this phone. They stop syncing until you sign in again.', [
    { text: 'Cancel', style: 'cancel' },
    { text: 'Sign out', style: 'destructive', onPress: () => void sync.signOut() },
  ]);
}

export default function Settings() {
  const now = useNow();
  const account = useSync();
  const { settings, hasKey } = useAi();

  const edit = async (title: string, initial: string, apply: (v: string) => void) => {
    const v = await askName({ title, initial, confirm: 'Save' });
    if (v) {
      apply(v);
      haptic('success');
    }
  };

  const setKey = async () => {
    const v = await askName({ title: 'API key', initial: '', confirm: 'Save', secure: true, allowEmpty: true });
    if (v === null) return;
    await ai.setKey(v);
    haptic('success');
  };

  return (
    <Host style={{ flex: 1 }}>
      <FieldGroup>
        <FieldGroup.Section title="Account">
          {account.email ? (
            <>
              <ListItem supportingText={syncLine(account, now)}>{account.email}</ListItem>
              <ListItem onPress={() => void sync.syncNow()}>Sync now</ListItem>
              <ListItem onPress={() => router.push('/password')}>Change password</ListItem>
              <ListItem onPress={signOut}>Sign out</ListItem>
            </>
          ) : (
            <>
              <ListItem onPress={() => router.push({ pathname: '/account', params: { mode: 'signup' } })}>Create account</ListItem>
              <ListItem onPress={() => router.push({ pathname: '/account', params: { mode: 'signin' } })}>Sign in</ListItem>
            </>
          )}
          <FieldGroup.SectionFooter>
            <Text>
              {account.email
                ? 'Notes are encrypted on this phone before they sync. A forgotten password cannot be reset.'
                : 'Your notes live on this phone. An account syncs them, encrypted, with your other devices. A forgotten password cannot be reset.'}
            </Text>
          </FieldGroup.SectionFooter>
        </FieldGroup.Section>

        <FieldGroup.Section title="Ask">
          <ListItem
            trailing={
              <Picker
                selectedValue={settings.provider}
                appearance="menu"
                onValueChange={(p: Provider) => {
                  haptic('selection');
                  ai.setProvider(p);
                }}
              >
                <Picker.Item label="Anthropic" value="anthropic" />
                <Picker.Item label="OpenAI-compatible" value="openai" />
              </Picker>
            }
          >
            Provider
          </ListItem>
          <ListItem supportingText={settings.model} onPress={() => void edit('Model', settings.model, (model) => ai.update({ model }))}>
            Model
          </ListItem>
          <ListItem
            supportingText={settings.baseUrl}
            onPress={() => void edit('Base URL', settings.baseUrl, (baseUrl) => ai.update({ baseUrl }))}
          >
            Base URL
          </ListItem>
          <ListItem supportingText={hasKey ? 'Saved in the keychain' : 'Not set'} onPress={() => void setKey()}>
            API key
          </ListItem>
          {settings.model !== DEFAULTS[settings.provider].model || settings.baseUrl !== DEFAULTS[settings.provider].baseUrl ? (
            <ListItem onPress={() => ai.update(DEFAULTS[settings.provider])}>Use the provider defaults</ListItem>
          ) : null}
          <FieldGroup.SectionFooter>
            <Text>
              Questions go from this phone straight to your provider with your key. Only the notes the model asks to read leave the phone.
            </Text>
          </FieldGroup.SectionFooter>
        </FieldGroup.Section>

        <FieldGroup.Section title="Appearance">
          <ListItem supportingText="Light and dark follow the system.">Theme</ListItem>
        </FieldGroup.Section>

        {account.email ? (
          <FieldGroup.Section>
            <ListItem onPress={() => router.push('/delete-account')}>Delete account</ListItem>
          </FieldGroup.Section>
        ) : null}

        <FieldGroup.Section>
          <ListItem supportingText={`Version ${Constants.expoConfig?.version ?? ''}`}>Formal Notes</ListItem>
        </FieldGroup.Section>
      </FieldGroup>
    </Host>
  );
}
