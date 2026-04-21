mod app;
mod clipboard;
mod tools;
mod headless;
mod server;
mod websocket;
mod ws_client;
mod synapse;
mod events;
mod ollama;
mod ui;

use std::time::Duration;
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event};
use color_eyre::Result;
use app::App;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
pub const DEFAULT_SERVER_HOST: &str = "0.0.0.0";
pub const DEFAULT_SERVER_PORT: u16 = 8765;

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let ollama_url = get_ollama_url(&argv);

    // --read --remote <url>: dump room history and exit, no message sent
    if argv.iter().any(|a| a == "--read") {
        if let Some(pos) = argv.iter().position(|a| a == "--remote") {
            if let Some(base_url) = argv.get(pos + 1) {
                // append username so the server doesn't fall back to the active model name
                let username = argv.iter().position(|a| a == "--username")
                    .and_then(|i| argv.get(i + 1))
                    .map(|u| url_encode(u))
                    .unwrap_or_else(|| url_encode("observer"));
                let sep = if base_url.contains('?') { '&' } else { '?' };
                let url = format!("{base_url}{sep}username={username}");
                let code = ws_client::run_read_history(&url);
                std::process::exit(code);
            }
        }
    }

    // headless + remote: connect to a WS room and send one message (non-interactive)
    if argv.iter().any(|a| a == "--headless") {
        if let Some(ws_url) = parse_remote_arg_headless(&argv) {
            if let Some(args) = parse_headless_args(&argv) {
                let message = args.message.as_deref().unwrap_or("").to_string();
                let code = ws_client::run_headless(&ws_url, &message, args.thinking_stderr);
                std::process::exit(code);
            }
        }
    }

    if let Some(args) = parse_headless_args(&argv) {
        let code = headless::run(&ollama_url, args);
        std::process::exit(code);
    }

    if let Some((host, port, thinking)) = parse_server_args(&argv) {
        server::run(&ollama_url, &host, port, thinking, crate::websocket::new_room_registry(), false);
        return Ok(());
    }

    if let Some(ws_url) = parse_remote_arg(&argv) {
        match ws_client::connect(&ws_url) {
            Err(e) => {
                eprintln!("cognilite: --remote connection failed: {e}");
                std::process::exit(1);
            }
            Ok((tx, rx)) => {
                color_eyre::install()?;
                ratatui::run(|terminal| {
                    crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;
                    let mut app = App::new(ollama_url.clone());
                    App::prewarm_highlight();
                    app.ws_tx = Some(tx);
                    app.ws_rx = Some(rx);
                    app.loading_models = true;
                    app.screen = app::Screen::ModelSelect;
                    let result = run_loop(terminal, &mut app);
                    let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
                    result
                })?;
            }
        }
        return Ok(());
    }

    color_eyre::install()?;
    ratatui::run(|terminal| {
        crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;
        let mut app = App::new(ollama_url.clone());
        App::prewarm_highlight();

        // start embedded WS server so others can join the local chat
        let rooms = crate::websocket::new_room_registry();
        if let Some(ref room_id) = app.room_id.clone() {
            use std::sync::{Arc, Mutex};
            let room: crate::websocket::SharedRoom = Arc::new(Mutex::new(crate::websocket::RoomState {
                messages: vec![], version: 0,
                live_tokens: String::new(), live_token_version: 0, live_user: String::new(),
                active_session_ids: std::collections::HashSet::new(),
            }));
            rooms.lock().unwrap().insert(room_id.clone(), room.clone());
            // reserve both IDs so WS clients can't collide with the local TUI
            {
                let mut r = room.lock().unwrap();
                r.active_session_ids.insert(app.session_id.clone());
                r.active_session_ids.insert(app.user_session_id.clone());
            }
            app.shared_room = Some(room);
        }
        {
            let rooms = rooms.clone();
            let host = DEFAULT_SERVER_HOST.to_string();
            let ollama = ollama_url.clone();
            std::thread::spawn(move || server::run(&ollama, &host, DEFAULT_SERVER_PORT, false, rooms, true));
        }

        load_models(&mut app);
        let result = run_loop(terminal, &mut app);
        let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
        result
    })?;
    Ok(())
}

fn get_ollama_url(argv: &[String]) -> String {
    let mut i = 0;
    while i + 1 < argv.len() {
        if argv[i] == "--ollama-url" { return argv[i + 1].clone(); }
        i += 1;
    }
    std::env::var("OLLAMA_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string())
}

fn parse_server_args(argv: &[String]) -> Option<(String, u16, bool)> {
    if !argv.iter().any(|a| a == "--server") { return None; }
    let mut host = DEFAULT_SERVER_HOST.to_string();
    let mut port = DEFAULT_SERVER_PORT;
    let mut thinking = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--host" => { i += 1; if i < argv.len() { host = argv[i].clone(); } }
            "--port" => { i += 1; if i < argv.len() { port = argv[i].parse().unwrap_or(DEFAULT_SERVER_PORT); } }
            "--thinking" => { thinking = true; }
            _ => {}
        }
        i += 1;
    }
    Some((host, port, thinking))
}

