use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};

use crate::pane_context::{
    PaneActionRecord, PaneFrontState, PaneMode, PaneShellContext, RemoteWorkspaceAnchor,
};
use crate::pane_control::{
    PaneAddressSpace, PaneControlCapability, PaneControlChannel, PaneProtocolAttachment,
    PaneVisibleTarget, TmuxControlState,
};
use crate::shell_probe::ShellProbeResult;
use crate::tmux::{TmuxCapture, TmuxExecLocation, TmuxExecResult, TmuxSnapshot};

/// Request to execute a command in a visible terminal pane.
/// When `pane_index` is None, targets the focused pane.
#[derive(Debug)]
pub struct TerminalExecRequest {
    pub command: String,
    pub working_dir: Option<String>,
    pub target: PaneSelector,
    pub response_tx: Sender<TerminalExecResponse>,
}

/// Response from a visible terminal execution.
#[derive(Debug, Clone)]
pub struct TerminalExecResponse {
    pub output: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaneCreateLocation {
    Right,
    Down,
}

impl PaneCreateLocation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Down => "down",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct PaneSelector {
    pub pane_index: Option<usize>,
    pub pane_id: Option<usize>,
}

impl PaneSelector {
    pub const fn new(pane_index: Option<usize>, pane_id: Option<usize>) -> Self {
        Self {
            pane_index,
            pane_id,
        }
    }

