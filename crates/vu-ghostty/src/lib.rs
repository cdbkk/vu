//! Rust wrapper around libghostty's embedded C API.
//!
//! Backend selection per target:
//!
//! | target | backend | source |
//! |---|---|---|
//! | macOS | full libghostty (Metal + AppKit NSView) | `terminal.rs` + `ffi.rs` |
//! | Windows | libghostty-vt + ConPTY + D3D11 + DirectWrite, hosted via GPUI image composition | `windows/` |
//! | Linux | local backend scaffold (Unix PTY + future GPUI-owned renderer) | `linux/` |
//! | other | no-op stub (UI compiles, terminal pane shows placeholder) | `stub.rs` |
//!
//! All backends expose the same public type names — `GhosttyApp`,
//! `GhosttyTerminal`, `TerminalColors`, etc. — so cross-platform UI
//! code in `vu-app` consumes them without per-callsite cfg gates.

// Suppress warnings from objc 0.2's `sel_impl!` and `class!` macros.
#![allow(unexpected_cfgs)]

/// Palette colors passed to a terminal backend.
#[derive(Debug, Clone)]
pub struct TerminalColors {
    pub foreground: [u8; 3],
    pub background: [u8; 3],
    pub palette: [[u8; 3]; 16],
}

/// Structured terminal rendering tweaks shared by every backend.
#[derive(Debug, Clone, PartialEq)]
pub struct Tweaks {
    pub line_height_percent: f32,
    pub letter_spacing_percent: f32,
    pub ligatures: bool,
    pub font_thicken: bool,
    pub cursor_blink: bool,
    pub bold_is_bright: bool,
    pub minimum_contrast: f32,
    pub unfocused_split_opacity: f32,
    pub window_padding_x: f32,
    pub window_padding_y: f32,
    pub mouse_hide_while_typing: bool,
    pub selection_background: Option<String>,
    pub selection_foreground: Option<String>,
}

impl Default for Tweaks {
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

/// Complete appearance state passed across the app/backend boundary.
#[derive(Debug, Clone)]
pub struct AppearanceConfig {
    pub colors: TerminalColors,
    pub font_family: String,
    pub font_size: f32,
    pub background_opacity: f32,
    pub background_blur: bool,
    pub cursor_style: String,
    pub background_image: Option<String>,
    pub background_image_opacity: f32,
    pub background_image_position: Option<String>,
    pub background_image_fit: Option<String>,
    pub background_image_repeat: bool,
    pub tweaks: Tweaks,
}

pub fn restored_terminal_output_text(lines: &[String]) -> Option<String> {
    if lines.is_empty() {
        return None;
    }

    let mut output = String::new();
    for line in lines {
        for ch in line.chars() {
            if ch == '\t' || !ch.is_control() {
                output.push(ch);
            }
        }
        output.push_str("\r\n");
    }

    (!output.trim().is_empty()).then_some(output)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod transcript;

#[cfg(target_os = "macos")]
pub mod ffi;
#[cfg(target_os = "macos")]
pub mod terminal;

// `stub` defines non-appearance shared shapes (GhosttySplitDirection,
// MouseButton, etc.) used by the Windows and Linux facades. On macOS the stub
// module isn't compiled because those types come from `terminal.rs`.
#[cfg(not(target_os = "macos"))]
pub mod stub;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub mod vt;

#[cfg(target_os = "windows")]
pub mod windows;

// ── Re-exports per platform ────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub use terminal::{
    CommandFinishedSignal, CommandRecord, GhosttyApp, GhosttyConfigPatch, GhosttyScrollbar,
    GhosttySplitDirection, GhosttySurfaceEvent, GhosttyTerminal, MouseButton, SurfaceSize,
    TerminalState,
};

#[cfg(target_os = "windows")]
pub use stub::{
    CommandFinishedSignal, CommandRecord, GhosttyConfigPatch, GhosttyScrollbar,
    GhosttySplitDirection, GhosttySurfaceEvent, MouseButton, SurfaceSize,
};
#[cfg(target_os = "windows")]
pub use windows::{WindowsGhosttyApp as GhosttyApp, WindowsGhosttyTerminal as GhosttyTerminal};

#[cfg(target_os = "linux")]
pub use linux::{LinuxGhosttyApp as GhosttyApp, LinuxGhosttyTerminal as GhosttyTerminal};
#[cfg(target_os = "linux")]
pub use stub::{
    CommandFinishedSignal, CommandRecord, GhosttyConfigPatch, GhosttyScrollbar,
    GhosttySplitDirection, GhosttySurfaceEvent, MouseButton, SurfaceSize,
};
/// Re-exports for the Linux GPUI-owned terminal renderer in
/// `vu-app/src/linux_view.rs`. These types are part of the cross-
/// platform `vt` parser surface and are stable enough for the view
/// to consume directly while we iterate on the Linux paint path.
#[cfg(target_os = "linux")]
pub use vt::{
    ATTR_BOLD, ATTR_INVERSE, ATTR_ITALIC, ATTR_STRIKE, ATTR_UNDERLINE, Cell as VtCell,
    Cursor as VtCursor, ScreenSnapshot,
};

#[cfg(all(
    not(target_os = "macos"),
    not(target_os = "windows"),
    not(target_os = "linux")
))]
pub use stub::{
    CommandFinishedSignal, CommandRecord, GhosttyApp, GhosttyConfigPatch, GhosttyScrollbar,
    GhosttySplitDirection, GhosttySurfaceEvent, GhosttyTerminal, MouseButton, SurfaceSize,
};
