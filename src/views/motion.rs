//! The design's motion, as nd animations. Its springs are given as stiffness `k` and damping `c`
//! on a unit mass, which `Animation::spring_physics` takes as they stand.

use nd::prelude::*;

/// Layout moves: a card gliding to its new place on reflow, insert or remove, and going out.
/// 420 / 31, settles in about a quarter second.
pub fn layout() -> Animation {
    Animation::spring_physics(420., 31.)
}

/// The hover lift: fast in (1100 / 66), slow out (300 / 35).
pub fn hover(hovered: bool) -> Animation {
    if hovered { Animation::spring_physics(1100., 66.) } else { Animation::spring_physics(300., 35.) }
}

/// A folder card opening its mouth on hover: 260 / 21.
pub fn folder_open() -> Animation {
    Animation::spring_physics(260., 21.)
}

/// A card dipping while a press on it is held, and coming back: the folder press spring,
/// 520 / 15, a little bouncy.
pub fn press() -> Animation {
    Animation::spring_physics(520., 15.)
}

/// The papers in a folder card, each a little softer so they fan out one after another.
pub fn paper(i: usize) -> Animation {
    match i {
        0 => Animation::spring_physics(320., 21.),
        1 => Animation::spring_physics(230., 18.),
        _ => Animation::spring_physics(170., 15.5),
    }
}

/// How far a card rises on hover.
pub const LIFT: f32 = 3.;
