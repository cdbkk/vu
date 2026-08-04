//! Public facade: `WindowsGhosttyApp` / `WindowsGhosttyTerminal` that
//! the rest of the workspace consumes through the same `GhosttyApp` /
//! `GhosttyTerminal` type names that the macOS path uses (re-exported
//! from `crate::lib`). This keeps callers in `vu-app` shape-identical
//! across platforms.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use super::host_view::RenderSession;
use super::render::{RendererConfig, ThemeColors};
use crate::stub::{
    CommandFinishedSignal, CommandRecord, GhosttyConfigPatch, GhosttyScrollbar,
    GhosttySplitDirection, GhosttySurfaceEvent, MouseButton, SurfaceSize,
};
use crate::{AppearanceConfig, TerminalColors};

fn theme_from_colors(colors: &TerminalColors) -> ThemeColors {
    ThemeColors::from_ansi16(colors.foreground, colors.background, colors.palette)
}

fn clamp_opacity(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// One per GPUI window. Holds shared, app-wide terminal config.
pub struct WindowsGhosttyApp {
    config: Mutex<RendererConfig>,
}

impl WindowsGhosttyApp {
    pub fn new(appearance: &AppearanceConfig) -> Result<Self, String> {
        let mut config = RendererConfig::default();
        config.font_family = appearance.font_family.clone();
        config.font_size_px = appearance.font_size;
        let theme = theme_from_colors(&appearance.colors);
        config.clear_color = [
            appearance.colors.background[0] as f32 / 255.0,
            appearance.colors.background[1] as f32 / 255.0,
            appearance.colors.background[2] as f32 / 255.0,
            1.0,
        ];
        config.theme = Some(theme);
        config.background_opacity = clamp_opacity(appearance.background_opacity);
        Ok(Self {
            config: Mutex::new(config),
        })
    }

    pub fn tick(&self) {}

    /// Stub for parity with the macOS `GhosttyApp::wake_generation`.
    pub fn wake_generation(&self) -> u64 {
        0
    }

    pub fn update_colors(&self, _colors: &TerminalColors) -> Result<(), String> {
        Ok(())
    }

    pub fn update_appearance(&self, appearance: &AppearanceConfig) -> Result<(), String> {
        let theme = theme_from_colors(&appearance.colors);
        let mut config = self.config.lock();
        config.font_family = appearance.font_family.clone();
        config.font_size_px = appearance.font_size;
        config.clear_color = [
            appearance.colors.background[0] as f32 / 255.0,
            appearance.colors.background[1] as f32 / 255.0,
            appearance.colors.background[2] as f32 / 255.0,
            1.0,
        ];
        config.theme = Some(theme);
        config.background_opacity = clamp_opacity(appearance.background_opacity);
        Ok(())
    }

    pub fn update_config(&self, _patch: &GhosttyConfigPatch) -> Result<(), String> {
        Ok(())
    }

    pub fn set_color_scheme(&self, _dark: bool) {}

    /// Snapshot the current renderer config — used by `WindowsTerminalView`
    /// when constructing a new `RenderSession`.
    pub fn renderer_config(&self) -> RendererConfig {
        self.config.lock().clone()
    }
}

unsafe impl Send for WindowsGhosttyApp {}
unsafe impl Sync for WindowsGhosttyApp {}

/// One per pane. The `RenderSession` (renderer + VT + ConPTY) is
/// constructed lazily by the GPUI view once it has a size and DPI.
pub struct WindowsGhosttyTerminal {
    inner: Arc<Mutex<Option<RenderSession>>>,
}

impl WindowsGhosttyTerminal {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Attach a live `RenderSession`. Idempotent: replaces any existing one.
    pub fn attach(&self, session: RenderSession) {
        *self.inner.lock() = Some(session);
    }

    pub fn is_attached(&self) -> bool {
        self.inner.lock().is_some()
    }

    /// Accessor for the GPUI view to reach into the session for render
    /// / input methods. Guarded by the same Mutex that protects attach.
    pub fn inner(&self) -> Arc<Mutex<Option<RenderSession>>> {
        self.inner.clone()
    }

    pub fn draw(&self) {}
    pub fn refresh(&self) {}
    pub fn set_size(&self, _w: u32, _h: u32) {}

    pub fn size(&self) -> SurfaceSize {
        SurfaceSize {
            columns: 0,
            rows: 0,
            width_px: 0,
            height_px: 0,
            cell_width_px: 0,
            cell_height_px: 0,
        }
    }

    pub fn scrollbar(&self) -> Option<GhosttyScrollbar> {
        self.inner.lock().as_ref().and_then(|s| s.scrollbar())
    }

    pub fn set_content_scale(&self, _scale: f64) {}
    pub fn set_focus(&self, _focused: bool) {}
    pub fn set_occlusion(&self, _occluded: bool) {}
    pub fn set_color_scheme(&self, _dark: bool) {}
    pub fn perform_binding_action(&self, _action: &str) -> Result<bool, String> {
        Ok(false)
    }
    pub fn clear_screen_and_scrollback(&self) -> Result<(), String> {
        if let Some(session) = self.inner.lock().as_ref() {
            session.clear_screen_and_scrollback();
        }
        Ok(())
    }
    pub fn request_split(&self, _direction: GhosttySplitDirection) {}
    pub fn update_config(&self, _patch: &GhosttyConfigPatch) -> Result<(), String> {
        Ok(())
    }

    pub fn update_appearance(&self, appearance: &AppearanceConfig) -> Result<(), String> {
        if let Some(session) = self.inner.lock().as_ref() {
            let theme = theme_from_colors(&appearance.colors);
            session.set_appearance(Some(&theme), clamp_opacity(appearance.background_opacity));
        }
        Ok(())
    }

    pub fn write_to_pty(&self, data: &[u8]) {
        if let Some(session) = self.inner.lock().as_ref() {
            if let Ok(s) = std::str::from_utf8(data) {
                session.write_input(s);
            }
        }
    }

    pub fn send_text(&self, text: &str) {
        if let Some(session) = self.inner.lock().as_ref() {
            session.write_input(text);
        }
    }

    pub fn send_mouse_button(&self, _pressed: bool, _button: MouseButton, _mods: i32) -> bool {
        false
    }
    pub fn send_mouse_pos(&self, _x: f64, _y: f64, _mods: i32) {}
    pub fn send_mouse_scroll(&self, _x: f64, _y: f64, _mods: i32) {}
    pub fn request_close(&self) {
        *self.inner.lock() = None;
    }

    pub fn title(&self) -> Option<String> {
        None
    }
    pub fn current_dir(&self) -> Option<String> {
        self.inner
            .lock()
            .as_ref()
            .and_then(RenderSession::current_dir)
    }
    pub fn is_alive(&self) -> bool {
        match self.inner.lock().as_ref() {
            Some(session) => session.is_alive(),
            None => true,
        }
    }
    pub fn is_busy(&self) -> bool {
        false
    }
    pub fn command_history(&self) -> Vec<CommandRecord> {
        Vec::new()
    }
    pub fn take_command_finished(&self) -> Option<CommandFinishedSignal> {
        None
    }
    pub fn last_exit_code(&self) -> Option<i32> {
        None
    }
    pub fn last_command_duration(&self) -> Option<Duration> {
        None
    }
    pub fn input_generation(&self) -> u64 {
        0
    }
    pub fn last_command_finished_input_generation(&self) -> u64 {
        0
    }
    pub fn recover_shell_prompt_state(&self) {}
    pub fn has_selection(&self) -> bool {
        self.inner
            .lock()
            .as_ref()
            .is_some_and(|s| s.has_selection())
    }
    pub fn selection_text(&self) -> Option<String> {
        self.inner.lock().as_ref().and_then(|s| s.selection_text())
    }
    pub fn clear_selection(&self) {
        if let Some(session) = self.inner.lock().as_ref() {
            session.clear_selection();
        }
    }
    pub fn read_screen_text(&self, max_lines: usize) -> Vec<String> {
        self.inner
            .lock()
            .as_ref()
            .map(|session| session.read_screen_text(max_lines))
            .unwrap_or_default()
    }
    pub fn read_recent_lines(&self, max_lines: usize) -> Vec<String> {
        self.inner
            .lock()
            .as_ref()
            .map(|session| session.read_recent_lines(max_lines))
            .unwrap_or_default()
    }
    pub fn search_text(&self, pattern: &str, limit: usize) -> Vec<(usize, String)> {
        self.inner
            .lock()
            .as_ref()
            .map(|session| session.search_text(pattern, limit))
            .unwrap_or_default()
    }
    pub fn take_needs_render(&self) -> bool {
        false
    }
    pub fn take_pending_events(&self) -> Vec<GhosttySurfaceEvent> {
        Vec::new()
    }
}

impl Default for WindowsGhosttyTerminal {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for WindowsGhosttyTerminal {}
unsafe impl Sync for WindowsGhosttyTerminal {}
