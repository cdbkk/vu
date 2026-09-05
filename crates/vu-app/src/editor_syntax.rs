use gpui::{FontStyle, FontWeight, Hsla, Pixels, TextRun, TextStyle, WhiteSpace};
use gpui_component::Colorize;
use gpui_component::{
    Theme,
    highlighter::SyntaxHighlighter,
    input::{InputEdit, Point},
};
use std::path::Path;

/// Retains the parser and feeds it each changed byte range for incremental parsing.
pub(crate) struct SyntaxState {
    language: &'static str,
    text: String,
    highlighter: SyntaxHighlighter,
}

impl SyntaxState {
    fn new(language: &'static str) -> Self {
        Self {
            language,
            text: String::new(),
            highlighter: SyntaxHighlighter::new(language),
        }
    }

    fn sync(&mut self, text: &str) {
        if self.text == text {
            return;
        }
        let edit = text_edit(&self.text, text);
        let mut rope = self.highlighter.text().clone();
        rope.remove(edit.start_byte..edit.old_end_byte);
        rope.insert(edit.start_byte, &text[edit.start_byte..edit.new_end_byte]);
        self.highlighter.update(Some(edit), &rope, None);
        self.text.clear();
        self.text.push_str(text);
    }
}

/// Single edit turning `old` into `new`: the differing span between the
/// longest common prefix and suffix, snapped to char boundaries.
fn text_edit(old: &str, new: &str) -> InputEdit {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !(old.is_char_boundary(prefix) && new.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(old.len().min(new.len()) - prefix)
        .take_while(|(a, b)| a == b)
        .count();
    while !(old.is_char_boundary(old.len() - suffix) && new.is_char_boundary(new.len() - suffix)) {
        suffix -= 1;
    }
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    InputEdit {
        start_byte: prefix,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: point_at(old, prefix),
        old_end_position: point_at(old, old_end),
        new_end_position: point_at(new, new_end),
    }
}

fn point_at(text: &str, offset: usize) -> Point {
    let before = &text[..offset];
    let row = before.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    Point::new(row, offset - line_start)
}

pub(crate) fn language_for_path(path: &Path) -> Option<&'static str> {
    let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    match file_name.as_str() {
        "cargo.toml" | "pyproject.toml" => return Some("toml"),
        "package.json" | "tsconfig.json" => return Some("json"),
        "dockerfile" => return Some("dockerfile"),
        "makefile" => return Some("make"),
        _ => {}
    }

    match path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => Some("rust"),
        "toml" => Some("toml"),
        "json" | "jsonc" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "md" | "markdown" => Some("markdown"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("jsx"),
        "py" => Some("python"),
        "go" => Some("go"),
        "sh" | "bash" | "zsh" => Some("bash"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" => Some("scss"),
        "sql" => Some("sql"),
        _ => None,
    }
}

pub(crate) fn highlighted_line_runs(
    syntax: &mut Option<SyntaxState>,
    text: &str,
    lines: &[String],
    language: Option<&'static str>,
    theme: &Theme,
    mono_font_family: impl Into<gpui::SharedString>,
    font_size: Pixels,
    line_height: Pixels,
) -> Vec<Vec<TextRun>> {
    let base_style = base_text_style(
        theme.foreground.opacity(0.90),
        mono_font_family,
        font_size,
        line_height,
    );
    let Some(language) = language else {
        *syntax = None;
        return base_line_runs(lines, &base_style);
    };

    if syntax
        .as_ref()
        .is_some_and(|state| state.language != language)
    {
        *syntax = None;
    }
    let state = syntax.get_or_insert_with(|| SyntaxState::new(language));
    state.sync(text);
    let highlights = state
        .highlighter
        .styles(&(0..text.len()), &theme.highlight_theme);

    line_runs(lines, text, &highlights, &base_style, theme)
}

/// `highlights` is sorted and non-overlapping (see `unique_styles` upstream),
/// so one forward sweep maps them onto lines.
fn line_runs(
    lines: &[String],
    text: &str,
    highlights: &[(std::ops::Range<usize>, gpui::HighlightStyle)],
    base_style: &TextStyle,
    theme: &Theme,
) -> Vec<Vec<TextRun>> {
    let line_starts = line_start_offsets(text, lines.len());
    let mut next = 0;
    lines
        .iter()
        .zip(line_starts)
        .map(|(line, line_start)| {
            let line_end = line_start + line.len();
            while highlights
                .get(next)
                .is_some_and(|(range, _)| range.end <= line_start)
            {
                next += 1;
            }
            runs_for_line(
                line_start,
                line_end,
                line.len(),
                &highlights[next..],
                base_style,
                theme,
            )
        })
        .collect()
}

fn base_line_runs(lines: &[String], base_style: &TextStyle) -> Vec<Vec<TextRun>> {
    lines
        .iter()
        .map(|line| vec![base_style.to_run(line.len())])
        .collect()
}

fn line_start_offsets(text: &str, line_count: usize) -> Vec<usize> {
    let mut starts = Vec::with_capacity(line_count);
    starts.push(0);
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' && starts.len() < line_count {
            starts.push(index + 1);
        }
    }
    while starts.len() < line_count {
        starts.push(text.len());
    }
    starts
}

