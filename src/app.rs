use std::sync::mpsc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use ratatui::text::Line;
use crate::adapter::ollama::{ChatMessage, ModelEntry, StreamChunk};

use crate::runtime::picker::{FilePanel, FilePicker};
pub use crate::runtime::picker::{FilePickerEntry, highlight_code};

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Config,
    ModelSelect,
    RemoteConnect,
    Chat,
}

/// Short unique ID (4 hex chars) to disambiguate participants with the same name.
pub fn new_session_id() -> String {
    let mut buf = [0u8; 3];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut buf);
    } else {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u32;
        let p = std::process::id();
        let v = t ^ p;
        buf = [v as u8, (v >> 8) as u8, (v >> 16) as u8];
    }
    format!("{:02x}{:02x}{:02x}", buf[0], buf[1], buf[2])
}

/// Strip version tag from model name: "qwen3.6:latest" → "qwen3.6"
pub fn model_display_name(name: &str) -> &str {
    name.splitn(2, ':').next().unwrap_or(name)
}

/// Deterministic color per username from a palette of distinct Catppuccin hues.
pub fn username_color(username: &str) -> ratatui::style::Color {
    use ratatui::style::Color;
    const PALETTE: &[(u8, u8, u8)] = &[
        (166, 227, 161), // green
        (137, 180, 250), // blue
        (249, 226, 175), // yellow
        (250, 179, 135), // orange
        (243, 139, 168), // pink/red
        (116, 199, 236), // sky
        (148, 226, 213), // teal
        (180, 190, 254), // lavender
    ];
    let mut h: u64 = 5381;
    for b in username.bytes() { h = h.wrapping_mul(33).wrapping_add(b as u64); }
    let (r, g, b) = PALETTE[(h as usize) % PALETTE.len()];
    Color::Rgb(r, g, b)
}

/// Extract all `#name` or `#name#id` mentions from a message, lowercased.
pub fn extract_mentions(content: &str) -> Vec<String> {
    content.split_whitespace()
        .filter_map(|w| {
            let w = w.trim_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ':' | ';'));
            let name = w.strip_prefix('#').filter(|n| !n.is_empty())?;
            // allow alnum/_/- and at most one embedded '#' (for name#session_id format)
            let hash_count = name.chars().filter(|&c| c == '#').count();
            let valid = name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '#');
            if valid && hash_count <= 1 { Some(name.to_ascii_lowercase()) } else { None }
        })
        .collect()
}

/// Returns true if `display_username` (e.g. "qwen3#a3f2") is explicitly mentioned in `content`.
/// Only the full `#nombre#xxxx` form matches — or the special keyword `#all`.
pub fn is_mentioned(display_username: &str, content: &str) -> bool {
    let full = display_username.to_ascii_lowercase();
    let mentions = extract_mentions(content);
    mentions.iter().any(|m| m == &full || m == "all")
}

pub fn fuzzy_match(query: &str, target: &str) -> bool {
    if query.is_empty() { return true; }
    target.to_lowercase().contains(&query.to_lowercase())
}

pub use crate::domain::config::{
    CtxStrategy, GEN_PARAMS, NeuronMode, NeuronPreset, config_path, default_username, load_config,
};
pub use crate::domain::message::{Attachment, AttachmentKind, Message, Role, TokenStats};
pub use crate::domain::tags::{
    AskKind, InputRequest, extract_ask_tag, extract_load_neuron_tag, extract_mood_tag,
    extract_patch_tag, extract_preview_tag, extract_tool_call,
};
pub use crate::domain::prompt::{
    RuntimeMode, build_raw_prompt, build_runtime_context, detect_template_format,
};

pub use crate::runtime::input::{Completion, CompletionKind};
pub use crate::runtime::pinned::PinnedFile;

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

