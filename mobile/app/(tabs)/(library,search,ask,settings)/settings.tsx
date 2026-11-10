import { FieldGroup, Host, ListItem, Picker, RNHostView, Text } from '@expo/ui';
import Constants from 'expo-constants';
import { router } from 'expo-router';
import { Alert } from 'react-native';

import { ProfileHeader } from '@/components/account/profile-header';
import { ai, useAi } from '@/lib/app/ai';
import { useNow } from '@/lib/app/clock';
import { askName } from '@/lib/app/prompt';
import { sync, useSync, type SyncState } from '@/lib/app/sync';
import { DEFAULTS, type Provider } from '@/lib/ai/ask';
import { haptic } from '@/lib/haptics';
import { colors } from '@/lib/ui/colors';
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

/** A row's current value, on its trailing edge. */
function value(text: string) {
  return <Text textStyle={{ color: colors.secondary as string }}>{text}</Text>;
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

  const computer = settings.provider === 'computer';
  const setKey = async () => {
    const v = await askName({ title: computer ? 'Access code' : 'API key', initial: '', confirm: 'Save', secure: true, allowEmpty: true });
    if (v === null) return;
    await ai.setKey(v);
    haptic('success');
  };

  return (
    <Host style={{ flex: 1 }}>
      <FieldGroup>
        {account.email ? (
          <FieldGroup.Section>
            <RNHostView matchContents={{ vertical: true }}>
              <ProfileHeader email={account.email} uri={account.avatar} detail={syncLine(account, now)} />
            </RNHostView>
          </FieldGroup.Section>
        ) : null}

        <FieldGroup.Section title={account.email ? undefined : 'Account'}>
          {account.email ? (
            <>
              <ListItem onPress={() => void sync.syncNow()}>Sync now</ListItem>
              <ListItem onPress={() => router.push('/password')}>Change password</ListItem>
              <ListItem onPress={signOut}>Sign out</ListItem>
              <ListItem onPress={() => router.push('/delete-account')}>Delete account</ListItem>
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
                <Picker.Item label="Your Computer" value="computer" />
              </Picker>
            }
          >
            Provider
          </ListItem>
          {computer ? null : (
            <ListItem trailing={value(settings.model)} onPress={() => void edit('Model', settings.model, (model) => ai.update({ model }))}>
              Model
            </ListItem>
          )}
          <ListItem
            trailing={value(settings.baseUrl.replace(/^https?:\/\//, '') || 'Not set')}
            onPress={() => void edit(computer ? 'Address' : 'Base URL', settings.baseUrl, (baseUrl) => ai.update({ baseUrl }))}
          >
            {computer ? 'Address' : 'Base URL'}
          </ListItem>
          <ListItem trailing={value(hasKey ? 'Saved' : 'Not set')} onPress={() => void setKey()}>
            {computer ? 'Access code' : 'API key'}
          </ListItem>
          {!computer && (settings.model !== DEFAULTS[settings.provider].model || settings.baseUrl !== DEFAULTS[settings.provider].baseUrl) ? (
            <ListItem onPress={() => ai.update(DEFAULTS[settings.provider])}>Use the provider defaults</ListItem>
          ) : null}
          <FieldGroup.SectionFooter>
            <Text>
              {computer
                ? 'Answers come from Claude Code on your computer. In Formal Notes there, turn on Sharing in Settings and copy the address and access code it shows. Only the notes the model asks to read leave the phone.'
                : 'Questions go from this phone straight to your provider with your key. Only the notes the model asks to read leave the phone.'}
            </Text>
          </FieldGroup.SectionFooter>
        </FieldGroup.Section>

        <FieldGroup.Section>
          <ListItem trailing={value(Constants.expoConfig?.version ?? '')}>Version</ListItem>
        </FieldGroup.Section>
      </FieldGroup>
    </Host>
  );
}
