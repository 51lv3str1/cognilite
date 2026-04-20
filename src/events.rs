use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::app::{App, AskKind, ChatFocus, NeuronMode, Screen, StreamState, fuzzy_match};

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
        Screen::Config        => handle_config(app, key),
        Screen::ModelSelect   => handle_model_select(app, key),
        Screen::RemoteConnect => handle_remote_connect(app, key),
        Screen::Chat          => handle_chat(app, key),
    }
}

fn handle_config(app: &mut App, key: KeyEvent) {
    if handle_help_keys(app, key) { return; }
    match key.code {
        KeyCode::Esc => { app.toggle_config(); return; }
        KeyCode::Tab => {
            app.config_section = (app.config_section + 1) % 5;
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
        // Preset name input captures all keys
        if app.preset_name_input.is_some() {
            match key.code {
                KeyCode::Enter => {
                    let name = app.preset_name_input.take().unwrap_or_default();
                    if !name.trim().is_empty() {
                        app.save_current_as_preset(name.trim().to_string());
                    }
                }
                KeyCode::Esc => { app.preset_name_input = None; }
                KeyCode::Backspace => { if let Some(ref mut s) = app.preset_name_input { s.pop(); } }
                KeyCode::Char(c) => { if let Some(ref mut s) = app.preset_name_input { s.push(c); } }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Left  => { app.neuron_sub_section = app.neuron_sub_section.saturating_sub(1); }
            KeyCode::Right => { app.neuron_sub_section = (app.neuron_sub_section + 1).min(2); }
            _ => {}
        }

        match app.neuron_sub_section {
            0 => {
                // Manual
                let filtered: Vec<usize> = app.neurons.iter().enumerate()
                    .filter(|(_, n)| fuzzy_match(&app.config_search, &n.name))
                    .map(|(i, _)| i).collect();
                match key.code {
                    KeyCode::Up   => nav_prev(&mut app.neuron_cursor, &filtered),
                    KeyCode::Down => nav_next(&mut app.neuron_cursor, &filtered),
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        app.toggle_neuron();
                        if app.neuron_mode != NeuronMode::Manual {
                            app.set_neuron_mode(NeuronMode::Manual);
                        } else {
                            app.warmup_last_hash = None;
                            app.trigger_warmup();
                        }
                    }
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
                // Switching to Manual sub-section activates Manual mode
                if key.code == KeyCode::Left || key.code == KeyCode::Right {
                    app.set_neuron_mode(NeuronMode::Manual);
                }
            }
            1 => {
                // Smart — read-only display; switching here activates Smart mode
                if key.code == KeyCode::Left || key.code == KeyCode::Right {
                    app.set_neuron_mode(NeuronMode::Smart);
                }
            }
            _ => {
                // Presets
                if key.code == KeyCode::Left || key.code == KeyCode::Right {
                    app.set_neuron_mode(NeuronMode::Presets);
                }
                // cursor: 0=Pure, 1..=n=user presets, n+1=+New
                let new_idx = app.neuron_presets.len() + 1;
                match key.code {
                    KeyCode::Up => {
                        if app.preset_cursor > 0 { app.preset_cursor -= 1; }
                    }
                    KeyCode::Down => {
                        app.preset_cursor = (app.preset_cursor + 1).min(new_idx);
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if app.preset_cursor == 0 {
                            app.apply_preset("__pure__");
                        } else if app.preset_cursor == new_idx {
                            app.preset_name_input = Some(String::new());
                        } else if let Some(preset) = app.neuron_presets.get(app.preset_cursor - 1) {
                            let name = preset.name.clone();
                            app.apply_preset(&name);
                        }
                    }
                    KeyCode::Char('n') => { app.preset_name_input = Some(String::new()); }
                    KeyCode::Char('d') | KeyCode::Delete => {
                        if app.preset_cursor > 0 && app.preset_cursor <= app.neuron_presets.len() {
                            app.preset_cursor -= 1;
                            app.delete_preset();
                        }
                    }
                    _ => {}
                }
            }
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
    } else if app.config_section == 3 {
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
    } else {
        const FEATURE_LABELS: &[&str] = &["Thinking"];
        const USERNAME_IDX: usize = FEATURE_LABELS.len(); // index of the username row

        // username editing mode: intercept all keys
        if app.username_editing {
            match key.code {
                KeyCode::Enter => { let name = app.username.clone(); app.set_username(name); }
                KeyCode::Esc   => { app.username_editing = false; }
                KeyCode::Backspace => { app.username.pop(); }
                KeyCode::Char(c)   => { app.username.push(c); }
                _ => {}
            }
            return;
        }

        let filtered: Vec<usize> = FEATURE_LABELS.iter().enumerate()
            .filter(|(_, l)| fuzzy_match(&app.config_search, l))
            .map(|(i, _)| i)
            .collect();
        match key.code {
            KeyCode::Up => {
                if app.features_cursor == USERNAME_IDX {
                    app.features_cursor = filtered.last().copied().unwrap_or(0);
                } else {
                    nav_prev(&mut app.features_cursor, &filtered);
                }
            }
            KeyCode::Down => {
                if app.features_cursor == *filtered.last().unwrap_or(&0) {
                    app.features_cursor = USERNAME_IDX;
                } else {
                    nav_next(&mut app.features_cursor, &filtered);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if app.features_cursor == USERNAME_IDX {
                    app.username_editing = true;
                } else {
                    app.toggle_feature(app.features_cursor);
                }
            }
            KeyCode::Backspace => { app.config_search.pop(); }
            KeyCode::Char(c) => {
                app.config_search.push(c);
                let new_filtered: Vec<usize> = FEATURE_LABELS.iter().enumerate()
                    .filter(|(_, l)| fuzzy_match(&app.config_search, l))
                    .map(|(i, _)| i).collect();
                snap_cursor(&mut app.features_cursor, &new_filtered);
            }
            _ => {}
        }
    }
}

fn handle_remote_connect(app: &mut App, key: KeyEvent) {
    if app.remote_connecting { return; }
    match key.code {
        KeyCode::Tab => { app.toggle_config(); return; }
        KeyCode::Esc => {
            app.remote_connect_error = None;
            app.screen = crate::app::Screen::ModelSelect;
        }
        KeyCode::Enter => {
            let url = app.remote_url.trim().to_string();
            if !url.is_empty() {
                app.remote_connect_error = None;
                if url.starts_with("ws://") {
                    app.start_remote_connect();
                } else {
                    app.start_remote_ollama();
                }
            }
        }
        KeyCode::Backspace => {
            if app.remote_url_cursor > 0 {
                let byte = char_to_byte(&app.remote_url, app.remote_url_cursor - 1);
                let end  = char_to_byte(&app.remote_url, app.remote_url_cursor);
                app.remote_url.drain(byte..end);
                app.remote_url_cursor -= 1;
            }
        }
        KeyCode::Delete => {
            if app.remote_url_cursor < app.remote_url.chars().count() {
                let byte = char_to_byte(&app.remote_url, app.remote_url_cursor);
                let end  = char_to_byte(&app.remote_url, app.remote_url_cursor + 1);
                app.remote_url.drain(byte..end);
            }
        }
        KeyCode::Left => {
            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                app.remote_url_cursor = word_left(&app.remote_url, app.remote_url_cursor);
            } else if app.remote_url_cursor > 0 {
                app.remote_url_cursor -= 1;
            }
        }
        KeyCode::Right => {
            let len = app.remote_url.chars().count();
            if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                app.remote_url_cursor = word_right(&app.remote_url, app.remote_url_cursor);
            } else if app.remote_url_cursor < len {
                app.remote_url_cursor += 1;
            }
        }
        KeyCode::Home => { app.remote_url_cursor = 0; }
        KeyCode::End  => { app.remote_url_cursor = app.remote_url.chars().count(); }
        KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
            let byte = char_to_byte(&app.remote_url, app.remote_url_cursor);
            app.remote_url.insert(byte, c);
            app.remote_url_cursor += 1;
        }
        _ => {}
    }
}

/// Byte offset of the nth char in s.
fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}
fn word_left(s: &str, cur: usize) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = cur.saturating_sub(1);
    while i > 0 && chars[i - 1] != ' ' { i -= 1; }
    i
}
fn word_right(s: &str, cur: usize) -> usize {
    let chars: Vec<char> = s.chars().collect();
    let mut i = cur;
    while i < chars.len() && chars[i] != ' ' { i += 1; }
    while i < chars.len() && chars[i] == ' ' { i += 1; }
    i
}

