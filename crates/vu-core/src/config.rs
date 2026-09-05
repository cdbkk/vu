use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MIN_UI_FONT_SIZE: f32 = 12.0;
pub const MAX_UI_FONT_SIZE: f32 = 24.0;
pub const MIN_ICON_SCALE: f32 = 0.75;
pub const MAX_ICON_SCALE: f32 = 2.5;
pub const DEFAULT_TERMINAL_FONT_FAMILY: &str = "Ioskeley Mono";

fn default_font_family() -> String {
    DEFAULT_TERMINAL_FONT_FAMILY.into()
}
fn default_font_size() -> f32 {
    14.0
}
fn default_theme() -> String {
    "flexoki-light".into()
}
fn default_cursor_style() -> String {
    "bar".into()
}
fn default_ui_font_family() -> String {
    ".SystemUIFont".into()
}
fn default_ui_font_size() -> f32 {
    16.0f32.clamp(MIN_UI_FONT_SIZE, MAX_UI_FONT_SIZE)
}
fn default_icon_scale() -> f32 {
    1.0
}
fn default_terminal_opacity() -> f32 {
    0.80
}
fn default_ui_opacity() -> f32 {
    0.90
}
fn default_terminal_blur() -> bool {
    true
}
fn default_background_image_opacity() -> f32 {
    0.55
}
fn default_background_image_position() -> String {
    "center".into()
}
fn default_background_image_fit() -> String {
    "contain".into()
}
fn default_tab_accent_inactive_alpha() -> f32 {
    0.15
}
fn default_tab_accent_inactive_hover_alpha() -> f32 {
    0.22
}
fn default_tab_inactive_opacity() -> f32 {
    0.35
}
fn default_tab_close_size() -> f32 {
    13.0
}

fn sanitize_tab_accent_alpha(value: f32, default: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(AppearanceConfig::MIN_TAB_ACCENT_ALPHA, max)
    } else {
        default
    }
}

fn default_restore_terminal_text() -> bool {
    true
}

pub fn is_gpui_pseudo_font_family(name: &str) -> bool {
    name.trim_start().starts_with('.')
}

pub fn sanitize_terminal_font_family(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() || is_gpui_pseudo_font_family(trimmed) {
        DEFAULT_TERMINAL_FONT_FAMILY.to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    pub font_family: String,
    pub font_size: f32,
    pub theme: String,
    pub cursor_style: String,
    /// Terminal behaviour ghostty supports but vu never surfaced.
    pub tweaks: TerminalTweaks,
    /// Working directory for new tabs. `"inherit"` reuses the active tab's
    /// cwd (default). Any other value is tilde-expanded and used if it
    /// exists on disk at spawn time; otherwise this falls back to inherit.
    pub new_tab_directory: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            font_family: default_font_family(),
            font_size: default_font_size(),
            theme: default_theme(),
            cursor_style: default_cursor_style(),
            tweaks: TerminalTweaks::default(),
            new_tab_directory: default_new_tab_directory(),
        }
    }
}

fn default_new_tab_directory() -> String {
    "inherit".into()
}

/// Resolve the working directory for a newly-opened tab from the
/// `new_tab_directory` config value.
///
/// `"inherit"` (or blank) keeps the existing behaviour of reusing
/// `inherited`, the active tab's cwd. Any other value is tilde-expanded
/// and used if it exists on disk at spawn time; otherwise this falls back
/// to `inherited`, same as `"inherit"`, so a stale/typo'd path never
/// crashes tab creation.
pub fn resolve_new_tab_directory(setting: &str, inherited: Option<&str>) -> Option<String> {
    let trimmed = setting.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit") {
        return inherited.map(str::to_string);
    }
    let expanded = expand_tilde(trimmed);
    if Path::new(&expanded).is_dir() {
        Some(expanded)
    } else {
        inherited.map(str::to_string)
    }
}

fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return std::env::home_dir()
            .map(|home| home.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return std::env::home_dir()
            .map(|home| home.join(rest).to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
    }
    path.to_string()
}

/// Terminal rendering options converted to the backend's plain-data tweak type
/// at the `vu-app` boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TerminalTweaks {
    /// Extra cell height as a percentage; 0 is ghostty's natural line height.
    pub line_height_percent: f32,
    /// Extra cell width as a percentage.
    pub letter_spacing_percent: f32,
    /// Font ligatures. Off disables calt/liga/dlig.
    pub ligatures: bool,
    /// Synthetic bolding for thin faces on low-DPI displays.
    pub font_thicken: bool,
    pub cursor_blink: bool,
    /// Render bold text using the bright ANSI colour.
    pub bold_is_bright: bool,
    /// Minimum contrast ratio between text and its background. 1 disables it;
    /// raising it rescues unreadable colours without hand-tuning the palette.
    pub minimum_contrast: f32,
    /// Dim splits that do not have focus. 1 disables the effect.
    pub unfocused_split_opacity: f32,
    pub window_padding_x: f32,
    pub window_padding_y: f32,
    pub mouse_hide_while_typing: bool,
    /// `None` leaves ghostty's default (inverted cell colours).
    pub selection_background: Option<String>,
    pub selection_foreground: Option<String>,
}

impl Default for TerminalTweaks {
    fn default() -> Self {
        Self {
            line_height_percent: 0.0,
            letter_spacing_percent: 0.0,
            ligatures: true,
            font_thicken: false,
            cursor_blink: true,
            bold_is_bright: false,
            minimum_contrast: 1.0,
            unfocused_split_opacity: 1.0,
            window_padding_x: 0.0,
            window_padding_y: 0.0,
            mouse_hide_while_typing: false,
            selection_background: None,
            selection_foreground: None,
        }
    }
}

