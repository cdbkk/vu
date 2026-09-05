use gpui::*;
use vu_core::{
    Config,
    config::{
        AppearanceConfig, DEFAULT_TERMINAL_FONT_FAMILY, MAX_ICON_SCALE, MAX_UI_FONT_SIZE,
        MIN_ICON_SCALE, MIN_UI_FONT_SIZE, TabsOrientation, TerminalTweaks,
        is_gpui_pseudo_font_family, sanitize_terminal_font_family,
    },
};

use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_component::collapsible::Collapsible;
use gpui_component::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use gpui_component::input::InputState;
use gpui_component::select::{SearchableVec, Select, SelectEvent, SelectState};
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::switch::Switch;
use gpui_component::{ActiveTheme, Disableable, Icon, IndexPath, Sizable as _, input::Input};

use crate::motion::{MotionValue, vertical_reveal_offset};
use crate::ui_scale::ui_density_scale;
use std::collections::HashSet;
use url::Url;

actions!(settings, [ToggleSettings, SaveSettings, DismissSettings]);

/// Emitted when the user selects a different terminal theme for live preview.
pub struct ThemePreview(pub String);

/// An unsaved palette from the theme editor, applied to the live window so
/// edits show up on real tabs and terminal text. `ThemePreview` can't do this:
/// it carries a name, and the workspace resolves names off disk.
#[derive(Clone)]
pub struct ThemeLivePreview(pub vu_terminal::TerminalTheme);

/// Emitted for lightweight appearance changes that should be visible
/// immediately but should not persist/rebuild the full config.
pub struct AppearancePreview;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SettingsSection {
    General,
    Appearance,
    Keys,
}

impl SettingsSection {
    fn label(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Keys => "Keys",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Self::General => "phosphor/sliders.svg",
            Self::Appearance => "phosphor/sun.svg",
            Self::Keys => "phosphor/keyboard.svg",
        }
    }
}

const ALL_SECTIONS: &[SettingsSection] = &[
    SettingsSection::General,
    SettingsSection::Appearance,
    SettingsSection::Keys,
];

pub struct SettingsPanel {
    visible: bool,
    // ponytail: theme list cached here; all_available() hits disk and render runs per scroll tick
    all_themes: Vec<vu_terminal::TerminalTheme>,
    standalone: bool,
    config: Config,
    preview_snapshot: Option<Config>,
    focus_handle: FocusHandle,
    active_section: SettingsSection,
    overlay_motion: MotionValue,

    terminal_font_select: Entity<SelectState<SearchableVec<FontChoice>>>,
    ui_font_select: Entity<SelectState<SearchableVec<FontChoice>>>,
    cursor_style_select: Entity<SelectState<Vec<String>>>,
    font_size_input: Entity<InputState>,
    ui_font_size_input: Entity<InputState>,
    terminal_opacity_slider: Entity<SliderState>,
    icon_scale_slider: Entity<SliderState>,
    line_height_slider: Entity<SliderState>,
    letter_spacing_slider: Entity<SliderState>,
    minimum_contrast_slider: Entity<SliderState>,
    unfocused_split_slider: Entity<SliderState>,
    window_padding_slider: Entity<SliderState>,
    chrome_surface_slider: Entity<SliderState>,
    chrome_border_slider: Entity<SliderState>,
    terminal_blur: bool,
    ui_opacity_slider: Entity<SliderState>,
    tab_accent_inactive_alpha_slider: Entity<SliderState>,
    tab_accent_inactive_hover_alpha_slider: Entity<SliderState>,
    tab_inactive_opacity_slider: Entity<SliderState>,
    tab_close_size_slider: Entity<SliderState>,
    /// One picker per `TabColorSlot`, in `TabColorSlot::ALL` order.
    tab_color_pickers: Vec<Entity<ColorPickerState>>,
    background_image_input: Entity<InputState>,
    background_image_opacity_slider: Entity<SliderState>,
    background_image_position_select: Entity<SelectState<Vec<String>>>,
    background_image_fit_select: Entity<SelectState<Vec<String>>>,
    background_image_repeat: bool,
    hide_pane_title_bar: bool,
    save_error: Option<String>,
    save_error_kind: Option<SettingsSaveErrorKind>,
    last_saved_at: Option<std::time::SystemTime>,
    close_confirmation_visible: bool,

    // Theme import
    custom_theme_name_input: Entity<InputState>,
    custom_theme_preview: Option<vu_terminal::TerminalTheme>,
    custom_theme_status: Option<String>,
    /// Working copy for the palette editor. `None` means the editor is closed.
    theme_editor: Option<vu_terminal::TerminalTheme>,
    /// Slot picked by clicking the preview, surfaced at the top of the editor.
    theme_editor_slot: Option<usize>,
    /// One picker per `THEME_SLOTS` entry, built once and retargeted on open.
    theme_editor_pickers: Vec<Entity<ColorPickerState>>,
    theme_editor_name_input: Entity<InputState>,
    theme_editor_status: Option<String>,
    /// Theme to restore if the editor is closed without saving.
    theme_editor_original: Option<String>,
    /// Group headings the user has collapsed, by group id. Everything starts
    /// expanded, so a setting is never hidden until the user hides it.
    /// ponytail: in-memory only — the panel outlives every open/close, so this
    /// survives a session. Persist to config if it needs to survive a restart.
    collapsed_groups: HashSet<&'static str>,

    // Keybindings — which binding is being recorded (field name, e.g. "new_tab")
    recording_key: Option<String>,
    #[cfg(target_os = "macos")]
    recording_resume_keybindings: Option<vu_core::config::KeybindingConfig>,

    // Network / proxy
    http_proxy_input: Entity<InputState>,
    https_proxy_input: Entity<InputState>,

    new_tab_directory_input: Entity<InputState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsSaveErrorKind {
    KeybindingConflict,
    Other,
}

const BACKGROUND_IMAGE_POSITIONS: &[&str] = &[
    "top-left",
    "top-center",
    "top-right",
    "center-left",
    "center",
    "center-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];

const BACKGROUND_IMAGE_FITS: &[&str] = &["contain", "cover", "stretch", "none"];

impl SettingsPanel {
    fn clamp_ui_font_size(value: f32) -> f32 {
        value.clamp(MIN_UI_FONT_SIZE, MAX_UI_FONT_SIZE)
    }

    fn clamp_icon_scale(value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(MIN_ICON_SCALE, MAX_ICON_SCALE)
        } else {
            1.0
        }
    }

    fn icon_scale_value(&self) -> f32 {
        Self::clamp_icon_scale(self.config.appearance.icon_scale)
    }

    fn clamp_terminal_opacity(value: f32) -> f32 {
        value.clamp(0.25, 1.0)
    }

    fn clamp_ui_opacity(value: f32) -> f32 {
        value.clamp(0.35, 1.0)
    }

    fn terminal_blur_supported() -> bool {
        !cfg!(target_os = "linux")
    }

    fn effective_terminal_blur(value: bool) -> bool {
        value && Self::terminal_blur_supported()
    }

    fn terminal_opacity_value(&self) -> f32 {
        Self::clamp_terminal_opacity(self.config.appearance.terminal_opacity)
    }

    fn ui_opacity_value(&self) -> f32 {
        Self::clamp_ui_opacity(self.config.appearance.ui_opacity)
    }

    fn clamp_tab_accent_inactive_alpha(value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(
                AppearanceConfig::MIN_TAB_ACCENT_ALPHA,
                AppearanceConfig::MAX_TAB_ACCENT_INACTIVE_ALPHA,
            )
        } else {
            crate::tab_colors::TAB_ACCENT_INACTIVE_ALPHA
        }
    }

    fn clamp_tab_accent_inactive_hover_alpha(value: f32, inactive: f32) -> f32 {
        let value = if value.is_finite() {
            value.clamp(
                AppearanceConfig::MIN_TAB_ACCENT_ALPHA,
                AppearanceConfig::MAX_TAB_ACCENT_INACTIVE_HOVER_ALPHA,
            )
        } else {
            crate::tab_colors::TAB_ACCENT_INACTIVE_HOVER_ALPHA
        };
        value.max(inactive)
    }

