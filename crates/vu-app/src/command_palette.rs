use gpui::*;
use gpui_component::input::InputState;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{ActiveTheme, input::Input};

use crate::motion::{MotionValue, vertical_reveal_offset};

actions!(command_palette, [ToggleCommandPalette]);

/// A command palette action entry
#[derive(Clone)]
struct PaletteAction {
    id: &'static str,
    label: &'static str,
    /// GPUI action whose live keymap binding is shown as the shortcut.
    /// Resolved at render time so user keybinding overrides show up.
    action: Option<&'static dyn Action>,
    /// Shortcut hint for entries handled outside the GPUI keymap.
    /// Ignored when `action` is set.
    shortcut: &'static str,
    category: &'static str,
}

const PALETTE_ACTIONS: &[PaletteAction] = &[
    PaletteAction {
        id: "new-window",
        label: "New Window",
        action: Some(&crate::NewWindow),
        shortcut: "",
        category: "App",
    },
    #[cfg(target_os = "macos")]
    PaletteAction {
        id: "minimize-window",
        label: "Minimize Window",
        action: Some(&crate::Minimize),
        shortcut: "",
        category: "App",
    },
    PaletteAction {
        id: "new-tab",
        label: "New Tab",
        action: Some(&crate::NewTab),
        shortcut: "",
        category: "Terminal",
    },
    PaletteAction {
        id: "export-workspace-layout",
        label: "Save Layout Profile",
        action: None,
        shortcut: "",
        category: "Workspace",
    },
    PaletteAction {
        id: "add-workspace-layout-tabs",
        label: "Add Tabs from Layout Profile",
        action: None,
        shortcut: "",
        category: "Workspace",
    },
    PaletteAction {
        id: "open-workspace-layout-window",
        label: "Open Layout Profile in New Window",
        action: None,
        shortcut: "",
        category: "Workspace",
    },
    PaletteAction {
        id: "next-tab",
        label: "Next Tab",
        action: Some(&crate::NextTab),
        shortcut: "",
        category: "Terminal",
    },
    PaletteAction {
        id: "previous-tab",
        label: "Previous Tab",
        action: Some(&crate::PreviousTab),
        shortcut: "",
        category: "Terminal",
    },
    PaletteAction {
        id: "close-tab",
        label: "Close Tab",
        action: Some(&crate::CloseTab),
        shortcut: "",
        category: "Terminal",
    },
    PaletteAction {
        id: "clear-terminal",
        label: "Clear Terminal",
        action: None,
        shortcut: "secondary-k",
        category: "Terminal",
    },
    PaletteAction {
        id: "clear-restored-terminal-history",
        label: "Clear Restored Terminal History",
        action: None,
        shortcut: "",
        category: "Privacy",
    },
    PaletteAction {
        id: "focus-terminal",
        label: "Focus Terminal",
        action: None,
        shortcut: "",
        category: "Terminal",
    },
    PaletteAction {
        id: "split-right",
        label: "Split Right",
        action: Some(&crate::SplitRight),
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "split-down",
        label: "Split Down",
        action: Some(&crate::SplitDown),
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "split-left",
        label: "Split Left",
        action: None,
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "split-up",
        label: "Split Up",
        action: None,
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "toggle-pane-zoom",
        label: "Toggle Pane Zoom",
        action: Some(&crate::TogglePaneZoom),
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "focus-next-pane",
        label: "Focus Next Pane",
        action: Some(&crate::FocusNextPane),
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "focus-previous-pane",
        label: "Focus Previous Pane",
        action: Some(&crate::FocusPreviousPane),
        shortcut: "",
        category: "Pane",
    },
    PaletteAction {
        id: "new-surface",
        label: "New Surface Tab",
        action: Some(&crate::NewSurface),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "new-surface-split-right",
        label: "New Surface Pane Right",
        action: Some(&crate::NewSurfaceSplitRight),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "new-surface-split-down",
        label: "New Surface Pane Down",
        action: Some(&crate::NewSurfaceSplitDown),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "next-surface",
        label: "Next Surface Tab",
        action: Some(&crate::NextSurface),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "previous-surface",
        label: "Previous Surface Tab",
        action: Some(&crate::PreviousSurface),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "rename-surface",
        label: "Rename Current Surface",
        action: Some(&crate::RenameSurface),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "close-surface",
        label: "Close Current Surface",
        action: Some(&crate::CloseSurface),
        shortcut: "",
        category: "Surface",
    },
    PaletteAction {
        id: "toggle-input-bar",
        label: "Toggle Input Bar",
        action: Some(&crate::ToggleInputBar),
        shortcut: "",
        category: "View",
    },
    PaletteAction {
        id: "toggle-left-sidebar",
        label: "Toggle Left Sidebar",
        action: Some(&crate::ToggleLeftPanel),
        shortcut: "",
        category: "View",
    },
    PaletteAction {
        id: "focus-files",
        label: "Focus Files",
        action: Some(&crate::FocusFiles),
        shortcut: "",
        category: "Workspace",
    },
    PaletteAction {
        id: "search-files",
        label: "Search Files",
        action: Some(&crate::SearchFiles),
        shortcut: "",
        category: "Workspace",
    },
    PaletteAction {
        id: "settings",
        label: "Open Settings",
        action: Some(&crate::settings_panel::ToggleSettings),
        shortcut: "",
        category: "Settings",
    },
    PaletteAction {
        id: "check-for-updates",
        label: "Check for Updates",
        action: None,
        shortcut: "",
        category: "App",
    },
    PaletteAction {
        id: "quit",
        label: "Quit",
        action: Some(&crate::Quit),
        shortcut: "",
        category: "App",
    },
];

