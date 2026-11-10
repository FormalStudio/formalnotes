import { router, Stack, useLocalSearchParams } from 'expo-router';
import { useRef, useState } from 'react';
import type { TextInputInstance } from 'react-native';

import { Field, Form, FormButton, FormError, FormLink, FormNote } from '@/components/form';
import { accountCall, sync } from '@/lib/app/sync';

const MIN_PASSWORD = 8;

export default function Account() {
  const params = useLocalSearchParams<{ mode?: 'signup' | 'signin' }>();
  const [mode, setMode] = useState(params.mode === 'signin' ? 'signin' : 'signup');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const passwordRef = useRef<TextInputInstance>(null);
  const signup = mode === 'signup';
  const ready = /.+@.+\..+/.test(email.trim()) && password.length >= (signup ? MIN_PASSWORD : 1);

  const submit = async () => {
    if (!ready || busy) return;
    const done = await accountCall(
      () => (signup ? sync.signUp(email.trim(), password) : sync.signIn(email.trim(), password)),
      setBusy,
      setError,
    );
    if (done) router.back();
  };

  return (
    <>
      <Stack.Screen options={{ title: signup ? 'Create account' : 'Sign in' }} />
      <Form>
        <Field
          label="Email"
          value={email}
          onChangeText={setEmail}
          autoCapitalize="none"
          autoCorrect={false}
          autoComplete="email"
          keyboardType="email-address"
          textContentType="username"
          returnKeyType="next"
          submitBehavior="submit"
          onSubmitEditing={() => passwordRef.current?.focus()}
          autoFocus
        />
        <Field
          ref={passwordRef}
          label="Password"
          value={password}
          onChangeText={setPassword}
          secureTextEntry
          autoComplete={signup ? 'new-password' : 'current-password'}
          textContentType={signup ? 'newPassword' : 'password'}
          placeholder={signup ? `At least ${MIN_PASSWORD} characters` : undefined}
          returnKeyType="go"
          onSubmitEditing={() => void submit()}
        />
        <FormError message={error} />
        <FormButton title={signup ? 'Create account' : 'Sign in'} onPress={() => void submit()} busy={busy} disabled={!ready} />
        <FormNote>
          {signup
            ? 'Your password encrypts your notes on this phone before they leave it. Nobody can reset it for you, so keep it somewhere safe.'
            : 'Notes already on this phone are kept and sync into your account.'}
        </FormNote>
        <FormLink
          title={signup ? 'I have an account' : 'Create an account instead'}
          onPress={() => {
            setMode(signup ? 'signin' : 'signup');
            setError(null);
          }}
        />
      </Form>
    </>
  );
}