    fn clamp_tab_inactive_opacity(value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.35
        }
    }

    fn clamp_tab_close_size(value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(
                AppearanceConfig::MIN_TAB_CLOSE_SIZE,
                AppearanceConfig::MAX_TAB_CLOSE_SIZE,
            )
        } else {
            13.0
        }
    }

    fn tab_inactive_opacity_value(&self) -> f32 {
        Self::clamp_tab_inactive_opacity(self.config.appearance.tab_inactive_opacity)
    }

    fn tab_close_size_value(&self) -> f32 {
        Self::clamp_tab_close_size(self.config.appearance.tab_close_size)
    }

    fn tab_accent_inactive_alpha_value(&self) -> f32 {
        Self::clamp_tab_accent_inactive_alpha(self.config.appearance.tab_accent_inactive_alpha)
    }

    fn tab_accent_inactive_hover_alpha_value(&self) -> f32 {
        Self::clamp_tab_accent_inactive_hover_alpha(
            self.config.appearance.tab_accent_inactive_hover_alpha,
            self.tab_accent_inactive_alpha_value(),
        )
    }

    fn clamp_background_image_opacity(value: f32) -> f32 {
        value.clamp(0.0, 1.0)
    }

    fn background_image_opacity_value(&self) -> f32 {
        Self::clamp_background_image_opacity(self.config.appearance.background_image_opacity)
    }

    fn make_string_select(
        options: &[&str],
        current_value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<Vec<String>>> {
        let items: Vec<String> = options.iter().map(|value| (*value).to_string()).collect();
        let selected_index = items
            .iter()
            .position(|item| item == current_value)
            .map(IndexPath::new);
        cx.new(|cx| SelectState::new(items, selected_index, window, cx))
    }

    fn cursor_style_label(value: &str) -> &'static str {
        match value.trim().to_ascii_lowercase().as_str() {
            "block" => "Block",
            "underline" => "Underline",
            "block_hollow" | "block-hollow" | "hollow" => "Hollow Block",
            _ => "Bar",
        }
    }

    fn cursor_style_from_label(label: &str) -> &'static str {
        match label {
            "Block" => "block",
            "Underline" => "underline",
            "Hollow Block" => "block_hollow",
            _ => "bar",
        }
    }

    fn prepare_terminal_font_families(
        config: &Config,
        mut font_families: Vec<String>,
    ) -> Vec<String> {
        font_families.sort_by_key(|name| name.to_lowercase());
        font_families.dedup();

        let mut preferred = Vec::new();
        let sanitized_terminal_family = sanitize_terminal_font_family(&config.terminal.font_family);
        for family in [
            DEFAULT_TERMINAL_FONT_FAMILY,
            sanitized_terminal_family.as_str(),
        ] {
            if !family.is_empty() && !preferred.iter().any(|existing| existing == family) {
                preferred.push(family.to_string());
            }
        }

        for family in font_families {
            if !is_gpui_pseudo_font_family(&family)
                && !preferred.iter().any(|existing| existing == &family)
            {
                preferred.push(family);
            }
        }
        preferred
    }

    fn prepare_ui_font_families(config: &Config, mut font_families: Vec<String>) -> Vec<String> {
        font_families.sort_by_key(|name| name.to_lowercase());
        font_families.dedup();

        let mut preferred = Vec::new();
        for family in [
            ".SystemUIFont",
            config.appearance.ui_font_family.as_str(),
            DEFAULT_TERMINAL_FONT_FAMILY,
        ] {
            if !family.is_empty() && !preferred.iter().any(|existing| existing == family) {
                preferred.push(family.to_string());
            }
        }

        for family in font_families {
            if !preferred.iter().any(|existing| existing == &family) {
                preferred.push(family);
            }
        }
        preferred
    }

    fn make_font_select(
        options: &[String],
        current_value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SelectState<SearchableVec<FontChoice>>> {
        let items = SearchableVec::new(options.iter().cloned().map(FontChoice).collect::<Vec<_>>());
        let selected_index = options
            .iter()
            .position(|item| item == current_value)
            .map(IndexPath::new);
        cx.new(|cx| SelectState::new(items, selected_index, window, cx).searchable(true))
    }

    fn card_opacity(&self) -> f32 {
        0.74
    }

    pub fn new(config: &Config, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut config = config.clone();
        config.appearance.terminal_blur =
            Self::effective_terminal_blur(config.appearance.terminal_blur);
        let all_font_families = cx.text_system().all_font_names();
        let terminal_font_families =
            Self::prepare_terminal_font_families(&config, all_font_families.clone());
        let ui_font_families = Self::prepare_ui_font_families(&config, all_font_families);
        let terminal_font_family = sanitize_terminal_font_family(&config.terminal.font_family);
        let terminal_font_select =
            Self::make_font_select(&terminal_font_families, &terminal_font_family, window, cx);
        let ui_font_select = Self::make_font_select(
            &ui_font_families,
            &config.appearance.ui_font_family,
            window,
            cx,
        );
        let cursor_style_select = Self::make_string_select(
            &["Bar", "Block", "Underline", "Hollow Block"],
            Self::cursor_style_label(&config.terminal.cursor_style),
            window,
            cx,
        );
        let font_size_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_placeholder("14.0", window, cx);
            s.set_value(&config.terminal.font_size.to_string(), window, cx);
            s
        });
        let ui_font_size_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_placeholder("16.0", window, cx);
            s.set_value(
                &Self::clamp_ui_font_size(config.appearance.ui_font_size).to_string(),
                window,
                cx,
            );
            s
        });
        let tw = &config.terminal.tweaks;
        let line_height_slider = cx.new(|_| {
            SliderState::new()
                .min(TerminalTweaks::MIN_SPACING_PERCENT)
                .max(60.0)
                .step(1.0)
                .default_value(tw.line_height_percent)
        });
        let letter_spacing_slider = cx.new(|_| {
            SliderState::new()
                .min(TerminalTweaks::MIN_SPACING_PERCENT)
                .max(60.0)
                .step(1.0)
                .default_value(tw.letter_spacing_percent)
        });
        let minimum_contrast_slider = cx.new(|_| {
            SliderState::new()
                .min(1.0)
                .max(TerminalTweaks::MAX_CONTRAST)
                .step(0.5)
                .default_value(tw.minimum_contrast)
        });
        let unfocused_split_slider = cx.new(|_| {
            SliderState::new()
                .min(0.15)
                .max(1.0)
                .step(0.05)
                .default_value(tw.unfocused_split_opacity)
        });
        let window_padding_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(TerminalTweaks::MAX_PADDING)
                .step(1.0)
                .default_value(tw.window_padding_x)
        });
        let chrome_surface_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(3.0)
                .step(0.1)
                .default_value(config.appearance.chrome_surface_strength)
        });
        let chrome_border_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(3.0)
                .step(0.1)
                .default_value(config.appearance.chrome_border_strength)
        });
        let icon_scale_slider = cx.new(|_| {
            SliderState::new()
                .min(MIN_ICON_SCALE)
                .max(MAX_ICON_SCALE)
                .step(0.05)
                .default_value(Self::clamp_icon_scale(config.appearance.icon_scale))
        });
        let terminal_opacity_slider = cx.new(|_| {
            SliderState::new()
                .min(0.25)
                .max(1.0)
                .step(0.01)
                .default_value(Self::clamp_terminal_opacity(
                    config.appearance.terminal_opacity,
                ))
        });
        let ui_opacity_slider = cx.new(|_| {
            SliderState::new()
                .min(0.35)
                .max(1.0)
                .step(0.01)
                .default_value(Self::clamp_ui_opacity(config.appearance.ui_opacity))
        });
        let tab_accent_inactive_alpha_slider = cx.new(|_| {
            SliderState::new()
                .min(AppearanceConfig::MIN_TAB_ACCENT_ALPHA)
                .max(AppearanceConfig::MAX_TAB_ACCENT_INACTIVE_ALPHA)
                .step(0.01)
                .default_value(Self::clamp_tab_accent_inactive_alpha(
                    config.appearance.tab_accent_inactive_alpha,
                ))
        });
        let tab_accent_inactive_hover_alpha_slider = cx.new(|_| {
            let inactive =
                Self::clamp_tab_accent_inactive_alpha(config.appearance.tab_accent_inactive_alpha);
            SliderState::new()
                .min(AppearanceConfig::MIN_TAB_ACCENT_ALPHA)
                .max(AppearanceConfig::MAX_TAB_ACCENT_INACTIVE_HOVER_ALPHA)
                .step(0.01)
                .default_value(Self::clamp_tab_accent_inactive_hover_alpha(
                    config.appearance.tab_accent_inactive_hover_alpha,
                    inactive,
                ))
        });
        let tab_inactive_opacity_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(Self::clamp_tab_inactive_opacity(
                    config.appearance.tab_inactive_opacity,
                ))
        });
        let tab_close_size_slider = cx.new(|_| {
            SliderState::new()
                .min(AppearanceConfig::MIN_TAB_CLOSE_SIZE)
                .max(AppearanceConfig::MAX_TAB_CLOSE_SIZE)
                .step(1.0)
                .default_value(Self::clamp_tab_close_size(config.appearance.tab_close_size))
        });
        let background_image_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_placeholder("~/Pictures/wallpaper.jpg", window, cx);
            s.set_value(
                &config
                    .appearance
                    .background_image
                    .clone()
                    .unwrap_or_default(),
                window,
                cx,
            );
            s
        });
        let background_image_opacity_slider = cx.new(|_| {
            SliderState::new()
                .min(0.0)
                .max(1.0)
                .step(0.01)
                .default_value(Self::clamp_background_image_opacity(
                    config.appearance.background_image_opacity,
                ))
        });
        let background_image_position_select = Self::make_string_select(
            BACKGROUND_IMAGE_POSITIONS,
            &config.appearance.background_image_position,
            window,
            cx,
        );
        let background_image_fit_select = Self::make_string_select(
            BACKGROUND_IMAGE_FITS,
            &config.appearance.background_image_fit,
            window,
            cx,
        );
        let custom_theme_name_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_placeholder("Save as, e.g. flexoki-amber", window, cx);
            s
        });
        let theme_editor_pickers: Vec<Entity<ColorPickerState>> = THEME_SLOTS
            .iter()
            .map(|_| cx.new(|cx| ColorPickerState::new(window, cx)))
            .collect();
        for (idx, picker) in theme_editor_pickers.iter().enumerate() {
            cx.subscribe(picker, move |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(hsla)) = event else {
                    return;
                };
                this.set_theme_slot(idx, *hsla, cx);
            })
            .detach();
        }
        let theme_editor_name_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_placeholder("my-theme", window, cx);
            s
        });
        let http_proxy_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            let val = config.network.http_proxy.clone().unwrap_or_default();
            s.set_value(val, window, cx);
            s.set_placeholder("http://127.0.0.1:1086", window, cx);
            s
        });
        let https_proxy_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            let val = config.network.https_proxy.clone().unwrap_or_default();
            s.set_value(val, window, cx);
            s.set_placeholder("http://127.0.0.1:1086", window, cx);
            s
        });
        let new_tab_directory_input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(&config.terminal.new_tab_directory, window, cx);
            s.set_placeholder("inherit", window, cx);
            s
        });

        macro_rules! tweak_slider {
            ($slider:expr, $field:ident) => {
                cx.subscribe(&$slider, |this, _, event: &SliderEvent, cx| {
                    let SliderEvent::Change(value) = event;
                    this.config.terminal.tweaks.$field = value.end();
                    this.config.terminal.tweaks.normalize();
                    cx.emit(AppearancePreview);
                    cx.notify();
                })
                .detach();
            };
        }
        macro_rules! chrome_slider {
            ($slider:expr, $field:ident) => {
                cx.subscribe(&$slider, |this, _, event: &SliderEvent, cx| {
                    let SliderEvent::Change(value) = event;
                    this.config.appearance.$field = value.end();
                    crate::theme::set_chrome_strengths(
                        this.config.appearance.chrome_surface_strength,
                        this.config.appearance.chrome_border_strength,
                    );
                    cx.emit(AppearancePreview);
                    cx.notify();
                })
                .detach();
            };
        }
        chrome_slider!(chrome_surface_slider, chrome_surface_strength);
        chrome_slider!(chrome_border_slider, chrome_border_strength);
        tweak_slider!(line_height_slider, line_height_percent);
        tweak_slider!(letter_spacing_slider, letter_spacing_percent);
        tweak_slider!(minimum_contrast_slider, minimum_contrast);
        tweak_slider!(unfocused_split_slider, unfocused_split_opacity);
        cx.subscribe(
            &window_padding_slider,
            |this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(value) = event;
                // One control drives both axes; separate x/y padding is a config-file
                // level knob, not something worth two sliders in the panel.
                this.config.terminal.tweaks.window_padding_x = value.end();
                this.config.terminal.tweaks.window_padding_y = value.end();
                this.config.terminal.tweaks.normalize();
                cx.emit(AppearancePreview);
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(&icon_scale_slider, |this, _, event: &SliderEvent, cx| {
            match event {
                SliderEvent::Change(value) => {
                    let scale = Self::clamp_icon_scale(value.end());
                    this.config.appearance.icon_scale = scale;
                    // Icon sizing is read from a process global at paint time, so
                    // push it before notifying or the preview lags one frame.
                    crate::ui_scale::set_icon_scale(scale);
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            }
        })
        .detach();
        cx.subscribe(
            &terminal_opacity_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.config.appearance.terminal_opacity =
                        Self::clamp_terminal_opacity(value.end());
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            &ui_opacity_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.config.appearance.ui_opacity = Self::clamp_ui_opacity(value.end());
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &tab_accent_inactive_alpha_slider,
            window,
            |this, _, event: &SliderEvent, window, cx| match event {
                SliderEvent::Change(value) => {
                    let inactive = Self::clamp_tab_accent_inactive_alpha(value.end());
                    this.config.appearance.tab_accent_inactive_alpha = inactive;
                    let hover = Self::clamp_tab_accent_inactive_hover_alpha(
                        this.config.appearance.tab_accent_inactive_hover_alpha,
                        inactive,
                    );
                    this.config.appearance.tab_accent_inactive_hover_alpha = hover;
                    this.tab_accent_inactive_hover_alpha_slider
                        .update(cx, |slider, cx| slider.set_value(hover, window, cx));
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &tab_accent_inactive_hover_alpha_slider,
            window,
            |this, _, event: &SliderEvent, window, cx| match event {
                SliderEvent::Change(value) => {
                    let inactive = Self::clamp_tab_accent_inactive_alpha(
                        this.config.appearance.tab_accent_inactive_alpha,
                    );
                    let hover = Self::clamp_tab_accent_inactive_hover_alpha(value.end(), inactive);
                    this.config.appearance.tab_accent_inactive_hover_alpha = hover;
                    this.tab_accent_inactive_hover_alpha_slider
                        .update(cx, |slider, cx| slider.set_value(hover, window, cx));
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            &tab_inactive_opacity_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.config.appearance.tab_inactive_opacity =
                        Self::clamp_tab_inactive_opacity(value.end());
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            &tab_close_size_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.config.appearance.tab_close_size = Self::clamp_tab_close_size(value.end());
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        let tab_color_pickers: Vec<Entity<ColorPickerState>> = TabColorSlot::ALL
            .iter()
            .map(|_| cx.new(|cx| ColorPickerState::new(window, cx)))
            .collect();
        for (idx, picker) in tab_color_pickers.iter().enumerate() {
            let slot = TabColorSlot::ALL[idx];
            cx.subscribe(picker, move |this, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(Some(hsla)) = event else {
                    return;
                };
                slot.write(
                    &mut this.config.appearance,
                    Some(crate::tab_colors::hsla_to_hex(*hsla)),
                );
                cx.emit(AppearancePreview);
                cx.notify();
            })
            .detach();
        }
        cx.subscribe(
            &background_image_opacity_slider,
            |this, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.config.appearance.background_image_opacity =
                        Self::clamp_background_image_opacity(value.end());
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &terminal_font_select,
            window,
            |this, _, ev: &SelectEvent<SearchableVec<FontChoice>>, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = ev {
                    this.config.terminal.font_family = sanitize_terminal_font_family(value);
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &ui_font_select,
            window,
            |this, _, ev: &SelectEvent<SearchableVec<FontChoice>>, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = ev {
                    this.config.appearance.ui_font_family = value.clone();
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &cursor_style_select,
            window,
            |this, _, ev: &SelectEvent<Vec<String>>, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = ev {
                    this.config.terminal.cursor_style =
                        Self::cursor_style_from_label(value).to_string();
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &background_image_position_select,
            window,
            |this, _, ev: &SelectEvent<Vec<String>>, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = ev {
                    this.config.appearance.background_image_position = value.clone();
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &background_image_fit_select,
            window,
            |this, _, ev: &SelectEvent<Vec<String>>, _, cx| {
                if let SelectEvent::Confirm(Some(value)) = ev {
                    this.config.appearance.background_image_fit = value.clone();
                    cx.emit(AppearancePreview);
                    cx.notify();
                }
            },
        )
        .detach();

        Self {
            visible: false,
            all_themes: vu_terminal::TerminalTheme::all_available(),
            standalone: false,
            config: config.clone(),
            preview_snapshot: None,
            focus_handle: cx.focus_handle(),
            active_section: SettingsSection::General,
            overlay_motion: MotionValue::new(0.0),
            terminal_font_select,
            ui_font_select,
            cursor_style_select,
            font_size_input,
            ui_font_size_input,
            terminal_opacity_slider,
            icon_scale_slider,
            line_height_slider,
            letter_spacing_slider,
            minimum_contrast_slider,
            unfocused_split_slider,
            window_padding_slider,
            chrome_surface_slider,
            chrome_border_slider,
            terminal_blur: Self::effective_terminal_blur(config.appearance.terminal_blur),
            ui_opacity_slider,
            tab_accent_inactive_alpha_slider,
            tab_accent_inactive_hover_alpha_slider,
            tab_inactive_opacity_slider,
            tab_close_size_slider,
            tab_color_pickers,
            background_image_input,
            background_image_opacity_slider,
            background_image_position_select,
            background_image_fit_select,
            background_image_repeat: config.appearance.background_image_repeat,
            hide_pane_title_bar: config.appearance.hide_pane_title_bar,
            save_error: None,
            save_error_kind: None,
            last_saved_at: std::fs::metadata(Config::config_path())
                .and_then(|m| m.modified())
                .ok(),
            close_confirmation_visible: false,
            custom_theme_name_input,
            custom_theme_preview: None,
            custom_theme_status: None,
            theme_editor: None,
            theme_editor_slot: None,
            theme_editor_pickers,
            theme_editor_name_input,
            theme_editor_status: None,
            theme_editor_original: None,
            collapsed_groups: HashSet::new(),
            recording_key: None,
            #[cfg(target_os = "macos")]
            recording_resume_keybindings: None,
            http_proxy_input,
            https_proxy_input,
            new_tab_directory_input,
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.standalone = false;
        self.visible = !self.visible;
        self.overlay_motion.set_target(
            if self.visible { 1.0 } else { 0.0 },
            std::time::Duration::from_millis(if self.visible { 220 } else { 180 }),
        );
        if self.visible {
            self.all_themes = vu_terminal::TerminalTheme::all_available();
            self.refresh_controls_from_config(window, cx);
        } else {
            // Ensure hotkeys are always re-enabled when the panel closes,
            // even if recording was active when the user dismissed it.
            self.set_recording_key(None);
        }
        cx.emit(VisibilityChanged);
        cx.notify();
    }

    pub fn open_standalone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.standalone = true;
        self.visible = true;
        self.preview_snapshot = Some(self.config.clone());
        self.close_confirmation_visible = false;
        self.overlay_motion
            .set_target(1.0, std::time::Duration::ZERO);
        self.refresh_controls_from_config(window, cx);
        cx.emit(VisibilityChanged);
        cx.notify();
    }

    pub fn revert_standalone_preview(&mut self, cx: &mut Context<Self>) {
        if !self.standalone {
            return;
        }
        self.close_confirmation_visible = false;
        self.set_recording_key(None);
        if let Some(snapshot) = self.preview_snapshot.take() {
            self.config = snapshot;
            cx.emit(AppearancePreview);
            cx.notify();
        }
    }

    fn refresh_controls_from_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_font_select.update(cx, |select, cx| {
            select.set_selected_value(
                &sanitize_terminal_font_family(&self.config.terminal.font_family),
                window,
                cx,
            );
        });
        self.ui_font_select.update(cx, |select, cx| {
            select.set_selected_value(&self.config.appearance.ui_font_family, window, cx);
        });
        self.cursor_style_select.update(cx, |select, cx| {
            select.set_selected_value(
                &Self::cursor_style_label(&self.config.terminal.cursor_style).to_string(),
                window,
                cx,
            );
        });
        self.font_size_input.update(cx, |s, cx| {
            s.set_value(&self.config.terminal.font_size.to_string(), window, cx)
        });
        self.ui_font_size_input.update(cx, |s, cx| {
            s.set_value(
                &Self::clamp_ui_font_size(self.config.appearance.ui_font_size).to_string(),
                window,
                cx,
            )
        });
        self.icon_scale_slider.update(cx, |slider, cx| {
            slider.set_value(
                Self::clamp_icon_scale(self.config.appearance.icon_scale),
                window,
                cx,
            );
        });
        crate::ui_scale::set_icon_scale(self.config.appearance.icon_scale);
        self.terminal_opacity_slider.update(cx, |slider, cx| {
            slider.set_value(
                Self::clamp_terminal_opacity(self.config.appearance.terminal_opacity),
                window,
                cx,
            );
        });
        self.terminal_blur = Self::effective_terminal_blur(self.config.appearance.terminal_blur);
        self.config.appearance.terminal_blur = self.terminal_blur;
        self.ui_opacity_slider.update(cx, |slider, cx| {
            slider.set_value(
                Self::clamp_ui_opacity(self.config.appearance.ui_opacity),
                window,
                cx,
            );
        });
        self.tab_accent_inactive_alpha_slider
            .update(cx, |slider, cx| {
                slider.set_value(
                    Self::clamp_tab_accent_inactive_alpha(
                        self.config.appearance.tab_accent_inactive_alpha,
                    ),
                    window,
                    cx,
                );
            });
        self.tab_accent_inactive_hover_alpha_slider
            .update(cx, |slider, cx| {
                slider.set_value(
                    Self::clamp_tab_accent_inactive_hover_alpha(
                        self.config.appearance.tab_accent_inactive_hover_alpha,
                        Self::clamp_tab_accent_inactive_alpha(
                            self.config.appearance.tab_accent_inactive_alpha,
                        ),
                    ),
                    window,
                    cx,
                );
            });
        self.tab_inactive_opacity_slider.update(cx, |slider, cx| {
            slider.set_value(
                Self::clamp_tab_inactive_opacity(self.config.appearance.tab_inactive_opacity),
                window,
                cx,
            );
        });
        self.tab_close_size_slider.update(cx, |slider, cx| {
            slider.set_value(
                Self::clamp_tab_close_size(self.config.appearance.tab_close_size),
                window,
                cx,
            );
        });
        self.sync_tab_color_pickers(window, cx);
        self.background_image_input.update(cx, |s, cx| {
            s.set_value(
                &self
                    .config
                    .appearance
                    .background_image
                    .clone()
                    .unwrap_or_default(),
                window,
                cx,
            );
        });
        self.background_image_opacity_slider
            .update(cx, |slider, cx| {
                slider.set_value(
                    Self::clamp_background_image_opacity(
                        self.config.appearance.background_image_opacity,
                    ),
                    window,
                    cx,
                );
            });
        self.background_image_position_select
            .update(cx, |select, cx| {
                select.set_selected_value(
                    &self.config.appearance.background_image_position,
                    window,
                    cx,
                );
            });
        self.background_image_fit_select.update(cx, |select, cx| {
            select.set_selected_value(&self.config.appearance.background_image_fit, window, cx);
        });
        self.background_image_repeat = self.config.appearance.background_image_repeat;
        self.hide_pane_title_bar = self.config.appearance.hide_pane_title_bar;
        self.set_recording_key(None);
        // Network / proxy — repopulate so reopening the panel shows current values.
        self.http_proxy_input.update(cx, |s, cx| {
            s.set_value(
                &self.config.network.http_proxy.clone().unwrap_or_default(),
                window,
                cx,
            )
        });
        self.https_proxy_input.update(cx, |s, cx| {
            s.set_value(
                &self.config.network.https_proxy.clone().unwrap_or_default(),
                window,
                cx,
            )
        });
        self.new_tab_directory_input.update(cx, |s, cx| {
            s.set_value(&self.config.terminal.new_tab_directory, window, cx)
        });
        self.focus_handle.focus(window, cx);
    }

    /// Parse a ghostty config from clipboard and show a live preview.
    fn paste_theme_from_clipboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = match cx
            .read_from_clipboard()
            .and_then(|c| c.text().map(|s| s.to_string()))
        {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                self.custom_theme_status =
                    Some("Error: clipboard is empty. Copy a Ghostty theme first.".into());
                cx.notify();
                return;
            }
        };

        // Get theme name from input, fallback to "custom"
        let name_raw = self.custom_theme_name_input.read(cx).value().to_string();
        let name = if name_raw.trim().is_empty() {
            "custom".to_string()
        } else {
            name_raw.trim().to_lowercase().replace(' ', "-")
        };

        match vu_terminal::TerminalTheme::from_ghostty_format(&name, &text) {
            Some(theme) => {
                self.custom_theme_status = Some(format!(
                    "Loaded \"{}\" into the editor. Adjust any slot, then Save & Apply.",
                    display_theme_name(&name)
                ));
                self.custom_theme_preview = Some(theme.clone());
                // Drop the import straight into the editor rather than leaving
                // it as a preview the user has to separately apply and reopen.
                self.load_theme_into_editor(theme, window, cx);
            }
            None => {
                self.custom_theme_status = Some(
                    "Error: couldn't read a Ghostty theme. Include background, foreground, and palette entries.".into()
                );
                self.custom_theme_preview = None;
            }
        }
        cx.notify();
    }

    fn browse_background_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a background image".into()),
        });

        let input = self.background_image_input.clone();
        cx.spawn_in(window, async move |this, window| {
            let path = paths.await.ok()?.ok()??.into_iter().next()?;
            let path_text = path.to_string_lossy().to_string();

            window
                .update(|window, cx| {
                    _ = input.update(cx, |state, cx| {
                        state.set_value(&path_text, window, cx);
                    });
                    _ = this.update(cx, |panel, cx| {
                        panel.config.appearance.background_image = Some(path_text.clone());
                        cx.emit(AppearancePreview);
                        cx.notify();
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    /// A settings group with a clickable heading that collapses its card.
    ///
    /// Replaces the bare `group_label` + card pairs. The heading stays visible
    /// when collapsed, so collapsing hides bulk without hiding that the group
    /// exists.
    fn group(
        &self,
        id: &'static str,
        label: &str,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Div {
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;
        let open = !self.collapsed_groups.contains(id);
        let heading = div()
            .id(SharedString::from(format!("group-{id}")))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .px(px(6.0))
            .py(px(5.0))
            .rounded(px(5.0))
            .cursor_pointer()
            .hover(|s| s.bg(theme.muted.opacity(0.10)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if !this.collapsed_groups.remove(id) {
                        this.collapsed_groups.insert(id);
                    }
                    cx.notify();
                }),
            )
            .child(
                svg()
                    .path(if open {
                        "phosphor/caret-down.svg"
                    } else {
                        "phosphor/caret-right.svg"
                    })
                    .size(px(12.0))
                    .text_color(theme.muted_foreground.opacity(0.85)),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground.opacity(0.85))
                    .child(label.to_string()),
            );

        div().flex().flex_col().gap(px(8.0)).child(
            Collapsible::new()
                .open(open)
                .child(heading)
                .content(content),
        )
    }

    /// Seed the palette editor from whichever theme is currently active.
    fn open_theme_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let base =
            vu_terminal::TerminalTheme::by_name(&self.config.terminal.theme).unwrap_or_default();
        self.load_theme_into_editor(base, window, cx);
    }

    /// Seed the editor and every picker from `theme`. Used both by Customize and
    /// by a clipboard import, so an imported theme lands somewhere editable.
    fn load_theme_into_editor(
        &mut self,
        theme: vu_terminal::TerminalTheme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let suggested = if theme.name.ends_with("-custom") {
            theme.name.clone()
        } else {
            format!("{}-custom", theme.name)
        };
        self.theme_editor_name_input.update(cx, |s, cx| {
            s.set_value(&suggested, window, cx);
        });
        for (idx, spec) in THEME_SLOTS.iter().enumerate() {
            let hsla = color_to_hsla(spec.read(&theme));
            if let Some(picker) = self.theme_editor_pickers.get(idx) {
                picker.update(cx, |state, cx| state.set_value(hsla, window, cx));
            }
        }
        if self.theme_editor_original.is_none() {
            self.theme_editor_original = Some(self.config.terminal.theme.clone());
        }
        // Show the import on the real window immediately, same as an edit.
        cx.emit(ThemeLivePreview(theme.clone()));
        self.theme_editor = Some(theme);
        self.theme_editor_slot = None;
        self.theme_editor_status = None;
        cx.notify();
    }

    /// Close the editor. Unsaved edits are dropped and the previous theme is
    /// restored, since every edit was already pushed to the live window.
    fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        let saved = self
            .theme_editor_status
            .as_ref()
            .is_some_and(|s| s.starts_with("Saved"));
        if let (false, Some(original)) = (saved, self.theme_editor_original.take()) {
            cx.emit(ThemePreview(original));
        }
        self.theme_editor = None;
        self.theme_editor_slot = None;
        self.theme_editor_original = None;
        self.theme_editor_status = None;
        cx.notify();
    }

    /// Load a slot into the editor's top picker, from a preview click, and open
    /// the picker so one click on the coloured text is enough.
    fn select_theme_slot(&mut self, slot: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let deselecting = self.theme_editor_slot == Some(slot);

        // Close whatever was open first, or two popovers fight over the anchor.
        if let Some(previous) = self.theme_editor_slot
            && let Some(picker) = self.theme_editor_pickers.get(previous)
        {
            picker.update(cx, |state, cx| state.set_open(false, cx));
        }

        self.theme_editor_slot = if deselecting { None } else { Some(slot) };

        if !deselecting && let Some(picker) = self.theme_editor_pickers.get(slot) {
            picker.update(cx, |state, cx| state.set_open(true, cx));
        }
        cx.notify();
    }

    /// Write a picked color into the working theme and push it to the window.
    fn set_theme_slot(&mut self, slot: usize, hsla: Hsla, cx: &mut Context<Self>) {
        let Some(theme) = self.theme_editor.as_mut() else {
            return;
        };
        let Some(spec) = THEME_SLOTS.get(slot) else {
            return;
        };
        spec.write(theme, hsla_to_color(hsla));
        let live = theme.clone();
        self.theme_editor_status = None;
        cx.emit(ThemeLivePreview(live));
        cx.notify();
    }

    /// Copy the working palette as a Ghostty theme file, so it can be pasted
    /// into another machine's editor or committed somewhere.
    fn copy_theme_to_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(theme) = self.theme_editor.as_ref() else {
            return;
        };
        let name = self
            .theme_editor_name_input
            .read(cx)
            .value()
            .trim()
            .to_lowercase()
            .replace(' ', "-");
        let mut out = theme.clone();
        if !name.is_empty() {
            out.name = name;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(out.to_ghostty_format()));
        self.theme_editor_status = Some(format!("Copied \"{}\" to the clipboard.", out.name));
        cx.notify();
    }

    /// Write the edited palette to the user themes directory and activate it.
    fn save_theme_editor(&mut self, cx: &mut Context<Self>) {
        let Some(theme) = self.theme_editor.as_ref() else {
            return;
        };
        let name = self
            .theme_editor_name_input
            .read(cx)
            .value()
            .trim()
            .to_lowercase()
            .replace(' ', "-");
        if name.is_empty() {
            self.theme_editor_status = Some("Name the theme before saving.".into());
            cx.notify();
            return;
        }
        // Built-in names resolve from Rust before the user themes directory is
        // consulted, so saving over one would write a file that never loads.
        if vu_terminal::TerminalTheme::available().contains(&name.as_str()) {
            self.theme_editor_status = Some(format!(
                "\"{name}\" is a built-in theme. Pick another name."
            ));
            cx.notify();
            return;
        }

        let mut saved = theme.clone();
        saved.name = name.clone();

        let dir = vu_terminal::TerminalTheme::user_themes_dir();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.theme_editor_status = Some(format!("Error: {e}"));
            cx.notify();
            return;
        }
        if let Err(e) = std::fs::write(dir.join(&name), saved.to_ghostty_format()) {
            self.theme_editor_status = Some(format!("Error: {e}"));
            cx.notify();
            return;
        }

        self.config.terminal.theme = name.clone();
        self.all_themes = vu_terminal::TerminalTheme::all_available();
        cx.emit(ThemePreview(name.clone()));
        self.theme_editor_status = Some(format!("Saved and applied: {name}"));
        cx.notify();
    }

    /// Save the custom theme to the user themes directory and apply it.
    fn apply_custom_theme(&mut self, cx: &mut Context<Self>) {
        let preview = match &self.custom_theme_preview {
            Some(t) => t.clone(),
            None => return,
        };

        let dir = vu_terminal::TerminalTheme::user_themes_dir();

        // Create directory if needed
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.custom_theme_status = Some(format!("Error: {}", e));
            cx.notify();
            return;
        }

        // Read the config text from clipboard again for saving
        let text = match cx
            .read_from_clipboard()
            .and_then(|c| c.text().map(|s| s.to_string()))
        {
            Some(t) if !t.trim().is_empty() => t,
            _ => {
                self.custom_theme_status =
                    Some("Error: the clipboard changed before save. Load the theme again.".into());
                cx.notify();
                return;
            }
        };

        let file_path = dir.join(&preview.name);
        if let Err(e) = std::fs::write(&file_path, &text) {
            self.custom_theme_status = Some(format!("Error: {}", e));
            cx.notify();
            return;
        }

        // Apply the theme
        self.config.terminal.theme = preview.name.clone();
        cx.emit(ThemePreview(preview.name.clone()));
        self.custom_theme_status = Some(format!("Saved and applied: {}", file_path.display()));
        self.custom_theme_preview = None;
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible || self.overlay_motion.is_animating()
    }

    pub fn is_overlay_visible(&self) -> bool {
        !self.standalone && self.is_visible()
    }

    fn sync_config_from_controls(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let font_size_text = self.font_size_input.read(cx).value().to_string();
        let ui_font_size_text = self.ui_font_size_input.read(cx).value().trim().to_string();

        self.config.terminal.font_family =
            sanitize_terminal_font_family(&self.config.terminal.font_family);
        self.config.terminal.font_size = font_size_text.parse().unwrap_or(14.0);
        let parsed_ui_font_size = if ui_font_size_text.is_empty() {
            Some(self.config.appearance.ui_font_size)
        } else {
            ui_font_size_text.parse::<f32>().ok()
        };
        let Some(parsed_ui_font_size) = parsed_ui_font_size else {
            return Err(format!(
                "UI Size must be a number between {:.1} and {:.1}.",
                MIN_UI_FONT_SIZE, MAX_UI_FONT_SIZE
            ));
        };
        self.config.appearance.ui_font_size = Self::clamp_ui_font_size(parsed_ui_font_size);
        self.config.appearance.icon_scale =
            Self::clamp_icon_scale(self.icon_scale_slider.read(cx).value().end());
        self.config.appearance.terminal_opacity =
            Self::clamp_terminal_opacity(self.terminal_opacity_slider.read(cx).value().end());
        self.config.appearance.terminal_blur = Self::effective_terminal_blur(self.terminal_blur);
        self.terminal_blur = self.config.appearance.terminal_blur;
        self.config.appearance.ui_opacity =
            Self::clamp_ui_opacity(self.ui_opacity_slider.read(cx).value().end());
        self.config.appearance.tab_accent_inactive_alpha = Self::clamp_tab_accent_inactive_alpha(
            self.tab_accent_inactive_alpha_slider.read(cx).value().end(),
        );
        self.config.appearance.tab_accent_inactive_hover_alpha =
            Self::clamp_tab_accent_inactive_hover_alpha(
                self.tab_accent_inactive_hover_alpha_slider
                    .read(cx)
                    .value()
                    .end(),
                self.config.appearance.tab_accent_inactive_alpha,
            );
        self.config.appearance.tab_inactive_opacity = Self::clamp_tab_inactive_opacity(
            self.tab_inactive_opacity_slider.read(cx).value().end(),
        );
        self.config.appearance.tab_close_size =
            Self::clamp_tab_close_size(self.tab_close_size_slider.read(cx).value().end());
        let background_image_text = self
            .background_image_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.config.appearance.background_image = if background_image_text.is_empty() {
            None
        } else {
            Some(background_image_text)
        };
        self.config.appearance.background_image_opacity = Self::clamp_background_image_opacity(
            self.background_image_opacity_slider.read(cx).value().end(),
        );
        self.config.appearance.background_image_repeat = self.background_image_repeat;

        // Network / proxy
        // Blank field → None (leave inherited env untouched).
        // Non-empty   → Some(value) (override or clear on next startup).
        let http_proxy_text = self.http_proxy_input.read(cx).value().trim().to_string();
        let https_proxy_text = self.https_proxy_input.read(cx).value().trim().to_string();
        self.config.network.http_proxy = if http_proxy_text.is_empty() {
            None
        } else {
            Some(http_proxy_text)
        };
        self.config.network.https_proxy = if https_proxy_text.is_empty() {
            None
        } else {
            Some(https_proxy_text)
        };

        let new_tab_directory_text = self
            .new_tab_directory_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        self.config.terminal.new_tab_directory = if new_tab_directory_text.is_empty() {
            "inherit".to_string()
        } else {
            new_tab_directory_text
        };

        Ok(())
    }

    fn config_matches(a: &Config, b: &Config) -> bool {
        a == b
    }

    fn has_unsaved_changes(&mut self, cx: &mut Context<Self>) -> bool {
        if self.preview_snapshot.is_none() {
            return false;
        }

        match self.sync_config_from_controls(cx) {
            Ok(()) => !Self::config_matches(self.preview_snapshot.as_ref().unwrap(), &self.config),
            Err(_) => true,
        }
    }

    pub fn request_standalone_close(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.standalone {
            return true;
        }

        if !self.has_unsaved_changes(cx) {
            self.close_confirmation_visible = false;
            self.revert_standalone_preview(cx);
            return true;
        }

        self.close_confirmation_visible = true;
        cx.notify();
        false
    }

    fn keep_editing_after_close_prompt(&mut self, cx: &mut Context<Self>) {
        self.close_confirmation_visible = false;
        cx.notify();
    }

    fn save_and_close_standalone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save(window, cx);
        if self.save_error.is_none() {
            self.close_confirmation_visible = false;
            window.remove_window();
        } else {
            self.close_confirmation_visible = true;
            cx.notify();
        }
    }

    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_recording_key(None);

        if let Err(message) = self.sync_config_from_controls(cx) {
            self.save_error = Some(message);
            self.save_error_kind = Some(SettingsSaveErrorKind::Other);
            cx.notify();
            return;
        }

        // Keybindings are updated directly via record_keystroke — no reading needed
        if let Some(message) = keybinding_conflict_message(&self.config.keybindings) {
            self.save_error = Some(message);
            self.save_error_kind = Some(SettingsSaveErrorKind::KeybindingConflict);
            cx.notify();
            return;
        }

        match self.persist_config() {
            Ok(()) => {
                self.save_error = None;
                self.save_error_kind = None;
                self.last_saved_at = Some(std::time::SystemTime::now());
                self.preview_snapshot = Some(self.config.clone());
                if !self.standalone {
                    self.visible = false;
                    self.overlay_motion
                        .set_target(0.0, std::time::Duration::from_millis(180));
                    cx.emit(VisibilityChanged);
                }
                cx.emit(SaveSettings);
            }
            Err(e) => {
                log::error!("Failed to save config: {}", e);
                self.save_error = Some(e.to_string());
                self.save_error_kind = Some(SettingsSaveErrorKind::Other);
            }
        }
        cx.notify();
    }

    /// Record a keystroke for the binding currently being recorded.

    fn set_recording_key(&mut self, key: Option<String>) {
        #[cfg(target_os = "macos")]
        let was_recording = self.recording_key.is_some();
        #[cfg(target_os = "macos")]
        let will_record = key.is_some();
        self.recording_key = key;

        #[cfg(target_os = "macos")]
        match (was_recording, will_record) {
            (false, true) => {
                let keybindings = vu_core::Config::load()
                    .map(|config| config.keybindings)
                    .unwrap_or_else(|err| {
                        log::warn!(
                            "settings: failed to load persisted config before hotkey recording: {err}"
                        );
                        self.config.keybindings.clone()
                    });
                self.recording_resume_keybindings = Some(keybindings.clone());
                crate::global_hotkey::suspend_global_hotkeys(&keybindings);
            }
            (true, false) => {
                if let Some(keybindings) = self.recording_resume_keybindings.take() {
                    crate::global_hotkey::resume_global_hotkeys(&keybindings);
                } else {
                    log::warn!("settings: hotkey recording ended without saved resume keybindings");
                }
            }
            _ => {}
        }
    }

    fn record_keystroke(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let field = match &self.recording_key {
            Some(f) => f.clone(),
            None => return,
        };

        // Don't record bare modifier keys or escape (used to cancel)
        let key = &keystroke.key;
        if matches!(
            key.as_str(),
            "shift" | "control" | "alt" | "meta" | "fn" | "escape"
        ) {
            if key == "escape" {
                self.set_recording_key(None);
                cx.notify();
            }
            return;
        }

        // Build GPUI binding format: cmd-shift-k
        let binding = keystroke_to_binding(keystroke);

        // Write directly into config
        match field.as_str() {
            "global_summon" => self.config.keybindings.global_summon = binding,
            "new_window" => self.config.keybindings.new_window = binding,
            "new_tab" => self.config.keybindings.new_tab = binding,
            "close_tab" => self.config.keybindings.close_tab = binding,
            "close_pane" => self.config.keybindings.close_pane = binding,
            "toggle_pane_zoom" => self.config.keybindings.toggle_pane_zoom = binding,
            "focus_next_pane" => self.config.keybindings.focus_next_pane = binding,
            "focus_previous_pane" => self.config.keybindings.focus_previous_pane = binding,
            "next_tab" => self.config.keybindings.next_tab = binding,
            "previous_tab" => self.config.keybindings.previous_tab = binding,
            "settings" => self.config.keybindings.settings = binding,
            "command_palette" => self.config.keybindings.command_palette = binding,
            "toggle_input_bar" => self.config.keybindings.toggle_input_bar = binding,
            "focus_input" => self.config.keybindings.focus_input = binding,
            "split_right" => self.config.keybindings.split_right = binding,
            "split_down" => self.config.keybindings.split_down = binding,
            "toggle_pane_scope" => self.config.keybindings.toggle_pane_scope = binding,
            "toggle_left_panel" => self.config.keybindings.toggle_left_panel = binding,
            "focus_files" => self.config.keybindings.focus_files = binding,
            "search_files" => self.config.keybindings.search_files = binding,
            "collapse_sidebar" => self.config.keybindings.collapse_sidebar = binding,
            "new_surface" => self.config.keybindings.new_surface = binding,
            "new_surface_split_right" => self.config.keybindings.new_surface_split_right = binding,
            "new_surface_split_down" => self.config.keybindings.new_surface_split_down = binding,
            "next_surface" => self.config.keybindings.next_surface = binding,
            "previous_surface" => self.config.keybindings.previous_surface = binding,
            "rename_surface" => self.config.keybindings.rename_surface = binding,
            "close_surface" => self.config.keybindings.close_surface = binding,
            "quit" => self.config.keybindings.quit = binding,
            _ => {}
        }
        self.set_recording_key(None);
        sync_keybinding_conflict_error(
            &mut self.save_error,
            &mut self.save_error_kind,
            &self.config.keybindings,
        );
        cx.notify();
    }

    /// Get the current value of a keybinding by field name.
    fn binding_value(&self, field: &str) -> &str {
        match field {
            "global_summon" => &self.config.keybindings.global_summon,
            "new_window" => &self.config.keybindings.new_window,
            "new_tab" => &self.config.keybindings.new_tab,
            "close_tab" => &self.config.keybindings.close_tab,
            "close_pane" => &self.config.keybindings.close_pane,
            "toggle_pane_zoom" => &self.config.keybindings.toggle_pane_zoom,
            "focus_next_pane" => &self.config.keybindings.focus_next_pane,
            "focus_previous_pane" => &self.config.keybindings.focus_previous_pane,
            "next_tab" => &self.config.keybindings.next_tab,
            "previous_tab" => &self.config.keybindings.previous_tab,
            "settings" => &self.config.keybindings.settings,
            "command_palette" => &self.config.keybindings.command_palette,
            "toggle_input_bar" => &self.config.keybindings.toggle_input_bar,
            "focus_input" => &self.config.keybindings.focus_input,
            "split_right" => &self.config.keybindings.split_right,
            "split_down" => &self.config.keybindings.split_down,
            "toggle_pane_scope" => &self.config.keybindings.toggle_pane_scope,
            "toggle_left_panel" => &self.config.keybindings.toggle_left_panel,
            "focus_files" => &self.config.keybindings.focus_files,
            "search_files" => &self.config.keybindings.search_files,
            "collapse_sidebar" => &self.config.keybindings.collapse_sidebar,
            "new_surface" => &self.config.keybindings.new_surface,
            "new_surface_split_right" => &self.config.keybindings.new_surface_split_right,
            "new_surface_split_down" => &self.config.keybindings.new_surface_split_down,
            "next_surface" => &self.config.keybindings.next_surface,
            "previous_surface" => &self.config.keybindings.previous_surface,
            "rename_surface" => &self.config.keybindings.rename_surface,
            "close_surface" => &self.config.keybindings.close_surface,
            "quit" => &self.config.keybindings.quit,
            _ => "",
        }
    }

    /// Updates the settings draft only. Normal Settings edits persist through
    /// the Save button; callers that already persisted this value should use
    /// `set_persisted_restore_terminal_text` to keep close/revert semantics
    /// aligned with disk.
    pub fn set_restore_terminal_text(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.preview_snapshot.is_none() {
            self.preview_snapshot = Some(self.config.clone());
        }
        self.config.appearance.restore_terminal_text = enabled;
        cx.notify();
    }

    pub fn set_persisted_restore_terminal_text(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.config.appearance.restore_terminal_text = enabled;
        if let Some(snapshot) = &mut self.preview_snapshot {
            snapshot.appearance.restore_terminal_text = enabled;
        } else {
            self.preview_snapshot = Some(self.config.clone());
        }
        cx.notify();
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn terminal_config(&self) -> &vu_core::config::TerminalConfig {
        &self.config.terminal
    }
    pub fn appearance_config(&self) -> &vu_core::config::AppearanceConfig {
        &self.config.appearance
    }
    fn persist_config(&self) -> anyhow::Result<()> {
        self.config.save()
    }

    // ── Section content ──────────────────────────────────────────

    fn render_general(&mut self, cx: &mut Context<Self>) -> Div {
        let card_opacity = self.card_opacity();

        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;

        // Build the Updates card (only shown for channels that poll).
        let channel = vu_core::release_channel::current();
        // On any target outside macOS / Windows / Linux we have no
        // update backend, so skip the card even if the channel
        // otherwise would poll.
        #[cfg_attr(
            all(
                not(target_os = "macos"),
                not(target_os = "windows"),
                not(target_os = "linux")
            ),
            allow(unused_variables)
        )]
        let show_updates = channel.polls_for_updates();

        #[cfg_attr(
            all(
                not(target_os = "macos"),
                not(target_os = "windows"),
                not(target_os = "linux")
            ),
            allow(unused_mut)
        )]
        let mut container = section_content(
            "General",
            "Terminal defaults and shared app behavior.",
            theme,
        );

        #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
        if show_updates {
            let updater_status = crate::updater::status();
            let latest_state = crate::updater::latest_check();

            container = container.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.group("updates", "Updates", card(theme, card_opacity)
                            .child(
                                div()
                                    .flex()
                                    .px(px(16.0))
                                    .py(px(14.0))
                                    .flex_col()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_start()
                                            .justify_between()
                                            .gap(px(16.0))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(8.0))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(
                                                                theme
                                                                    .muted_foreground
                                                                    .opacity(0.5),
                                                            )
                                                            .child("Channel"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(14.0))
                                                            .line_height(px(20.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .child(channel.display_name()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .items_end()
                                                    .gap(px(8.0))
                                                    .child(
                                                        div()
                                                            .text_size(px(11.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(
                                                                theme
                                                                    .muted_foreground
                                                                    .opacity(0.5),
                                                            )
                                                            .child("Version"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .line_height(px(20.0))
                                                            .font_family(
                                                                theme.mono_font_family.clone(),
                                                            )
                                                            .text_color(
                                                                theme
                                                                    .muted_foreground
                                                                    .opacity(0.82),
                                                            )
                                                            .child(format!(
                                                                "{} ({})",
                                                                crate::app_display_version(),
                                                                crate::app_build_number()
                                                            )),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .mx(px(16.0))
                                    .h(px(1.0))
                                    .bg(theme.muted.opacity(0.10)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_end()
                                    .justify_between()
                                    .gap(px(16.0))
                                    .px(px(16.0))
                                    .pb(px(14.0))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(3.0))
                                            .max_w(px(420.0))
                                            .child(
                                                div()
                                                    .text_size(px(11.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(
                                                        theme
                                                            .muted_foreground
                                                            .opacity(0.5),
                                                    )
                                                    .child("Status"),
                                            )
                                            .child({
                                                let (summary, detail) =
                                                    update_summary_and_detail(
                                                        &latest_state,
                                                        updater_status,
                                                    );
                                                let mut col = div()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(3.0))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .line_height(px(19.0))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(
                                                                theme.foreground.opacity(0.88),
                                                            )
                                                            .child(summary),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(12.0))
                                                            .line_height(px(17.0))
                                                            .text_color(
                                                                theme
                                                                    .muted_foreground
                                                                    .opacity(0.62),
                                                            )
                                                            .child(detail),
                                                    );
                                                if let Some(url) =
                                                    update_download_url(&latest_state)
                                                {
                                                    let label = match &latest_state {
                                                        crate::updater::CheckState::UpdateAvailable { version, .. } =>
                                                            format!("Download v{version}"),
                                                        _ => "Download release".to_string(),
                                                    };
                                                    col = col.child(
                                                        div().pt(px(4.0)).child(
                                                            gpui_component::link::Link::new(
                                                                "update-download-link",
                                                            )
                                                            .href(url)
                                                            .text_size(px(12.5))
                                                            .child(label),
                                                        ),
                                                    );
                                                }
                                                col
                                            }),
                                    )
                                    .child({
                                        let actions = div().flex().items_center().gap(px(6.0));

                                        // The notify-only updater
                                        // (Windows + Linux) shows
                                        // "Update now" when the
                                        // latest state has a fresh
                                        // version. macOS uses
                                        // Sparkle's own dialog
                                        // instead.
                                        #[cfg(any(target_os = "windows", target_os = "linux"))]
                                        let actions = if matches!(
                                            &latest_state,
                                            crate::updater::CheckState::UpdateAvailable { .. }
                                        ) {
                                            actions.child(
                                                Button::new("apply-update")
                                                    .small()
                                                    .primary()
                                                    .label("Update now")
                                                    .on_click(cx.listener(
                                                        |_this, _, _window, _cx| {
                                                            crate::updater::apply_update_in_place();
                                                        },
                                                    )),
                                            )
                                        } else {
                                            actions
                                        };

                                        actions.child(
                                            Button::new("check-updates")
                                                .small()
                                                .ghost()
                                                .disabled(!updater_status.can_check_manually())
                                                .label("Check for Updates")
                                                .on_click(cx.listener(
                                                    |_this, _, _window, _cx| {
                                                        crate::updater::check_for_updates();
                                                    },
                                                )),
                                        )
                                    }),
                            ), cx)),
            );
        }

        container
            // Continuity
            .child(
                div().flex().flex_col().gap(px(8.0)).child(
                    self.group(
                        "continuity",
                        "Continuity",
                        card(theme, card_opacity).child(toggle_row(
                            "Restore Terminal Text",
                            "Keep terminal text on restart continuity.",
                            Switch::new("restore-terminal-text-toggle")
                                .checked(self.config.appearance.restore_terminal_text)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.set_restore_terminal_text(*checked, cx);
                                })),
                            theme,
                        )),
                        cx,
                    ),
                ),
            )
            // New tabs
            .child(
                div().flex().flex_col().gap(px(8.0)).child(
                    self.group(
                        "new-tabs",
                        "New Tabs",
                        card(theme, card_opacity)
                            .child(row_field("Directory", &self.new_tab_directory_input)),
                        cx,
                    ),
                ),
            )
            // Network / proxy
            .child(
                div().flex().flex_col().gap(px(8.0)).child(
                    self.group(
                        "network",
                        "Network",
                        card(theme, card_opacity)
                            .child(row_field("HTTP Proxy", &self.http_proxy_input))
                            .child(row_separator(theme))
                            .child(row_field("HTTPS Proxy", &self.https_proxy_input)),
                        cx,
                    ),
                ),
            )
    }

    fn render_appearance(&self, cx: &mut Context<Self>) -> Div {
        let current_theme = self.config.terminal.theme.clone();
        let terminal_font_select = self.terminal_font_select.clone();
        let ui_font_select = self.ui_font_select.clone();
        let font_size_input = self.font_size_input.clone();
        let ui_font_size_input = self.ui_font_size_input.clone();
        let terminal_opacity_slider = self.terminal_opacity_slider.clone();
        let icon_scale_slider = self.icon_scale_slider.clone();
        let icon_scale = self.icon_scale_value();
        let ui_opacity_slider = self.ui_opacity_slider.clone();
        let tab_accent_inactive_alpha_slider = self.tab_accent_inactive_alpha_slider.clone();
        let tab_accent_inactive_hover_alpha_slider =
            self.tab_accent_inactive_hover_alpha_slider.clone();
        let background_image_input = self.background_image_input.clone();
        let background_image_opacity_slider = self.background_image_opacity_slider.clone();
        let background_image_position_select = self.background_image_position_select.clone();
        let background_image_fit_select = self.background_image_fit_select.clone();
        let terminal_opacity = self.terminal_opacity_value();
        let ui_opacity = self.ui_opacity_value();
        let tab_accent_inactive_alpha = self.tab_accent_inactive_alpha_value();
        let tab_accent_inactive_hover_alpha = self.tab_accent_inactive_hover_alpha_value();
        let tab_inactive_opacity_slider = self.tab_inactive_opacity_slider.clone();
        let tab_close_size_slider = self.tab_close_size_slider.clone();
        let tab_inactive_opacity = self.tab_inactive_opacity_value();
        let tab_close_size = self.tab_close_size_value();
        let background_image_opacity = self.background_image_opacity_value();
        let card_opacity = self.card_opacity();
        let image_repeat_toggle = Switch::new("background-image-repeat")
            .checked(self.background_image_repeat)
            .small()
            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                this.background_image_repeat = *checked;
                this.config.appearance.background_image_repeat = *checked;
                cx.emit(AppearancePreview);
                cx.notify();
            }));
        let all_themes = &self.all_themes;

        // Split into built-in and user themes
        let builtin_names: Vec<&str> = vu_terminal::TerminalTheme::available().to_vec();
        let mut builtin_themes = Vec::new();
        let mut user_themes = Vec::new();
        for t in all_themes.iter() {
            if builtin_names.contains(&t.name.as_str()) {
                builtin_themes.push(t);
            } else {
                user_themes.push(t);
            }
        }
        let has_user_themes = !user_themes.is_empty();
        let total_count = builtin_themes.len() + user_themes.len();

        // Build theme grids first (these need &mut cx for listeners)
        let builtin_grid = self.render_theme_grid(&builtin_themes, &current_theme, cx);
        let user_grid = if has_user_themes {
            Some(self.render_theme_grid(&user_themes, &current_theme, cx))
        } else {
            None
        };
        let theme_editor_card = self.render_theme_editor(card_opacity, cx);

        // Build import section
        let custom_theme_name_input = self.custom_theme_name_input.clone();
        let paste_btn = Button::new("paste-theme-btn")
            .label("Load from Clipboard")
            .icon(Icon::default().path("phosphor/clipboard-text.svg"))
            .small()
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.paste_theme_from_clipboard(window, cx);
            }));
        let browse_background_image_btn = Button::new("browse-background-image")
            .label("Browse…")
            .icon(Icon::default().path("phosphor/folder-open.svg"))
            .small()
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                this.browse_background_image(window, cx);
            }));
        let editor_open = self.theme_editor.is_some();
        let customize_btn = Button::new("theme-customize")
            .label(if editor_open { "Editing" } else { "Customize" })
            .icon(Icon::default().path("phosphor/palette.svg"))
            .small()
            .ghost()
            .on_click(cx.listener(|this, _, window, cx| {
                if this.theme_editor.is_none() {
                    this.open_theme_editor(window, cx);
                }
            }));
        let open_catalog_btn = Button::new("theme-catalog-link")
            .label("Browse Themes")
            .icon(Icon::default().path("phosphor/arrow-square-out.svg"))
            .small()
            .ghost()
            .on_click(cx.listener(|_, _, _, cx| {
                cx.open_url("https://ghostty-style.vercel.app/");
            }));
        let preview_card = self
            .custom_theme_preview
            .as_ref()
            .map(|preview| self.render_single_theme_card(preview, false, cx));

        let preview_actions: Option<AnyElement> = if let Some(card) = preview_card {
            let apply_btn = Button::new("apply-custom-theme")
                .label("Save & Apply")
                .icon(Icon::default().path("phosphor/check.svg"))
                .small()
                .primary()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.apply_custom_theme(cx);
                }));
            let preview_btn = Button::new("preview-custom-theme")
                .label("Preview")
                .icon(Icon::default().path("phosphor/eye.svg"))
                .small()
                .ghost()
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(ref preview) = this.custom_theme_preview {
                        cx.emit(ThemePreview(preview.name.clone()));
                    }
                }));
            Some(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.0))
                    .child(card)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(apply_btn)
                            .child(preview_btn),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        // Now all mutable borrows are done — get theme for pure layout
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;

        let mut import_section = div()
            .px(px(18.0))
            .py(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("Import Theme"),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .text_color(theme.muted_foreground.opacity(0.8))
                    .child("Browse community Ghostty themes, copy, and paste here."),
            )
            // Name input
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(theme.muted_foreground.opacity(0.75))
                            .child("Theme name"),
                    )
                    .child(Input::new(&custom_theme_name_input)),
            )
            // Action buttons — compact row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(paste_btn)
                    .child(open_catalog_btn),
            );

        // Preview card with save/preview actions
        if let Some(preview) = preview_actions {
            import_section = import_section.child(div().pt(px(4.0)).child(preview));
        }

        if let Some(ref status) = self.custom_theme_status {
            import_section = import_section.child(
                div()
                    .px(px(10.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .bg(if status.starts_with("Error") {
                        theme.danger.opacity(0.08)
                    } else {
                        theme.success.opacity(0.08)
                    })
                    .text_size(px(12.5))
                    .line_height(px(19.0))
                    .text_color(if status.starts_with("Error") {
                        theme.danger
                    } else {
                        theme.success
                    })
                    .child(status.clone()),
            );
        }

        let mut theme_card_inner = div()
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .mb(px(12.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Terminal Theme"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(theme.muted_foreground.opacity(0.75))
                            .child(format!("{total_count} themes")),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.muted_foreground.opacity(0.4))
                    .mb(px(10.0))
                    .child("You can also import community-maintained Ghostty styles."),
            )
            .child(builtin_grid);

        // User-installed themes
        if let Some(user_grid) = user_grid {
            theme_card_inner = theme_card_inner.child(
                div()
                    .mt(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .mb(px(10.0))
                            .child(
                                svg()
                                    .path("phosphor/folder.svg")
                                    .size(px(12.0))
                                    .text_color(theme.muted_foreground.opacity(0.75)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.muted_foreground.opacity(0.8))
                                    .child("Installed"),
                            ),
                    )
                    .child(user_grid),
            );
        }

        let mut content = section_content(
            "Appearance",
            "Tweak the terminal's textures, tastes and feels.",
            theme,
        );

        // Theme first: it is what people open Appearance to change, and every
        // group below it only adjusts details of whatever is picked here.
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        // Not a collapsible group: the editor is opened by
                        // Customize and closed by its own Done button, so a
                        // second hide control would just be confusing.
                        .child(group_label("Palette", theme))
                        .child(customize_btn),
                )
                .children(theme_editor_card),
        );
        content = content.child(card(theme, card_opacity).child(theme_card_inner));

        content = content.child(
            div().flex().flex_col().gap(px(8.0)).child(
                self.group(
                    "fonts",
                    "Fonts & Icons",
                    card(theme, card_opacity)
                        .child(searchable_select_row(
                            "Terminal Font",
                            "Terminal and mono UI like code blocks.",
                            &terminal_font_select,
                            "Search fonts…",
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(searchable_select_row(
                            "UI Font",
                            "Settings, prose, and other UI.",
                            &ui_font_select,
                            "Search fonts…",
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(row_field("UI Size", &ui_font_size_input))
                        .child(row_separator(theme))
                        .child(row_field("Terminal Size", &font_size_input))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Icon Size",
                            "Sidebar, tab strip, and pane header icons. Does not move text.",
                            &icon_scale_slider,
                            icon_scale,
                            theme,
                        )),
                    cx,
                ),
            ),
        );

        let tw = self.config.terminal.tweaks.clone();
        let line_height_slider = self.line_height_slider.clone();
        let letter_spacing_slider = self.letter_spacing_slider.clone();
        let minimum_contrast_slider = self.minimum_contrast_slider.clone();
        let unfocused_split_slider = self.unfocused_split_slider.clone();
        let window_padding_slider = self.window_padding_slider.clone();
        let chrome_surface_slider = self.chrome_surface_slider.clone();
        let chrome_border_slider = self.chrome_border_slider.clone();
        let chrome_surface_strength = self.config.appearance.chrome_surface_strength;
        let chrome_border_strength = self.config.appearance.chrome_border_strength;
        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(self.group(
                    "terminal-render",
                    "Terminal Rendering",
                    card(theme, card_opacity)
                        .child(slider_row(
                            "Line Spacing",
                            "Extra height per row, as a percentage of the natural line height.",
                            &line_height_slider,
                            tw.line_height_percent,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Letter Spacing",
                            "Extra width per cell, as a percentage.",
                            &letter_spacing_slider,
                            tw.letter_spacing_percent,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Minimum Contrast",
                            "Force a readability floor between text and its background. 1 leaves colours untouched.",
                            &minimum_contrast_slider,
                            tw.minimum_contrast,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Unfocused Split Dim",
                            "Fade splits that do not have focus. 1 disables the effect.",
                            &unfocused_split_slider,
                            tw.unfocused_split_opacity,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Window Padding",
                            "Space between the terminal grid and the window edge.",
                            &window_padding_slider,
                            tw.window_padding_x,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Ligatures",
                            "Render font ligatures such as -> and != as single glyphs.",
                            Switch::new("tweak-ligatures")
                                .checked(tw.ligatures)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.config.terminal.tweaks.ligatures = *checked;
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Thicken Font",
                            "Synthetic bolding. Helps thin faces on low-DPI displays.",
                            Switch::new("tweak-thicken")
                                .checked(tw.font_thicken)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.config.terminal.tweaks.font_thicken = *checked;
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Blink Cursor",
                            "Blink the terminal cursor.",
                            Switch::new("tweak-blink")
                                .checked(tw.cursor_blink)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.config.terminal.tweaks.cursor_blink = *checked;
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Bold Uses Bright",
                            "Draw bold text in the bright variant of its ANSI colour.",
                            Switch::new("tweak-bold-bright")
                                .checked(tw.bold_is_bright)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.config.terminal.tweaks.bold_is_bright = *checked;
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Hide Mouse While Typing",
                            "Hide the pointer until the mouse moves again.",
                            Switch::new("tweak-mouse-hide")
                                .checked(tw.mouse_hide_while_typing)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.config.terminal.tweaks.mouse_hide_while_typing = *checked;
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        )),
                    cx,
                )),
        );

        content = content.child(div().flex().flex_col().gap(px(8.0)).child(self.group(
            "cursor",
            "Cursor",
            card(theme, card_opacity).child(div().px(px(16.0)).child(select_row(
                "Cursor Style",
                "Choose how the terminal insertion point is drawn.",
                &self.cursor_style_select,
                theme,
            ))),
            cx,
        )));

        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(self.group("transparency", "Transparency", card(theme, card_opacity)
                        .child(slider_row(
                            "Terminal Glass",
                            "How much of the desktop shows through the terminal.",
                            &terminal_opacity_slider,
                            terminal_opacity,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Terminal Blur",
                            if cfg!(target_os = "linux") {
                                "Disabled on Linux until rounded compositor blur regions are available."
                            } else {
                                "Blur the desktop behind transparent terminal surfaces."
                            },
                            {
                                let terminal_blur_supported = Self::terminal_blur_supported();
                                let mut toggle = Switch::new("terminal-blur-toggle")
                                    .checked(Self::effective_terminal_blur(self.terminal_blur))
                                    .small()
                                    .disabled(!terminal_blur_supported);

                                if terminal_blur_supported {
                                    toggle = toggle.on_click(cx.listener(
                                        |this, checked: &bool, _, cx| {
                                            this.terminal_blur = *checked;
                                            this.config.appearance.terminal_blur = *checked;
                                            cx.emit(AppearancePreview);
                                            cx.notify();
                                        },
                                    ));
                                }

                                toggle
                            },
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Window Chrome",
                            "Opacity for tabs, panels, and window controls.",
                            &ui_opacity_slider,
                            ui_opacity,
                            theme,
                        )), cx)),
        );

        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(self.group("tabs-top-bar", "Tabs & Top Bar", card(theme, card_opacity)
                        .child(
                            div()
                                .px(px(12.0))
                                .pt(px(10.0))
                                .text_size(px(12.0))
                                .text_color(theme.muted_foreground.opacity(0.85))
                                .child(
                                    "Surfaces are blended from the terminal background toward \
                                     its foreground; the accent follows the palette's Blue.",
                                ),
                        )
                        .child(toggle_row(
                            "Vertical Tabs",
                            "Use the left sidebar for workspace tabs.",
                            Switch::new("vertical-tabs-toggle")
                                .checked(
                                    self.config.appearance.tabs_orientation
                                        == TabsOrientation::Vertical,
                                )
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    this.config.appearance.tabs_orientation = if *checked {
                                        TabsOrientation::Vertical
                                    } else {
                                        TabsOrientation::Horizontal
                                    };
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(toggle_row(
                            "Hide Pane Title Bar",
                            "Hide the title bar on split panes.",
                            Switch::new("hide-pane-title-bar-toggle")
                                .checked(self.hide_pane_title_bar)
                                .small()
                                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                    let previous = this.config.appearance.hide_pane_title_bar;
                                    this.hide_pane_title_bar = *checked;
                                    this.config.appearance.hide_pane_title_bar = *checked;
                                    if let Err(err) = this.config.save() {
                                        this.hide_pane_title_bar = previous;
                                        this.config.appearance.hide_pane_title_bar = previous;
                                        log::warn!(
                                            "settings: persist hide_pane_title_bar failed: {err}"
                                        );
                                        this.save_error = Some(err.to_string());
                                        this.save_error_kind = Some(SettingsSaveErrorKind::Other);
                                        cx.notify();
                                        return;
                                    }
                                    if let Some(snapshot) = &mut this.preview_snapshot {
                                        snapshot.appearance.hide_pane_title_bar = *checked;
                                    }
                                    this.save_error = None;
                                    this.save_error_kind = None;
                                    cx.emit(AppearancePreview);
                                    cx.notify();
                                })),
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Inactive Tab Visibility",
                            "Surface opacity of inactive tab chips. Raise it to make background tabs read as tabs.",
                            &tab_inactive_opacity_slider,
                            tab_inactive_opacity,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Tab Close Size",
                            "Size of the tab close (X) button in px.",
                            &tab_close_size_slider,
                            tab_close_size,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Inactive Accent",
                            "Accent strength for inactive tabs and unfocused pane titles.",
                            &tab_accent_inactive_alpha_slider,
                            tab_accent_inactive_alpha,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Hover Accent",
                            "Accent strength when hovering inactive tabs.",
                            &tab_accent_inactive_hover_alpha_slider,
                            tab_accent_inactive_hover_alpha,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .children(TabColorSlot::ALL.iter().flat_map(|slot| {
                            [
                                self.render_tab_color_row(*slot, theme, cx),
                                row_separator(theme),
                            ]
                        }))
                        .child(slider_row(
                            "Surface Contrast",
                            "How far the title bar, tab strip, sidebar, and cards sit from the terminal background. 0 makes them match it exactly.",
                            &chrome_surface_slider,
                            chrome_surface_strength,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Border Contrast",
                            "Visibility of borders and dividers between panes and panels.",
                            &chrome_border_slider,
                            chrome_border_strength,
                            theme,
                        )), cx)),
        );

        content = content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(self.group("background-image", "Background Image", card(theme, card_opacity)
                        .child(
                            div()
                                .px(px(16.0))
                                .pt(px(12.0))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.0))
                                                .child(
                                                    div()
                                                        .text_size(px(13.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child("Image Path"),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .line_height(px(18.0))
                                                        .text_color(
                                                            theme.muted_foreground.opacity(0.65),
                                                        )
                                                        .child(
                                                            "Choose a PNG or JPEG. The image is applied per terminal.",
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .child(Input::new(&background_image_input)),
                                                )
                                                .child(browse_background_image_btn),
                                        ),
                                ),
                        )
                        .child(row_separator(theme))
                        .child(
                            div()
                                .px(px(16.0))
                                .child(
                                    select_row(
                                        "Fit",
                                        "Choose how the image fills the terminal.",
                                        &background_image_fit_select,
                                        theme,
                                    ),
                                ),
                        )
                        .child(row_separator(theme))
                        .child(
                            div()
                                .px(px(16.0))
                                .child(
                                    select_row(
                                        "Position",
                                        "Anchor if not filling the full surface.",
                                        &background_image_position_select,
                                        theme,
                                    ),
                                ),
                        )
                        .child(row_separator(theme))
                        .child(
                            toggle_row(
                                "Repeat",
                                "Tile if the fit leaves empty space around it.",
                                image_repeat_toggle,
                                theme,
                            ),
                        )
                        .child(row_separator(theme))
                        .child(slider_row(
                            "Image Strength",
                            "Blend more softly or let come forward.",
                            &background_image_opacity_slider,
                            background_image_opacity,
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(
                            div()
                                .px(px(16.0))
                                .pb(px(12.0))
                                .text_size(px(12.5))
                                .line_height(px(18.0))
                                .text_color(theme.muted_foreground.opacity(0.82))
                                .child(
                                    "Ghostty renders the image per terminal.",
                                ),
                        ), cx)),
        );

        // ── Built-in themes ──
        content = content.child(card(theme, card_opacity).child(import_section));

        content
    }

    /// The palette editor card: live preview, slot list, hex field, save row.
    fn render_theme_editor(&self, card_opacity: f32, cx: &mut Context<Self>) -> Option<Div> {
        let working = self.theme_editor.clone()?;
        // These take &mut cx for their click listeners, so they have to be built
        // before cx.theme() borrows cx immutably.
        let preview = self.render_theme_editor_preview(&working, cx);
        let selected_row = self.render_selected_slot_row(&working, cx);
        let slots = self.render_theme_editor_slots(&working, cx);
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;
        let name_input = self.theme_editor_name_input.clone();

        let save_btn = Button::new("theme-editor-save")
            .label("Save & Apply")
            .icon(Icon::default().path("phosphor/check.svg"))
            .small()
            .primary()
            .on_click(cx.listener(|this, _, _, cx| this.save_theme_editor(cx)));
        let copy_btn = Button::new("theme-editor-copy")
            .label("Copy")
            .icon(Icon::default().path("phosphor/copy.svg"))
            .small()
            .ghost()
            .on_click(cx.listener(|this, _, _, cx| this.copy_theme_to_clipboard(cx)));
        let close_btn = Button::new("theme-editor-close")
            .label("Done")
            .small()
            .ghost()
            .on_click(cx.listener(|this, _, _, cx| this.close_theme_editor(cx)));

        Some(
            card(theme, card_opacity)
                .flex()
                .flex_col()
                .gap(px(12.0))
                .p(px(16.0))
                .children(selected_row)
                .child(preview)
                .child(slots)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_1().min_w_0().child(name_input))
                        .child(copy_btn)
                        .child(save_btn)
                        .child(close_btn),
                )
                .children(self.theme_editor_status.as_ref().map(|status| {
                    div()
                        .text_size(px(12.5))
                        .text_color(if status.starts_with("Saved") {
                            theme.success
                        } else {
                            theme.danger
                        })
                        .child(status.clone())
                })),
        )
    }

    /// Live preview of the working palette, shaped like terminal output so each
    /// slot is shown doing the job its hint describes.
    fn render_theme_editor_preview(
        &self,
        term_theme: &vu_terminal::TerminalTheme,
        cx: &mut Context<Self>,
    ) -> Div {
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;
        let bg = gpui::rgb(term_theme.background.to_u32());
        // Spans carry a THEME_SLOTS index, not a colour, so clicking any run of
        // text knows which slot painted it and can jump straight to its picker.
        let (fg, red, green, yellow, blue, cyan, dim) = (1, 3, 4, 5, 6, 8, 10);

        // (indent, [(text, slot)]) — one screen line each.
        let lines: Vec<(f32, Vec<(&str, usize)>)> = vec![
            (0.0, vec![("~/code/vu", dim), ("  main", dim)]),
            (0.0, vec![("$ ", green), ("cargo test -p vu-terminal", fg)]),
            (0.0, vec![("", fg)]),
            (0.0, vec![("Compiling ", cyan), ("vu-terminal", fg)]),
            (0.0, vec![("", fg)]),
            (
                2.0,
                vec![("test ", cyan), ("palette::mapping ", yellow), ("...", dim)],
            ),
            (2.0, vec![("⎿  running 1 test", dim)]),
            (5.0, vec![("test result: ok. 1 passed", green)]),
            (0.0, vec![("", fg)]),
            (2.0, vec![("diff --git ", cyan), ("src/theme.rs", blue)]),
            (2.0, vec![("⎿  + pub fn to_ghostty_format(&self)", green)]),
            (5.0, vec![("- pub fn legacy_format(&self)", red)]),
            (0.0, vec![("", fg)]),
            (2.0, vec![("warning: unused variable `slot`", yellow)]),
            (2.0, vec![("error: mismatched types", red)]),
        ];

        let mut screen = div()
            .flex()
            .flex_col()
            .w_full()
            .bg(bg)
            .rounded(px(8.0))
            .px(px(14.0))
            .py(px(12.0))
            .gap(px(2.0))
            .font_family(theme.mono_font_family.clone())
            .text_size(px(12.5))
            .line_height(px(18.0));

        let selected = self.theme_editor_slot;
        for (line_idx, (indent, spans)) in lines.into_iter().enumerate() {
            let mut row = div().flex().flex_row().pl(px(indent * 6.0));
            for (span_idx, (text, slot)) in spans.into_iter().enumerate() {
                // Empty spans are blank spacer lines; a zero-width child would
                // collapse the row height, so keep a non-breaking space.
                let is_blank = text.is_empty();
                let text = if is_blank { "\u{00a0}" } else { text };
                let color = THEME_SLOTS
                    .get(slot)
                    .map(|spec| gpui::rgb(spec.read(term_theme).to_u32()))
                    .unwrap_or(bg);
                // Always stateful: .id() changes the type, so branching on it
                // would leave the two arms unable to unify.
                let mut span = div()
                    .id(SharedString::from(format!("preview-{line_idx}-{span_idx}")))
                    .text_color(color)
                    .rounded(px(3.0))
                    .child(text.to_string());
                if selected == Some(slot) && !is_blank {
                    // Keep the preview and the top picker in agreement. Blank
                    // spacer spans carry a slot too, and highlighting them lights
                    // up empty lines.
                    span = span.bg(theme.foreground.opacity(0.16));
                }
                if !is_blank {
                    span = span
                        .cursor_pointer()
                        .hover(|d| d.bg(theme.foreground.opacity(0.10)));
                    span = span.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.select_theme_slot(slot, window, cx);
                        }),
                    );
                }
                row = row.child(span);
            }
            screen = screen.child(row);
        }

        screen
    }

    /// The slot most recently clicked in the preview, hoisted to the top of the
    /// editor with its own picker so you never hunt for it in the list below.
    fn render_selected_slot_row(
        &self,
        term_theme: &vu_terminal::TerminalTheme,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let slot = self.theme_editor_slot?;
        let spec = THEME_SLOTS.get(slot)?;
        let picker_state = self.theme_editor_pickers.get(slot)?;
        let featured: Vec<Hsla> = THEME_SLOTS
            .iter()
            .map(|s| color_to_hsla(s.read(term_theme)))
            .collect();
        let picker = ColorPicker::new(picker_state)
            .featured_colors(featured)
            .anchor(Corner::TopLeft);
        let theme = cx.theme();
        let color = spec.read(term_theme);

        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .bg(theme.primary.opacity(0.10))
                .child(picker)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.foreground)
                                .child(spec.label),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme.muted_foreground)
                                .child(spec.hint),
                        ),
                )
                .child(
                    div()
                        .text_size(px(12.5))
                        .font_family(theme.mono_font_family.clone())
                        .text_color(theme.muted_foreground)
                        .child(color_to_hex(color)),
                ),
        )
    }

    /// The 18 palette slots, each opening a colour picker. The picker carries
    /// its own hex field and HSL sliders, so there is no separate hex row.
    fn render_theme_editor_slots(
        &self,
        term_theme: &vu_terminal::TerminalTheme,
        cx: &mut Context<Self>,
    ) -> Div {
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;
        // Offer the palette's own colours as one-click swatches — most edits
        // are "make this the same green as that", not a fresh hue.
        let featured: Vec<Hsla> = THEME_SLOTS
            .iter()
            .map(|spec| color_to_hsla(spec.read(term_theme)))
            .collect();

        let mut grid = div().flex().flex_col().gap(px(1.0));

        for (idx, spec) in THEME_SLOTS.iter().enumerate() {
            let Some(picker_state) = self.theme_editor_pickers.get(idx) else {
                continue;
            };
            let color = spec.read(term_theme);
            // A ColorPickerState renders one popover per ColorPicker bound to it.
            // The selected slot already has a picker hoisted to the top of the
            // editor, so binding a second one here opened two popovers at once.
            let is_hoisted = self.theme_editor_slot == Some(idx);
            let swatch = if is_hoisted {
                div()
                    .size(px(15.0))
                    .rounded(px(3.0))
                    .flex_shrink_0()
                    .bg(gpui::rgb(color.to_u32()))
                    .into_any_element()
            } else {
                ColorPicker::new(picker_state)
                    .featured_colors(featured.clone())
                    .anchor(Corner::TopLeft)
                    .into_any_element()
            };
            grid = grid.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(6.0))
                    .hover(|s| s.bg(theme.primary.opacity(0.05)))
                    .child(swatch)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.foreground)
                                    .child(spec.label),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme.muted_foreground)
                                    .child(spec.hint),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(6.0))
                            .children(
                                THEME_SLOTS
                                    .iter()
                                    .take(idx)
                                    .position(|other| other.read(term_theme) == color)
                                    .and_then(|dupe| THEME_SLOTS.get(dupe))
                                    .map(|dupe| {
                                        div()
                                            .text_size(px(11.0))
                                            .px(px(5.0))
                                            .py(px(1.0))
                                            .rounded(px(3.0))
                                            .bg(theme.muted.opacity(0.35))
                                            .text_color(theme.muted_foreground)
                                            .child(format!("= {}", dupe.label))
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(theme.muted_foreground)
                                    .child(color_to_hex(color)),
                            ),
                    ),
            );
        }

        grid
    }

    fn render_theme_grid(
        &self,
        themes: &[&vu_terminal::TerminalTheme],
        current_theme: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut grid = div().flex().flex_wrap().gap(px(10.0));
        for term_theme in themes.iter() {
            let is_sel = term_theme.name.as_str() == current_theme;
            grid = grid.child(self.render_single_theme_card(term_theme, is_sel, cx));
        }
        grid
    }

    /// Render a single theme preview card.
    fn render_single_theme_card(
        &self,
        term_theme: &vu_terminal::TerminalTheme,
        is_sel: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;
        let name = term_theme.name.clone();
        let theme_name = name.clone();
        let bg = term_theme.background;
        let fg = term_theme.foreground;
        let bg_gpui = gpui::rgb(bg.to_u32());
        let fg_gpui = gpui::rgb(fg.to_u32());
        let green = gpui::rgb(term_theme.ansi[2].to_u32());
        let cyan = gpui::rgb(term_theme.ansi[6].to_u32());
        let blue = gpui::rgb(term_theme.ansi[4].to_u32());
        let yellow = gpui::rgb(term_theme.ansi[3].to_u32());
        let red = gpui::rgb(term_theme.ansi[1].to_u32());
        let magenta = gpui::rgb(term_theme.ansi[5].to_u32());

        let terminal_preview = div()
            .flex()
            .flex_col()
            .bg(bg_gpui)
            .rounded_t(px(8.0))
            .px(px(8.0))
            .pt(px(6.0))
            .pb(px(6.0))
            .gap(px(1.0))
            .font_family(theme.mono_font_family.clone())
            .text_size(px(9.5))
            .line_height(px(13.5))
            .child(
                div()
                    .flex()
                    .gap(px(3.0))
                    .pb(px(4.0))
                    .child(div().size(px(5.0)).rounded_full().bg(red))
                    .child(div().size(px(5.0)).rounded_full().bg(yellow))
                    .child(div().size(px(5.0)).rounded_full().bg(green)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(3.0))
                    .child(div().text_color(green).child("$"))
                    .child(div().text_color(cyan).child("git"))
                    .child(div().text_color(fg_gpui).child("log --oneline")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(3.0))
                    .child(div().text_color(yellow).child("a1b2c3d"))
                    .child(div().text_color(fg_gpui).child("feat: init")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(3.0))
                    .child(div().text_color(yellow).child("e4f5g6h"))
                    .child(div().text_color(fg_gpui).child("fix: theme")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(3.0))
                    .child(div().text_color(green).child("$"))
                    .child(div().text_color(blue).child("ls"))
                    .child(div().text_color(fg_gpui).child("src/")),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .child(div().text_color(blue).child("lib/"))
                    .child(div().text_color(magenta).child("main.rs"))
                    .child(div().text_color(fg_gpui).child("README")),
            );

        let mut palette_strip = div().flex().h(px(4.0));
        for idx in 0..16 {
            let c = term_theme.ansi[idx];
            palette_strip = palette_strip.child(div().flex_1().h_full().bg(gpui::rgb(c.to_u32())));
        }

        let display_name = display_theme_name(&name);

        div()
            .id(SharedString::from(format!("term-theme-{name}")))
            .cursor_pointer()
            .w(px(150.0))
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .overflow_hidden()
            .bg(if is_sel {
                theme.primary.opacity(0.10)
            } else {
                theme.muted.opacity(0.04)
            })
            .hover(|s| s.bg(theme.primary.opacity(0.06)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.config.terminal.theme = theme_name.clone();
                    cx.emit(ThemePreview(theme_name.clone()));
                    cx.notify();
                }),
            )
            .child(terminal_preview)
            .child(palette_strip)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(4.0))
                    .h(px(26.0))
                    .text_size(px(12.0))
                    .font_weight(if is_sel {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if is_sel {
                        theme.primary
                    } else {
                        theme.muted_foreground
                    })
                    .children(if is_sel {
                        Some(
                            svg()
                                .path("phosphor/check.svg")
                                .size(px(10.0))
                                .text_color(theme.primary),
                        )
                    } else {
                        None
                    })
                    .child(display_name),
            )
    }

    fn render_keys(&mut self, cx: &mut Context<Self>) -> Div {
        let recording = self.recording_key.clone();
        let card_opacity = self.card_opacity();

        // Editable keybinding definitions: (label, field_name)
        let general_keys: &[(&str, &str)] = &[
            ("New Window", "new_window"),
            ("New Tab", "new_tab"),
            ("Next Tab", "next_tab"),
            ("Previous Tab", "previous_tab"),
            ("Close Tab", "close_tab"),
            ("Settings", "settings"),
            ("Command Palette", "command_palette"),
            ("Toggle Input Bar", "toggle_input_bar"),
            ("Toggle Input / Terminal", "focus_input"),
            ("Toggle Pane Scope", "toggle_pane_scope"),
            ("Toggle Left Sidebar", "toggle_left_panel"),
            ("Focus Files", "focus_files"),
            ("Search Files", "search_files"),
            ("Hide Left Sidebar", "collapse_sidebar"),
            ("Quit", "quit"),
        ];

        let pane_keys: &[(&str, &str)] = &[
            ("Split Right", "split_right"),
            ("Split Down", "split_down"),
            ("Toggle Pane Zoom", "toggle_pane_zoom"),
            ("Focus Next Pane", "focus_next_pane"),
            ("Focus Previous Pane", "focus_previous_pane"),
            ("Close Pane", "close_pane"),
        ];

        let surface_keys: &[(&str, &str)] = &[
            ("New Surface Tab", "new_surface"),
            ("New Surface Pane Right", "new_surface_split_right"),
            ("New Surface Pane Down", "new_surface_split_down"),
            ("Next Surface Tab", "next_surface"),
            ("Previous Surface Tab", "previous_surface"),
            ("Rename Surface", "rename_surface"),
            ("Close Surface", "close_surface"),
        ];

        let build_card = |keys: &[(&str, &str)],
                          recording: &Option<String>,
                          this: &mut Self,
                          cx: &mut Context<Self>|
         -> Div {
            // Owned clone: self.group() needs &mut cx, which a live cx.theme()
            // borrow would block.
            let theme_owned = cx.theme().clone();
            let theme = &theme_owned;
            let mut c = card(theme, card_opacity);
            for (i, (label, field)) in keys.iter().enumerate() {
                if i > 0 {
                    c = c.child(row_separator(theme));
                }
                let value = this.binding_value(field).to_string();
                let is_recording = recording.as_deref() == Some(*field);
                let badge = if is_recording {
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Press shortcut…")
                        .into_any_element()
                } else if value.trim().is_empty() {
                    // Unbound (empty in config.toml). Keep a visible target so
                    // the row still reads as clickable instead of a blank gap.
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child("Not set")
                        .into_any_element()
                } else {
                    crate::keycaps::keycaps_for_binding(&value, theme)
                };
                let field_str = field.to_string();
                c = c.child(
                    div()
                        .id(SharedString::from(format!("key-{field}")))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(16.0))
                        .h(px(34.0))
                        .hover(|s| s.bg(theme.muted.opacity(0.025)))
                        .child(
                            div()
                                .text_size(px(13.5))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.foreground.opacity(0.86))
                                .child(label.to_string()),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("key-badge-{field}")))
                                .min_h(px(23.0))
                                .px(px(4.0))
                                .flex()
                                .items_center()
                                .rounded(px(5.0))
                                .cursor_pointer()
                                .bg(if is_recording {
                                    theme.primary.opacity(0.12)
                                } else {
                                    theme.transparent
                                })
                                .text_color(if is_recording {
                                    theme.primary
                                } else {
                                    theme.muted_foreground
                                })
                                .hover(|s| s.bg(theme.muted.opacity(0.055)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.set_recording_key(Some(field_str.clone()));
                                        cx.notify();
                                    }),
                                )
                                .child(badge),
                        ),
                );
            }
            c
        };

        let general_card = build_card(general_keys, &recording, self, cx);
        let pane_card_keys = pane_keys;
        let pane_card = build_card(pane_card_keys, &recording, self, cx);
        let surface_card = build_card(surface_keys, &recording, self, cx);
        let global_summon_enabled = self.config.keybindings.global_summon_enabled;
        let global_summon_value = self.config.keybindings.global_summon.clone();
        let global_summon_recording = recording.as_deref() == Some("global_summon");
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;

        let fixed_tab_card = card(theme, card_opacity).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .px(px(16.0))
                .h(px(34.0))
                .hover(|s| s.bg(theme.muted.opacity(0.025)))
                .child(
                    div()
                        .text_size(px(13.5))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground.opacity(0.86))
                        .child("Select Tab by Number"),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(crate::keycaps::keycaps_for_binding("secondary-1", theme))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(theme.muted_foreground.opacity(0.78))
                                .child("…"),
                        )
                        .child(crate::keycaps::keycaps_for_binding("secondary-9", theme)),
                ),
        );
        #[cfg(target_os = "macos")]
        let fixed_tab_card = fixed_tab_card
            .child(row_separator(theme))
            .child(key_row("Minimize Window", "cmd-m", theme))
            .child(row_separator(theme))
            .child(key_row("Next Window", "cmd-`", theme))
            .child(row_separator(theme))
            .child(key_row("Previous Window", "cmd-shift-`", theme));

        let global_summon_badge = if global_summon_recording {
            div()
                .min_h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .bg(theme.primary.opacity(0.10))
                .text_color(theme.primary)
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .child("Press shortcut…")
                .into_any_element()
        } else if !global_summon_value.trim().is_empty() {
            crate::keycaps::keycaps_for_binding(&global_summon_value, theme)
        } else {
            div()
                .min_h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .rounded(px(8.0))
                .bg(theme.muted.opacity(0.08))
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.muted_foreground)
                .child("Not set")
                .into_any_element()
        };

        let global_summon_card = card(theme, card_opacity).child(
            div()
                .px(px(16.0))
                .py(px(13.0))
                .flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .flex_1()
                        .max_w(px(430.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Global Hotkey"),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .line_height(px(19.0))
                                .text_color(theme.muted_foreground.opacity(0.85))
                                .child(
                                    "Show Vu from anywhere in macOS. Press it again while Vu is frontmost to hide the app.",
                                ),
                        ),
                )
                .child(
                    div().pt(px(1.0)).child(
                        Switch::new("global-summon-enabled")
                            .checked(global_summon_enabled)
                            .small()
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.config.keybindings.global_summon_enabled = *checked;
                                if *checked
                                    && this.config.keybindings.global_summon.trim().is_empty()
                                {
                                    this.config.keybindings.global_summon =
                                        "alt-space".to_string();
                                }
                                sync_keybinding_conflict_error(
                                    &mut this.save_error,
                                    &mut this.save_error_kind,
                                    &this.config.keybindings,
                                );
                                cx.notify();
                            })),
                    ),
                )
        )
        .child(row_separator(theme))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .px(px(16.0))
                .py(px(11.0))
                .hover(|s| s.bg(theme.muted.opacity(0.035)))
                .text_color(if global_summon_enabled {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .child("Shortcut"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(theme.muted_foreground.opacity(0.8))
                                .child(if global_summon_enabled {
                                    "Use a low-conflict system shortcut. Option-Space is familiar, but may collide with launchers."
                                } else {
                                    "Off by default to avoid conflicts with other global shortcuts."
                                }),
                        ),
                )
                .child(
                    div()
                        .id("key-badge-global-summon")
                        .min_w(px(112.0))
                        .flex()
                        .justify_end()
                        .opacity(if global_summon_enabled { 1.0 } else { 0.45 })
                        .cursor_pointer()
                        .rounded(px(7.0))
                        .px(px(4.0))
                        .py(px(3.0))
                        .hover(|s| s.bg(theme.muted.opacity(0.08)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                if this.config.keybindings.global_summon_enabled {
                                    this.set_recording_key(Some("global_summon".to_string()));
                                    cx.notify();
                                }
                            }),
                        )
                        .child(global_summon_badge),
                ),
        );

        let shortcut_groups = div().flex().flex_col().gap(px(8.0)).child(self.group(
            "global",
            "Global",
            global_summon_card,
            cx,
        ));
        let shortcut_groups = shortcut_groups.child(div().h(px(8.0))).child(self.group(
            "general",
            "General",
            general_card,
            cx,
        ));

        section_content(
            "Keyboard Shortcuts",
            "Click a shortcut to record a new key combination.",
            theme,
        )
        .child(shortcut_groups)
        .child(div().flex().flex_col().gap(px(8.0)).child(self.group(
            "fixed-shortcuts",
            "Fixed Shortcuts",
            fixed_tab_card,
            cx,
        )))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(self.group("panes", "Panes", pane_card, cx)),
        )
        .child(div().flex().flex_col().gap(px(8.0)).child(self.group(
            "surfaces",
            "Surfaces",
            surface_card,
            cx,
        )))
        .child(
            div().flex().flex_col().gap(px(8.0)).child(
                self.group(
                    "terminal",
                    "Terminal",
                    card(theme, card_opacity)
                        // Terminal clipboard uses ⌘C/V on macOS and the
                        // Windows-Terminal-standard Ctrl+Shift+C/V on
                        // Windows (plain Ctrl+C would raise SIGINT in
                        // the shell). `secondary-` would collapse to
                        // Ctrl-only on Windows, so we branch explicitly.
                        .child(key_row(
                            "Copy",
                            if cfg!(target_os = "macos") {
                                "cmd-c"
                            } else {
                                "ctrl-shift-c"
                            },
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(key_row(
                            "Paste",
                            if cfg!(target_os = "macos") {
                                "cmd-v"
                            } else {
                                "ctrl-shift-v"
                            },
                            theme,
                        ))
                        .child(row_separator(theme))
                        .child(key_row("Select All", "secondary-a", theme)),
                    cx,
                ),
            ),
        )
    }
}

