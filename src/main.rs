mod app;
mod clipboard;
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
