// The desktop's springs (src/views/motion.rs), given there as stiffness and damping on a unit
// mass, which Reanimated's withSpring takes as they stand.

import type { WithSpringConfig } from 'react-native-reanimated';

const spring = (stiffness: number, damping: number): WithSpringConfig => ({ stiffness, damping, mass: 1 });

/** Layout moves: a card gliding to a new place, a page coming in. Settles in about 250ms. */
export const LAYOUT = spring(420, 31);
/** The lift going in fast and coming back slow. */
export const LIFT_IN = spring(1100, 66);
export const LIFT_OUT = spring(300, 35);
/** A folder card opening its mouth. */
export const FOLDER_OPEN = spring(260, 21);
/** A card dipping under a held press and coming back, a little bouncy. */
export const PRESS = spring(520, 15);
/** The papers in a folder card, each softer so they fan out one after another. */
export const PAPER = [spring(320, 21), spring(230, 18), spring(170, 15.5)];

/** How far a card rises when lifted, and how far below its place a page starts. */
export const LIFT = 3;
export const PAGE_RISE = 8;
/** A pressed card's scale. */
export const PRESSED_SCALE = 0.98;