pub struct VisibilityChanged;
impl EventEmitter<VisibilityChanged> for SettingsPanel {}
impl EventEmitter<SaveSettings> for SettingsPanel {}
impl EventEmitter<ThemePreview> for SettingsPanel {}
impl EventEmitter<ThemeLivePreview> for SettingsPanel {}
impl EventEmitter<AppearancePreview> for SettingsPanel {}

impl Focusable for SettingsPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overlay_progress = self.overlay_motion.value(window);
        if overlay_progress <= 0.001 && !self.visible {
            return div().id("settings-overlay");
        }

        let active = self.active_section;

        let content = match active {
            SettingsSection::General => self.render_general(cx),
            SettingsSection::Appearance => self.render_appearance(cx),
            SettingsSection::Keys => self.render_keys(cx),
        };

        let has_unsaved_changes = self.standalone && self.has_unsaved_changes(cx);
        // Owned clone: self.group() needs &mut cx, which a live cx.theme()
        // borrow would block.
        let theme_owned = cx.theme().clone();
        let theme = &theme_owned;
        let viewport = window.viewport_size();
        let viewport_w = viewport.width.as_f32();
        let viewport_h = viewport.height.as_f32();
        let compact = viewport_w < 980.0;
        let narrow = viewport_w < 840.0;
        let sidebar_w = if narrow {
            px(48.0)
        } else if compact {
            px(144.0)
        } else {
            px(160.0)
        };
        let content_pad = if narrow {
            px(14.0)
        } else if compact {
            px(18.0)
        } else {
            px(24.0)
        };
        // Uniform width for all sections — prevents position jumping when switching tabs
        let card_width = px(((viewport_w * 0.76).clamp(680.0, 980.0)).min(viewport_w - 32.0));
        // While picking colours the panel drops to the bottom and shrinks, so the
        // tab strip and terminal it is restyling stay visible above it.
        let theme_editing = self.theme_editor.is_some() && active == SettingsSection::Appearance;
        let card_height = {
            let target = match active {
                _ if theme_editing => (viewport_h * 0.58).clamp(360.0, 620.0),
                SettingsSection::Appearance => (viewport_h * 0.82).clamp(440.0, 780.0),
                _ => (viewport_h * 0.76).clamp(420.0, 720.0),
            };
            px(target.min(viewport_h - 32.0))
        };

        // Sidebar
        let mut sidebar = div()
            .flex()
            .flex_col()
            .w(sidebar_w)
            .pt(px(8.0))
            .pb(px(12.0))
            .px(if narrow { px(4.0) } else { px(8.0) })
            .gap(px(2.0))
            .flex_shrink_0();

        for section in ALL_SECTIONS {
            let is_active = *section == active;
            let section_val = *section;
            let mut nav_item = div()
                .id(SharedString::from(format!("nav-{}", section.label())))
                .flex()
                .items_center()
                .h(px(32.0))
                .rounded(px(8.0))
                .cursor_pointer()
                .bg(if is_active {
                    theme.muted.opacity(0.15)
                } else {
                    theme.transparent
                })
                .text_color(if is_active {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .font_weight(if is_active {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .hover(|s| {
                    if is_active {
                        s
                    } else {
                        s.bg(theme.muted.opacity(0.08))
                    }
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.active_section = section_val;
                        cx.notify();
                    }),
                );

            if narrow {
                // Icon-only mode: centered icon, no label
                nav_item = nav_item.justify_center().size(px(36.0)).mx_auto().child(
                    svg()
                        .path(section.icon())
                        .size(px(16.0))
                        .text_color(if is_active {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        }),
                );
            } else {
                nav_item = nav_item
                    .gap(px(8.0))
                    .px(px(10.0))
                    .text_size(px(14.0))
                    .child(
                        svg()
                            .path(section.icon())
                            .size(px(15.0))
                            .text_color(if is_active {
                                theme.foreground
                            } else {
                                theme.muted_foreground
                            }),
                    )
                    .child(section.label());
            }

            sidebar = sidebar.child(nav_item);
        }

        let content_scroll = div()
            .id("settings-content-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p(content_pad)
            .child(content);
        let save_button_tint = theme.foreground.opacity(if has_unsaved_changes {
            if theme.is_dark() { 0.88 } else { 0.82 }
        } else {
            if theme.is_dark() { 0.62 } else { 0.52 }
        });
        let config_button_tone =
            theme
                .foreground
                .opacity(if theme.is_dark() { 0.66 } else { 0.54 });
        let save_button_style = ButtonCustomVariant::new(cx)
            .color(save_button_tint.opacity(if has_unsaved_changes { 0.11 } else { 0.04 }))
            .foreground(save_button_tint)
            .hover(save_button_tint.opacity(if has_unsaved_changes { 0.16 } else { 0.04 }))
            .active(save_button_tint.opacity(if has_unsaved_changes { 0.20 } else { 0.04 }));
        let header_density = ui_density_scale(theme);
        let surface_rounding = if self.standalone { px(0.0) } else { px(12.0) };
        let header_left_padding = if self.standalone && cfg!(target_os = "macos") {
            px(78.0)
        } else {
            px(20.0)
        };
        let mut header_title_area = div()
            .id("settings-titlebar-drag-area")
            .flex()
            .items_center()
            .h_full()
            .flex_1()
            .min_w_0()
            .gap(px(8.0))
            .pl(header_left_padding)
            .pr(px(12.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child("Settings"),
            );
        if self.standalone && cfg!(target_os = "macos") {
            header_title_area = header_title_area
                .window_control_area(WindowControlArea::Drag)
                .on_click(|event, window, _cx| {
                    if event.click_count() == 2 {
                        window.titlebar_double_click();
                    }
                });
        }
        let surface = div()
            .id("settings-card")
            .w(if self.standalone {
                px(viewport_w)
            } else {
                card_width
            })
            .h(if self.standalone {
                px(viewport_h)
            } else {
                card_height
            })
            .rounded(surface_rounding)
            .bg(theme.title_bar)
            .overflow_hidden()
            .flex()
            .flex_col()
            .occlude()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                // If recording a keybinding, capture the keystroke.
                if this.recording_key.is_some() {
                    this.record_keystroke(&event.keystroke, cx);
                    return;
                }
                match event.keystroke.key.as_str() {
                    "escape" => {
                        if this.standalone {
                            if this.request_standalone_close(window, cx) {
                                window.remove_window();
                            }
                        } else {
                            this.save(window, cx);
                        }
                    }
                    "enter" if event.keystroke.modifiers.platform => {
                        this.save(window, cx);
                    }
                    "s" if event.keystroke.modifiers.platform => {
                        this.save(window, cx);
                    }
                    "w" if event.keystroke.modifiers.platform => {
                        if this.standalone {
                            if this.request_standalone_close(window, cx) {
                                window.remove_window();
                            }
                        } else {
                            this.save(window, cx);
                        }
                    }
                    _ => {}
                }
            }))
            // Header
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(44.0))
                            .child(header_title_area)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .flex_shrink_0()
                                    .pr(px(20.0))
                                    .children(self.standalone.then(|| {
                                        let (icon, label, tone) = if has_unsaved_changes {
                                            (
                                                "phosphor/warning.svg",
                                                "Unsaved",
                                                theme
                                                    .warning
                                                    .opacity(if theme.is_dark() { 0.96 } else { 0.92 }),
                                            )
                                        } else {
                                            (
                                                "phosphor/check-circle-fill.svg",
                                                "Saved",
                                                theme
                                                    .foreground
                                                    .opacity(if theme.is_dark() { 0.52 } else { 0.42 }),
                                            )
                                        };
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(5.0))
                                            .min_w(px(64.0))
                                            .justify_end()
                                            .px(px(7.0))
                                            .h(px(24.0))
                                            .rounded(px(6.0))
                                            .bg(if has_unsaved_changes {
                                                theme.warning.opacity(if theme.is_dark() {
                                                    0.095
                                                } else {
                                                    0.070
                                                })
                                            } else {
                                                theme.transparent
                                            })
                                            .child(
                                                svg()
                                                    .path(icon)
                                                    .size(px(12.0))
                                                    .text_color(tone),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.5))
                                                    .line_height(px(15.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(tone)
                                                    .whitespace_nowrap()
                                                    .child(label),
                                            )
                                    }))
                                    .child(
                                        Button::new("settings-open-config")
                                            .ghost()
                                            .small()
                                            .compact()
                                            .h(px(28.0 * header_density))
                                            .w(px(28.0 * header_density))
                                            .rounded(px(7.0 * header_density))
                                            .tooltip("Open config.toml")
                                            .child(
                                                svg()
                                                    .path("phosphor/file-text.svg")
                                                    .size(px(15.0 * header_density))
                                                    .text_color(config_button_tone),
                                            )
                                            .on_click(|_, _, cx| {
                                                let path = Config::config_path();
                                                // Ensure the file exists so the editor has something to open.
                                                if !path.exists() {
                                                    if let Some(parent) = path.parent() {
                                                        let _ = std::fs::create_dir_all(parent);
                                                    }
                                                    let _ = std::fs::write(&path, "");
                                                }
                                                match Url::from_file_path(&path) {
                                                    Ok(url) => cx.open_url(url.as_str()),
                                                    Err(()) => {
                                                        log::warn!(
                                                            "settings: failed to build file URL for {}",
                                                            path.display()
                                                        );
                                                    }
                                                }
                                            }),
                                    )
                                    .children(self.standalone.then(|| {
                                        Button::new("settings-apply")
                                            .small()
                                            .compact()
                                            .custom(save_button_style)
                                            .disabled(!has_unsaved_changes)
                                            .h(px(28.0 * header_density))
                                            .px(px(10.0 * header_density))
                                            .rounded(px(8.0 * header_density))
                                            .gap(px(5.0 * header_density))
                                            .tooltip("Save settings")
                                            .child(
                                                svg()
                                                    .path("phosphor/check.svg")
                                                    .size(px(12.0 * header_density))
                                                    .text_color(save_button_tint),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0 * header_density))
                                                    .line_height(px(15.0 * header_density))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(save_button_tint)
                                                    .whitespace_nowrap()
                                                    .child("Save"),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save(window, cx);
                                            }))
                                    })),
                            ),
                    )
                    .child(div().h(px(1.0)).bg(theme.muted.opacity(0.10))),
            )
            .children(
                (self.standalone && self.close_confirmation_visible && has_unsaved_changes).then(
                    || {
                        div()
                            .id("settings-close-confirmation")
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .min_h(px(42.0))
                            .px(px(20.0))
                            .bg(theme.warning.opacity(if theme.is_dark() {
                                0.075
                            } else {
                                0.055
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .min_w_0()
                                    .child(
                                        svg()
                                            .path("phosphor/warning.svg")
                                            .size(px(14.0))
                                            .text_color(theme.warning.opacity(if theme.is_dark() {
                                                0.96
                                            } else {
                                                0.90
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.5))
                                            .line_height(px(18.0))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.foreground.opacity(if theme.is_dark() {
                                                0.84
                                            } else {
                                                0.76
                                            }))
                                            .whitespace_nowrap()
                                            .child("Save changes before closing?"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        Button::new("settings-close-prompt-keep-editing")
                                            .ghost()
                                            .small()
                                            .compact()
                                            .h(px(26.0 * header_density))
                                            .px(px(8.0 * header_density))
                                            .rounded(px(7.0 * header_density))
                                            .child(
                                                div()
                                                    .text_size(px(12.0 * header_density))
                                                    .line_height(px(15.0 * header_density))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(
                                                        theme.muted_foreground.opacity(0.74),
                                                    )
                                                    .whitespace_nowrap()
                                                    .child("Keep Editing"),
                                            )
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.keep_editing_after_close_prompt(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("settings-close-prompt-save")
                                            .small()
                                            .compact()
                                            .custom(save_button_style)
                                            .h(px(26.0 * header_density))
                                            .px(px(9.0 * header_density))
                                            .rounded(px(7.0 * header_density))
                                            .gap(px(5.0 * header_density))
                                            .child(
                                                svg()
                                                    .path("phosphor/check.svg")
                                                    .size(px(12.0 * header_density))
                                                    .text_color(save_button_tint),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0 * header_density))
                                                    .line_height(px(15.0 * header_density))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(save_button_tint)
                                                    .whitespace_nowrap()
                                                    .child("Save and Close"),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_and_close_standalone(window, cx);
                                            })),
                                    ),
                            )
                    },
                ),
            )
            // Error banner
            .children(self.save_error.as_ref().map(|err| {
                let message = if self.save_error_kind == Some(SettingsSaveErrorKind::KeybindingConflict) {
                    err.to_string()
                } else {
                    format!("Save failed: {err}")
                };
                div()
                    .px_4()
                    .py_2()
                    .mx_4()
                    .mt_2()
                    .rounded_md()
                    .bg(theme.danger)
                    .text_color(theme.danger_foreground)
                    .text_xs()
                    .child(message)
            }))
            // Body
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar)
                    .child(content_scroll),
            );

        if self.standalone {
            return div()
                .id("settings-window")
                .size_full()
                .font_family(theme.font_family.clone())
                .bg(theme.background)
                .child(surface);
        }

        let backdrop = div()
            .id("settings-backdrop")
            .occlude()
            .absolute()
            .size_full()
            .bg(theme.background.opacity(
                // Nearly clear while editing colours — the point is to see the
                // app behind the panel repaint as you pick.
                if theme_editing { 0.10 } else { 0.6 } * overlay_progress,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.save(window, cx);
                }),
            );

        let card_shell = div().id("settings-card-shell").absolute().inset_0().flex();
        let card_shell = if theme_editing {
            card_shell.items_end().pb(px(16.0))
        } else {
            card_shell.items_center()
        };
        let card = card_shell.justify_center().opacity(overlay_progress).child(
            div()
                .pt(vertical_reveal_offset(overlay_progress, 18.0))
                .opacity(overlay_progress)
                .child(surface),
        );

        div()
            .id("settings-overlay")
            .absolute()
            .size_full()
            .font_family(theme.font_family.clone())
            .child(backdrop)
            .child(card)
    }
}

