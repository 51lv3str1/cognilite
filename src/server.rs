use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Only one session at a time may own the terminal stdin for interactive <ask> prompts.
static STDIN_LOCK: Mutex<()> = Mutex::new(());

/// Starts the HTTP server. Blocks until the process is killed.
pub fn run(ollama_url: &str, host: &str, port: u16, thinking_server: bool) {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("[server] failed to bind {addr}: {e}");
        std::process::exit(1);
    });
    eprintln!("[server] cognilite listening on http://{addr}");
    eprintln!("[server] POST /chat  {{\"message\": \"...\", ...}}");
    eprintln!("[server] Ctrl+C to stop");

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => { eprintln!("[server] cannot resolve binary path: {e}"); return; }
    };

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let exe = exe.clone();
                let ollama = ollama_url.to_string();
                std::thread::spawn(move || handle(s, exe, ollama, thinking_server));
            }
            Err(e) => eprintln!("[server] accept error: {e}"),
        }
    }
}

fn handle(mut stream: TcpStream, exe: std::path::PathBuf, ollama_url: String, thinking_server: bool) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".into());

    let Some((method, path, body)) = parse_http(&mut stream) else {
        let _ = write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        return;
    };

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

    eprintln!("[server] {} — {}", peer, &message[..message.len().min(80)]);

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
    eprintln!("[server] {} done", peer);
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

fn parse_http(stream: &mut TcpStream) -> Option<(String, String, Vec<u8>)> {
    let mut reader = BufReader::new(stream as &mut dyn Read);

    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let line = line.trim();
    let mut parts = line.splitn(3, ' ');
    let method = parts.next()?.to_string();
    let path = parts.next()?.split('?').next()?.to_string();

    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim();
        if line.is_empty() { break; }
        if line.to_ascii_lowercase().starts_with("content-length:") {
            content_length = line.split(':').nth(1)?.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 { reader.read_exact(&mut body).ok()?; }

    Some((method, path, body))
}
