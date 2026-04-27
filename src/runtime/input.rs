use std::path::{Path, PathBuf};
use crate::app::App;

#[derive(Debug, Clone, PartialEq)]
pub enum CompletionKind {
    Path,
    Template,
}

#[derive(Debug)]
pub struct Completion {
    pub candidates: Vec<String>, // completion strings (names for templates, paths for files)
    pub cursor: usize,           // selected index
    pub token_start: usize,      // char position of the trigger char (@ or /) in input
    pub kind: CompletionKind,
}

impl App {
    pub fn input_kill_to_end(&mut self) {
        // Ctrl+K: delete from cursor to end of current line
        let byte = self.char_to_byte(self.cursor_pos);
        let after = &self.input[byte..];
        let to_nl = after.find('\n').unwrap_or(after.len());
        self.input.drain(byte..byte + to_nl);
    }

    pub fn input_kill_to_start(&mut self) {
        // Ctrl+U: delete from start of current line to cursor
        let byte = self.char_to_byte(self.cursor_pos);
        let before = &self.input[..byte];
        let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let removed_chars = self.input[line_start..byte].chars().count();
        self.input.drain(line_start..byte);
        self.cursor_pos -= removed_chars;
        self.completion = None;
    }

    pub fn input_delete_word_before(&mut self) {
        // Ctrl+W: delete the word (and whitespace) immediately before the cursor
        if self.cursor_pos == 0 {
            return;
        }
        let end_byte = self.char_to_byte(self.cursor_pos);
        // skip trailing spaces
        let mut i = end_byte;
        while i > 0 {
            let c = self.input[..i].chars().next_back().unwrap();
            if c == '\n' { break; }
            if c != ' ' { break; }
            i -= c.len_utf8();
        }
        // skip word chars
        while i > 0 {
            let c = self.input[..i].chars().next_back().unwrap();
            if c == ' ' || c == '\n' { break; }
            i -= c.len_utf8();
        }
        let removed_chars = self.input[i..end_byte].chars().count();
        self.input.drain(i..end_byte);
        self.cursor_pos -= removed_chars;
        self.completion = None;
    }

    pub fn input_move_word_left(&mut self) {
        // Alt+Left / Ctrl+Left: jump to start of previous word
        if self.cursor_pos == 0 {
            return;
        }
        let byte = self.char_to_byte(self.cursor_pos);
        let mut i = byte;
        // skip spaces
        while i > 0 {
            let c = self.input[..i].chars().next_back().unwrap();
            if c != ' ' && c != '\n' { break; }
            i -= c.len_utf8();
            self.cursor_pos -= 1;
        }
        // skip word
        while i > 0 {
            let c = self.input[..i].chars().next_back().unwrap();
            if c == ' ' || c == '\n' { break; }
            i -= c.len_utf8();
            self.cursor_pos -= 1;
        }
    }

    pub fn input_move_word_right(&mut self) {
        // Alt+Right / Ctrl+Right: jump to end of next word
        let total = self.input.chars().count();
        if self.cursor_pos >= total {
            return;
        }
        let mut byte = self.char_to_byte(self.cursor_pos);
        // skip spaces
        while self.cursor_pos < total {
            let c = self.input[byte..].chars().next().unwrap();
            if c != ' ' && c != '\n' { break; }
            byte += c.len_utf8();
            self.cursor_pos += 1;
        }
        // skip word
        while self.cursor_pos < total {
            let c = self.input[byte..].chars().next().unwrap();
            if c == ' ' || c == '\n' { break; }
            byte += c.len_utf8();
            self.cursor_pos += 1;
        }
    }

    pub fn input_insert(&mut self, c: char) {
        let idx = self.char_to_byte(self.cursor_pos);
        self.input.insert(idx, c);
        self.cursor_pos += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.cursor_pos == 0 {
            return;
        }
        let end = self.char_to_byte(self.cursor_pos);
        let start = self.char_to_byte(self.cursor_pos - 1);
        self.input.drain(start..end);
        self.cursor_pos -= 1;
    }

    pub fn input_delete(&mut self) {
        if self.cursor_pos >= self.input.chars().count() {
            return;
        }
        let start = self.char_to_byte(self.cursor_pos);
        let end = self.char_to_byte(self.cursor_pos + 1);
        self.input.drain(start..end);
    }

    pub fn input_move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn input_move_right(&mut self) {
        if self.cursor_pos < self.input.chars().count() {
            self.cursor_pos += 1;
        }
    }

    pub fn input_home(&mut self) {
        // move to start of current line
        let before = &self.input[..self.char_to_byte(self.cursor_pos)];
        if let Some(nl) = before.rfind('\n') {
            // count chars up to after the \n
            self.cursor_pos = self.input[..nl + 1].chars().count();
        } else {
            self.cursor_pos = 0;
        }
    }