    pub fn describe(self) -> String {
        match (self.pane_index, self.pane_id) {
            (Some(index), Some(id)) => format!("pane {index} (id {id})"),
            (Some(index), None) => format!("pane {index}"),
            (None, Some(id)) => format!("pane id {id}"),
            (None, None) => "focused pane".to_string(),
        }
    }
}

// Tool → PaneRequest → Workspace → PaneResponse → Tool

/// Metadata about a single terminal pane.
///
/// Includes runtime and control state so clients do not execute against
/// the wrong target or over-trust missing backend facts.
#[derive(Debug, Clone, Serialize)]
pub struct PaneInfo {
    pub index: usize,
    /// Stable pane id within the current tab lifetime.
    pub pane_id: usize,
    pub title: String,
    pub cwd: Option<String>,
    pub is_focused: bool,
    pub rows: usize,
    pub cols: usize,
    /// Whether the embedded Ghostty surface has actually been initialized.
    pub surface_ready: bool,
    /// Whether the PTY child process is still running.
    pub is_alive: bool,
    /// Proven hostname when the backend can actually supply one.
    pub hostname: Option<String>,
    /// Confidence for the effective hostname, when detected.
    pub hostname_confidence: Option<crate::pane_context::PaneConfidence>,
    /// Evidence source for the effective hostname, when detected.
    pub hostname_source: Option<crate::pane_context::PaneEvidenceSource>,
    /// Best current remote SSH workspace anchor for this pane.
    pub remote_workspace: Option<RemoteWorkspaceAnchor>,
    /// Current verified front-state for the pane.
    pub front_state: PaneFrontState,
    /// Current pane mode: shell, tmux-like multiplexer, or another TUI.
    pub mode: PaneMode,
    /// Whether shell metadata like cwd and last_command is likely fresh for the visible app.
    pub shell_metadata_fresh: bool,
    /// Whether the most recent typed shell-context snapshot still matches the current shell frame.
    pub shell_context_fresh: bool,
    /// What the backend can authoritatively observe for this pane today.
    pub observation_support: crate::pane_context::PaneObservationSupport,
    /// The only address space valid for pane_index today.
    pub address_space: PaneAddressSpace,
    /// The best-known visible target inside this vu pane.
    pub visible_target: PaneVisibleTarget,
    /// Nested runtime/control targets from outer shell toward the front-most visible app.
    pub target_stack: Vec<PaneVisibleTarget>,
    /// tmux adapter state when a tmux layer is present in this pane.
    pub tmux_control: Option<TmuxControlState>,
    /// Explicit protocol attachments currently available on this pane.
    pub control_attachments: Vec<PaneProtocolAttachment>,
    /// Control channels vu can use on this pane.
    pub control_channels: Vec<PaneControlChannel>,
    /// Capabilities currently available on this pane.
    pub control_capabilities: Vec<PaneControlCapability>,
    /// Addressing and control notes clients should respect.
    pub control_notes: Vec<String>,
    /// The top-most active runtime scope, when detected.
    pub active_scope: Option<crate::pane_context::PaneRuntimeScope>,
    /// Evidence behind the current runtime summary.
    pub evidence: Vec<crate::pane_context::PaneEvidence>,
    /// Structured runtime scopes inferred from pane-local evidence.
    pub runtime_stack: Vec<crate::pane_context::PaneRuntimeScope>,
    /// Last verified shell-frame stack captured for this pane.
    pub last_verified_runtime_stack: Vec<crate::pane_context::PaneRuntimeScope>,
    /// Warnings about stale or advisory runtime metadata.
    pub runtime_warnings: Vec<String>,
    /// Last typed shell-context snapshot captured for this pane.
    pub shell_context: Option<PaneShellContext>,
    /// Recent vu-originated actions for this pane.
    pub recent_actions: Vec<PaneActionRecord>,
    /// Weak observation hints derived from the current visible screen snapshot.
    pub screen_hints: Vec<crate::pane_context::PaneObservationHint>,
    /// tmux session hint when detected from the pane itself.
    pub tmux_session: Option<String>,
    /// Whether shell integration (OSC 133) is active.
    pub has_shell_integration: bool,
    /// Most recent command text when the backend can prove it.
    pub last_command: Option<String>,
    /// Exit code of the last command.
    pub last_exit_code: Option<i32>,
    /// A command is currently executing (between OSC 133 C and D).
    /// Only reliable when has_shell_integration is true.
    pub is_busy: bool,
}

/// A request from a pane tool to the workspace.
#[derive(Debug)]
pub struct PaneRequest {
    pub query: PaneQuery,
    pub response_tx: Sender<PaneResponse>,
}

/// Pane query types — the workspace interprets these against PaneTree/Grid.
#[derive(Debug)]
pub enum PaneQuery {
    /// List all panes with metadata.
    List,
    /// Read recent output from a specific pane.
    ReadContent { target: PaneSelector, lines: usize },
    /// Send raw keystrokes to a specific pane (for TUI interaction, Ctrl-C, etc.).
    SendKeys { target: PaneSelector, keys: String },
    /// Search scrollback + visible screen for a text pattern.
    SearchText {
        target: PaneSelector,
        pattern: String,
        max_matches: usize,
    },
    /// Return tmux adapter state for a pane whose target stack contains tmux.
    InspectTmux { target: PaneSelector },
    /// Query tmux windows/panes through a same-session tmux control anchor.
    TmuxList { pane: PaneSelector },
    /// Capture pane content from a tmux pane target through a same-session tmux control anchor.
    TmuxCapture {
        pane: PaneSelector,
        target: Option<String>,
        lines: usize,
    },
    /// Send literal text or tmux key names to a tmux pane target through a same-session tmux control anchor.
    TmuxSendKeys {
        pane: PaneSelector,
        target: String,
        literal_text: Option<String>,
        key_names: Vec<String>,
        append_enter: bool,
    },
    /// Run a command through tmux itself by creating a new tmux target.
    TmuxRunCommand {
        pane: PaneSelector,
        target: Option<String>,
        location: TmuxExecLocation,
        command: String,
        window_name: Option<String>,
        cwd: Option<String>,
        detached: bool,
    },
    /// Run a read-only shell-scoped probe in a pane with a proven fresh shell prompt.
    ProbeShellContext { target: PaneSelector },
    /// Lightweight busy check for a single pane (used by wait_for polling).
    /// Returns only is_busy + has_shell_integration, avoiding full List forensics.
    CheckBusy { target: PaneSelector },
    /// Wait for a pane to become idle or match a pattern.
    WaitFor {
        target: PaneSelector,
        timeout_secs: Option<u64>,
        pattern: Option<String>,
    },
    /// Create a new terminal pane (tab), optionally running a command in it.
    CreatePane {
        command: Option<String>,
        location: PaneCreateLocation,
    },
}

/// Response from the workspace to a pane tool.
#[derive(Debug, Clone)]
pub enum PaneResponse {
    PaneList(Vec<PaneInfo>),
    Content(String),
    KeysSent,
    TmuxInfo(TmuxControlState),
    TmuxList(TmuxSnapshot),
    TmuxCapture(TmuxCapture),
    TmuxExec(TmuxExecResult),
    ShellProbe(ShellProbeResult),
    /// Search results: Vec of (pane_index, line_number, line_text).
    SearchResults(Vec<(usize, usize, String)>),
    /// Lightweight busy-check response.
    BusyStatus {
        surface_ready: bool,
        is_alive: bool,
        is_busy: bool,
        has_shell_integration: bool,
    },
    /// Response from a wait_for operation.
    WaitComplete {
        status: String,
        output: String,
    },
    /// A new pane was created successfully.
    PaneCreated {
        pane_index: usize,
        pane_id: usize,
        surface_ready: bool,
        is_alive: bool,
        has_shell_integration: bool,
    },
    Error(String),
}