impl TerminalTweaks {
    pub const MAX_SPACING_PERCENT: f32 = 100.0;
    pub const MIN_SPACING_PERCENT: f32 = -20.0;
    pub const MAX_CONTRAST: f32 = 21.0;
    pub const MAX_PADDING: f32 = 64.0;

    pub fn normalize(&mut self) {
        let pct = |v: f32| {
            if v.is_finite() {
                v.clamp(Self::MIN_SPACING_PERCENT, Self::MAX_SPACING_PERCENT)
            } else {
                0.0
            }
        };
        self.line_height_percent = pct(self.line_height_percent);
        self.letter_spacing_percent = pct(self.letter_spacing_percent);
        self.minimum_contrast = if self.minimum_contrast.is_finite() {
            self.minimum_contrast.clamp(1.0, Self::MAX_CONTRAST)
        } else {
            1.0
        };
        self.unfocused_split_opacity = if self.unfocused_split_opacity.is_finite() {
            self.unfocused_split_opacity.clamp(0.15, 1.0)
        } else {
            1.0
        };
        let pad = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, Self::MAX_PADDING)
            } else {
                0.0
            }
        };
        self.window_padding_x = pad(self.window_padding_x);
        self.window_padding_y = pad(self.window_padding_y);
        if !self
            .selection_background
            .as_deref()
            .is_some_and(is_hex_color)
        {
            self.selection_background = None;
        }
        if !self
            .selection_foreground
            .as_deref()
            .is_some_and(is_hex_color)
        {
            self.selection_foreground = None;
        }
    }
}

fn is_hex_color(v: &str) -> bool {
    let h = v.strip_prefix('#').unwrap_or(v);
    h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub terminal_opacity: f32,
    pub terminal_blur: bool,
    pub ui_opacity: f32,
    pub ui_font_family: String,
    pub ui_font_size: f32,
    pub background_image: Option<String>,
    pub background_image_opacity: f32,
    pub background_image_position: String,
    pub background_image_fit: String,
    pub background_image_repeat: bool,
    /// Accent color alpha for inactive tabs and unfocused pane titles.
    pub tab_accent_inactive_alpha: f32,
    /// Accent color alpha when hovering inactive tabs.
    pub tab_accent_inactive_hover_alpha: f32,
    /// Surface opacity of inactive tab chips without an accent color. 0 hides
    /// the chip entirely; accent-colored tabs use `tab_accent_inactive_alpha`.
    pub tab_inactive_opacity: f32,
    /// Tab close (X) glyph size in px. The hit target grows with it.
    pub tab_close_size: f32,
    /// Horizontal tab strip chrome overrides, `#RRGGBB`. `None` keeps the
    /// theme-derived default for that surface.
    pub tab_active_background: Option<String>,
    pub tab_active_border: Option<String>,
    pub tab_inactive_background: Option<String>,
    pub tab_inactive_border: Option<String>,
    pub tab_inactive_hover_background: Option<String>,
    /// Keep bounded private terminal text so restart continuity can show what
    /// was on screen. This is never exported to workspace layout profiles.
    pub restore_terminal_text: bool,
    /// Hide the per-pane title bar when there are multiple panes. Defaults to
    /// `false` (title bar visible). When `true` the title bar is suppressed
    /// even in split layouts; the fullscreen/close buttons are also hidden.
    pub hide_pane_title_bar: bool,
    /// Workspace tab presentation. Vertical is the default; horizontal remains
    /// available for users who prefer a top tab strip.
    pub tabs_orientation: TabsOrientation,
    /// Multiplier for chrome icon glyphs — activity bar, tab strip, pane
    /// header. Independent of `ui_font_size` so icons can be enlarged without
    /// growing labels or padding. Buttons grow only far enough to contain the
    /// icon.
    pub icon_scale: f32,
    /// Scales how far chrome surfaces (title bar, sidebar, cards) are blended
    /// from the terminal background toward its foreground. 0 makes chrome match
    /// the terminal exactly; higher values separate it more.
    pub chrome_surface_strength: f32,
    /// Same, for borders and dividers specifically.
    pub chrome_border_strength: f32,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            terminal_opacity: default_terminal_opacity(),
            terminal_blur: default_terminal_blur(),
            ui_opacity: default_ui_opacity(),
            ui_font_family: default_ui_font_family(),
            ui_font_size: default_ui_font_size(),
            background_image: None,
            background_image_opacity: default_background_image_opacity(),
            background_image_position: default_background_image_position(),
            background_image_fit: default_background_image_fit(),
            background_image_repeat: false,
            tab_accent_inactive_alpha: default_tab_accent_inactive_alpha(),
            tab_accent_inactive_hover_alpha: default_tab_accent_inactive_hover_alpha(),
            tab_inactive_opacity: default_tab_inactive_opacity(),
            tab_close_size: default_tab_close_size(),
            tab_active_background: None,
            tab_active_border: None,
            tab_inactive_background: None,
            tab_inactive_border: None,
            tab_inactive_hover_background: None,
            restore_terminal_text: default_restore_terminal_text(),
            hide_pane_title_bar: false,
            tabs_orientation: TabsOrientation::Vertical,
            icon_scale: default_icon_scale(),
            chrome_surface_strength: 1.0,
            chrome_border_strength: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TabsOrientation {
    Horizontal,
    #[default]
    Vertical,
}

impl TabsOrientation {
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }
}