fn runs_for_line(
    line_start: usize,
    line_end: usize,
    line_len: usize,
    highlights: &[(std::ops::Range<usize>, gpui::HighlightStyle)],
    base_style: &TextStyle,
    theme: &Theme,
) -> Vec<TextRun> {
    if line_len == 0 {
        return vec![base_style.to_run(0)];
    }

    let mut runs = Vec::new();
    let mut cursor = line_start;

    for (range, highlight) in highlights {
        if range.start >= line_end {
            break;
        }
        let start = range.start.max(line_start).min(line_end);
        let end = range.end.max(line_start).min(line_end);
        if start >= end {
            continue;
        }

        if start > cursor {
            runs.push(base_style.to_run(start - cursor));
        }

        let mut style = base_style.clone();
        apply_highlight_style(&mut style, *highlight, theme);
        runs.push(style.to_run(end - start));
        cursor = end;
    }

    if cursor < line_end {
        runs.push(base_style.to_run(line_end - cursor));
    }

    if runs.is_empty() {
        runs.push(base_style.to_run(line_len));
    }
    runs
}

fn base_text_style(
    color: Hsla,
    mono_font_family: impl Into<gpui::SharedString>,
    font_size: Pixels,
    line_height: Pixels,
) -> TextStyle {
    TextStyle {
        color,
        font_family: mono_font_family.into(),
        font_size: font_size.into(),
        line_height: line_height.into(),
        font_weight: FontWeight::NORMAL,
        font_style: FontStyle::Normal,
        white_space: WhiteSpace::Nowrap,
        ..Default::default()
    }
}