    pub fn input_end(&mut self) {
        // move to end of current line
        let byte = self.char_to_byte(self.cursor_pos);
        let after = &self.input[byte..];
        let to_nl = after.find('\n').unwrap_or(after.len());
        self.cursor_pos += self.input[byte..byte + to_nl].chars().count();
    }

    pub fn input_newline(&mut self) {
        self.input_insert('\n');
    }

    pub fn input_move_up(&mut self) {
        let (row, col) = self.cursor_row_col();
        if row == 0 {
            return;
        }
        // find start of previous line
        let lines: Vec<&str> = self.input.split('\n').collect();
        let prev_line_len = lines[row - 1].chars().count();
        let target_col = col.min(prev_line_len);
        // recompute cursor_pos
        self.cursor_pos = lines[..row - 1].iter().map(|l| l.chars().count() + 1).sum::<usize>()
            + target_col;
    }

    pub fn input_move_down(&mut self) {
        let (row, col) = self.cursor_row_col();
        let lines: Vec<&str> = self.input.split('\n').collect();
        if row + 1 >= lines.len() {
            return;
        }
        let next_line_len = lines[row + 1].chars().count();
        let target_col = col.min(next_line_len);
        self.cursor_pos = lines[..row].iter().map(|l| l.chars().count() + 1).sum::<usize>()
            + lines[row].chars().count() + 1  // skip \n
            + target_col;
    }

    /// Returns (row, col) of cursor within the input string
    pub fn cursor_row_col(&self) -> (usize, usize) {
        let byte = self.char_to_byte(self.cursor_pos);
        let before = &self.input[..byte];
        let row = before.matches('\n').count();
        let col = before.rfind('\n').map(|i| before[i + 1..].chars().count()).unwrap_or(before.chars().count());
        (row, col)
    }

    /// Number of visual lines the input currently occupies
    pub fn input_line_count(&self) -> usize {
        self.input.matches('\n').count() + 1
    }

