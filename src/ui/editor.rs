// ---------- Simple text editor with lightweight syntax highlighting ----------
use std::cell::Cell;
use std::collections::VecDeque;

#[derive(Clone)]
struct EditorState {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

#[derive(PartialEq, Clone, Copy, Default)]
enum LastOp {
    #[default]
    Other,
    Char,
}

#[derive(Default)]
pub struct Editor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub selection_anchor: Option<(usize, usize)>,
    history: VecDeque<EditorState>,
    redo_stack: VecDeque<EditorState>,
    /// Set by the render function each frame so page_up/page_down use the real visible height.
    pub page_size: Cell<usize>,
    /// Stable scroll offset — written by stable_scroll_start(), read by mouse handler.
    pub scroll_offset: Cell<usize>,
    last_op: LastOp,
}

impl Editor {
    pub fn with_sample() -> Self {
        let sample = vec![
            ".data".to_string(),
            "    msg: .ascii \"Hello, world!\"".to_string(),
            "    break_line: .byte 10".to_string(),
            "    len = . - msg".to_string(),
            ".text".to_string(),
            ".globl _start".to_string(),
            "_start:".to_string(),
            "    li a0, 1".to_string(),   // fd=1 (stdout)
            "    la a1, msg".to_string(), // buf
            "    li a2, len".to_string(), // count
            "    li a7, 64".to_string(),  // write
            "    ecall".to_string(),
            "    li a0, 0".to_string(),  // status
            "    li a7, 93".to_string(), // exit
            "    ecall".to_string(),
        ];
        Self {
            lines: sample,
            cursor_row: 0,
            cursor_col: 0,
            selection_anchor: None,
            history: VecDeque::new(),
            redo_stack: VecDeque::new(),
            page_size: Cell::new(20),
            scroll_offset: Cell::new(0),
            last_op: LastOp::Other,
        }
    }