fn handle_model_select(app: &mut App, key: KeyEvent) {
    if handle_help_keys(app, key) { return; }
    use crossterm::event::KeyModifiers;

    // join-room dialog intercepts all keys when open
    if app.join_room_input.is_some() {
        match key.code {
            KeyCode::Esc => { app.join_room_input = None; }
            KeyCode::Enter => {
                let uuid = app.join_room_input.take().unwrap_or_default().trim().to_string();
                if !uuid.is_empty() && !app.remote_url.is_empty() {
                    let base = app.remote_url.trim_end_matches('/').to_string();
                    let joined = format!("{base}/id/{uuid}");
                    app.remote_url = joined;
                    app.remote_connect_error = None;
                    app.start_remote_connect();
                }
            }
            KeyCode::Backspace => { if let Some(ref mut s) = app.join_room_input { s.pop(); } }
            KeyCode::Char(c) => { if let Some(ref mut s) = app.join_room_input { s.push(c); } }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Tab => { app.toggle_config(); return; }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.remote_connect_error = None;
            app.screen = crate::app::Screen::RemoteConnect;
            return;
        }
        KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // open join-room dialog (needs a server URL already set)
            app.join_room_input = Some(String::new());
            return;
        }
        KeyCode::Esc => {
            if app.ws_tx.is_some() {
                app.switch_to_local();
                return;
            }
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
        KeyCode::Enter => {
            if app.ws_tx.is_some() { app.select_model_remote(); } else { app.select_model(); }
        }
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

    // Room share popup intercepts keys when open
    if app.show_room_share {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => { app.show_room_share = false; }
            KeyCode::Char('y') => {
                if let Some(url) = app.room_share_url() {
                    if crate::clipboard::copy(&url) {
                        app.copy_notice = Some(std::time::Instant::now());
                    }
                }
                app.show_room_share = false;
            }
            _ => {}
        }
        return;
    }

    // File picker intercepts all keys when open
    if app.file_picker.is_some() {
        match key.code {
            KeyCode::Esc              => app.close_file_picker(),
            KeyCode::Up               => app.file_picker_prev(),
            KeyCode::Down             => app.file_picker_next(),
            KeyCode::Enter | KeyCode::Right => app.file_picker_accept(),
            KeyCode::Left             => app.file_picker_go_up(),
            KeyCode::Backspace => {
                let has_query = app.file_picker.as_ref().map(|fp| !fp.query.is_empty()).unwrap_or(false);
                if has_query {
                    if let Some(fp) = &mut app.file_picker { fp.query.pop(); fp.cursor = 0; }
                    app.update_preview();
                } else {
                    app.file_picker_go_up();
                }
            }
            KeyCode::PageUp   => app.file_picker_scroll_preview_up(),
            KeyCode::PageDown => app.file_picker_scroll_preview_down(),
            KeyCode::Char(c) => {
                if let Some(fp) = &mut app.file_picker { fp.query.push(c); fp.cursor = 0; }
                app.update_preview();
            }
            _ => {}
        }
        return;
    }

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

    // Ctrl+B: toggle file panel visibility (works in any focus mode)
    if ctrl && key.code == KeyCode::Char('b') {
        app.toggle_file_panel();
        return;
    }

    // FilePanel focus mode
    if app.chat_focus == ChatFocus::FilePanel {
        match key.code {
            KeyCode::Tab => {
                app.chat_focus = ChatFocus::Input;
                app.auto_scroll = true;
            }
            KeyCode::Esc | KeyCode::Char('q') => app.close_file_panel(),
            KeyCode::PageUp   => app.file_panel_scroll_up(),
            KeyCode::PageDown => app.file_panel_scroll_down(),
            _ => {}
        }
        return;
    }

    // History focus mode: navigate blocks, copy selected, Tab cycles to file panel or input
    if app.chat_focus == ChatFocus::History {
        match key.code {
            KeyCode::Tab => {
                if app.file_panel.is_some() && app.file_panel_visible {
                    app.chat_focus = ChatFocus::FilePanel;
                } else {
                    app.chat_focus = ChatFocus::Input;
                    app.auto_scroll = true;
                }
            }
            KeyCode::Esc => {
                app.chat_focus = ChatFocus::Input;
                app.auto_scroll = true;
            }
            KeyCode::Up   => app.history_nav_prev(),
            KeyCode::Down => app.history_nav_next(),
            KeyCode::PageUp   => { app.auto_scroll = false; app.scroll = app.scroll.saturating_sub(10); }
            KeyCode::PageDown => { app.scroll = app.scroll.saturating_add(10); }
            KeyCode::Enter => app.cycle_message_attachment(),
            KeyCode::Char('q') => app.close_file_panel(),
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
            } else if app.ws_tx.is_some() {
                app.switch_to_local();
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
        KeyCode::Char('p') if ctrl => app.open_file_picker(),
        KeyCode::Char('s') if ctrl => app.export_chat(),
        KeyCode::Char('o') if ctrl => app.open_file_picker_load(),
        KeyCode::Char('j') if ctrl => { if app.room_id.is_some() { app.show_room_share = true; } }

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