pub struct App {
    pub screen: Screen,
    pub base_url: String,
    pub working_dir: PathBuf,
    pub runtime_context: String, // injected at top of system prompt; set by headless/server mode
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
    // Smart mode: neurons deferred to <load_neuron> (not in initial system prompt).
    pub on_demand_neurons: std::collections::HashSet<String>,
    pub ctx_pow2: bool,
    pub keep_alive: bool,
    pub warmup: bool,
    pub thinking: bool,
    pub features_cursor: usize,
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
    // Raw Go template string from /api/show — used to detect the model's
    // chat format (ChatML / Llama-3 / Gemma) when we need to build a raw
    // continuation prompt after a tool call.
    pub model_template: Option<String>,
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
    pub ws_warmup_started_at: Option<std::time::Instant>, // remote warmup (WS mode)
    pub warmup_prompt_tokens: Option<u64>,
    pub warmup_last_hash: Option<u64>,     // hash of the system prompt used in the last warmup
    pub stream_started_at: Option<std::time::Instant>,
    pub thinking_end_secs: Option<f64>, // captured when first content token arrives
    pub completion: Option<Completion>,
    // neurons (groups of synapses)
    pub neurons: Vec<crate::domain::neuron::Neuron>,
    // prompt templates (name, body) loaded from .cognilite/templates/ and global dir
    pub templates: Vec<(String, String)>,
    // input history
    pub input_history: Vec<String>,
    pub history_pos: Option<usize>,
    pub input_draft: String,
    // generation params
    pub gen_params: [f64; 4],
    pub param_cursor: usize,
    // misc
    pub should_quit: bool,
    pub show_help: bool,
    pub help_scroll: u16,
    pub copy_notice: Option<std::time::Instant>,
    pub status_notice: Option<(std::time::Instant, String)>,
    pub plan_mode:   bool, // model plans only — no tool/patch execution
    pub auto_accept: bool, // auto-apply patches and confirm asks
    // chat focus / history navigation
    pub chat_focus: ChatFocus,
    pub history_cursor: usize, // index into messages[] for selected block
    // ask / user input requests
    pub ask: Option<InputRequest>,
    pub ask_cursor: usize, // selected index for Choice
    // mood
    pub current_mood: Option<String>,
    // live tokens from another room participant (user, accumulated_tokens)
    pub room_live: Option<(String, String)>,
    // pending patch waiting for user confirmation
    pub pending_patch: Option<String>,
    // pending shell tool call awaiting destructive-confirm
    pub pending_tool_call: Option<String>,
    // pinned files (always in system prompt)
    pub pinned_files: Vec<PinnedFile>,
    pub file_picker: Option<FilePicker>,
    // file panel (right-side code viewer)
    pub file_panel: Option<FilePanel>,
    pub file_panel_visible: bool,
    pub file_panel_attachment: Option<(usize, usize)>, // (msg_idx, att_idx)
    // highlight cache: path → (mtime, highlighted lines)
    pub highlight_cache: HashMap<PathBuf, (std::time::SystemTime, Vec<Line<'static>>)>,
    // neurons injected on-demand in the current conversation (Smart mode)
    pub injected_neurons: std::collections::HashSet<String>,
    // remote WebSocket client connection (--remote mode)
    pub ws_tx: Option<std::net::TcpStream>,
    pub ws_rx: Option<mpsc::Receiver<crate::adapter::ws_client::WsClientFrame>>,
    // background model fetch triggered by switch_to_local()
    pub local_models_rx: Option<mpsc::Receiver<Result<Vec<crate::adapter::ollama::ModelEntry>, String>>>,
    // remote connect screen state
    pub remote_url: String,
    pub remote_url_cursor: usize,
    pub remote_connecting: bool,
    pub remote_connect_error: Option<String>,
    pub remote_connect_rx: Option<mpsc::Receiver<Result<(std::net::TcpStream, mpsc::Receiver<crate::adapter::ws_client::WsClientFrame>), String>>>,
    pub remote_ollama_rx: Option<mpsc::Receiver<Result<Vec<crate::adapter::ollama::ModelEntry>, String>>>,
    pub remote_label: Option<String>, // shown in title bar when connected remotely
    pub username: String,
    pub session_id: String,      // unique ID for the MODEL participant in this session
    pub user_session_id: String, // unique ID for the HUMAN participant in this session
    pub room_id: Option<String>,      // UUID of the current WS room
    pub shared_room: Option<crate::adapter::ws_server::SharedRoom>, // shared room state (local server)
    pub room_synced_len: usize,       // how many room messages we've already pulled into app.messages
    pub join_room_input: Option<String>, // Some = join-room dialog open
    pub username_editing: bool,       // true while editing username in settings
    pub show_room_share: bool,        // show room share popup in chat
    // dirty flag: true when in-memory config differs from disk; flushed on
    // screen transitions and quit, not on every keystroke.
    pub config_dirty: bool,
}

impl App {
    pub fn new(base_url: String) -> Self {
        let cfg = load_config();
        let config_cursor = cfg.ctx_strategy.index();
        Self {
            screen: Screen::ModelSelect,
            base_url,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            runtime_context: String::new(),
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
            on_demand_neurons: cfg.on_demand_neurons,
            ctx_pow2: cfg.ctx_pow2,
            keep_alive: cfg.keep_alive,
            warmup: cfg.warmup,
            thinking: cfg.thinking,
            features_cursor: 0,
            config_search: String::new(),
            models: Vec::new(),
            model_cursor: 0,
            model_search: String::new(),
            loading_models: true,
            models_error: None,
            selected_model: None,
            context_length: None,
            model_template: None,
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
            ws_warmup_started_at: None,
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
                n.extend(crate::domain::neuron::load_from_dir(&local));
                if let Ok(home) = std::env::var("HOME") {
                    let global = std::path::PathBuf::from(home)
                        .join(".config/cognilite/neurons");
                    n.extend(crate::domain::neuron::load_from_dir(&global));
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
            status_notice: None,
            plan_mode:   false,
            auto_accept: false,
            chat_focus: ChatFocus::Input,
            history_cursor: 0,
            ask: None,
            ask_cursor: 0,
            current_mood: None,
            room_live: None,
            pending_patch: None,
            pending_tool_call: None,
            pinned_files: Vec::new(),
            file_picker: None,
            file_panel: None,
            file_panel_visible: true,
            file_panel_attachment: None,
            highlight_cache: HashMap::new(),
            injected_neurons: std::collections::HashSet::new(),
            ws_tx: None,
            ws_rx: None,
            local_models_rx: None,
            remote_url: String::new(),
            remote_url_cursor: 0,
            remote_connecting: false,
            remote_connect_error: None,
            remote_connect_rx: None,
            remote_ollama_rx: None,
            remote_label: None,
            username: cfg.username,
            session_id: new_session_id(),
            user_session_id: new_session_id(),
            room_id: Some(crate::adapter::ws_server::new_uuid()),
            shared_room: None,
            room_synced_len: 0,
            join_room_input: None,
            username_editing: false,
            show_room_share: false,
            config_dirty: false,
        }
    }

    /// Mark the in-memory config as dirty. The actual disk write happens at
    /// `flush_config()` time — invoked on screen transitions and quit so we
    /// don't re-serialize JSON on every arrow-key in a slider.
    fn save_config(&mut self) {
        self.config_dirty = true;
    }

    /// Persist the in-memory config to disk if dirty. Idempotent.
    pub fn flush_config(&mut self) {
        if !self.config_dirty { return; }
        self.config_dirty = false;
        let Some(path) = config_path() else { return };
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        let disabled: Vec<&str> = self.disabled_neurons.iter().map(String::as_str).collect();
        let on_demand: Vec<&str> = self.on_demand_neurons.iter().map(String::as_str).collect();
        let presets: Vec<serde_json::Value> = self.neuron_presets.iter().map(|p| {
            serde_json::json!({ "name": p.name, "enabled": p.enabled })
        }).collect();
        let json = serde_json::json!({
            "ctx_strategy":    self.ctx_strategy.as_str(),
            "disabled_neurons": disabled,
            "on_demand_neurons": on_demand,
            "temperature":      self.gen_params[0],
            "top_p":            self.gen_params[1],
            "repeat_penalty":   self.gen_params[2],
            "thinking_budget":  self.gen_params[3],
            "ctx_pow2":        self.ctx_pow2,
            "keep_alive":      self.keep_alive,
            "warmup":          self.warmup,
            "thinking":        self.thinking,
            "neuron_mode":     self.neuron_mode.as_str(),
            "neuron_presets":  presets,
            "active_preset":   self.active_preset,
            "username":        self.username,
        });
        let _ = std::fs::write(&path, json.to_string());
    }

    pub fn confirm_config(&mut self) {
        self.ctx_strategy = CtxStrategy::from_index(self.config_cursor);
        self.save_config();
        self.flush_config();
    }

    /// Full display identity: "username#session_id" — unique per connection.
    pub fn display_username(&self) -> String {
        format!("{}#{}", self.username, self.user_session_id)
    }

    pub fn set_username(&mut self, name: String) {
        let name = name.trim().to_string();
        if !name.is_empty() { self.username = name; }
        self.username_editing = false;
        self.save_config();
        self.flush_config();
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

    pub fn toggle_feature(&mut self, index: usize) {
        match index {
            0 => self.thinking = !self.thinking,
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

    /// Smart mode three-state cycle per neuron:
    ///   disabled → enabled as initial → enabled as on-demand → disabled.
    pub fn cycle_neuron_smart_state(&mut self) {
        let Some(neuron) = self.neurons.get(self.neuron_cursor) else { return };
        let name = neuron.name.clone();
        let disabled = self.disabled_neurons.contains(&name);
        let on_demand = self.on_demand_neurons.contains(&name);
        if disabled {
            // disabled → initial
            self.disabled_neurons.remove(&name);
            self.on_demand_neurons.remove(&name);
        } else if !on_demand {
            // initial → on-demand
            self.on_demand_neurons.insert(name);
        } else {
            // on-demand → disabled
            self.on_demand_neurons.remove(&name);
            self.disabled_neurons.insert(name);
        }
        self.save_config();
        self.warmup_last_hash = None;
        self.trigger_warmup();
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

    pub fn cycle_thinking_budget(&mut self) {
        const PRESETS: &[f64] = &[0.0, 512.0, 1024.0, 2048.0, 4096.0];
        let cur = self.gen_params[3];
        let next_idx = PRESETS.iter().position(|&p| p == cur)
            .map(|i| (i + 1) % PRESETS.len())
            .unwrap_or(0);
        self.gen_params[3] = PRESETS[next_idx];
        let label = if PRESETS[next_idx] == 0.0 {
            "think: unlimited".to_string()
        } else {
            format!("think: {} tok", PRESETS[next_idx] as u64)
        };
        self.status_notice = Some((std::time::Instant::now(), label));
        self.save_config();
    }

    pub fn cycle_mode(&mut self) {
        let label = match (self.plan_mode, self.auto_accept) {
            (false, false) => { self.plan_mode = true;  "plan mode" }
            (true,  false) => { self.plan_mode = false; self.auto_accept = true; "auto-accept" }
            _              => { self.plan_mode = false; self.auto_accept = false; "normal" }
        };
        self.status_notice = Some((std::time::Instant::now(), label.into()));
    }

    pub fn toggle_config(&mut self) {
        self.show_help = false;
        self.screen = match self.screen {
            Screen::Config => { self.flush_config(); Screen::ModelSelect }
            Screen::ModelSelect => {
                self.config_cursor = self.ctx_strategy.index();
                self.config_section = 0;
                Screen::Config
            }
            Screen::Chat | Screen::RemoteConnect => Screen::Chat,
        };
    }

    pub fn select_model(&mut self) {
        if let Some(entry) = self.models.get(self.model_cursor) {
            let name = entry.name.clone();
            self.selected_model = Some(name.clone());
            self.warmup_last_hash = None;
            self.context_length = crate::adapter::ollama::fetch_context_length(&self.base_url, &name);
            self.model_template = crate::adapter::ollama::fetch_template(&self.base_url, &name);
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
            self.runtime_context = build_runtime_context(&name, self.context_length, RuntimeMode::Tui);

            self.trigger_warmup();
        }
    }

    /// Send model selection to the server and show the warmup spinner while it sets up.
    pub fn select_model_remote(&mut self) {
        let Some(entry) = self.models.get(self.model_cursor) else { return };
        let name = entry.name.clone();
        if let Some(ref mut tx) = self.ws_tx {
            crate::adapter::ws_client::send_json(tx, serde_json::json!({"type":"select_model","model":name}));
        }
        self.selected_model = Some(name);
        self.messages.clear();
        self.input.clear();
        self.cursor_pos = 0;
        self.scroll = 0;
        self.auto_scroll = true;
        self.stream_state = StreamState::Streaming; // spinner while server does warmup
        self.chat_focus = ChatFocus::Input;
        self.current_mood = None;
        self.pending_patch = None;
        self.ask = None;
        self.screen = Screen::Chat;
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

        // ── Remote WS path ───────────────────────────────────────────────
        if self.ws_tx.is_some() {
            // extract @path refs for the server; keep the rest as the visible message
            let (text, attach_paths) = split_at_paths(&raw);
            let content = if text.is_empty() { raw.clone() } else { text };

            // build local display (no file reading — files live on the server)
            let attachments: Vec<Attachment> = attach_paths.iter().map(|p| {
                let path = std::path::Path::new(p);
                Attachment {
                    filename: path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.clone()),
                    path: PathBuf::from(p),
                    kind: AttachmentKind::Text,
                    size: 0,
                }
            }).collect();

            let user_identity = self.display_username();
            let model_identity = self.selected_model.as_deref()
                .filter(|m| !m.is_empty())
                .map(|m| format!("{}#{}", model_display_name(m), self.session_id));
            self.messages.push(Message {
                role: Role::User,
                content: raw.clone(),
                llm_content: raw.clone(),
                images: vec![],
                attachments,
                thinking: String::new(),
                thinking_secs: None,
                stats: None,
                tool_call: Some(user_identity),
                tool_collapsed: false,
            });
            // empty assistant placeholder — identity locked at send time
            self.messages.push(Message {
                role: Role::Assistant,
                content: String::new(),
                llm_content: String::new(),
                images: vec![],
                attachments: vec![],
                thinking: String::new(),
                thinking_secs: None,
                stats: None,
                tool_call: model_identity,
                tool_collapsed: false,
            });
            self.auto_scroll = true;
            self.stream_state = StreamState::Streaming;
            self.stream_started_at = Some(std::time::Instant::now());
            self.thinking_end_secs = None;

            if let Some(ref mut tx) = self.ws_tx {
                crate::adapter::ws_client::send_json(tx, serde_json::json!({
                    "type": "message",
                    "content": content,
                    "attach": attach_paths,
                }));
            }
            return;
        }

        // ── Local Ollama path ────────────────────────────────────────────
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

        if self.plan_mode {
            llm_content = format!("{llm_content}\n\n[PLAN MODE: Describe your plan step by step — which files, commands, and changes are involved. Do NOT emit <tool>, <patch>, or <ask> tags. Output the plan only.]");
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
            tool_call: Some(self.display_username()),
            tool_collapsed: false,
        });
        self.room_sync_user_msg();
        self.auto_scroll = true;

        // if the message mentions specific participants and the local model/user is not among them,
        // don't trigger the local stream — let the mentioned remote participant respond
        if !extract_mentions(&raw).is_empty() {
            let local_model = model_display_name(self.selected_model.as_deref().unwrap_or(""));
            let addressed_here = is_mentioned(&self.display_username(), &raw)
                || is_mentioned(local_model, &raw);
            if !addressed_here {
                return; // directed elsewhere — skip local stream
            }
        }

        self.start_stream();
    }


    /// Pushes an empty assistant placeholder and starts the Ollama stream
    /// from the current message history.
    ///
    /// Two paths:
    ///   1. Fresh turn (last API-visible message is user) → /api/chat as usual.
    ///   2. Continuation (last API-visible message is an assistant with
    ///      non-empty content — happens after a tool call injects its result
    ///      into the assistant's llm_content) → /api/generate with raw:true,
    ///      using a hand-built prompt that leaves the assistant turn OPEN.
    ///      Without this, the chat template closes the turn and the model
    ///      treats the tool output as "response complete" and emits stop.
    pub fn start_stream(&mut self) {
        let Some(model) = self.selected_model.clone() else {
            self.stream_state = StreamState::Error("No model selected".to_string());
            return;
        };

        // lock identity at stream start so it stays stable across tool-call restarts
        let identity = match self.selected_model.as_deref() {
            Some(m) if !m.is_empty() => format!("{}#{}", model_display_name(m), self.session_id),
            _ => self.display_username(),
        };

        self.messages.push(Message {
            role: Role::Assistant,
            content: String::new(),
            llm_content: String::new(),
            images: vec![],
            attachments: vec![],
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: Some(identity),
            tool_collapsed: false,
        });

        let base_url = self.base_url.clone();
        let num_ctx = match self.ctx_strategy {
            CtxStrategy::Full => self.context_length,
            CtxStrategy::Dynamic => self.context_length.map(|max| {
                // Estimate tokens from current message content (4 chars ≈ 1 token).
                // Tool results can be large and are added AFTER used_tokens was last
                // recorded (stream is interrupted on tool calls, so used_tokens never
                // updates mid-conversation). Using the content sizes avoids context
                // overflow that would silently drop the original user question.
                let msg_tokens: u64 = self.messages
                    .iter()
                    .filter(|m| !m.llm_content.is_empty() || m.role == Role::User)
                    .map(|m| (m.llm_content.len() / 4) as u64)
                    .sum();
                let base = self.used_tokens.max(msg_tokens);
                let needed = (base * 2).max(8192);
                let rounded = if self.ctx_pow2 { needed.next_power_of_two() } else { needed };
                rounded.min(max)
            }),
        };

        // Build the history the API would see. The just-pushed placeholder has
        // empty llm_content so it's filtered out.
        let (system, history) = self.build_api_history();

        let (tx, rx) = mpsc::channel();
        self.stream_rx = Some(rx);
        self.stream_state = StreamState::Streaming;
        self.stream_started_at = Some(std::time::Instant::now());
        self.thinking_end_secs = None;

        let gen_params = self.gen_params;
        let keep_alive = self.keep_alive;
        let thinking   = self.thinking;

        // Detect continuation and, if the template format is known, build a
        // raw prompt. Images disqualify the raw path (the generate endpoint
        // doesn't accept them in raw mode) — fall back to /api/chat.
        let is_continuation = history.last().is_some_and(|(r, _)| r == "assistant");
        let has_images = self.messages.iter().any(|m| !m.images.is_empty());
        let raw_prompt = if is_continuation && !has_images {
            self.model_template
                .as_deref()
                .and_then(detect_template_format)
                .map(|fmt| {
                    let sys = if system.is_empty() { None } else { Some(system.as_str()) };
                    build_raw_prompt(fmt, sys, &history)
                })
        } else {
            None
        };

        if let Some(prompt) = raw_prompt {
            std::thread::spawn(move || {
                crate::adapter::ollama::stream_generate_raw(&base_url, model, prompt, num_ctx, gen_params, keep_alive, thinking, tx);
            });
            return;
        }

        // /api/chat path
        let mut chat_messages: Vec<ChatMessage> = Vec::new();
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

        std::thread::spawn(move || {
            crate::adapter::ollama::stream_chat(&base_url, model, chat_messages, num_ctx, gen_params, keep_alive, thinking, tx);
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
                                    let budget = self.gen_params[3] as usize;
                                    let over = budget > 0 && last.thinking.len() / 4 >= budget;
                                    if !over { last.thinking.push_str(&t); }
                                }
                            }
                        }
                        // broadcast token to WS room clients
                        if !msg.content.is_empty() {
                            self.room_push_token(&msg.content);
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
                                            // Truncate llm_content at end of </tool> to avoid sending
                                            // text the model started generating AFTER the tag (before
                                            // the tool ran). That pre-result text confuses the model
                                            // into continuing a wrong analysis on the next turn.
                                            // We still keep the <tool>…</tool> tag itself so the
                                            // model can see it called a tool in the history.
                                            if let Some(end) = last.llm_content.find("</tool>") {
                                                last.llm_content.truncate(end + 7);
                                            }
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
                            let auto = self.auto_accept && matches!(&kind, AskKind::Confirm);
                            self.ask = Some(InputRequest { question, kind });
                            self.ask_cursor = 0;
                            if auto { self.submit_ask("Yes".to_string()); }
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
                            if self.auto_accept { self.submit_ask("Yes".to_string()); }
                            return;
                        }
                        // detect <mood>...</mood> — update state, strip from display, continue streaming
                        // check both content and thinking (native thinking models emit <mood> in thinking tokens)
                        let mood_info: Option<(String, bool)> = if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant {
                                extract_mood_tag(&last.content).map(|e| (e, false))
                                    .or_else(|| extract_mood_tag(&last.thinking).map(|e| (e, true)))
                            } else { None }
                        } else { None };
                        if let Some((emoji, in_thinking)) = mood_info {
                            if let Some(last) = self.messages.last_mut() {
                                let target = if in_thinking { &mut last.thinking } else { &mut last.content };
                                crate::domain::tags::strip_tag(target, "mood");
                            }
                            self.current_mood = Some(emoji);
                        }
                        // detect <preview path="..."/> — open file panel
                        let preview_path: Option<String> = if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant { extract_preview_tag(&last.content) } else { None }
                        } else { None };
                        if let Some(rel_path) = preview_path {
                            if let Some(last) = self.messages.last_mut() {
                                let sf = last.content.rfind("</think>").map(|i| i + 8).unwrap_or(0);
                                if let Some(p) = last.content[sf..].find("<preview") {
                                    let abs = sf + p;
                                    if let Some(end) = last.content[abs..].find("/>") {
                                        let tag_end = abs + end + 2;
                                        let before = last.content[..abs].trim_end().to_string();
                                        last.content = before + &last.content[tag_end..];
                                    }
                                }
                            }
                            let path = self.working_dir.join(&rel_path);
                            self.open_file_panel(path);
                        }
                        // detect <load_neuron>Name</load_neuron> — inject neuron and restart stream
                        let load_name: Option<String> = if let Some(last) = self.messages.last() {
                            if last.role == Role::Assistant { extract_load_neuron_tag(&last.content) } else { None }
                        } else { None };
                        if let Some(name) = load_name {
                            if !self.injected_neurons.contains(&name) {
                                let neuron_content = self.neurons.iter()
                                    .find(|n| n.name.eq_ignore_ascii_case(&name))
                                    .map(|n| format!("## Neuron: {}\n\n{}", n.name, n.system_prompt));
                                if let Some(content) = neuron_content {
                                    // strip <load_neuron> tag from display only
                                    if let Some(last) = self.messages.last_mut() {
                                        crate::domain::tags::strip_tag(&mut last.content, "load_neuron");
                                        last.thinking_secs = self.thinking_end_secs
                                            .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                                    }
                                    self.injected_neurons.insert(name.clone());
                                    let label = format!("Neuron \u{203a} {}", name);
                                    let size = content.len();
                                    self.messages.push(Message {
                                        role: Role::Tool,
                                        content: content.clone(),
                                        llm_content: format!("Neuron loaded:\n{content}"),
                                        images: vec![],
                                        attachments: vec![Attachment {
                                            filename: name,
                                            path: PathBuf::new(),
                                            kind: AttachmentKind::Text,
                                            size,
                                        }],
                                        thinking: String::new(),
                                        thinking_secs: None,
                                        stats: None,
                                        tool_call: Some(label),
                                        tool_collapsed: true,
                                    });
                                    self.stream_state = StreamState::Idle;
                                    self.stream_started_at = None;
                                    self.thinking_end_secs = None;
                                    self.auto_scroll = true;
                                    self.start_stream();
                                    return;
                                }
                            }
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
                        // tag the completed assistant message with model identity for the room
                        let display = match self.selected_model.as_deref() {
                            Some(m) if !m.is_empty() => format!("{}#{}", model_display_name(m), self.session_id),
                            _ => self.display_username(),
                        };
                        if let Some(last) = self.messages.last_mut() {
                            if last.role == Role::Assistant { last.tool_call = Some(display); }
                        }
                        self.room_sync_done();
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

    /// Poll incoming frames from a remote WebSocket server and update TUI state.
    /// Returns after consuming all immediately available frames, or after a control
    /// frame (Ask/Patch/Done/Error) that requires a state change.
    pub fn poll_ws(&mut self) {
        let rx = match self.ws_rx.take() {
            Some(r) => r,
            None => return,
        };
        loop {
            match rx.try_recv() {
                Ok(frame) => {
                    let keep_alive = self.handle_ws_frame(frame);
                    if !keep_alive {
                        // connection dead — don't put rx back
                        return;
                    }
                    // stop this batch on control frames (Ask/Patch/Done/Error)
                    // detected by stream_state having changed to non-Streaming
                    if self.stream_state != StreamState::Streaming {
                        self.ws_rx = Some(rx);
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => { self.ws_rx = Some(rx); return; }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.stream_state = StreamState::Error("Remote server disconnected".to_string());
                    return;
                }
            }
        }
    }

    /// Process one WsClientFrame. Returns false if the connection is dead (ws_rx should not
    /// be restored), true otherwise.
    fn handle_ws_frame(&mut self, frame: crate::adapter::ws_client::WsClientFrame) -> bool {
        use crate::adapter::ws_client::WsClientFrame as F;
        match frame {
            F::Models { entries } => {
                self.models = entries;
                self.loading_models = false;
                self.model_cursor = 0;
                self.model_search.clear();
                self.screen = Screen::ModelSelect;
            }

            F::Connected { model, room_id, session_id, username, user_session_id, .. } => {
                // if models weren't sent (old server or --model in query), populate fallback
                if self.models.is_empty() {
                    self.models = vec![crate::adapter::ollama::ModelEntry {
                        name: model.clone(),
                        parameter_size: None,
                        quantization_level: None,
                        size_bytes: None,
                    }];
                    self.loading_models = false;
                }
                self.selected_model = Some(model);
                if !room_id.is_empty() { self.room_id = Some(room_id); }
                // adopt server-assigned identities so labels are consistent with the room
                if !session_id.is_empty()      { self.session_id      = session_id; }
                if !user_session_id.is_empty() { self.user_session_id = user_session_id; }
                if !username.is_empty() {
                    // server sends display_username() = "name#user_id"; extract the name part
                    let bare = username.rsplit_once('#').map(|(n, _)| n).unwrap_or(&username);
                    self.username = bare.to_string();
                }
                if self.stream_state == StreamState::Streaming {
                    self.stream_state = StreamState::Idle;
                }
            }
            F::WarmupStart => { self.ws_warmup_started_at = Some(std::time::Instant::now()); }
            F::WarmupDone  => { self.ws_warmup_started_at = None; }
            F::Unknown => {}

            F::Token(s) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        if !s.is_empty()
                            && last.content.is_empty()
                            && !last.thinking.is_empty()
                            && self.thinking_end_secs.is_none()
                        {
                            self.thinking_end_secs = self.stream_started_at
                                .map(|t| t.elapsed().as_secs_f64());
                        }
                        last.content.push_str(&s);
                        last.llm_content.push_str(&s);
                    }
                }
            }
            F::ThinkingStart => {}
            F::Thinking(s) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.thinking.push_str(&s);
                    }
                }
            }
            F::ThinkingEnd => {
                if self.thinking_end_secs.is_none() {
                    self.thinking_end_secs = self.stream_started_at.map(|t| t.elapsed().as_secs_f64());
                }
            }

