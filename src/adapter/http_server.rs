use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Only one session at a time may own the terminal stdin for interactive <ask> prompts.
static STDIN_LOCK: Mutex<()> = Mutex::new(());

/// Maximum concurrent connections (HTTP /chat + WebSocket sessions combined).
/// Each connection costs a thread, an Ollama request slot, and ~MB of context;
/// without a cap a misbehaving client could fork-bomb the server.
const MAX_CONNECTIONS: usize = 64;
static ACTIVE_CONNECTIONS: AtomicUsize = AtomicUsize::new(0);

/// Starts the HTTP server. Blocks until the process is killed.
pub fn run(ollama_url: &str, host: &str, port: u16, thinking_server: bool, rooms: crate::adapter::ws_server::RoomRegistry, silent: bool) {
    let addr = format!("{host}:{port}");
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            if !silent { eprintln!("[server] failed to bind {addr}: {e}"); }
            return;
        }
    };
    if !silent {
        eprintln!("[server] cognilite listening on http://{addr}");
        eprintln!("[server] POST /chat  {{\"message\": \"...\", ...}}");
        eprintln!("[server] WS    ws://{addr}/id/{{uuid}}  — join room");
        eprintln!("[server] Ctrl+C to stop");
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => { eprintln!("[server] cannot resolve binary path: {e}"); return; }
    };

    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let active = ACTIVE_CONNECTIONS.load(Ordering::Relaxed);
                if active >= MAX_CONNECTIONS {
                    let msg = b"server at capacity, try again later";
                    let _ = write!(s, "HTTP/1.1 503 Service Unavailable\r\n\
                                       Retry-After: 5\r\n\
                                       Content-Length: {}\r\n\r\n", msg.len());
                    let _ = s.write_all(msg);
                    if !silent { eprintln!("[server] rejected connection — at cap ({MAX_CONNECTIONS})"); }
                    continue;
                }
                ACTIVE_CONNECTIONS.fetch_add(1, Ordering::Relaxed);
                let exe    = exe.clone();
                let ollama = ollama_url.to_string();
                let rooms  = rooms.clone();
                std::thread::spawn(move || {
                    handle(s, exe, ollama, thinking_server, rooms, silent);
                    ACTIVE_CONNECTIONS.fetch_sub(1, Ordering::Relaxed);
                });
            }
            Err(e) => { if !silent { eprintln!("[server] accept error: {e}"); } }
        }
    }
}

fn handle(mut stream: TcpStream, exe: std::path::PathBuf, ollama_url: String, thinking_server: bool, rooms: crate::adapter::ws_server::RoomRegistry, silent: bool) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());

    let Some((method, full_path, headers, body)) = parse_http(&mut stream) else {
        let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        return;
    };

    // WebSocket upgrade — route to WS session handler
    let is_ws = headers.get("upgrade").map(|v| v.to_ascii_lowercase() == "websocket").unwrap_or(false);
    if is_ws {
        let key = headers.get("sec-websocket-key").cloned().unwrap_or_default();
        if !crate::adapter::ws_server::handshake(&mut stream, &key) { return; }
        let path_only = full_path.splitn(2, '?').next().unwrap_or("/");
        let (room_id, room) = resolve_room(path_only, &rooms);
        let query = crate::adapter::ws_server::parse_query(&full_path);
        let cfg   = crate::adapter::ws_server::SessionConfig::from_query(&query, thinking_server);
        if !silent { eprintln!("[ws] {peer} upgrading → room {room_id}"); }
        crate::adapter::ws_server::run_session(stream, &ollama_url, cfg, room, room_id, silent);
        return;
    }

    let path = full_path.splitn(2, '?').next().unwrap_or(&full_path).to_string();

    if method != "POST" || path != "/chat" {
        let msg = b"cognilite server: POST /chat required";
        let _ = write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n", msg.len());
        let _ = stream.write_all(msg);
        return;
    }

    let val = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(v) => v,
        Err(_) => {
            let msg = b"invalid JSON body";
            let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\n\r\n", msg.len());
            let _ = stream.write_all(msg);
            return;
        }
    };

    let message = val.get("message").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if message.is_empty() {
        let msg = b"\"message\" field required";
        let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\n\r\n", msg.len());
        let _ = stream.write_all(msg);
        return;
    }

    if !silent { eprintln!("[server] {} — {}", peer, &message[..message.len().min(80)]); }

    let argv = build_argv(&val, &ollama_url, &message, thinking_server);

    // Serialize: only one session at a time can use the terminal for interactive prompts.
    // Concurrent non-interactive requests queue here and proceed as soon as the active one finishes.
    let _guard = STDIN_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut child = match Command::new(&exe)
        .args(&argv)
        .stdin(Stdio::inherit())   // child reads <ask> responses directly from server terminal
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[server] spawn error: {e}");
            let _ = write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };

    let _ = write!(stream, "HTTP/1.1 200 OK\r\n");
    let _ = write!(stream, "Content-Type: text/plain; charset=utf-8\r\n");
    let _ = write!(stream, "Transfer-Encoding: chunked\r\n");
    let _ = write!(stream, "Access-Control-Allow-Origin: *\r\n");
    let _ = write!(stream, "\r\n");

    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout);
        let mut buf = [0u8; 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if write!(stream, "{:x}\r\n", n).is_err() { break; }
                    if stream.write_all(&buf[..n]).is_err() { break; }
                    if write!(stream, "\r\n").is_err() { break; }
                    let _ = stream.flush();
                }
            }
        }
    }

    let _ = write!(stream, "0\r\n\r\n");
    let _ = stream.flush();
    let _ = child.wait();
    if !silent { eprintln!("[server] {} done", peer); }
    // _guard dropped here — next queued connection acquires STDIN_LOCK and proceeds
}

