use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Mutex};

use crate::app::{
    App, AskKind, Attachment, AttachmentKind, Message, NeuronMode, Role, StreamState,
    extract_ask_tag, extract_load_neuron_tag, extract_mood_tag, extract_patch_tag,
    extract_preview_tag, extract_tool_call, build_runtime_context, RuntimeMode,
};
use crate::headless::safe_print_boundary;

// ── Room registry ─────────────────────────────────────────────────────────

pub struct RoomState {
    pub messages: Vec<crate::app::Message>,
    pub version:  u64,
    // live token stream from the current turn (cleared when turn completes)
    pub live_tokens:        String,
    pub live_token_version: u64,
    pub live_user:          String, // username of whoever is currently generating
}

pub type SharedRoom    = Arc<Mutex<RoomState>>;
pub type RoomRegistry  = Arc<Mutex<HashMap<String, SharedRoom>>>;

pub fn new_room_registry() -> RoomRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn new_uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
    let c = CTR.fetch_add(1, Ordering::Relaxed);
    let p = std::process::id() as u64;
    let a = t ^ (p << 32) ^ c.wrapping_mul(0x9e3779b97f4a7c15);
    let b = t.wrapping_mul(0x6c62272e07bb0142) ^ c.wrapping_add(p);
    format!("{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        ((a >> 16) & 0xffff) as u16,
        (a & 0xfff) as u16,
        (0x8000 | (b >> 48 & 0x3fff)) as u16,
        b & 0x0000_ffff_ffff_ffff)
}

// ── SHA-1 ─────────────────────────────────────────────────────────────────

fn sha1(data: &[u8]) -> [u8; 20] {
    let ml = data.len() as u64 * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&ml.to_be_bytes());

    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i*4..i*4+4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19  => ((b & c) | (!b & d),            0x5A827999u32),
                20..=39 => (b ^ c ^ d,                     0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d),  0x8F1BBCDC),
                _       => (b ^ c ^ d,                     0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = t;
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 { out[i*4..i*4+4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

// ── Base64 ────────────────────────────────────────────────────────────────

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let b = [c[0] as usize, if c.len()>1 {c[1] as usize} else {0}, if c.len()>2 {c[2] as usize} else {0}];
        out.push(T[b[0]>>2] as char);
        out.push(T[((b[0]&3)<<4)|(b[1]>>4)] as char);
        out.push(if c.len()>1 { T[((b[1]&0xf)<<2)|(b[2]>>6)] as char } else { '=' });
        out.push(if c.len()>2 { T[b[2]&0x3f] as char } else { '=' });
    }
    out
}

// ── WebSocket handshake ───────────────────────────────────────────────────

const WS_MAGIC: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn handshake(stream: &mut TcpStream, key: &str) -> bool {
    let accept = b64(&sha1(format!("{key}{WS_MAGIC}").as_bytes()));
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    stream.write_all(resp.as_bytes()).is_ok()
}

// ── Frame I/O ─────────────────────────────────────────────────────────────

const OP_TEXT:  u8 = 1;
const OP_CLOSE: u8 = 8;
const OP_PING:  u8 = 9;
const OP_PONG:  u8 = 10;

/// Read one WebSocket frame. Returns (opcode, payload). Returns None on connection error.
fn read_frame(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 2];
    stream.read_exact(&mut hdr).ok()?;
    let opcode = hdr[0] & 0x0f;
    let masked = (hdr[1] & 0x80) != 0;
    let mut len = (hdr[1] & 0x7f) as usize;

    if len == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext).ok()?;
        len = u16::from_be_bytes(ext) as usize;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        stream.read_exact(&mut ext).ok()?;
        len = u64::from_be_bytes(ext) as usize;
    }

    let mask = if masked {
        let mut m = [0u8; 4];
        stream.read_exact(&mut m).ok()?;
        Some(m)
    } else { None };

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).ok()?;
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() { *b ^= m[i % 4]; }
    }
    Some((opcode, payload))
}

fn write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> bool {
    let mut frame = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126    => frame.push(n as u8),
        n if n < 65536  => { frame.push(126); frame.extend_from_slice(&(n as u16).to_be_bytes()); }
        n               => { frame.push(127); frame.extend_from_slice(&(n as u64).to_be_bytes()); }
    }
    frame.extend_from_slice(payload);
    stream.write_all(&frame).is_ok()
}

