use std::sync::mpsc;
use std::path::{Path, PathBuf};
use crate::ollama::{ChatMessage, ModelEntry, StreamChunk};

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    ModelSelect,
    Chat,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    User,
    Assistant,
    Tool, // tool results — sent to the model as "user" turns
}

impl Role {
    pub fn to_api_str(&self) -> &'static str {
        match self {
            Role::User | Role::Tool => "user",
            Role::Assistant         => "assistant",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    pub prompt_tokens: u64,
    pub response_tokens: u64,
    pub tokens_per_sec: f64,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttachmentKind {
    Text,
    Image,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,
    pub kind: AttachmentKind,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,       // display content (without file bodies)
    pub llm_content: String,   // content sent to model (includes file bodies)
    pub images: Vec<String>,   // base64 images
    pub attachments: Vec<Attachment>,
    pub thinking: String,
    pub stats: Option<TokenStats>,
}

#[derive(Debug, PartialEq)]
pub enum StreamState {
    Idle,
    Streaming,
    Error(String),
}

#[derive(Debug)]
pub struct Completion {
    pub candidates: Vec<String>, // completion strings (paths relative to working_dir or absolute)
    pub cursor: usize,           // selected index
    pub token_start: usize,      // char position of the @ in input
}

pub struct App {
    pub screen: Screen,
    pub base_url: String,
    pub working_dir: PathBuf,
    // model select
    pub models: Vec<ModelEntry>,
    pub model_cursor: usize,
    pub loading_models: bool,
    pub models_error: Option<String>,
    // chat
    pub selected_model: Option<String>,
    pub context_length: Option<u64>,
    pub used_tokens: u64,
    pub messages: Vec<Message>,
    pub input: String,
    pub cursor_pos: usize,
    pub scroll: u16,
    pub auto_scroll: bool,
    pub content_lines: u16,
    pub stream_state: StreamState,
    pub stream_rx: Option<mpsc::Receiver<StreamChunk>>,
    pub stream_started_at: Option<std::time::Instant>,
    pub completion: Option<Completion>,
    // behaviours
    pub behaviours: Vec<crate::behaviour::Behaviour>,
    pub tool_context: String,
    // misc
    pub should_quit: bool,
}

impl App {
    pub fn new(base_url: String) -> Self {
        Self {
            screen: Screen::ModelSelect,
            base_url,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            models: Vec::new(),
            model_cursor: 0,
            loading_models: true,
            models_error: None,
            selected_model: None,
            context_length: None,
            used_tokens: 0,
            messages: Vec::new(),
            input: String::new(),
            cursor_pos: 0,
            scroll: 0,
            auto_scroll: true,
            content_lines: 0,
            stream_state: StreamState::Idle,
            stream_rx: None,
            stream_started_at: None,
            completion: None,
            behaviours: {
                let mut b = crate::behaviour::built_ins();
                let local = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".cognilite/behaviours");
                b.extend(crate::behaviour::load_from_dir(&local));
                if let Ok(home) = std::env::var("HOME") {
                    let global = std::path::PathBuf::from(home)
                        .join(".config/cognilite/behaviours");
                    b.extend(crate::behaviour::load_from_dir(&global));
                }
                b
            },
            tool_context: String::new(), // built after behaviours are loaded, in select_model
            should_quit: false,
        }
    }

    pub fn select_model(&mut self) {
        if let Some(entry) = self.models.get(self.model_cursor) {
            let name = entry.name.clone();
            self.selected_model = Some(name.clone());
            self.context_length = crate::ollama::fetch_context_length(&self.base_url, &name);
            self.tool_context = crate::behaviour::build_tool_context(&self.behaviours);
            self.used_tokens = 0;
            self.messages.clear();
            self.input.clear();
            self.cursor_pos = 0;
            self.scroll = 0;
            self.auto_scroll = true;
            self.stream_state = StreamState::Idle;
            self.screen = Screen::Chat;
        }
    }

    pub fn send_message(&mut self) {
        if self.input.trim().is_empty() || self.stream_state == StreamState::Streaming {
            return;
        }
        let raw = self.input.trim().to_string();
        self.input.clear();
        self.cursor_pos = 0;
        self.completion = None;

        let (display, llm_content, attachments, images) =
            resolve_attachments(&raw, &self.working_dir, self.context_length, self.used_tokens);

        self.messages.push(Message {
            role: Role::User,
            content: display,
            llm_content,
            images: images.clone(),
            attachments,
            thinking: String::new(),
            stats: None,
        });
        self.auto_scroll = true;
        self.start_stream();
    }

    /// Pushes an empty assistant placeholder and starts the Ollama stream
    /// from the current message history.
    fn start_stream(&mut self) {
        self.messages.push(Message {
            role: Role::Assistant,
            content: String::new(),
            llm_content: String::new(),
            images: vec![],
            attachments: vec![],
            thinking: String::new(),
            stats: None,
        });

        let model = self.selected_model.clone().unwrap();
        let base_url = self.base_url.clone();

        // prepend tool context as a system message if we have tools
        let mut chat_messages: Vec<ChatMessage> = Vec::new();
        if !self.tool_context.is_empty() {
            chat_messages.push(ChatMessage {
                role: "system".to_string(),
                content: self.tool_context.clone(),
                thinking: None,
                images: None,
            });
        }
        chat_messages.extend(
            self.messages
                .iter()
                .filter(|m| !m.llm_content.is_empty() || m.role == Role::User)
                .map(|m| ChatMessage {
                    role: m.role.to_api_str().to_string(),
                    content: m.llm_content.clone(),
                    thinking: None,
                    images: if m.images.is_empty() { None } else { Some(m.images.clone()) },
                }),
        );

        let (tx, rx) = mpsc::channel();
        self.stream_rx = Some(rx);
        self.stream_state = StreamState::Streaming;
        self.stream_started_at = Some(std::time::Instant::now());

        std::thread::spawn(move || {
            crate::ollama::stream_chat(&base_url, model, chat_messages, tx);
        });
    }

    pub fn poll_stream(&mut self) {
        let rx = match self.stream_rx.take() {
            Some(r) => r,
            None => return,
        };

        loop {
            match rx.try_recv() {
                Ok(chunk) => {
                    if let Some(e) = chunk.error {
                        // remove empty assistant message and set error
                        if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant && last.content.is_empty() && last.thinking.is_empty() {
                                self.messages.pop();
                            }
                        }
                        self.stream_state = StreamState::Error(e);
                        self.stream_started_at = None;
                        return;
                    }
                    if let Some(msg) = chunk.message {
                        if let Some(last) = self.messages.last_mut() {
                            if last.role == Role::Assistant {
                                last.content.push_str(&msg.content);
                                last.llm_content.push_str(&msg.content);
                                if let Some(t) = msg.thinking {
                                    last.thinking.push_str(&t);
                                }
                            }
                        }
                        // detect a complete tool call in the accumulated content
                        if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant {
                                if let Some(call) = extract_tool_call(&last.content) {
                                    let call = call.to_string();
                                    // strip <tool>...</tool> from display
                                    if let Some(last) = self.messages.last_mut() {
                                        if let Some(pos) = last.content.find("<tool>") {
                                            last.content.truncate(pos);
                                            last.content = last.content.trim_end().to_string();
                                            last.llm_content = last.content.clone();
                                        }
                                    }
                                    // stop current stream
                                    self.stream_state = StreamState::Idle;
                                    self.stream_started_at = None;
                                    // execute tool and restart stream
                                    self.handle_tool_call(&call);
                                    return;
                                }
                            }
                        }
                    }
                    if chunk.done {
                        // attach token stats
                        if let (Some(pt), Some(et), Some(ed)) = (
                            chunk.prompt_eval_count,
                            chunk.eval_count,
                            chunk.eval_duration,
                        ) {
                            let tps = if ed > 0 {
                                et as f64 / (ed as f64 / 1_000_000_000.0)
                            } else {
                                0.0
                            };
                            self.used_tokens = pt;
                            if let Some(last) = self.messages.last_mut() {
                                last.stats = Some(TokenStats {
                                    prompt_tokens: pt,
                                    response_tokens: et,
                                    tokens_per_sec: tps,
                                    duration_secs: ed as f64 / 1_000_000_000.0,
                                });
                            }
                        }
                        self.stream_state = StreamState::Idle;
                        self.stream_started_at = None;
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.stream_rx = Some(rx);
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.stream_state = StreamState::Idle;
                    return;
                }
            }
        }
    }