// ── Update status helpers ─────────────────────────────────────────

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn update_summary_and_detail(
    state: &crate::updater::CheckState,
    status: crate::updater::UpdaterStatus,
) -> (String, String) {
    use crate::updater::CheckState;
    match state {
        CheckState::Checking => (
            "Checking for updates…".to_string(),
            "Fetching the release feed.".to_string(),
        ),
        CheckState::UpdateAvailable { version, .. } => (
            format!("Update available: {version}"),
            "A newer build has been published.".to_string(),
        ),
        CheckState::UpToDate => (
            "Up to date".to_string(),
            format!(
                "Running {} — latest published build.",
                crate::app_display_version()
            ),
        ),
        CheckState::Error(e) => ("Update check failed".to_string(), e.clone()),
        CheckState::Idle => (status.summary().to_string(), status.detail().to_string()),
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn update_download_url(state: &crate::updater::CheckState) -> Option<String> {
    match state {
        crate::updater::CheckState::UpdateAvailable { url, .. } => Some(url.clone()),
        _ => None,
    }
}

// ── Reusable building blocks ──────────────────────────────────────

fn section_content(title: &str, subtitle: &str, theme: &gpui_component::Theme) -> Div {
    div().flex().flex_col().gap(px(20.0)).child(
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(20.0))
                    .line_height(px(27.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_string()),
            )
            .child(
                div()
                    .max_w(px(520.0))
                    .text_size(px(13.5))
                    .line_height(px(21.5))
                    .text_color(theme.muted_foreground.opacity(0.85))
                    .child(subtitle.to_string()),
            ),
    )
}

fn group_label(text: &str, theme: &gpui_component::Theme) -> Div {
    div()
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.muted_foreground.opacity(0.75))
        .px(px(2.0))
        .pb(px(2.0))
        .child(text.to_string())
}

