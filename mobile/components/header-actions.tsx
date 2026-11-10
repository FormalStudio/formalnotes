import { Stack } from 'expo-router';

import type { HeaderItem } from './header-actions-types';

export type { HeaderItem, MenuItem } from './header-actions-types';

/** Native header buttons and menus on the right of a stack screen, drawn with SF Symbols. */
export function HeaderActions({ items }: { items: HeaderItem[] }) {
  return (
    <Stack.Toolbar placement="right">
      {items.map((item) =>
        item.kind === 'button' ? (
          <Stack.Toolbar.Button key={item.label} icon={item.sf} accessibilityLabel={item.label} onPress={item.onPress} />
        ) : (
          <Stack.Toolbar.Menu key={item.label} icon={item.sf} accessibilityLabel={item.label}>
            {item.groups.map((group, g) => (
              <Stack.Toolbar.Menu key={g} inline>
                {group.map((m) => (
                  <Stack.Toolbar.MenuAction key={m.title} icon={m.sf} isOn={m.checked} destructive={m.destructive} onPress={m.onPress}>
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
