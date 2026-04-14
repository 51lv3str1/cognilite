mod app;
mod behaviour;
mod events;
mod ollama;
mod ui;

use std::time::Duration;
use crossterm::event::{self, Event};
use color_eyre::Result;
use app::App;

const OLLAMA_BASE_URL: &str = "http://localhost:11434";

fn main() -> Result<()> {
    color_eyre::install()?;
    ratatui::run(|terminal| {
        let mut app = App::new(OLLAMA_BASE_URL.to_string());
        load_models(&mut app);
        run_loop(terminal, &mut app)
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
        app.poll_stream();
        terminal.draw(|frame| ui::draw(frame, app))?;

        let timeout = if app.stream_state == app::StreamState::Streaming {
            Duration::from_millis(30)
        } else {
            Duration::from_millis(200)
        };

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => events::handle_key(app, key),
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