fn card(theme: &gpui_component::Theme, opacity: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .overflow_hidden()
        .bg(theme.background.opacity(opacity.clamp(0.35, 0.98)))
}

fn row_separator(_theme: &gpui_component::Theme) -> Div {
    div().h(px(6.0))
}

/// A font family in the font pickers, drawn in the face it names so the list
/// shows the typeface itself instead of eighteen identical rows of UI text.
#[derive(Clone)]
struct FontChoice(String);

impl gpui_component::select::SelectItem for FontChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        SharedString::from(self.0.clone())
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }

    fn display_title(&self) -> Option<AnyElement> {
        Some(
            div()
                .font_family(self.0.clone())
                .truncate()
                .child(self.0.clone())
                .into_any_element(),
        )
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .w_full()
            // Both halves use the family being offered: the name shows its
            // letterforms, the specimen shows digits and glyph width, which is
            // what actually differs between the mono faces.
            .font_family(self.0.clone())
            // Long names must truncate, not run under the specimen: some faces
            // render far wider than the UI font the list was sized for.
            .child(div().flex_1().min_w_0().truncate().child(self.0.clone()))
            .child(div().flex_shrink_0().text_color(muted).child("Aa 0123 !="))
    }
}

/// One editable colour in a terminal theme.
///
/// `hint` says what the slot actually paints on screen, since "ANSI 6" alone is
/// not descriptive. Chrome colours are derived from this same palette by
/// `theme::generate_gpui_theme_json`, so editing ANSI 4 restyles the tab strip
/// and sidebar accent too.
struct ThemeSlot {
    label: &'static str,
    hint: &'static str,
    /// `None` targets `foreground`/`background`, `Some(i)` targets `ansi[i]`.
    ansi: Option<usize>,
    is_background: bool,
}

