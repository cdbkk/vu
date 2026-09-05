use super::*;

impl VuWorkspace {
    pub fn from_session(
        config: Config,
        session: Session,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let window_width = window.bounds().size.width.as_f32();
        let initial_sidebar_width = SessionSidebar::clamped_panel_width(
            session.left_panel_width.unwrap_or(PANEL_MIN_WIDTH * 1.25),
            max_sidebar_panel_width(window_width, 0.0),
        );
        let sidebar = cx.new(|cx| {
            let mut s = SessionSidebar::new(cx);
            s.set_panel_width(initial_sidebar_width, cx);
            s.set_pinned(session.vertical_tabs_pinned.unwrap_or(false), cx);
            s
        });
        let terminal_font_family = sanitize_terminal_font_family(&config.terminal.font_family);
        let ui_font_family = config.appearance.ui_font_family.clone();
        let ui_font_size = config.appearance.ui_font_size;
        let font_size = config.terminal.font_size;
        let terminal_cursor_style = config.terminal.cursor_style.clone();
        let terminal_opacity = Self::effective_terminal_opacity(config.appearance.terminal_opacity);
        let terminal_blur = Self::effective_terminal_blur(config.appearance.terminal_blur);
        let ui_opacity = Self::clamp_ui_opacity(config.appearance.ui_opacity);
        let tab_accent_inactive_alpha = config.appearance.tab_accent_inactive_alpha;
        let tab_accent_inactive_hover_alpha = config.appearance.tab_accent_inactive_hover_alpha;
        let tab_inactive_opacity = config.appearance.tab_inactive_opacity;
        let tab_close_size = config.appearance.tab_close_size;
        let tab_chrome_colors = crate::tab_colors::TabChromeColors::from_config(&config.appearance);
        let effective_ui_opacity = Self::effective_ui_opacity(ui_opacity);
        sidebar.update(cx, |s, cx| {
            s.set_ui_opacity(effective_ui_opacity, cx);
            s.set_tab_accent_alphas(
                tab_accent_inactive_alpha,
                tab_accent_inactive_hover_alpha,
                cx,
            );
        });
        let background_image = config.appearance.background_image.clone();
        let background_image_opacity =
            Self::clamp_background_image_opacity(config.appearance.background_image_opacity);
        let background_image_position = config.appearance.background_image_position.clone();
        let background_image_fit = config.appearance.background_image_fit.clone();
        let background_image_repeat = config.appearance.background_image_repeat;
        let terminal_theme = TerminalTheme::by_name(&config.terminal.theme).unwrap_or_default();
        let colors = theme_to_ghostty_colors(&terminal_theme);
        let terminal_tweaks = terminal_tweaks_to_ghostty(&config.terminal.tweaks);
        let ghostty_app = vu_ghostty::GhosttyApp::new(&vu_ghostty::AppearanceConfig {
            colors,
            font_family: terminal_font_family.clone(),
            font_size,
            background_opacity: terminal_opacity,
            background_blur: terminal_blur,
            cursor_style: terminal_cursor_style.clone(),
            background_image: background_image.clone(),
            background_image_opacity,
            background_image_position: Some(background_image_position.clone()),
            background_image_fit: Some(background_image_fit.clone()),
            background_image_repeat,
            tweaks: terminal_tweaks.clone(),
        })
        .map(std::sync::Arc::new)
        .unwrap_or_else(|e| panic!("Fatal: failed to initialize Ghostty: {}", e));
        let session_save_tx = spawn_session_save_worker();
        let (control_request_tx, mut control_request_rx) = tokio::sync::mpsc::unbounded_channel();
        let control_runtime = std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("control runtime must initialize"),
        );
        let control_socket =
            match vu_core::spawn_control_socket_server(control_runtime.clone(), control_request_tx)
            {
                Ok(handle) => Some(handle),
                Err(err) => {
                    log::error!("Failed to start vu control socket: {}", err);
                    None
                }
            };
        let restore_terminal_text = config.appearance.restore_terminal_text;
        let make_terminal = |cwd: Option<&str>,
                             restored_screen_text: Option<&[String]>,
                             force_restored_screen_text: bool,
                             window: &mut Window,
                             cx: &mut Context<Self>|
         -> TerminalPane {
            let restored_screen_text = if restore_terminal_text || force_restored_screen_text {
                restored_screen_text
            } else {
                None
            };
            make_ghostty_terminal(
                &ghostty_app,
                cwd,
                restored_screen_text,
                font_size,
                window,
                cx,
            )
        };