fn apply_highlight_style(
    text_style: &mut TextStyle,
    highlight: gpui::HighlightStyle,
    theme: &Theme,
) {
    if let Some(color) = highlight.color {
        let base = if theme.is_dark() {
            theme.foreground.opacity(0.96)
        } else {
            theme.foreground.opacity(0.90)
        };
        text_style.color = color.mix_oklab(base, 0.76).opacity(0.99);
    }
    if let Some(weight) = highlight.font_weight {
        text_style.font_weight = weight;
    }
    if let Some(style) = highlight.font_style {
        text_style.font_style = style;
    }
    text_style.background_color = highlight.background_color;
    text_style.underline = highlight.underline;
    text_style.strikethrough = highlight.strikethrough;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_for_path_recognizes_common_editor_files() {
        for (path, language) in [
            ("src/main.rs", "rust"),
            ("Cargo.toml", "toml"),
            ("package.json", "json"),
            ("src/app.tsx", "tsx"),
            ("script.py", "python"),
            ("README.md", "markdown"),
            ("Dockerfile", "dockerfile"),
        ] {
            assert_eq!(language_for_path(Path::new(path)), Some(language));
        }
    }

    #[test]
    fn line_start_offsets_account_for_lf_newlines() {
        let text = "abc\nde\n\nf";

        assert_eq!(line_start_offsets(text, 4), vec![0, 4, 7, 8]);
    }

    #[test]
    fn line_start_offsets_account_for_crlf_newlines() {
        let text = "abc\r\nde\r\n\r\nf";

        assert_eq!(line_start_offsets(text, 4), vec![0, 5, 9, 11]);
    }

    use crate::editor_buffer::EditorBuffer;
    use gpui::{HighlightStyle, px};
    use gpui_component::highlighter::HighlightTheme;
    use ropey::Rope;
    use std::ops::Range;

    const RUST_SAMPLE: &str = "//! crate docs\n\
        use std::fmt;\n\
        \n\
        /* block comment\n   spanning lines with héllo 🌍 ไทย */\n\
        fn main() {\n\
            let greeting = \"héllo 🌍 \\\"quoted\\\"\";\n\
            let thai = 'ก';\n\
        \n\
            println!(\"{greeting}{thai}\"); // trailing\n\
        }\n";

    const HTML_SAMPLE: &str = "<html>\n\
        <style>\n  body { color: #ff0000; }\n</style>\n\
        <body>\n\
          <script>\n    const x = \"héllo 🌍\";\n    function f() { return x; }\n  </script>\n\
        </body>\n\
        </html>\n";

    const MARKDOWN_SAMPLE: &str = "# Title\n\
        \n\
        Some *emphasis* and `code` with 🌍.\n\
        \n\
        ```rust\n\
        fn injected() -> u8 { /* comment */ 1 }\n\
        ```\n\
        \n\
        - item\n";

    fn test_base_style(theme: &Theme, font_size: f32) -> TextStyle {
        base_text_style(
            theme.foreground.opacity(0.90),
            "Test Mono",
            px(font_size),
            px(font_size * 1.5),
        )
    }

    fn fresh_styles(
        language: &str,
        text: &str,
        theme: &Theme,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut highlighter = SyntaxHighlighter::new(language);
        highlighter.update(None, &Rope::from_str(text), None);
        highlighter.styles(&(0..text.len()), &theme.highlight_theme)
    }

    fn buffer_lines(text: &str) -> Vec<String> {
        EditorBuffer::from_text(text).lines().to_vec()
    }

    /// The previous mapping: every line scanned every highlight.
    fn nested_scan_runs(
        lines: &[String],
        text: &str,
        highlights: &[(Range<usize>, HighlightStyle)],
        base_style: &TextStyle,
        theme: &Theme,
    ) -> Vec<Vec<TextRun>> {
        let line_starts = line_start_offsets(text, lines.len());
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let line_start = line_starts[index];
                let line_end = line_start + line.len();
                if line.is_empty() {
                    return vec![base_style.to_run(0)];
                }
                let mut runs = Vec::new();
                let mut cursor = line_start;
                for (range, highlight) in highlights {
                    let start = range.start.max(line_start).min(line_end);
                    let end = range.end.max(line_start).min(line_end);
                    if start >= end {
                        continue;
                    }
                    if start > cursor {
                        runs.push(base_style.to_run(start - cursor));
                    }
                    let mut style = base_style.clone();
                    apply_highlight_style(&mut style, *highlight, theme);
                    runs.push(style.to_run(end - start));
                    cursor = end;
                }
                if cursor < line_end {
                    runs.push(base_style.to_run(line_end - cursor));
                }
                if runs.is_empty() {
                    runs.push(base_style.to_run(line.len()));
                }
                runs
            })
            .collect()
    }

    fn assert_runs_cover_lines(runs: &[Vec<TextRun>], lines: &[String]) {
        assert_eq!(runs.len(), lines.len());
        for (line_runs, line) in runs.iter().zip(lines) {
            let covered: usize = line_runs.iter().map(|run| run.len).sum();
            assert_eq!(covered, line.len(), "runs must cover {line:?} exactly");
        }
    }

    #[test]
    fn sweep_matches_nested_scan_for_real_highlights() {
        let theme = Theme::default();
        let base_style = test_base_style(&theme, 14.0);
        let crlf_rust = RUST_SAMPLE.replace('\n', "\r\n");
        let samples = [
            ("rust", RUST_SAMPLE),
            ("rust", crlf_rust.as_str()),
            ("html", HTML_SAMPLE),
            ("markdown", MARKDOWN_SAMPLE),
            ("rust", ""),
            ("rust", "\n\n\n"),
            ("rust", "no newline at end"),
        ];

        for (language, text) in samples {
            let lines = buffer_lines(text);
            let highlights = fresh_styles(language, text, &theme);
            let sweep = line_runs(&lines, text, &highlights, &base_style, &theme);
            let nested = nested_scan_runs(&lines, text, &highlights, &base_style, &theme);

            assert_eq!(sweep, nested, "{language}: {text:?}");
            assert_runs_cover_lines(&sweep, &lines);
        }

        let rust_highlights = fresh_styles("rust", RUST_SAMPLE, &theme);
        let distinct_colors = rust_highlights
            .iter()
            .filter_map(|(_, style)| style.color)
            .map(|color| format!("{color:?}"))
            .collect::<std::collections::HashSet<_>>();
        assert!(
            distinct_colors.len() > 2,
            "rust grammar must be active for this test to be meaningful"
        );
    }

    #[test]
    fn runs_for_line_stops_at_first_highlight_past_line_end() {
        let theme = Theme::default();
        let base_style = test_base_style(&theme, 14.0);
        let red = HighlightStyle {
            color: Some(gpui::red()),
            ..Default::default()
        };
        let highlights = vec![
            (0..3, red),
            (10..20, red),
            // Out of order on purpose: a sweep over sorted input never reads
            // past the first range starting at or after line_end.
            (1..2, red),
        ];

        let runs = runs_for_line(0, 5, 5, &highlights, &base_style, &theme);

        let lengths = runs.iter().map(|run| run.len).collect::<Vec<_>>();
        assert_eq!(lengths, vec![3, 2]);
        assert_ne!(runs[0].color, base_style.color);
        assert_eq!(runs[1].color, base_style.color);
    }

    #[test]
    fn incremental_sync_matches_fresh_parse() {
        let theme = Theme::default();
        let steps = [
            "fn main() {\n    let x = 1;\n}\n",
            "fn main() {\n    let x = 1;\n    let y = \"héllo 🌍\";\n}\n",
            "fn main() {\n    /* let x = 1;\n    let y = \"héllo 🌍\";\n}\n",
            "fn main() {\n    /* let x = 1;\n    let y = \"héllo 🌍\"; */\n}\n",
            "fn run_everything() {\n    /* let x = 1;\n    let y = \"héllo 🌍\"; */\n}\n",
            "fn run_everything() {\n    let y = \"héllo 🌍\";\n}\n",
            "// top\nfn run_everything() {\n    let y = \"héllo 🌍\";\n}\n\nfn tail() {}\n",
            "// top\nfn run_everything() {\n    let y = \"hello ก\";\n}\n\nfn tail() {}\n",
            "// top\r\nfn run_everything() {\r\n    let y = \"hello ก\";\r\n}\r\n\r\nfn tail() {}\r\n",
            "// top\r\nfn run_everything() {\r\n    let y = \"hello ก\";\r\n    let z = y;\r\n}\r\n\r\nfn tail() {}\r\n",
            "struct Unrelated;\n",
            "",
            "fn main() {}\n",
        ];

        let mut state = SyntaxState::new("rust");
        for text in steps {
            state.sync(text);

            assert_eq!(state.highlighter.text().to_string(), text);
            assert_eq!(
                state
                    .highlighter
                    .styles(&(0..text.len()), &theme.highlight_theme),
                fresh_styles("rust", text, &theme),
                "incremental parse diverged after syncing {text:?}"
            );
        }
    }

    #[test]
    fn incremental_sync_matches_fresh_parse_for_injections() {
        let theme = Theme::default();
        let closed =
            HTML_SAMPLE.replace("function f() { return x; }", "function g() { return 2; }");
        let unclosed = HTML_SAMPLE.replace("  </script>\n", "");
        let steps = [HTML_SAMPLE, closed.as_str(), unclosed.as_str(), HTML_SAMPLE];

        let mut state = SyntaxState::new("html");
        for text in steps {
            state.sync(text);

            assert_eq!(
                state
                    .highlighter
                    .styles(&(0..text.len()), &theme.highlight_theme),
                fresh_styles("html", text, &theme),
                "injection layers diverged after syncing {text:?}"
            );
        }
    }

    #[test]
    fn text_edit_spans_only_the_changed_bytes_on_char_boundaries() {
        let cases: [(&str, &str, [usize; 3], [(usize, usize); 3]); 8] = [
            ("héllo", "hèllo", [1, 3, 3], [(0, 1), (0, 3), (0, 3)]),
            ("a\nbc\nd", "a\nbXc\nd", [3, 3, 4], [(1, 1), (1, 1), (1, 2)]),
            ("abc", "abc\r\nxyz", [3, 3, 8], [(0, 3), (0, 3), (1, 3)]),
            ("aaaa", "aa", [2, 4, 2], [(0, 2), (0, 4), (0, 2)]),
            ("", "x", [0, 0, 1], [(0, 0), (0, 0), (0, 1)]),
            ("x", "", [0, 1, 0], [(0, 0), (0, 1), (0, 0)]),
            ("🌍a", "🌏a", [0, 4, 4], [(0, 0), (0, 4), (0, 4)]),
            ("a\r\nb", "a\r\nc", [3, 4, 4], [(1, 0), (1, 1), (1, 1)]),
        ];

        for (old, new, bytes, points) in cases {
            let edit = text_edit(old, new);

            assert_eq!(
                [edit.start_byte, edit.old_end_byte, edit.new_end_byte],
                bytes,
                "{old:?} -> {new:?}"
            );
            assert_eq!(
                [
                    (edit.start_position.row, edit.start_position.column),
                    (edit.old_end_position.row, edit.old_end_position.column),
                    (edit.new_end_position.row, edit.new_end_position.column),
                ],
                points,
                "{old:?} -> {new:?}"
            );
            assert!(old.is_char_boundary(edit.start_byte));
            assert!(old.is_char_boundary(edit.old_end_byte));
            assert!(new.is_char_boundary(edit.new_end_byte));
            let spliced = format!(
                "{}{}{}",
                &old[..edit.start_byte],
                &new[edit.start_byte..edit.new_end_byte],
                &old[edit.old_end_byte..]
            );
            assert_eq!(spliced, new);
        }
    }

    #[test]
    fn highlighted_line_runs_retains_state_and_tracks_language_theme_and_font() {
        let light = Theme::default();
        let mut dark = light.clone();
        dark.highlight_theme = HighlightTheme::default_dark();
        let lines = buffer_lines(RUST_SAMPLE);
        let mut state = None;

        let runs = |state: &mut Option<SyntaxState>, language, theme: &Theme, size| {
            highlighted_line_runs(
                state,
                RUST_SAMPLE,
                &lines,
                language,
                theme,
                "Test Mono",
                px(size),
                px(size * 1.5),
            )
        };

        let first = runs(&mut state, Some("rust"), &light, 14.0);
        assert_eq!(state.as_ref().map(|s| s.language), Some("rust"));
        assert_eq!(first, runs(&mut None, Some("rust"), &light, 14.0));

        let bigger = runs(&mut state, Some("rust"), &light, 18.0);
        // Font size belongs to text layout, not TextRun's syntax attributes.
        assert_eq!(bigger, first);
        assert_eq!(bigger, runs(&mut None, Some("rust"), &light, 18.0));

        let dark_runs = runs(&mut state, Some("rust"), &dark, 14.0);
        assert_ne!(dark_runs, first);
        assert_eq!(dark_runs, runs(&mut None, Some("rust"), &dark, 14.0));

        let toml_runs = runs(&mut state, Some("toml"), &light, 14.0);
        assert_eq!(state.as_ref().map(|s| s.language), Some("toml"));
        assert_eq!(toml_runs, runs(&mut None, Some("toml"), &light, 14.0));

        let plain = runs(&mut state, None, &light, 14.0);
        assert!(state.is_none());
        assert_eq!(
            plain,
            lines
                .iter()
                .map(|line| vec![test_base_style(&light, 14.0).to_run(line.len())])
                .collect::<Vec<_>>()
        );
    }

    fn synthetic_highlights(lines: &[String]) -> Vec<(Range<usize>, HighlightStyle)> {
        let styled = HighlightStyle {
            color: Some(gpui::red()),
            ..Default::default()
        };
        let mut highlights = Vec::new();
        let mut offset = 0;
        for line in lines {
            let len = line.len();
            highlights.push((offset..offset + len / 3, styled));
            highlights.push((
                offset + len / 3..offset + 2 * len / 3,
                HighlightStyle::default(),
            ));
            highlights.push((offset + 2 * len / 3..offset + len + 1, styled));
            offset += len + 1;
        }
        highlights
    }

    #[test]
    #[ignore = "benchmark: run with --ignored --nocapture"]
    fn bench_sweep_against_nested_scan() {
        let theme = Theme::default();
        let base_style = test_base_style(&theme, 14.0);
        let text = "    let value = compute(alpha, beta);\n".repeat(20_000);
        let lines = buffer_lines(&text);
        let highlights = synthetic_highlights(&lines);

        let started = std::time::Instant::now();
        let sweep = line_runs(&lines, &text, &highlights, &base_style, &theme);
        let sweep_time = started.elapsed();
        let started = std::time::Instant::now();
        let nested = nested_scan_runs(&lines, &text, &highlights, &base_style, &theme);
        let nested_time = started.elapsed();

        assert_eq!(sweep, nested);
        println!(
            "lines={} highlights={} sweep={sweep_time:?} nested={nested_time:?}",
            lines.len(),
            highlights.len()
        );
        assert!(sweep_time * 20 < nested_time);
    }
}