impl ThemeSlot {
    fn read(&self, theme: &vu_terminal::TerminalTheme) -> vu_terminal::Color {
        match self.ansi {
            Some(i) => theme.ansi[i],
            None if self.is_background => theme.background,
            None => theme.foreground,
        }
    }

    fn write(&self, theme: &mut vu_terminal::TerminalTheme, color: vu_terminal::Color) {
        match self.ansi {
            Some(i) => theme.ansi[i] = color,
            None if self.is_background => theme.background = color,
            None => theme.foreground = color,
        }
    }
}

const fn slot(label: &'static str, hint: &'static str, ansi: Option<usize>) -> ThemeSlot {
    ThemeSlot {
        label,
        hint,
        ansi,
        is_background: false,
    }
}

const THEME_SLOTS: &[ThemeSlot] = &[
    ThemeSlot {
        label: "Background",
        hint: "Terminal and app background",
        ansi: None,
        is_background: true,
    },
    slot("Foreground", "Default terminal and UI text", None),
    slot("Black", "Dim fills and separators", Some(0)),
    slot("Red", "Errors and removed diff lines", Some(1)),
    slot("Green", "Success, added diff lines, shell prompt", Some(2)),
    slot("Yellow", "Warnings and command arguments", Some(3)),
    slot("Blue", "UI accent, tab strip, links, directories", Some(4)),
    slot("Magenta", "Keywords and special values", Some(5)),
    slot("Cyan", "Commands and informational output", Some(6)),
    slot("White", "Bright default text", Some(7)),
    slot(
        "Bright Black",
        "Dimmed output, comments, thinking text",
        Some(8),
    ),
    slot("Bright Red", "Bright variant of Red", Some(9)),
    slot("Bright Green", "Bright variant of Green", Some(10)),
    slot("Bright Yellow", "Bright variant of Yellow", Some(11)),
    slot("Bright Blue", "Bright variant of Blue", Some(12)),
    slot("Bright Magenta", "Bright variant of Magenta", Some(13)),
    slot("Bright Cyan", "Bright variant of Cyan", Some(14)),
    slot("Bright White", "Bright variant of White", Some(15)),
];

