import { Link } from 'expo-router';
import { Text, View } from 'react-native';

import { colors } from '@/lib/ui/colors';
import { CARD_FOOTER, CARD_FRAME, CARD_PADDING, CARD_RADIUS, cardScale } from '@/lib/ui/metrics';
import { NotePage } from './note-page';

type Props = {
  title: string;
  text: string;
  caption: string;
  width: number;
  height: number;
  /** The frame is where the note grows out of when it opens. */
  zoom: boolean;
};

/** A note as a miniature page in a frame, with its date under it. */
export function NoteCard({ title, text, caption, width, height, zoom }: Props) {
  const k = cardScale(width);
  const frame = (
    <View
      style={{
        flex: 1,
        borderRadius: CARD_RADIUS * k,
        borderCurve: 'continuous',
        borderWidth: CARD_FRAME * k,
        borderColor: colors.frame,
        backgroundColor: colors.raised,
        overflow: 'hidden',
      }}
    >
      <NotePage title={title} text={text} padding={CARD_PADDING * k} />
    </View>
  );
  return (
    <View style={{ width, height }}>
      {zoom ? <Link.AppleZoom>{frame}</Link.AppleZoom> : frame}
      <View style={{ height: CARD_FOOTER, paddingHorizontal: 6, justifyContent: 'flex-end' }}>
        <Text numberOfLines={1} style={{ fontSize: 12, color: colors.secondary }}>
          {caption}
        </Text>
      </View>
    </View>
  );
}
