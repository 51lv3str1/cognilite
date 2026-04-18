use std::sync::{mpsc, OnceLock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use crate::ollama::{ChatMessage, ModelEntry, StreamChunk};

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet>   = OnceLock::new();

/// Generation parameter definitions: (name, description, default, min, max, step)
pub const GEN_PARAMS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("temperature",    "randomness of output",    0.8, 0.0, 2.0, 0.05),
    ("top_p",          "nucleus sampling cutoff", 0.9, 0.0, 1.0, 0.05),
    ("repeat_penalty", "repetition penalty",      1.1, 0.5, 2.0, 0.05),
];

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Config,
    ModelSelect,
    Chat,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CtxStrategy {
    Dynamic, // max(8192, used_tokens * 2) — faster, smaller KV cache
    Full,    // model's max context length — slower but never truncates history
}

impl CtxStrategy {
    pub fn index(&self) -> usize {
        match self { CtxStrategy::Dynamic => 0, CtxStrategy::Full => 1 }
    }
    fn from_index(i: usize) -> Self {
        match i { 1 => CtxStrategy::Full, _ => CtxStrategy::Dynamic }
    }
    fn as_str(&self) -> &'static str {
        match self { CtxStrategy::Dynamic => "dynamic", CtxStrategy::Full => "full" }
    }
    fn from_str(s: &str) -> Self {
        match s { "full" => CtxStrategy::Full, _ => CtxStrategy::Dynamic }
    }
}

fn config_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config/cognilite/config.json"))
}

#[derive(Debug, Clone, PartialEq)]
pub enum NeuronMode { Manual, Smart, Presets }

impl NeuronMode {
    pub fn as_str(&self) -> &'static str {
        match self { NeuronMode::Manual => "manual", NeuronMode::Smart => "smart", NeuronMode::Presets => "presets" }
    }
    pub fn from_str(s: &str) -> Self {
        match s { "smart" => NeuronMode::Smart, "presets" => NeuronMode::Presets, _ => NeuronMode::Manual }
    }
}

#[derive(Debug, Clone)]
pub struct NeuronPreset {
    pub name: String,
    pub enabled: Vec<String>,
}

pub struct Config {
    pub ctx_strategy: CtxStrategy,
    pub disabled_neurons: std::collections::HashSet<String>,
    pub gen_params: [f64; 3],
    pub ctx_pow2: bool,
    pub keep_alive: bool,
    pub warmup: bool,
    pub neuron_mode: NeuronMode,
    pub neuron_presets: Vec<NeuronPreset>,
    pub active_preset: Option<String>,
}

pub fn load_config() -> Config {
    let default = Config {
        ctx_strategy: CtxStrategy::Dynamic, disabled_neurons: Default::default(),
        gen_params: [GEN_PARAMS[0].2, GEN_PARAMS[1].2, GEN_PARAMS[2].2],
        ctx_pow2: true, keep_alive: false, warmup: true,
        neuron_mode: NeuronMode::Manual, neuron_presets: Vec::new(), active_preset: None,
    };
    let path = match config_path() { Some(p) => p, None => return default };
    let Ok(text) = std::fs::read_to_string(&path) else { return default };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else { return default };
    let ctx_strategy = val.get("ctx_strategy")
        .and_then(|v| v.as_str()).map(CtxStrategy::from_str).unwrap_or(CtxStrategy::Dynamic);
    let disabled_neurons = val.get("disabled_neurons")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let gen_params = [
        val.get("temperature").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[0].2),
        val.get("top_p").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[1].2),
        val.get("repeat_penalty").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[2].2),
    ];
    let ctx_pow2   = val.get("ctx_pow2").and_then(|v| v.as_bool()).unwrap_or(true);
    let keep_alive = val.get("keep_alive").and_then(|v| v.as_bool()).unwrap_or(false);
    let warmup     = val.get("warmup").and_then(|v| v.as_bool()).unwrap_or(true);
    let neuron_mode = val.get("neuron_mode").and_then(|v| v.as_str())
        .map(NeuronMode::from_str).unwrap_or(NeuronMode::Manual);
    let neuron_presets = val.get("neuron_presets").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|p| {
            let name    = p.get("name")?.as_str()?.to_string();
            let enabled = p.get("enabled")?.as_array()?
                .iter().filter_map(|v| v.as_str().map(String::from)).collect();
            Some(NeuronPreset { name, enabled })
        }).collect())
        .unwrap_or_default();
    let active_preset = val.get("active_preset").and_then(|v| v.as_str()).map(String::from);
    Config { ctx_strategy, disabled_neurons, gen_params, ctx_pow2, keep_alive, warmup, neuron_mode, neuron_presets, active_preset }
}

/// Returns true if the neuron has active tool capabilities (shell passthrough or synapse tools).
pub fn neuron_is_tooling(n: &crate::synapse::Neuron) -> bool {
    n.shell || !n.synapses.is_empty()
}