/// Which horizontal tab strip surface a color picker edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabColorSlot {
    ActiveBackground,
    ActiveBorder,
    InactiveBackground,
    InactiveBorder,
    InactiveHoverBackground,
}

impl TabColorSlot {
    const ALL: [TabColorSlot; 5] = [
        TabColorSlot::ActiveBackground,
        TabColorSlot::ActiveBorder,
        TabColorSlot::InactiveBackground,
        TabColorSlot::InactiveBorder,
        TabColorSlot::InactiveHoverBackground,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::ActiveBackground => "active-bg",
            Self::ActiveBorder => "active-border",
            Self::InactiveBackground => "inactive-bg",
            Self::InactiveBorder => "inactive-border",
            Self::InactiveHoverBackground => "inactive-hover-bg",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ActiveBackground => "Active Tab Color",
            Self::ActiveBorder => "Active Tab Border",
            Self::InactiveBackground => "Inactive Tab Color",
            Self::InactiveBorder => "Inactive Tab Border",
            Self::InactiveHoverBackground => "Inactive Tab Hover",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::ActiveBackground => "Background of the tab you are on.",
            Self::ActiveBorder => "Outline around the active tab. Theme default draws none.",
            Self::InactiveBackground => "Background of tabs you are not on.",
            Self::InactiveBorder => "Outline around inactive tabs.",
            Self::InactiveHoverBackground => {
                "Background of an inactive tab while the mouse is over it."
            }
        }
    }

    fn is_border(self) -> bool {
        matches!(self, Self::ActiveBorder | Self::InactiveBorder)
    }

    fn read(self, a: &AppearanceConfig) -> Option<&str> {
        match self {
            Self::ActiveBackground => a.tab_active_background.as_deref(),
            Self::ActiveBorder => a.tab_active_border.as_deref(),
            Self::InactiveBackground => a.tab_inactive_background.as_deref(),
            Self::InactiveBorder => a.tab_inactive_border.as_deref(),
            Self::InactiveHoverBackground => a.tab_inactive_hover_background.as_deref(),
        }
    }

    fn write(self, a: &mut AppearanceConfig, value: Option<String>) {
        let slot = match self {
            Self::ActiveBackground => &mut a.tab_active_background,
            Self::ActiveBorder => &mut a.tab_active_border,
            Self::InactiveBackground => &mut a.tab_inactive_background,
            Self::InactiveBorder => &mut a.tab_inactive_border,
            Self::InactiveHoverBackground => &mut a.tab_inactive_hover_background,
        };
        *slot = value;
    }

    /// What the picker shows while no override is set, so the swatch matches
    /// roughly what the tab strip is drawing.
    fn fallback(self, theme: &gpui_component::Theme) -> Hsla {
        if self.is_border() {
            theme.border
        } else {
            theme.background
        }
    }
}

