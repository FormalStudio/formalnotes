import { Image } from 'expo-image';
import { Text, View } from 'react-native';

import { avatarGradient, avatarLetter } from '@/lib/ui/avatar';

/** The account's picture in a circle: its photo, or the email's first letter on its gradient. */
export function Avatar({ email, uri, size }: { email: string; uri: string | null; size: number }) {
  return (
    <View
      style={{
        width: size,
        height: size,
        borderRadius: size / 2,
        overflow: 'hidden',
        alignItems: 'center',
        justifyContent: 'center',
        experimental_backgroundImage: avatarGradient(email),
      }}
    >
      {uri ? (
        <Image source={{ uri }} style={{ position: 'absolute', width: size, height: size }} contentFit="cover" transition={150} />
      ) : (
        <Text style={{ color: '#ffffff', fontSize: size * 0.42, fontWeight: '700' }}>{avatarLetter(email)}</Text>
      )}
    </View>
  );
}