            F::Tool { command, label, result } => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.thinking_secs = self.thinking_end_secs
                            .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                        // Same as local path: inject result into assistant llm_content so
                        // the API never sees a "user" role carrying tool output.
                        last.llm_content.push_str(&format!("\n[Tool output for: {command}]\n{result}"));
                    }
                }
                let label_display = if label.is_empty() { command.clone() } else { label };
                let fname = command.split_whitespace().nth(1)
                    .map(str::to_string)
                    .unwrap_or_else(|| ".".to_string());
                let size = result.len();
                self.messages.push(Message {
                    role: Role::Tool,
                    content: result.clone(),
                    llm_content: String::new(), // empty → excluded from API, UI-only
                    images: vec![],
                    attachments: vec![Attachment {
                        filename: fname, path: PathBuf::new(),
                        kind: AttachmentKind::Text, size,
                    }],
                    thinking: String::new(), thinking_secs: None, stats: None,
                    tool_call: Some(label_display),
                    tool_collapsed: true,
                });
                // new assistant placeholder for the follow-up response — same identity
                let ws_identity = self.selected_model.as_deref()
                    .filter(|m| !m.is_empty())
                    .map(|m| format!("{}#{}", model_display_name(m), self.session_id));
                self.messages.push(Message {
                    role: Role::Assistant,
                    content: String::new(), llm_content: String::new(),
                    images: vec![], attachments: vec![],
                    thinking: String::new(), thinking_secs: None, stats: None,
                    tool_call: ws_identity,
                    tool_collapsed: false,
                });
                self.thinking_end_secs = None;
                self.stream_started_at = Some(std::time::Instant::now());
                self.auto_scroll = true;
            }

            F::LoadNeuron(name) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.thinking_secs = self.thinking_end_secs
                            .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                    }
                }
                let label = format!("Neuron \u{203a} {name}");
                self.messages.push(Message {
                    role: Role::Tool,
                    content: String::new(), llm_content: String::new(),
                    images: vec![],
                    attachments: vec![Attachment {
                        filename: name, path: PathBuf::new(),
                        kind: AttachmentKind::Text, size: 0,
                    }],
                    thinking: String::new(), thinking_secs: None, stats: None,
                    tool_call: Some(label),
                    tool_collapsed: false,
                });
                let ws_identity = self.selected_model.as_deref()
                    .filter(|m| !m.is_empty())
                    .map(|m| format!("{}#{}", model_display_name(m), self.session_id));
                self.messages.push(Message {
                    role: Role::Assistant,
                    content: String::new(), llm_content: String::new(),
                    images: vec![], attachments: vec![],
                    thinking: String::new(), thinking_secs: None, stats: None,
                    tool_call: ws_identity,
                    tool_collapsed: false,
                });
                self.thinking_end_secs = None;
                self.stream_started_at = Some(std::time::Instant::now());
                self.auto_scroll = true;
            }

            F::Ask { kind, question, options } => {
                let ask_kind = match kind.as_str() {
                    "confirm" => AskKind::Confirm,
                    "choice"  => AskKind::Choice(options),
                    _         => AskKind::Text,
                };
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.content = last.content.trim_end().to_string();
                        last.thinking_secs = self.thinking_end_secs
                            .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                    }
                }
                self.ask = Some(InputRequest { question, kind: ask_kind });
                self.ask_cursor = 0;
                self.stream_state = StreamState::Idle;
                self.stream_started_at = None;
                self.thinking_end_secs = None;
            }

            F::Patch(diff) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        let rendered = format!("```diff\n{}\n```", diff.trim());
                        if !last.content.is_empty() { last.content.push('\n'); }
                        last.content.push_str(&rendered);
                        last.thinking_secs = self.thinking_end_secs
                            .or_else(|| self.stream_started_at.map(|t| t.elapsed().as_secs_f64()));
                    }
                }
                self.pending_patch = Some(diff);
                self.ask = Some(InputRequest {
                    question: "Apply this patch?".to_string(),
                    kind: AskKind::Confirm,
                });
                self.ask_cursor = 0;
                self.stream_state = StreamState::Idle;
                self.stream_started_at = None;
                self.thinking_end_secs = None;
            }

            F::Mood(emoji) => { self.current_mood = Some(emoji); }

            F::RoomUpdate { messages } => {
                for msg in messages {
                    self.messages.push(msg);
                }
                self.room_live = None; // clear any live preview — turn is done
                self.auto_scroll = true;
            }

            F::RoomToken { user, content } => {
                match &mut self.room_live {
                    Some((u, tokens)) if u == &user => tokens.push_str(&content),
                    _ => self.room_live = Some((user, content)),
                }
                self.auto_scroll = true;
            }

            F::FilePreview { path, content } => {
                self.open_file_panel_remote(&path, &content);
            }

            F::LsResult { path, entries } => {
                if let Some(fp) = &mut self.file_picker {
                    let base = if path == "." || path.is_empty() { String::new() } else { format!("{}/", path.trim_end_matches('/')) };
                    let has_parent = !path.is_empty() && path != ".";
                    let mut result: Vec<FilePickerEntry> = vec![];
                    if has_parent { result.push(FilePickerEntry::Parent); }
                    for (name, is_dir) in entries {
                        if name.starts_with('.') { continue; }
                        if is_dir {
                            result.push(FilePickerEntry::Dir(format!("{base}{name}")));
                        } else {
                            result.push(FilePickerEntry::File(format!("{base}{name}")));
                        }
                    }
                    fp.current_dir = PathBuf::from(&path);
                    fp.entries = result;
                    fp.cursor = 0;
                    fp.loading = false;
                }
            }

            F::Done { tps, tokens, prompt_eval } => {
                let wall_secs = self.stream_started_at
                    .map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);
                if let Some(last) = self.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.stats = Some(TokenStats {
                            response_tokens:   tokens,
                            tokens_per_sec:    tps,
                            thinking_secs:     self.thinking_end_secs,
                            prompt_eval_count: prompt_eval,
                            wall_secs,
                        });
                    }
                }
                self.thinking_end_secs = None;
                self.stream_state = StreamState::Idle;
                self.stream_started_at = None;
            }

            F::Error(e) => {
                self.stream_state = StreamState::Error(e);
                self.stream_started_at = None;
            }

            F::Disconnected => {
                self.stream_state = StreamState::Error("Remote server disconnected".to_string());
                self.ws_tx = None;
                return false; // connection dead — don't restore ws_rx
            }
        }
        true
    }

    pub fn stop_stream(&mut self) {
        if self.ws_tx.is_some() {
            // WS mode: can't cancel the server stream; just stop rendering tokens
            self.stream_state = StreamState::Idle;
            self.stream_started_at = None;
            return;
        }
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
            if crate::adapter::clipboard::copy(&text) {
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
            if !text.is_empty() && crate::adapter::clipboard::copy(&text) {
                self.copy_notice = Some(std::time::Instant::now());
            }
        }
    }

    pub fn submit_ask(&mut self, response: String) {
        // ── Remote WS path: send ask_response frame, wait for server tokens ──
        if self.ws_tx.is_some() {
            self.ask = None;
            self.ask_cursor = 0;
            self.input.clear();
            self.cursor_pos = 0;
            self.auto_scroll = true;

            if let Some(_diff) = self.pending_patch.take() {
                // patch is applied server-side; just show a local status message
                let status = if response == "Yes" { "Patch applied on server." } else { "Patch declined." };
                self.messages.push(Message {
                    role: Role::Tool,
                    content: status.to_string(),
                    llm_content: format!("Patch response: {response}"),
                    images: vec![], attachments: vec![],
                    thinking: String::new(), thinking_secs: None, stats: None,
                    tool_call: Some("⊕ patch".to_string()),
                    tool_collapsed: false,
                });
            } else {
                let ask_question = self.ask.as_ref().map(|a| a.question.clone()).unwrap_or_default();
                let label = if ask_question.is_empty() { "↩ User selection".to_string() }
                            else { format!("↩ {ask_question}") };
                self.messages.push(Message {
                    role: Role::Tool,
                    content: response.clone(),
                    llm_content: format!("User response: {response}"),
                    images: vec![], attachments: vec![],
                    thinking: String::new(), thinking_secs: None, stats: None,
                    tool_call: Some(label),
                    tool_collapsed: false,
                });
            }

            if let Some(ref mut tx) = self.ws_tx {
                crate::adapter::ws_client::send_json(tx, serde_json::json!({
                    "type": "ask_response", "content": response
                }));
            }
            self.stream_state = StreamState::Streaming;
            return;
        }

        // ── Local Ollama path ────────────────────────────────────────────
        // destructive shell command: execute or decline before normal ask handling
        if let Some(call) = self.pending_tool_call.take() {
            self.ask = None;
            self.ask_cursor = 0;
            self.input.clear();
            self.cursor_pos = 0;
            self.auto_scroll = true;
            if response == "Yes" {
                self.execute_tool_call(&call);
            } else {
                let cmd = call.split_whitespace().next().unwrap_or("?");
                self.push_tool_result(
                    &call,
                    "Command declined by user.".to_string(),
                    format!("\u{26d4} {cmd}"),
                    "",
                );
            }
            return;
        }
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
                tool_collapsed: false,
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
            tool_collapsed: false,
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

    /// Disconnect from a remote WS session and reload local models so the
    /// model select screen works exactly as if the app had started normally.
    pub fn switch_to_local(&mut self) {
        // close WS connection
        if let Some(ref mut tx) = self.ws_tx {
            let _ = crate::adapter::ws_client::write_frame(tx, 8, &[]); // opcode 8 = CLOSE
        }
        self.ws_tx = None;
        self.ws_rx = None;
        self.remote_label = None;
        self.ws_warmup_started_at = None;

        // reset to model select state
        self.screen = Screen::ModelSelect;
        self.stream_state = StreamState::Idle;
        self.stream_rx = None;
        self.models = Vec::new();
        self.models_error = None;
        self.loading_models = true;
        self.selected_model = None;

        // fetch local models in background so the UI stays responsive
        let base_url = self.base_url.clone();
        let (tx, rx) = mpsc::channel::<Result<Vec<crate::adapter::ollama::ModelEntry>, String>>();
        std::thread::spawn(move || {
            let _ = tx.send(crate::adapter::ollama::list_models(&base_url));
        });
        // store receiver so poll_local_models() can pick it up
        self.local_models_rx = Some(rx);
    }

    pub fn poll_local_models(&mut self) {
        let rx = match self.local_models_rx.take() {
            Some(r) => r,
            None => return,
        };
        match rx.try_recv() {
            Ok(Ok(entries)) => {
                self.models = entries;
                self.loading_models = false;
            }
            Ok(Err(e)) => {
                self.models_error = Some(e);
                self.loading_models = false;
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.local_models_rx = Some(rx); // not done yet
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loading_models = false;
            }
        }
    }

    /// Build a WS URL from remote_url, appending app settings as query params.
    pub fn remote_ws_url(&self) -> String {
        let mut url = self.remote_url.trim().to_string();
        let mut params: Vec<String> = vec!["client=tui".into()];

        match self.neuron_mode {
            NeuronMode::Presets => {
                params.push("neuron_mode=presets".into());
                if let Some(ref p) = self.active_preset {
                    params.push(format!("preset={p}"));
                }
            }
            NeuronMode::Smart  => params.push("neuron_mode=smart".into()),
            NeuronMode::Manual => params.push("neuron_mode=manual".into()),
        }

        let sep = if url.contains('?') { '&' } else { '?' };
        url.push(sep);
        url.push_str(&params.join("&"));
        url
    }

    /// Start a background WS connection attempt from the RemoteConnect screen.
    pub fn start_remote_connect(&mut self) {
        self.remote_connecting = true;
        self.remote_connect_error = None;
        self.remote_label = Some(format!("remote session · {}", self.remote_url.trim()));
        let ws_url = self.remote_ws_url();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::adapter::ws_client::connect(&ws_url));
        });
        self.remote_connect_rx = Some(rx);
    }

    /// Switch to a remote Ollama URL (remote model, local execution).
    pub fn start_remote_ollama(&mut self) {
        let url = self.remote_url.trim().to_string();
        let base_url = if url.starts_with("http://") || url.starts_with("https://") {
            url.clone()
        } else {
            format!("http://{url}")
        };
        self.base_url = base_url.clone();
        self.remote_label = Some(format!("remote ollama · {base_url}"));
        self.remote_connecting = true;
        self.remote_connect_error = None;

        let (tx, rx) = mpsc::channel::<Result<Vec<crate::adapter::ollama::ModelEntry>, String>>();
        std::thread::spawn(move || {
            let _ = tx.send(crate::adapter::ollama::list_models(&base_url));
        });
        self.remote_ollama_rx = Some(rx);
    }

    /// Poll the background Ollama model fetch triggered from RemoteConnect.
    pub fn poll_remote_ollama(&mut self) {
        let rx = match self.remote_ollama_rx.take() {
            Some(r) => r,
            None => return,
        };
        match rx.try_recv() {
            Ok(Ok(entries)) => {
                self.models = entries;
                self.loading_models = false;
                self.models_error = None;
                self.remote_connecting = false;
                self.selected_model = None;
                self.screen = Screen::ModelSelect;
            }
            Ok(Err(e)) => {
                self.remote_connect_error = Some(e);
                self.remote_connecting = false;
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.remote_ollama_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.remote_connect_error = Some("connection failed".into());
                self.remote_connecting = false;
            }
        }
    }

    /// Poll the background WS connection. On success, transition to Chat.
    pub fn poll_remote_connect(&mut self) {
        let rx = match self.remote_connect_rx.take() {
            Some(r) => r,
            None => return,
        };
        match rx.try_recv() {
            Ok(Ok((ws_tx, ws_rx))) => {
                self.ws_tx = Some(ws_tx);
                self.ws_rx = Some(ws_rx);
                self.messages.clear();
                self.models = vec![];
                self.loading_models = true;
                self.model_cursor = 0;
                self.model_search.clear();
                self.selected_model = None;
                self.screen = Screen::ModelSelect;
                self.stream_state = StreamState::Idle;
                self.remote_connecting = false;
            }
            Ok(Err(e)) => {
                self.remote_connect_error = Some(e);
                self.remote_connecting = false;
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.remote_connect_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.remote_connect_error = Some("connection failed".into());
                self.remote_connecting = false;
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
        self.ask = None;
        self.ask_cursor = 0;
        self.current_mood = None;
        self.pending_patch = None;
        self.injected_neurons.clear();
    }

    // --- pinned files ---

    pub fn effective_enabled_neurons(&self) -> Vec<&crate::domain::neuron::Neuron> {
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
        if !self.runtime_context.is_empty() {
            parts.push(self.runtime_context.clone());
        }
        let enabled = self.effective_enabled_neurons();
        match self.neuron_mode {
            NeuronMode::Smart => {
                // User chooses per-neuron: initial (always loaded) vs on-demand
                // (referenced in a manifest, pulled in via <load_neuron>).
                let initial: Vec<&crate::domain::neuron::Neuron> = enabled.iter()
                    .filter(|n| !self.on_demand_neurons.contains(&n.name))
                    .copied().collect();
                let on_demand: Vec<&crate::domain::neuron::Neuron> = enabled.iter()
                    .filter(|n| self.on_demand_neurons.contains(&n.name))
                    .copied().collect();
                let ctx = crate::domain::neuron::build_tool_context(&initial);
                if !ctx.is_empty() { parts.push(ctx); }
                if !on_demand.is_empty() {
                    let mut manifest = "## On-demand neurons\nNot currently loaded. Request one with `<load_neuron>Name</load_neuron>` when you need it:\n".to_string();
                    for n in &on_demand {
                        let desc = if n.description.is_empty() { String::new() } else { format!(" — {}", n.description) };
                        manifest.push_str(&format!("- **{}**{}\n", n.name, desc));
                    }
                    parts.push(manifest);
                }
            }
            _ => {
                let ctx = crate::domain::neuron::build_tool_context(&enabled);
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
            crate::adapter::ollama::warmup(&base_url, model, system, num_ctx, keep_alive);
            let _ = tx.send(());
        });
    }

    pub fn export_chat(&mut self) {
        if self.messages.is_empty() { return; }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("cognilite_chat_{now}.json");
        let path = self.working_dir.join(&filename);
        match serde_json::to_string_pretty(&self.messages) {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(_) => {
                    self.status_notice = Some((std::time::Instant::now(), format!("saved: {filename}")));
                }
                Err(e) => {
                    self.status_notice = Some((std::time::Instant::now(), format!("export failed: {e}")));
                }
            },
            Err(e) => {
                self.status_notice = Some((std::time::Instant::now(), format!("serialize failed: {e}")));
            }
        }
    }

    pub fn load_chat(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str::<Vec<Message>>(&json) {
                Ok(messages) => {
                    self.messages = messages;
                    self.scroll = u16::MAX;
                    self.auto_scroll = true;
                    self.injected_neurons.clear();
                    let name = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("chat");
                    self.status_notice = Some((std::time::Instant::now(), format!("loaded: {name}")));
                }
                Err(e) => {
                    self.status_notice = Some((std::time::Instant::now(), format!("load failed: {e}")));
                }
            },
            Err(e) => {
                self.status_notice = Some((std::time::Instant::now(), format!("read failed: {e}")));
            }
        }
    }

}

// ── file attachment resolution ────────────────────────────────────────────────

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

///// Splits `@path` tokens out of user input.
/// Returns (text_without_at_refs, vec_of_paths).
/// Used in WS client mode where file reading happens on the server.
pub fn split_at_paths(raw: &str) -> (String, Vec<String>) {
    let mut paths = Vec::new();
    let mut words = Vec::new();
    for word in raw.split_whitespace() {
        if let Some(p) = word.strip_prefix('@') {
            if !p.is_empty() { paths.push(p.to_string()); continue; }
        }
        words.push(word);
    }
    (words.join(" "), paths)
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

impl App {
    /// Assemble the (system, history) tuple that would be sent to /api/chat,
    /// matching the filter used in start_stream. Used by the raw-prompt builder.
    pub fn build_api_history(&self) -> (String, Vec<(String, String)>) {
        let system = self.full_system_prompt();
        let history: Vec<(String, String)> = self.messages
            .iter()
            .filter(|m| !m.llm_content.is_empty() || m.role == Role::User)
            .map(|m| (m.role.to_api_str().to_string(), m.llm_content.clone()))
            .collect();
        (system, history)
    }

}
