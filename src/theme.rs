//! The colours the app names itself: the design's shadows and the folder panel's see-through
//! fills, which no theme token carries.

use nd::prelude::*;
use nd::theme::{Scheme, Theme};

/// A card lifted under the pointer casts the design's two-layer card shadow, (1, 2, 4%) and
/// (4, 14, 6%); a dark scheme triples the alphas, up to 60%. At rest it casts none.
pub fn card_shadow(theme: &Theme, lifted: bool) -> Vec<BoxShadow> {
    if !lifted {
        return Vec::new();
    }
    let k = if theme.scheme == Scheme::Dark { 3. } else { 1. };
    [(1., 2., 0.04), (4., 14., 0.06)]
        .into_iter()
        .map(|(y, blur, alpha): (f32, f32, f32)| BoxShadow {
            color: hsla(0., 0., 0., (alpha * k).min(0.6)),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
            inset: false,
        })
        .collect()
}

/// Darkens the clear glass of a folder's front panel by half the raised surface.
pub fn folder_glass_tint(theme: &Theme) -> Hsla {
    theme.surface.raised.opacity(0.5)
}

/// A folder's front panel where there is no glass to blur the papers behind it.
pub fn folder_front_fill(theme: &Theme) -> Hsla {
    theme.surface.raised.opacity(0.85)
}

#[cfg(test)]
mod tests {
    use nd::theme::{Metrics, Palette, Preset};

    use super::*;

    #[test]
    fn a_lifted_card_casts_two_layers_stronger_in_the_dark() {
        let theme =
            |scheme| Theme::derive(Preset::MacOs, scheme, Palette::baked(scheme), Metrics::preset(Preset::MacOs), 1.);
        let light = theme(Scheme::Light);
        assert!(card_shadow(&light, false).is_empty(), "none at rest");
        let lifted = card_shadow(&light, true);
        assert_eq!(
            lifted.iter().map(|s| (s.offset.y, s.blur_radius)).collect::<Vec<_>>(),
            [(px(1.), px(2.)), (px(4.), px(14.))]
        );
        assert!((lifted[1].color.a - 0.06).abs() < 1e-6);
        let dark = theme(Scheme::Dark);
        assert!((card_shadow(&dark, true)[1].color.a - 0.18).abs() < 1e-6, "three times as dark");
    }
}
