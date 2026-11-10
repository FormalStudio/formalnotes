import { router, Stack } from 'expo-router';

import { HeaderActions, type MenuItem } from '@/components/header-actions';
import { newFolder, newNote } from '@/lib/app/actions';
import { library, useLibrary } from '@/lib/app/library';
import { LibraryView } from './library-view';

/** Home or a folder: its cards, with New note and the folder's menu in the header. */
export function FolderScreen({ folder, title }: { folder: string; title: string }) {
  const view = useLibrary((lib) => lib.folderView(folder));
  const show = (as: 'cards' | 'table'): MenuItem => ({
    title: as === 'cards' ? 'Cards' : 'Table',
    sf: as === 'cards' ? 'square.grid.2x2' : 'tablecells',
    md: as === 'cards' ? 'grid_view' : 'table_rows',
    checked: view.view === as,
    onPress: () => library.setFolderView(folder, { ...view, view: as }),
  });
  const groups: MenuItem[][] = [
    [{ title: 'New folder', sf: 'folder.badge.plus', md: 'create_new_folder', onPress: () => void newFolder(folder) }],
    [show('cards'), show('table')],
  ];
  if (folder === '') groups.push([{ title: 'Trash', sf: 'trash', md: 'delete', onPress: () => router.push('/trash') }]);
  return (
    <>
      <Stack.Screen options={{ title }} />
      <HeaderActions
        items={[
          { kind: 'menu', sf: 'ellipsis', md: 'more_vert', label: 'More', groups },
          { kind: 'button', sf: 'square.and.pencil', md: 'edit_square', label: 'New note', onPress: () => newNote(folder) },
        ]}
      />
      <LibraryView folder={folder} />
    </>
  );
}
