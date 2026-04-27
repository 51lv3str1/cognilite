use std::path::{Path, PathBuf};
use std::sync::{mpsc, OnceLock};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::app::App;
use crate::domain::message::{AttachmentKind, Role};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet>   = OnceLock::new();

const TEXT_EXTS: &[&str] = &[
    "txt", "md", "rs", "py", "js", "ts", "go", "c", "cpp", "h", "hpp",
    "java", "rb", "sh", "toml", "yaml", "yml", "json", "xml", "html",
    "css", "sql", "env", "dockerfile", "makefile", "lock", "log",
];

#[derive(Clone)]
pub enum FilePickerEntry {
    Parent,
    Dir(String),
    File(String), // relative path from working_dir
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilePickerMode {
    Pin,      // default: toggle pinned files
    LoadChat, // Ctrl+O: load a saved chat JSON
}

pub struct FilePicker {
    pub current_dir: PathBuf,
    pub entries: Vec<FilePickerEntry>,
    pub cursor: usize,
    pub query: String,
    pub preview: Vec<Line<'static>>,
    pub preview_scroll: usize,
    pub pending_path: Option<PathBuf>,
    pub highlight_rx: Option<mpsc::Receiver<(PathBuf, Vec<Line<'static>>)>>,
    pub loading: bool,
    pub mode: FilePickerMode,
}

pub struct FilePanel {
    pub path: PathBuf,
    pub display_path: String,
    pub lines: Vec<Line<'static>>,
    pub scroll: usize,
    pub h_scroll: usize,
    pub mtime: Option<std::time::SystemTime>,
    pub reloaded_at: Option<std::time::Instant>,
}

impl App {
    /// Initialize syntect's SyntaxSet and ThemeSet in a background thread so the
    /// first file preview doesn't block the UI.
    pub fn prewarm_highlight() {
        std::thread::spawn(|| {
            SYNTAX_SET.get_or_init(|| two_face::syntax::extra_newlines());
            THEME_SET.get_or_init(ThemeSet::load_defaults);
        });
    }

    pub fn open_file_picker(&mut self) {
        if self.ws_tx.is_some() {
            self.request_server_ls(".");
            return;
        }
        let dir = self.working_dir.clone();
        let entries = load_picker_entries(&dir, &dir);
        self.file_picker = Some(FilePicker {
            current_dir: dir, entries, cursor: 0, query: String::new(),
            preview: vec![], preview_scroll: 0,
            pending_path: None, highlight_rx: None, loading: false,
            mode: FilePickerMode::Pin,
        });
        self.update_preview();
    }

    pub fn open_file_picker_load(&mut self) {
        let dir = self.working_dir.clone();
        let entries = load_picker_entries(&dir, &dir);
        self.file_picker = Some(FilePicker {
            current_dir: dir, entries, cursor: 0, query: String::new(),
            preview: vec![], preview_scroll: 0,
            pending_path: None, highlight_rx: None, loading: false,
            mode: FilePickerMode::LoadChat,
        });
        self.update_preview();
    }

    /// In WS mode: send an `ls` request to the server and show the picker in loading state.
    pub fn request_server_ls(&mut self, rel_path: &str) {
        if let Some(ref mut tx) = self.ws_tx {
            crate::adapter::ws_client::send_json(tx, serde_json::json!({"type":"ls","path":rel_path}));
        }
        let dir = PathBuf::from(rel_path);
        if self.file_picker.is_none() {
            self.file_picker = Some(FilePicker {
                current_dir: dir, entries: vec![], cursor: 0, query: String::new(),
                preview: vec![], preview_scroll: 0,
                pending_path: None, highlight_rx: None, loading: true,
                mode: FilePickerMode::Pin,
            });
        } else if let Some(fp) = &mut self.file_picker {
            fp.current_dir = dir;
            fp.entries.clear();
            fp.cursor = 0;
            fp.query.clear();
            fp.loading = true;
        }
    }

    pub fn close_file_picker(&mut self) {
        self.file_picker = None;
    }