impl AppearanceConfig {
    pub const MIN_TAB_ACCENT_ALPHA: f32 = 0.05;
    pub const MAX_TAB_ACCENT_INACTIVE_ALPHA: f32 = 1.0;
    pub const MAX_TAB_ACCENT_INACTIVE_HOVER_ALPHA: f32 = 1.0;
    pub const MIN_TAB_CLOSE_SIZE: f32 = 8.0;
    pub const MAX_TAB_CLOSE_SIZE: f32 = 24.0;

    pub fn normalize(&mut self) {
        self.tab_inactive_opacity = if self.tab_inactive_opacity.is_finite() {
            self.tab_inactive_opacity.clamp(0.0, 1.0)
        } else {
            default_tab_inactive_opacity()
        };
        self.tab_close_size = if self.tab_close_size.is_finite() {
            self.tab_close_size
                .clamp(Self::MIN_TAB_CLOSE_SIZE, Self::MAX_TAB_CLOSE_SIZE)
        } else {
            default_tab_close_size()
        };
        self.tab_accent_inactive_alpha = sanitize_tab_accent_alpha(
            self.tab_accent_inactive_alpha,
            default_tab_accent_inactive_alpha(),
            Self::MAX_TAB_ACCENT_INACTIVE_ALPHA,
        );
        self.tab_accent_inactive_hover_alpha = sanitize_tab_accent_alpha(
            self.tab_accent_inactive_hover_alpha,
            default_tab_accent_inactive_hover_alpha(),
            Self::MAX_TAB_ACCENT_INACTIVE_HOVER_ALPHA,
        )
        .max(self.tab_accent_inactive_alpha);
        for slot in [
            &mut self.tab_active_background,
            &mut self.tab_active_border,
            &mut self.tab_inactive_background,
            &mut self.tab_inactive_border,
            &mut self.tab_inactive_hover_background,
        ] {
            if !slot.as_deref().is_some_and(is_hex_color) {
                *slot = None;
            }
        }
        self.icon_scale = if self.icon_scale.is_finite() {
            self.icon_scale.clamp(MIN_ICON_SCALE, MAX_ICON_SCALE)
        } else {
            default_icon_scale()
        };
        let strength = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 4.0)
            } else {
                1.0
            }
        };
        self.chrome_surface_strength = strength(self.chrome_surface_strength);
        self.chrome_border_strength = strength(self.chrome_border_strength);
    }
}

// Default keybindings are chosen per platform. On macOS the `secondary-`
// modifier token (⌘) is the right primary: Cmd+<letter> doesn't collide
// with anything the terminal expects. On Windows/Linux `secondary-`
// resolves to `Ctrl`, and bare Ctrl+<letter> often has shell meaning
// (Ctrl+L = clear, Ctrl+C = SIGINT, Ctrl+I = Tab, ...), so most app
// actions avoid that space. Pane-management shortcuts therefore stay in
// app-level modifier space instead of borrowing terminal control
// characters like Ctrl+D, which terminals consume as EOF before Vu's
// keybindings ever see them.
fn default_command_palette() -> String {
    // Ctrl+Shift+P on Windows / Linux, Cmd+Shift+P on macOS.
    "secondary-shift-p".into()
}
#[cfg(target_os = "macos")]
fn default_focus_files() -> String {
    // Cmd+Shift+E collides with the embedded terminal/AppKit search-selection
    // path on macOS, so keep the E mnemonic on an option chord that survives
    // native terminal focus.
    "secondary-alt-e".into()
}
#[cfg(not(target_os = "macos"))]
fn default_focus_files() -> String {
    // Matches the common editor convention: Ctrl+Shift+E.
    "secondary-shift-e".into()
}
fn default_search_files() -> String {
    // Matches the common editor convention: Cmd/Ctrl+Shift+F.
    "secondary-shift-f".into()
}

#[cfg(target_os = "macos")]
fn default_new_tab() -> String {
    "secondary-t".into()
}
#[cfg(not(target_os = "macos"))]
fn default_new_tab() -> String {
    "ctrl-shift-t".into()
}

#[cfg(target_os = "macos")]
fn default_new_window() -> String {
    "secondary-n".into()
}
#[cfg(not(target_os = "macos"))]
fn default_new_window() -> String {
    "ctrl-shift-n".into()
}

#[cfg(target_os = "macos")]
fn default_close_tab() -> String {
    "secondary-w".into()
}
#[cfg(not(target_os = "macos"))]
fn default_close_tab() -> String {
    "ctrl-shift-w".into()
}

#[cfg(target_os = "macos")]
fn default_close_pane() -> String {
    "secondary-alt-w".into()
}
#[cfg(not(target_os = "macos"))]
fn default_close_pane() -> String {
    "alt-shift-w".into()
}

#[cfg(target_os = "macos")]
fn default_toggle_pane_zoom() -> String {
    "secondary-shift-enter".into()
}
#[cfg(not(target_os = "macos"))]
fn default_toggle_pane_zoom() -> String {
    "alt-shift-enter".into()
}

#[cfg(target_os = "macos")]
fn default_focus_next_pane() -> String {
    "alt-tab".into()
}
#[cfg(not(target_os = "macos"))]
fn default_focus_next_pane() -> String {
    "ctrl-alt-tab".into()
}

#[cfg(target_os = "macos")]
fn default_focus_previous_pane() -> String {
    "alt-shift-tab".into()
}
#[cfg(not(target_os = "macos"))]
fn default_focus_previous_pane() -> String {
    "ctrl-alt-shift-tab".into()
}

fn default_next_tab() -> String {
    "ctrl-tab".into()
}
fn default_previous_tab() -> String {
    "ctrl-shift-tab".into()
}

