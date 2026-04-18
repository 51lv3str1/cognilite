mod app;
mod clipboard;
mod headless;
mod synapse;
mod events;
mod ollama;
mod ui;

use std::time::Duration;
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event};
use color_eyre::Result;
use app::App;

const OLLAMA_BASE_URL: &str = "http://localhost:11434";

fn main() -> Result<()> {
    if let Some(args) = parse_headless_args() {
        let code = headless::run(OLLAMA_BASE_URL, args);
        std::process::exit(code);
    }

    color_eyre::install()?;
    ratatui::run(|terminal| {
        crossterm::execute!(std::io::stdout(), EnableBracketedPaste)?;
        let mut app = App::new(OLLAMA_BASE_URL.to_string());
        App::prewarm_highlight();
        load_models(&mut app);
        let result = run_loop(terminal, &mut app);
        let _ = crossterm::execute!(std::io::stdout(), DisableBracketedPaste);
        result
    })?;
    Ok(())
}

fn parse_headless_args() -> Option<headless::HeadlessArgs> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if !argv.iter().any(|a| a == "--headless") {
        return None;
    }
    let mut ha = headless::HeadlessArgs::default();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--headless" => {}
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
