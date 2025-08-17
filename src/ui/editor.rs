// ---------- Simple text editor with lightweight syntax highlighting ----------
#[derive(Default)]
pub struct Editor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub selection_anchor: Option<(usize, usize)>,
}

impl Editor {
    pub fn with_sample() -> Self {
        let sample = vec![
            ".data".to_string(),
            "arr: .byte 1,2,3,4".to_string(),
            ".text".to_string(),
            "  la t0, arr".to_string(),
            "  lb t1, 0(t0)".to_string(),
            "  ecall".to_string(),
        ];
        Self {
            lines: sample,
            cursor_row: 0,
            cursor_col: 0,
            selection_anchor: None,
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

    pub fn insert_char(&mut self, ch: char) {
        self.ensure_line();
        let line = self.current_line();
        let col = self.cursor_col.min(Self::char_count(line));
        let byte_idx = Self::byte_at(line, col);
        self.current_line_mut().insert(byte_idx, ch);
        self.cursor_col = col + 1; // inserting advances the cursor
    }

    pub fn insert_spaces(&mut self, n: usize) {
        for _ in 0..n {
            self.insert_char(' ');
        }
    }

    pub fn backspace(&mut self) {
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
