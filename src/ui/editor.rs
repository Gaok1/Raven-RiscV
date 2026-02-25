// ---------- Simple text editor with lightweight syntax highlighting ----------
use std::collections::VecDeque;

#[derive(Clone)]
struct EditorState {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

#[derive(Default)]
pub struct Editor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub selection_anchor: Option<(usize, usize)>,
    history: VecDeque<EditorState>,
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
            "    li a0, 1".to_string(),     // fd=1 (stdout)
            "    la a1, msg".to_string(),   // buf
            "    li a2, len".to_string(),   // count
            "    li a7, 64".to_string(),    // write
            "    ecall".to_string(),
            "    li a0, 0".to_string(),     // status
            "    li a7, 93".to_string(),    // exit
            "    ecall".to_string(),
        ];
        Self {
            lines: sample,
            cursor_row: 0,
            cursor_col: 0,
            selection_anchor: None,
            history: VecDeque::new(),
        }
    }

    fn snapshot(&mut self) {
        if self.history.len() == 15 {
            self.history.pop_front();
        }
        self.history.push_back(EditorState {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
    }

    pub fn undo(&mut self) {
        if let Some(state) = self.history.pop_back() {
            self.lines = state.lines;
            self.cursor_row = state.cursor_row;
            self.cursor_col = state.cursor_col;
            self.clear_selection();
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
        self.snapshot();
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

    pub fn enter(&mut self) {
        self.snapshot();
        if let Some((start, end)) = self.selection_range() {
            self.delete_range(start, end);
        }
        self.ensure_line();
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
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, rest);
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = Self::char_count(&self.lines[self.cursor_row]);
        }
    }
    pub fn move_right(&mut self) {
        let len = Self::char_count(self.current_line());
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }
    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = Self::char_count(self.current_line());
            self.cursor_col = self.cursor_col.min(len);
        }
    }
    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = Self::char_count(self.current_line());
            self.cursor_col = self.cursor_col.min(len);
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        let len = Self::char_count(self.current_line());
        self.cursor_col = len;
    }

    pub fn page_up(&mut self) {
        let h = 20usize;
        if self.cursor_row >= h {
            self.cursor_row -= h;
        } else {
            self.cursor_row = 0;
        }
        let len = Self::char_count(self.current_line());
        self.cursor_col = self.cursor_col.min(len);
    }

    pub fn page_down(&mut self) {
        let h = 20usize;
        let max_row = self.lines.len().saturating_sub(1);
        self.cursor_row = (self.cursor_row + h).min(max_row);
        let len = Self::char_count(self.current_line());
        self.cursor_col = self.cursor_col.min(len);
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}
