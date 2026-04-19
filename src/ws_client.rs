use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;

// ── Frame types ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum WsClientFrame {
    Connected { model: String, #[allow(dead_code)] ctx: String },
    Token(String),
    ThinkingStart,
    Thinking(String),
    ThinkingEnd,
    /// Full tool event: command, display label, and result (sent after server executes)
    Tool { command: String, label: String, result: String },
    LoadNeuron(String),
    Ask { kind: String, question: String, options: Vec<String> },
    Patch(String),
    Mood(String),
    FilePreview { path: String, content: String },
    Models { entries: Vec<crate::ollama::ModelEntry> },
    LsResult { path: String, entries: Vec<(String, bool)> }, // (name, is_dir)
    Done { tps: f64, tokens: u64, prompt_eval: u64 },
    WarmupStart,
    WarmupDone,
    Error(String),
    Disconnected,
    Unknown,
}

// ── Wire helpers ──────────────────────────────────────────────────────────

const OP_TEXT:  u8 = 1;
const OP_CLOSE: u8 = 8;
const OP_PING:  u8 = 9;

// Client frames MUST be masked (RFC 6455 §5.3)
const MASK: [u8; 4] = [0x37, 0x4f, 0x2a, 0x8c];

pub fn write_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> bool {
    let mut frame = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126   => frame.push(0x80 | n as u8),
        n if n < 65536 => { frame.push(0x80 | 126); frame.extend_from_slice(&(n as u16).to_be_bytes()); }
        n              => { frame.push(0x80 | 127); frame.extend_from_slice(&(n as u64).to_be_bytes()); }
    }
    frame.extend_from_slice(&MASK);
    for (i, b) in payload.iter().enumerate() { frame.push(b ^ MASK[i % 4]); }
    stream.write_all(&frame).is_ok()
}

pub fn send_json(stream: &mut TcpStream, val: serde_json::Value) -> bool {
    write_frame(stream, OP_TEXT, val.to_string().as_bytes())
}

fn read_frame(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut hdr = [0u8; 2];
    stream.read_exact(&mut hdr).ok()?;
    let opcode = hdr[0] & 0x0f;
    let masked  = (hdr[1] & 0x80) != 0;
    let mut len = (hdr[1] & 0x7f) as usize;
    if len == 126 {
        let mut ext = [0u8; 2]; stream.read_exact(&mut ext).ok()?;
        len = u16::from_be_bytes(ext) as usize;
    } else if len == 127 {
        let mut ext = [0u8; 8]; stream.read_exact(&mut ext).ok()?;
        len = u64::from_be_bytes(ext) as usize;
    }
    let mask = if masked { let mut m = [0u8; 4]; stream.read_exact(&mut m).ok()?; Some(m) } else { None };
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).ok()?;
    if let Some(m) = mask { for (i, b) in payload.iter_mut().enumerate() { *b ^= m[i % 4]; } }
    Some((opcode, payload))
}

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let b = [c[0] as usize, *c.get(1).unwrap_or(&0) as usize, *c.get(2).unwrap_or(&0) as usize];
        out.push(T[b[0] >> 2] as char);
        out.push(T[((b[0] & 3) << 4) | (b[1] >> 4)] as char);
        out.push(if c.len() > 1 { T[((b[1] & 0xf) << 2) | (b[2] >> 6)] as char } else { '=' });
        out.push(if c.len() > 2 { T[b[2] & 0x3f] as char } else { '=' });
    }
    out
}

// ── Connection ────────────────────────────────────────────────────────────

