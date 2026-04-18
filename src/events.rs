use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::{App, AskKind, ChatFocus, Screen, StreamState, fuzzy_match};

fn nav_prev(cursor: &mut usize, filtered: &[usize]) {
    if let Some(pos) = filtered.iter().position(|&i| i == *cursor) {
        if pos > 0 { *cursor = filtered[pos - 1]; }
    } else if !filtered.is_empty() {
        *cursor = *filtered.last().unwrap();
    }
}

fn nav_next(cursor: &mut usize, filtered: &[usize]) {
    if let Some(pos) = filtered.iter().position(|&i| i == *cursor) {
        if pos + 1 < filtered.len() { *cursor = filtered[pos + 1]; }
    } else if !filtered.is_empty() {
        *cursor = filtered[0];
    }
}

fn snap_cursor(cursor: &mut usize, filtered: &[usize]) {
    if !filtered.iter().any(|&i| i == *cursor) {
        *cursor = filtered.first().copied().unwrap_or(0);
    }
}

/// Handles help popup scroll/close keys. Returns true if the key was consumed.
fn handle_help_keys(app: &mut App, key: KeyEvent) -> bool {
    if key.code == KeyCode::F(1) {
        app.show_help = !app.show_help;
        app.help_scroll = 0;
        return true;
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
        return true;
    }
    false
}

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
    if handle_help_keys(app, key) { return; }
    match key.code {
        KeyCode::Esc => { app.toggle_config(); return; }
        KeyCode::Tab => {
            app.config_section = (app.config_section + 1) % 4;
            app.config_search.clear();
            return;
        }
        _ => {}
    }

    if app.config_section == 0 {
        const CTX_LABELS: &[&str] = &["Dynamic context", "Full context"];
        let filtered: Vec<usize> = CTX_LABELS.iter().enumerate()
            .filter(|(_, l)| fuzzy_match(&app.config_search, l))
            .map(|(i, _)| i)
            .collect();
        match key.code {
            KeyCode::Up   => nav_prev(&mut app.config_cursor, &filtered),
            KeyCode::Down => nav_next(&mut app.config_cursor, &filtered),
            KeyCode::Enter => app.confirm_config(),
            KeyCode::Backspace => { app.config_search.pop(); }
            KeyCode::Char(c) => {
                app.config_search.push(c);
                let new_filtered: Vec<usize> = CTX_LABELS.iter().enumerate()
                    .filter(|(_, l)| fuzzy_match(&app.config_search, l))
                    .map(|(i, _)| i).collect();
                snap_cursor(&mut app.config_cursor, &new_filtered);
            }
            _ => {}
        }
    } else if app.config_section == 1 {
        let filtered: Vec<usize> = app.neurons.iter().enumerate()
            .filter(|(_, n)| fuzzy_match(&app.config_search, &n.name))
            .map(|(i, _)| i)
            .collect();
        match key.code {
            KeyCode::Up   => nav_prev(&mut app.neuron_cursor, &filtered),
            KeyCode::Down => nav_next(&mut app.neuron_cursor, &filtered),
            KeyCode::Enter | KeyCode::Char(' ') => app.toggle_neuron(),
            KeyCode::Backspace => { app.config_search.pop(); }
            KeyCode::Char(c) => {
                app.config_search.push(c);
                let new_filtered: Vec<usize> = app.neurons.iter().enumerate()
                    .filter(|(_, n)| fuzzy_match(&app.config_search, &n.name))
                    .map(|(i, _)| i).collect();
                snap_cursor(&mut app.neuron_cursor, &new_filtered);
            }
            _ => {}
        }
    } else if app.config_section == 2 {
        let filtered: Vec<usize> = crate::app::GEN_PARAMS.iter().enumerate()
            .filter(|(_, (name, _, _, _, _, _))| fuzzy_match(&app.config_search, name))
            .map(|(i, _)| i)
            .collect();
        match key.code {
            KeyCode::Up    => nav_prev(&mut app.param_cursor, &filtered),
            KeyCode::Down  => nav_next(&mut app.param_cursor, &filtered),
            KeyCode::Left  | KeyCode::Char('-') => app.param_adjust(-1.0),
            KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => app.param_adjust(1.0),
            KeyCode::Char('r') => app.param_reset(),
            KeyCode::Backspace => {
                if !app.config_search.is_empty() { app.config_search.pop(); }
                else { app.param_reset(); }
            }
            KeyCode::Char(c) if c != '-' && c != '+' && c != '=' && c != 'r' => {
                app.config_search.push(c);
                let new_filtered: Vec<usize> = crate::app::GEN_PARAMS.iter().enumerate()
                    .filter(|(_, (name, _, _, _, _, _))| fuzzy_match(&app.config_search, name))
                    .map(|(i, _)| i).collect();
                snap_cursor(&mut app.param_cursor, &new_filtered);
            }
            _ => {}
        }
    } else {
        const PERF_LABELS: &[&str] = &["Stable num_ctx", "Keep model alive", "Warm-up cache"];
        let filtered: Vec<usize> = PERF_LABELS.iter().enumerate()
            .filter(|(_, l)| fuzzy_match(&app.config_search, l))
            .map(|(i, _)| i)
            .collect();
        match key.code {
            KeyCode::Up   => nav_prev(&mut app.perf_cursor, &filtered),
            KeyCode::Down => nav_next(&mut app.perf_cursor, &filtered),
            KeyCode::Enter | KeyCode::Char(' ') => app.toggle_perf(app.perf_cursor),
            KeyCode::Backspace => { app.config_search.pop(); }
            KeyCode::Char(c) => {
                app.config_search.push(c);
                let new_filtered: Vec<usize> = PERF_LABELS.iter().enumerate()
                    .filter(|(_, l)| fuzzy_match(&app.config_search, l))
                    .map(|(i, _)| i).collect();
                snap_cursor(&mut app.perf_cursor, &new_filtered);
            }
            _ => {}
        }
    }
}