    pub fn open_file_panel(&mut self, path: PathBuf) {
        let display_path = path.strip_prefix(&self.working_dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string());
        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        let lines = if let Some((cached_mt, cached_lines)) = self.highlight_cache.get(&path) {
            if Some(*cached_mt) == mtime { cached_lines.clone() } else { highlight_file(&path) }
        } else {
            highlight_file(&path)
        };
        if let Some(mt) = mtime {
            self.highlight_cache.insert(path.clone(), (mt, lines.clone()));
        }
        self.file_panel = Some(FilePanel { path, display_path, lines, scroll: 0, h_scroll: 0, mtime, reloaded_at: None });
        self.file_panel_visible = true;
    }

    /// Open the file panel from content delivered over WS (no local disk read).
    pub fn open_file_panel_remote(&mut self, path_str: &str, content: &str) {
        let path = PathBuf::from(path_str);
        let display_path = path.to_string_lossy().to_string();
        let lines = highlight_content(content, &path);
        self.file_panel = Some(FilePanel {
            path, display_path, lines, scroll: 0, h_scroll: 0,
            mtime: None, reloaded_at: None,
        });
        self.file_panel_visible = true;
    }

    pub fn toggle_file_panel(&mut self) {
        if self.file_panel.is_none() { return; }
        self.file_panel_visible = !self.file_panel_visible;
        if !self.file_panel_visible && self.chat_focus == crate::app::ChatFocus::FilePanel {
            self.chat_focus = crate::app::ChatFocus::Input;
        }
    }

    pub fn close_file_panel(&mut self) {
        self.file_panel = None;
        self.file_panel_attachment = None;
        if self.chat_focus == crate::app::ChatFocus::FilePanel {
            self.chat_focus = crate::app::ChatFocus::Input;
        }
    }

    pub fn check_file_panel(&mut self) {
        let Some(fp) = &self.file_panel else { return };
        let path = fp.path.clone();
        let old_mtime = fp.mtime;
        let new_mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        if new_mtime.is_some() && new_mtime != old_mtime {
            let lines = highlight_file(&path);
            if let Some(mt) = new_mtime {
                self.highlight_cache.insert(path, (mt, lines.clone()));
            }
            if let Some(fp) = &mut self.file_panel {
                fp.lines = lines;
                fp.mtime = new_mtime;
                fp.reloaded_at = Some(std::time::Instant::now());
            }
        }
    }

    pub fn file_panel_scroll_up(&mut self) {
        if let Some(fp) = &mut self.file_panel {
            fp.scroll = fp.scroll.saturating_sub(10);
        }
    }

    pub fn file_panel_scroll_down(&mut self) {
        if let Some(fp) = &mut self.file_panel {
            let max = fp.lines.len().saturating_sub(1);
            fp.scroll = (fp.scroll + 10).min(max);
        }
    }

    pub fn file_panel_scroll_left(&mut self) {
        if let Some(fp) = &mut self.file_panel {
            fp.h_scroll = fp.h_scroll.saturating_sub(8);
        }
    }

    pub fn file_panel_scroll_right(&mut self) {
        if let Some(fp) = &mut self.file_panel {
            fp.h_scroll = fp.h_scroll.saturating_add(8);
        }
    }

    /// Open/cycle through text attachments of the currently selected history message.
    pub fn cycle_message_attachment(&mut self) {
        let msg_idx = self.history_cursor;
        let Some(msg) = self.messages.get(msg_idx) else { return };
        if msg.role != Role::User { return; }

        let text_att_indices: Vec<usize> = msg.attachments.iter().enumerate()
            .filter(|(_, a)| a.kind == AttachmentKind::Text)
            .map(|(i, _)| i)
            .collect();
        if text_att_indices.is_empty() { return; }

        let next_att_idx = if let Some((cur_msg, cur_att)) = self.file_panel_attachment {
            if cur_msg == msg_idx {
                let pos = text_att_indices.iter().position(|&i| i == cur_att).unwrap_or(0);
                text_att_indices[(pos + 1) % text_att_indices.len()]
            } else {
                text_att_indices[0]
            }
        } else {
            text_att_indices[0]
        };

        let path = msg.attachments[next_att_idx].path.clone();
        self.file_panel_attachment = Some((msg_idx, next_att_idx));
        self.open_file_panel(path);
    }

