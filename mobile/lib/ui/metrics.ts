// The desktop's card geometry (src/views/metrics.rs) in points. A phone card is narrower than the
// desktop's 236-point design card, so radii, rings and padding scale with it by `cardScale`.

import { Platform } from 'react-native';

export const MONO = Platform.select({ ios: 'Menlo', default: 'monospace' });

export const DESIGN_CARD_WIDTH = 236;
export const CARD_ASPECT = 264 / 236;
export const CARD_GAP = 16;
export const CARD_GAP_FOLDERS = 24;
export const CARD_FRAME = 5;
export const CARD_RADIUS = 20;
export const CARD_PADDING = 16;
export const CARD_FOOTER = 22;
export const PREVIEW_LINES = 14;

export const FOLDER_RADIUS = 18;
export const FOLDER_TAB_WIDTH = 0.4;
export const FOLDER_TAB_RISE = 0.06;
export const PAPER_RADIUS = 10;
export const FOLDER_FRONT_TOP = 0.5;

/** Each paper's left edge and top as shares of the card, and how far it rises when pressed. */
export const PAPERS: [number, number, number][] = [
  [0.1, 0.17, 10],
  [0.15, 0.135, 7],
  [0.05, 0.115, 4],
];

export const PAGE_PAD_X = 16;

/** How far above the safe area a floating bar sits to clear the tab bar. */
export const TAB_BAR_CLEARANCE = 64;

/** Text on a phone card: the desktop's miniature sizes, raised to stay legible at arm's length. */
export const TITLE_TEXT = 15;
export const TITLE_LINE = 19;
export const PREVIEW_TEXT = 9;
export const PREVIEW_LINE = 13;
export const PREVIEW_HEADING = 10;

/** The editor's body text. */
export const BODY_TEXT = 17;
export const BODY_LINE = 26;

/** The editor's title, from the desktop's 34 / 55, sized for a phone column. */
export const EDITOR_TITLE = 30;

/**
 * How many cards a row of `avail` points holds: four or more once four 200-point cards fit (each
 * then at least 240), otherwise one to three of at least 160. The desktop's rule; a phone in
 * portrait gets two.
 */
export function cardColumns(avail: number, gap: number): number {
  const fit = (w: number) => Math.max(0, Math.floor((avail + gap) / (w + gap)));
  return fit(200) >= 4 ? Math.max(fit(240), 4) : Math.min(Math.max(fit(160), 1), 3);
}

export function cardScale(width: number): number {
  return Math.min(1, width / DESIGN_CARD_WIDTH);
}
