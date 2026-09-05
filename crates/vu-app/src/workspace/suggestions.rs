use super::path_completion::{complete_path, path_token};
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

impl VuWorkspace {
    pub(super) fn record_shell_command(
        &mut self,
        tab_idx: usize,
        pane_id: usize,
        command: &str,
        cwd: Option<String>,
    ) {
        let trimmed = command.trim();
        if trimmed.is_empty() || tab_idx >= self.tabs.len() {
            return;
        }

        let history = self.tabs[tab_idx].shell_history.entry(pane_id).or_default();
        if let Some(existing_idx) = history.iter().position(|entry| entry.command == trimmed) {
            history.remove(existing_idx);
        }
        history.push_back(CommandSuggestionEntry {
            command: trimmed.to_string(),
            cwd: cwd.clone(),
        });
        while history.len() > MAX_SHELL_HISTORY_PER_PANE {
            history.pop_front();
        }

        if let Some(existing_idx) = self
            .global_shell_history
            .iter()
            .position(|entry| entry.command == trimmed)
        {
            self.global_shell_history.remove(existing_idx);
        }
        self.global_shell_history.push_back(CommandSuggestionEntry {
            command: trimmed.to_string(),
            cwd,
        });
        while self.global_shell_history.len() > MAX_GLOBAL_SHELL_HISTORY {
            self.global_shell_history.pop_front();
        }
    }

    pub(super) fn record_input_history(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        if let Some(existing_idx) = self
            .global_input_history
            .iter()
            .position(|entry| entry == trimmed)
        {
            self.global_input_history.remove(existing_idx);
        }
        self.global_input_history.push_back(trimmed.to_string());
        while self.global_input_history.len() > MAX_GLOBAL_INPUT_HISTORY {
            self.global_input_history.pop_front();
        }
    }

    pub(super) fn recent_input_history(&self, limit: usize) -> Vec<String> {
        self.global_input_history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub(super) fn history_completion_for_prefix(
        &self,
        prefix: &str,
        cwd: Option<&str>,
    ) -> Option<String> {
        let mut fallback: Option<String> = None;

        for entry in self.global_shell_history.iter().rev() {
            if entry.command == prefix || !entry.command.starts_with(prefix) {
                continue;
            }

            if cwd.is_some() && entry.cwd.as_deref() == cwd {
                return Some(entry.command.clone());
            }

            if fallback.is_none() {
                fallback = Some(entry.command.clone());
            }
        }

        if fallback.is_some() {
            return fallback;
        }

        self.global_input_history
            .iter()
            .rev()
            .find(|entry| entry.as_str() != prefix && entry.starts_with(prefix))
            .cloned()
    }

    pub(super) fn refresh_input_suggestion(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input_suggestion_cancel.store(true, Ordering::Relaxed);
        self.input_suggestion_task.take();
        self.input_suggestion_cancel = Arc::new(AtomicBool::new(false));
        if !self.has_active_tab() {
            self.input_bar
                .update(cx, |bar, _| bar.clear_completion_ui());
            return;
        }

        let (text, target_ids) = self
            .input_bar
            .update(cx, |bar, cx| (bar.current_text(cx), bar.target_pane_ids()));
        let trimmed = text.trim();
        if trimmed.is_empty() || text.contains('\n') || target_ids.len() != 1 {
            self.input_bar
                .update(cx, |bar, _| bar.clear_completion_ui());
            return;
        }

        let pane_id = target_ids[0];
        let pane = self.tabs[self.active_tab]
            .pane_tree
            .pane_terminals()
            .into_iter()
            .find_map(|(id, terminal)| (id == pane_id).then_some(terminal));
        let cwd = pane.as_ref().and_then(|pane| pane.current_dir(cx));

        if let Some(terminal) = pane.filter(|terminal| {
            path_token(&text).is_some()
                && self
                    .effective_remote_host_for_tab(self.active_tab, terminal, cx)
                    .is_none()
        }) && let Some(cwd) = cwd.clone()
        {
            let tab_id = self.tabs[self.active_tab].summary_id;
            let terminal_id = terminal.entity_id();
            let cancelled = self.input_suggestion_cancel.clone();
            self.input_bar
                .update(cx, |bar, _| bar.clear_completion_ui());
            self.input_suggestion_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let search_cwd = cwd.clone();
                let search_text = text.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { complete_path(&search_cwd, &search_text, &cancelled) })
                    .await;
                let _ = this.update(cx, |workspace, cx| {
                    let Some(tab) = workspace.tabs.get(workspace.active_tab) else {
                        return;
                    };
                    if tab.summary_id != tab_id {
                        return;
                    }
                    let Some(terminal) = tab
                        .pane_tree
                        .pane_terminals()
                        .into_iter()
                        .find_map(|(id, terminal)| (id == pane_id).then_some(terminal))
                    else {
                        return;
                    };
                    let bar = workspace.input_bar.read(cx);
                    if terminal.entity_id() != terminal_id
                        || terminal.current_dir(cx).as_deref() != Some(cwd.as_str())
                        || workspace
                            .effective_remote_host_for_tab(workspace.active_tab, &terminal, cx)
                            .is_some()
                        || bar.current_text(cx) != text
                        || bar.target_pane_ids() != [pane_id]
                    {
                        return;
                    }
                    let history = workspace.history_completion_for_prefix(&text, Some(&cwd));
                    workspace.input_bar.update(cx, |bar, _| match result {
                        Some(LocalPathCompletion::Inline(completion)) => {
                            bar.set_path_inline_suggestion(&text, &completion)
                        }
                        Some(LocalPathCompletion::Candidates(candidates)) => {
                            bar.set_path_completion_candidates(&text, candidates)
                        }
                        None => match history {
                            Some(completion) => {
                                bar.set_history_inline_suggestion(&text, &completion)
                            }
                            None => bar.clear_completion_ui(),
                        },
                    });
                    cx.notify();
                });
            }));
            return;
        }

        if let Some(completion) = self.history_completion_for_prefix(&text, cwd.as_deref()) {
            self.input_bar.update(cx, |bar, _| {
                bar.set_history_inline_suggestion(&text, &completion)
            });
            return;
        }

        self.input_bar
            .update(cx, |bar, _| bar.clear_completion_ui());
    }

    #[cfg(test)]
    pub(super) fn new_tab_sync_policy_for_tests() -> NewTabSyncPolicy {
        NewTabSyncPolicy {
            activates_new_tab: true,
            syncs_sidebar: true,
            notifies_ui: true,
            syncs_native_visibility: true,
            reuses_shared_tab_activation_flow: true,
        }
    }

    pub(super) fn should_defer_top_chrome_refresh_when_tab_strip_appears() -> bool {
        true
    }

    #[cfg(test)]
    pub(super) fn should_defer_top_chrome_refresh_when_tab_strip_appears_for_tests() -> bool {
        Self::should_defer_top_chrome_refresh_when_tab_strip_appears()
    }
}