fn build_argv(val: &serde_json::Value, ollama_url: &str, message: &str, thinking_server: bool) -> Vec<String> {
    let mut argv = vec![
        "--headless".to_string(),
        "--server-mode".to_string(),
        "--ollama-url".to_string(), ollama_url.to_string(),
        "--message".to_string(), message.to_string(),
    ];

    if val.get("yes").and_then(|v| v.as_bool()).unwrap_or(false) {
        argv.push("--yes".to_string());
    }
    if val.get("thinking").and_then(|v| v.as_bool()).unwrap_or(false) {
        argv.push("--thinking".to_string());
    }
    if thinking_server {
        argv.push("--thinking-stderr".to_string());
    }

    if let Some(v) = val.get("model").and_then(|v| v.as_str()) {
        argv.extend(["--model".into(), v.into()]);
    }
    if let Some(v) = val.get("neuron_mode").and_then(|v| v.as_str()) {
        argv.extend(["--neuron-mode".into(), v.into()]);
    }
    if let Some(v) = val.get("preset").and_then(|v| v.as_str()) {
        argv.extend(["--preset".into(), v.into()]);
    }
    if let Some(v) = val.get("ctx_strategy").and_then(|v| v.as_str()) {
        argv.extend(["--ctx-strategy".into(), v.into()]);
    }
    if let Some(v) = val.get("temperature").and_then(|v| v.as_f64()) {
        argv.extend(["--temperature".into(), v.to_string()]);
    }
    if let Some(v) = val.get("top_p").and_then(|v| v.as_f64()) {
        argv.extend(["--top-p".into(), v.to_string()]);
    }
    if let Some(v) = val.get("repeat_penalty").and_then(|v| v.as_f64()) {
        argv.extend(["--repeat-penalty".into(), v.to_string()]);
    }
    if let Some(arr) = val.get("no_neurons").and_then(|v| v.as_array()) {
        for n in arr.iter().filter_map(|v| v.as_str()) {
            argv.extend(["--no-neuron".into(), n.into()]);
        }
    }
    if let Some(arr) = val.get("pin").and_then(|v| v.as_array()) {
        for p in arr.iter().filter_map(|v| v.as_str()) {
            argv.extend(["--pin".into(), p.into()]);
        }
    }
    if let Some(arr) = val.get("attach").and_then(|v| v.as_array()) {
        for a in arr.iter().filter_map(|v| v.as_str()) {
            argv.extend(["--attach".into(), a.into()]);
        }
    }
    if val.get("keep_alive").and_then(|v| v.as_bool()).unwrap_or(false) {
        argv.push("--keep-alive".into());
    }

    argv
}

/// Returns (method, full_path_with_query, headers_lowercase_map, body).
fn parse_http(stream: &mut TcpStream) -> Option<(String, String, std::collections::HashMap<String, String>, Vec<u8>)> {
    let mut reader = BufReader::new(stream as &mut dyn Read);

    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let line = line.trim();
    let mut parts = line.splitn(3, ' ');
    let method = parts.next()?.to_string();
    let full_path = parts.next()?.to_string();

    let mut headers: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim();
        if line.is_empty() { break; }
        if let Some(colon) = line.find(':') {
            let key   = line[..colon].trim().to_ascii_lowercase();
            let value = line[colon+1..].trim().to_string();
            if key == "content-length" { content_length = value.parse().unwrap_or(0); }
            headers.insert(key, value);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 { reader.read_exact(&mut body).ok()?; }

    Some((method, full_path, headers, body))
}

fn resolve_room(path: &str, rooms: &crate::adapter::ws_server::RoomRegistry) -> (String, crate::adapter::ws_server::SharedRoom) {
    use std::sync::{Arc, Mutex};
    let uuid = path.strip_prefix("/id/").filter(|s| !s.is_empty()).map(str::to_string);
    let id = uuid.unwrap_or_else(crate::adapter::ws_server::new_uuid);
    let room = {
        let mut map = rooms.lock().unwrap();
        map.entry(id.clone()).or_insert_with(|| Arc::new(Mutex::new(crate::adapter::ws_server::RoomState {
            messages: vec![], version: 0,
            live_tokens: String::new(), live_token_version: 0, live_user: String::new(),
            active_session_ids: std::collections::HashSet::new(),
        }))).clone()
    };
    (id, room)
}
