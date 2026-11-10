import { router } from 'expo-router';
import { useState } from 'react';

import { Field, Form, FormButton, FormError, FormNote } from '@/components/form';
import { showNotice } from '@/lib/app/notice';
import { accountCall, sync } from '@/lib/app/sync';

export default function ChangePassword() {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ready = current !== '' && next.length >= 8;

  const submit = async () => {
    if (!ready || busy) return;
    if (await accountCall(() => sync.changePassword(current, next), setBusy, setError)) {
      showNotice('Password changed');
      router.back();
    }
  };

  return (
    <Form>
      <Field label="Current password" value={current} onChangeText={setCurrent} secureTextEntry textContentType="password" autoFocus />
      <Field
        label="New password"
        value={next}
        onChangeText={setNext}
        secureTextEntry
        textContentType="newPassword"
        placeholder="At least 8 characters"
        returnKeyType="go"
        onSubmitEditing={() => void submit()}
      />
      <FormError message={error} />
      <FormButton title="Change password" onPress={() => void submit()} busy={busy} disabled={!ready} />
      <FormNote>Your notes stay encrypted with the same key; only the password that unlocks it changes. Other devices sign in again with the new one.</FormNote>
    </Form>
  );
}