pub fn fuzzy_match(query: &str, target: &str) -> bool {
    if query.is_empty() { return true; }
    target.to_lowercase().contains(&query.to_lowercase())
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

#[derive(Debug, Clone, PartialEq)]
pub enum CompletionKind {
    Path,
    Template,
}

pub struct PinnedFile {
    pub path: PathBuf,
    pub display: String,          // relative path shown in UI
    pub content: String,          // snapshot at last warmup / last send
    pub mtime: Option<std::time::SystemTime>,
    pub changed: bool,            // mtime differs from snapshot
}

#[derive(Clone)]
pub enum FilePickerEntry {
    Parent,
    Dir(String),
    File(String), // relative path from working_dir
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
}

#[derive(Debug, Clone, Default)]
pub struct TokenStats {
    pub response_tokens: u64,
    pub tokens_per_sec: f64,
    pub thinking_secs: Option<f64>, // time until first content token (thinking phase only)
    pub wall_secs: f64,             // total wall-clock time from send to done
    pub prompt_eval_count: u64,     // tokens Ollama actually re-evaluated (0 = cache hit)
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttachmentKind {
    Text,
    Image,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub filename: String,   // basename, for display
    pub path: PathBuf,      // resolved absolute path, for reopening
    pub kind: AttachmentKind,
    pub size: usize,
}

pub struct FilePanel {
    pub path: PathBuf,
    pub display_path: String,
    pub lines: Vec<Line<'static>>,
    pub scroll: usize,
    pub mtime: Option<std::time::SystemTime>,
    pub reloaded_at: Option<std::time::Instant>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,       // display content (without file bodies)
    pub llm_content: String,   // content sent to model (includes file bodies)
    pub images: Vec<String>,   // base64 images
    pub attachments: Vec<Attachment>,
    pub thinking: String,
    pub thinking_secs: Option<f64>, // set on intermediate messages interrupted by a tool call
    pub stats: Option<TokenStats>,
    pub tool_call: Option<String>, // "Neuron › trigger" label, set for Role::Tool messages
}

#[derive(Debug, PartialEq)]
pub enum StreamState {
    Idle,
    Streaming,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatFocus {
    Input,
    History,
    FilePanel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AskKind {
    Text,            // free text — user types, Enter submits
    Confirm,         // yes/no — y/Enter = Yes, n/Esc = No
    Choice(Vec<String>), // pick one — ↑↓ navigate, Enter selects
}

#[derive(Debug, Clone)]
pub struct InputRequest {
    pub question: String, // shown in UI (empty for Choice — question is in model's preceding text)
    pub kind: AskKind,
}

#[derive(Debug)]
pub struct Completion {
    pub candidates: Vec<String>, // completion strings (names for templates, paths for files)
    pub cursor: usize,           // selected index
    pub token_start: usize,      // char position of the trigger char (@ or /) in input
    pub kind: CompletionKind,
}

pub struct App {
    pub screen: Screen,
    pub base_url: String,
    pub working_dir: PathBuf,
    // config
    pub ctx_strategy: CtxStrategy,
    pub config_cursor: usize,   // cursor in ctx strategy section
    pub config_section: usize,  // 0 = ctx strategy, 1 = neurons, 2 = generation, 3 = performance
    pub neuron_cursor: usize,
    pub neuron_mode: NeuronMode,
    pub neuron_sub_section: usize,        // 0=Manual 1=Smart 2=Presets within neurons tab
    pub neuron_presets: Vec<NeuronPreset>,
    pub active_preset: Option<String>,
    pub preset_cursor: usize,
    pub preset_name_input: Option<String>, // Some(s) while creating a new preset
    pub disabled_neurons: std::collections::HashSet<String>,
    pub ctx_pow2: bool,
    pub keep_alive: bool,
    pub warmup: bool,
    pub perf_cursor: usize,
    pub config_search: String,  // filter query for all config sections
    // model select
    pub models: Vec<ModelEntry>,
    pub model_cursor: usize,
    pub model_search: String,
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
    pub warmup_rx: Option<mpsc::Receiver<()>>,
    pub warmup_started_at: Option<std::time::Instant>,
    pub warmup_prompt_tokens: Option<u64>,
    pub warmup_last_hash: Option<u64>,     // hash of the system prompt used in the last warmup
    pub stream_started_at: Option<std::time::Instant>,
    pub thinking_end_secs: Option<f64>, // captured when first content token arrives
    pub completion: Option<Completion>,
    // neurons (groups of synapses)
    pub neurons: Vec<crate::synapse::Neuron>,
    // prompt templates (name, body) loaded from .cognilite/templates/ and global dir
    pub templates: Vec<(String, String)>,
    // input history
    pub input_history: Vec<String>,
    pub history_pos: Option<usize>,
    pub input_draft: String,
    // generation params
    pub gen_params: [f64; 3],
    pub param_cursor: usize,
    // misc
    pub should_quit: bool,
    pub show_help: bool,
    pub help_scroll: u16,
    pub copy_notice: Option<std::time::Instant>,
    // chat focus / history navigation
    pub chat_focus: ChatFocus,
    pub history_cursor: usize, // index into messages[] for selected block
    // ask / user input requests
    pub ask: Option<InputRequest>,
    pub ask_cursor: usize, // selected index for Choice
    // mood
    pub current_mood: Option<String>,
    // pending patch waiting for user confirmation
    pub pending_patch: Option<String>,
    // pinned files (always in system prompt)
    pub pinned_files: Vec<PinnedFile>,
    pub file_picker: Option<FilePicker>,
    // file panel (right-side code viewer)
    pub file_panel: Option<FilePanel>,
    pub file_panel_visible: bool,
    pub file_panel_attachment: Option<(usize, usize)>, // (msg_idx, att_idx)
    // highlight cache: path → (mtime, highlighted lines)
    pub highlight_cache: HashMap<PathBuf, (std::time::SystemTime, Vec<Line<'static>>)>,
}

impl App {
    pub fn new(base_url: String) -> Self {
        let cfg = load_config();
        let config_cursor = cfg.ctx_strategy.index();
        Self {
            screen: Screen::ModelSelect,
            base_url,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            ctx_strategy: cfg.ctx_strategy,
            config_cursor,
            config_section: 0,
            neuron_cursor: 0,
            neuron_mode: cfg.neuron_mode,
            neuron_sub_section: 0,
            neuron_presets: cfg.neuron_presets,
            active_preset: cfg.active_preset,
            preset_cursor: 0,
            preset_name_input: None,
            disabled_neurons: cfg.disabled_neurons,
            ctx_pow2: cfg.ctx_pow2,
            keep_alive: cfg.keep_alive,
            warmup: cfg.warmup,
            perf_cursor: 0,
            config_search: String::new(),
            models: Vec::new(),
            model_cursor: 0,
            model_search: String::new(),
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
            warmup_rx: None,
            warmup_started_at: None,
            warmup_prompt_tokens: None,
            warmup_last_hash: None,
            stream_started_at: None,
            thinking_end_secs: None,
            completion: None,
            neurons: {
                let mut n = Vec::new();
                let local = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".cognilite/neurons");
                n.extend(crate::synapse::load_from_dir(&local));
                if let Ok(home) = std::env::var("HOME") {
                    let global = std::path::PathBuf::from(home)
                        .join(".config/cognilite/neurons");
                    n.extend(crate::synapse::load_from_dir(&global));
                }
                n
            },
            templates: {
                let mut t = Vec::new();
                let local = std::env::current_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join(".cognilite/templates");
                t.extend(load_templates(&local));
                if let Ok(home) = std::env::var("HOME") {
                    let global = std::path::PathBuf::from(home)
                        .join(".config/cognilite/templates");
                    t.extend(load_templates(&global));
                }
                t
            },
            input_history: Vec::new(),
            history_pos: None,
            input_draft: String::new(),
            gen_params: cfg.gen_params,
            param_cursor: 0,
            should_quit: false,
            show_help: false,
            help_scroll: 0,
            copy_notice: None,
            chat_focus: ChatFocus::Input,
            history_cursor: 0,
            ask: None,
            ask_cursor: 0,
            current_mood: None,
            pending_patch: None,
            pinned_files: Vec::new(),
            file_picker: None,
            file_panel: None,
            file_panel_visible: true,
            file_panel_attachment: None,
            highlight_cache: HashMap::new(),
        }
    }

    /// Initialize syntect's SyntaxSet and ThemeSet in a background thread so the
    /// first file preview doesn't block the UI.
    pub fn prewarm_highlight() {
        std::thread::spawn(|| {
            SYNTAX_SET.get_or_init(|| two_face::syntax::extra_newlines());
            THEME_SET.get_or_init(ThemeSet::load_defaults);
        });
    }

    fn save_config(&self) {
        let Some(path) = config_path() else { return };
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        let disabled: Vec<&str> = self.disabled_neurons.iter().map(String::as_str).collect();
        let presets: Vec<serde_json::Value> = self.neuron_presets.iter().map(|p| {
            serde_json::json!({ "name": p.name, "enabled": p.enabled })
        }).collect();
        let json = serde_json::json!({
            "ctx_strategy":    self.ctx_strategy.as_str(),
            "disabled_neurons": disabled,
            "temperature":     self.gen_params[0],
            "top_p":           self.gen_params[1],
            "repeat_penalty":  self.gen_params[2],
            "ctx_pow2":        self.ctx_pow2,
            "keep_alive":      self.keep_alive,
            "warmup":          self.warmup,
            "neuron_mode":     self.neuron_mode.as_str(),
            "neuron_presets":  presets,
            "active_preset":   self.active_preset,
        });
        let _ = std::fs::write(&path, json.to_string());
    }

    pub fn confirm_config(&mut self) {
        self.ctx_strategy = CtxStrategy::from_index(self.config_cursor);
        self.save_config();
    }

    pub fn toggle_perf(&mut self, index: usize) {
        match index {
            0 => self.ctx_pow2   = !self.ctx_pow2,
            1 => self.keep_alive = !self.keep_alive,
            2 => self.warmup     = !self.warmup,
            _ => {}
        }
        self.save_config();
    }

    pub fn toggle_neuron(&mut self) {
        if let Some(neuron) = self.neurons.get(self.neuron_cursor) {
            let name = neuron.name.clone();
            if self.disabled_neurons.contains(&name) {
                self.disabled_neurons.remove(&name);
            } else {
                self.disabled_neurons.insert(name);
            }
            self.save_config();
        }
    }

    pub fn param_adjust(&mut self, direction: f64) {
        let (_, _, _, min, max, step) = GEN_PARAMS[self.param_cursor];
        let v = &mut self.gen_params[self.param_cursor];
        *v = (*v + direction * step).clamp(min, max);
        *v = (*v * 100.0).round() / 100.0;
        self.save_config();
    }

    pub fn param_reset(&mut self) {
        self.gen_params[self.param_cursor] = GEN_PARAMS[self.param_cursor].2;
        self.save_config();
    }

    pub fn set_neuron_mode(&mut self, mode: NeuronMode) {
        self.neuron_mode = mode;
        self.save_config();
        self.warmup_last_hash = None;
        self.trigger_warmup();
    }

    pub fn apply_preset(&mut self, name: &str) {
        if self.active_preset.as_deref() == Some(name) {
            self.active_preset = None;
        } else {
            self.active_preset = Some(name.to_string());
        }
        self.save_config();
        self.warmup_last_hash = None;
        self.trigger_warmup();
    }

    pub fn save_current_as_preset(&mut self, name: String) {
        let enabled: Vec<String> = self.neurons.iter()
            .filter(|n| !self.disabled_neurons.contains(&n.name))
            .map(|n| n.name.clone())
            .collect();
        if let Some(p) = self.neuron_presets.iter_mut().find(|p| p.name == name) {
            p.enabled = enabled;
        } else {
            self.neuron_presets.push(NeuronPreset { name, enabled });
        }
        self.save_config();
    }

    pub fn delete_preset(&mut self) {
        if self.neuron_presets.is_empty() { return; }
        let idx = self.preset_cursor.min(self.neuron_presets.len() - 1);
        let name = self.neuron_presets[idx].name.clone();
        self.neuron_presets.remove(idx);
        if self.active_preset.as_deref() == Some(&name) { self.active_preset = None; }
        self.preset_cursor = self.preset_cursor.min(self.neuron_presets.len().saturating_sub(1));
        self.save_config();
    }

    pub fn toggle_config(&mut self) {
        self.show_help = false;
        self.screen = match self.screen {
            Screen::Config => Screen::ModelSelect,
            Screen::ModelSelect => {
                self.config_cursor = self.ctx_strategy.index();
                self.config_section = 0;
                Screen::Config
            }
            Screen::Chat => Screen::Chat,
        };
    }

    pub fn select_model(&mut self) {
        if let Some(entry) = self.models.get(self.model_cursor) {
            let name = entry.name.clone();
            self.selected_model = Some(name.clone());
            self.warmup_last_hash = None;
            self.context_length = crate::ollama::fetch_context_length(&self.base_url, &name);
            self.used_tokens = 0;
            self.messages.clear();
            self.input.clear();
            self.cursor_pos = 0;
            self.scroll = 0;
            self.auto_scroll = true;
            self.stream_state = StreamState::Idle;
            self.chat_focus = ChatFocus::Input;
            self.history_cursor = 0;
            self.ask = None;
            self.ask_cursor = 0;
            self.current_mood = None;
            self.pending_patch = None;
            self.screen = Screen::Chat;

            self.trigger_warmup();
        }
    }

    pub fn send_message(&mut self) {
        if self.input.trim().is_empty() || self.stream_state == StreamState::Streaming {
            return;
        }
        let raw = self.input.trim().to_string();
        // push to history (skip if duplicate of last entry)
        if self.input_history.last().map(String::as_str) != Some(&raw) {
            self.input_history.push(raw.clone());
            if self.input_history.len() > 100 {
                self.input_history.remove(0);
            }
        }
        self.history_pos = None;
        self.input_draft.clear();
        self.input.clear();
        self.cursor_pos = 0;
        self.completion = None;
        self.chat_focus = ChatFocus::Input;

        let diff_note = self.collect_pinned_diffs();

        let (display, mut llm_content, attachments, images) =
            resolve_attachments(&raw, &self.working_dir, self.context_length, self.used_tokens);

        if !diff_note.is_empty() {
            llm_content = format!("{diff_note}\n\n{llm_content}");
        }

        if let Some(fp) = &self.file_panel {
            let ui_note = format!("[UI context: the file panel is showing \"{}\"]\n\n", fp.display_path);
            llm_content = format!("{ui_note}{llm_content}");
        }

        self.messages.push(Message {
            role: Role::User,
            content: display,
            llm_content,
            images: images.clone(),
            attachments,
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: None,
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
            thinking_secs: None,
            stats: None,
            tool_call: None,
        });

        let model = self.selected_model.clone().unwrap();
        let base_url = self.base_url.clone();
        let num_ctx = match self.ctx_strategy {
            CtxStrategy::Full => self.context_length,
            CtxStrategy::Dynamic => self.context_length.map(|max| {
                let needed = (self.used_tokens * 2).max(8192);
                let rounded = if self.ctx_pow2 { needed.next_power_of_two() } else { needed };
                rounded.min(max)
            }),
        };

        let mut chat_messages: Vec<ChatMessage> = Vec::new();
        let system = self.full_system_prompt();
        if !system.is_empty() {
            chat_messages.push(ChatMessage {
                role: "system".to_string(),
                content: system,
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
        self.thinking_end_secs = None;

        let gen_params = self.gen_params;
        let keep_alive = self.keep_alive;
        std::thread::spawn(move || {
            crate::ollama::stream_chat(&base_url, model, chat_messages, num_ctx, gen_params, keep_alive, tx);
        });
    }

    pub fn poll_warmup(&mut self) {
        if let Some(rx) = &self.warmup_rx {
            if rx.try_recv().is_ok() {
                self.warmup_rx = None;
                self.warmup_started_at = None;
            }
        }
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
                                // capture thinking end time on first content token
                                if !msg.content.is_empty()
                                    && last.content.is_empty()
                                    && !last.thinking.is_empty()
                                    && self.thinking_end_secs.is_none()
                                {
                                    self.thinking_end_secs = self.stream_started_at
                                        .map(|t| t.elapsed().as_secs_f64());
                                }
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
                                    // strip <tool>...</tool> from the display content only.
                                    // llm_content keeps the <tool> tag so the model can see
                                    // its own tool calls in the conversation history — without
                                    // this, the model loses context and loops forever.
                                    // must find <tool> AFTER </think> to avoid truncating
                                    // inside a thinking block.
                                    if let Some(last) = self.messages.last_mut() {
                                        let tool_pos = match last.content.rfind("</think>") {
                                            Some(i) => last.content[i + 8..]
                                                .find("<tool>")
                                                .map(|p| i + 8 + p),
                                            None => last.content.find("<tool>"),
                                        };
                                        if let Some(pos) = tool_pos {
                                            last.content.truncate(pos);
                                            last.content = last.content.trim_end().to_string();
                                            // llm_content intentionally NOT updated here
                                        }
                                    }
                                    // capture thinking duration for the intermediate message
                                    if let Some(last) = self.messages.last_mut() {
                                        last.thinking_secs = self.thinking_end_secs
                                            .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                                    }
                                    // stop current stream
                                    self.stream_state = StreamState::Idle;
                                    self.stream_started_at = None;
                                    self.thinking_end_secs = None;
                                    // execute tool and restart stream
                                    self.handle_tool_call(&call);
                                    return;
                                }
                            }
                        }
                        // detect <ask> input request tag
                        let ask_info: Option<(AskKind, String)> = if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant { extract_ask_tag(&last.content) } else { None }
                        } else { None };
                        if let Some((kind, question)) = ask_info {
                            if let Some(last) = self.messages.last_mut() {
                                let scan_from = last.content.rfind("</think>").map(|i| i + 8).unwrap_or(0);
                                if let Some(p) = last.content[scan_from..].find("<ask") {
                                    last.content.truncate(scan_from + p);
                                    last.content = last.content.trim_end().to_string();
                                }
                            }
                            self.stream_state = StreamState::Idle;
                            self.stream_started_at = None;
                            self.thinking_end_secs = None;
                            self.ask = Some(InputRequest { question, kind });
                            self.ask_cursor = 0;
                            return;
                        }
                        // detect <patch> tag — show diff, ask confirmation, stop stream
                        let patch_content: Option<String> = if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant { extract_patch_tag(&last.content) } else { None }
                        } else { None };
                        if let Some(diff) = patch_content {
                            if let Some(last) = self.messages.last_mut() {
                                let scan_from = last.content.rfind("</think>").map(|i| i + 8).unwrap_or(0);
                                if let Some(p) = last.content[scan_from..].find("<patch>") {
                                    let abs = scan_from + p;
                                    if let Some(end) = last.content[abs..].find("</patch>") {
                                        let tag_end = abs + end + 8;
                                        let before = last.content[..abs].trim_end().to_string();
                                        let after  = last.content[tag_end..].to_string();
                                        let rendered = format!("```diff\n{}\n```", diff.trim());
                                        last.content = if before.is_empty() {
                                            rendered + &after
                                        } else {
                                            format!("{before}\n{rendered}{after}")
                                        };
                                    }
                                }
                                last.thinking_secs = self.thinking_end_secs
                                    .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                            }
                            self.pending_patch = Some(diff);
                            self.stream_state = StreamState::Idle;
                            self.stream_started_at = None;
                            self.thinking_end_secs = None;
                            self.ask = Some(InputRequest {
                                question: "Apply this patch?".to_string(),
                                kind: AskKind::Confirm,
                            });
                            self.ask_cursor = 0;
                            return;
                        }
                        // detect <mood>...</mood> — update state, strip from display, continue streaming
                        let mood_info: Option<String> = if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant { extract_mood_tag(&last.content) } else { None }
                        } else { None };
                        if let Some(emoji) = mood_info {
                            if let Some(last) = self.messages.last_mut() {
                                let scan_from = last.content.rfind("</think>").map(|i| i + 8).unwrap_or(0);
                                if let Some(p) = last.content[scan_from..].find("<mood>") {
                                    let abs = scan_from + p;
                                    if let Some(end) = last.content[abs..].find("</mood>") {
                                        let tag_end = abs + end + 7;
                                        let before = last.content[..abs].trim_end().to_string();
                                        let after = last.content[tag_end..].to_string();
                                        last.content = before + &after;
                                    }
                                }
                            }
                            self.current_mood = Some(emoji);
                        }
                    }
                    if chunk.done {
                        // attach token stats
                        let wall_secs = self.stream_started_at
                            .map(|t| t.elapsed().as_secs_f64())
                            .unwrap_or(0.0);
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
                                    response_tokens: et,
                                    tokens_per_sec: tps,
                                    thinking_secs: self.thinking_end_secs,
                                    prompt_eval_count: pt,
                                    wall_secs,
                                });
                            }
                        } else if let Some(last) = self.messages.last_mut() {
                            last.stats = Some(TokenStats {
                                thinking_secs: self.thinking_end_secs,
                                wall_secs,
                                ..Default::default()
                            });
                        }
                        self.thinking_end_secs = None;
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

    pub fn copy_last_response(&mut self) {
        let text = self.messages.iter().rev()
            .find(|m| m.role == Role::Assistant && !m.content.is_empty())
            .map(|m| m.content.clone());
        if let Some(text) = text {
            if crate::clipboard::copy(&text) {
                self.copy_notice = Some(std::time::Instant::now());
            }
        }
    }

    /// Returns message indices that are visible/navigable in History mode.
    pub fn navigable_messages(&self) -> Vec<usize> {
        self.messages.iter().enumerate()
            .filter(|(_, m)| {
                !(m.role == Role::Assistant && m.content.is_empty() && m.thinking.is_empty())
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn enter_history_mode(&mut self) {
        let navigable = self.navigable_messages();
        if !navigable.is_empty() {
            self.history_cursor = *navigable.last().unwrap();
            self.chat_focus = ChatFocus::History;
            self.auto_scroll = false;
        }
    }

    pub fn history_nav_prev(&mut self) {
        let navigable = self.navigable_messages();
        if let Some(pos) = navigable.iter().position(|&i| i == self.history_cursor) {
            if pos > 0 {
                self.history_cursor = navigable[pos - 1];
            }
        }
    }

    pub fn history_nav_next(&mut self) {
        let navigable = self.navigable_messages();
        if let Some(pos) = navigable.iter().position(|&i| i == self.history_cursor) {
            if pos + 1 < navigable.len() {
                self.history_cursor = navigable[pos + 1];
            }
        }
    }

    pub fn copy_block(&mut self, idx: usize) {
        if let Some(msg) = self.messages.get(idx) {
            let text = msg.content.clone();
            if !text.is_empty() && crate::clipboard::copy(&text) {
                self.copy_notice = Some(std::time::Instant::now());
            }
        }
    }

    pub fn submit_ask(&mut self, response: String) {
        // patch confirmation: apply or decline before normal ask handling
        if let Some(diff) = self.pending_patch.take() {
            self.ask = None;
            self.ask_cursor = 0;
            self.input.clear();
            self.cursor_pos = 0;
            self.auto_scroll = true;
            let result = if response == "Yes" {
                apply_patch(&diff, &self.working_dir)
            } else {
                "Patch declined.".to_string()
            };
            self.messages.push(Message {
                role: Role::Tool,
                content: result.clone(),
                llm_content: result,
                images: vec![],
                attachments: vec![],
                thinking: String::new(),
                thinking_secs: None,
                stats: None,
                tool_call: Some("⊕ patch".to_string()),
            });
            self.start_stream();
            return;
        }

        let ask = match self.ask.take() { Some(a) => a, None => return };
        let label = if ask.question.is_empty() {
            "↩ User selection".to_string()
        } else {
            format!("↩ {}", ask.question)
        };
        self.messages.push(Message {
            role: Role::Tool,
            content: response.clone(),
            llm_content: format!("User response: {response}"),
            images: vec![],
            attachments: vec![],
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: Some(label),
        });
        self.input.clear();
        self.cursor_pos = 0;
        self.ask_cursor = 0;
        self.auto_scroll = true;
        self.start_stream();
    }

    pub fn cancel_ask(&mut self) {
        self.ask = None;
        self.ask_cursor = 0;
        self.input.clear();
        self.cursor_pos = 0;
    }

    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.scroll = 0;
        self.auto_scroll = true;
        self.stream_state = StreamState::Idle;
        self.stream_rx = None;
        self.completion = None;
        self.ask = None;
        self.ask_cursor = 0;
        self.current_mood = None;
        self.pending_patch = None;
    }

    // --- pinned files ---

    pub fn effective_enabled_neurons(&self) -> Vec<&crate::synapse::Neuron> {
        match self.neuron_mode {
            NeuronMode::Presets => {
                if let Some(ref pname) = self.active_preset {
                    if pname == "__pure__" { return vec![]; }
                    if let Some(preset) = self.neuron_presets.iter().find(|p| &p.name == pname) {
                        return self.neurons.iter()
                            .filter(|n| preset.enabled.contains(&n.name))
                            .collect();
                    }
                }
                self.neurons.iter().filter(|n| !self.disabled_neurons.contains(&n.name)).collect()
            }
            _ => self.neurons.iter().filter(|n| !self.disabled_neurons.contains(&n.name)).collect(),
        }
    }

    fn full_system_prompt(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        let enabled = self.effective_enabled_neurons();
        match self.neuron_mode {
            NeuronMode::Smart => {
                let reasoning: Vec<&crate::synapse::Neuron> = enabled.iter()
                    .filter(|n| !neuron_is_tooling(n)).copied().collect();
                let tooling: Vec<&crate::synapse::Neuron> = enabled.iter()
                    .filter(|n| neuron_is_tooling(n)).copied().collect();
                let ctx = crate::synapse::build_tool_context(&reasoning);
                if !ctx.is_empty() { parts.push(ctx); }
                if !tooling.is_empty() {
                    let mut manifest = "## On-demand neurons\nNot currently loaded. Request one with `<load_neuron>Name</load_neuron>` when you need it:\n".to_string();
                    for n in &tooling {
                        let desc = if n.description.is_empty() { String::new() } else { format!(" — {}", n.description) };
                        manifest.push_str(&format!("- **{}**{}\n", n.name, desc));
                    }
                    parts.push(manifest);
                }
            }
            _ => {
                let ctx = crate::synapse::build_tool_context(&enabled);
                if !ctx.is_empty() { parts.push(ctx); }
            }
        }
        if !self.pinned_files.is_empty() {
            let mut section = "## Pinned files\n\nThese files are always in context:".to_string();
            for pf in &self.pinned_files {
                section.push_str(&format!("\n\n<file_content path=\"{}\">\n{}\n</file_content>", pf.display, pf.content));
            }
            parts.push(section);
        }
        parts.join("\n\n---\n\n")
    }

    pub fn trigger_warmup(&mut self) {
        if !self.warmup { return; }
        let Some(model) = self.selected_model.clone() else { return };
        let system = self.full_system_prompt();
        if system.is_empty() { return; }
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            system.hash(&mut h);
            h.finish()
        };
        if self.warmup_last_hash == Some(hash) { return; }
        self.warmup_last_hash = Some(hash);
        let num_ctx = match self.ctx_strategy {
            CtxStrategy::Full => self.context_length,
            CtxStrategy::Dynamic => self.context_length.map(|max| {
                let rounded = if self.ctx_pow2 { 8192u64.next_power_of_two() } else { 8192 };
                rounded.min(max)
            }),
        };
        let base_url   = self.base_url.clone();
        let keep_alive = self.keep_alive;
        let (tx, rx) = mpsc::channel();
        self.warmup_rx = Some(rx);
        self.warmup_started_at = Some(std::time::Instant::now());
        self.warmup_prompt_tokens = Some((system.len() / 4) as u64);
        std::thread::spawn(move || {
            crate::ollama::warmup(&base_url, model, system, num_ctx, keep_alive);
            let _ = tx.send(());
        });
    }

    /// Check mtime of all pinned files and update `changed` flag.
    pub fn check_pinned_files(&mut self) {
        for pf in &mut self.pinned_files {
            let new_mtime = pf.path.metadata().ok().and_then(|m| m.modified().ok());
            pf.changed = new_mtime != pf.mtime;
        }
    }

    /// Generate diffs for changed pinned files, update snapshots, return note for llm_content.
    fn collect_pinned_diffs(&mut self) -> String {
        let mut notes: Vec<String> = Vec::new();
        for pf in &mut self.pinned_files {
            let Ok(new_content) = std::fs::read_to_string(&pf.path) else { continue };
            let new_mtime = pf.path.metadata().ok().and_then(|m| m.modified().ok());
            if new_mtime == pf.mtime && new_content == pf.content { continue; }
            let diff = file_diff(&pf.content, &new_content, &pf.display);
            if !diff.is_empty() {
                notes.push(format!("[{} changed since your last response]\n```diff\n{}\n```", pf.display, diff.trim()));
            }
            pf.content = new_content;
            pf.mtime   = new_mtime;
            pf.changed = false;
        }
        notes.join("\n\n")
    }

    pub fn pin_file(&mut self, display: String) {
        if self.pinned_files.iter().any(|pf| pf.display == display) { return; }
        let path = self.working_dir.join(&display);
        let Ok(content) = std::fs::read_to_string(&path) else { return };
        let mtime = path.metadata().ok().and_then(|m| m.modified().ok());
        self.pinned_files.push(PinnedFile { path, display, content, mtime, changed: false });
        self.trigger_warmup();
    }

    pub fn unpin_file(&mut self, display: &str) {
        self.pinned_files.retain(|pf| pf.display != display);
        self.trigger_warmup();
    }

    pub fn open_file_picker(&mut self) {
        let dir = self.working_dir.clone();
        let entries = load_picker_entries(&dir, &dir);
        self.file_picker = Some(FilePicker {
            current_dir: dir, entries, cursor: 0, query: String::new(),
            preview: vec![], preview_scroll: 0,
            pending_path: None, highlight_rx: None,
        });
        self.update_preview();
    }

    pub fn close_file_picker(&mut self) {
        self.file_picker = None;
    }

    // --- file panel (right-side viewer) ---

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
        self.file_panel = Some(FilePanel { path, display_path, lines, scroll: 0, mtime, reloaded_at: None });
        self.file_panel_visible = true;
    }

    pub fn toggle_file_panel(&mut self) {
        if self.file_panel.is_none() { return; }
        self.file_panel_visible = !self.file_panel_visible;
        if !self.file_panel_visible && self.chat_focus == ChatFocus::FilePanel {
            self.chat_focus = ChatFocus::Input;
        }
    }

    pub fn close_file_panel(&mut self) {
        self.file_panel = None;
        self.file_panel_attachment = None;
        if self.chat_focus == ChatFocus::FilePanel {
            self.chat_focus = ChatFocus::Input;
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
            FilePickerEntry::Parent => { self.file_picker_go_up(); return; } // go_up calls update_preview
            FilePickerEntry::Dir(name) => {
                let new_dir    = self.file_picker.as_ref().unwrap().current_dir.join(&name);
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

    // --- synapse execution ---

    fn handle_tool_call(&mut self, call: &str) {
        let call = call.trim();
        let (cmd, args) = call.split_once(' ').unwrap_or((call, ""));
        let args = args.trim().trim_start_matches('@');

        // search across all neurons for a matching behaviour
        let found = self.neurons.iter().find_map(|n| {
            n.synapses.iter().find(|s| s.trigger == cmd).map(|s| (n.name.clone(), s.clone()))
        });

        // fall back to shell passthrough if no specific behaviour matched
        let shell_neuron = if found.is_none() {
            self.neurons.iter().find(|n| n.shell)
        } else {
            None
        };

        let result = if let Some((_, b)) = &found {
            let crate::synapse::SynapseKind::Tool { command, .. } = &b.kind;
            execute_command(command, args, &self.working_dir)
        } else if shell_neuron.is_some() {
            execute_command(cmd, args, &self.working_dir)
        } else {
            format!("unknown tool: {cmd}")
        };

        let tool_label = found
            .as_ref()
            .map(|(neuron_name, b)| format!("{} \u{203a} {}", neuron_name, b.trigger))
            .or_else(|| shell_neuron.map(|n| format!("{} \u{203a} {}", n.name, cmd)))
            .unwrap_or_else(|| cmd.to_string());

        let filename = Path::new(args)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| if args.is_empty() { ".".to_string() } else { args.to_string() });
        let size = result.len();
        let llm_content = format!("Tool result:\n{result}");

        self.messages.push(Message {
            role: Role::Tool,
            content: result,
            llm_content,
            images: vec![],
            attachments: vec![Attachment {
                filename,
                path: PathBuf::new(), // tool outputs don't correspond to a real file path
                kind: AttachmentKind::Text,
                size,
            }],
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: Some(tool_label),
        });
        self.auto_scroll = true;
        self.start_stream();
    }


    // --- input editing helpers ---

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

    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.input.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(self.input.len())
    }

    // --- path completion ---

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

// ── tool execution ────────────────────────────────────────────────────────────

/// Returns the content between `<tool>` and `</tool>` if both tags are present,
/// ignoring anything inside `<think>...</think>` blocks (model reasoning).
fn extract_tool_call(content: &str) -> Option<&str> {
    // if a <think> block is open but not yet closed, we're still inside reasoning — skip
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],  // only look after the closing think tag
        None => {
            if content.contains("<think>") {
                return None;            // still inside an open <think> block
            }
            content
        }
    };
    let start = scan.find("<tool>")? + 6;
    let end   = scan.find("</tool>")?;
    if end >= start { Some(&scan[start..end]) } else { None }
}

