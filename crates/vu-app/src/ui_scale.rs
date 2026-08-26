use gpui::{Pixels, px};
use gpui_component::Theme;
use std::sync::atomic::{AtomicU32, Ordering};
use vu_core::config::{MAX_ICON_SCALE, MAX_UI_FONT_SIZE, MIN_ICON_SCALE, MIN_UI_FONT_SIZE};

const DEFAULT_UI_FONT_SIZE: f32 = 16.0;
const DEFAULT_MONO_FONT_SIZE: f32 = 13.0;
const MIN_DENSITY_SCALE: f32 = 0.92;
const MAX_DENSITY_SCALE: f32 = 1.25;
const DENSITY_SCALE_WEIGHT: f32 = 0.45;

// ponytail: one read-mostly f32 shared by every render pass. An atomic beats
// threading the config through every icon call site. Promote to a GPUI global if
// icon sizing ever has to differ per window.
static ICON_SCALE: AtomicU32 = AtomicU32::new(1.0f32.to_bits());

/// Set the chrome icon multiplier. Call whenever `appearance.icon_scale` changes.
pub(crate) fn set_icon_scale(scale: f32) {
    let scale = if scale.is_finite() {
        scale.clamp(MIN_ICON_SCALE, MAX_ICON_SCALE)
    } else {
        1.0
    };
    ICON_SCALE.store(scale.to_bits(), Ordering::Relaxed);
}

pub(crate) fn icon_scale() -> f32 {
    f32::from_bits(ICON_SCALE.load(Ordering::Relaxed))
}

/// Size for an icon glyph. Independent of `ui_font_size` so icons can be grown
/// without moving labels or padding.
pub(crate) fn icon_px(base_px: f32) -> Pixels {
    px(base_px * icon_scale())
}

/// Size for a control that contains an icon. Keeps the design's original padding
/// and grows only once the scaled glyph would overflow the base box.
pub(crate) fn icon_box_px(base_box_px: f32, base_icon_px: f32) -> Pixels {
    let padding = (base_box_px - base_icon_px).max(0.0);
    px(base_box_px.max(base_icon_px * icon_scale() + padding))
}

pub(crate) fn ui_font_scale(theme: &Theme) -> f32 {
    font_scale(
        theme.font_size.as_f32(),
        DEFAULT_UI_FONT_SIZE,
        MIN_UI_FONT_SIZE / DEFAULT_UI_FONT_SIZE,
        MAX_UI_FONT_SIZE / DEFAULT_UI_FONT_SIZE,
    )
}

pub(crate) fn mono_font_scale(theme: &Theme) -> f32 {
    font_scale(
        theme.mono_font_size.as_f32(),
        DEFAULT_MONO_FONT_SIZE,
        (MIN_UI_FONT_SIZE - 1.0) / DEFAULT_MONO_FONT_SIZE,
        (MAX_UI_FONT_SIZE - 3.0) / DEFAULT_MONO_FONT_SIZE,
    )
}

pub(crate) fn ui_density_scale(theme: &Theme) -> f32 {
    density_scale(ui_font_scale(theme))
}

pub(crate) fn mono_density_scale(theme: &Theme) -> f32 {
    density_scale(mono_font_scale(theme))
}

pub(crate) fn ui_px(theme: &Theme, base_px: f32) -> Pixels {
    px(base_px * ui_font_scale(theme))
}

pub(crate) fn mono_px(theme: &Theme, base_px: f32) -> Pixels {
    px(base_px * mono_font_scale(theme))
}

pub(crate) fn mono_space_px(theme: &Theme, base_px: f32) -> Pixels {
    px(base_px * mono_density_scale(theme))
}

fn font_scale(current_px: f32, default_px: f32, min_scale: f32, max_scale: f32) -> f32 {
    if !current_px.is_finite()
        || !default_px.is_finite()
        || !min_scale.is_finite()
        || !max_scale.is_finite()
        || default_px <= 0.0
        || min_scale > max_scale
    {
        return 1.0;
    }

    (current_px / default_px).clamp(min_scale, max_scale)
}

fn density_scale(font_scale: f32) -> f32 {
    if !font_scale.is_finite() {
        return 1.0;
    }

    (1.0 + (font_scale - 1.0) * DENSITY_SCALE_WEIGHT).clamp(MIN_DENSITY_SCALE, MAX_DENSITY_SCALE)
}

#[cfg(test)]
mod tests {
    use super::{density_scale, font_scale, icon_box_px, icon_px, set_icon_scale};
    use gpui::px;

    #[test]
    fn icon_scale_grows_glyph_and_keeps_padding() {
        set_icon_scale(1.0);
        assert_eq!(icon_px(12.0), px(12.0));
        // 22px box around a 12px glyph = 10px of padding, preserved when scaled.
        assert_eq!(icon_box_px(22.0, 12.0), px(22.0));

        set_icon_scale(2.0);
        assert_eq!(icon_px(12.0), px(24.0));
        assert_eq!(icon_box_px(22.0, 12.0), px(34.0));

        // Shrinking never collapses a control below its designed size.
        set_icon_scale(0.75);
        assert_eq!(icon_box_px(22.0, 12.0), px(22.0));

        set_icon_scale(f32::NAN);
        assert_eq!(icon_px(12.0), px(12.0));
        set_icon_scale(1.0);
    }

    #[test]
    fn font_scale_preserves_default_and_clamps_extremes() {
        assert_eq!(font_scale(16.0, 16.0, 0.75, 1.5), 1.0);
        assert_eq!(font_scale(1.0, 16.0, 0.75, 1.5), 0.75);
        assert_eq!(font_scale(200.0, 16.0, 0.75, 1.5), 1.5);
    }

    #[test]
    fn font_scale_ignores_invalid_values() {
        assert_eq!(font_scale(f32::NAN, 16.0, 0.75, 1.5), 1.0);
        assert_eq!(font_scale(16.0, 0.0, 0.75, 1.5), 1.0);
        assert_eq!(font_scale(16.0, 16.0, 1.5, 0.75), 1.0);
    }

    #[test]
    fn density_scale_grows_slower_than_text() {
        assert_eq!(density_scale(1.0), 1.0);
        assert!(density_scale(1.5) < 1.5);
        assert_eq!(density_scale(1.7), 1.25);
    }
}