fn default_settings() -> String {
    // Ctrl+, is the cross-editor convention (VSCode, IntelliJ, Windows
    // Terminal) and doesn't produce a control character, so it works
    // the same on both platforms via `secondary-`.
    "secondary-,".into()
}

#[cfg(target_os = "macos")]
fn default_quit() -> String {
    "secondary-q".into()
}
#[cfg(not(target_os = "macos"))]
fn default_quit() -> String {
    // Alt+F4 is the Windows platform convention for "close the app
    // window". Ctrl+Q is XOFF / pwsh's quoted-insert, so it can't be
    // used without stealing it from the shell.
    "alt-f4".into()
}

#[cfg(target_os = "macos")]
fn default_split_right() -> String {
    "secondary-d".into()
}
#[cfg(not(target_os = "macos"))]
fn default_split_right() -> String {
    "alt-d".into()
}

#[cfg(target_os = "macos")]
fn default_split_down() -> String {
    "secondary-shift-d".into()
}
#[cfg(not(target_os = "macos"))]
fn default_split_down() -> String {
    "alt-shift-d".into()
}

#[cfg(target_os = "macos")]
fn default_focus_input() -> String {
    "secondary-i".into()
}
#[cfg(not(target_os = "macos"))]
fn default_focus_input() -> String {
    // Ctrl+I is the Tab character (0x09). Ctrl+Shift+I stays free.
    "ctrl-shift-i".into()
}

fn default_toggle_input_bar() -> String {
    "ctrl-`".into()
}
fn default_toggle_pane_scope() -> String {
    "secondary-'".into()
}
#[cfg(target_os = "macos")]
fn default_toggle_left_panel() -> String {
    // Cmd+B is the established macOS/editor convention for showing or
    // hiding the left sidebar, and Cmd chords do not steal terminal input.
    "secondary-b".into()
}
#[cfg(not(target_os = "macos"))]
fn default_toggle_left_panel() -> String {
    // Avoid bare Ctrl+B on Windows/Linux: it is tmux's prefix and a
    // real terminal control character. Ctrl+Shift+B stays app-level.
    "ctrl-shift-b".into()
}
#[cfg(target_os = "macos")]
fn default_collapse_sidebar() -> String {
    "secondary-shift-b".into()
}
#[cfg(not(target_os = "macos"))]
fn default_collapse_sidebar() -> String {
    "ctrl-alt-b".into()
}

#[cfg(target_os = "macos")]
fn default_new_surface() -> String {
    "secondary-alt-t".into()
}
#[cfg(not(target_os = "macos"))]
fn default_new_surface() -> String {
    "alt-shift-t".into()
}

#[cfg(target_os = "macos")]
fn default_new_surface_split_right() -> String {
    "secondary-alt-d".into()
}
#[cfg(not(target_os = "macos"))]
fn default_new_surface_split_right() -> String {
    "alt-shift-right".into()
}

#[cfg(target_os = "macos")]
fn default_new_surface_split_down() -> String {
    "secondary-alt-shift-d".into()
}
#[cfg(not(target_os = "macos"))]
fn default_new_surface_split_down() -> String {
    "alt-shift-down".into()
}

#[cfg(target_os = "macos")]
fn default_next_surface() -> String {
    "secondary-ctrl-]".into()
}
#[cfg(not(target_os = "macos"))]
fn default_next_surface() -> String {
    "alt-shift-]".into()
}

#[cfg(target_os = "macos")]
fn default_previous_surface() -> String {
    "secondary-ctrl-[".into()
}
#[cfg(not(target_os = "macos"))]
fn default_previous_surface() -> String {
    "alt-shift-[".into()
}

#[cfg(target_os = "macos")]
fn default_rename_surface() -> String {
    "secondary-alt-r".into()
}
#[cfg(not(target_os = "macos"))]
fn default_rename_surface() -> String {
    "alt-shift-r".into()
}

#[cfg(target_os = "macos")]
fn default_close_surface() -> String {
    "secondary-alt-shift-w".into()
}
#[cfg(not(target_os = "macos"))]
fn default_close_surface() -> String {
    "alt-shift-x".into()
}