    pub fn stop_stream(&mut self) {
        self.stream_rx = None; // dropping the receiver makes the sender fail → thread exits
        self.stream_state = StreamState::Idle;
        self.stream_started_at = None;
        // remove placeholder if the model hadn't written anything yet
        if let Some(last) = self.messages.last() {
            if last.role == Role::Assistant && last.content.is_empty() && last.thinking.is_empty() {
                self.messages.pop();
            }
        }
    }

    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.scroll = 0;
        self.auto_scroll = true;
        self.stream_state = StreamState::Idle;
        self.stream_rx = None;
        self.completion = None;
    }

    // --- behaviour execution ---

    fn handle_tool_call(&mut self, call: &str) {
        let call = call.trim();
        let (cmd, args) = call.split_once(' ').unwrap_or((call, ""));
        let args = args.trim().trim_start_matches('@');

        let result = match self.behaviours.iter().find(|b| b.trigger == cmd) {
            Some(b) => match &b.kind {
                crate::behaviour::BehaviourKind::Tool { action, .. } => match action.as_str() {
                    "cat" => tool_cat(args, &self.working_dir),
                    _     => format!("unknown tool action: {action}"),
                },
            },
            None => format!("unknown tool: {cmd}"),
        };

        let filename = Path::new(args)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| args.to_string());
        let size = result.len();
        let llm_content = format!("Tool result:\n{result}");

        self.messages.push(Message {
            role: Role::Tool,
            content: result,
            llm_content,
            images: vec![],
            attachments: vec![Attachment {
                filename,
                kind: AttachmentKind::Text,
                size,
            }],
            thinking: String::new(),
            stats: None,
        });
        self.auto_scroll = true;
        self.start_stream();
    }


    // --- input editing helpers ---

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

    // returns (row, col) of cursor within the input string
    pub fn cursor_row_col(&self) -> (usize, usize) {
        let byte = self.char_to_byte(self.cursor_pos);
        let before = &self.input[..byte];
        let row = before.matches('\n').count();
        let col = before.rfind('\n').map(|i| before[i + 1..].chars().count()).unwrap_or(before.chars().count());
        (row, col)
    }

    // number of visual lines the input currently occupies
    pub fn input_line_count(&self) -> usize {
        self.input.matches('\n').count() + 1
    }

    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.input.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(self.input.len())
    }

    // --- path completion ---

    /// Recompute completion candidates based on the @token under the cursor.
    pub fn update_completion(&mut self) {
        if let Some((at_pos, partial)) = self.at_token_before_cursor() {
            let candidates = get_path_completions(partial, &self.working_dir);
            if !candidates.is_empty() {
                let cursor = match &self.completion {
                    Some(c) if c.token_start == at_pos => {
                        c.cursor.min(candidates.len().saturating_sub(1))
                    }
                    _ => 0,
                };
                self.completion = Some(Completion { candidates, cursor, token_start: at_pos });
                return;
            }
        }
        self.completion = None;
    }

    /// Accept the currently selected completion candidate.
    pub fn complete_accept(&mut self) {
        if let Some(comp) = self.completion.take() {
            if let Some(selected) = comp.candidates.get(comp.cursor) {
                let at_byte = self.char_to_byte(comp.token_start);
                let cursor_byte = self.char_to_byte(self.cursor_pos);
                let new_token = format!("@{selected}");
                self.input.replace_range(at_byte..cursor_byte, &new_token);
                self.cursor_pos = comp.token_start + new_token.chars().count();
                // keep popup open when a directory was selected
                if selected.ends_with('/') {
                    self.update_completion();
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
}

// ── tool execution ────────────────────────────────────────────────────────────

/// Returns the content between `<tool>` and `</tool>` if both tags are present.
fn extract_tool_call(content: &str) -> Option<&str> {
    let start = content.find("<tool>")? + 6;
    let end   = content.find("</tool>")?;
    if end >= start { Some(&content[start..end]) } else { None }
}

fn tool_cat(args: &str, working_dir: &Path) -> String {
    if args.is_empty() {
        return "usage: cat <path>".to_string();
    }
    let path = resolve_path(args, working_dir);
    match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e)      => format!("error: {e}"),
    }
}

// ── path completion ───────────────────────────────────────────────────────────

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

// ── file attachment resolution ────────────────────────────────────────────────

static IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp"];
static TEXT_EXTS: &[&str] = &[
    "txt", "md", "rs", "py", "js", "ts", "go", "c", "cpp", "h", "hpp",
    "java", "rb", "sh", "toml", "yaml", "yml", "json", "xml", "html",
    "css", "sql", "env", "dockerfile", "makefile", "lock", "log",
];

fn resolve_path(raw: &str, working_dir: &Path) -> PathBuf {
    if raw.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(raw.replacen('~', &home, 1))
    } else if raw.starts_with('/') {
        PathBuf::from(raw)
    } else {
        working_dir.join(raw)
    }
}