fn parse_headless_args(argv: &[String]) -> Option<headless::HeadlessArgs> {
    if !argv.iter().any(|a| a == "--headless") {
        return None;
    }
    let mut ha = headless::HeadlessArgs::default();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--headless" | "--server" => {}
            // global flags with a value — skip both flag and value
            "--ollama-url" | "--host" | "--port" => { i += 1; }
            "--message" => {
                i += 1; if i < argv.len() { ha.message = Some(argv[i].clone()); }
            }
            "--model" | "-m" => {
                i += 1; if i < argv.len() { ha.model = Some(argv[i].clone()); }
            }
            "--neuron-mode" => {
                i += 1; if i < argv.len() { ha.neuron_mode = Some(app::NeuronMode::from_str(&argv[i])); }
            }
            "--preset" => {
                i += 1; if i < argv.len() { ha.preset = Some(argv[i].clone()); }
            }
            "--no-neuron" => {
                i += 1; if i < argv.len() { ha.no_neurons.push(argv[i].clone()); }
            }
            "--temperature" => {
                i += 1; if i < argv.len() { ha.temperature = argv[i].parse().ok(); }
            }
            "--top-p" => {
                i += 1; if i < argv.len() { ha.top_p = argv[i].parse().ok(); }
            }
            "--repeat-penalty" => {
                i += 1; if i < argv.len() { ha.repeat_penalty = argv[i].parse().ok(); }
            }
            "--ctx-strategy" => {
                i += 1; if i < argv.len() { ha.ctx_strategy = Some(app::CtxStrategy::from_str(&argv[i])); }
            }
            "--keep-alive" => { ha.keep_alive = true; }
            "--pin" => {
                i += 1; if i < argv.len() { ha.pin.push(argv[i].clone()); }
            }
            "--attach" => {
                i += 1; if i < argv.len() { ha.attach.push(argv[i].clone()); }
            }
            "--yes" | "-y" => { ha.yes = true; }
            "--thinking" => { ha.thinking = true; }
            "--thinking-stderr" => { ha.thinking_stderr = true; }
            "--server-mode" => { ha.server_mode = true; }
            "--username" => {
                i += 1; if i < argv.len() { ha.username = Some(argv[i].clone()); }
            }
            arg if !arg.starts_with('-') => {
                ha.message = Some(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    Some(ha)
}

/// Like parse_remote_arg but for headless mode: omits `client=tui`, adds username only.
fn parse_remote_arg_headless(argv: &[String]) -> Option<String> {
    let pos = argv.iter().position(|a| a == "--remote")?;
    let mut url = argv.get(pos + 1)?.clone();
    let mut params: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--thinking" => params.push("thinking=true".into()),
            "--yes" | "-y" => params.push("yes=true".into()),
            "--model" | "-m" => {
                i += 1;
                if i < argv.len() { params.push(format!("model={}", argv[i])); }
            }
            "--username" => {
                i += 1;
                if i < argv.len() { params.push(format!("username={}", url_encode(&argv[i]))); }
            }
            _ => {}
        }
        i += 1;
    }
    // inject config username if --username was not explicitly passed
    if !params.iter().any(|p| p.starts_with("username=")) {
        let username = app::load_config().username;
        if !username.is_empty() {
            params.push(format!("username={}", url_encode(&username)));
        }
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    url.push(sep);
    url.push_str(&params.join("&"));
    Some(url)
}

/// Returns the WS URL if --remote <url> is present, with any extra flags appended as query params.
fn parse_remote_arg(argv: &[String]) -> Option<String> {
    let pos = argv.iter().position(|a| a == "--remote")?;
    let mut url = argv.get(pos + 1)?.clone();
    // append supported per-session query params from remaining flags
    let mut params: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--thinking" => params.push("thinking=true".into()),
            "--yes" | "-y" => params.push("yes=true".into()),
            "--model" | "-m" => {
                i += 1;
                if i < argv.len() { params.push(format!("model={}", argv[i])); }
            }
            "--preset" => {
                i += 1;
                if i < argv.len() { params.push(format!("preset={}", argv[i])); }
            }
            "--neuron-mode" => {
                i += 1;
                if i < argv.len() { params.push(format!("neuron_mode={}", argv[i])); }
            }
            "--username" => {
                i += 1;
                if i < argv.len() { params.push(format!("username={}", url_encode(&argv[i]))); }
            }
            _ => {}
        }
        i += 1;
    }
    // always identify as TUI client so the server injects the right runtime context
    params.push("client=tui".into());
    // pass username from client's own config if not explicitly overridden
    if !params.iter().any(|p| p.starts_with("username=")) {
        let username = app::load_config().username;
        if !username.is_empty() {
            params.push(format!("username={}", url_encode(&username)));
        }
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    url.push(sep);
    url.push_str(&params.join("&"));
    Some(url)
}

fn url_encode(s: &str) -> String {
    s.chars().fold(String::new(), |mut out, c| {
        if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    })
}

fn load_models(app: &mut App) {
    match ollama::list_models(&app.base_url) {
        Ok(entries) => {
            app.models = entries;
            app.loading_models = false;
        }
        Err(e) => {
            app.models_error = Some(e);
            app.loading_models = false;
        }
    }
}

fn run_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> color_eyre::Result<()> {
    loop {
        app.poll_warmup();
        app.poll_stream();
        app.poll_room();
        app.poll_ws();
        app.poll_local_models();
        app.poll_remote_connect();
        app.poll_remote_ollama();
        app.poll_highlight();
        app.check_pinned_files();
        app.check_file_panel();
        terminal.draw(|frame| ui::draw(frame, app))?;

        let timeout = if app.stream_state == app::StreamState::Streaming {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(200)
        };

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => events::handle_key(app, key),
                Event::Paste(text) => events::handle_paste(app, &text),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