fn default_global_summon() -> String {
    "alt-space".into()
}
fn default_global_summon_enabled() -> bool {
    false
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingConfig {
    pub command_palette: String,
    pub new_window: String,
    pub new_tab: String,
    pub close_tab: String,
    pub close_pane: String,
    pub toggle_pane_zoom: String,
    pub focus_next_pane: String,
    pub focus_previous_pane: String,
    pub next_tab: String,
    pub previous_tab: String,
    pub settings: String,
    pub quit: String,
    pub split_right: String,
    pub split_down: String,
    pub focus_input: String,
    pub toggle_input_bar: String,
    pub toggle_pane_scope: String,
    #[serde(alias = "toggle_vertical_tabs")]
    pub toggle_left_panel: String,
    pub focus_files: String,
    pub search_files: String,
    pub collapse_sidebar: String,
    pub new_surface: String,
    pub new_surface_split_right: String,
    pub new_surface_split_down: String,
    pub next_surface: String,
    pub previous_surface: String,
    pub rename_surface: String,
    pub close_surface: String,
    pub global_summon_enabled: bool,
    pub global_summon: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingConflict {
    pub binding: String,
    pub actions: Vec<String>,
}

impl KeybindingConfig {
    pub fn normalize(&mut self) {
        #[cfg(target_os = "macos")]
        {
            // Option+bracket resolves to a Unicode punctuation character on
            // macOS before GPUI can treat it as a shortcut, so migrate only
            // the exact former defaults to the current reachable defaults.
            if canonical_keybinding(&self.next_surface) == canonical_keybinding("secondary-alt-]") {
                self.next_surface = default_next_surface();
            }
            if canonical_keybinding(&self.previous_surface)
                == canonical_keybinding("secondary-alt-[")
            {
                self.previous_surface = default_previous_surface();
            }
        }
    }

    pub fn active_shortcuts(&self) -> Vec<(&'static str, &str)> {
        let mut shortcuts = vec![
            ("New Window", self.new_window.as_str()),
            ("New Tab", self.new_tab.as_str()),
            ("Next Tab", self.next_tab.as_str()),
            ("Previous Tab", self.previous_tab.as_str()),
            ("Close Tab", self.close_tab.as_str()),
            ("Close Pane", self.close_pane.as_str()),
            ("Toggle Pane Zoom", self.toggle_pane_zoom.as_str()),
            ("Focus Next Pane", self.focus_next_pane.as_str()),
            ("Focus Previous Pane", self.focus_previous_pane.as_str()),
            ("Settings", self.settings.as_str()),
            ("Command Palette", self.command_palette.as_str()),
            ("Toggle Input Bar", self.toggle_input_bar.as_str()),
            ("Toggle Input / Terminal", self.focus_input.as_str()),
            ("Split Right", self.split_right.as_str()),
            ("Split Down", self.split_down.as_str()),
            ("Toggle Pane Scope", self.toggle_pane_scope.as_str()),
            ("Toggle Left Sidebar", self.toggle_left_panel.as_str()),
            ("Focus Files", self.focus_files.as_str()),
            ("Search Files", self.search_files.as_str()),
            ("Collapse/Expand Sidebar", self.collapse_sidebar.as_str()),
            ("New Surface Tab", self.new_surface.as_str()),
            (
                "New Surface Pane Right",
                self.new_surface_split_right.as_str(),
            ),
            (
                "New Surface Pane Down",
                self.new_surface_split_down.as_str(),
            ),
            ("Next Surface Tab", self.next_surface.as_str()),
            ("Previous Surface Tab", self.previous_surface.as_str()),
            ("Rename Surface", self.rename_surface.as_str()),
            ("Close Surface", self.close_surface.as_str()),
            ("Quit", self.quit.as_str()),
        ];

        if self.global_summon_enabled {
            shortcuts.push(("Summon / Hide Vu", self.global_summon.as_str()));
        }
        shortcuts
            .into_iter()
            .filter(|(_, binding)| !binding.trim().is_empty())
            .collect()
    }

    pub fn shortcut_conflicts(
        &self,
        reserved_shortcuts: &[(&'static str, &'static str)],
    ) -> Vec<KeybindingConflict> {
        let mut seen = std::collections::BTreeMap::<String, (String, Vec<String>)>::new();

        for (label, binding) in self
            .active_shortcuts()
            .into_iter()
            .chain(reserved_shortcuts.iter().copied())
        {
            let Some(canonical) = canonical_keybinding(binding) else {
                continue;
            };
            let entry = seen
                .entry(canonical.clone())
                .or_insert_with(|| (canonical, Vec::new()));
            if !entry.1.iter().any(|existing| existing == label) {
                entry.1.push(label.to_string());
            }
        }

        seen.into_values()
            .filter_map(|(binding, actions)| {
                (actions.len() > 1).then_some(KeybindingConflict { binding, actions })
            })
            .collect()
    }
}

impl Default for KeybindingConfig {
    fn default() -> Self {
        Self {
            command_palette: default_command_palette(),
            new_window: default_new_window(),
            new_tab: default_new_tab(),
            close_tab: default_close_tab(),
            close_pane: default_close_pane(),
            toggle_pane_zoom: default_toggle_pane_zoom(),
            focus_next_pane: default_focus_next_pane(),
            focus_previous_pane: default_focus_previous_pane(),
            next_tab: default_next_tab(),
            previous_tab: default_previous_tab(),
            settings: default_settings(),
            quit: default_quit(),
            split_right: default_split_right(),
            split_down: default_split_down(),
            focus_input: default_focus_input(),
            toggle_input_bar: default_toggle_input_bar(),
            toggle_pane_scope: default_toggle_pane_scope(),
            toggle_left_panel: default_toggle_left_panel(),
            focus_files: default_focus_files(),
            search_files: default_search_files(),
            collapse_sidebar: default_collapse_sidebar(),
            new_surface: default_new_surface(),
            new_surface_split_right: default_new_surface_split_right(),
            new_surface_split_down: default_new_surface_split_down(),
            next_surface: default_next_surface(),
            previous_surface: default_previous_surface(),
            rename_surface: default_rename_surface(),
            close_surface: default_close_surface(),
            global_summon_enabled: default_global_summon_enabled(),
            global_summon: default_global_summon(),
        }
    }
}

pub fn canonical_keybinding(binding: &str) -> Option<String> {
    let strokes = binding
        .split_whitespace()
        .filter_map(canonical_keystroke)
        .collect::<Vec<_>>();
    (!strokes.is_empty()).then(|| strokes.join(" "))
}

fn canonical_keystroke(stroke: &str) -> Option<String> {
    let mut modifiers = Vec::<&'static str>::new();
    // A trailing separator is the literal minus key, e.g. "ctrl--"
    // means Control-Minus rather than a missing key token.
    let is_minus_key = stroke.ends_with('-');
    let mut key = is_minus_key.then(|| "-".to_string());
    let mut parts = stroke
        .split('-')
        .map(|part| part.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    if is_minus_key {
        parts.pop();
    }

    for raw in parts {
        if raw.is_empty() {
            continue;
        }
        match raw.as_str() {
            "cmd" | "command" | "meta" => push_unique_modifier(&mut modifiers, "cmd"),
            "platform" | "secondary" => {
                push_unique_modifier(&mut modifiers, platform_modifier_name())
            }
            "ctrl" | "control" => push_unique_modifier(&mut modifiers, "ctrl"),
            "alt" | "option" => push_unique_modifier(&mut modifiers, "alt"),
            "shift" => push_unique_modifier(&mut modifiers, "shift"),
            "fn" => push_unique_modifier(&mut modifiers, "fn"),
            "return" => key = Some("enter".to_string()),
            "esc" => key = Some("escape".to_string()),
            other => key = Some(other.to_string()),
        }
    }

    let key = key?;
    let mut parts = ["cmd", "ctrl", "alt", "shift", "fn"]
        .into_iter()
        .filter(|modifier| modifiers.contains(modifier))
        .map(str::to_string)
        .collect::<Vec<_>>();
    parts.push(key);
    Some(parts.join("-"))
}

fn platform_modifier_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    }
}

fn push_unique_modifier(modifiers: &mut Vec<&'static str>, modifier: &'static str) {
    if !modifiers.contains(&modifier) {
        modifiers.push(modifier);
    }
}

/// Network proxy configuration.
///
/// When set, these values override any `HTTP_PROXY` / `HTTPS_PROXY` environment
/// variables that may have been inherited from the shell.  Leave empty to rely
/// on the environment (the default).
///
/// # Example
/// ```toml
/// [network]
/// http_proxy  = "http://127.0.0.1:1086"
/// https_proxy = "http://127.0.0.1:1086"
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// HTTP proxy URL, e.g. `http://127.0.0.1:1086`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<String>,
    /// HTTPS proxy URL, e.g. `http://127.0.0.1:1086`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_proxy: Option<String>,
}