    pub fn input_history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        let new_pos = match self.history_pos {
            None => {
                // save current draft before entering history
                self.input_draft = self.input.clone();
                self.input_history.len() - 1
            }
            Some(0) => 0,
            Some(p) => p - 1,
        };
        self.history_pos = Some(new_pos);
        self.input = self.input_history[new_pos].clone();
        self.cursor_pos = self.input.chars().count();
    }

    pub fn input_history_next(&mut self) {
        let Some(pos) = self.history_pos else { return };
        if pos + 1 < self.input_history.len() {
            let new_pos = pos + 1;
            self.history_pos = Some(new_pos);
            self.input = self.input_history[new_pos].clone();
            self.cursor_pos = self.input.chars().count();
        } else {
            // past the end — restore draft
            self.history_pos = None;
            self.input = self.input_draft.clone();
            self.cursor_pos = self.input.chars().count();
        }
    }

    pub(crate) fn char_to_byte(&self, char_idx: usize) -> usize {
        self.input.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(self.input.len())
    }

    /// Recompute completion candidates based on the trigger token under the cursor.
    /// `@partial` → path completion; `/partial` at line start → template completion.
    pub fn update_completion(&mut self) {
        if let Some((at_pos, partial)) = self.at_token_before_cursor() {
            let candidates = get_path_completions(partial, &self.working_dir);
            if !candidates.is_empty() {
                let cursor = match &self.completion {
                    Some(c) if c.token_start == at_pos && c.kind == CompletionKind::Path => {
                        c.cursor.min(candidates.len().saturating_sub(1))
                    }
                    _ => 0,
                };
                self.completion = Some(Completion { candidates, cursor, token_start: at_pos, kind: CompletionKind::Path });
                return;
            }
        }
        if let Some((slash_pos, partial)) = self.slash_token_before_cursor() {
            let partial_lower = partial.to_lowercase();
            let candidates: Vec<String> = self.templates.iter()
                .filter(|(name, _)| name.to_lowercase().starts_with(&partial_lower))
                .map(|(name, _)| name.clone())
                .collect();
            if !candidates.is_empty() {
                let cursor = match &self.completion {
                    Some(c) if c.token_start == slash_pos && c.kind == CompletionKind::Template => {
                        c.cursor.min(candidates.len().saturating_sub(1))
                    }
                    _ => 0,
                };
                self.completion = Some(Completion { candidates, cursor, token_start: slash_pos, kind: CompletionKind::Template });
                return;
            }
        }
        self.completion = None;
    }

    /// Accept the currently selected completion candidate.
    pub fn complete_accept(&mut self) {
        if let Some(comp) = self.completion.take() {
            if let Some(selected) = comp.candidates.get(comp.cursor).cloned() {
                let token_byte = self.char_to_byte(comp.token_start);
                let cursor_byte = self.char_to_byte(self.cursor_pos);
                match comp.kind {
                    CompletionKind::Template => {
                        if let Some((_, body)) = self.templates.iter().find(|(n, _)| n == &selected) {
                            let body = body.clone();
                            self.input.replace_range(token_byte..cursor_byte, &body);
                            self.cursor_pos = comp.token_start + body.chars().count();
                        }
                    }
                    CompletionKind::Path => {
                        let new_token = format!("@{selected}");
                        self.input.replace_range(token_byte..cursor_byte, &new_token);
                        self.cursor_pos = comp.token_start + new_token.chars().count();
                        if selected.ends_with('/') {
                            self.update_completion();
                        }
                    }
                }
            }
        }
    }

    pub fn complete_next(&mut self) {
        if let Some(ref mut c) = self.completion {
            c.cursor = (c.cursor + 1) % c.candidates.len();
        }
    }

    pub fn complete_prev(&mut self) {
        if let Some(ref mut c) = self.completion {
            if c.cursor == 0 {
                c.cursor = c.candidates.len().saturating_sub(1);
            } else {
                c.cursor -= 1;
            }
        }
    }

    pub fn complete_dismiss(&mut self) {
        self.completion = None;
    }

    /// Returns the (char_pos_of_@, partial_path_after_@) if the cursor is
    /// inside an @token (no space between @ and cursor).
    fn at_token_before_cursor(&self) -> Option<(usize, &str)> {
        let byte_cursor = self.char_to_byte(self.cursor_pos);
        let before = &self.input[..byte_cursor];
        if let Some(at_byte) = before.rfind('@') {
            let after_at = &before[at_byte + 1..];
            if !after_at.contains(' ') && !after_at.contains('\n') {
                let at_char_pos = self.input[..at_byte].chars().count();
                return Some((at_char_pos, after_at));
            }
        }
        None
    }

    /// Returns the (char_pos_of_/, partial_after_/) if the cursor is inside a /token
    /// that begins at the start of the input or right after a newline.
    fn slash_token_before_cursor(&self) -> Option<(usize, &str)> {
        let byte_cursor = self.char_to_byte(self.cursor_pos);
        let before = &self.input[..byte_cursor];
        if let Some(slash_byte) = before.rfind('/') {
            let after_slash = &before[slash_byte + 1..];
            if !after_slash.contains(' ') && !after_slash.contains('\n') {
                let before_slash = &before[..slash_byte];
                if before_slash.is_empty() || before_slash.ends_with('\n') {
                    let slash_char_pos = self.input[..slash_byte].chars().count();
                    return Some((slash_char_pos, after_slash));
                }
            }
        }
        None
    }
}

/// Returns sorted completion candidates for a partial path typed after `@`.
/// Directories are listed first and get a trailing `/`.
fn get_path_completions(partial: &str, working_dir: &Path) -> Vec<String> {
    // expand leading ~
    let expanded: String = if partial.starts_with("~/") || partial == "~" {
        let home = std::env::var("HOME").unwrap_or_default();
        partial.replacen('~', &home, 1)
    } else {
        partial.to_string()
    };

    // split into directory prefix and name filter
    let (dir_str, name_prefix): (&str, &str) = if expanded.ends_with('/') {
        (expanded.as_str(), "")
    } else if let Some(slash) = expanded.rfind('/') {
        (&expanded[..=slash], &expanded[slash + 1..])
    } else {
        ("", expanded.as_str())
    };

    let search_dir: PathBuf = if dir_str.is_empty() {
        working_dir.to_path_buf()
    } else if dir_str.starts_with('/') {
        PathBuf::from(dir_str)
    } else {
        working_dir.join(dir_str)
    };

    let name_lower = name_prefix.to_lowercase();
    let mut results: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&search_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // skip hidden files unless the user explicitly typed a dot
            if name.starts_with('.') && !name_prefix.starts_with('.') {
                continue;
            }
            if name.to_lowercase().starts_with(&name_lower) {
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let candidate = if dir_str.is_empty() {
                    if is_dir { format!("{name}/") } else { name }
                } else {
                    if is_dir { format!("{dir_str}{name}/") } else { format!("{dir_str}{name}") }
                };
                results.push(candidate);
            }
        }
    }

    // directories first, then alphabetical
    results.sort_by(|a, b| {
        b.ends_with('/').cmp(&a.ends_with('/')).then(a.cmp(b))
    });
    results
}