        let mut tabs: Vec<Tab> = session
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab_state)| {
                let pane_tree = if let Some(layout) = &tab_state.layout {
                    let mut restore_terminal =
                        |restore_cwd: Option<&str>,
                         restored_screen_text: Option<&[String]>,
                         force_restored_screen_text: bool| {
                            make_terminal(
                                restore_cwd,
                                restored_screen_text,
                                force_restored_screen_text,
                                window,
                                cx,
                            )
                        };
                    PaneTree::from_state(layout, tab_state.focused_pane_id, &mut restore_terminal)
                } else {
                    let cwd = tab_state.cwd.as_deref();
                    PaneTree::new(make_terminal(cwd, None, false, window, cx))
                };
                Tab {
                    pane_tree,
                    title: if tab_state.title.is_empty() {
                        format!("Terminal {}", i + 1)
                    } else {
                        tab_state.title.clone()
                    },
                    user_label: tab_state.user_label.clone(),
                    color: tab_state.color,
                    summary_id: i as u64,
                    needs_attention: false,
                    runtime_trackers: RefCell::new(HashMap::new()),
                    runtime_cache: RefCell::new(HashMap::new()),
                    shell_history: Self::restore_shell_history(tab_state),
                }
            })
            .collect();
        if tabs.is_empty() {
            let terminal = make_terminal(None, None, false, window, cx);
            tabs.push(Tab {
                pane_tree: PaneTree::new(terminal),
                title: "Terminal".to_string(),
                user_label: None,
                color: None,
                summary_id: 0,
                needs_attention: false,
                runtime_trackers: RefCell::new(HashMap::new()),
                runtime_cache: RefCell::new(HashMap::new()),
                shell_history: HashMap::new(),
            });
        }
        let active_tab = session.active_tab.min(tabs.len() - 1);
        // Seed the shared tab metadata before the first `sync_sidebar`
        // call, which only fires when the live terminal title or tab set
        // changes.
        {
            let entries: Vec<SessionEntry> = tabs
                .iter()
                .enumerate()
                .map(|(i, tab)| {
                    let presentation = smart_tab_presentation(
                        tab.user_label.as_deref(),
                        None,
                        Some(tab.title.as_str()),
                        None,
                        i,
                    );
                    let pane_count = tab.pane_tree.pane_terminals().len();
                    SessionEntry {
                        id: tab.summary_id,
                        name: presentation.name,
                        subtitle: presentation.subtitle,
                        is_ssh: presentation.is_ssh,
                        needs_attention: false,
                        icon: presentation.icon,
                        has_user_label: tab.user_label.is_some(),
                        pane_count,
                        color: tab.color,
                    }
                })
                .collect();
            sidebar.update(cx, |s, cx| {
                s.sync_sessions(entries, active_tab, cx);
            });
        }
        let persisted_history = GlobalHistoryState::load().unwrap_or_else(|err| {
            log::warn!("Failed to load command history: {}", err);
            GlobalHistoryState::default()
        });
        let global_shell_history = Self::restore_global_shell_history(&session, &tabs);
        let global_shell_history =
            Self::merge_shell_histories(global_shell_history, &persisted_history);
        let global_input_history =
            Self::restore_global_input_history(&session, &persisted_history, &global_shell_history);
        let input_bar = cx.new(|cx| InputBar::new(window, cx));
        let settings_panel = cx.new(|cx| SettingsPanel::new(&config, window, cx));
        let command_palette = cx.new(|cx| CommandPalette::new(window, cx));
        let initial_recent_inputs = global_input_history
            .iter()
            .rev()
            .take(80)
            .cloned()
            .collect::<Vec<_>>();
        input_bar.update(cx, |bar, cx| {
            bar.set_ui_opacity(effective_ui_opacity);
            bar.set_recent_commands(initial_recent_inputs, cx);
        });
        sidebar.update(cx, |s, cx| s.set_ui_opacity(effective_ui_opacity, cx));
        command_palette.update(cx, |palette, _cx| {
            palette.set_ui_opacity(effective_ui_opacity)
        });
        cx.subscribe_in(&input_bar, window, Self::on_input_submit)
            .detach();
        cx.subscribe_in(&input_bar, window, Self::on_input_edited)
            .detach();
        cx.subscribe_in(&input_bar, window, Self::on_input_scope_changed)
            .detach();
        cx.subscribe_in(&input_bar, window, Self::on_input_escape)
            .detach();
        cx.subscribe_in(
            &input_bar,
            window,
            Self::on_toggle_pane_scope_picker_requested,
        )
        .detach();
        cx.subscribe_in(&settings_panel, window, Self::on_settings_saved)
            .detach();
        cx.subscribe_in(&settings_panel, window, Self::on_theme_preview)
            .detach();
        cx.subscribe_in(&settings_panel, window, Self::on_appearance_preview)
            .detach();
        // Re-render workspace only when settings panel visibility changes (e.g. X close button).
        // A blanket observe re-rendered the whole workspace on every settings scroll tick.
        cx.subscribe_in(
            &settings_panel,
            window,
            |_, _, _: &VisibilityChanged, _, cx| cx.notify(),
        )
        .detach();
        cx.subscribe_in(&command_palette, window, Self::on_palette_select)
            .detach();
        cx.subscribe_in(&command_palette, window, Self::on_palette_dismissed)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_select)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_new_session)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_close_tab)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_rename)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_duplicate)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_reorder)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_pane_to_tab)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_close_others)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_set_color)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_open_tool_slot)
            .detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar_show_sessions)
            .detach();
        cx.observe(&sidebar, |_this, _sidebar, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_window_activation(window, Self::on_window_activation_changed)
            .detach();

        // Activity bar: sync file/search drawer state back to workspace on click.
        let activity_bar_entity = cx.new(|_cx| ActivityBar::new());
        // Sync initial state from session.
        {
            let initial_slot =
                ActivitySlot::from_str(session.activity_slot.as_deref().unwrap_or("files"));
            let initial_open = !config.appearance.tabs_orientation.is_vertical()
                && session.left_panel_open.unwrap_or(true);
            activity_bar_entity.update(cx, |bar, _cx| {
                bar.active_slot = initial_slot;
                bar.left_panel_open = initial_open;
            });
        }
        cx.subscribe_in(
            &activity_bar_entity,
            window,
            |this, _bar, event: &ActivitySlotChanged, _window, cx| {
                this.activity_slot = event.slot;
                this.left_panel_open = true;
                this.sidebar_tools_open = true;
                this.save_session(cx);
                cx.notify();
            },
        )
        .detach();
        cx.subscribe_in(
            &activity_bar_entity,
            window,
            |this, _bar, _event: &ActivityTogglePanel, _window, cx| {
                if this.vertical_tabs_enabled() {
                    this.sidebar_tools_open = !this.sidebar_tools_open;
                } else {
                    this.left_panel_open = !this.left_panel_open;
                    this.sidebar_tools_open = this.left_panel_open;
                }
                this.save_session(cx);
                cx.notify();
            },
        )
        .detach();

        // Sidebar content views.
        let file_tree_entity = cx.new(|_cx| FileTreeView::new());
        let search_view_entity = cx.new(|cx| SidebarSearchView::new(window, cx));

        // File tree: open file in the tab's shared editor pane when user clicks a file row.
        cx.subscribe_in(
            &file_tree_entity,
            window,
            |this, _tree, event: &OpenFile, window, cx| {
                this.open_path_in_active_editor(event.path.clone(), window, cx);
            },
        )
        .detach();
        cx.subscribe_in(
            &search_view_entity,
            window,
            |this, _search, event: &OpenFile, window, cx| {
                this.open_path_in_active_editor(event.path.clone(), window, cx);
            },
        )
        .detach();

        let workspace_handle = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            // Two shutdown paths, two behaviours:
            //
            // macOS — run prepare + return true so NSApp destroys the
            // window. Background tasks riding the main runloop finish
            // naturally; NSApp keeps the process alive without windows
            // per platform convention, so no explicit quit is needed.
            //
            // Windows — returning true tears the HWND down inside the
            // same WM_CLOSE iteration that fired this callback. That
            // races with pending async window tasks (e.g.
            // `handle_activate_msg::async_block$0`) which then run on
            // a dead HWND and log "Invalid window handle" / "window
            // not found". Defer the cleanup so in-flight tasks drain
            // first, then remove the window and quit (closing the
            // last window does NOT auto-terminate the process on
            // Windows). This mirrors `close_window_from_last_tab`,
            // which the user confirmed shuts down cleanly from the
            // pane-exit path.
            #[cfg(target_os = "windows")]
            {
                // `cx.defer_in` delays by a single event-loop tick, which
                // isn't enough: GPUI's `handle_activate_msg` spawns an
                // async task on the same executor when WM_ACTIVATE fires
                // during close, and those tasks outlive one tick — they
                // then run `window.update()` on a freshly-destroyed HWND
                // and log `window not found` with a backtrace. Spawn a
                // small timer instead so every in-flight window task
                // has time to drain before we tear the HWND down.
                //
                // The 120ms timer is a probability reduction, not a
                // guarantee: Windows can still deliver `WM_ACTIVATE` /
                // `WM_PAINT` to a closing HWND and surface the same log
                // error via GPUI's `async_context::update_window`. That
                // residual noise is benign — `prepare_window_close`
                // runs, sessions save, surfaces shut down, and the
                // process exits cleanly. See
                // `postmortem/2026-04-21-windows-x-close-log-noise.md`
                // for the full analysis and the upstream GPUI fix that
                // would eliminate the noise.
                //
                // `cx` inside this callback is `&mut App`, which has
                // `spawn` (not `spawn_in`), so reach the window via its
                // handle inside the spawned task.
                let handle = workspace_handle.clone();
                let window_handle = window.window_handle();
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(120))
                        .await;
                    let _ = window_handle.update(cx, |_, window, cx| {
                        let _ = handle.update(cx, |workspace, cx| {
                            workspace.prepare_window_close(cx);
                        });
                        let should_quit = cx.windows().len() <= 1;
                        window.remove_window();
                        if should_quit {
                            cx.quit();
                        }
                    });
                })
                .detach();
                return false;
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = window;
                let _ = workspace_handle.update(cx, |workspace, cx| {
                    workspace.prepare_window_close(cx);
                });
                true
            }
        });

        let terminal_wakeup = std::sync::Arc::new(tokio::sync::Notify::new());
        #[cfg(target_os = "macos")]
        ghostty_app.set_wakeup_callback({
            let terminal_wakeup = terminal_wakeup.clone();
            std::sync::Arc::new(move || terminal_wakeup.notify_one())
        });
        cx.spawn(async move |this, cx| {
            let mut next_request = None;
            let mut control_open = true;
            loop {
                let alive = this.update(cx, |workspace, cx| {
                    if workspace.window_close_prepared {
                        return false;
                    }
                    if let Some(request) = next_request.take() {
                        workspace.handle_control_request(request, cx);
                    }
                    while let Ok(request) = control_request_rx.try_recv() {
                        workspace.handle_control_request(request, cx);
                    }
                    if workspace.pump_ghostty_views(cx) {
                        cx.notify();
                    }
                    true
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }

                // Non-macOS backends still require polling for surface events.
                let fallback =
                    Duration::from_millis(if cfg!(target_os = "macos") { 250 } else { 8 });
                tokio::select! {
                    request = control_request_rx.recv(), if control_open => {
                        next_request = request;
                        control_open = next_request.is_some();
                    }
                    _ = terminal_wakeup.notified() => {}
                    _ = cx.background_executor().timer(fallback) => {}
                }
            }
        })
        .detach();

        // Focus the initial terminal so the user can start typing immediately
        if let Some(initial_terminal) = tabs[active_tab].pane_tree.try_focused_terminal().cloned() {
            initial_terminal.focus(window, cx);
            Self::schedule_terminal_bootstrap_reassert(
                &initial_terminal,
                true,
                window.window_handle(),
                cx.weak_entity(),
                cx,
            );
        }

        // Hide non-active tabs' ghostty NSViews so only the active tab is visible
        for (i, tab) in tabs.iter().enumerate() {
            if i != active_tab {
                for terminal in tab.pane_tree.all_surface_terminals() {
                    terminal.set_native_view_visible(false, cx);
                }
            }
        }

        let has_multiple_tabs = tabs.len() > 1;
        let last_ghostty_wake_generation = ghostty_app.wake_generation();
        let next_tab_summary_id_init = tabs.len() as u64;

        let workspace = Self {
            config: config.clone(),
            sidebar,
            tabs,
            active_tab,
            terminal_font_family,
            terminal_tweaks,
            ui_font_family,
            ui_font_size,
            font_size,
            terminal_cursor_style,
            terminal_opacity,
            terminal_blur,
            ui_opacity,
            tab_accent_inactive_alpha,
            tab_accent_inactive_hover_alpha,
            tab_inactive_opacity,
            tab_close_size,
            tab_chrome_colors,
            background_image,
            background_image_opacity,
            background_image_position,
            background_image_fit,
            background_image_repeat,
            input_bar,
            input_suggestion_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            input_suggestion_task: None,
            settings_panel,
            settings_window: None,
            settings_window_panel: None,
            command_palette,
            global_shell_history,
            global_input_history,
            pane_scope_picker_open: false,
            tab_strip_motion: MotionValue::new(if has_multiple_tabs { 1.0 } else { 0.0 }),
            input_bar_visible: session.input_bar_visible,
            input_bar_motion: MotionValue::new(if session.input_bar_visible { 1.0 } else { 0.0 }),
            modal_was_open: false,
            ghostty_hidden: false,
            sidebar_drag: None,
            terminal_theme,
            ghostty_app,
            last_ghostty_wake_generation,
            #[cfg(target_os = "macos")]
            chrome_transition_underlay_until: None,
            #[cfg(target_os = "macos")]
            #[cfg(target_os = "macos")]
            input_bar_snap_guard_until: None,
            #[cfg(target_os = "macos")]
            top_chrome_snap_guard_until: None,
            #[cfg(target_os = "macos")]
            #[cfg(target_os = "macos")]
            input_bar_release_cover_until: None,
            #[cfg(target_os = "macos")]
            top_chrome_release_cover_until: None,
            #[cfg(target_os = "linux")]
            linux_window_shape_signature: None,
            pending_create_pane_requests: Vec::new(),
            pending_window_control_requests: Vec::new(),
            pending_surface_control_requests: Vec::new(),
            surface_rename: None,
            _control_runtime: control_runtime,
            control_socket,
            next_tab_summary_id: next_tab_summary_id_init,
            window_handle: window.window_handle(),
            workspace_handle: cx.weak_entity(),
            window_close_prepared: false,
            session_save_tx,
            session_save_task: RefCell::new(None),
            tab_rename: None,
            tab_rename_cancelled_generation: None,
            tab_rename_generation: 0,
            tab_strip_drop_slot: None,
            tab_drag_target: None,
            active_dragged_tab_session_id: std::sync::Arc::new(std::sync::Mutex::new(None)),
            tab_drag_preview: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(target_os = "macos")]
            top_bar_should_move: false,
            pane_title_drag: None,
            pane_content_bounds: std::sync::Arc::new(std::sync::Mutex::new(None)),
            tab_strip_tab_bounds: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            pane_title_drag_tab_bounds: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            hide_pane_title_bar: config.appearance.hide_pane_title_bar,

            // ── Code editor (Phase 1) ──────────────────────────────────────
            activity_bar: activity_bar_entity,
            activity_slot: ActivitySlot::from_str(
                session.activity_slot.as_deref().unwrap_or("files"),
            ),
            left_panel_open: session.left_panel_open.unwrap_or(true),
            sidebar_tools_open: !config.appearance.tabs_orientation.is_vertical()
                && session.left_panel_open.unwrap_or(true),
            file_tree_view: file_tree_entity,
            search_view: search_view_entity,
            workspace_focus: cx.focus_handle(),
        };

        workspace
    }

    /// Create a new Ghostty terminal pane.
    pub(super) fn create_terminal(
        &mut self,
        cwd: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> TerminalPane {
        make_ghostty_terminal(&self.ghostty_app, cwd, None, self.font_size, window, cx)
    }

    pub(super) fn open_path_in_active_editor(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.has_active_tab() {
            return;
        }

        let active_tab = self.active_tab;
        let (pane_id, editor_view) =
            if let Some(pane_id) = self.tabs[active_tab].pane_tree.find_editor_pane() {
                let Some(editor_view) = self.tabs[active_tab]
                    .pane_tree
                    .editor_view_for_pane(pane_id)
                else {
                    return;
                };
                (pane_id, editor_view)
            } else {
                let editor_font_size = self.font_size;
                let editor_view = cx.new(|cx| EditorView::new_with_font_size(editor_font_size, cx));
                cx.subscribe_in(
                    &editor_view,
                    window,
                    |this, _editor, _event: &ActiveFileChanged, _window, cx| {
                        this.sync_file_tree_from_active_focus(cx);
                    },
                )
                .detach();
                cx.subscribe_in(
                    &editor_view,
                    window,
                    |this, editor, _event: &EditorEmptied, window, cx| {
                        let Some((tab_index, pane_id)) =
                            this.tabs.iter().enumerate().find_map(|(tab_index, tab)| {
                                tab.pane_tree
                                    .pane_id_for_editor_view(editor)
                                    .map(|pane_id| (tab_index, pane_id))
                            })
                        else {
                            return;
                        };
                        if !this.close_pane_in_tab(tab_index, pane_id, window, cx) {
                            this.close_tab_by_index(tab_index, window, cx);
                        }
                    },
                )
                .detach();
                let Some(pane_id) = self.tabs[active_tab]
                    .pane_tree
                    .split_with_editor(editor_view.clone())
                else {
                    return;
                };
                (pane_id, editor_view)
            };

        self.tabs[active_tab].pane_tree.focus_pane(pane_id);
        let editor_focus = editor_view.read(cx).focus_handle(cx).clone();
        editor_view.update(cx, |editor: &mut EditorView, cx| {
            editor.open_file(path.clone(), cx);
        });
        editor_focus.focus(window, cx);
        self.sync_file_tree_from_active_focus(cx);
        cx.notify();
    }

    pub(super) fn horizontal_tabs_visible(&self) -> bool {
        !self.vertical_tabs_enabled() && self.tabs.len() > 1
    }

    pub(super) fn vertical_tabs_enabled(&self) -> bool {
        self.config.appearance.tabs_orientation == TabsOrientation::Vertical
    }

    pub(super) fn sync_tab_strip_motion(&mut self) -> bool {
        let target = if self.horizontal_tabs_visible() {
            1.0
        } else {
            0.0
        };
        let changed = (self.tab_strip_motion.current() - target).abs() > 0.001;
        self.tab_strip_motion.set_target(
            target,
            Self::terminal_adjacent_chrome_duration(target > 0.5, 180, 180),
        );
        changed
    }

    pub(super) fn current_top_bar_height(&self) -> f32 {
        if self.tab_strip_motion.is_animating() || self.horizontal_tabs_visible() {
            TOP_BAR_TABS_HEIGHT
        } else {
            TOP_BAR_COMPACT_HEIGHT
        }
    }

    #[allow(dead_code)]
    pub(super) fn active_terminal(&self) -> &TerminalPane {
        self.tabs[self.active_tab].pane_tree.focused_terminal()
    }

    pub(super) fn try_active_terminal(&self) -> Option<&TerminalPane> {
        self.tabs[self.active_tab].pane_tree.try_focused_terminal()
    }

    pub(super) fn refocus_active_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_active_tab() {
            self.focus_terminal(window, cx);
        }
    }

    pub(super) fn sync_active_terminal_focus_states(&self, cx: &mut App) {
        if !self.has_active_tab() {
            return;
        }

        let pane_tree = &self.tabs[self.active_tab].pane_tree;
        let surface_terminals = pane_tree.surface_terminals();
        let focused_id = pane_tree.focused_pane_id();
        let (is_broadcast, is_focused, selected_ids) = {
            let input_bar = self.input_bar.read(cx);
            (
                input_bar.is_broadcast_scope(),
                input_bar.is_focused_scope(),
                input_bar.scope_selected_ids(),
            )
        };
        let all_ids = surface_terminals
            .iter()
            .map(|(pane_id, _, _)| *pane_id)
            .collect::<HashSet<_>>();

        let target_ids: HashSet<usize> = if is_focused {
            [focused_id].into_iter().collect()
        } else if is_broadcast {
            all_ids
        } else {
            let selected = selected_ids
                .into_iter()
                .filter(|pane_id| all_ids.contains(pane_id))
                .collect::<HashSet<_>>();
            if selected.is_empty() {
                [focused_id].into_iter().collect()
            } else {
                selected
            }
        };

        for (pane_id, is_active_surface, terminal) in surface_terminals {
            terminal.set_focus_state(is_active_surface && target_ids.contains(&pane_id), cx);
        }
    }

    pub(super) fn clear_terminal_focus_states_for_active_tab(&self, cx: &mut App) {
        if !self.has_active_tab() {
            return;
        }
        for (_, _, terminal) in self.tabs[self.active_tab].pane_tree.surface_terminals() {
            terminal.set_focus_state(false, cx);
        }
    }

    pub(super) fn sync_tab_native_view_visibility(
        &self,
        tab_index: usize,
        visible: bool,
        cx: &App,
    ) {
        let Some(tab) = self.tabs.get(tab_index) else {
            return;
        };
        let zoomed_pane_id = tab.pane_tree.zoomed_pane_id();

        for surface in tab.pane_tree.surface_infos(None) {
            let pane_visible =
                visible && zoomed_pane_id.is_none_or(|zoomed| zoomed == surface.pane_id);
            surface
                .terminal
                .set_native_view_visible(pane_visible && surface.is_active, cx);
        }
    }

    pub(super) fn sync_active_tab_native_view_visibility(&self, cx: &App) {
        if self.has_active_tab() {
            self.sync_tab_native_view_visibility(self.active_tab, true, cx);
        }
    }

    pub(super) fn hide_active_tab_native_views_not_visible_after_layout(&self, cx: &App) {
        if !self.has_active_tab() {
            return;
        }
        let tab = &self.tabs[self.active_tab];
        let zoomed_pane_id = tab.pane_tree.zoomed_pane_id();
        let focused_pane_id = tab.pane_tree.focused_pane_id();

        for surface in tab.pane_tree.surface_infos(None) {
            let pane_visible = zoomed_pane_id.is_none_or(|zoomed| zoomed == surface.pane_id);
            // Same-pane surface switches should keep the old surface visible
            // until the newly active one has a committed frame. Hiding it here
            // creates a one-frame blank pane. Active surfaces in other visible
            // panes should also stay visible; they are not leaving the layout.
            if !pane_visible || (!surface.is_active && surface.pane_id != focused_pane_id) {
                surface.terminal.set_native_view_visible(false, cx);
            }
        }
    }

    pub(super) fn notify_tab_terminal_views(&self, tab_index: usize, cx: &mut App) {
        let Some(tab) = self.tabs.get(tab_index) else {
            return;
        };
        for terminal in tab.pane_tree.all_terminals() {
            terminal.notify(cx);
        }
    }

    pub(super) fn notify_active_tab_terminal_views(&self, cx: &mut App) {
        if self.has_active_tab() {
            self.notify_tab_terminal_views(self.active_tab, cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn mark_tab_terminal_native_layout_pending(&self, tab_index: usize, cx: &mut App) {
        let Some(tab) = self.tabs.get(tab_index) else {
            return;
        };
        for terminal in tab.pane_tree.all_terminals() {
            terminal.mark_native_layout_pending(cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn mark_active_tab_terminal_native_layout_pending(&self, cx: &mut App) {
        if self.has_active_tab() {
            self.mark_tab_terminal_native_layout_pending(self.active_tab, cx);
        }
    }

    pub(super) fn sync_active_tab_native_view_visibility_after_layout(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Native Ghostty NSViews live outside GPUI's element tree. Hide
        // surfaces that are definitely no longer visible immediately, but
        // delay revealing newly-visible surfaces until GPUI has committed one
        // layout frame so they do not flash at stale split coordinates.
        self.hide_active_tab_native_views_not_visible_after_layout(cx);
        cx.on_next_frame(window, |_workspace, window, cx| {
            cx.notify();
            cx.on_next_frame(window, |workspace, _window, cx| {
                if workspace.has_active_tab()
                    && !workspace.ghostty_hidden
                    && !workspace.is_modal_open(cx)
                {
                    workspace.sync_active_tab_native_view_visibility(cx);
                }
            });
        });
    }

    pub(super) fn sync_active_tab_native_view_visibility_after_zoom_layout(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Zoom/unzoom should not hide anything before GPUI has produced the
        // new pane frames. Keeping the old visual state for one frame is less
        // visible than flashing the matte/fallback background.
        cx.on_next_frame(window, |workspace, _window, cx| {
            if workspace.has_active_tab()
                && !workspace.ghostty_hidden
                && !workspace.is_modal_open(cx)
            {
                workspace.sync_active_tab_native_view_visibility(cx);
            }
        });
    }

    pub(super) fn sync_active_tab_native_view_visibility_now_or_after_layout(
        &self,
        was_zoomed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if was_zoomed
            || self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| tab.pane_tree.zoomed_pane_id())
                .is_some()
        {
            self.sync_active_tab_native_view_visibility_after_zoom_layout(window, cx);
            return;
        }
        self.sync_active_tab_native_view_visibility_after_layout(window, cx);
    }

    pub(super) fn pump_ghostty_views(&mut self, cx: &mut Context<Self>) -> bool {
        let started = perf_trace_enabled().then(std::time::Instant::now);
        let mut changed = false;
        let mut terminal_count = 0usize;
        let mut drain_count = 0usize;

        let generation = self.ghostty_app.wake_generation();
        let generation_changed = generation != self.last_ghostty_wake_generation;
        self.last_ghostty_wake_generation = generation;
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            let sync_native_scroll = generation_changed && tab_index == self.active_tab;
            for terminal in tab.pane_tree.all_surface_terminals() {
                terminal_count += 1;
                changed |= terminal.pump_surface_deferred_work(cx);
                drain_count += 1;
                changed |= terminal.drain_surface_state_with_native_scroll(sync_native_scroll, cx);
            }
        }

        if let Some(started) = started
            && (generation_changed || changed)
        {
            log::info!(
                target: "vu::perf",
                "pump_ghostty_views generation={} terminals={} drains={} changed={} elapsed_ms={:.3}",
                generation,
                terminal_count,
                drain_count,
                changed,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }

        changed
    }

    pub(super) fn schedule_terminal_bootstrap_reassert(
        terminal: &TerminalPane,
        should_focus: bool,
        window_handle: AnyWindowHandle,
        workspace_handle: WeakEntity<Self>,
        cx: &mut Context<Self>,
    ) {
        let terminal = terminal.clone();
        cx.spawn(async move |_, cx| {
            for attempt in 0..8 {
                let delay_ms = if attempt == 0 { 16 } else { 250 };
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(delay_ms))
                    .await;
                let ready = window_handle
                    .update(cx, |_root, window, cx| {
                        let Some(workspace) = workspace_handle.upgrade() else {
                            return false;
                        };
                        workspace.update(cx, |_workspace, cx| {
                            let terminal_entity_id = terminal.entity_id();
                            let terminal_still_owned = _workspace.tabs.iter().any(|tab| {
                                tab.pane_tree
                                    .all_surface_terminals()
                                    .iter()
                                    .any(|candidate| candidate.entity_id() == terminal_entity_id)
                            });
                            if !terminal_still_owned {
                                terminal.set_focus_state(false, cx);
                                return true;
                            }

                            terminal.ensure_surface(window, cx);
                            terminal.notify(cx);
                            _workspace.sync_active_tab_native_view_visibility(cx);
                            if should_focus {
                                let still_active_terminal = _workspace
                                    .tabs
                                    .get(_workspace.active_tab)
                                    .is_some_and(|tab| {
                                        tab.pane_tree.all_terminals().iter().any(|candidate| {
                                            candidate.entity_id() == terminal_entity_id
                                        })
                                    });
                                if still_active_terminal {
                                    _workspace.sync_active_terminal_focus_states(cx);
                                    terminal.focus(window, cx);
                                } else {
                                    terminal.set_focus_state(false, cx);
                                }
                            } else {
                                terminal.set_focus_state(false, cx);
                            }
                            if terminal.surface_ready(cx) {
                                terminal.recover_shell_prompt_state(cx);
                                true
                            } else {
                                false
                            }
                        })
                    })
                    .unwrap_or(false);
                if ready {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn schedule_pending_create_pane_flush(&self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let workspace_handle = self.workspace_handle.clone();
        cx.defer(move |cx| {
            let result = window_handle.update(cx, |_root, window, cx| {
                if let Some(workspace) = workspace_handle.upgrade() {
                    let _ = workspace.update(cx, |workspace, cx| {
                        workspace.flush_pending_window_control_requests(window, cx);
                        workspace.flush_pending_create_pane_requests(window, cx);
                        workspace.flush_pending_surface_control_requests(window, cx);
                    });
                }
            });
            if let Err(err) = result {
                log::warn!(
                    "[control] failed to flush deferred pane creation in a window-aware context: {err}"
                );
            }
        });
    }

    pub(super) fn schedule_active_terminal_focus(&self, was_zoomed: bool, cx: &mut Context<Self>) {
        #[cfg(not(target_os = "macos"))]
        let _ = was_zoomed;

        let window_handle = self.window_handle;
        let workspace_handle = self.workspace_handle.clone();
        cx.defer(move |cx| {
            let result = window_handle.update(cx, |_root, window, cx| {
                if let Some(workspace) = workspace_handle.upgrade() {
                    let _ = workspace.update(cx, |workspace, cx| {
                        if !workspace.has_active_tab() {
                            return;
                        }
                        if let Some(terminal) = workspace.tabs[workspace.active_tab]
                            .pane_tree
                            .try_focused_terminal()
                            .cloned()
                        {
                            terminal.ensure_surface(window, cx);
                            #[cfg(target_os = "macos")]
                            workspace.sync_active_tab_native_view_visibility_now_or_after_layout(
                                was_zoomed, window, cx,
                            );
                            #[cfg(not(target_os = "macos"))]
                            workspace.sync_active_tab_native_view_visibility(cx);
                            terminal.focus(window, cx);
                            workspace.sync_active_terminal_focus_states(cx);
                        }
                        cx.notify();
                    });
                }
            });
            if let Err(err) = result {
                log::warn!(
                    "[control] failed to focus active terminal in a window-aware context: {err}"
                );
            }
        });
    }
}