    /// Returns the visible entries after applying the query filter.
    pub fn file_picker_visible(&self) -> Vec<FilePickerEntry> {
        let Some(fp) = &self.file_picker else { return vec![] };
        if fp.query.is_empty() {
            fp.entries.clone()
        } else {
            let q = fp.query.to_lowercase();
            fp.entries.iter().filter(|e| match e {
                FilePickerEntry::Parent     => false,
                FilePickerEntry::Dir(n)    => n.to_lowercase().contains(&q),
                FilePickerEntry::File(f)   => f.to_lowercase().contains(&q),
            }).cloned().collect()
        }
    }

    pub fn file_picker_accept(&mut self) {
        let visible = self.file_picker_visible();
        let cursor  = self.file_picker.as_ref().map(|fp| fp.cursor).unwrap_or(0);
        let Some(entry) = visible.into_iter().nth(cursor) else { return };
        match entry {
            FilePickerEntry::Parent => { self.file_picker_go_up(); return; }
            FilePickerEntry::Dir(name) => {
                if self.ws_tx.is_some() {
                    let cur = self.file_picker.as_ref().unwrap().current_dir.clone();
                    let sub = cur.join(&name).to_string_lossy().to_string();
                    self.request_server_ls(&sub);
                    return;
                }
                let new_dir     = self.file_picker.as_ref().unwrap().current_dir.join(&name);
                let working_dir = self.working_dir.clone();
                if new_dir.is_dir() {
                    let entries = load_picker_entries(&new_dir, &working_dir);
                    if let Some(fp) = &mut self.file_picker {
                        fp.current_dir = new_dir;
                        fp.entries     = entries;
                        fp.cursor      = 0;
                        fp.query.clear();
                    }
                }
            }
            FilePickerEntry::File(display) => {
                let mode = self.file_picker.as_ref().map(|fp| fp.mode.clone()).unwrap_or(FilePickerMode::Pin);
                if mode == FilePickerMode::LoadChat {
                    let path = self.file_picker.as_ref().unwrap().current_dir.join(&display);
                    self.file_picker = None;
                    self.load_chat(path);
                    return;
                }
                if self.ws_tx.is_some() {
                    // insert @path into the message input and close picker
                    let at_path = format!("@{display}");
                    let byte = self.input.char_indices().nth(self.cursor_pos)
                        .map(|(b, _)| b).unwrap_or(self.input.len());
                    self.input.insert_str(byte, &at_path);
                    self.cursor_pos += at_path.chars().count();
                    self.file_picker = None;
                    return;
                }
                if self.pinned_files.iter().any(|pf| pf.display == display) {
                    let d = display.clone();
                    self.unpin_file(&d);
                } else {
                    self.pin_file(display);
                }
            }
        }
        self.update_preview();
    }

    pub fn file_picker_go_up(&mut self) {
        if self.file_picker.is_none() { return; }

        let query_non_empty = self.file_picker.as_ref().unwrap().query.len() > 0;
        if query_non_empty {
            if let Some(fp) = &mut self.file_picker { fp.query.clear(); fp.cursor = 0; }
            self.update_preview();
            return;
        }

        if self.ws_tx.is_some() {
            let current = self.file_picker.as_ref().unwrap().current_dir.clone();
            let parent = current.parent()
                .map(|p| if p == std::path::Path::new("") { "." } else { p.to_str().unwrap_or(".") })
                .unwrap_or(".");
            let parent_s = parent.to_string();
            self.request_server_ls(&parent_s);
            return;
        }

        let (current_dir, working_dir) = {
            let fp = self.file_picker.as_ref().unwrap();
            (fp.current_dir.clone(), self.working_dir.clone())
        };
        if let Some(parent) = current_dir.parent() {
            if parent.starts_with(&working_dir) {
                let entries = load_picker_entries(parent, &working_dir);
                if let Some(fp) = &mut self.file_picker {
                    fp.current_dir = parent.to_path_buf();
                    fp.entries     = entries;
                    fp.cursor      = 0;
                }
            }
        }
        self.update_preview();
    }

