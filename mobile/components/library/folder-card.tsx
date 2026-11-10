import { Link } from 'expo-router';
import { Text, View } from 'react-native';
import Animated, { useAnimatedStyle, useSharedValue, withSpring, type SharedValue } from 'react-native-reanimated';

import { GlassSurface } from '@/components/glass-surface';
import { lineKeys, preview } from '@/lib/library/model';
import { colors } from '@/lib/ui/colors';
import {
  cardScale,
  FOLDER_FRONT_TOP,
  FOLDER_RADIUS,
  FOLDER_TAB_RISE,
  FOLDER_TAB_WIDTH,
  PAPER_RADIUS,
  PAPERS,
} from '@/lib/ui/metrics';
import { FOLDER_OPEN, PAPER } from '@/lib/ui/motion';
import { Line } from './note-page';

/** A folder card opening: its papers rise one after another, the front panel dips. */
export type FolderMotion = { papers: SharedValue<number>[]; front: SharedValue<number>; open: (on: boolean) => void };

export function useFolderMotion(): FolderMotion {
  const p0 = useSharedValue(0);
  const p1 = useSharedValue(0);
  const p2 = useSharedValue(0);
  const front = useSharedValue(0);
  const papers = [p0, p1, p2];
  return {
    papers,
    front,
    open: (on) => {
      const to = on ? 1 : 0;
      papers.forEach((p, i) => p.set(withSpring(to, PAPER[i])));
      front.set(withSpring(to, FOLDER_OPEN));
    },
  };
}

type Props = {
  name: string;
  items: number;
  /** The first note's text, drawn on the front paper. */
  firstPage: string | null;
  width: number;
  height: number;
  motion: FolderMotion;
  /** The card is where the folder's page grows out of when it opens. */
  zoom: boolean;
};

function Paper({ index, width, height, k, firstPage, lift }: { index: number; width: number; height: number; k: number; firstPage: string | null; lift: SharedValue<number> }) {
  const [left, top, rise] = PAPERS[index];
  const lines = index === 0 && firstPage !== null ? preview(firstPage, 6) : [];
  const style = useAnimatedStyle(() => ({ transform: [{ translateY: -rise * k * lift.get() }] }));
  return (
    <Animated.View
      style={[
        {
          position: 'absolute',
          left: left * width,
          top: top * height,
          width: 0.8 * width,
          height: 0.64 * height,
          borderRadius: PAPER_RADIUS * k,
          borderCurve: 'continuous',
          backgroundColor: colors.paper,
          borderWidth: 1,
          borderColor: colors.fill,
          overflow: 'hidden',
          padding: 12 * k,
        },
        style,
      ]}
    >
      {index === 0 && firstPage !== null ? (
        <View style={{ gap: 1 }}>
          {lineKeys(lines).map((key, i) => (
            <Line key={key} line={lines[i]} />
          ))}
        </View>
      ) : (
        <View style={{ gap: 5, paddingTop: 4 }}>
          {[0.9, 0.75, 0.85, 0.6].map((w) => (
            <View key={w} style={{ height: 3, width: `${w * 100}%`, borderRadius: 2, backgroundColor: colors.fill }} />
          ))}
        </View>
      )}
    </Animated.View>
  );
}

/** A folder as stacked papers behind a Liquid Glass front panel, the desktop's folder card. */
export function FolderCard({ name, items, firstPage, width, height, motion, zoom }: Props) {
  const k = cardScale(width);
  const papers = Math.min(Math.max(items, 1), 3);
  const front = useAnimatedStyle(() => ({ transform: [{ translateY: 2 * motion.front.get() }] }));
  const plateTop = FOLDER_TAB_RISE * height;
  // Not collapsable: the zoom source takes one native child, and a flattened card would hand it the tab alone.
  // Page-coloured: the zoom draws the card's transparent corners black.
  const card = (
    <View collapsable={false} style={{ width, height, backgroundColor: colors.page }}>
      <View
        style={{
          position: 'absolute',
          top: 0,
          left: 0,
          width: FOLDER_TAB_WIDTH * width,
          height: plateTop + PAPER_RADIUS * k,
          borderTopLeftRadius: PAPER_RADIUS * k,
          borderTopRightRadius: PAPER_RADIUS * k,
          borderCurve: 'continuous',
          backgroundColor: colors.plate,
        }}
      />
      <View
        style={{
          position: 'absolute',
          top: plateTop,
          left: 0,
          right: 0,
          bottom: 0,
          borderRadius: FOLDER_RADIUS * k,
          borderTopLeftRadius: 0,
          borderCurve: 'continuous',
          backgroundColor: colors.plate,
        }}
      />
      {[2, 1, 0]
        .filter((i) => i < papers)
        .map((i) => (
          <Paper key={i} index={i} width={width} height={height} k={k} firstPage={firstPage} lift={motion.papers[i]} />
        ))}
      <Animated.View
        style={[{ position: 'absolute', left: 0, right: 0, bottom: 0, top: FOLDER_FRONT_TOP * height }, front]}
      >
        <GlassSurface style={{ flex: 1, borderRadius: FOLDER_RADIUS * k, borderCurve: 'continuous' }} />
      </Animated.View>
      <View style={{ position: 'absolute', left: 16 * k, right: 14 * k, bottom: 14 * k }}>
        <Text numberOfLines={1} style={{ fontSize: 17, fontWeight: '600', color: colors.text }}>
          {name}
        </Text>
        <Text style={{ fontSize: 12, color: colors.secondary, fontVariant: ['tabular-nums'] }}>
          {items === 1 ? '1 item' : `${items} items`}
        </Text>
      </View>
    </View>
  );
  return zoom ? <Link.AppleZoom>{card}</Link.AppleZoom> : card;
}
