import { router, Stack } from 'expo-router';
import { useEffect, useState } from 'react';

import { Field, Form, FormButton } from '@/components/form';
import { answerAsk, pendingAsk } from '@/lib/app/prompt';

/** Android's stand-in for the iOS text prompt: asks for one line and answers whoever asked. */
export default function Name() {
  const [ask] = useState(pendingAsk);
  const [value, setValue] = useState(ask?.initial ?? '');
  // Closing the sheet any other way is a cancel; answering twice is a no-op.
  useEffect(() => () => answerAsk(null), []);
  const done = () => {
    answerAsk(value);
    router.back();
  };
  return (
    <>
      <Stack.Screen options={{ title: ask?.title ?? '' }} />
      <Form>
        <Field
          label={ask?.title ?? 'Name'}
          value={value}
          onChangeText={setValue}
          secureTextEntry={ask?.secure}
          autoFocus
          selectTextOnFocus
          returnKeyType="done"
          onSubmitEditing={done}
        />
        <FormButton title={ask?.confirm ?? 'Done'} onPress={done} disabled={!ask?.allowEmpty && value.trim() === ''} />
      </Form>
    </>
  );
}
