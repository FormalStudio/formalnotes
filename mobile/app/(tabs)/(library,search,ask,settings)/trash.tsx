import { Alert, FlatList, Pressable, Text, View } from 'react-native';

import { HeaderActions } from '@/components/header-actions';
import { Icon } from '@/components/icon';
import { RowSeparator } from '@/components/note-row';
import { useNow } from '@/lib/app/clock';
import { library, useLibrary } from '@/lib/app/library';
import { showNotice } from '@/lib/app/notice';
import { haptic } from '@/lib/haptics';
import { relative } from '@/lib/library/model';
import { colors } from '@/lib/ui/colors';
import { PAGE_PAD_X } from '@/lib/ui/metrics';

function empty() {
  Alert.alert('Empty trash?', 'What is in the trash is deleted from this phone for good.', [
    { text: 'Cancel', style: 'cancel' },
    {
      text: 'Empty trash',
      style: 'destructive',
      onPress: () => {
        library.emptyTrash();
        haptic('medium');
      },
    },
  ]);
}

export default function Trash() {
  const now = useNow();
  const items = useLibrary((lib) => lib.trashItems());
  return (
    <>
      {items.length > 0 ? (
        <HeaderActions items={[{ kind: 'button', sf: 'trash.slash', md: 'delete_forever', label: 'Empty trash', onPress: empty }]} />
      ) : null}
      <FlatList
        data={items}
        keyExtractor={(i) => i.batch + i.id}
        contentInsetAdjustmentBehavior="automatic"
        ItemSeparatorComponent={RowSeparator}
        ListEmptyComponent={<Text style={{ paddingTop: 96, textAlign: 'center', fontSize: 15, color: colors.secondary }}>Trash is empty</Text>}
        renderItem={({ item }) => (
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12, paddingHorizontal: PAGE_PAD_X, paddingVertical: 10 }}>
            <Icon
              sf={item.kind === 'folder' ? 'folder' : 'doc.text'}
              md={item.kind === 'folder' ? 'folder' : 'description'}
              color={colors.secondary}
            />
            <View style={{ flex: 1, gap: 2 }}>
              <Text numberOfLines={1} style={{ fontSize: 17, color: colors.text }}>
                {item.title}
              </Text>
              <Text style={{ fontSize: 13, color: colors.secondary }}>Deleted {relative(item.trashed, now).toLowerCase()}</Text>
            </View>
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={`Restore ${item.title}`}
              hitSlop={8}
              onPress={() => {
                library.restore(item.batch);
                haptic('success');
                showNotice(`"${item.title}" restored`);
              }}
              style={({ pressed }) => ({ paddingHorizontal: 8, paddingVertical: 6, opacity: pressed ? 0.5 : 1 })}
            >
              <Text style={{ fontSize: 15, fontWeight: '600', color: colors.accent }}>Restore</Text>
            </Pressable>
          </View>
        )}
      />
    </>
  );
}