/// Connect to a cognilite WebSocket server.
/// url must start with "ws://", e.g. "ws://host:8765/ws?model=qwen2.5:7b"
pub fn connect(url: &str) -> Result<(TcpStream, mpsc::Receiver<WsClientFrame>), String> {
    let rest = url.strip_prefix("ws://").ok_or("URL must start with ws://")?;
    let (hostport, path_rest) = rest.split_once('/').unwrap_or((rest, "ws"));
    let path = format!("/{path_rest}");

    let mut stream = TcpStream::connect(hostport)
        .map_err(|e| format!("connect {hostport}: {e}"))?;

    let key = b64(b"cognilite-remote-00");
    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {hostport}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).map_err(|e| format!("write upgrade: {e}"))?;

    // read HTTP response until end of headers
    let mut resp = Vec::new();
    loop {
        let mut b = [0u8; 1];
        stream.read_exact(&mut b).map_err(|e| format!("read upgrade: {e}"))?;
        resp.push(b[0]);
        if resp.ends_with(b"\r\n\r\n") { break; }
        if resp.len() > 8192 { return Err("HTTP response too large".into()); }
    }
    let resp_str = String::from_utf8_lossy(&resp);
    if !resp_str.contains("101") {
        return Err(format!("server rejected upgrade: {}",
            resp_str.lines().next().unwrap_or("")));
    }

    let mut reader = stream.try_clone().map_err(|e| format!("clone stream: {e}"))?;
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        loop {
            match read_frame(&mut reader) {
                None | Some((OP_CLOSE, _)) => {
                    let _ = tx.send(WsClientFrame::Disconnected);
                    break;
                }
                Some((OP_TEXT, payload)) => {
                    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&payload) {
                        let frame = parse_frame(val);
                        let done = matches!(&frame, WsClientFrame::Disconnected);
                        let _ = tx.send(frame);
                        if done { break; }
                    }
                }
                Some((OP_PING, _)) => {} // server doesn't ping us in practice; ignore
                _ => {}
            }
        }
    });

    Ok((stream, rx))
}

// ── Frame parsing ─────────────────────────────────────────────────────────

fn parse_frame(val: serde_json::Value) -> WsClientFrame {
    let s = |k: &str| val.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match val.get("type").and_then(|v| v.as_str()) {
        Some("connected") => WsClientFrame::Connected { model: s("model"), ctx: s("ctx") },

        Some("token")         => WsClientFrame::Token(s("content")),
        Some("thinking_start") => WsClientFrame::ThinkingStart,
        Some("thinking")      => WsClientFrame::Thinking(s("content")),
        Some("thinking_end")  => WsClientFrame::ThinkingEnd,

        Some("tool") => WsClientFrame::Tool {
            // accept both new "command" key and old "content" key for backwards compat
            command: val.get("command").and_then(|v| v.as_str())
                .or_else(|| val.get("content").and_then(|v| v.as_str()))
                .unwrap_or("").to_string(),
            label:  s("label"),
            result: s("result"),
        },

        Some("load_neuron") => WsClientFrame::LoadNeuron(s("name")),

        Some("ask") => WsClientFrame::Ask {
            kind:     s("kind"),
            question: s("question"),
            options:  val["options"].as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).map(str::to_string).collect())
                .unwrap_or_default(),
        },

        Some("patch")  => WsClientFrame::Patch(s("diff")),
        Some("mood")   => WsClientFrame::Mood(s("emoji")),
        Some("file_preview") => WsClientFrame::FilePreview {
            path:    s("path"),
            content: s("content"),
        },
        Some("models") => WsClientFrame::Models {
            entries: val["entries"].as_array().map(|arr| {
                arr.iter().map(|e| crate::ollama::ModelEntry {
                    name: e["name"].as_str().unwrap_or("").to_string(),
                    parameter_size: e["parameter_size"].as_str().map(String::from),
                    quantization_level: e["quantization_level"].as_str().map(String::from),
                    size_bytes: e["size_bytes"].as_u64(),
                }).collect()
            }).unwrap_or_default(),
        },
        Some("ls_result") => WsClientFrame::LsResult {
            path: s("path"),
            entries: val["entries"].as_array().map(|arr| {
                arr.iter().filter_map(|e| {
                    let name   = e["name"].as_str()?.to_string();
                    let is_dir = e["is_dir"].as_bool().unwrap_or(false);
                    Some((name, is_dir))
                }).collect()
            }).unwrap_or_default(),
        },

        Some("done") => {
            let st = val.get("stats");
            WsClientFrame::Done {
                tps:         st.and_then(|s| s["tps"].as_f64()).unwrap_or(0.0),
                tokens:      st.and_then(|s| s["tokens"].as_u64()).unwrap_or(0),
                prompt_eval: st.and_then(|s| s["prompt_eval"].as_u64()).unwrap_or(0),
            }
        }

        Some("warmup_start") => WsClientFrame::WarmupStart,
        Some("warmup_done")  => WsClientFrame::WarmupDone,
        Some("error")        => WsClientFrame::Error(s("content")),

        // frames the server sends that we don't need to act on in TUI
        Some("pinned") | Some("unpinned") | Some("pong") => WsClientFrame::Unknown,

        _ => WsClientFrame::Unknown,
    }
}