/// Executes a command via `sh -c` so the full shell syntax is supported:
/// quotes, spaces, redirections, pipes, etc.
/// `command` may include fixed flags (e.g. "grep -rn").
/// `args` are appended after the fixed command string.
/// Runs with `working_dir` as the current directory.
fn execute_command(command: &str, args: &str, working_dir: &Path) -> String {
    if command.is_empty() {
        return "error: empty command".to_string();
    }
    let full = if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    };

    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&full)
        .current_dir(working_dir)
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                let msg = if !stderr.is_empty() { stderr } else { stdout };
                format!("error: {msg}")
            } else if stdout.is_empty() {
                "Done.".to_string()
            } else {
                stdout.into_owned()
            }
        }
        Err(e) => format!("error: {e}"),
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

// ── pinned file helpers ───────────────────────────────────────────────────────

/// Generate a unified diff between two content strings using the `diff` command.
fn file_diff(old: &str, new: &str, label: &str) -> String {
    let dir = std::env::temp_dir();
    let old_path = dir.join("cognilite_pin_old.txt");
    let new_path = dir.join("cognilite_pin_new.txt");
    let _ = std::fs::write(&old_path, old);
    let _ = std::fs::write(&new_path, new);
    let out = std::process::Command::new("diff")
        .args(["-u", "--label", &format!("a/{label}"), "--label", &format!("b/{label}")])
        .arg(&old_path).arg(&new_path).output();
    let _ = std::fs::remove_file(&old_path);
    let _ = std::fs::remove_file(&new_path);
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
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
    let ss = SYNTAX_SET.get_or_init(|| two_face::syntax::extra_newlines());
    let ts = THEME_SET.get_or_init(ThemeSet::load_defaults);
    let theme = &ts.themes["base16-ocean.dark"];
    let syntax = ss.find_syntax_for_file(path).ok().flatten()
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);
    LinesWithEndings::from(&content).enumerate().take(500).map(|(i, line)| {
        let ranges = h.highlight_line(line, ss).unwrap_or_default();
        let mut spans = vec![Span::styled(
            format!("{:4} ", i + 1),
            Style::default().fg(Color::Rgb(88, 91, 112)),
        )];
        spans.extend(syntax_to_spans(&ranges));
        Line::from(spans)
    }).collect()
}

