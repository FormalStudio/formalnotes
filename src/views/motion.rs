//! The design's motion, as nd animations. Its springs are given as stiffness `k` and damping `c`
//! on a unit mass; nd takes Apple's response (the period, `2π/√k`) and damping ratio
//! (`c / 2√k`).

use std::f64::consts::TAU;
use std::time::Duration;

use nd::prelude::*;

pub fn spring(k: f64, c: f64) -> Animation {
    let omega = k.sqrt();
    Animation::spring(TAU / omega, c / (2. * omega))
}

/// Layout moves and the hover lift: 420 / 31, settles in about a quarter second.
pub fn layout() -> Animation {
    spring(420., 31.)
}

/// A folder card opening its mouth on hover: 260 / 21.
pub fn folder_open() -> Animation {
    spring(260., 21.)
}

/// The papers in a folder card, each a little softer so they fan out one after another.
pub fn paper(i: usize) -> Animation {
    match i {
        0 => spring(320., 21.),
        1 => spring(230., 18.),
        _ => spring(170., 15.5),
    }
}

/// Cards coming in the first time a page shows: short fades, staggered, all done well within a
/// quarter second.
pub fn card_enter(index: usize, reduce: bool) -> Animation {
    const STEP: Duration = Duration::from_millis(15);
    const MAX_DELAY: Duration = Duration::from_millis(90);
    const FADE: Duration = Duration::from_millis(150);
    if reduce {
        return Animation::ease_out(FADE);
    }
    let delay = (STEP * index as u32).min(MAX_DELAY);
    Animation::Delayed { delay, duration: FADE, curve: Curve::EaseOut }
}

/// How far a card rises on hover, and comes up from on enter.
pub const LIFT: f32 = 3.;
pub const ENTER_RISE: f32 = 6.;