    pub fn file_picker_prev(&mut self) {
        if let Some(fp) = &mut self.file_picker {
            if fp.cursor > 0 { fp.cursor -= 1; }
        }
        self.update_preview();
    }

    pub fn file_picker_next(&mut self) {
        let len = self.file_picker_visible().len();
        if let Some(fp) = &mut self.file_picker {
            if fp.cursor + 1 < len { fp.cursor += 1; }
        }
        self.update_preview();
    }

    pub fn file_picker_scroll_preview_up(&mut self) {
        if let Some(fp) = &mut self.file_picker {
            fp.preview_scroll = fp.preview_scroll.saturating_sub(5);
        }
    }

    pub fn file_picker_scroll_preview_down(&mut self) {
        if let Some(fp) = &mut self.file_picker {
            let max = fp.preview.len().saturating_sub(1);
            fp.preview_scroll = (fp.preview_scroll + 5).min(max);
        }
    }

    fn file_picker_selected_path(&self) -> Option<PathBuf> {
        let fp = self.file_picker.as_ref()?;
        let visible = self.file_picker_visible();
        match visible.get(fp.cursor)? {
            FilePickerEntry::File(display) => Some(self.working_dir.join(display)),
            _ => None,
        }
    }

    pub fn update_preview(&mut self) {
        if self.ws_tx.is_some() { return; } // server-side files, no local preview
        let path = self.file_picker_selected_path();

        // No file selected — clear preview
        let Some(path) = path else {
            if let Some(fp) = &mut self.file_picker {
                fp.preview = vec![];
                fp.preview_scroll = 0;
                fp.pending_path = None;
            }
            return;
        };

        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());

        // Cache hit — instant, no thread needed
        if let Some((cached_mt, cached_lines)) = self.highlight_cache.get(&path) {
            if Some(*cached_mt) == mtime {
                let lines = cached_lines.clone();
                if let Some(fp) = &mut self.file_picker {
                    fp.preview = lines;
                    fp.preview_scroll = 0;
                    fp.pending_path = None;
                }
                return;
            }
        }

        // Cache miss — show spinner, highlight in background
        let fp = self.file_picker.as_mut().unwrap();
        fp.preview = vec![Line::from(Span::styled(
            "  ⟳ loading...".to_string(),
            Style::default().fg(Color::Rgb(88, 91, 112)),
        ))];
        fp.preview_scroll = 0;
        fp.pending_path = Some(path.clone());

        let (tx, rx) = mpsc::channel();
        fp.highlight_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send((path.clone(), highlight_file(&path)));
        });
    }

    /// Poll the background highlight channel; update preview and cache when done.
    pub fn poll_highlight(&mut self) {
        let result = {
            let Some(fp) = &self.file_picker else { return };
            let Some(rx) = &fp.highlight_rx else { return };
            rx.try_recv().ok()
        };
        if let Some((path, lines)) = result {
            let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
            if let Some(mt) = mtime {
                self.highlight_cache.insert(path.clone(), (mt, lines.clone()));
            }
            if let Some(fp) = &mut self.file_picker {
                if fp.pending_path.as_ref() == Some(&path) {
                    fp.preview = lines;
                    fp.highlight_rx = None;
                    fp.pending_path = None;
                }
            }
        }
    }
}

/// Returns the entries for a single directory level: optionally a Parent entry,
/// then subdirectories, then text files — all sorted alphabetically.
fn load_picker_entries(dir: &Path, working_dir: &Path) -> Vec<FilePickerEntry> {
    let mut dirs  = Vec::new();
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" || name == "node_modules" { continue; }
            if path.is_dir() {
                dirs.push(FilePickerEntry::Dir(name));
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                if TEXT_EXTS.contains(&ext.as_str()) || ext.is_empty() {
                    if let Ok(rel) = path.strip_prefix(working_dir) {
                        files.push(FilePickerEntry::File(rel.to_string_lossy().to_string()));
                    }
                }
            }
        }
    }
    let mut result = Vec::new();
    if dir != working_dir { result.push(FilePickerEntry::Parent); }
    result.extend(dirs);
    result.extend(files);
    result
}

