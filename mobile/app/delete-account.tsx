import { router } from 'expo-router';
import { useState } from 'react';

import { Field, Form, FormButton, FormError, FormNote } from '@/components/form';
import { showNotice } from '@/lib/app/notice';
import { accountCall, sync, useSync } from '@/lib/app/sync';

export default function DeleteAccount() {
  const { email } = useSync();
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (password === '' || busy) return;
    if (await accountCall(() => sync.deleteAccount(password), setBusy, setError)) {
      showNotice('Account deleted. Your notes are still on this phone.');
      router.back();
    }
  };

  return (
    <Form>
      <FormNote>
        This deletes {email ?? 'your account'} and every note stored on the server. The notes on this phone stay, and stop syncing.
      </FormNote>
      <Field
        label="Password"
        value={password}
        onChangeText={setPassword}
        secureTextEntry
        textContentType="password"
        returnKeyType="go"
        onSubmitEditing={() => void submit()}
        autoFocus
      />
      <FormError message={error} />
      <FormButton title="Delete account" destructive onPress={() => void submit()} busy={busy} disabled={password === ''} />
    </Form>
  );
}
