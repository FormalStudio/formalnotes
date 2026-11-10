import { useState } from 'react';
import { RefreshControl, Text, useWindowDimensions, View } from 'react-native';
import Animated, { LinearTransition } from 'react-native-reanimated';

import { Icon } from '@/components/icon';
import { useNow } from '@/lib/app/clock';
import { useLibrary } from '@/lib/app/library';
import { sync, useSync } from '@/lib/app/sync';
import { relative } from '@/lib/library/model';
import type { Folder, Note } from '@/lib/library/store';
import { colors } from '@/lib/ui/colors';
import { CARD_ASPECT, CARD_GAP, CARD_GAP_FOLDERS, cardColumns, PAGE_PAD_X } from '@/lib/ui/metrics';
import { LAYOUT } from '@/lib/ui/motion';
import { FolderCard, useFolderMotion } from './folder-card';
import { NoteCard } from './note-card';
import { TableView } from './table-view';
import { Tile } from './tile';

type Item = { kind: 'folder'; folder: Folder; items: number; firstPage: string | null } | { kind: 'note'; note: Note };

const GLIDE = LinearTransition.springify().stiffness(LAYOUT.stiffness ?? 420).damping(LAYOUT.damping ?? 31);

function FolderTile({ item, width, height }: { item: Extract<Item, { kind: 'folder' }>; width: number; height: number }) {
  const motion = useFolderMotion();
  const { folder } = item;
  return (
    <Tile
      id={folder.id}
      kind="folder"
      path={folder.path}
      href={{ pathname: '/folder/[id]', params: { id: folder.id } }}
      label={`${folder.name}, folder, ${item.items === 1 ? '1 item' : `${item.items} items`}`}
      onPressed={motion.open}
    >
      <FolderCard name={folder.name} items={item.items} firstPage={item.firstPage} width={width} height={height} motion={motion} />
    </Tile>
  );
}

function NoteTile({ note, width, height, besideFolders, now }: { note: Note; width: number; height: number; besideFolders: boolean; now: number }) {
  const caption = relative(note.modified, now);
  return (
    <Tile
      id={note.id}
      kind="note"
      path={note.path}
      href={{ pathname: '/note/[id]', params: { id: note.id } }}
      label={`${note.title}, edited ${caption}`}
    >
      <NoteCard
        title={note.title}
        text={note.text}
        caption={caption}
        width={width}
        height={height}
        besideFolders={besideFolders}
        zoom={process.env.EXPO_OS === 'ios'}
      />
    </Tile>
  );
}

function Empty({ home }: { home: boolean }) {
  return (
    <View style={{ alignItems: 'center', gap: 8, paddingTop: 96, paddingHorizontal: 32 }}>
      <Icon sf="square.and.pencil" md="edit_square" size={34} color={colors.tertiary} />
      <Text style={{ fontSize: 17, fontWeight: '600', color: colors.text }}>{home ? 'No notes yet' : 'This folder is empty'}</Text>
      <Text style={{ fontSize: 15, color: colors.secondary, textAlign: 'center' }}>
        {home ? 'Tap the pencil to write your first note.' : 'Notes you add here show up as cards.'}
      </Text>
    </View>
  );
}

/** A folder's contents: its folders, then its notes, as the desktop's cards, or as a table. */
export function LibraryView({ folder }: { folder: string }) {
  const { width } = useWindowDimensions();
  const { email } = useSync();
  const [refreshing, setRefreshing] = useState(false);
  const now = useNow();
  const view = useLibrary((lib) => lib.folderView(folder));
  const items = useLibrary((lib): Item[] => [
    ...lib.subfolders(folder, 'title').map((f) => ({
      kind: 'folder' as const,
      folder: f,
      items: lib.count(f.path),
      firstPage: lib.notesIn(f.path)[0]?.text ?? null,
    })),
    ...lib.notesIn(folder).map((n) => ({ kind: 'note' as const, note: n })),
  ]);

  const refresh = email
    ? () => {
        setRefreshing(true);
        void sync.syncNow().finally(() => setRefreshing(false));
      }
    : undefined;
  const refreshControl = refresh ? <RefreshControl refreshing={refreshing} onRefresh={refresh} /> : undefined;

  if (view.view === 'table') return <TableView folder={folder} view={view} refreshControl={refreshControl} />;

  const hasFolders = items.some((i) => i.kind === 'folder');
  const gap = hasFolders ? CARD_GAP_FOLDERS : CARD_GAP;
  const avail = width - PAGE_PAD_X * 2;
  const columns = cardColumns(avail, CARD_GAP);
  const cardWidth = (avail - gap * (columns - 1)) / columns;
  const cardHeight = cardWidth * CARD_ASPECT;

  return (
    <Animated.FlatList
      key={columns}
      data={items}
      numColumns={columns}
      keyExtractor={(i) => (i.kind === 'folder' ? i.folder.id : i.note.id)}
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ paddingHorizontal: PAGE_PAD_X, paddingTop: 8, paddingBottom: 32, gap }}
      columnWrapperStyle={columns > 1 ? { gap } : undefined}
      itemLayoutAnimation={GLIDE}
      refreshControl={refreshControl}
      ListEmptyComponent={<Empty home={folder === ''} />}
      renderItem={({ item }) =>
        item.kind === 'folder' ? (
          <FolderTile item={item} width={cardWidth} height={cardHeight} />
        ) : (
          <NoteTile note={item.note} width={cardWidth} height={cardHeight} besideFolders={hasFolders} now={now} />
        )
      }
    />
  );
}