    pub fn snapshot(&mut self) {
        if self.history.len() == 50 {
            self.history.pop_front();
        }
        self.history.push_back(EditorState {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
        // Any explicit snapshot (non-char edit) invalidates the redo history.
        self.redo_stack.clear();
        self.last_op = LastOp::Other;
    }

    pub fn undo(&mut self) {
        if let Some(state) = self.history.pop_back() {
            // Save current state so redo can restore it.
            if self.redo_stack.len() == 50 {
                self.redo_stack.pop_front();
            }
            self.redo_stack.push_back(EditorState {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
            });
            self.lines = state.lines;
            self.cursor_row = state.cursor_row;
            self.cursor_col = state.cursor_col;
            self.clear_selection();
            self.last_op = LastOp::Other;
        }
    }

    pub fn redo(&mut self) {
        if let Some(state) = self.redo_stack.pop_back() {
            // Save current state back to history.
            if self.history.len() == 50 {
                self.history.pop_front();
            }
            self.history.push_back(EditorState {
                lines: self.lines.clone(),
                cursor_row: self.cursor_row,
                cursor_col: self.cursor_col,
            });
            self.lines = state.lines;
            self.cursor_row = state.cursor_row;
            self.cursor_col = state.cursor_col;
            self.clear_selection();
            self.last_op = LastOp::Other;
        }
    }

    #[inline]
    pub fn ensure_line(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }
    }
    #[inline]
    pub fn current_line(&self) -> &str {
        self.lines
            .get(self.cursor_row)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
    #[inline]
    pub fn current_line_mut(&mut self) -> &mut String {
        self.ensure_line();
        &mut self.lines[self.cursor_row]
    }

    // ---- helpers: work with character indices, convert to byte offsets when needed
    #[inline]
    pub fn char_count(s: &str) -> usize {
        s.chars().count()
    }
    #[inline]
    pub fn byte_at(s: &str, char_pos: usize) -> usize {
        // return len() if beyond the end
        s.char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or_else(|| s.len())
    }

    pub fn clear_selection(&mut self) {
        self.selection_anchor = None;
    }

    pub fn start_selection(&mut self) {
        if self.selection_anchor.is_none() {
            self.selection_anchor = Some((self.cursor_row, self.cursor_col));
        }
    }

    pub fn select_all(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.selection_anchor = Some((0, 0));
        self.cursor_row = self.lines.len() - 1;
        self.cursor_col = Self::char_count(&self.lines[self.cursor_row]);
        self.last_op = LastOp::Other;
    }

    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        self.selection_anchor.map(|(sr, sc)| {
            let mut start = (sr, sc);
            let mut end = (self.cursor_row, self.cursor_col);
            if start > end {
                std::mem::swap(&mut start, &mut end);
            }
            (start, end)
        })
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection_range().map(|(start, end)| {
            let (sr, sc) = start;
            let (er, ec) = end;
            if sr == er {
                let line = &self.lines[sr];
                let sb = Self::byte_at(line, sc);
                let eb = Self::byte_at(line, ec);
                line[sb..eb].to_string()
            } else {
                let mut out = String::new();
                let first = &self.lines[sr];
                let sb = Self::byte_at(first, sc);
                out.push_str(&first[sb..]);
                out.push('\n');
                for row in sr + 1..er {
                    out.push_str(&self.lines[row]);
                    out.push('\n');
                }
                let last = &self.lines[er];
                let eb = Self::byte_at(last, ec);
                out.push_str(&last[..eb]);
                out
            }
        })
    }

    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.snapshot();
            self.delete_range(start, end);
        }
    }

    fn delete_range(&mut self, start: (usize, usize), end: (usize, usize)) {
        let (sr, sc) = start;
        let (er, ec) = end;
        if sr == er {
            let line = &mut self.lines[sr];
            let sb = Self::byte_at(line, sc);
            let eb = Self::byte_at(line, ec);
            line.replace_range(sb..eb, "");
        } else {
            let tail = {
                let last = &self.lines[er];
                let eb = Self::byte_at(last, ec);
                last[eb..].to_string()
            };
            {
                let first = &mut self.lines[sr];
                let sb = Self::byte_at(first, sc);
                first.truncate(sb);
                first.push_str(&tail);
            }
            self.lines.drain(sr + 1..=er);
        }
        self.cursor_row = sr;
        self.cursor_col = sc;
        self.clear_selection();
    }

    fn insert_char_internal(&mut self, ch: char) {
        self.ensure_line();
        let line = self.current_line();
        let col = self.cursor_col.min(Self::char_count(line));
        let byte_idx = Self::byte_at(line, col);
        self.current_line_mut().insert(byte_idx, ch);
        self.cursor_col = col + 1;
    }

    pub fn insert_char(&mut self, ch: char) {
        // Coalesce consecutive char inserts into a single undo group.
        // A selection-replace always starts a new group so the delete isn't lost.
        let has_selection = self.selection_range().is_some();
        if self.last_op != LastOp::Char || has_selection {
            self.snapshot(); // also clears redo_stack and sets last_op = Other
        }
        self.last_op = LastOp::Char;
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
        }
        self.insert_char_internal(ch);
    }

    pub fn insert_spaces(&mut self, n: usize) {
        self.snapshot();
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
        }
        for _ in 0..n {
            self.insert_char_internal(' ');
        }
    }

    pub fn tab(&mut self) {
        self.snapshot();
        if let Some(((sr, _), (er, _))) = self.selection_range() {
            for row in sr..=er {
                if row < self.lines.len() {
                    self.lines[row].insert_str(0, "    ");
                }
            }
            if let Some((ar, ac)) = self.selection_anchor {
                if ar >= sr && ar <= er {
                    self.selection_anchor = Some((ar, ac + 4));
                }
            }
            if self.cursor_row >= sr && self.cursor_row <= er {
                self.cursor_col += 4;
            }
        } else {
            for _ in 0..4 {
                self.insert_char_internal(' ');
            }
        }
    }

    pub fn shift_tab(&mut self) {
        self.snapshot();
        if let Some(((sr, _), (er, _))) = self.selection_range() {
            for row in sr..=er {
                if row < self.lines.len() {
                    let removed = self.lines[row]
                        .chars()
                        .take(4)
                        .take_while(|c| *c == ' ')
                        .count();
                    let byte = Self::byte_at(&self.lines[row], removed);
                    self.lines[row].replace_range(0..byte, "");
                    if let Some((ar, ac)) = self.selection_anchor {
                        if ar == row {
                            self.selection_anchor = Some((ar, ac.saturating_sub(removed)));
                        }
                    }
                    if self.cursor_row == row {
                        self.cursor_col = self.cursor_col.saturating_sub(removed);
                    }
                }
            }
        } else {
            let removed = self
                .current_line()
                .chars()
                .take(4)
                .take_while(|c| *c == ' ')
                .count();
            if removed > 0 {
                let byte = Self::byte_at(self.current_line(), removed);
                self.current_line_mut().replace_range(0..byte, "");
                self.cursor_col = self.cursor_col.saturating_sub(removed);
            }
        }
    }

    pub fn backspace(&mut self) {
        self.snapshot();
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
            return;
        }
        if self.lines.is_empty() {
            return;
        }
        if self.cursor_col > 0 {
            // remove the character before the cursor
            let col = self.cursor_col - 1;
            let (start, end) = {
                let line = self.current_line();
                (Self::byte_at(line, col), Self::byte_at(line, col + 1))
            };
            self.current_line_mut().replace_range(start..end, "");
            self.cursor_col = col;
        } else if self.cursor_row > 0 {
            // merge with the previous line
            let cur = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_len_chars = Self::char_count(&self.lines[self.cursor_row]);
            self.lines[self.cursor_row].push_str(&cur);
            self.cursor_col = prev_len_chars;
        }
    }

    pub fn delete_char(&mut self) {
        self.snapshot();
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
            return;
        }
        if self.lines.is_empty() {
            return;
        }
        let len_chars = Self::char_count(self.current_line());
        let col = self.cursor_col.min(len_chars);
        if col < len_chars {
            // delete at the current position
            let (start, end) = {
                let line = self.current_line();
                (Self::byte_at(line, col), Self::byte_at(line, col + 1))
            };
            self.current_line_mut().replace_range(start..end, "");
        } else if self.cursor_row + 1 < self.lines.len() {
            // end of line: merge with the next
            let next = self.lines.remove(self.cursor_row + 1);
            self.current_line_mut().push_str(&next);
        }
    }

    /// Delete from the cursor back to the start of the previous word (Ctrl+Backspace).
    pub fn delete_word_back(&mut self) {
        self.snapshot();
        if self.lines.is_empty() {
            return;
        }
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                let cur = self.lines.remove(self.cursor_row);
                self.cursor_row -= 1;
                let prev_len = Self::char_count(&self.lines[self.cursor_row]);
                self.lines[self.cursor_row].push_str(&cur);
                self.cursor_col = prev_len;
            }
            return;
        }
        let end_col = self.cursor_col;
        let start_col = word_left_col(self.current_line(), end_col);
        if start_col < end_col {
            let row = self.cursor_row;
            let sb = Self::byte_at(&self.lines[row], start_col);
            let eb = Self::byte_at(&self.lines[row], end_col);
            self.lines[row].replace_range(sb..eb, "");
            self.cursor_col = start_col;
        }
    }

    /// Delete from the cursor forward to the end of the next word (Ctrl+Delete).
    pub fn delete_word_forward(&mut self) {
        self.snapshot();
        if self.lines.is_empty() {
            return;
        }
        let len = Self::char_count(self.current_line());
        let col = self.cursor_col.min(len);
        if col >= len {
            if self.cursor_row + 1 < self.lines.len() {
                let next = self.lines.remove(self.cursor_row + 1);
                self.current_line_mut().push_str(&next);
            }
            return;
        }
        let end_col = word_right_col(self.current_line(), col);
        if end_col > col {
            let row = self.cursor_row;
            let sb = Self::byte_at(&self.lines[row], col);
            let eb = Self::byte_at(&self.lines[row], end_col);
            self.lines[row].replace_range(sb..eb, "");
        }
    }

    pub fn enter(&mut self) {
        self.snapshot();
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
        }
        self.ensure_line();
        // Auto-indent: carry the leading whitespace of the current line.
        let indent: String = self
            .current_line()
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let (idx_bytes, rest) = {
            let line = self.current_line();
            let idx = Self::byte_at(line, self.cursor_col.min(Self::char_count(line)));
            (idx, line[idx..].to_string())
        };
        {
            let line_mut = self.current_line_mut();
            line_mut.truncate(idx_bytes);
        }
        self.cursor_row += 1;
        self.cursor_col = indent.chars().count();
        self.lines.insert(self.cursor_row, indent + &rest);
    }

    /// Toggle `;` comment on the current line (or each selected line).
    pub fn toggle_comment(&mut self) {
        self.snapshot();
        self.ensure_line();
        let (start_row, end_row) = if let Some(((sr, _), (er, _))) = self.selection_range() {
            (sr, er)
        } else {
            (self.cursor_row, self.cursor_row)
        };

        // Determine if ALL non-empty lines in range already start with ';'
        let all_commented = (start_row..=end_row).all(|r| {
            if r >= self.lines.len() {
                return true;
            }
            let trimmed = self.lines[r].trim_start();
            trimmed.is_empty() || trimmed.starts_with(';')
        });

        // Compute cursor column delta from the pre-modification state of the cursor row,
        // before the loop below mutates the lines.
        let cursor_delta: isize = if self.cursor_row >= start_row
            && self.cursor_row <= end_row
            && self.cursor_row < self.lines.len()
        {
            if all_commented {
                let ws_b: usize = self.lines[self.cursor_row]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .map(|c| c.len_utf8())
                    .sum();
                let content = &self.lines[self.cursor_row][ws_b..];
                if content.starts_with("; ") {
                    -2
                } else if content.starts_with(';') {
                    -1
                } else {
                    0
                }
            } else {
                // Only adjust if "; " will actually be inserted (non-empty line)
                if !self.lines[self.cursor_row].trim_start().is_empty() {
                    2
                } else {
                    0
                }
            }
        } else {
            0
        };

        for r in start_row..=end_row {
            if r >= self.lines.len() {
                break;
            }
            // Byte offset of the first non-whitespace character.
            let ws_bytes: usize = self.lines[r]
                .chars()
                .take_while(|c| c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum();
            if all_commented {
                let content = &self.lines[r][ws_bytes..];
                if content.starts_with("; ") {
                    self.lines[r].replace_range(ws_bytes..ws_bytes + 2, "");
                } else if content.starts_with(';') {
                    self.lines[r].replace_range(ws_bytes..ws_bytes + 1, "");
                }
            } else {
                if !self.lines[r].trim_start().is_empty() {
                    self.lines[r].insert_str(ws_bytes, "; ");
                }
            }
        }
        // Adjust cursor column using the delta computed before the loop.
        if cursor_delta != 0 {
            self.cursor_col = (self.cursor_col as isize + cursor_delta).max(0) as usize;
        }
        self.clear_selection();
    }

    /// Paste multi-line text at the cursor, normalizing line endings and replacing tabs with spaces.
    pub fn paste_text(&mut self, text: &str) {
        self.snapshot();
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
            self.selection_anchor = None;
        }
        // Normalize: CRLF → LF, lone CR → LF, tabs → 4 spaces
        let normalized = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', "    ");
        let chunks: Vec<&str> = normalized.split('\n').collect();
        if chunks.is_empty() {
            return;
        }
        // Insert first chunk inline at cursor
        for ch in chunks[0].chars() {
            self.insert_char_internal(ch);
        }
        // Each subsequent chunk starts a new line
        for chunk in &chunks[1..] {
            self.ensure_line();
            let col = self.cursor_col.min(Self::char_count(self.current_line()));
            let byte_idx = Self::byte_at(self.current_line(), col);
            let rest = self.current_line()[byte_idx..].to_string();
            self.current_line_mut().truncate(byte_idx);
            let new_line = chunk.to_string() + &rest;
            self.cursor_row += 1;
            self.lines.insert(self.cursor_row, new_line);
            self.cursor_col = Self::char_count(chunk);
        }
    }

    /// Duplicate the current line, inserting the copy below, and move down.
    pub fn duplicate_line(&mut self) {
        self.snapshot();
        self.ensure_line();
        let line = self.lines[self.cursor_row].clone();
        self.cursor_row += 1;
        self.lines.insert(self.cursor_row, line);
    }

    pub fn move_left(&mut self) {
        self.last_op = LastOp::Other;
        if self.cursor_col > 0 {
            self.cursor_col -= 1
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = Self::char_count(&self.lines[self.cursor_row]);
        }
    }
    pub fn move_right(&mut self) {
        self.last_op = LastOp::Other;
        let len = Self::char_count(self.current_line());
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }
    pub fn move_up(&mut self) {
        self.last_op = LastOp::Other;
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = Self::char_count(self.current_line());
            self.cursor_col = self.cursor_col.min(len);
        }
    }
    pub fn move_down(&mut self) {
        self.last_op = LastOp::Other;
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = Self::char_count(self.current_line());
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    /// Move left by one word (Ctrl+←).
    pub fn move_word_left(&mut self) {
        self.last_op = LastOp::Other;
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = Self::char_count(&self.lines[self.cursor_row]);
            }
            return;
        }
        self.cursor_col = word_left_col(self.current_line(), self.cursor_col);
    }

    /// Move right by one word (Ctrl+→).
    pub fn move_word_right(&mut self) {
        self.last_op = LastOp::Other;
        let len = Self::char_count(self.current_line());
        if self.cursor_col >= len {
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            return;
        }
        self.cursor_col = word_right_col(self.current_line(), self.cursor_col);
    }

    pub fn move_home(&mut self) {
        self.last_op = LastOp::Other;
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        self.last_op = LastOp::Other;
        let len = Self::char_count(self.current_line());
        self.cursor_col = len;
    }

    pub fn page_up(&mut self) {
        self.last_op = LastOp::Other;
        let h = self.page_size.get().max(1);
        if self.cursor_row >= h {
            self.cursor_row -= h;
        } else {
            self.cursor_row = 0;
        }
        let len = Self::char_count(self.current_line());
        self.cursor_col = self.cursor_col.min(len);
    }

    pub fn page_down(&mut self) {
        self.last_op = LastOp::Other;
        let h = self.page_size.get().max(1);
        let max_row = self.lines.len().saturating_sub(1);
        self.cursor_row = (self.cursor_row + h).min(max_row);
        let len = Self::char_count(self.current_line());
        self.cursor_col = self.cursor_col.min(len);
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// Edge-margin stable scroll: keeps cursor 3 lines from viewport edges.
    /// Writes the result to `self.scroll_offset` so the mouse handler can read it.
    pub fn stable_scroll_start(&self, visible_h: usize) -> usize {
        if visible_h == 0 {
            self.scroll_offset.set(0);
            return 0;
        }
        let len = self.lines.len();
        let margin = 3usize;
        let mut start = self.scroll_offset.get();
        // Scroll down if cursor is below viewport
        if self.cursor_row >= start + visible_h.saturating_sub(margin) {
            start = self.cursor_row + margin + 1;
            if start + visible_h > len {
                start = len.saturating_sub(visible_h);
            }
        }
        // Scroll up if cursor is above viewport
        if self.cursor_row < start + margin {
            start = self.cursor_row.saturating_sub(margin);
        }
        start = start.min(len.saturating_sub(visible_h));
        self.scroll_offset.set(start);
        start
    }

    /// Select the word (alphanumeric+underscore run) around the cursor.
    pub fn select_word_at_cursor(&mut self) {
        let col = self.cursor_col;
        let line = self.current_line().to_string();
        let left = word_left_col_inclusive(&line, col);
        let right = word_right_col_inclusive(&line, col);
        if left < right {
            self.selection_anchor = Some((self.cursor_row, left));
            self.cursor_col = right;
        }
    }
}

