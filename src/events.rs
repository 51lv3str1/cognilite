use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::{App, Screen, StreamState};

pub fn handle_paste(app: &mut App, text: &str) {
    if app.screen != Screen::Chat {
        return;
    }
    for c in text.chars() {
        if c != '\r' {
            app.input_insert(c);
        }
    }
    app.update_completion();
}

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // global quit
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match app.screen {
        Screen::Config      => handle_config(app, key),
        Screen::ModelSelect => handle_model_select(app, key),
        Screen::Chat        => handle_chat(app, key),
    }
}

fn handle_config(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => { app.toggle_config(); return; }
        // Tab switches between sections
        KeyCode::Tab => {
            app.config_section = if app.config_section == 0 { 1 } else { 0 };
            return;
        }
        _ => {}
    }

    if app.config_section == 0 {
        // ctx strategy section
        const OPTIONS: usize = 2;
        match key.code {
            KeyCode::Up   | KeyCode::Char('k') => {
                if app.config_cursor > 0 { app.config_cursor -= 1; }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if app.config_cursor + 1 < OPTIONS { app.config_cursor += 1; }
            }
            KeyCode::Enter => app.confirm_config(),
            _ => {}
        }
    } else {
        // neurons section
        let count = app.neurons.len();
        match key.code {
            KeyCode::Up   | KeyCode::Char('k') => {
                if app.neuron_cursor > 0 { app.neuron_cursor -= 1; }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if count > 0 && app.neuron_cursor + 1 < count { app.neuron_cursor += 1; }
            }
            KeyCode::Enter | KeyCode::Char(' ') => app.toggle_neuron(),
            _ => {}
        }
    }
}

fn handle_model_select(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Tab => { app.toggle_config(); return; }
        _ => {}
    }
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
    // F1 toggles help popup; while open only scrolling and close keys work
    if key.code == KeyCode::F(1) {
        app.show_help = !app.show_help;
        app.help_scroll = 0;
        return;
    }
    if app.show_help {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => app.show_help = false,
            KeyCode::Up   | KeyCode::Char('k') => app.help_scroll = app.help_scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => app.help_scroll = app.help_scroll.saturating_add(1),
            KeyCode::PageUp   => app.help_scroll = app.help_scroll.saturating_sub(10),
            KeyCode::PageDown => app.help_scroll = app.help_scroll.saturating_add(10),
            _ => {}
        }
        return;
    }

    // completion popup intercepts Esc, Tab, Up, Down
    if app.completion.is_some() {
        match key.code {
            KeyCode::Esc               => { app.complete_dismiss(); return; }
            KeyCode::Tab | KeyCode::Enter => { app.complete_accept(); return; }
            KeyCode::Up                => { app.complete_prev(); return; }
            KeyCode::Down              => { app.complete_next(); return; }
            _ => {}
        }
    }

    let ctrl  = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt   = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        // ── send / cancel / navigate screens ────────────────────────────────
        KeyCode::Enter  => app.send_message(),
        KeyCode::Esc => {
            if app.stream_state == StreamState::Streaming {
                app.stop_stream();
            } else {
                app.stream_rx = None;
                app.screen = Screen::ModelSelect;
            }
        }

        // ── newline ──────────────────────────────────────────────────────────
        KeyCode::Char('n') if ctrl => { app.input_newline(); app.update_completion(); }

        // ── line navigation (readline style) ────────────────────────────────
        KeyCode::Char('a') if ctrl => app.input_home(),
        KeyCode::Char('e') if ctrl => app.input_end(),
        KeyCode::Home               => app.input_home(),
        KeyCode::End if ctrl        => { app.auto_scroll = true; }
        KeyCode::End                => app.input_end(),

        // ── word navigation ──────────────────────────────────────────────────
        KeyCode::Left  if ctrl || alt => { app.input_move_word_left();  app.update_completion(); }
        KeyCode::Right if ctrl || alt => { app.input_move_word_right(); app.update_completion(); }

        // ── kill / clear ─────────────────────────────────────────────────────
        KeyCode::Char('k') if ctrl => app.input_kill_to_end(),
        KeyCode::Char('u') if ctrl => app.input_kill_to_start(),
        KeyCode::Char('w') if ctrl => { app.input_delete_word_before(); app.update_completion(); }
        KeyCode::Char('l') if ctrl => app.clear_chat(),

        // ── cursor movement ──────────────────────────────────────────────────
        KeyCode::Left  => { app.input_move_left();  app.update_completion(); }
        KeyCode::Right => { app.input_move_right(); app.update_completion(); }
        KeyCode::Up if alt => { app.auto_scroll = false; app.scroll = app.scroll.saturating_sub(1); }
        KeyCode::Down if alt => { app.scroll = app.scroll.saturating_add(1); }
        KeyCode::Up => {
            if app.input_line_count() > 1 {
                app.input_move_up();
                app.update_completion();
            } else if app.history_pos.is_some() || !app.input_history.is_empty() {
                app.input_history_prev();
            } else {
                app.auto_scroll = false;
                app.scroll = app.scroll.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if app.input_line_count() > 1 {
                app.input_move_down();
                app.update_completion();
            } else if app.history_pos.is_some() {
                app.input_history_next();
            } else {
                app.scroll = app.scroll.saturating_add(1);
            }
        }
        KeyCode::PageUp   => { app.auto_scroll = false; app.scroll = app.scroll.saturating_sub(10); }
        KeyCode::PageDown => { app.scroll = app.scroll.saturating_add(10); }

        // ── character editing ────────────────────────────────────────────────
        KeyCode::Char(c) => { app.input_insert(c); app.update_completion(); }
        KeyCode::Backspace => { app.input_backspace(); app.update_completion(); }
        KeyCode::Delete    => { app.input_delete();    app.update_completion(); }

        _ => {}
    }
}