fn file_kind(path: &Path) -> Option<AttachmentKind> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        return Some(AttachmentKind::Image);
    }
    // text: known extensions OR no extension (try as text)
    if TEXT_EXTS.contains(&ext.as_str()) {
        return Some(AttachmentKind::Text);
    }
    // unknown extension — try as text
    Some(AttachmentKind::Text)
}

/// Parses @references from the input, reads files, and returns:
/// (display_text, llm_content, attachments_metadata, base64_images)
pub fn resolve_attachments(
    input: &str,
    working_dir: &Path,
    context_length: Option<u64>,
    used_tokens: u64,
) -> (String, String, Vec<Attachment>, Vec<String>) {
    let display = input.to_string();
    let mut llm_parts: Vec<String> = Vec::new();
    let mut attachments: Vec<Attachment> = Vec::new();
    let mut images: Vec<String> = Vec::new();

    // collect @path tokens, deduplicating by resolved path
    let mut seen_paths = std::collections::HashSet::new();
    let refs: Vec<String> = input
        .split_whitespace()
        .filter(|w| w.starts_with('@') && w.len() > 1)
        .map(|w| w[1..].to_string())
        .filter(|r| seen_paths.insert(resolve_path(r, working_dir)))
        .collect();

    // base text without @refs goes first in llm_parts
    let mut base_text = input.to_string();
    for r in &refs {
        base_text = base_text.replace(&format!("@{r}"), "").trim().to_string();
    }
    if !base_text.is_empty() {
        llm_parts.push(base_text);
    }

    for raw_path in &refs {
        let path = resolve_path(raw_path, working_dir);
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| raw_path.clone());

        match std::fs::metadata(&path) {
            Err(_) => {
                // file not found — leave @ref in display, add error note to llm
                llm_parts.push(format!("[File not found: {raw_path}]"));
            }
            Ok(meta) => {
                let size = meta.len() as usize;
                // check if the file fits in the remaining context window
                let estimated_tokens = meta.len() / 4; // bytes/4 ≈ tokens
                let remaining_tokens = context_length
                    .map(|ctx| ctx.saturating_sub(used_tokens));
                if let Some(remaining) = remaining_tokens {
                    if estimated_tokens > remaining {
                        let ctx_k = context_length.unwrap_or(0) / 1000;
                        llm_parts.push(format!(
                            "[{filename} is too large for the current context \
                             (~{estimated_tokens} tokens needed, {remaining} remaining of {ctx_k}k)]"
                        ));
                        continue;
                    }
                }

                match file_kind(&path) {
                    Some(AttachmentKind::Image) => {
                        if let Ok(data) = std::fs::read(&path) {
                            let b64 = base64_encode(&data);
                            images.push(b64);
                            attachments.push(Attachment {
                                filename: filename.clone(),
                                kind: AttachmentKind::Image,
                                size,
                            });
                        }
                    }
                    _ => {
                        if let Ok(text) = std::fs::read_to_string(&path) {
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            llm_parts.push(format!(
                                "<file_content path=\"{raw_path}\">\n```{ext}\n{}\n```\n</file_content>",
                                text.trim_end()
                            ));
                            attachments.push(Attachment {
                                filename: filename.clone(),
                                kind: AttachmentKind::Text,
                                size,
                            });
                        }
                    }
                }
            }
        }
    }

    let llm_content = llm_parts.join("\n\n");
    (display, llm_content, attachments, images)
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[(n >> 18) & 63] as char);
        out.push(CHARS[(n >> 12) & 63] as char);
        out.push(if chunk.len() > 1 { CHARS[(n >> 6) & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { CHARS[n & 63] as char } else { '=' });
    }
    out
}
