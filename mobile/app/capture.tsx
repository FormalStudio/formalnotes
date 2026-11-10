import { router } from 'expo-router';
import { useEffect } from 'react';

import { INBOX } from '@/lib/app/actions';
import { library } from '@/lib/app/library';

/** `formalnotes://capture`: a new note in Inbox, opened with the keyboard in its body. */
export default function Capture() {
  useEffect(() => {
    const id = library.createNote(INBOX);
    router.replace({ pathname: '/note/[id]', params: { id, focus: 'body' } });
  }, []);
  return null;
}
