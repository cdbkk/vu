use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;

use super::pty::{LinuxPtyOptions, LinuxPtySession, LinuxWakeCallback};
use crate::stub::{
    CommandFinishedSignal, CommandRecord, GhosttyConfigPatch, GhosttySplitDirection,
    GhosttySurfaceEvent, MouseButton, SurfaceSize,
};
use crate::vt::ScreenSnapshot;
use crate::{AppearanceConfig, TerminalColors, Tweaks};

#[derive(Debug, Clone)]
pub struct LinuxBackendConfig {
    pub shell_program: Option<String>,
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub colors: Option<TerminalColors>,
    /// 0.0 (fully see-through) … 1.0 (opaque). Multiplied into the
    /// terminal pane's solid background fill so the GPUI window
    /// composites over the desktop / Wayland blur surface beneath.
    /// Mirrors the Windows backend's `RendererConfig.background_opacity`
    /// and the macOS pass-through to libghostty.
    pub background_opacity: f32,
    /// Whether the user opted into the Wayland `org_kde_kwin_blur`
    /// surface region (only honored on KDE Plasma). Stored so
    /// `LinuxGhosttyApp::backend_config()` consumers see the
    /// authoritative state. Has no effect on the per-cell paint
    /// itself — the `WindowBackgroundAppearance::Blurred` toggle is
    /// applied at the GPUI window level in `vu-app/main.rs`.
    pub background_blur: bool,
    pub tweaks: Tweaks,
}

impl Default for LinuxBackendConfig {
    fn default() -> Self {
        Self {
            shell_program: None,
            font_family: None,
            font_size: None,
            colors: None,
            background_opacity: 1.0,
            background_blur: false,
            tweaks: Tweaks::default(),
        }
    }
}

/// One per GPUI window. Holds Linux backend configuration that future
/// PTY and renderer setup can consume.
pub struct LinuxGhosttyApp {
    config: Mutex<LinuxBackendConfig>,
    wake_generation: Arc<AtomicU64>,
}

impl LinuxGhosttyApp {
    pub fn new(appearance: &AppearanceConfig) -> Result<Self, String> {
        Ok(Self {
            config: Mutex::new(LinuxBackendConfig {
                shell_program: default_linux_shell_program(),
                font_family: Some(appearance.font_family.clone()),
                font_size: Some(appearance.font_size),
                colors: Some(appearance.colors.clone()),
                background_opacity: clamp_opacity(appearance.background_opacity),
                background_blur: appearance.background_blur,
                tweaks: appearance.tweaks.clone(),
            }),
            wake_generation: Arc::new(AtomicU64::new(1)),
        })
    }

    pub fn tick(&self) {}

    pub fn wake_generation(&self) -> u64 {
        self.wake_generation.load(Ordering::Acquire)
    }

    pub fn update_colors(&self, colors: &TerminalColors) -> Result<(), String> {
        let mut config = self.config.lock();
        config.colors = Some(colors.clone());
        Ok(())
    }

    pub fn update_appearance(&self, appearance: &AppearanceConfig) -> Result<(), String> {
        let mut config = self.config.lock();
        config.font_family = Some(appearance.font_family.clone());
        config.font_size = Some(appearance.font_size);
        config.colors = Some(appearance.colors.clone());
        config.background_opacity = clamp_opacity(appearance.background_opacity);
        config.background_blur = appearance.background_blur;
        config.tweaks = appearance.tweaks.clone();
        Ok(())
    }

    /// Current background opacity (0.0..=1.0). The view multiplies
    /// this into its terminal-pane fill so per-pane translucency
    /// composites against the GPUI window's transparent or blurred
    /// background.
    pub fn background_opacity(&self) -> f32 {
        self.config.lock().background_opacity
    }

    pub fn update_config(&self, _patch: &GhosttyConfigPatch) -> Result<(), String> {
        Ok(())
    }

    pub fn set_color_scheme(&self, _dark: bool) {}

    pub fn backend_config(&self) -> LinuxBackendConfig {
        self.config.lock().clone()
    }

    pub fn default_pty_options(&self, cwd: Option<&str>) -> LinuxPtyOptions {
        let config = self.backend_config();
        LinuxPtyOptions {
            cwd: cwd.map(PathBuf::from),
            program: config.shell_program,
            wake_generation: Some(self.wake_generation.clone()),
            theme: config.colors,
            bold_is_bright: config.tweaks.bold_is_bright,
            ..LinuxPtyOptions::default()
        }
    }
}

unsafe impl Send for LinuxGhosttyApp {}
unsafe impl Sync for LinuxGhosttyApp {}

fn clamp_opacity(value: f32) -> f32 {
    // `f32::clamp` propagates NaN, which would then leak into the
    // pane fill alpha and downstream color math (every Hsla
    // multiplication also propagates NaN, so a malformed setting
    // could make the entire pane go black). Settings already
    // round-trip through serde validation in practice, but we get
    // the safety belt for free.
    if !value.is_finite() {
        return 1.0;
    }
    value.clamp(0.0, 1.0)
}

