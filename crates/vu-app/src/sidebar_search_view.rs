//! Sidebar search panel — searches files below the active sidebar root.

use crate::file_tree_view::OpenFile;
use gpui::{
    Context, Div, Entity, EventEmitter, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, StyledText, TextStyle,
    WhiteSpace, Window, div, prelude::*, px, svg,
};
use gpui_component::{
    ActiveTheme, Sizable as _,
    input::{Input, InputState},
    scroll::{Scrollbar, ScrollbarShow},
    switch::Switch,
};
use regex::{Regex, RegexBuilder};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);
const MAX_SEARCH_FILES: usize = 800;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_RESULTS: usize = 200;
const MAX_MATCHES_PER_FILE: usize = 20;
/// Bytes of context kept before the match in a result preview.
const PREVIEW_LEAD_BYTES: usize = 32;
/// Hard cap on the bytes stored per result preview. A 512 KiB minified line
/// contributes at most this much to the result list.
const MAX_PREVIEW_BYTES: usize = 200;

/// One match. `preview` is a bounded slice of the matching line and
/// `match_start` / `match_len` are byte offsets into that preview.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchMatch {
    path: Arc<Path>,
    line_number: usize,
    preview: SharedString,
    match_start: usize,
    match_len: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SearchOptions {
    case_sensitive: bool,
    regex: bool,
}

enum SearchMatcher {
    Literal {
        needle: String,
        case_sensitive: bool,
        case_insensitive_regex: Option<Regex>,
    },
    Regex(Regex),
}

impl SearchMatcher {
    fn new(query: &str, options: SearchOptions) -> Option<Self> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        if options.regex {
            let regex = RegexBuilder::new(query)
                .case_insensitive(!options.case_sensitive)
                .build()
                .ok()?;
            Some(Self::Regex(regex))
        } else {
            let case_insensitive_regex = if options.case_sensitive {
                None
            } else {
                RegexBuilder::new(&regex::escape(query))
                    .case_insensitive(true)
                    .build()
                    .ok()
            };
            Some(Self::Literal {
                needle: query.to_string(),
                case_sensitive: options.case_sensitive,
                case_insensitive_regex,
            })
        }
    }

    fn find(&self, line: &str) -> Option<(usize, usize)> {
        match self {
            Self::Literal {
                needle,
                case_sensitive,
                case_insensitive_regex,
            } => {
                if *case_sensitive {
                    line.find(needle).map(|start| (start, needle.len()))
                } else {
                    case_insensitive_regex
                        .as_ref()?
                        .find(line)
                        .map(|matched| (matched.start(), matched.end() - matched.start()))
                }
            }
            Self::Regex(regex) => regex
                .find(line)
                .map(|matched| (matched.start(), matched.end() - matched.start())),
        }
    }
}

impl EventEmitter<OpenFile> for SidebarSearchView {}

pub struct SidebarSearchView {
    root: Option<PathBuf>,
    query: Entity<InputState>,
    results_scroll_handle: ScrollHandle,
    query_text: String,
    options: SearchOptions,
    results: Vec<SearchMatch>,
    search_generation: u64,
    search_in_progress: bool,
    search_cancel: Arc<AtomicBool>,
    search_task: Option<gpui::Task<()>>,
}

impl Drop for SidebarSearchView {
    fn drop(&mut self) {
        self.search_cancel.store(true, Ordering::Relaxed);
    }
}