// ── template loading ──────────────────────────────────────────────────────────

fn load_templates(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(body) = std::fs::read_to_string(&path) {
                    out.push((name.to_string(), body.trim().to_string()));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
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
                                path: path.clone(),
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
                                path: path.clone(),
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

/// Detects `<ask>`, `<ask type="confirm">`, `<ask type="choice">` tags in content,
/// skipping anything inside `<think>` blocks. Returns (kind, question).
pub fn extract_ask_tag(content: &str) -> Option<(AskKind, String)> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let open = scan.find("<ask")?;
    let tag_close = scan[open..].find('>')?;
    let inner_start = open + tag_close + 1;
    let close = scan.find("</ask>")?;
    if close < inner_start { return None; }

    let tag_str = &scan[open..open + tag_close + 1];
    let inner = scan[inner_start..close].trim();

    if tag_str.contains("type=\"confirm\"") {
        Some((AskKind::Confirm, inner.to_string()))
    } else if tag_str.contains("type=\"choice\"") {
        let options: Vec<String> = inner.split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if options.is_empty() { return None; }
        Some((AskKind::Choice(options), String::new()))
    } else {
        Some((AskKind::Text, inner.to_string()))
    }
}

/// Detects a complete `<patch>...</patch>` tag outside think blocks. Returns the raw diff content.
fn extract_patch_tag(content: &str) -> Option<String> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let start = scan.find("<patch>")? + 7;
    let end   = scan.find("</patch>")?;
    if end >= start { Some(scan[start..end].to_string()) } else { None }
}