fn default_linux_shell_program() -> Option<String> {
    if let Some(shell) = std::env::var("VU_LINUX_SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(shell);
    }

    if let Some(shell) = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(shell);
    }

    for candidate in ["/bin/bash", "/usr/bin/bash", "/bin/sh", "/usr/bin/sh"] {
        if PathBuf::from(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

/// One per pane. The GPUI view attaches the PTY + VT session lazily
/// once the pane has real bounds.
pub struct LinuxGhosttyTerminal {
    inner: Arc<Mutex<Option<LinuxPtySession>>>,
    wake_callback: Arc<Mutex<Option<LinuxWakeCallback>>>,
}

impl LinuxGhosttyTerminal {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            wake_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub fn attach(&self, session: LinuxPtySession) {
        *self.inner.lock() = Some(session);
    }

    pub fn is_attached(&self) -> bool {
        self.inner.lock().is_some()
    }

    pub fn spawn_with_options(&self, options: LinuxPtyOptions) -> Result<(), String> {
        let mut options = options;
        if options.wake_callback.is_none() {
            options.wake_callback = self.wake_callback.lock().clone();
        }
        let session = LinuxPtySession::spawn(options).map_err(|err| err.to_string())?;
        self.attach(session);
        Ok(())
    }

    /// Set the callback used by subsequent `spawn_with_options` calls.
    /// Existing PTY sessions keep the callback captured at spawn time.
    pub fn set_wake_callback(&self, callback: Option<LinuxWakeCallback>) {
        *self.wake_callback.lock() = callback;
    }

    pub fn inner(&self) -> Arc<Mutex<Option<LinuxPtySession>>> {
        self.inner.clone()
    }

    pub fn draw(&self) {}

    pub fn refresh(&self) {}

    pub fn set_size(&self, width_px: u32, height_px: u32) {
        if let Some(session) = self.inner.lock().as_ref() {
            if let Err(err) = session.set_pixel_size(width_px, height_px) {
                log::debug!("linux pty pixel resize failed: {err:#}");
            }
        }
    }

    pub fn resize_surface(&self, size: SurfaceSize) {
        if let Some(session) = self.inner.lock().as_ref() {
            if let Err(err) = session.resize(size) {
                log::debug!("linux pty resize failed: {err:#}");
            }
        }
    }

    pub fn size(&self) -> SurfaceSize {
        self.inner
            .lock()
            .as_ref()
            .map(LinuxPtySession::size)
            .unwrap_or(SurfaceSize {
                columns: 0,
                rows: 0,
                width_px: 0,
                height_px: 0,
                cell_width_px: 0,
                cell_height_px: 0,
            })
    }

    pub fn set_content_scale(&self, _scale: f64) {}
    pub fn set_focus(&self, _focused: bool) {}
    pub fn set_occlusion(&self, _occluded: bool) {}
    pub fn set_color_scheme(&self, dark: bool) {
        if let Some(session) = self.inner.lock().as_ref() {
            session.set_dark_mode(dark);
        }
    }

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
            session.set_theme(&appearance.colors);
            session.set_bold_is_bright(appearance.tweaks.bold_is_bright);
        }
        Ok(())
    }

    /// Returns the current libghostty-vt screen snapshot, if a PTY
    /// session has been spawned. Used by `vu-app/src/linux_view.rs`
    /// to drive the styled-cell paint path.
    pub fn snapshot(&self) -> Option<ScreenSnapshot> {
        self.inner.lock().as_ref().map(LinuxPtySession::snapshot)
    }

    pub fn write_to_pty(&self, data: &[u8]) {
        if let Some(session) = self.inner.lock().as_ref() {
            if let Err(err) = session.write_input(data) {
                log::debug!("linux pty write failed: {err:#}");
            }
        }
    }

    pub fn send_text(&self, text: &str) {
        self.write_to_pty(text.as_bytes());
    }

    pub fn is_bracketed_paste(&self) -> bool {
        self.inner
            .lock()
            .as_ref()
            .is_some_and(LinuxPtySession::is_bracketed_paste)
    }

    pub fn is_decckm(&self) -> bool {
        self.inner
            .lock()
            .as_ref()
            .is_some_and(LinuxPtySession::is_decckm)
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
        self.inner.lock().as_ref().and_then(LinuxPtySession::title)
    }

    pub fn current_dir(&self) -> Option<String> {
        self.inner
            .lock()
            .as_ref()
            .and_then(LinuxPtySession::current_dir)
    }

    pub fn is_alive(&self) -> bool {
        self.inner
            .lock()
            .as_ref()
            .is_some_and(LinuxPtySession::is_alive)
    }

    pub fn is_busy(&self) -> bool {
        false
    }

    pub fn command_history(&self) -> Vec<CommandRecord> {
        Vec::new()
    }

    pub fn take_command_finished(&self) -> Option<CommandFinishedSignal> {
        self.inner
            .lock()
            .as_ref()
            .and_then(LinuxPtySession::take_command_finished)
    }

    pub fn last_exit_code(&self) -> Option<i32> {
        self.inner
            .lock()
            .as_ref()
            .and_then(LinuxPtySession::last_exit_code)
    }

    pub fn last_command_duration(&self) -> Option<Duration> {
        self.inner
            .lock()
            .as_ref()
            .and_then(LinuxPtySession::last_command_duration)
    }

    pub fn input_generation(&self) -> u64 {
        self.inner
            .lock()
            .as_ref()
            .map(LinuxPtySession::input_generation)
            .unwrap_or(0)
    }

    pub fn last_command_finished_input_generation(&self) -> u64 {
        self.input_generation()
    }

    pub fn recover_shell_prompt_state(&self) {}

    pub fn has_selection(&self) -> bool {
        false
    }

    pub fn selection_text(&self) -> Option<String> {
        None
    }

    pub fn clear_selection(&self) {}

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
        self.inner
            .lock()
            .as_ref()
            .is_some_and(LinuxPtySession::take_needs_render)
    }

    pub fn take_pending_events(&self) -> Vec<GhosttySurfaceEvent> {
        Vec::new()
    }
}

impl Default for LinuxGhosttyTerminal {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl Send for LinuxGhosttyTerminal {}
unsafe impl Sync for LinuxGhosttyTerminal {}
