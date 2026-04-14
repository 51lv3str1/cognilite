use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::{App, Screen, StreamState};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // global quit
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match app.screen {
        Screen::ModelSelect => handle_model_select(app, key),
        Screen::Chat => handle_chat(app, key),
    }
}

fn handle_model_select(app: &mut App, key: KeyEvent) {
    if app.models.is_empty() {
        return;
    }
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if app.model_cursor > 0 {
                app.model_cursor -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.model_cursor + 1 < app.models.len() {
                app.model_cursor += 1;
            }
        }
        KeyCode::Enter => app.select_model(),
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}

fn handle_chat(app: &mut App, key: KeyEvent) {
    // completion popup intercepts Esc, Tab, Up, Down
    if app.completion.is_some() {
        match key.code {
            KeyCode::Esc => { app.complete_dismiss(); return; }
            KeyCode::Tab | KeyCode::Enter => { app.complete_accept(); return; }
            KeyCode::Up => { app.complete_prev(); return; }
            KeyCode::Down => { app.complete_next(); return; }
            _ => {}
        }
    }

    match key.code {
        KeyCode::Esc => {
            if app.stream_state == StreamState::Streaming {
                app.stop_stream();
            } else {
                app.stream_rx = None;
                app.screen = Screen::ModelSelect;
            }
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input_newline();
            app.update_completion();
        }
        KeyCode::Enter => app.send_message(),
        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.clear_chat();
        }
        // scroll messages (Alt) or move cursor between input lines (no modifier)
        KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(1);
        }
        KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
            app.scroll = app.scroll.saturating_add(1);
        }
        KeyCode::Up => {
            if app.input_line_count() > 1 {
                app.input_move_up();
            } else {
                app.auto_scroll = false;
                app.scroll = app.scroll.saturating_sub(1);
            }
            app.update_completion();
        }
        KeyCode::Down => {
            if app.input_line_count() > 1 {
                app.input_move_down();
            } else {
                app.scroll = app.scroll.saturating_add(1);
            }
            app.update_completion();
        }
        KeyCode::PageUp => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(10);
        }
        KeyCode::PageDown => {
            app.scroll = app.scroll.saturating_add(10);
        }
        KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.auto_scroll = true;
        }
        KeyCode::End => app.input_end(),
        // input editing
        KeyCode::Char(c) => {
            app.input_insert(c);
            app.update_completion();
        }
        KeyCode::Backspace => {
            app.input_backspace();
            app.update_completion();
        }
        KeyCode::Delete => {
            app.input_delete();
            app.update_completion();
        }
        KeyCode::Left => { app.input_move_left(); app.update_completion(); }
        KeyCode::Right => { app.input_move_right(); app.update_completion(); }
        KeyCode::Home => app.input_home(),
        _ => {}
    }
}
