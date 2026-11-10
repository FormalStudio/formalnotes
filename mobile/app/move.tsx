import { router, useLocalSearchParams } from 'expo-router';
import { FlatList, Pressable, Text } from 'react-native';

import { Icon } from '@/components/icon';
import { destinations, moveTo } from '@/lib/app/actions';
import { useLibrary } from '@/lib/app/library';
import { colors } from '@/lib/ui/colors';

/** Where a note or folder can go, for Android's long press and the note's menu. */
export default function Move() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const places = useLibrary(() => destinations(id));
  return (
    <FlatList
      data={places}
      keyExtractor={(p) => p.path}
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ paddingVertical: 8 }}
      ListEmptyComponent={<Text style={{ padding: 24, textAlign: 'center', color: colors.secondary }}>No other folders yet</Text>}
      renderItem={({ item }) => (
        <Pressable
          accessibilityRole="button"
          onPress={() => {
            moveTo(id, item.path);
            router.back();
          }}
          style={({ pressed }) => ({
            flexDirection: 'row',
            alignItems: 'center',
            gap: 12,
            minHeight: 48,
            paddingHorizontal: 20,
            backgroundColor: pressed ? colors.fill : 'transparent',
          })}
        >
          <Icon sf={item.path === '' ? 'house' : 'folder'} md={item.path === '' ? 'home' : 'folder'} color={colors.accent} />
          <Text numberOfLines={1} style={{ flex: 1, fontSize: 17, color: colors.text }}>
            {item.name}
          </Text>
        </Pressable>
      )}
    />
  );
}
