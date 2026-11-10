import { Link, router, type Href } from 'expo-router';
import type { ReactNode } from 'react';
import { Alert, Pressable, View } from 'react-native';
import Animated, { useAnimatedStyle, useReducedMotion, useSharedValue, withSpring, withTiming } from 'react-native-reanimated';

import { destinations, moveTo, newNote, rename, trash } from '@/lib/app/actions';
import { haptic } from '@/lib/haptics';
import { PRESS, PRESSED_SCALE } from '@/lib/ui/motion';

type Props = {
  id: string;
  kind: 'note' | 'folder';
  /** The folder's path, for "New note here". */
  path: string;
  href: Href;
  label: string;
  /** Called on press in and out, so a folder can lift its papers. */
  onPressed?: (pressed: boolean) => void;
  children: ReactNode;
};

/**
 * A card in the grid: it dips under the finger on the desktop's press spring, opens on a tap
 * and carries the long-press menu.
 */
export function Tile({ id, kind, path, href, label, onPressed, children }: Props) {
  const reduced = useReducedMotion();
  const scale = useSharedValue(1);
  const dim = useSharedValue(1);
  const style = useAnimatedStyle(() => ({ transform: [{ scale: scale.get() }], opacity: dim.get() }));

  const press = (on: boolean) => {
    if (reduced) dim.set(withTiming(on ? 0.7 : 1, { duration: 120 }));
    else scale.set(withSpring(on ? PRESSED_SCALE : 1, PRESS));
    if (!reduced) onPressed?.(on);
  };

  const androidMenu = () => {
    haptic('medium');
    Alert.alert(label, undefined, [
      { text: 'Rename', onPress: () => void rename(id) },
      { text: 'Move', onPress: () => router.push({ pathname: '/move', params: { id } }) },
      { text: 'Delete', style: 'destructive', onPress: () => trash(id) },
    ]);
  };

  // The link's menu mounts a second native view beside the trigger. One wrapper keeps the pair a
  // single child of the grid row, so the row's gap is applied once.
  return (
    <View>
      <Link href={href} asChild>
        <Link.Trigger>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={label}
            onPressIn={() => press(true)}
            onPressOut={() => press(false)}
            onPress={() => haptic('selection')}
            onLongPress={process.env.EXPO_OS === 'android' ? androidMenu : undefined}
          >
            <Animated.View style={style}>{children}</Animated.View>
          </Pressable>
        </Link.Trigger>
        <Link.Menu>
          {kind === 'folder' ? (
            <Link.MenuAction title="New note here" icon="square.and.pencil" onPress={() => newNote(path)} />
          ) : null}
          <Link.MenuAction title="Rename" icon="pencil" onPress={() => void rename(id)} />
          <Link.Menu title="Move to" icon="folder">
            {destinations(id).map((d) => (
              <Link.MenuAction
                key={d.path}
                title={d.name}
                icon={d.path === '' ? 'house' : 'folder'}
                onPress={() => moveTo(id, d.path)}
              />
            ))}
          </Link.Menu>
          <Link.MenuAction title="Delete" icon="trash" destructive onPress={() => trash(id)} />
        </Link.Menu>
      </Link>
    </View>
  );
}