impl NetworkConfig {
    /// Apply the configured proxy values to the current process environment so
    /// that all downstream `reqwest` clients, including the updater,
    /// pick them up automatically.
    ///
    /// - `Some(non_empty)` → sets the env var (overrides shell-inherited value).
    /// - `Some("")` / `None` → no-op (leave whatever the env has).
    ///
    /// # Safety
    ///
    /// Must be called from a single-threaded context (e.g. early in `main`)
    /// before any other threads are spawned, because `std::env::set_var` is
    /// not thread-safe.
    pub unsafe fn apply_to_env(&self) {
        unsafe {
            Self::apply_one("HTTP_PROXY", "http_proxy", self.http_proxy.as_deref());
            Self::apply_one("HTTPS_PROXY", "https_proxy", self.https_proxy.as_deref());
        }
    }

    /// # Safety
    /// Same as `apply_to_env` — must be called single-threaded.
    unsafe fn apply_one(upper: &str, lower: &str, value: Option<&str>) {
        if let Some(v) = value {
            if !v.is_empty() {
                unsafe {
                    std::env::set_var(upper, v);
                    std::env::set_var(lower, v);
                }
                log::info!("network: {upper} set from config");
            }
        }
        // None or empty string → leave the environment untouched.
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub terminal: TerminalConfig,
    pub appearance: AppearanceConfig,
    pub keybindings: KeybindingConfig,
    pub network: NetworkConfig,
}

impl Config {
    pub fn normalize(&mut self) {
        self.appearance.normalize();
        self.keybindings.normalize();
        self.terminal.tweaks.normalize();
    }

    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();
        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content)?
        } else {
            Config::default()
        };

        config.normalize();
        Ok(config)
    }

    pub fn config_path() -> PathBuf {
        std::env::var_os("VU_CONFIG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(vu_paths::config_file)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)?;
        write_private_atomic(&path, content.as_bytes())
    }
}