impl SettingsPanel {
    fn sync_tab_color_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = cx.theme().clone();
        for (idx, slot) in TabColorSlot::ALL.iter().enumerate() {
            let Some(picker) = self.tab_color_pickers.get(idx) else {
                continue;
            };
            let value = slot
                .read(&self.config.appearance)
                .and_then(crate::tab_colors::parse_hex_hsla)
                .unwrap_or_else(|| slot.fallback(&theme));
            picker.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }

    fn render_tab_color_row(
        &self,
        slot: TabColorSlot,
        theme: &gpui_component::Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let idx = TabColorSlot::ALL
            .iter()
            .position(|s| *s == slot)
            .unwrap_or(0);
        let current = slot.read(&self.config.appearance).map(str::to_string);
        let is_set = current.is_some();
        let picker = self
            .tab_color_pickers
            .get(idx)
            .map(|state| ColorPicker::new(state).small().anchor(Corner::TopRight));
        let reset_btn = Button::new(ElementId::Name(
            format!("tab-color-reset-{}", slot.id()).into(),
        ))
        .label("Reset")
        .small()
        .ghost()
        .disabled(!is_set)
        .on_click(cx.listener(move |this, _, window, cx| {
            slot.write(&mut this.config.appearance, None);
            let fallback = slot.fallback(&cx.theme().clone());
            if let Some(picker) = this.tab_color_pickers.get(idx) {
                picker.update(cx, |state, cx| state.set_value(fallback, window, cx));
            }
            cx.emit(AppearancePreview);
            cx.notify();
        }));

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(18.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .flex_1()
                    .min_w_0()
                    .max_w(px(380.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(slot.label().to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(theme.muted_foreground.opacity(0.82))
                            .child(slot.hint().to_string()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_family(theme.mono_font_family.clone())
                            .text_color(theme.muted_foreground.opacity(0.7))
                            .child(current.unwrap_or_else(|| "theme".to_string())),
                    )
                    .children(picker)
                    .child(reset_btn),
            )
    }
}

fn color_to_hex(c: vu_terminal::Color) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

fn color_to_hsla(c: vu_terminal::Color) -> Hsla {
    gpui::Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

fn hsla_to_color(h: Hsla) -> vu_terminal::Color {
    let rgba: gpui::Rgba = h.into();
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    vu_terminal::Color::rgb(to_u8(rgba.r), to_u8(rgba.g), to_u8(rgba.b))
}

fn row_field(label: &str, input: &Entity<InputState>) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .px(px(16.0))
        .h(px(46.0))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .flex_shrink_0()
                .child(label.to_string()),
        )
        .child(div().flex_1().min_w(px(160.0)).child(Input::new(input)))
}

fn slider_row(
    label: &str,
    hint: &str,
    slider: &Entity<SliderState>,
    value: f32,
    theme: &gpui_component::Theme,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(18.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .flex_1()
                .min_w_0()
                .max_w(px(380.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(19.0))
                        .text_color(theme.muted_foreground.opacity(0.82))
                        .child(hint.to_string()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .w(px(260.0))
                .flex_shrink_0()
                .child(
                    div().flex().justify_end().child(
                        div()
                            .min_w(px(58.0))
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(999.0))
                            .bg(theme.muted.opacity(0.10))
                            .text_size(px(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_align(TextAlign::Center)
                            .text_color(theme.foreground)
                            .child(format!("{:.0}%", value * 100.0)),
                    ),
                )
                .child(div().w_full().child(Slider::new(slider).w_full())),
        )
}

fn searchable_select_row<I>(
    label: &str,
    hint: &str,
    select: &Entity<SelectState<SearchableVec<I>>>,
    placeholder: &str,
    theme: &gpui_component::Theme,
) -> Div
where
    I: gpui_component::select::SelectItem + 'static,
{
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .flex_1()
                .max_w(px(340.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(19.0))
                        .text_color(theme.muted_foreground.opacity(0.82))
                        .child(hint.to_string()),
                ),
        )
        .child(
            div().w(px(236.0)).flex_shrink_0().child(
                Select::new(select)
                    .placeholder(placeholder.to_string())
                    .small(),
            ),
        )
}

fn select_row(
    label: &str,
    hint: &str,
    select: &Entity<SelectState<Vec<String>>>,
    theme: &gpui_component::Theme,
) -> Div {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .flex_1()
                .max_w(px(320.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(19.0))
                        .text_color(theme.muted_foreground.opacity(0.82))
                        .child(hint.to_string()),
                ),
        )
        .child(
            div()
                .w(px(188.0))
                .flex_shrink_0()
                .child(Select::new(select).small()),
        )
}

fn toggle_row(label: &str, hint: &str, toggle: Switch, theme: &gpui_component::Theme) -> Div {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .flex_1()
                .max_w(px(360.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(19.0))
                        .text_color(theme.muted_foreground.opacity(0.82))
                        .child(hint.to_string()),
                ),
        )
        .child(div().pt(px(2.0)).child(toggle))
}

/// Convert a GPUI Keystroke to the binding format string (e.g. "cmd-shift-d").
fn keystroke_to_binding(ks: &gpui::Keystroke) -> String {
    let mut parts = Vec::new();
    if ks.modifiers.platform {
        parts.push("cmd");
    }
    if ks.modifiers.control {
        parts.push("ctrl");
    }
    if ks.modifiers.alt {
        parts.push("alt");
    }
    if ks.modifiers.shift {
        parts.push("shift");
    }
    parts.push(&ks.key);
    parts.join("-")
}

fn keybinding_conflict_message(kb: &vu_core::config::KeybindingConfig) -> Option<String> {
    let conflicts = kb.shortcut_conflicts(&reserved_keybinding_shortcuts());
    let conflict = conflicts.first()?;
    Some(format!(
        "Shortcut conflict: {} is assigned to {}. Pick a different shortcut before saving.",
        conflict.binding,
        human_join(&conflict.actions)
    ))
}

fn sync_keybinding_conflict_error(
    save_error: &mut Option<String>,
    save_error_kind: &mut Option<SettingsSaveErrorKind>,
    kb: &vu_core::config::KeybindingConfig,
) {
    match keybinding_conflict_message(kb) {
        Some(message) => {
            *save_error = Some(message);
            *save_error_kind = Some(SettingsSaveErrorKind::KeybindingConflict);
        }
        None if *save_error_kind == Some(SettingsSaveErrorKind::KeybindingConflict) => {
            *save_error = None;
            *save_error_kind = None;
        }
        None => {}
    }
}

fn reserved_keybinding_shortcuts() -> Vec<(&'static str, &'static str)> {
    crate::fixed_app_keybinding_shortcuts()
}

fn human_join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let mut text = items[..items.len() - 1].join(", ");
            text.push_str(", and ");
            text.push_str(&items[items.len() - 1]);
            text
        }
    }
}

fn key_row(action: &str, shortcut: &str, theme: &gpui_component::Theme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(16.0))
        .h(px(34.0))
        .hover(|s| s.bg(theme.muted.opacity(0.025)))
        .child(
            div()
                .text_size(px(13.5))
                .line_height(px(18.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground.opacity(0.86))
                .child(action.to_string()),
        )
        .child(crate::keycaps::keycaps_for_binding(shortcut, theme))
}

fn display_theme_name(name: &str) -> String {
    match name {
        "flexoki-dark" => "Flexoki Dark".into(),
        "flexoki-light" => "Flexoki Light".into(),
        "catppuccin-mocha" => "Catppuccin".into(),
        "tokyonight" => "Tokyo Night".into(),
        "rose-pine" => "Rose Pine".into(),
        "gruvbox-dark" => "Gruvbox Dark".into(),
        "solarized-dark" => "Solarized Dark".into(),
        "solarized-light" => "Solarized Light".into(),
        "one-half-dark" => "One Half Dark".into(),
        "kanagawa-wave" => "Kanagawa Wave".into(),
        "everforest-dark" => "Everforest Dark".into(),
        "everforest-light" => "Everforest Light".into(),
        "paper-light" => "Paper Light".into(),
        // User themes: convert kebab-case to Title Case
        other => other
            .split('-')
            .map(|word| {
                let mut c = word.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().to_string() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}