fn handle_model_select(app: &mut App, key: KeyEvent) {
    if handle_help_keys(app, key) { return; }
    match key.code {
        KeyCode::Tab => { app.toggle_config(); return; }
        KeyCode::Esc => {
            if !app.model_search.is_empty() {
                app.model_search.clear();
                snap_cursor(&mut app.model_cursor, &(0..app.models.len()).collect::<Vec<_>>());
            }
            return;
        }
        _ => {}
    }
    if app.models.is_empty() { return; }

    let filtered: Vec<usize> = app.models.iter().enumerate()
        .filter(|(_, m)| fuzzy_match(&app.model_search, &m.name))
        .map(|(i, _)| i)
        .collect();

    match key.code {
        KeyCode::Up   => nav_prev(&mut app.model_cursor, &filtered),
        KeyCode::Down => nav_next(&mut app.model_cursor, &filtered),
        KeyCode::Enter => app.select_model(),
        KeyCode::Backspace => {
            app.model_search.pop();
            let new_filtered: Vec<usize> = app.models.iter().enumerate()
                .filter(|(_, m)| fuzzy_match(&app.model_search, &m.name))
                .map(|(i, _)| i).collect();
            snap_cursor(&mut app.model_cursor, &new_filtered);
        }
        KeyCode::Char(c) => {
            app.model_search.push(c);
            let new_filtered: Vec<usize> = app.models.iter().enumerate()
                .filter(|(_, m)| fuzzy_match(&app.model_search, &m.name))
                .map(|(i, _)| i).collect();
            snap_cursor(&mut app.model_cursor, &new_filtered);
        }
        _ => {}
    }
}

fn handle_chat(app: &mut App, key: KeyEvent) {
    if handle_help_keys(app, key) { return; }

    // Ask mode intercepts all keys when awaiting user input
    if app.ask.is_some() {
        let (kind_tag, opts_len, selected) = {
            let a = app.ask.as_ref().unwrap();
            match &a.kind {
                AskKind::Text    => ("text",    0usize, String::new()),
                AskKind::Confirm => ("confirm", 0,      String::new()),
                AskKind::Choice(opts) => {
                    let sel = opts.get(app.ask_cursor).cloned().unwrap_or_default();
                    ("choice", opts.len(), sel)
                }
            }
        };
        match kind_tag {
            "confirm" => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => app.submit_ask("Yes".to_string()),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc   => app.submit_ask("No".to_string()),
                _ => {}
            },
            "choice" => match key.code {
                KeyCode::Up    => { if app.ask_cursor > 0 { app.ask_cursor -= 1; } }
                KeyCode::Down  => { if app.ask_cursor + 1 < opts_len { app.ask_cursor += 1; } }
                KeyCode::Enter => app.submit_ask(selected),
                KeyCode::Esc   => app.cancel_ask(),
                _ => {}
            },
            _ => match key.code { // text
                KeyCode::Enter => {
                    let r = app.input.trim().to_string();
                    if !r.is_empty() { app.submit_ask(r); }
                }
                KeyCode::Esc       => app.cancel_ask(),
                KeyCode::Backspace => { app.input_backspace(); }
                KeyCode::Delete    => { app.input_delete(); }
                KeyCode::Left      => app.input_move_left(),
                KeyCode::Right     => app.input_move_right(),
                KeyCode::Char(c)   => { app.input_insert(c); }
                _ => {}
            },
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

    // History focus mode: navigate blocks, copy selected, Esc/Tab back to input
    if app.chat_focus == ChatFocus::History {
        match key.code {
            KeyCode::Esc | KeyCode::Tab => {
                app.chat_focus = ChatFocus::Input;
                app.auto_scroll = true;
            }
            KeyCode::Up   => app.history_nav_prev(),
            KeyCode::Down => app.history_nav_next(),
            KeyCode::Char('y') if ctrl => app.copy_block(app.history_cursor),
            _ => {}
        }
        return;
    }

    match key.code {
        // ── send / cancel / navigate screens ────────────────────────────────
        KeyCode::Tab    => app.enter_history_mode(),
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
        KeyCode::Char('y') if ctrl => app.copy_last_response(),
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