fn write_private_atomic(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        path.with_extension(format!("tmp.{}.{}", std::process::id(), unique))
    };

    let write_result = (|| -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp_path)?;
        file.write_all(content)?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }

    replace_file(&tmp_path, path)?;
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp_path: &Path, path: &Path) -> Result<()> {
    std::fs::rename(tmp_path, path)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(tmp_path: &Path, path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::rename(tmp_path, path)?;
        return Ok(());
    }

    let backup_path = {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        path.with_extension(format!("bak.{}.{}", std::process::id(), unique))
    };

    std::fs::rename(path, &backup_path)?;
    match std::fs::rename(tmp_path, path) {
        Ok(()) => {
            let _ = std::fs::remove_file(&backup_path);
            Ok(())
        }
        Err(err) => {
            let _ = std::fs::rename(&backup_path, path);
            Err(err.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Config, DEFAULT_TERMINAL_FONT_FAMILY, NetworkConfig, TabsOrientation,
        resolve_new_tab_directory, sanitize_terminal_font_family,
    };

    #[test]
    fn terminal_font_sanitizer_rejects_gpui_pseudo_families() {
        assert_eq!(
            sanitize_terminal_font_family(".ZedMono"),
            DEFAULT_TERMINAL_FONT_FAMILY
        );
        assert_eq!(
            sanitize_terminal_font_family(" .SystemUIFont "),
            DEFAULT_TERMINAL_FONT_FAMILY
        );
        assert_eq!(
            sanitize_terminal_font_family("JetBrains Mono"),
            "JetBrains Mono"
        );
    }

    #[test]
    fn new_configs_enable_restore_terminal_text_by_default() {
        assert!(Config::default().appearance.restore_terminal_text);
    }

    #[test]
    fn new_tab_directory_defaults_to_inherit() {
        assert_eq!(Config::default().terminal.new_tab_directory, "inherit");
    }

    #[test]
    fn resolve_new_tab_directory_uses_existing_path() {
        let dir = std::env::temp_dir();
        let dir_str = dir.to_string_lossy().into_owned();
        assert_eq!(
            resolve_new_tab_directory(&dir_str, Some("/other")),
            Some(dir_str)
        );
    }

    #[test]
    fn resolve_new_tab_directory_falls_back_to_inherit_on_missing_path() {
        assert_eq!(
            resolve_new_tab_directory("/definitely/not/a/real/path/vu-test", Some("/other")),
            Some("/other".to_string())
        );
        assert_eq!(
            resolve_new_tab_directory("inherit", Some("/other")),
            Some("/other".to_string())
        );
        assert_eq!(resolve_new_tab_directory("", None), None);
    }

    #[test]
    fn tweaks_normalize_rejects_bad_values() {
        use super::TerminalTweaks;
        let mut tweaks = TerminalTweaks {
            line_height_percent: f32::NAN,
            minimum_contrast: 999.0,
            unfocused_split_opacity: -3.0,
            selection_background: Some("not-a-color".into()),
            selection_foreground: Some("#AABBCC".into()),
            ..TerminalTweaks::default()
        };
        tweaks.normalize();
        assert_eq!(tweaks.line_height_percent, 0.0);
        assert_eq!(tweaks.minimum_contrast, TerminalTweaks::MAX_CONTRAST);
        assert_eq!(tweaks.unfocused_split_opacity, 0.15);
        assert_eq!(tweaks.selection_background, None);
        assert_eq!(tweaks.selection_foreground.as_deref(), Some("#AABBCC"));
    }

    #[test]
    fn loaded_legacy_configs_inherit_restore_terminal_text_default() {
        let content = r#"
[appearance]
terminal_opacity = 0.8
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert!(config.appearance.restore_terminal_text);
    }

    #[test]
    fn loaded_configs_preserve_explicit_restore_terminal_text() {
        let content = r#"
[appearance]
restore_terminal_text = false
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert!(!config.appearance.restore_terminal_text);
    }

    #[test]
    fn default_keybindings_include_file_sidebar_shortcuts() {
        let config = Config::default();
        let expected_focus = if cfg!(target_os = "macos") {
            "secondary-alt-e"
        } else {
            "secondary-shift-e"
        };

        assert_eq!(config.keybindings.focus_files, expected_focus);
        assert_eq!(config.keybindings.search_files, "secondary-shift-f");

        let shortcuts = config.keybindings.active_shortcuts();
        assert!(shortcuts.contains(&("Focus Files", expected_focus)));
        assert!(shortcuts.contains(&("Search Files", "secondary-shift-f")));
    }

    #[test]
    fn default_keybindings_include_reachable_surface_cycle_shortcuts() {
        let config = Config::default();
        let (expected_next, expected_previous) = if cfg!(target_os = "macos") {
            ("secondary-ctrl-]", "secondary-ctrl-[")
        } else {
            ("alt-shift-]", "alt-shift-[")
        };

        assert_eq!(config.keybindings.next_surface, expected_next);
        assert_eq!(config.keybindings.previous_surface, expected_previous);

        let shortcuts = config.keybindings.active_shortcuts();
        assert!(shortcuts.contains(&("Next Surface Tab", expected_next)));
        assert!(shortcuts.contains(&("Previous Surface Tab", expected_previous)));
    }

    #[test]
    fn default_keybindings_include_pane_cycle_shortcuts_without_conflicts() {
        let config = Config::default();
        let (expected_next, expected_previous) = if cfg!(target_os = "macos") {
            ("alt-tab", "alt-shift-tab")
        } else {
            ("ctrl-alt-tab", "ctrl-alt-shift-tab")
        };

        assert_eq!(config.keybindings.focus_next_pane, expected_next);
        assert_eq!(config.keybindings.focus_previous_pane, expected_previous);

        let shortcuts = config.keybindings.active_shortcuts();
        assert!(shortcuts.contains(&("Focus Next Pane", expected_next)));
        assert!(shortcuts.contains(&("Focus Previous Pane", expected_previous)));
        assert!(config.keybindings.shortcut_conflicts(&[]).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_normalize_migrates_broken_option_bracket_surface_defaults() {
        let mut config = Config::default();
        config.keybindings.next_surface = "secondary-alt-]".to_string();
        config.keybindings.previous_surface = "secondary-alt-[".to_string();

        config.normalize();

        assert_eq!(config.keybindings.next_surface, "secondary-ctrl-]");
        assert_eq!(config.keybindings.previous_surface, "secondary-ctrl-[");
    }

    #[test]
    fn legacy_configs_receive_file_sidebar_shortcut_defaults() {
        let content = r#"
[keybindings]
command_palette = "secondary-shift-p"
"#;
        let config: Config = toml::from_str(content).unwrap();
        let expected_focus = if cfg!(target_os = "macos") {
            "secondary-alt-e"
        } else {
            "secondary-shift-e"
        };

        assert_eq!(config.keybindings.focus_files, expected_focus);
        assert_eq!(config.keybindings.search_files, "secondary-shift-f");
    }

    #[test]
    fn loaded_configs_preserve_explicit_file_sidebar_shortcuts() {
        let content = r#"
[keybindings]
focus_files = "alt-e"
search_files = "alt-f"
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert_eq!(config.keybindings.focus_files, "alt-e");
        assert_eq!(config.keybindings.search_files, "alt-f");

        let shortcuts = config.keybindings.active_shortcuts();
        assert!(shortcuts.contains(&("Focus Files", "alt-e")));
        assert!(shortcuts.contains(&("Search Files", "alt-f")));
    }

    #[test]
    fn omitted_tabs_orientation_defaults_to_vertical() {
        let content = r#"
[appearance]
terminal_opacity = 0.8
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert_eq!(
            config.appearance.tabs_orientation,
            TabsOrientation::Vertical
        );
        assert_eq!(
            Config::default().appearance.tabs_orientation,
            TabsOrientation::Vertical
        );
    }

    #[test]
    fn tabs_orientation_config_is_preserved() {
        let content = r#"
[appearance]
tabs_orientation = "vertical"

[keybindings]
toggle_vertical_tabs = "secondary-b"
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert_eq!(
            config.appearance.tabs_orientation,
            TabsOrientation::Vertical
        );
        assert_eq!(config.keybindings.toggle_left_panel, "secondary-b");

        let serialized = toml::to_string(&config).unwrap();
        assert!(serialized.contains("tabs_orientation = \"vertical\""));
        assert!(!serialized.contains("toggle_vertical_tabs"));
        assert!(serialized.contains("toggle_left_panel"));
    }

    #[test]
    fn explicit_horizontal_tabs_orientation_is_preserved() {
        let content = r#"
[appearance]
tabs_orientation = "horizontal"
"#;
        let config: Config = toml::from_str(content).unwrap();

        assert_eq!(
            config.appearance.tabs_orientation,
            TabsOrientation::Horizontal
        );
    }

    #[test]
    fn keybinding_conflicts_detect_duplicate_configured_shortcuts() {
        let mut config = Config::default();
        config.keybindings.command_palette = "ctrl-shift-p".to_string();
        config.keybindings.settings = "control-shift-p".to_string();

        let conflicts = config.keybindings.shortcut_conflicts(&[]);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].actions,
            vec!["Settings".to_string(), "Command Palette".to_string()]
        );
    }

    #[test]
    fn keybinding_conflicts_match_secondary_to_platform_modifier() {
        let mut config = Config::default();
        config.keybindings.command_palette = "secondary-shift-p".to_string();
        config.keybindings.settings = if cfg!(target_os = "macos") {
            "cmd-shift-p".to_string()
        } else {
            "ctrl-shift-p".to_string()
        };

        let conflicts = config.keybindings.shortcut_conflicts(&[]);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].actions,
            vec!["Settings".to_string(), "Command Palette".to_string()]
        );
    }

    #[test]
    fn keybinding_conflicts_include_minus_key_shortcuts() {
        let mut config = Config::default();
        config.keybindings.command_palette = "ctrl--".to_string();
        config.keybindings.settings = "control--".to_string();

        let conflicts = config.keybindings.shortcut_conflicts(&[]);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].actions,
            vec!["Settings".to_string(), "Command Palette".to_string()]
        );
    }

    #[test]
    fn keybinding_conflicts_ignore_disabled_global_shortcuts() {
        let mut config = Config::default();
        config.keybindings.global_summon_enabled = false;
        config.keybindings.global_summon = config.keybindings.command_palette.clone();

        assert!(config.keybindings.shortcut_conflicts(&[]).is_empty());
    }

    #[test]
    fn keybinding_conflicts_include_reserved_shortcuts() {
        let mut config = Config::default();
        config.keybindings.command_palette = if cfg!(target_os = "macos") {
            "cmd-m".to_string()
        } else {
            "ctrl-m".to_string()
        };

        let conflicts = config
            .keybindings
            .shortcut_conflicts(&[("Minimize Window", "secondary-m")]);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].actions,
            vec!["Command Palette".to_string(), "Minimize Window".to_string()]
        );
    }

    #[test]
    fn network_config_defaults_to_empty() {
        let config = NetworkConfig::default();
        assert!(config.http_proxy.is_none());
        assert!(config.https_proxy.is_none());
    }

    #[test]
    fn network_config_deserializes_from_toml() {
        let content = r#"
[network]
http_proxy  = "http://127.0.0.1:1086"
https_proxy = "http://127.0.0.1:1086"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(
            config.network.http_proxy.as_deref(),
            Some("http://127.0.0.1:1086")
        );
        assert_eq!(
            config.network.https_proxy.as_deref(),
            Some("http://127.0.0.1:1086")
        );
    }

    #[test]
    fn network_config_missing_section_is_default() {
        let content = r#"
[terminal]
font_size = 14.0
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert!(config.network.http_proxy.is_none());
        assert!(config.network.https_proxy.is_none());
    }

    #[test]
    fn network_config_partial_fields() {
        let content = r#"
[network]
http_proxy = "http://proxy.example.com:8080"
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(
            config.network.http_proxy.as_deref(),
            Some("http://proxy.example.com:8080")
        );
        assert!(config.network.https_proxy.is_none());
    }

    #[test]
    fn network_config_round_trips_through_serialization() {
        let original = NetworkConfig {
            http_proxy: Some("http://127.0.0.1:1086".to_string()),
            https_proxy: Some("http://127.0.0.1:1087".to_string()),
        };
        let serialized = toml::to_string(&original).unwrap();
        let deserialized: NetworkConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(original.http_proxy, deserialized.http_proxy);
        assert_eq!(original.https_proxy, deserialized.https_proxy);
    }

    #[test]
    fn network_config_empty_strings_are_preserved() {
        // Empty strings are valid TOML values and deserialize to Some("").
        // apply_to_env treats them as no-op (same as None).
        let content = r#"
[network]
http_proxy  = ""
https_proxy = ""
"#;
        let config: Config = toml::from_str(content).unwrap();
        assert_eq!(config.network.http_proxy.as_deref(), Some(""));
        assert_eq!(config.network.https_proxy.as_deref(), Some(""));
    }
}