/// Compute the column position of the start of the word to the left of `col`.
fn word_left_col(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let mut c = col;
    // Skip non-word chars, then skip word chars
    while c > 0 && !is_word_char(chars[c - 1]) {
        c -= 1;
    }
    while c > 0 && is_word_char(chars[c - 1]) {
        c -= 1;
    }
    c
}

/// Compute the column position of the end of the word to the right of `col`.
fn word_right_col(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut c = col;
    // Skip word chars, then skip non-word chars
    while c < len && is_word_char(chars[c]) {
        c += 1;
    }
    while c < len && !is_word_char(chars[c]) {
        c += 1;
    }
    c
}

#[inline]
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn word_left_col_inclusive(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let mut c = col.min(chars.len());
    // If cursor is past end or on non-word char, step back into word
    while c > 0 && !is_word_char(chars[c - 1]) {
        c -= 1;
    }
    while c > 0 && is_word_char(chars[c - 1]) {
        c -= 1;
    }
    c
}

fn word_right_col_inclusive(line: &str, col: usize) -> usize {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut c = col.min(len);
    // If on non-word char, step into word
    while c < len && !is_word_char(chars[c]) {
        c += 1;
    }
    while c < len && is_word_char(chars[c]) {
        c += 1;
    }
    c
}
