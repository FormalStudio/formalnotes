import { Stack, useLocalSearchParams } from 'expo-router';

import { FolderScreen } from '@/components/library/folder-screen';
import { useLibrary } from '@/lib/app/library';
import { nameOf } from '@/lib/library/model';

export default function FolderRoute() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const entry = useLibrary((lib) => lib.entry(id));
  if (!entry || entry.kind !== 'folder') return <Stack.Screen options={{ title: '' }} />;
  return <FolderScreen folder={entry.path} title={nameOf(entry.path)} />;
}