impl SidebarSearchView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search")
                .auto_grow(1, 3)
        });

        Self {
            root: None,
            query,
            results_scroll_handle: ScrollHandle::default(),
            query_text: String::new(),
            options: SearchOptions::default(),
            results: Vec::new(),
            search_generation: 0,
            search_in_progress: false,
            search_cancel: Arc::new(AtomicBool::new(false)),
            search_task: None,
        }
    }

    pub fn set_root(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.root.as_deref() == Some(root.as_path()) {
            return;
        }
        self.root = Some(root);
        self.request_search(cx);
    }

    pub fn focus_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| query.focus(window, cx));
    }

    fn request_search(&mut self, cx: &mut Context<Self>) {
        self.search_cancel.store(true, Ordering::Relaxed);
        self.search_task.take();
        self.search_cancel = Arc::new(AtomicBool::new(false));
        let cancelled = self.search_cancel.clone();
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        let Some(root) = self.root.clone() else {
            self.results.clear();
            self.search_in_progress = false;
            cx.notify();
            return;
        };
        let query = self.query_text.clone();
        if query.trim().is_empty() {
            self.results.clear();
            self.search_in_progress = false;
            cx.notify();
            return;
        }

        let options = self.options;
        self.results.clear();
        self.search_in_progress = true;
        cx.notify();
        self.search_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            if cancelled.load(Ordering::Relaxed) {
                return;
            }
            let root_for_search = root.clone();
            let query_for_search = query.clone();
            let results = cx
                .background_executor()
                .spawn(async move {
                    search_files(&root_for_search, &query_for_search, options, &cancelled)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.search_generation == generation
                    && this.root.as_deref() == Some(root.as_path())
                    && this.query_text == query
                    && this.options == options
                {
                    this.results = results;
                    this.search_in_progress = false;
                    cx.notify();
                }
            });
        }));
    }

    fn set_case_sensitive(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.options.case_sensitive = enabled;
        self.request_search(cx);
    }

    fn set_regex(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.options.regex = enabled;
        self.request_search(cx);
    }
}

fn search_files(
    root: &Path,
    query: &str,
    options: SearchOptions,
    cancelled: &AtomicBool,
) -> Vec<SearchMatch> {
    if cancelled.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let Some(matcher) = SearchMatcher::new(query, options) else {
        return Vec::new();
    };
    let mut files_seen = 0;
    let mut results = Vec::new();
    search_dir(root, &matcher, &mut files_seen, &mut results, cancelled);
    results
}

fn search_dir(
    dir: &Path,
    matcher: &SearchMatcher,
    files_seen: &mut usize,
    results: &mut Vec<SearchMatch>,
    cancelled: &AtomicBool,
) {
    if cancelled.load(Ordering::Relaxed)
        || results.len() >= MAX_RESULTS
        || *files_seen >= MAX_SEARCH_FILES
    {
        return;
    }

    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries = read_dir
        .take_while(|_| !cancelled.load(Ordering::Relaxed))
        .flatten()
        .collect::<Vec<_>>();
    if cancelled.load(Ordering::Relaxed) {
        return;
    }
    entries.sort_by_cached_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

    for entry in entries {
        if cancelled.load(Ordering::Relaxed)
            || results.len() >= MAX_RESULTS
            || *files_seen >= MAX_SEARCH_FILES
        {
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules" | "dist") {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            search_dir(&path, matcher, files_seen, results, cancelled);
        } else if file_type.is_file() {
            *files_seen += 1;
            search_file(&path, matcher, results, cancelled);
        }
    }
}

fn search_file(
    path: &Path,
    matcher: &SearchMatcher,
    results: &mut Vec<SearchMatch>,
    cancelled: &AtomicBool,
) {
    if cancelled.load(Ordering::Relaxed) {
        return;
    }
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() > MAX_FILE_BYTES {
        return;
    }

    if cancelled.load(Ordering::Relaxed) {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };

    let path: Arc<Path> = Arc::from(path);
    let mut matches_for_file = 0;
    for (line_idx, line) in content.lines().enumerate() {
        if cancelled.load(Ordering::Relaxed)
            || matches_for_file >= MAX_MATCHES_PER_FILE
            || results.len() >= MAX_RESULTS
        {
            return;
        }
        let Some((match_start, match_len)) = matcher.find(line) else {
            continue;
        };
        let (preview, match_start, match_len) = result_preview_match(line, match_start, match_len);
        results.push(SearchMatch {
            path: path.clone(),
            line_number: line_idx + 1,
            preview: SharedString::from(preview),
            match_start,
            match_len,
        });
        matches_for_file += 1;
    }
}

