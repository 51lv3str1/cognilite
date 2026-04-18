mod app;
mod clipboard;
mod headless;
mod server;
mod synapse;
mod events;
mod ollama;
mod ui;

use std::time::Duration;
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event};
use color_eyre::Result;
use app::App;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_SERVER_HOST: &str = "0.0.0.0";
const DEFAULT_SERVER_PORT: u16 = 8765;

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let ollama_url = get_ollama_url(&argv);

    if let Some(args) = parse_headless_args(&argv) {
        let code = headless::run(&ollama_url, args);
        std::process::exit(code);
    }

    if let Some((host, port)) = parse_server_args(&argv) {
        server::run(&ollama_url, &host, port);
        return Ok(());
    }

    color_eyre::install()?;
    ratatui::run(|terminal| {
        crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;
        let mut app = App::new(ollama_url.clone());
        App::prewarm_highlight();
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

fn parse_server_args(argv: &[String]) -> Option<(String, u16)> {
    if !argv.iter().any(|a| a == "--server") { return None; }
    let mut host = DEFAULT_SERVER_HOST.to_string();
    let mut port = DEFAULT_SERVER_PORT;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--host" => { i += 1; if i < argv.len() { host = argv[i].clone(); } }
            "--port" => { i += 1; if i < argv.len() { port = argv[i].parse().unwrap_or(DEFAULT_SERVER_PORT); } }
            _ => {}
        }
        i += 1;
    }
    Some((host, port))
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
            arg if !arg.starts_with('-') => {
                ha.message = Some(arg.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    Some(ha)
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
