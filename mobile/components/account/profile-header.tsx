import { useState } from 'react';
import { ActivityIndicator, Pressable, Text, View } from 'react-native';

import { changeAvatar } from '@/lib/app/avatar';
import { haptic } from '@/lib/haptics';
import { colors } from '@/lib/ui/colors';

import { Avatar } from './avatar';

const SIZE = 88;

/** The account at the top of Settings. Tapping the picture offers a photo, or removing the one there is. */
export function ProfileHeader({ email, uri, detail }: { email: string; uri: string | null; detail: string }) {
  const [busy, setBusy] = useState(false);
  return (
    <View style={{ alignItems: 'center', gap: 4, paddingVertical: 12, paddingHorizontal: 16 }}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel="Change profile picture"
        disabled={busy}
        onPress={() => {
          haptic('light');
          changeAvatar(uri !== null, setBusy);
        }}
        style={{ marginBottom: 8 }}
      >
        {({ pressed }) => (
          <View style={{ opacity: pressed || busy ? 0.6 : 1 }}>
            <Avatar email={email} uri={uri} size={SIZE} />
            {busy ? <ActivityIndicator style={{ position: 'absolute', top: 0, left: 0, right: 0, bottom: 0 }} color="#ffffff" /> : null}
          </View>
        )}
      </Pressable>
      <Text
        selectable
        numberOfLines={1}
        ellipsizeMode="middle"
        style={{ alignSelf: 'stretch', textAlign: 'center', color: colors.text, fontSize: 17, fontWeight: '600' }}
      >
        {email}
      </Text>
      <Text style={{ color: colors.secondary, fontSize: 13 }}>{detail}</Text>
    </View>
  );
}