/// Convert syntect highlighted ranges to ratatui Spans (owned, 'static).
fn syntax_to_spans(ranges: &[(syntect::highlighting::Style, &str)]) -> Vec<Span<'static>> {
    ranges.iter().filter_map(|(style, text)| {
        let t = text.trim_end_matches('\n').trim_end_matches('\r');
        if t.is_empty() { return None; }
        let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
        let mut s = Style::default().fg(fg);
        if style.font_style.contains(FontStyle::BOLD)   { s = s.add_modifier(Modifier::BOLD); }
        if style.font_style.contains(FontStyle::ITALIC) { s = s.add_modifier(Modifier::ITALIC); }
        Some(Span::styled(t.to_string(), s))
    }).collect()
}

/// Resolve a language tag (e.g. "rust", "python", "js") to a syntect SyntaxReference.
fn resolve_syntax<'a>(ss: &'a SyntaxSet, lang: &str) -> &'a syntect::parsing::SyntaxReference {
    let l = lang.trim().to_lowercase();
    if let Some(s) = ss.find_syntax_by_extension(&l) { return s; }
    let name = match l.as_str() {
        "rust"                          => "Rust",
        "python" | "py"                => "Python",
        "javascript"                   => "JavaScript",
        "typescript"                   => "TypeScript",
        "bash" | "sh" | "shell" | "zsh" => "Bash",
        "c"                            => "C",
        "cpp" | "c++"                  => "C++",
        "java"                         => "Java",
        "go" | "golang"                => "Go",
        "ruby" | "rb"                  => "Ruby",
        "html"                         => "HTML",
        "css"                          => "CSS",
        "json"                         => "JSON",
        "toml"                         => "TOML",
        "yaml" | "yml"                 => "YAML",
        "markdown" | "md"              => "Markdown",
        "sql"                          => "SQL",
        "xml"                          => "XML",
        "lua"                          => "Lua",
        "haskell" | "hs"               => "Haskell",
        "swift"                        => "Swift",
        "kotlin" | "kt"                => "Kotlin",
        "scala"                        => "Scala",
        "perl"                         => "Perl",
        _                              => "",
    };
    if !name.is_empty() {
        if let Some(s) = ss.find_syntax_by_name(name) { return s; }
    }
    ss.find_syntax_plain_text()
}

/// Highlight `code` using the given language tag. Returns one Line per source line.
/// No line numbers — callers add their own prefix (gutter, line number, etc.).
pub fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let ss = SYNTAX_SET.get_or_init(|| two_face::syntax::extra_newlines());
    let ts = THEME_SET.get_or_init(ThemeSet::load_defaults);
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = resolve_syntax(ss, lang);
    let mut h = HighlightLines::new(syntax, theme);
    LinesWithEndings::from(code).take(500).map(|line| {
        let ranges = h.highlight_line(line, ss).unwrap_or_default();
        Line::from(syntax_to_spans(&ranges))
    }).collect()
}

fn highlight_file(path: &Path) -> Vec<Line<'static>> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return vec![Line::from(Span::styled(
            "(binary or unreadable)".to_string(),
            Style::default().fg(Color::DarkGray),
        ))],
    };
    highlight_content(&content, path)
}

/// Highlight `content` using `path_hint` only for syntax detection (no disk read).
pub fn highlight_content(content: &str, path_hint: &Path) -> Vec<Line<'static>> {
    let ss = SYNTAX_SET.get_or_init(|| two_face::syntax::extra_newlines());
    let ts = THEME_SET.get_or_init(ThemeSet::load_defaults);
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = ss.find_syntax_for_file(path_hint).ok().flatten()
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);
    LinesWithEndings::from(content).enumerate().take(500).map(|(i, line)| {
        let ranges = h.highlight_line(line, ss).unwrap_or_default();
        let mut spans = vec![Span::styled(
            format!("{:4} ", i + 1),
            Style::default().fg(Color::Rgb(88, 91, 112)),
        )];
        spans.extend(syntax_to_spans(&ranges));
        Line::from(spans)
    }).collect()
}
