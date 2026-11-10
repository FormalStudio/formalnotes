import type { AndroidSymbol, SFSymbol } from 'expo-symbols';

export type MenuItem = {
  title: string;
  sf?: SFSymbol;
  md?: AndroidSymbol;
  checked?: boolean;
  destructive?: boolean;
  onPress: () => void;
};

export type HeaderItem =
  | { kind: 'button'; sf: SFSymbol; md: AndroidSymbol; label: string; onPress: () => void }
  | { kind: 'menu'; sf: SFSymbol; md: AndroidSymbol; label: string; groups: MenuItem[][] };