impl Render for SidebarSearchView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let next_query = self.query.read(cx).value().to_string();
        if next_query != self.query_text {
            self.query_text = next_query;
            self.request_search(cx);
        }

        let theme = cx.theme();
        let root = self.root.as_deref();
        let query_empty = self.query_text.trim().is_empty();
        let case_sensitive = self.options.case_sensitive;
        let regex_enabled = self.options.regex;
        let mut rows = Vec::new();

        if root.is_none() {
            rows.push(
                empty_state("No folder open", theme)
                    .id("search-empty-root")
                    .into_any_element(),
            );
        } else if query_empty {
            rows.push(
                empty_state("Search files", theme)
                    .id("search-empty-query")
                    .into_any_element(),
            );
        } else if self.search_in_progress {
            rows.push(
                empty_state("Searching...", theme)
                    .id("search-in-progress")
                    .into_any_element(),
            );
        } else if self.results.is_empty() {
            rows.push(
                empty_state("No results", theme)
                    .id("search-empty-results")
                    .into_any_element(),
            );
        } else {
            // Matches for one file are always contiguous, so the file header
            // and its count fall out of grouping by path.
            let mut idx: usize = 0;
            for file_matches in self.results.chunk_by(|a, b| a.path == b.path) {
                let file_path: &Path = &file_matches[0].path;
                let display = root
                    .and_then(|root| file_path.strip_prefix(root).ok())
                    .unwrap_or(file_path)
                    .display()
                    .to_string();
                rows.push(
                    div()
                        .id(("search-file", idx))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .mx(px(6.0))
                        .px(px(6.0))
                        .mt(px(8.0))
                        .child(
                            svg()
                                .path("phosphor/file-text.svg")
                                .size(px(13.0))
                                .text_color(theme.muted_foreground.opacity(0.72)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(12.0))
                                .font_family(theme.font_family.clone())
                                .text_color(theme.foreground.opacity(0.82))
                                .child(SharedString::from(display)),
                        )
                        .child(result_count_badge(file_matches.len(), theme))
                        .into_any_element(),
                );

                for result in file_matches {
                    let path = result.path.clone();
                    let line_number = result.line_number;
                    rows.push(
                        div()
                            .id(("search-result", idx))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .mx(px(6.0))
                            .pl(px(24.0))
                            .pr(px(6.0))
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.foreground.opacity(0.055)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_this, _: &MouseDownEvent, _window, cx| {
                                    cx.emit(OpenFile {
                                        path: path.to_path_buf(),
                                    });
                                }),
                            )
                            .child(
                                div()
                                    .w(px(28.0))
                                    .text_size(px(10.0))
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(theme.muted_foreground.opacity(0.55))
                                    .child(SharedString::from(line_number.to_string())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .font_family(theme.mono_font_family.clone())
                                    .text_color(theme.foreground.opacity(0.78))
                                    .child(highlighted_result_text(
                                        &result.preview,
                                        result.match_start,
                                        result.match_len,
                                        theme,
                                    )),
                            )
                            .into_any_element(),
                    );
                    idx += 1;
                }
            }
        }

        div()
            .id("sidebar-search")
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(8.0))
                    .pb(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .gap(px(7.0))
                            .px(px(8.0))
                            .rounded(px(7.0))
                            .bg(theme.foreground.opacity(0.045))
                            .child(
                                svg()
                                    .path("phosphor/magnifying-glass.svg")
                                    .size(px(13.0))
                                    .flex_shrink_0()
                                    .text_color(theme.muted_foreground.opacity(0.72)),
                            )
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.query)
                                        .appearance(false)
                                        .text_size(px(12.0))
                                        .line_height(px(17.0)),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(22.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .px(px(2.0))
                            .child(search_option_button(
                                "search-case-sensitive",
                                "Aa",
                                case_sensitive,
                                theme,
                                cx.listener(|this, checked: &bool, _window, cx| {
                                    this.set_case_sensitive(*checked, cx);
                                }),
                            ))
                            .child(search_option_button(
                                "search-regex",
                                ".*",
                                regex_enabled,
                                theme,
                                cx.listener(|this, checked: &bool, _window, cx| {
                                    this.set_regex(*checked, cx);
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        div()
                            .id("search-results-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.results_scroll_handle)
                            .child(div().flex().flex_col().children(rows)),
                    )
                    .child(
                        Scrollbar::vertical(&self.results_scroll_handle)
                            .scrollbar_show(ScrollbarShow::Always),
                    ),
            )
            .into_any_element()
    }
}

/// Bounded, UTF-8 safe window of `line` around the match, with the highlight
/// offsets rebased onto that window. Leading indentation is dropped, at most
/// `PREVIEW_LEAD_BYTES` of context precede the match, and the window never
/// exceeds `MAX_PREVIEW_BYTES`, so a match longer than the window is clipped.
fn result_preview_match(
    line: &str,
    match_start: usize,
    match_len: usize,
) -> (String, usize, usize) {
    let trim_offset = line.len() - line.trim_start().len();
    let match_start = match_start.min(line.len());
    let match_end = match_start.saturating_add(match_len).min(line.len());
    let start = floor_char_boundary(
        line,
        match_start
            .saturating_sub(PREVIEW_LEAD_BYTES)
            .max(trim_offset),
    );
    let end = floor_char_boundary(line, line.len().min(start + MAX_PREVIEW_BYTES));
    let visible_start = match_start.clamp(start, end);
    let visible_end = match_end.clamp(visible_start, end);

    (
        line[start..end].to_string(),
        visible_start - start,
        visible_end - visible_start,
    )
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn highlighted_result_text(
    preview: &SharedString,
    match_start: usize,
    match_len: usize,
    theme: &gpui_component::Theme,
) -> StyledText {
    let text = preview.clone();
    let base_style = TextStyle {
        color: theme.foreground.opacity(0.78),
        font_family: theme.mono_font_family.clone(),
        font_size: px(12.0).into(),
        line_height: px(18.0).into(),
        white_space: WhiteSpace::Nowrap,
        ..Default::default()
    };
    let mut runs = Vec::new();
    let match_start = match_start.min(preview.len());
    let match_len = match_len.min(preview.len().saturating_sub(match_start));
    let match_end = match_start + match_len;

    if match_start > 0 {
        runs.push(base_style.to_run(match_start));
    }
    if match_len > 0 {
        let mut match_style = base_style.clone();
        match_style.color = theme.foreground.opacity(0.96);
        match_style.background_color = Some(theme.warning.opacity(0.34));
        runs.push(match_style.to_run(match_len));
    }
    if match_end < preview.len() {
        runs.push(base_style.to_run(preview.len() - match_end));
    }
    if runs.is_empty() {
        runs.push(base_style.to_run(preview.len()));
    }

    StyledText::new(text).with_runs(runs)
}

fn result_count_badge(count: usize, theme: &gpui_component::Theme) -> Div {
    div()
        .min_w(px(20.0))
        .h(px(20.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(theme.primary.opacity(0.24))
        .text_size(px(11.0))
        .font_family(theme.mono_font_family.clone())
        .text_color(theme.foreground.opacity(0.82))
        .child(SharedString::from(count.to_string()))
}

fn search_option_button<F>(
    id: &'static str,
    label: &'static str,
    active: bool,
    theme: &gpui_component::Theme,
    handler: F,
) -> impl IntoElement
where
    F: Fn(&bool, &mut Window, &mut gpui::App) + 'static,
{
    div()
        .id(id)
        .h(px(24.0))
        .px(px(4.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .text_size(px(11.0))
        .font_family(theme.font_family.clone())
        .text_color(if active {
            theme.primary
        } else {
            theme.muted_foreground.opacity(0.86)
        })
        .child(label)
        .child(
            Switch::new(format!("{id}-switch"))
                .checked(active)
                .small()
                .on_click(handler),
        )
}

fn empty_state(text: &'static str, theme: &gpui_component::Theme) -> Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.0))
        .font_family(theme.font_family.clone())
        .text_color(theme.muted_foreground.opacity(0.5))
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEARCH_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_search_tree() -> PathBuf {
        let id = SEARCH_TEST_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vu-sidebar-search-test-{}-{id}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("README.md"), "hello world\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\nlet needle = 1;\n").unwrap();
        fs::write(root.join("src/lib.rs"), "Needle again\n").unwrap();
        root
    }

    #[test]
    fn search_files_finds_case_insensitive_matches_recursively() {
        let root = temp_search_tree();
        let results = search_files(
            &root,
            "needle",
            SearchOptions::default(),
            &AtomicBool::new(false),
        );

        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .any(|result| result.path.ends_with("main.rs") && result.line_number == 2)
        );
        assert!(
            results
                .iter()
                .any(|result| result.path.ends_with("lib.rs") && result.line_number == 1)
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_files_ignores_empty_query() {
        let root = temp_search_tree();
        assert!(
            search_files(
                &root,
                " ",
                SearchOptions::default(),
                &AtomicBool::new(false)
            )
            .is_empty()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_files_respects_case_sensitive_option() {
        let root = temp_search_tree();
        let results = search_files(
            &root,
            "Needle",
            SearchOptions {
                case_sensitive: true,
                regex: false,
            },
            &AtomicBool::new(false),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].path.ends_with("lib.rs"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn literal_case_insensitive_match_offsets_stay_in_original_line() {
        let matcher = SearchMatcher::new("needle", SearchOptions::default()).unwrap();

        assert_eq!(matcher.find("İstanbul needle"), Some((10, 6)));
    }

    #[test]
    fn search_files_supports_regex_option() {
        let root = temp_search_tree();
        let results = search_files(
            &root,
            r"need(le)?\s*=\s*1",
            SearchOptions {
                case_sensitive: false,
                regex: true,
            },
            &AtomicBool::new(false),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].path.ends_with("main.rs"));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn search_files_does_not_recurse_into_directory_symlinks() {
        let root = temp_search_tree();
        std::os::unix::fs::symlink(&root, root.join("src/loop")).unwrap();

        let results = search_files(
            &root,
            "hello",
            SearchOptions::default(),
            &AtomicBool::new(false),
        );

        assert_eq!(results.len(), 1);
        assert!(results[0].path.ends_with("README.md"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn result_preview_match_keeps_highlight_aligned_after_trimming_indent() {
        let (preview, match_start, match_len) = result_preview_match("    let needle = 1;", 8, 6);

        assert_eq!(preview, "let needle = 1;");
        assert_eq!(match_start, 4);
        assert_eq!(match_len, 6);
    }

    #[test]
    fn cancelled_search_stops_before_reading_files_or_appending_matches() {
        let root = temp_search_tree();
        let cancelled = AtomicBool::new(true);
        assert!(search_files(&root, "needle", SearchOptions::default(), &cancelled).is_empty());

        let matcher = SearchMatcher::new("needle", SearchOptions::default()).unwrap();
        let mut results = Vec::new();
        let mut files_seen = 0;
        search_dir(&root, &matcher, &mut files_seen, &mut results, &cancelled);
        assert_eq!(files_seen, 0);
        search_file(
            &root.join("src/main.rs"),
            &matcher,
            &mut results,
            &cancelled,
        );
        assert!(results.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn result_preview_match_bounds_late_multibyte_match_in_huge_line() {
        let mut line = "\t".to_string();
        line.push_str(&"var a=1;".repeat(64 * 1024));
        line.push_str("ünï 日本 ");
        line.push_str("needle");
        line.push_str(" tail€");
        let matcher = SearchMatcher::new("NEEDLE", SearchOptions::default()).unwrap();
        let (match_start, match_len) = matcher.find(&line).unwrap();

        let (preview, start, len) = result_preview_match(&line, match_start, match_len);

        assert!(line.len() >= MAX_FILE_BYTES as usize);
        assert!(preview.len() <= MAX_PREVIEW_BYTES);
        assert_eq!(&preview[start..start + len], "needle");
        assert!(preview.ends_with("ünï 日本 needle tail€"));
        assert!(preview.len() >= PREVIEW_LEAD_BYTES + "needle tail€".len());
    }

    #[test]
    fn result_preview_match_snaps_lead_context_to_char_boundary() {
        // 3-byte chars: match_start - PREVIEW_LEAD_BYTES lands mid-character.
        let line = format!("{}needle", "€".repeat(100));
        let match_start = line.find("needle").unwrap();
        assert!(!line.is_char_boundary(match_start - PREVIEW_LEAD_BYTES));

        let (preview, start, len) = result_preview_match(&line, match_start, 6);

        assert!(preview.starts_with('€'));
        assert_eq!(&preview[start..start + len], "needle");
        assert!(preview.len() <= MAX_PREVIEW_BYTES);
    }

    #[test]
    fn result_preview_match_clips_match_longer_than_limit() {
        let line = format!("{}é", "x".repeat(MAX_PREVIEW_BYTES * 3));
        let matcher = SearchMatcher::new(
            "x+é",
            SearchOptions {
                case_sensitive: false,
                regex: true,
            },
        )
        .unwrap();
        let (match_start, match_len) = matcher.find(&line).unwrap();
        assert_eq!((match_start, match_len), (0, line.len()));

        let (preview, start, len) = result_preview_match(&line, match_start, match_len);

        assert_eq!(preview.len(), MAX_PREVIEW_BYTES);
        assert_eq!((start, len), (0, MAX_PREVIEW_BYTES));
    }

    #[test]
    fn result_preview_match_handles_zero_width_and_indent_only_matches() {
        let (preview, start, len) = result_preview_match("    foo", 4, 0);
        assert_eq!((preview.as_str(), start, len), ("foo", 0, 0));

        // Match entirely inside the trimmed indentation collapses to zero width.
        let (preview, start, len) = result_preview_match("    foo", 1, 2);
        assert_eq!((preview.as_str(), start, len), ("foo", 0, 0));

        // Match straddling the indentation keeps only its visible tail.
        let (preview, start, len) = result_preview_match("    foo", 2, 4);
        assert_eq!((preview.as_str(), start, len), ("foo", 0, 2));

        let (preview, start, len) = result_preview_match("", 0, 0);
        assert_eq!((preview.as_str(), start, len), ("", 0, 0));
    }

    #[test]
    fn search_files_stores_bounded_previews_for_minified_file() {
        let root = temp_search_tree();
        let mut content = String::from("// banner\n");
        let mut minified = "let x=1;".repeat(65_000);
        minified.push_str("ünï nEEdle 日本語");
        content.push_str(&minified);
        content.push_str("\nfn tail() {}\n");
        assert!(content.len() as u64 <= MAX_FILE_BYTES);
        assert!(minified.len() > 500 * 1024);
        fs::write(root.join("src/app.min.js"), &content).unwrap();

        let results = search_files(
            &root,
            r"ne+dle\s+\p{Han}+",
            SearchOptions {
                case_sensitive: false,
                regex: true,
            },
            &AtomicBool::new(false),
        );

        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert!(result.path.ends_with("src/app.min.js"));
        assert_eq!(result.line_number, 2);
        assert!(result.preview.len() <= MAX_PREVIEW_BYTES);
        let highlighted =
            &result.preview[result.match_start..result.match_start + result.match_len];
        assert_eq!(highlighted, "nEEdle 日本語");

        // Same fixture, old storage kept the whole line.
        let stored_new: usize = results.iter().map(|r| r.preview.len()).sum();
        assert!(stored_new <= MAX_PREVIEW_BYTES);
        assert!(minified.len() / stored_new >= 2_000);

        let _ = fs::remove_dir_all(root);
    }
}
