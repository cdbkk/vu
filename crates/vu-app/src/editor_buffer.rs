//! Editable text buffer for the lightweight editor pane.
//!
//! This module deliberately has no GPUI dependencies so editing behavior stays
//! unit-testable and can evolve independently from rendering.

use std::collections::VecDeque;

const MAX_UNDO_ENTRIES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CursorPosition {
    pub row: usize,
    pub column: usize,
}

impl CursorPosition {
    pub const fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }
}

#[derive(Debug, Clone)]
pub struct EditorBuffer {
    lines: Vec<String>,
    line_ending: LineEnding,
    cursor: CursorPosition,
    selection_anchor: Option<CursorPosition>,
    dirty: bool,
    undo_stack: VecDeque<UndoEdit>,
    revision: u64,
}

/// One reversible edit: `start..end` in the current text replaced `removed`.
#[derive(Debug, Clone)]
struct UndoEdit {
    start: CursorPosition,
    end: CursorPosition,
    removed: String,
    cursor: CursorPosition,
    selection_anchor: Option<CursorPosition>,
    dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    fn detect(text: &str) -> Self {
        if text.contains("\r\n") {
            Self::Crlf
        } else {
            Self::Lf
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

impl EditorBuffer {
    pub fn from_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let line_ending = LineEnding::detect(&text);
        let mut lines = text.lines().map(ToString::to_string).collect::<Vec<_>>();
        if text.ends_with('\n') {
            lines.push(String::new());
        }
        if lines.is_empty() {
            lines.push(String::new());
        }

        Self {
            lines,
            line_ending,
            cursor: CursorPosition::new(0, 0),
            selection_anchor: None,
            dirty: false,
            undo_stack: VecDeque::new(),
            revision: 0,
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> CursorPosition {
        self.cursor
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_selection(&self) -> bool {
        self.normalized_selection().is_some()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_cursor(&mut self, row: usize, column: usize) {
        let row = row.min(self.lines.len().saturating_sub(1));
        let column = clamp_to_char_boundary(&self.lines[row], column.min(self.lines[row].len()));
        self.cursor = CursorPosition::new(row, column);
        self.clear_selection();
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let (start, end) = self.selection_or_cursor();
        self.apply_edit(start, end, text);
    }

    pub fn insert_newline(&mut self) {
        let (start, end) = self.selection_or_cursor();
        self.apply_edit(start, end, "\n");
    }

    pub fn delete_backward(&mut self) {
        let (mut start, end) = self.selection_or_cursor();
        if start == end {
            let Some(previous) = self.previous_position(end) else {
                return;
            };
            start = previous;
        }
        self.apply_edit(start, end, "");
    }

    pub fn delete_forward(&mut self) {
        let (start, mut end) = self.selection_or_cursor();
        if start == end {
            let Some(next) = self.next_position(start) else {
                return;
            };
            end = next;
        }
        self.apply_edit(start, end, "");
    }

    pub fn move_left(&mut self) {
        self.clear_selection();
        self.move_left_raw();
    }

    pub fn move_right(&mut self) {
        self.clear_selection();
        self.move_right_raw();
    }

    pub fn move_up(&mut self) {
        self.clear_selection();
        self.move_up_raw();
    }

    pub fn move_down(&mut self) {
        self.clear_selection();
        self.move_down_raw();
    }

    pub fn move_home(&mut self) {
        self.clear_selection();
        self.move_to_line_start();
    }

    pub fn move_end(&mut self) {
        self.clear_selection();
        self.move_to_line_end();
    }

    pub fn move_to_line_start(&mut self) {
        self.cursor.column = 0;
    }

    pub fn move_to_line_end(&mut self) {
        self.cursor.column = self.lines[self.cursor.row].len();
    }

    pub fn move_left_selecting(&mut self) {
        self.move_selecting(Self::move_left_raw);
    }

    pub fn move_right_selecting(&mut self) {
        self.move_selecting(Self::move_right_raw);
    }

    pub fn move_up_selecting(&mut self) {
        self.move_selecting(Self::move_up_raw);
    }

    pub fn move_down_selecting(&mut self) {
        self.move_selecting(Self::move_down_raw);
    }

    pub fn move_home_selecting(&mut self) {
        self.move_selecting(Self::move_to_line_start);
    }

    pub fn move_end_selecting(&mut self) {
        self.move_selecting(Self::move_to_line_end);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_selection(&mut self, anchor: CursorPosition, cursor: CursorPosition) {
        self.selection_anchor = Some(self.clamp_position(anchor));
        self.cursor = self.clamp_position(cursor);
    }

    pub fn select_word_at(&mut self, row: usize, column: usize) -> bool {
        let row = row.min(self.lines.len().saturating_sub(1));
        let Some((start, end)) = word_range_at_column(&self.lines[row], column) else {
            self.set_cursor(row, column);
            return false;
        };

        self.selection_anchor = Some(CursorPosition::new(row, start));
        self.cursor = CursorPosition::new(row, end);
        true
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(CursorPosition::new(0, 0));
        let last_row = self.lines.len().saturating_sub(1);
        self.cursor = CursorPosition::new(last_row, self.lines[last_row].len());
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.normalized_selection()?;
        Some(self.text_in_range(start, end, self.line_ending.as_str()))
    }

    pub fn cut_selection(&mut self) -> Option<String> {
        let (start, end) = self.normalized_selection()?;
        let text = self.text_in_range(start, end, self.line_ending.as_str());
        self.apply_edit(start, end, "");
        Some(text)
    }

    pub fn text(&self) -> String {
        self.lines.join(self.line_ending.as_str())
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    pub fn save_to(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.text())?;
        self.mark_clean();
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(edit) = self.undo_stack.pop_back() else {
            return false;
        };
        let _ = self.splice(edit.start, edit.end, &edit.removed);
        self.cursor = edit.cursor;
        self.selection_anchor = edit.selection_anchor;
        self.dirty = edit.dirty;
        self.bump_revision();
        true
    }

    fn apply_edit(&mut self, start: CursorPosition, end: CursorPosition, text: &str) {
        let normalized;
        let text = if text.contains('\r') {
            normalized = text.replace('\r', "");
            normalized.as_str()
        } else {
            text
        };
        let (cursor, selection_anchor, dirty) = (self.cursor, self.selection_anchor, self.dirty);
        let (removed, end_after) = self.splice(start, end, text);
        while self.undo_stack.len() >= MAX_UNDO_ENTRIES {
            let _ = self.undo_stack.pop_front();
        }
        self.undo_stack.push_back(UndoEdit {
            start,
            end: end_after,
            removed,
            cursor,
            selection_anchor,
            dirty,
        });
        self.cursor = end_after;
        self.clear_selection();
        self.dirty = true;
        self.bump_revision();
    }

    /// Undo text uses LF separators and preserves literal carriage returns in the source.
    /// Returns the replaced text and the position just past the inserted text.
    fn splice(
        &mut self,
        start: CursorPosition,
        end: CursorPosition,
        text: &str,
    ) -> (String, CursorPosition) {
        let removed = self.text_in_range(start, end, "\n");
        let tail = self.lines[end.row].split_off(end.column);
        self.lines[start.row].truncate(start.column);

        let mut pieces = text.split('\n');
        self.lines[start.row].push_str(pieces.next().expect("split yields a first piece"));
        let new_lines = pieces.map(str::to_string).collect::<Vec<_>>();
        let end_row = start.row + new_lines.len();
        self.lines.splice((start.row + 1)..=end.row, new_lines);

        let end_after = CursorPosition::new(end_row, self.lines[end_row].len());
        self.lines[end_row].push_str(&tail);
        (removed, end_after)
    }

    fn selection_or_cursor(&self) -> (CursorPosition, CursorPosition) {
        self.normalized_selection()
            .unwrap_or((self.cursor, self.cursor))
    }

    fn previous_position(&self, position: CursorPosition) -> Option<CursorPosition> {
        if position.column > 0 {
            let column = previous_char_start(&self.lines[position.row], position.column)?;
            Some(CursorPosition::new(position.row, column))
        } else if position.row > 0 {
            let row = position.row - 1;
            Some(CursorPosition::new(row, self.lines[row].len()))
        } else {
            None
        }
    }

    fn next_position(&self, position: CursorPosition) -> Option<CursorPosition> {
        let line = &self.lines[position.row];
        if position.column < line.len() {
            Some(CursorPosition::new(
                position.row,
                next_char_end(line, position.column),
            ))
        } else if position.row + 1 < self.lines.len() {
            Some(CursorPosition::new(position.row + 1, 0))
        } else {
            None
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    fn move_right_raw(&mut self) {
        if self.cursor.column < self.lines[self.cursor.row].len() {
            self.cursor.column = next_char_end(&self.lines[self.cursor.row], self.cursor.column);
        } else if self.cursor.row + 1 < self.lines.len() {
            self.cursor.row += 1;
            self.cursor.column = 0;
        }
    }

    fn move_left_raw(&mut self) {
        if self.cursor.column > 0 {
            self.cursor.column =
                previous_char_start(&self.lines[self.cursor.row], self.cursor.column).unwrap_or(0);
        } else if self.cursor.row > 0 {
            self.cursor.row -= 1;
            self.cursor.column = self.lines[self.cursor.row].len();
        }
    }

    fn move_up_raw(&mut self) {
        if self.cursor.row == 0 {
            return;
        }
        self.cursor.row -= 1;
        self.cursor.column = clamp_to_char_boundary(
            &self.lines[self.cursor.row],
            self.cursor.column.min(self.lines[self.cursor.row].len()),
        );
    }

    fn move_down_raw(&mut self) {
        if self.cursor.row + 1 >= self.lines.len() {
            return;
        }
        self.cursor.row += 1;
        self.cursor.column = clamp_to_char_boundary(
            &self.lines[self.cursor.row],
            self.cursor.column.min(self.lines[self.cursor.row].len()),
        );
    }

    fn move_selecting(&mut self, move_cursor: fn(&mut Self)) {
        let anchor = self.selection_anchor.unwrap_or(self.cursor);
        move_cursor(self);
        self.selection_anchor = Some(anchor);
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    pub fn normalized_selection(&self) -> Option<(CursorPosition, CursorPosition)> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        let (start, end) = if anchor <= self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };
        Some((self.clamp_position(start), self.clamp_position(end)))
    }

    fn clamp_position(&self, position: CursorPosition) -> CursorPosition {
        let row = position.row.min(self.lines.len().saturating_sub(1));
        CursorPosition::new(
            row,
            clamp_to_char_boundary(&self.lines[row], position.column.min(self.lines[row].len())),
        )
    }

    fn text_in_range(&self, start: CursorPosition, end: CursorPosition, separator: &str) -> String {
        if start.row == end.row {
            return self.lines[start.row][start.column..end.column].to_string();
        }

        let mut text = String::new();
        text.push_str(&self.lines[start.row][start.column..]);
        for row in (start.row + 1)..end.row {
            text.push_str(separator);
            text.push_str(&self.lines[row]);
        }
        text.push_str(separator);
        text.push_str(&self.lines[end.row][..end.column]);
        text
    }
}

fn word_range_at_column(line: &str, column: usize) -> Option<(usize, usize)> {
    if line.is_empty() {
        return None;
    }

    let column = clamp_to_char_boundary(line, column.min(line.len()));
    let target = if column < line.len() && is_word_char_at(line, column) {
        column
    } else if column == line.len() {
        previous_char_start(line, column).filter(|&start| is_word_char_at(line, start))?
    } else {
        return None;
    };

    let mut start = target;
    while let Some(previous) = previous_char_start(line, start) {
        if !is_word_char_at(line, previous) {
            break;
        }
        start = previous;
    }

    let mut end = next_char_end(line, target);
    while end < line.len() && is_word_char_at(line, end) {
        end = next_char_end(line, end);
    }

    Some((start, end))
}

fn is_word_char_at(line: &str, index: usize) -> bool {
    line[index..]
        .chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphanumeric())
}

fn previous_char_start(line: &str, index: usize) -> Option<usize> {
    line[..index].char_indices().last().map(|(index, _)| index)
}

fn next_char_end(line: &str, index: usize) -> usize {
    let ch = line[index..].chars().next().expect("char at byte index");
    index + ch.len_utf8()
}

fn clamp_to_char_boundary(line: &str, mut column: usize) -> usize {
    while column > 0 && !line.is_char_boundary(column) {
        column -= 1;
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_text_updates_line_cursor_and_dirty_state() {
        let mut buffer = EditorBuffer::from_text("hello");

        buffer.set_cursor(0, 5);
        buffer.insert_text(" world");

        assert_eq!(buffer.text(), "hello world");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 11));
        assert!(buffer.is_dirty());
    }

    #[test]
    fn undo_restores_text_cursor_selection_and_dirty_state() {
        let mut buffer = EditorBuffer::from_text("hello");

        buffer.set_cursor(0, 5);
        buffer.insert_text(" world");
        assert_eq!(buffer.text(), "hello world");

        assert!(buffer.undo());

        assert_eq!(buffer.text(), "hello");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 5));
        assert!(!buffer.has_selection());
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn undo_history_is_bounded() {
        let mut buffer = EditorBuffer::from_text("");

        for _ in 0..(MAX_UNDO_ENTRIES + 8) {
            buffer.insert_text("x");
        }

        assert_eq!(buffer.undo_stack.len(), MAX_UNDO_ENTRIES);
    }

    #[test]
    fn revision_changes_only_when_text_changes() {
        let mut buffer = EditorBuffer::from_text("hello");
        let initial_revision = buffer.revision();

        buffer.set_cursor(0, 2);
        assert_eq!(buffer.revision(), initial_revision);

        buffer.insert_text("!");
        assert!(buffer.revision() > initial_revision);
        let edited_revision = buffer.revision();

        assert!(buffer.undo());
        assert!(buffer.revision() > edited_revision);
    }

    #[test]
    fn enter_splits_line_and_backspace_joins_lines() {
        let mut buffer = EditorBuffer::from_text("hello world");

        buffer.set_cursor(0, 5);
        buffer.insert_newline();

        assert_eq!(buffer.lines(), &["hello".to_string(), " world".to_string()]);
        assert_eq!(buffer.cursor(), CursorPosition::new(1, 0));

        buffer.delete_backward();

        assert_eq!(buffer.text(), "hello world");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 5));
    }

    #[test]
    fn delete_forward_merges_next_line_at_line_end() {
        let mut buffer = EditorBuffer::from_text("hello\nworld");

        buffer.set_cursor(0, 5);
        buffer.delete_forward();

        assert_eq!(buffer.text(), "helloworld");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 5));
    }

    #[test]
    fn vertical_movement_clamps_column_to_target_line_length() {
        let mut buffer = EditorBuffer::from_text("abcdef\nxy");

        buffer.set_cursor(0, 5);
        buffer.move_down();

        assert_eq!(buffer.cursor(), CursorPosition::new(1, 2));
    }

    #[test]
    fn horizontal_movement_keeps_cursor_on_utf8_boundaries() {
        let mut buffer = EditorBuffer::from_text("éx");

        buffer.move_right();
        assert_eq!(buffer.cursor(), CursorPosition::new(0, "é".len()));

        buffer.insert_text("!");
        assert_eq!(buffer.text(), "é!x");

        buffer.move_left();
        assert_eq!(buffer.cursor(), CursorPosition::new(0, "é".len()));
        buffer.delete_backward();

        assert_eq!(buffer.text(), "!x");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 0));
    }

    #[test]
    fn vertical_movement_clamps_to_utf8_boundary() {
        let mut buffer = EditorBuffer::from_text("ab\né");

        buffer.set_cursor(0, 1);
        buffer.move_down();
        assert!(buffer.lines()[1].is_char_boundary(buffer.cursor().column));

        buffer.insert_text("!");
        assert_eq!(buffer.text(), "ab\n!é");
    }

    #[test]
    fn select_all_and_replace_with_text() {
        let mut buffer = EditorBuffer::from_text("hello\nworld");

        buffer.select_all();
        assert_eq!(buffer.selected_text().as_deref(), Some("hello\nworld"));
        buffer.insert_text("replacement");

        assert_eq!(buffer.text(), "replacement");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 11));
        assert!(!buffer.has_selection());
    }

    #[test]
    fn copy_and_cut_selected_text() {
        let mut buffer = EditorBuffer::from_text("hello world");
        buffer.set_selection(CursorPosition::new(0, 6), CursorPosition::new(0, 11));

        assert_eq!(buffer.selected_text().as_deref(), Some("world"));
        assert_eq!(buffer.cut_selection().as_deref(), Some("world"));
        assert_eq!(buffer.text(), "hello ");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn ctrl_a_and_ctrl_e_move_to_line_boundaries() {
        let mut buffer = EditorBuffer::from_text("hello world");
        buffer.set_cursor(0, 5);

        buffer.move_to_line_start();
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 0));

        buffer.move_to_line_end();
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 11));
    }

    #[test]
    fn non_selecting_line_boundary_moves_clear_selection() {
        let mut buffer = EditorBuffer::from_text("hello world");
        buffer.set_selection(CursorPosition::new(0, 0), CursorPosition::new(0, 5));

        buffer.move_end();

        assert_eq!(buffer.cursor(), CursorPosition::new(0, 11));
        assert!(!buffer.has_selection());

        buffer.set_selection(CursorPosition::new(0, 0), CursorPosition::new(0, 5));

        buffer.move_home();

        assert_eq!(buffer.cursor(), CursorPosition::new(0, 0));
        assert!(!buffer.has_selection());
    }

    #[test]
    fn shift_movement_extends_selection() {
        let mut buffer = EditorBuffer::from_text("hello");
        buffer.set_cursor(0, 1);

        buffer.move_right_selecting();
        buffer.move_right_selecting();

        assert_eq!(buffer.cursor(), CursorPosition::new(0, 3));
        assert_eq!(buffer.selected_text().as_deref(), Some("el"));

        buffer.move_left();
        assert!(!buffer.has_selection());
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 2));
    }

    #[test]
    fn shift_vertical_movement_extends_multiline_selection() {
        let mut buffer = EditorBuffer::from_text("abc\ndef");
        buffer.set_cursor(0, 1);

        buffer.move_down_selecting();

        assert_eq!(buffer.cursor(), CursorPosition::new(1, 1));
        assert_eq!(buffer.selected_text().as_deref(), Some("bc\nd"));
    }

    #[test]
    fn selecting_empty_range_does_not_copy_or_cut() {
        let mut buffer = EditorBuffer::from_text("hello");
        buffer.set_selection(CursorPosition::new(0, 2), CursorPosition::new(0, 2));

        assert_eq!(buffer.selected_text(), None);
        assert_eq!(buffer.cut_selection(), None);
        assert_eq!(buffer.text(), "hello");
    }

    #[test]
    fn select_word_at_selects_identifier_under_cursor() {
        let mut buffer = EditorBuffer::from_text("pub use iceberg_inspect::IcebergInspectTable;");

        assert!(buffer.select_word_at(0, 10));

        assert_eq!(buffer.cursor(), CursorPosition::new(0, 23));
        assert_eq!(buffer.selected_text().as_deref(), Some("iceberg_inspect"));
    }

    #[test]
    fn select_word_at_line_end_selects_previous_word() {
        let mut buffer = EditorBuffer::from_text("hello");

        assert!(buffer.select_word_at(0, 5));

        assert_eq!(buffer.selected_text().as_deref(), Some("hello"));
    }

    #[test]
    fn select_word_at_separator_places_cursor_without_selection() {
        let mut buffer = EditorBuffer::from_text("hello world");

        assert!(!buffer.select_word_at(0, 5));

        assert_eq!(buffer.cursor(), CursorPosition::new(0, 5));
        assert!(!buffer.has_selection());
    }

    #[test]
    fn save_writes_text_to_disk_and_marks_buffer_clean() {
        let path = std::env::temp_dir().join(format!(
            "vu-editor-buffer-save-{}-{}.txt",
            std::process::id(),
            unique_suffix()
        ));
        let mut buffer = EditorBuffer::from_text("hello");
        buffer.set_cursor(0, 5);
        buffer.insert_text(" world");

        buffer.save_to(&path).expect("save buffer");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
        assert!(!buffer.is_dirty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn text_preserves_detected_crlf_line_endings() {
        let mut buffer = EditorBuffer::from_text("hello\r\nworld\r\n");

        buffer.set_cursor(1, 5);
        buffer.insert_text("!");

        assert_eq!(buffer.text(), "hello\r\nworld!\r\n");
    }

    #[test]
    fn undo_reverts_multiline_paste_and_restores_cursor() {
        let mut buffer = EditorBuffer::from_text("ab\ncd");
        buffer.set_cursor(0, 1);

        buffer.insert_text("1\r\n2\n3");
        assert_eq!(buffer.text(), "a1\n2\n3b\ncd");
        assert_eq!(buffer.cursor(), CursorPosition::new(2, 1));

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "ab\ncd");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 1));
        assert!(!buffer.is_dirty());
        assert!(!buffer.undo());
    }

    #[test]
    fn undo_reverts_line_joins_in_both_directions() {
        let mut buffer = EditorBuffer::from_text("héllo\nwörld\n!");

        buffer.set_cursor(1, 0);
        buffer.delete_backward();
        assert_eq!(buffer.text(), "héllowörld\n!");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, "héllo".len()));

        buffer.move_to_line_end();
        buffer.delete_forward();
        assert_eq!(buffer.text(), "héllowörld!");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "héllowörld\n!");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, "héllowörld".len()));

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "héllo\nwörld\n!");
        assert_eq!(buffer.cursor(), CursorPosition::new(1, 0));
    }

    #[test]
    fn undo_reverts_multiline_selection_replacement_and_restores_selection() {
        let mut buffer = EditorBuffer::from_text("one\ntwo\nthree");
        buffer.set_selection(CursorPosition::new(2, 2), CursorPosition::new(0, 1));

        buffer.insert_text("X");
        assert_eq!(buffer.text(), "oXree");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 2));
        assert!(!buffer.has_selection());

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "one\ntwo\nthree");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 1));
        assert_eq!(
            buffer.normalized_selection(),
            Some((CursorPosition::new(0, 1), CursorPosition::new(2, 2)))
        );
    }

    #[test]
    fn undo_reverts_unicode_deletes_on_char_boundaries() {
        let mut buffer = EditorBuffer::from_text("a😀é");
        buffer.set_cursor(0, "a😀".len());

        buffer.delete_backward();
        assert_eq!(buffer.text(), "aé");
        buffer.delete_forward();
        assert_eq!(buffer.text(), "a");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "aé");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 1));

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "a😀é");
        assert_eq!(buffer.cursor(), CursorPosition::new(0, "a😀".len()));
    }

    #[test]
    fn undo_restores_crlf_buffer_after_multiline_cut() {
        let mut buffer = EditorBuffer::from_text("one\r\ntwo\r\nthree\r\n");
        buffer.set_selection(CursorPosition::new(0, 1), CursorPosition::new(2, 2));

        assert_eq!(buffer.cut_selection().as_deref(), Some("ne\r\ntwo\r\nth"));
        assert_eq!(buffer.text(), "oree\r\n");

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "one\r\ntwo\r\nthree\r\n");
        assert_eq!(buffer.lines().len(), 4);
        assert!(buffer.lines().iter().all(|line| !line.contains('\r')));
        assert_eq!(buffer.selected_text().as_deref(), Some("ne\r\ntwo\r\nth"));
    }

    #[test]
    fn undo_after_mark_clean_restores_recorded_dirty_state() {
        let mut buffer = EditorBuffer::from_text("hello");
        buffer.set_cursor(0, 5);
        buffer.insert_text("!");
        buffer.mark_clean();
        buffer.insert_text("?");
        assert!(buffer.is_dirty());

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "hello!");
        assert!(!buffer.is_dirty());

        assert!(buffer.undo());
        assert_eq!(buffer.text(), "hello");
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn no_op_edits_do_not_record_undo_entries() {
        let mut buffer = EditorBuffer::from_text("ab");
        let revision = buffer.revision();

        buffer.delete_backward();
        buffer.insert_text("");
        buffer.set_cursor(0, 2);
        buffer.delete_forward();

        assert!(buffer.undo_stack.is_empty());
        assert_eq!(buffer.revision(), revision);
        assert!(!buffer.is_dirty());
        assert!(!buffer.undo());
        assert_eq!(buffer.text(), "ab");
    }

    #[test]
    fn undo_cap_evicts_oldest_edits_first() {
        let mut buffer = EditorBuffer::from_text("");
        for _ in 0..(MAX_UNDO_ENTRIES + 8) {
            buffer.insert_text("x");
        }

        let mut undone = 0;
        while buffer.undo() {
            undone += 1;
        }

        assert_eq!(undone, MAX_UNDO_ENTRIES);
        assert_eq!(buffer.text(), "x".repeat(8));
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 8));
        assert!(buffer.is_dirty());
    }

    #[test]
    fn undo_replays_scripted_history_exactly() {
        type Step = fn(&mut EditorBuffer);
        let steps: Vec<(Step, Step)> = vec![
            (|b| b.set_cursor(0, 2), |b| b.insert_text("ÿ")),
            (|_| {}, |b| b.insert_newline()),
            (
                |b| b.set_selection(CursorPosition::new(0, 1), CursorPosition::new(2, 3)),
                |b| b.delete_backward(),
            ),
            (|b| b.select_all(), |b| b.insert_text("a\nb\r\nc")),
            (|b| b.set_cursor(1, 0), |b| b.delete_backward()),
            (|b| b.set_cursor(0, 0), |b| b.delete_forward()),
            (
                |b| b.set_selection(CursorPosition::new(0, 0), CursorPosition::new(1, 0)),
                |b| {
                    b.cut_selection();
                },
            ),
            (|b| b.select_all(), |b| b.delete_forward()),
        ];
        let snapshot =
            |b: &EditorBuffer| (b.text(), b.cursor(), b.normalized_selection(), b.is_dirty());

        let mut buffer = EditorBuffer::from_text("héllo wörld\nsecond\n");
        let mut states = Vec::new();
        for (setup, edit) in &steps {
            setup(&mut buffer);
            states.push(snapshot(&buffer));
            edit(&mut buffer);
        }
        assert_eq!(buffer.text(), "");

        for expected in states.iter().rev() {
            assert!(buffer.undo());
            assert_eq!(&snapshot(&buffer), expected);
        }
        assert!(!buffer.undo());
    }

    #[test]
    fn single_char_edits_retain_only_edit_deltas() {
        let text = ("x".repeat(127) + "\n").repeat(8192);
        let mut buffer = EditorBuffer::from_text(text.clone());
        for _ in 0..64 {
            buffer.insert_text("a");
        }

        let retained = buffer
            .undo_stack
            .iter()
            .map(|edit| std::mem::size_of::<UndoEdit>() + edit.removed.capacity())
            .sum::<usize>();
        assert!(
            retained < 16 * 1024,
            "undo history retains {retained} bytes for 64 one-character edits"
        );

        for _ in 0..64 {
            assert!(buffer.undo());
        }
        assert_eq!(buffer.text(), text);
    }

    #[test]
    fn undo_restores_literal_carriage_returns_without_changing_clipboard_line_endings() {
        for original in ["a\rb", "a\rb\nc", "a\rb\r\nc\r\n"] {
            let mut buffer = EditorBuffer::from_text(original);
            buffer.select_all();
            assert_eq!(buffer.cut_selection().as_deref(), Some(original));
            assert!(buffer.undo());
            assert_eq!(buffer.text(), original);
            buffer.insert_text("x\r\ny");
            assert!(buffer.undo());
            assert_eq!(buffer.text(), original);
        }
    }

    #[test]
    fn collapsed_selection_does_not_survive_a_line_join() {
        let mut buffer = EditorBuffer::from_text("abc\n");
        let cursor = CursorPosition::new(1, 0);
        buffer.set_selection(cursor, cursor);
        buffer.delete_backward();
        assert_eq!(buffer.text(), "abc");
        assert_eq!(buffer.selected_text(), None);
        assert_eq!(buffer.cursor(), CursorPosition::new(0, 3));
        assert!(buffer.undo());
        assert_eq!(buffer.text(), "abc\n");
        assert_eq!(buffer.cursor(), cursor);
        assert_eq!(buffer.selected_text(), None);
    }

    fn unique_suffix() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}