/// Applies a unified diff using `patch -p1` in the given working directory.
fn apply_patch(diff: &str, working_dir: &std::path::Path) -> String {
    let tmp = std::env::temp_dir().join("cognilite_patch.diff");
    if let Err(e) = std::fs::write(&tmp, diff) {
        return format!("error: could not write patch file: {e}");
    }
    let file = match std::fs::File::open(&tmp) {
        Ok(f) => f,
        Err(e) => return format!("error opening patch file: {e}"),
    };
    let out = std::process::Command::new("patch")
        .args(["-p1", "--batch"])
        .stdin(file)
        .current_dir(working_dir)
        .output();
    let _ = std::fs::remove_file(&tmp);
    match out {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let combined = format!("{}{}", stdout.trim(), stderr.trim());
            if o.status.success() {
                format!("Patch applied successfully.\n{}", combined.trim())
            } else {
                format!("Patch failed:\n{}", combined.trim())
            }
        }
        Err(e) => format!("error: patch command not found or failed: {e}"),
    }
}

/// Detects a complete `<mood>...</mood>` tag outside think blocks. Returns the emoji string.
fn extract_mood_tag(content: &str) -> Option<String> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let start = scan.find("<mood>")? + 6;
    let end   = scan.find("</mood>")?;
    if end >= start { Some(scan[start..end].trim().to_string()) } else { None }
}

pub fn base64_encode(data: &[u8]) -> String {
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
