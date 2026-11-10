import { Stack } from 'expo-router';
import { useDeferredValue, useState } from 'react';
import { FlatList, Text, View } from 'react-native';

import { HeaderActions } from '@/components/header-actions';
import { Icon } from '@/components/icon';
import { NoteRow, RowSeparator } from '@/components/note-row';
import { capture } from '@/lib/app/actions';
import { useNow } from '@/lib/app/clock';
import { useLibrary } from '@/lib/app/library';
import { relative } from '@/lib/library/model';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

function Empty({ query }: { query: string }) {
  return (
    <View style={{ alignItems: 'center', gap: 8, paddingTop: 96, paddingHorizontal: 32 }}>
      <Icon sf="magnifyingglass" md="search" size={34} color={colors.tertiary} />
      <Text style={{ fontSize: 17, fontWeight: '600', color: colors.text, textAlign: 'center' }}>
        {query ? `Nothing for "${query}"` : 'No notes yet'}
      </Text>
      <Text style={{ fontSize: 15, color: colors.secondary, textAlign: 'center' }}>
        {query ? 'Search looks at titles and at every line of every note.' : 'Notes you write show up here, newest first.'}
      </Text>
    </View>
  );
}

export default function Search() {
  const now = useNow();
  const [query, setQuery] = useState('');
  const deferred = useDeferredValue(query);
  const trimmed = deferred.trim();
  const searching = trimmed !== '';
  const notes = useLibrary((lib) =>
    searching ? lib.search(deferred) : lib.allNotes().sort((a, b) => b.modified - a.modified).slice(0, 20),
  );
  return (
    <>
      <Stack.SearchBar
        // The tab has the search role, so on iOS 26 the field lives in the tab bar; "automatic"
        // leaves that placement to UIKit.
        placement="automatic"
        placeholder="Search notes"
        autoCapitalize="none"
        hideWhenScrolling={false}
        obscureBackground={false}
        onChangeText={(e) => setQuery(e.nativeEvent.text)}
        onCancelButtonPress={() => setQuery('')}
        onClose={() => setQuery('')}
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
        contentContainerStyle={{ paddingBottom: 24 }}
        ItemSeparatorComponent={RowSeparator}
        ListHeaderComponent={
          notes.length > 0 ? (
            <Text
              accessibilityRole="header"
              style={{ paddingHorizontal: PAGE_PAD_X, paddingTop: 8, paddingBottom: 6, fontSize: 20, fontWeight: '700', color: colors.text }}
            >
              {searching ? (notes.length === 1 ? '1 note' : `${notes.length} notes`) : 'Recently edited'}
            </Text>
          ) : undefined
        }
        ListEmptyComponent={<Empty query={trimmed} />}
        renderItem={({ item }) => <NoteRow note={item} query={deferred} detail={searching ? undefined : relative(item.modified, now)} />}
      />
    </>
  );
}