/// Command palette overlay — searchable action list
pub struct CommandPalette {
    visible: bool,
    query: Entity<InputState>,
    query_text: String,
    selected_index: usize,
    reveal_selected: bool,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    ui_opacity: f32,
    overlay_motion: MotionValue,
}

/// Emitted when the user selects an action
pub struct PaletteSelect {
    pub action_id: String,
}

/// Emitted when the palette is dismissed without selecting an action.
pub struct PaletteDismissed;

impl EventEmitter<PaletteSelect> for CommandPalette {}
impl EventEmitter<PaletteDismissed> for CommandPalette {}

impl CommandPalette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_placeholder("Search commands", window, cx);
            state
        });

        Self {
            visible: false,
            query,
            query_text: String::new(),
            selected_index: 0,
            reveal_selected: false,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            ui_opacity: 0.90,
            overlay_motion: MotionValue::new(0.0),
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            self.show(window, cx);
        } else {
            self.visible = false;
            self.overlay_motion
                .set_target(0.0, std::time::Duration::from_millis(150));
            cx.notify();
        }
    }

    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_visible = self.visible;
        self.visible = true;
        self.overlay_motion
            .set_target(1.0, std::time::Duration::from_millis(180));
        if !was_visible {
            self.query_text.clear();
            self.selected_index = 0;
            self.reveal_selected = true;
            self.query.update(cx, |s, cx| {
                s.set_value("", window, cx);
            });
        }
        self.query.update(cx, |s, cx| {
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.overlay_motion
            .set_target(0.0, std::time::Duration::from_millis(150));
        cx.emit(PaletteDismissed);
        cx.notify();
    }

    pub fn is_visible(&self) -> bool {
        self.visible || self.overlay_motion.is_animating()
    }

    pub fn set_ui_opacity(&mut self, opacity: f32) {
        self.ui_opacity = opacity.clamp(0.35, 1.0);
    }

    fn filtered_actions(&self) -> Vec<&PaletteAction> {
        if self.query_text.is_empty() {
            return PALETTE_ACTIONS.iter().collect();
        }
        let query = self.query_text.to_lowercase();
        PALETTE_ACTIONS
            .iter()
            .filter(|a| {
                a.label.to_lowercase().contains(&query)
                    || a.category.to_lowercase().contains(&query)
                    || a.id.contains(&query)
            })
            .collect()
    }

    fn select_action(&mut self, cx: &mut Context<Self>) {
        let actions = self.filtered_actions();
        if let Some(action) = actions.get(self.selected_index) {
            let id = action.id.to_string();
            self.visible = false;
            self.overlay_motion
                .set_target(0.0, std::time::Duration::from_millis(150));
            cx.emit(PaletteSelect { action_id: id });
            cx.notify();
        }
    }
}

impl Focusable for CommandPalette {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Keycaps for an action row: the live keymap binding when the entry maps to
/// a GPUI action (so user overrides show), otherwise the static hint.
fn shortcut_keycaps(
    action: &PaletteAction,
    window: &Window,
    theme: &gpui_component::Theme,
) -> Option<AnyElement> {
    match action.action {
        Some(gpui_action) => crate::keycaps::first_action_keystroke(gpui_action, window)
            .map(|stroke| crate::keycaps::keycaps_for_stroke(&stroke, theme).into_any_element()),
        None if action.shortcut.is_empty() => None,
        None => Some(crate::keycaps::keycaps_for_binding(action.shortcut, theme)),
    }
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let overlay_progress = self.overlay_motion.value(window);
        if !self.visible && overlay_progress <= 0.001 {
            return div().id("palette-overlay");
        }

        // Read current query text from input state
        let previous_query = self.query_text.clone();
        self.query_text = self.query.read(cx).value().to_string();
        if self.query_text != previous_query {
            self.selected_index = 0;
            self.reveal_selected = true;
        }
        let actions = self.filtered_actions();
        let selected = if !actions.is_empty() {
            self.selected_index.min(actions.len().saturating_sub(1))
        } else {
            0
        };

        let theme = cx.theme();
        let selected_bg = theme.foreground.opacity(0.075);
        let selected_hover_bg = theme.foreground.opacity(0.095);
        let row_hover_bg = theme.foreground.opacity(0.045);
        let selected_marker = theme.primary.opacity(0.82);

        let mut list_content = div()
            .id("palette-list")
            .flex()
            .flex_col()
            .size_full()
            .py(px(8.0))
            .px(px(8.0))
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle);

