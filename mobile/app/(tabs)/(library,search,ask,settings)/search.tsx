import { Stack } from 'expo-router';
import { useDeferredValue, useState } from 'react';
import { FlatList, Text } from 'react-native';

import { HeaderActions } from '@/components/header-actions';
import { NoteRow, RowSeparator } from '@/components/note-row';
import { capture } from '@/lib/app/actions';
import { useLibrary } from '@/lib/app/library';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

export default function Search() {
  const [query, setQuery] = useState('');
  const deferred = useDeferredValue(query);
  const searching = deferred.trim() !== '';
  const notes = useLibrary((lib) =>
    searching ? lib.search(deferred) : lib.allNotes().sort((a, b) => b.modified - a.modified).slice(0, 20),
  );
  return (
    <>
      <Stack.SearchBar
        placeholder="Search notes"
        autoCapitalize="none"
        onChangeText={(e) => setQuery(e.nativeEvent.text)}
        onCancelButtonPress={() => setQuery('')}
      />
      <HeaderActions
        items={[{ kind: 'button', sf: 'square.and.pencil', md: 'edit_square', label: 'New note in Inbox', onPress: capture }]}
      />
      <FlatList
        data={notes}
        keyExtractor={(n) => n.id}
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="on-drag"
        keyboardShouldPersistTaps="handled"
        ItemSeparatorComponent={RowSeparator}
        ListHeaderComponent={
          <Text style={{ paddingHorizontal: PAGE_PAD_X, paddingTop: 8, paddingBottom: 4, fontSize: 15, fontWeight: '600', color: colors.secondary }}>
            {searching ? (notes.length === 1 ? '1 note' : `${notes.length} notes`) : 'Recently edited'}
          </Text>
        }
        ListEmptyComponent={
          <Text style={{ paddingTop: 80, textAlign: 'center', fontSize: 15, color: colors.secondary }}>
            {searching ? `No notes match "${deferred.trim()}"` : 'No notes yet'}
          </Text>
        }
        renderItem={({ item }) => <NoteRow note={item} query={deferred} />}
      />
    </>
  );
}
