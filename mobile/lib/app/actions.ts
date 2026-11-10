// What the user does to notes and folders, with the feedback each one gives.

import { router } from 'expo-router';

import { haptic } from '../haptics';
import { nameOf, stemOf } from '../library/model';
import { library } from './library';
import { showNotice } from './notice';
import { askName } from './prompt';

export const INBOX = 'Inbox';

export type Focus = 'title' | 'body';

export function openNote(id: string, focus?: Focus): void {
  router.push({ pathname: '/note/[id]', params: focus ? { id, focus } : { id } });
}

export function newNote(folder: string): void {
  haptic('light');
  openNote(library.createNote(folder), 'title');
}

/** The brain dump: a new note in Inbox with the keyboard up in its body. */
export function capture(): void {
  haptic('light');
  openNote(library.createNote(INBOX), 'body');
}

export async function newFolder(parent: string): Promise<void> {
  haptic('light');
  const name = await askName({ title: 'New folder', initial: '', confirm: 'Create' });
  if (!name) return;
  library.createFolder(parent, name);
  haptic('success');
}

export async function rename(id: string): Promise<void> {
  const e = library.entry(id);
  if (!e) return;
  const note = e.kind === 'note' ? library.note(id) : null;
  const name = await askName({
    title: e.kind === 'note' ? 'Rename note' : 'Rename folder',
    initial: note ? note.title : nameOf(e.path),
    confirm: 'Rename',
  });
  if (!name) return;
  if (e.kind === 'note') library.retitle(id, name);
  else library.renameFolder(id, name);
  haptic('success');
}

export function moveTo(id: string, folder: string): void {
  library.move(id, folder);
  haptic('success');
  showNotice(folder === '' ? 'Moved to Home' : `Moved to ${nameOf(folder)}`);
}

export function trash(id: string): void {
  const e = library.entry(id);
  if (!e) return;
  const label = e.kind === 'note' ? (library.note(id)?.title ?? stemOf(e.path)) : nameOf(e.path);
  const batch = library.trash(id);
  if (!batch) return;
  haptic('medium');
  showNotice(`"${label}" moved to trash`, {
    label: 'Undo',
    run: () => {
      library.restore(batch);
      haptic('success');
    },
  });
}

/** Where an entry can move: Home and every folder but its own and, for a folder, its own subtree. */
export function destinations(id: string): { name: string; path: string }[] {
  const e = library.entry(id);
  if (!e) return [];
  const from = e.path.includes('/') ? e.path.slice(0, e.path.lastIndexOf('/')) : '';
  const out = from === '' ? [] : [{ name: 'Home', path: '' }];
  for (const f of library.allFolders()) {
    if (f.path === from || (e.kind === 'folder' && (f.path === e.path || f.path.startsWith(`${e.path}/`)))) continue;
    out.push({ name: f.path.replaceAll('/', ' / '), path: f.path });
  }
  return out;
}
