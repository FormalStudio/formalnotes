import { Stack } from 'expo-router';
import { unstable_getMaterialSymbolSourceAsync, type AndroidSymbol } from 'expo-symbols';
import { useEffect, useState } from 'react';
import type { ImageSourcePropType } from 'react-native';

import type { HeaderItem } from './header-actions-types';

export type { HeaderItem, MenuItem } from './header-actions-types';

/** The Android toolbar takes images, so each Material glyph is drawn to one first. */
function useMaterialSources(names: AndroidSymbol[]): Record<string, ImageSourcePropType> {
  const [sources, setSources] = useState<Record<string, ImageSourcePropType>>({});
  const key = [...new Set(names)].join(',');
  useEffect(() => {
    let live = true;
    void Promise.all(
      key.split(',').map(async (n) => [n, await unstable_getMaterialSymbolSourceAsync(n as AndroidSymbol, 24, '#000000')] as const),
    ).then((pairs) => {
      if (live) setSources(Object.fromEntries(pairs.filter((p): p is [string, ImageSourcePropType] => p[1] !== null)));
    });
    return () => {
      live = false;
    };
  }, [key]);
  return sources;
}

/** Native header buttons and menus on the right of a stack screen, drawn with Material Symbols. */
export function HeaderActions({ items }: { items: HeaderItem[] }) {
  const names = items.flatMap((i) => [i.md, ...(i.kind === 'menu' ? i.groups.flat().flatMap((m) => (m.md ? [m.md] : [])) : [])]);
  const sources = useMaterialSources(names);
  const icon = (md: AndroidSymbol | undefined) => (md ? sources[md] : undefined);
  return (
    <Stack.Toolbar placement="right">
      {items.map((item) =>
        item.kind === 'button' ? (
          <Stack.Toolbar.Button key={item.label} icon={icon(item.md)} accessibilityLabel={item.label} onPress={item.onPress} />
        ) : (
          <Stack.Toolbar.Menu key={item.label} icon={icon(item.md)} accessibilityLabel={item.label}>
            {item.groups.map((group, g) => (
              <Stack.Toolbar.Menu key={g} inline>
                {group.map((m) => (
                  <Stack.Toolbar.MenuAction key={m.title} icon={icon(m.md)} isOn={m.checked} destructive={m.destructive} onPress={m.onPress}>
                    {m.title}
                  </Stack.Toolbar.MenuAction>
                ))}
              </Stack.Toolbar.Menu>
            ))}
          </Stack.Toolbar.Menu>
        ),
      )}
    </Stack.Toolbar>
  );
}