fn send_json(stream: &mut TcpStream, val: serde_json::Value) -> bool {
    write_frame(stream, OP_TEXT, val.to_string().as_bytes())
}

enum FrameResult { Frame(u8, Vec<u8>), Timeout, Disconnected }

fn read_frame_timeout(stream: &mut TcpStream) -> FrameResult {
    let mut hdr = [0u8; 2];
    match stream.read_exact(&mut hdr) {
        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return FrameResult::Timeout,
        Err(_) => return FrameResult::Disconnected,
        Ok(_) => {}
    }
    let opcode = hdr[0] & 0x0f;
    let masked = (hdr[1] & 0x80) != 0;
    let mut len = (hdr[1] & 0x7f) as usize;
    if len == 126 {
        let mut ext = [0u8; 2];
        if stream.read_exact(&mut ext).is_err() { return FrameResult::Disconnected; }
        len = u16::from_be_bytes(ext) as usize;
    } else if len == 127 {
        let mut ext = [0u8; 8];
        if stream.read_exact(&mut ext).is_err() { return FrameResult::Disconnected; }
        len = u64::from_be_bytes(ext) as usize;
    }
    let mask = if masked {
        let mut m = [0u8; 4];
        if stream.read_exact(&mut m).is_err() { return FrameResult::Disconnected; }
        Some(m)
    } else { None };
    let mut payload = vec![0u8; len];
    if stream.read_exact(&mut payload).is_err() { return FrameResult::Disconnected; }
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() { *b ^= m[i % 4]; }
    }
    FrameResult::Frame(opcode, payload)
}

// ── Query string parser ───────────────────────────────────────────────────

pub fn parse_query(path: &str) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let qs = path.splitn(2, '?').nth(1).unwrap_or("");
    for pair in qs.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = if let Some(i) = pair.find('=') {
            (&pair[..i], &pair[i+1..])
        } else {
            (pair, "")
        };
        let k = url_decode(k);
        let v = url_decode(v);
        map.entry(k).or_default().push(v);
    }
    map
}

fn url_decode(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '+' { out.push(' '); }
        else if c == '%' {
            let h1 = chars.next().and_then(|c| c.to_digit(16));
            let h2 = chars.next().and_then(|c| c.to_digit(16));
            if let (Some(h1), Some(h2)) = (h1, h2) {
                out.push(char::from_u32(h1 * 16 + h2).unwrap_or(c));
            }
        } else { out.push(c); }
    }
    out
}

// ── Session ───────────────────────────────────────────────────────────────

pub struct SessionConfig {
    pub model:        Option<String>,
    pub neuron_mode:  Option<NeuronMode>,
    pub preset:       Option<String>,
    pub no_neurons:   Vec<String>,
    pub thinking:     bool,
    pub yes:          bool,
    pub thinking_srv: bool, // also show thinking on server stderr
    pub tui_client:   bool, // client is the cognilite TUI (--remote mode)
    pub username:     String,
}

impl SessionConfig {
    pub fn from_query(q: &HashMap<String, Vec<String>>, thinking_srv: bool) -> Self {
        let get = |k: &str| q.get(k).and_then(|v| v.first()).map(|s| s.as_str());
        Self {
            model:       get("model").map(str::to_string),
            neuron_mode: get("neuron_mode").map(|s| NeuronMode::from_str(s)),
            preset:      get("preset").map(str::to_string),
            no_neurons:  q.get("no_neuron").cloned().unwrap_or_default(),
            thinking:    get("thinking").map(|s| s == "true" || s == "1").unwrap_or(false),
            yes:         get("yes").map(|s| s == "true" || s == "1").unwrap_or(false),
            thinking_srv,
            tui_client:  get("client").map(|s| s == "tui").unwrap_or(false),
            username:    get("username").map(str::to_string)
                             .unwrap_or_else(crate::app::default_username),
        }
    }
}

