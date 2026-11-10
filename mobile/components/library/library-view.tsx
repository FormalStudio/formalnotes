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
import { CARD_ASPECT, CARD_FOOTER, CARD_GAP, cardColumns, PAGE_PAD_X, SECTION_GAP } from '@/lib/ui/metrics';
import { LAYOUT } from '@/lib/ui/motion';
import { FolderCard, useFolderMotion } from './folder-card';
import { NoteCard } from './note-card';
import { TableView } from './table-view';
import { Tile } from './tile';

type FolderItem = { folder: Folder; items: number; firstPage: string | null };

const GLIDE = LinearTransition.springify().stiffness(LAYOUT.stiffness ?? 420).damping(LAYOUT.damping ?? 31);
const ZOOM = process.env.EXPO_OS === 'ios';

function FolderTile({ item, width, height }: { item: FolderItem; width: number; height: number }) {
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
      <FolderCard name={folder.name} items={item.items} firstPage={item.firstPage} width={width} height={height} motion={motion} zoom={ZOOM} />
    </Tile>
  );
}

function NoteTile({ note, width, height, now }: { note: Note; width: number; height: number; now: number }) {
  const caption = relative(note.modified, now);
  return (
    <Tile
      id={note.id}
      kind="note"
      path={note.path}
      href={{ pathname: '/note/[id]', params: { id: note.id } }}
      label={`${note.title}, edited ${caption}`}
    >
      <NoteCard title={note.title} text={note.text} caption={caption} width={width} height={height} zoom={ZOOM} />
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

/** A folder's contents as the desktop's cards, folders in their own rows above the notes, or as a table. */
export function LibraryView({ folder }: { folder: string }) {
  const { width } = useWindowDimensions();
  const { email } = useSync();
  const [refreshing, setRefreshing] = useState(false);
  const now = useNow();
  const view = useLibrary((lib) => lib.folderView(folder));
  const folders = useLibrary((lib): FolderItem[] =>
    lib.subfolders(folder, 'title').map((f) => ({ folder: f, items: lib.count(f.path), firstPage: lib.notesIn(f.path)[0]?.text ?? null })),
  );
  const notes = useLibrary((lib) => lib.notesIn(folder));

  const refresh = email
    ? () => {
        setRefreshing(true);
        void sync.syncNow().finally(() => setRefreshing(false));
      }
    : undefined;
  const refreshControl = refresh ? <RefreshControl refreshing={refreshing} onRefresh={refresh} /> : undefined;

  if (view.view === 'table') return <TableView folder={folder} view={view} refreshControl={refreshControl} />;

  const avail = width - PAGE_PAD_X * 2;
  const columns = cardColumns(avail, CARD_GAP);
  const cardWidth = (avail - CARD_GAP * (columns - 1)) / columns;
  const cardHeight = cardWidth * CARD_ASPECT;

  return (
    <Animated.FlatList
      key={columns}
      data={notes}
      numColumns={columns}
      keyExtractor={(n) => n.id}
      contentInsetAdjustmentBehavior="automatic"
      showsVerticalScrollIndicator={false}
      contentContainerStyle={{ paddingHorizontal: PAGE_PAD_X, paddingTop: CARD_GAP / 2, paddingBottom: SECTION_GAP, gap: CARD_GAP }}
      columnWrapperStyle={columns > 1 ? { gap: CARD_GAP } : undefined}
      itemLayoutAnimation={GLIDE}
      refreshControl={refreshControl}
      ListHeaderComponent={
        folders.length > 0 ? (
          <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: CARD_GAP, paddingBottom: notes.length > 0 ? SECTION_GAP - CARD_GAP : 0 }}>
            {folders.map((item) => (
              <Animated.View key={item.folder.id} layout={GLIDE}>
                <FolderTile item={item} width={cardWidth} height={cardHeight} />
              </Animated.View>
            ))}
          </View>
        ) : undefined
      }
      ListEmptyComponent={folders.length === 0 ? <Empty home={folder === ''} /> : undefined}
      renderItem={({ item }) => <NoteTile note={item} width={cardWidth} height={cardHeight + CARD_FOOTER} now={now} />}
    />
  );
}