        for (i, action) in actions.iter().enumerate() {
            let is_selected = i == selected;
            let idx = i;
            let shortcut = div()
                .min_w(px(96.0))
                .flex()
                .justify_end()
                .children(shortcut_keycaps(action, window, theme))
                .into_any_element();

            list_content = list_content.child(
                div()
                    .id(SharedString::from(format!("palette-{}", action.id)))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(8.0))
                    .cursor_pointer()
                    .bg(if is_selected {
                        selected_bg
                    } else {
                        theme.transparent
                    })
                    .text_color(theme.foreground)
                    .hover(move |s| {
                        s.bg(if is_selected {
                            selected_hover_bg
                        } else {
                            row_hover_bg
                        })
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.selected_index = idx;
                            this.select_action(cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        if this.selected_index != idx {
                            this.selected_index = idx;
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .gap(px(12.0))
                            .overflow_x_hidden()
                            .child(
                                div()
                                    .w(px(3.0))
                                    .h(px(18.0))
                                    .rounded(px(2.0))
                                    .flex_shrink_0()
                                    .bg(if is_selected {
                                        selected_marker
                                    } else {
                                        theme.transparent
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .line_height(px(14.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .flex_shrink_0()
                                    .text_color(if is_selected {
                                        theme.foreground.opacity(0.68)
                                    } else {
                                        theme.muted_foreground.opacity(0.58)
                                    })
                                    .min_w(px(78.0))
                                    .child(action.category.to_uppercase()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.0))
                                    .line_height(px(18.0))
                                    .font_weight(if is_selected {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .text_color(if is_selected {
                                        theme.foreground.opacity(0.94)
                                    } else {
                                        theme.foreground.opacity(0.82)
                                    })
                                    .child(action.label),
                            ),
                    )
                    .child(shortcut),
            );
        }

        if actions.is_empty() {
            list_content = list_content.child(
                div()
                    .px(px(16.0))
                    .py(px(20.0))
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .text_color(theme.muted_foreground.opacity(0.74))
                    .child("No matching commands"),
            );
        }

        if self.reveal_selected && !actions.is_empty() {
            self.scroll_handle.scroll_to_item(selected);
            self.reveal_selected = false;
        }

        let list = div()
            .relative()
            .max_h(px(360.0))
            .min_h_0()
            .child(list_content)
            .vertical_scrollbar(&self.scroll_handle);

        let backdrop = div()
            .id("palette-backdrop")
            .occlude()
            .absolute()
            .size_full()
            .bg(theme.background.opacity(0.72 * overlay_progress))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.dismiss(cx);
                }),
            );

        let card = div()
            .absolute()
            .top(px(60.0))
            .left_0()
            .right_0()
            .mx_auto()
            .w(px(560.0))
            .rounded(px(16.0))
            .bg(theme.popover.opacity(self.ui_opacity))
            .opacity(overlay_progress)
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .font_family(theme.font_family.clone())
            .pt(vertical_reveal_offset(overlay_progress, 16.0))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let actions = this.filtered_actions();
                let count = actions.len();
                match event.keystroke.key.as_str() {
                    "escape" => {
                        this.dismiss(cx);
                    }
                    "enter" => {
                        this.select_action(cx);
                    }
                    "up" => {
                        if count > 0 {
                            this.selected_index = if this.selected_index == 0 {
                                count - 1
                            } else {
                                this.selected_index - 1
                            };
                            this.reveal_selected = true;
                            cx.notify();
                        }
                    }
                    "down" => {
                        if count > 0 {
                            this.selected_index = (this.selected_index + 1) % count;
                            this.reveal_selected = true;
                            cx.notify();
                        }
                    }
                    _ => {}
                }
            }))
            .child(
                div().p(px(12.0)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .h(px(44.0))
                        .px(px(12.0))
                        .rounded(px(12.0))
                        .bg(theme.foreground.opacity(0.055))
                        .child(
                            svg()
                                .path("phosphor/magnifying-glass.svg")
                                .size(px(15.0))
                                .flex_shrink_0()
                                .text_color(theme.muted_foreground.opacity(0.72)),
                        )
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(&self.query)
                                    .appearance(false)
                                    .text_size(px(15.0))
                                    .line_height(px(20.0))
                                    .text_color(theme.foreground.opacity(0.90)),
                            ),
                        ),
                ),
            )
            .child(list);

        div()
            .id("palette-overlay")
            .absolute()
            .size_full()
            .child(backdrop)
            .child(card)
    }
}