pub fn run_session(mut stream: TcpStream, base_url: &str, cfg: SessionConfig, room: SharedRoom, room_id: String, silent: bool) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());

    let mut app = App::new(base_url.to_string());
    app.username = cfg.username.clone();
    app.room_id  = Some(room_id.clone());

    // apply config
    if let Some(mode) = cfg.neuron_mode { app.neuron_mode = mode; }
    if let Some(ref preset) = cfg.preset {
        app.neuron_mode = NeuronMode::Presets;
        app.active_preset = Some(preset.clone());
    }
    for name in &cfg.no_neurons { app.disabled_neurons.insert(name.clone()); }
    app.warmup = false;

    // load models
    match crate::ollama::list_models(base_url) {
        Ok(models) => app.models = models,
        Err(e) => {
            send_json(&mut stream, serde_json::json!({"type":"error","content": format!("cannot list models: {e}")}));
            return;
        }
    }
    let model_name: String = if cfg.tui_client && cfg.model.is_none() {
        // send model list and wait for the client to choose
        let entries: Vec<serde_json::Value> = app.models.iter().map(|e| serde_json::json!({
            "name": e.name,
            "parameter_size": e.parameter_size,
            "quantization_level": e.quantization_level,
            "size_bytes": e.size_bytes,
        })).collect();
        if app.models.is_empty() {
            send_json(&mut stream, serde_json::json!({"type":"error","content":"no models available"}));
            return;
        }
        if !send_json(&mut stream, serde_json::json!({"type":"models","entries":entries})) { return; }
        // blocking wait for select_model frame
        loop {
            match read_frame(&mut stream) {
                None => return,
                Some((OP_CLOSE, _)) => { write_frame(&mut stream, OP_CLOSE, &[]); return; }
                Some((OP_PING, p))  => { write_frame(&mut stream, OP_PONG, &p); }
                Some((OP_TEXT, payload)) => {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&payload) {
                        if v.get("type").and_then(|t| t.as_str()) == Some("select_model") {
                            if let Some(name) = v.get("model").and_then(|m| m.as_str()) {
                                if app.models.iter().any(|e| e.name == name) {
                                    break name.to_string();
                                }
                                send_json(&mut stream, serde_json::json!({"type":"error","content":format!("model '{name}' not found")}));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    } else {
        match cfg.model {
            Some(ref m) => {
                if app.models.iter().any(|e| e.name == *m) { m.clone() }
                else {
                    send_json(&mut stream, serde_json::json!({"type":"error","content": format!("model '{m}' not found")}));
                    return;
                }
            }
            None => match app.models.first() {
                Some(e) => e.name.clone(),
                None => {
                    send_json(&mut stream, serde_json::json!({"type":"error","content":"no models available"}));
                    return;
                }
            }
        }
    };

    app.selected_model = Some(model_name.clone());
    app.context_length = crate::ollama::fetch_context_length(base_url, &model_name);
    app.stream_state = StreamState::Idle;
    app.screen = crate::app::Screen::Chat;
    app.runtime_context = build_runtime_context(&model_name, app.context_length,
        if cfg.tui_client {
            RuntimeMode::RemoteTui { auto_yes: cfg.yes }
        } else {
            RuntimeMode::WebSocket { auto_yes: cfg.yes }
        });

    // warmup — pre-fill KV cache with system prompt before accepting the first message
    app.warmup = true;
    app.trigger_warmup();
    if app.warmup_rx.is_some() {
        if !send_json(&mut stream, serde_json::json!({"type":"warmup_start"})) { return; }
        loop {
            match app.warmup_rx.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(())) => { app.warmup_rx = None; break; }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => { app.warmup_rx = None; break; }
                _ => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
        if !send_json(&mut stream, serde_json::json!({"type":"warmup_done"})) { return; }
    }

    let ctx_str = app.context_length.map(|n| format!("{}k", n/1024)).unwrap_or_else(|| "?".into());
    if !silent { eprintln!("[ws] {peer} connected — {model_name} (room {room_id})"); }

    // load room history and push join presence event
    let (mut known_version, mut known_token_version, mut sent_token_len) = {
        let mut r = room.lock().unwrap();
        app.messages = r.messages.clone();
        let join_msg = crate::app::Message {
            role: crate::app::Role::Tool,
            content: format!("**{}** se unió a la sala.", cfg.username),
            llm_content: format!("{} joined the room.", cfg.username),
            images: vec![],
            attachments: vec![],
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: Some("⬡ Sala".to_string()),
        };
        r.messages.push(join_msg);
        r.version += 1;
        (r.version, r.live_token_version, r.live_tokens.len())
    };

    // send history snapshot to client before `connected` frame
    if !app.messages.is_empty() {
        let history: Vec<serde_json::Value> = app.messages.iter().map(|m| {
            serde_json::json!({
                "role":    m.role.to_api_str(),
                "content": m.content,
                "user":    m.tool_call.as_deref().unwrap_or(""),
            })
        }).collect();
        if !send_json(&mut stream, serde_json::json!({"type":"history","messages":history})) { return; }
    }

    if !send_json(&mut stream, serde_json::json!({
        "type": "connected",
        "model": model_name,
        "ctx": ctx_str,
        "room_id": room_id,
        "username": cfg.username,
    })) { return; }

    // use 100ms read timeout so we can poll room version between client frames
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(100)));

    // outer loop: one frame per interaction
    loop {
        // ── poll room for live tokens from local TUI ─────────────────────
        {
            let r = room.lock().unwrap();
            if r.live_token_version != known_token_version {
                let new_part = &r.live_tokens[sent_token_len.min(r.live_tokens.len())..];
                if !new_part.is_empty() {
                    if !send_json(&mut stream, serde_json::json!({
                        "type": "room_token",
                        "user": r.live_user,
                        "content": new_part,
                    })) { break; }
                }
                sent_token_len = r.live_tokens.len();
                known_token_version = r.live_token_version;
            }
        }

        // ── poll room for completed messages from other users ─────────────
        let current_version = room.lock().unwrap().version;
        if current_version != known_version {
            let new_msgs: Vec<serde_json::Value> = {
                let r = room.lock().unwrap();
                r.messages[app.messages.len()..].iter().map(|m| serde_json::json!({
                    "role":    m.role.to_api_str(),
                    "content": m.content,
                    "user":    m.tool_call.as_deref().unwrap_or(""),
                })).collect()
            };
            if !new_msgs.is_empty() {
                if !send_json(&mut stream, serde_json::json!({"type":"room_update","messages":new_msgs})) { break; }
                // sync local messages
                let r = room.lock().unwrap();
                app.messages = r.messages.clone();
            }
            known_version = current_version;
            let r = room.lock().unwrap();
            known_token_version = r.live_token_version;
            sent_token_len = r.live_tokens.len();
        }

        let (opcode, payload) = match read_frame_timeout(&mut stream) {
            FrameResult::Frame(op, p) => (op, p),
            FrameResult::Timeout      => continue,
            FrameResult::Disconnected => break,
        };

        match opcode {
            OP_CLOSE => { write_frame(&mut stream, OP_CLOSE, &[]); break; }
            OP_PING  => { write_frame(&mut stream, OP_PONG, &payload); continue; }
            OP_TEXT  => {}
            _ => continue,
        }

        let Ok(val) = serde_json::from_slice::<serde_json::Value>(&payload) else { continue };

        match val.get("type").and_then(|v| v.as_str()) {
            Some("message") => {
                let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                if content.is_empty() { continue; }

                // check pinned files for changes (mtime) — same as TUI main loop
                app.check_pinned_files();

                // append @path refs from optional "attach" array so resolve_attachments() picks them up
                let input = match val.get("attach").and_then(|v| v.as_array()) {
                    Some(arr) => {
                        let refs = arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(|p| format!("@{p}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if refs.is_empty() { content } else { format!("{content} {refs}") }
                    }
                    None => content,
                };

                // delegate to send_message() to get pinned-file diffs and @path handling for free
                app.input = input;
                app.cursor_pos = app.input.len();
                // tag the user message with the username
                app.send_message();
                if let Some(last) = app.messages.iter_mut().rev().find(|m| m.role == Role::User) {
                    last.tool_call = Some(cfg.username.clone());
                }

                if !stream_loop(&mut app, &mut stream, cfg.thinking, cfg.thinking_srv, cfg.yes) {
                    break;
                }
                app.stream_state = StreamState::Idle;

                // push new messages (user + assistant + any tools) to shared room
                {
                    let mut r = room.lock().unwrap();
                    r.messages = app.messages.clone();
                    r.version += 1;
                    known_version = r.version;
                }
            }
            Some("pin") => {
                let path = val.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if path.is_empty() { continue; }
                app.pin_file(path.clone());
                // re-warm after pinning since system prompt changed
                app.trigger_warmup();
                send_json(&mut stream, serde_json::json!({"type":"pinned","path":path}));
            }
            Some("unpin") => {
                let path = val.get("path").and_then(|v| v.as_str()).unwrap_or("");
                app.unpin_file(path);
                app.trigger_warmup();
                send_json(&mut stream, serde_json::json!({"type":"unpinned","path":path}));
            }
            Some("ls") => {
                let rel = val.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                let dir = app.working_dir.join(rel);
                let mut entries: Vec<serde_json::Value> = vec![];
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    let mut list: Vec<_> = rd.flatten().collect();
                    list.sort_by_key(|e| e.file_name());
                    for e in list {
                        let name = e.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') { continue; }
                        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        entries.push(serde_json::json!({"name": name, "is_dir": is_dir}));
                    }
                }
                send_json(&mut stream, serde_json::json!({"type":"ls_result","path":rel,"entries":entries}));
            }
            Some("ping") => { send_json(&mut stream, serde_json::json!({"type":"pong"})); }
            _ => {}
        }
    }

    let _ = stream.set_read_timeout(None);
    if !silent { eprintln!("[ws] {peer} disconnected (room {room_id})"); }
    // push leave presence event to room
    {
        let mut r = room.lock().unwrap();
        let leave_msg = crate::app::Message {
            role: crate::app::Role::Tool,
            content: format!("**{}** salió de la sala.", cfg.username),
            llm_content: format!("{} left the room.", cfg.username),
            images: vec![],
            attachments: vec![],
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: Some("⬡ Sala".to_string()),
        };
        r.messages.push(leave_msg);
        r.version += 1;
    }
}

/// Runs the stream loop for one assistant turn. Returns false if the WebSocket connection dropped.
fn stream_loop(app: &mut App, stream: &mut TcpStream, thinking: bool, thinking_srv: bool, auto_yes: bool) -> bool {
    'outer: loop {
        let rx = match app.stream_rx.take() {
            Some(r) => r,
            None => {
                send_json(stream, serde_json::json!({"type":"done"}));
                return true;
            }
        };

        let mut printed_up_to: usize = 0;
        let mut thinking_open = false;

        loop {
            let chunk = match rx.try_recv() {
                Ok(c)  => c,
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                Err(TryRecvError::Disconnected) => {
                    send_json(stream, serde_json::json!({"type":"done"}));
                    return true;
                }
            };

            if let Some(e) = chunk.error {
                return send_json(stream, serde_json::json!({"type":"error","content": e}));
            }

            if let Some(ref msg) = chunk.message {
                if let Some(last) = app.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.content.push_str(&msg.content);
                        last.llm_content.push_str(&msg.content);
                        if let Some(ref t) = msg.thinking {
                            if !t.is_empty() {
                                if !thinking_open {
                                    if thinking { if !send_json(stream, serde_json::json!({"type":"thinking_start"})) { return false; } }
                                    if thinking_srv { eprint!("[thinking]\n"); }
                                    thinking_open = true;
                                }
                                if thinking { if !send_json(stream, serde_json::json!({"type":"thinking","content":t})) { return false; } }
                                if thinking_srv { eprint!("{t}"); }
                                last.thinking.push_str(t);
                            }
                        }
                    }
                }

                // flush printable content as tokens
                if let Some(last) = app.messages.last() {
                    if last.role == Role::Assistant {
                        let safe = safe_print_boundary(&last.content, printed_up_to);
                        if safe > printed_up_to {
                            if thinking_open {
                                if thinking { send_json(stream, serde_json::json!({"type":"thinking_end"})); }
                                if thinking_srv { eprintln!("\n[/thinking]"); }
                                thinking_open = false;
                            }
                            let token = &last.content[printed_up_to..safe];
                            if !send_json(stream, serde_json::json!({"type":"token","content":token})) { return false; }
                            printed_up_to = safe;
                        }
                    }
                }

                // ── <tool> ──────────────────────────────────────────────
                let maybe_call = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_tool_call(&m.content))
                    .map(|s| s.to_string());
                if let Some(call) = maybe_call {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8)
                            .or_else(|| last.content.rfind("</thought>").map(|i| i+10))
                            .unwrap_or(0);
                        if let Some(pos) = last.content[sf..].find("<tool>").map(|p| sf+p) {
                            last.content.truncate(pos);
                            last.content = last.content.trim_end().to_string();
                        }
                    }
                    eprintln!("[ws tool: {call}]");
                    app.handle_tool_call(&call);
                    // send after execution so the frame includes the result
                    let (label, result) = app.messages.last()
                        .filter(|m| m.role == Role::Tool)
                        .map(|m| (m.tool_call.clone().unwrap_or_else(|| call.clone()), m.content.as_str()))
                        .map(|(l, r)| (l, r.to_string()))
                        .unwrap_or_else(|| (call.clone(), String::new()));
                    if !send_json(stream, serde_json::json!({
                        "type": "tool", "command": &call, "label": label, "result": result
                    })) { return false; }
                    continue 'outer;
                }

                // ── <load_neuron> ───────────────────────────────────────
                let load_name = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_load_neuron_tag(&m.content));
                if let Some(name) = load_name {
                    if !app.injected_neurons.contains(&name) {
                        let neuron_content = app.neurons.iter()
                            .find(|n| n.name.eq_ignore_ascii_case(&name))
                            .map(|n| format!("## Neuron: {}\n\n{}", n.name, n.system_prompt));
                        if let Some(content) = neuron_content {
                            if let Some(last) = app.messages.last_mut() {
                                let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                                if let Some(p) = last.content[sf..].find("<load_neuron>") {
                                    let abs = sf + p;
                                    if let Some(end) = last.content[abs..].find("</load_neuron>") {
                                        let tag_end = abs + end + 14;
                                        let before = last.content[..abs].trim_end().to_string();
                                        let after  = last.content[tag_end..].to_string();
                                        last.content = before + &after;
                                    }
                                }
                            }
                            app.injected_neurons.insert(name.clone());
                            if !send_json(stream, serde_json::json!({"type":"load_neuron","name":&name})) { return false; }
                            eprintln!("[ws load_neuron: {name}]");
                            let size = content.len();
                            app.messages.push(Message {
                                role: Role::Tool,
                                content: content.clone(),
                                llm_content: format!("Neuron loaded:\n{content}"),
                                images: vec![],
                                attachments: vec![Attachment {
                                    filename: name.clone(),
                                    path: PathBuf::new(),
                                    kind: AttachmentKind::Text,
                                    size,
                                }],
                                thinking: String::new(),
                                thinking_secs: None,
                                stats: None,
                                tool_call: Some(format!("Neuron \u{203a} {name}")),
                            });
                            app.start_stream();
                            continue 'outer;
                        }
                    }
                }

                // ── <ask> ───────────────────────────────────────────────
                let ask_info = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_ask_tag(&m.content));
                if let Some((kind, question)) = ask_info {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                        if let Some(p) = last.content[sf..].find("<ask") {
                            last.content.truncate(sf + p);
                            last.content = last.content.trim_end().to_string();
                        }
                    }

                    let response = if auto_yes {
                        match &kind {
                            AskKind::Confirm       => "Yes".to_string(),
                            AskKind::Choice(opts)  => opts.first().cloned().unwrap_or_default(),
                            AskKind::Text          => String::new(),
                        }
                    } else {
                        // send ask to client, wait for ask_response frame
                        let ask_frame = match &kind {
                            AskKind::Confirm => serde_json::json!({
                                "type": "ask", "kind": "confirm", "question": &question
                            }),
                            AskKind::Choice(opts) => serde_json::json!({
                                "type": "ask", "kind": "choice", "question": &question, "options": opts
                            }),
                            AskKind::Text => serde_json::json!({
                                "type": "ask", "kind": "text", "question": &question
                            }),
                        };
                        if !send_json(stream, ask_frame) { return false; }
                        // blocking wait for response
                        loop {
                            match read_frame(stream) {
                                None => return false,
                                Some((OP_CLOSE, _)) => {
                                    write_frame(stream, OP_CLOSE, &[]);
                                    return false;
                                }
                                Some((OP_PING, p)) => { write_frame(stream, OP_PONG, &p); }
                                Some((OP_TEXT, p)) => {
                                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&p) {
                                        if v.get("type").and_then(|t| t.as_str()) == Some("ask_response") {
                                            break v.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    };

                    app.ask = Some(crate::app::InputRequest { question, kind });
                    app.submit_ask(response);
                    continue 'outer;
                }

                // ── <patch> ─────────────────────────────────────────────
                let patch = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_patch_tag(&m.content));
                if let Some(diff) = patch {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                        if let Some(p) = last.content[sf..].find("<patch>") {
                            let abs = sf + p;
                            if let Some(end) = last.content[abs..].find("</patch>") {
                                let tag_end = abs + end + 8;
                                let before = last.content[..abs].trim_end().to_string();
                                let after  = last.content[tag_end..].to_string();
                                let rendered = format!("```diff\n{}\n```", diff.trim());
                                last.content = if before.is_empty() { rendered + &after }
                                               else { format!("{before}\n{rendered}{after}") };
                            }
                        }
                    }

                    let confirmed = if auto_yes {
                        true
                    } else {
                        if !send_json(stream, serde_json::json!({
                            "type": "patch", "diff": &diff, "question": "Apply this patch?"
                        })) { return false; }
                        loop {
                            match read_frame(stream) {
                                None => return false,
                                Some((OP_CLOSE, _)) => { write_frame(stream, OP_CLOSE, &[]); return false; }
                                Some((OP_PING, p)) => { write_frame(stream, OP_PONG, &p); }
                                Some((OP_TEXT, p)) => {
                                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&p) {
                                        if v.get("type").and_then(|t| t.as_str()) == Some("ask_response") {
                                            break matches!(v.get("content").and_then(|c| c.as_str()), Some("Yes" | "yes" | "y"));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    };

                    app.pending_patch = Some(diff);
                    app.ask = Some(crate::app::InputRequest {
                        question: "Apply this patch?".to_string(),
                        kind: AskKind::Confirm,
                    });
                    app.submit_ask(if confirmed { "Yes".to_string() } else { "No".to_string() });
                    continue 'outer;
                }

                // ── <mood> ──────────────────────────────────────────────
                let mood = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_mood_tag(&m.content));
                if let Some(emoji) = mood {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                        if let Some(p) = last.content[sf..].find("<mood>") {
                            let abs = sf + p;
                            if let Some(end) = last.content[abs..].find("</mood>") {
                                let tag_end = abs + end + 7;
                                let before = last.content[..abs].trim_end().to_string();
                                last.content = before + &last.content[tag_end..];
                            }
                        }
                    }
                    app.current_mood = Some(emoji.clone());
                    send_json(stream, serde_json::json!({"type":"mood","emoji":emoji}));
                }

                // ── <preview path="..."/> ───────────────────────────────
                let preview_path = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_preview_tag(&m.content));
                if let Some(rel_path) = preview_path {
                    if let Some(last) = app.messages.last_mut() {
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
                    let file_path = app.working_dir.join(&rel_path);
                    let content = std::fs::read_to_string(&file_path)
                        .unwrap_or_else(|e| format!("(cannot read {rel_path}: {e})"));
                    send_json(stream, serde_json::json!({
                        "type": "file_preview",
                        "path": rel_path,
                        "content": content
                    }));
                }
            }

            if chunk.done {
                if thinking_open {
                    if thinking { send_json(stream, serde_json::json!({"type":"thinking_end"})); }
                    if thinking_srv { eprintln!("\n[/thinking]"); }
                }
                let stats = if let (Some(pt), Some(et), Some(ed)) =
                    (chunk.prompt_eval_count, chunk.eval_count, chunk.eval_duration)
                {
                    let tps = et as f64 / (ed as f64 / 1_000_000_000.0);
                    serde_json::json!({"tps": tps, "tokens": et, "prompt_eval": pt})
                } else {
                    serde_json::json!({})
                };
                return send_json(stream, serde_json::json!({"type":"done","stats":stats}));
            }
        }
    }
}
