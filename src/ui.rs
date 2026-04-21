use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use crate::app::{App, AskKind, ChatFocus, CompletionKind, FilePickerEntry, CtxStrategy, Role, Screen, StreamState};

const ACCENT: Color = Color::Rgb(137, 180, 250);  // blue
const USER_COLOR: Color = Color::Rgb(166, 227, 161); // green
const ASSISTANT_COLOR: Color = Color::Rgb(203, 166, 247); // purple
const DIM: Color = Color::Rgb(108, 112, 134);
const THINKING_COLOR: Color = Color::Rgb(73, 77, 100); // dark muted — thinking blocks
const ERROR_COLOR: Color = Color::Rgb(243, 139, 168);
const BG: Color = Color::Rgb(30, 30, 46);
const SURFACE: Color = Color::Rgb(49, 50, 68);
const CODE_FG: Color = Color::Rgb(205, 214, 244);  // bright — code text
const CODE_BORDER: Color = Color::Rgb(88, 91, 112); // dim — left gutter bar

pub fn draw(frame: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Config        => draw_config(frame, app),
        Screen::ModelSelect   => draw_model_select(frame, app),
        Screen::RemoteConnect => draw_remote_connect(frame, app),
        Screen::Chat          => draw_chat(frame, app),
    }
}


// Shared page skeleton: 1-row top pad · 3-row title · Fill content · 2-row hints.
// Returns (title_area, content_area, hints_area).
fn page_layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(area);
    (chunks[1], chunks[2], chunks[3])
}

fn render_title(frame: &mut Frame, area: Rect, app: &App) {
    let suffix = app.remote_label.as_deref().unwrap_or("ollama TUI");
    let title = Paragraph::new(Line::from(vec![
        Span::styled("cogni", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("lite", Style::default().fg(ASSISTANT_COLOR).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  ·  {suffix}"), Style::default().fg(DIM)),
    ]))
    .alignment(ratatui::layout::Alignment::Center)
    .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(SURFACE)));
    frame.render_widget(title, area);
}


// Renders a / search textbox with rounded border and cursor placement.
fn render_search_input(frame: &mut Frame, area: Rect, query: &str, placeholder: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));
    let widget = if query.is_empty() {
        Paragraph::new(Line::from(vec![
            Span::styled("/ ", Style::default().fg(ACCENT)),
            Span::styled(placeholder.to_owned(), Style::default().fg(THINKING_COLOR)),
        ])).block(block)
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("/ ", Style::default().fg(ACCENT)),
            Span::styled(query.to_owned(), Style::default().fg(Color::White)),
        ])).block(block)
    };
    frame.render_widget(widget, area);
    let cursor_x = area.x + 1 + 2 + query.chars().count() as u16;
    frame.set_cursor_position((cursor_x.min(area.x + area.width - 2), area.y + 1));
}

// Renders hints spanning the full row width, centered.
fn render_hints(frame: &mut Frame, area: Rect, spans: Vec<Span>) {
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(Style::default().fg(DIM))
            .alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

fn draw_config(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let (title_area, content_area, hints_area) = page_layout(area);
    render_title(frame, title_area, app);

    // split content: tab bar · search textbox · settings box
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .split(content_area);

    // ── Tab bar ───────────────────────────────────────────────────────────────
    let tabs = ["General", "Context", "Neurons", "Features"];
    let mut tab_spans: Vec<Span> = Vec::new();
    for (i, name) in tabs.iter().enumerate() {
        if i > 0 { tab_spans.push(Span::styled("  ·  ", Style::default().fg(THINKING_COLOR))); }
        if i == app.config_section {
            tab_spans.push(Span::styled(*name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));
        } else {
            tab_spans.push(Span::styled(*name, Style::default().fg(DIM)));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)).alignment(ratatui::layout::Alignment::Center), content_chunks[0]);

    // ── Search textbox ────────────────────────────────────────────────────────
    render_search_input(frame, content_chunks[1], &app.config_search, "filter");

    // ── Content box ───────────────────────────────────────────────────────────
    let box_area = content_chunks[2];
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));
    let inner = Rect {
        x: box_area.x + 1,
        y: box_area.y + 1,
        width: box_area.width.saturating_sub(2),
        height: box_area.height.saturating_sub(2),
    };
    frame.render_widget(content_block, box_area);

    let items_y = inner.y;

    match app.config_section {
        0 => {
            // ── General ───────────────────────────────────────────────────────
            let value_style = if app.username_editing {
                Style::default().fg(ACCENT).bg(SURFACE)
            } else {
                Style::default().fg(DIM)
            };
            let value_text = if app.username_editing {
                format!("  ›  {}█", app.username)
            } else {
                format!("  —  {}", app.username)
            };
            frame.render_widget(Paragraph::new(Line::from(vec![
                Span::styled("     ", Style::default().bg(SURFACE)),
                Span::styled("Username", Style::default().fg(Color::White).bg(SURFACE).add_modifier(Modifier::BOLD)),
                Span::styled(value_text, value_style),
            ])), Rect { x: inner.x, y: items_y, width: inner.width, height: 1 });
        }
        1 => {
            // ── Context strategy ──────────────────────────────────────────────
            struct CtxOption<'a> { label: &'a str, desc: &'a str, strategy: CtxStrategy }
            let ctx_options = [
                CtxOption { label: "Dynamic context", desc: "Grows with the conversation. Faster, lower memory.", strategy: CtxStrategy::Dynamic },
                CtxOption { label: "Full context",    desc: "Always uses the model's max window. Slower but never\ntruncates long histories.", strategy: CtxStrategy::Full },
            ];
            let mut y = items_y;
            for (i, opt) in ctx_options.iter().enumerate() {
                if !crate::app::fuzzy_match(&app.config_search, opt.label) { continue; }
                let cursor  = i == app.config_cursor;
                let checked = opt.strategy == app.ctx_strategy;
                let (marker, circle_fg) = if checked { ("●", ACCENT) } else { ("○", DIM) };
                let label_style = if cursor { Style::default().fg(Color::White).bg(SURFACE).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
                let bg = if cursor { Style::default().bg(SURFACE) } else { Style::default() };
                frame.render_widget(Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {marker} "), bg.patch(Style::default().fg(circle_fg))),
                    Span::styled(opt.label, label_style),
                ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                y += 1;
                for desc_line in opt.desc.lines() {
                    frame.render_widget(Paragraph::new(Line::from(Span::styled(
                        format!("    {desc_line}"), Style::default().fg(DIM),
                    ))), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    y += 1;
                }
                y += 1;
            }
        }
        2 => {
            // ── Neurons ───────────────────────────────────────────────────────
            use crate::app::neuron_is_tooling;

            // Sub-tab bar: Manual · Smart · Presets
            let sub_tabs = ["Manual", "Smart", "Presets"];
            let mut sub_spans: Vec<Span> = Vec::new();
            for (i, name) in sub_tabs.iter().enumerate() {
                if i > 0 { sub_spans.push(Span::styled("  ·  ", Style::default().fg(DIM))); }
                if i == app.neuron_sub_section {
                    sub_spans.push(Span::styled(*name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));
                } else {
                    sub_spans.push(Span::styled(*name, Style::default().fg(DIM)));
                }
            }
            frame.render_widget(
                Paragraph::new(Line::from(sub_spans)).alignment(ratatui::layout::Alignment::Center),
                Rect { x: inner.x, y: items_y, width: inner.width, height: 1 },
            );

            // Token summary line
            {
                let enabled = app.effective_enabled_neurons();
                let summary = if app.neuron_sub_section == 1 {
                    // Smart: split reasoning vs tooling
                    let r_tok: u64 = enabled.iter().filter(|n| !neuron_is_tooling(n)).map(|n| (n.system_prompt.len() / 4) as u64).sum();
                    let t_tok: u64 = enabled.iter().filter(|n| neuron_is_tooling(n)).map(|n| (n.system_prompt.len() / 4) as u64).sum();
                    let t_count = enabled.iter().filter(|n| neuron_is_tooling(n)).count();
                    if t_count > 0 {
                        format!("~{r_tok}tok active  +  ~{t_tok}tok on-demand ({t_count} tooling)")
                    } else {
                        format!("~{r_tok}tok active")
                    }
                } else {
                    let total: u64 = enabled.iter().map(|n| (n.system_prompt.len() / 4) as u64).sum();
                    let count = enabled.len();
                    format!("~{total}tok  ·  {count} neurons")
                };
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(summary, Style::default().fg(THINKING_COLOR))))
                        .alignment(ratatui::layout::Alignment::Center),
                    Rect { x: inner.x, y: items_y + 1, width: inner.width, height: 1 },
                );
            }

            let content_y = items_y + 3;

            match app.neuron_sub_section {
                0 => {
                    // Manual — current neuron toggle list
                    let filtered: Vec<(usize, &crate::synapse::Neuron)> = app.neurons.iter().enumerate()
                        .filter(|(_, n)| crate::app::fuzzy_match(&app.config_search, &n.name))
                        .collect();
                    for (row, (orig_idx, neuron)) in filtered.iter().enumerate() {
                        let cursor  = *orig_idx == app.neuron_cursor;
                        let enabled = !app.disabled_neurons.contains(&neuron.name);
                        let (marker, circle_fg) = if enabled { ("●", ACCENT) } else { ("○", DIM) };
                        let name_style = if cursor { Style::default().fg(Color::White).bg(SURFACE).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
                        let desc_style = if cursor { Style::default().fg(DIM).bg(SURFACE) } else { Style::default().fg(DIM) };
                        let bg = if cursor { Style::default().bg(SURFACE) } else { Style::default() };
                        let desc = if neuron.description.is_empty() { String::new() } else { format!("  —  {}", neuron.description) };
                        let tok = (neuron.system_prompt.len() / 4) as u64;
                        frame.render_widget(Paragraph::new(Line::from(vec![
                            Span::styled(format!("  {marker} "), bg.patch(Style::default().fg(circle_fg))),
                            Span::styled(&neuron.name, name_style),
                            Span::styled(desc, desc_style),
                            Span::styled(format!("  ~{tok}tok"), bg.patch(Style::default().fg(THINKING_COLOR))),
                        ])), Rect { x: inner.x, y: content_y + row as u16, width: inner.width, height: 1 });
                    }
                }
                1 => {
                    // Smart — reasoning always active, tooling on demand
                    let mut y = content_y;
                    let reasoning: Vec<_> = app.neurons.iter()
                        .filter(|n| !neuron_is_tooling(n) && !app.disabled_neurons.contains(&n.name))
                        .collect();
                    let tooling: Vec<_> = app.neurons.iter()
                        .filter(|n| neuron_is_tooling(n) && !app.disabled_neurons.contains(&n.name))
                        .collect();

                    frame.render_widget(Paragraph::new(Line::from(Span::styled(
                        "  Always active (reasoning)", Style::default().fg(DIM).add_modifier(Modifier::BOLD),
                    ))), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    y += 1;
                    for n in &reasoning {
                        let tok = (n.system_prompt.len() / 4) as u64;
                        let desc = if n.description.is_empty() { String::new() } else { format!("  —  {}", n.description) };
                        frame.render_widget(Paragraph::new(Line::from(vec![
                            Span::styled("  ● ", Style::default().fg(ACCENT)),
                            Span::styled(&n.name, Style::default().fg(Color::White)),
                            Span::styled(desc, Style::default().fg(DIM)),
                            Span::styled(format!("  ~{tok}tok"), Style::default().fg(THINKING_COLOR)),
                        ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                        y += 1;
                    }
                    if reasoning.is_empty() {
                        frame.render_widget(Paragraph::new(Line::from(
                            Span::styled("  (none)", Style::default().fg(DIM))
                        )), Rect { x: inner.x, y, width: inner.width, height: 1 });
                        y += 1;
                    }
                    y += 1;
                    frame.render_widget(Paragraph::new(Line::from(Span::styled(
                        "  On demand (tooling)", Style::default().fg(DIM).add_modifier(Modifier::BOLD),
                    ))), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    y += 1;
                    for n in &tooling {
                        let tok = (n.system_prompt.len() / 4) as u64;
                        let desc = if n.description.is_empty() { String::new() } else { format!("  —  {}", n.description) };
                        frame.render_widget(Paragraph::new(Line::from(vec![
                            Span::styled("  ◈ ", Style::default().fg(THINKING_COLOR)),
                            Span::styled(&n.name, Style::default().fg(Color::White)),
                            Span::styled(desc, Style::default().fg(DIM)),
                            Span::styled(format!("  ~{tok}tok"), Style::default().fg(THINKING_COLOR)),
                        ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                        y += 1;
                    }
                    if tooling.is_empty() {
                        frame.render_widget(Paragraph::new(Line::from(
                            Span::styled("  (none)", Style::default().fg(DIM))
                        )), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    }
                }
                _ => {
                    // Presets
                    // cursor layout: 0 = Raw, 1..=n = user presets, n+1 = + New
                    let mut y = content_y;

                    // Name input when creating
                    if let Some(ref draft) = app.preset_name_input {
                        frame.render_widget(Paragraph::new(Line::from(vec![
                            Span::styled("  New preset name: ", Style::default().fg(DIM)),
                            Span::styled(draft.as_str(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                            Span::styled("_", Style::default().fg(ACCENT)),
                        ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                        return;
                    }

                    // Built-in: Raw (no neurons)
                    let pure_active   = app.active_preset.as_deref() == Some("__pure__");
                    let pure_selected = app.preset_cursor == 0;
                    let (pure_marker, pure_fg) = if pure_active { ("●", ACCENT) } else { ("○", DIM) };
                    let pure_bg = if pure_selected { Style::default().bg(SURFACE) } else { Style::default() };
                    frame.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(format!("  {pure_marker} "), pure_bg.patch(Style::default().fg(pure_fg))),
                        Span::styled("Raw", pure_bg.patch(Style::default().fg(Color::White).add_modifier(if pure_selected { Modifier::BOLD } else { Modifier::empty() }))),
                        Span::styled("   no neurons loaded", pure_bg.patch(Style::default().fg(DIM))),
                    ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    y += 2;

                    // User presets (cursor offset by 1)
                    for (i, preset) in app.neuron_presets.iter().enumerate() {
                        let selected = app.preset_cursor == i + 1;
                        let active   = app.active_preset.as_deref() == Some(&preset.name);
                        let (marker, marker_fg) = if active { ("●", ACCENT) } else { ("○", DIM) };
                        let bg = if selected { Style::default().bg(SURFACE) } else { Style::default() };
                        let summary = preset.enabled.join(" · ");
                        let summary_str = if summary.is_empty() { "(empty)".to_string() } else { summary };
                        frame.render_widget(Paragraph::new(Line::from(vec![
                            Span::styled(format!("  {marker} "), bg.patch(Style::default().fg(marker_fg))),
                            Span::styled(&preset.name, bg.patch(Style::default().fg(Color::White).add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }))),
                            Span::styled(format!("   {summary_str}"), bg.patch(Style::default().fg(DIM))),
                        ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                        y += 1;
                    }

                    // "+ New" at bottom
                    let new_idx = app.neuron_presets.len() + 1;
                    let new_selected = app.preset_cursor == new_idx;
                    let new_bg = if new_selected { Style::default().bg(SURFACE) } else { Style::default() };
                    y += 1;
                    frame.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled("  + ", new_bg.patch(Style::default().fg(ACCENT))),
                        Span::styled("New preset from current selection", new_bg.patch(Style::default().fg(if new_selected { Color::White } else { DIM }))),
                    ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                }
            }
        }
        _ => {
            // ── Features (Generation + Performance) ───────────────────────────
            // cursor 0-2 = gen params, 3-5 = perf flags, 6 = thinking
            struct Toggle<'a> { label: &'a str, desc: &'a str, value: bool }
            let toggles = [
                Toggle { label: "Stable num_ctx",   desc: "Round context window to powers of 2 to preserve KV cache", value: app.ctx_pow2   },
                Toggle { label: "Keep model alive", desc: "Prevent Ollama from unloading the model between requests",  value: app.keep_alive },
                Toggle { label: "Warm-up cache",    desc: "Pre-fill KV cache with the system prompt on model load",    value: app.warmup     },
                Toggle { label: "Thinking",         desc: "Enable extended thinking for supported models (think: true)", value: app.thinking },
            ];

            let mut y = items_y;

            // Generation sub-section
            let gen_any = (0..crate::app::GEN_PARAMS.len()).any(|i| crate::app::fuzzy_match(&app.config_search, crate::app::GEN_PARAMS[i].0));
            if gen_any {
                frame.render_widget(Paragraph::new(Line::from(
                    Span::styled("  Generation", Style::default().fg(DIM).add_modifier(Modifier::BOLD))
                )), Rect { x: inner.x, y, width: inner.width, height: 1 });
                y += 1;
                for orig_idx in 0..crate::app::GEN_PARAMS.len() {
                    let (name, desc, default, _, _, _) = crate::app::GEN_PARAMS[orig_idx];
                    if !crate::app::fuzzy_match(&app.config_search, name) { continue; }
                    let cursor = orig_idx == app.features_cursor;
                    let value = app.gen_params[orig_idx];
                    let is_default = (value - default).abs() < 0.001;
                    let bg        = if cursor { Style::default().bg(SURFACE) } else { Style::default() };
                    let name_style = bg.patch(if cursor { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) });
                    let val_style  = bg.patch(if is_default { Style::default().fg(DIM) } else { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) });
                    let dim_style  = bg.patch(Style::default().fg(THINKING_COLOR));
                    let val_str = if orig_idx == 3 {
                        if value == 0.0 { "unlimited".to_string() } else { format!("{}", value as u64) }
                    } else {
                        format!("{value:.2}")
                    };
                    frame.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(format!("  {name:<16}"), name_style),
                        Span::styled("← ", dim_style),
                        Span::styled(val_str, val_style),
                        Span::styled(" →", dim_style),
                        Span::styled(format!("  {desc}"), dim_style),
                    ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    y += 1;
                }
                y += 1;
            }

            // Performance sub-section (indices 3-6)
            let perf_any = (0..4usize).any(|i| crate::app::fuzzy_match(&app.config_search, toggles[i].label));
            if perf_any {
                frame.render_widget(Paragraph::new(Line::from(
                    Span::styled("  Performance", Style::default().fg(DIM).add_modifier(Modifier::BOLD))
                )), Rect { x: inner.x, y, width: inner.width, height: 1 });
                y += 1;
                for (ti, opt) in toggles.iter().enumerate() {
                    let feature_idx = 3 + ti;
                    if !crate::app::fuzzy_match(&app.config_search, opt.label) { continue; }
                    let cursor = feature_idx == app.features_cursor;
                    let (marker, circle_fg) = if opt.value { ("●", ACCENT) } else { ("○", DIM) };
                    let name_style = if cursor { Style::default().fg(Color::White).bg(SURFACE).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
                    let desc_style = if cursor { Style::default().fg(DIM).bg(SURFACE) } else { Style::default().fg(DIM) };
                    let bg = if cursor { Style::default().bg(SURFACE) } else { Style::default() };
                    frame.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(format!("  {marker} "), bg.patch(Style::default().fg(circle_fg))),
                        Span::styled(opt.label, name_style),
                        Span::styled(format!("  —  {}", opt.desc), desc_style),
                    ])), Rect { x: inner.x, y, width: inner.width, height: 1 });
                    y += 1;
                }
            }
        }
    }

    // ── Hints ─────────────────────────────────────────────────────────────────
    let action_hint: Vec<Span> = if app.config_section == 0 {
        if app.username_editing {
            vec![hint("Enter", "save"), Span::raw("  "), hint("Esc", "cancel")]
        } else {
            vec![hint("Enter", "edit")]
        }
    } else if app.config_section == 1 {
        vec![hint("Enter", "confirm")]
    } else if app.config_section == 3 && app.features_cursor < crate::app::GEN_PARAMS.len() {
        vec![hint("←/→", "adjust"), Span::raw("  "), hint("r", "reset")]
    } else {
        vec![hint("Enter", "toggle")]
    };
    let mut hint_spans = vec![hint("↑/↓", "navigate"), Span::raw("  ")];
    hint_spans.extend(action_hint);
    hint_spans.extend([Span::raw("  "), hint("type", "filter"), Span::raw("  "), hint("Tab", "next tab"), Span::raw("  "), hint("Esc", "close"), Span::raw("  "), hint("F1", "help")]);
    render_hints(frame, hints_area, hint_spans);

    if app.show_help {
        const SECTIONS: &[(&str, &[(&str, &str)])] = &[
            ("Navigation", &[
                ("↑  ↓",          "Move selection"),
                ("Tab",           "Next tab"),
                ("type",          "Filter list"),
                ("Esc",           "Close settings"),
            ]),
            ("General", &[
                ("Enter",         "Edit username"),
                ("Esc",           "Cancel edit without saving"),
            ]),
            ("Context", &[
                ("Enter",         "Confirm selection"),
            ]),
            ("Neurons", &[
                ("Enter / Space", "Toggle on / off"),
            ]),
            ("Features — Generation", &[
                ("←  →",          "Adjust value"),
                ("r",             "Reset to default"),
            ]),
            ("Features — Performance", &[
                ("Enter / Space", "Toggle on / off"),
            ]),
        ];
        draw_help_popup(frame, app, area, SECTIONS);
    }
}

fn draw_remote_connect(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let (title_area, content_area, hints_area) = page_layout(area);
    render_title(frame, title_area, app);

    let w = area.width.min(62);
    let h = 11u16;
    let panel = Rect {
        x: content_area.x + (content_area.width.saturating_sub(w)) / 2,
        y: content_area.y + (content_area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, panel);
    frame.render_widget(
        Block::default()
            .title(Span::styled(" remote connect ", Style::default().fg(ACCENT)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG)),
        panel,
    );

    let inner = Rect {
        x: panel.x + 2,
        y: panel.y + 1,
        width: panel.width.saturating_sub(4),
        height: panel.height.saturating_sub(2),
    };

    // label
    frame.render_widget(
        Paragraph::new(Span::styled(
            "ws://host:port  ·  or  ·  http://host:11434",
            Style::default().fg(DIM),
        )),
        Rect { y: inner.y + 1, height: 1, ..inner },
    );

    // url input
    let input_area = Rect { y: inner.y + 2, height: 3, ..inner };
    let border_color = if app.remote_connect_error.is_some() { ERROR_COLOR } else { ACCENT };
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(SURFACE));

    let display = if app.remote_url.is_empty() {
        Span::styled("ws://192.168.1.10:8765", Style::default().fg(DIM))
    } else {
        Span::styled(app.remote_url.as_str(), Style::default().fg(Color::White))
    };
    frame.render_widget(Paragraph::new(display).block(input_block), input_area);

    if !app.remote_connecting && !app.remote_url.is_empty() {
        let cx = input_area.x + 1 + app.remote_url_cursor as u16;
        if cx < input_area.x + input_area.width - 1 {
            frame.set_cursor_position((cx, input_area.y + 1));
        }
    }

    // status / error
    let status_area = Rect { y: inner.y + 5, height: 2, ..inner };
    if app.remote_connecting {
        frame.render_widget(
            Paragraph::new(Span::styled("Connecting…", Style::default().fg(ACCENT))),
            status_area,
        );
    } else if let Some(ref err) = app.remote_connect_error {
        frame.render_widget(
            Paragraph::new(Span::styled(err.as_str(), Style::default().fg(ERROR_COLOR)))
                .wrap(Wrap { trim: true }),
            status_area,
        );
    }

    render_hints(frame, hints_area, vec![
        hint("Enter", "connect"),
        Span::raw("  "),
        hint("Esc", "back"),
        Span::raw("  "),
        hint("Tab", "settings"),
        Span::raw("  "),
        hint("Ctrl+←/→", "word"),
    ]);
}

fn draw_model_select(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let (title_area, content_area, hints_area) = page_layout(area);
    render_title(frame, title_area, app);

    // split content: search input · model list
    let content_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Fill(1)])
        .split(content_area);

    // search input box
    render_search_input(frame, content_chunks[0], &app.model_search, "search models…");

    // model list
    let block = Block::default()
        .title(Span::styled(" models ", Style::default().fg(ACCENT)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));

    let list_area = content_chunks[1];

    if app.loading_models {
        let p = Paragraph::new("Loading models…")
            .style(Style::default().fg(DIM))
            .block(block);
        frame.render_widget(p, list_area);
    } else if let Some(ref err) = app.models_error {
        let p = Paragraph::new(err.as_str())
            .style(Style::default().fg(ERROR_COLOR))
            .wrap(Wrap { trim: true })
            .block(block);
        frame.render_widget(p, list_area);
    } else if app.models.is_empty() {
        let p = Paragraph::new("No models found. Pull one with:\n  ollama pull llama3.2:1b")
            .style(Style::default().fg(DIM))
            .wrap(Wrap { trim: true })
            .block(block);
        frame.render_widget(p, list_area);
    } else {
        // column widths based on actual available area
        // layout: [2 left][name_w][2 gap][META_W][2 right] inside 1-char borders each side
        // meta layout: [{:>8}  {:>7}  {:>7}] = 8+2+7+2+7 = 26
        const META_W: usize = 26;
        const LEFT_PAD: usize = 2;
        const GAP: usize = 2;
        const RIGHT_PAD: usize = 2;
        let inner = list_area.width.saturating_sub(2) as usize; // subtract left+right borders
        let name_w = inner.saturating_sub(LEFT_PAD + GAP + META_W + RIGHT_PAD);

        let filtered_models: Vec<(usize, &crate::ollama::ModelEntry)> = app.models.iter().enumerate()
            .filter(|(_, m)| crate::app::fuzzy_match(&app.model_search, &m.name))
            .collect();
        let selected_pos = filtered_models.iter().position(|(i, _)| *i == app.model_cursor);

        let items: Vec<ListItem> = filtered_models
            .iter()
            .map(|(orig_idx, entry)| {
                let selected = *orig_idx == app.model_cursor;

                // truncate name if needed
                let name_chars: Vec<char> = entry.name.chars().collect();
                let name_col = if name_chars.len() > name_w {
                    let truncated: String = name_chars[..name_w.saturating_sub(1)].iter().collect();
                    format!("{truncated}…")
                } else {
                    format!("{:<width$}", entry.name, width = name_w)
                };

                // each metadata sub-column has a fixed width so they align across rows
                let param = entry.parameter_size.as_deref().unwrap_or("");
                let quant = entry.quantization_level.as_deref().unwrap_or("");
                let size  = entry.size_bytes.map(format_bytes).unwrap_or_default();
                let meta_col = format!("{:>8}  {:>7}  {:>7}", param, quant, size);

                if selected {
                    ListItem::new(Line::from(vec![
                        Span::styled("  ", Style::default().bg(ACCENT)),
                        Span::styled(name_col, Style::default().fg(BG).bg(ACCENT).add_modifier(Modifier::BOLD)),
                        Span::styled("  ", Style::default().bg(ACCENT)),
                        Span::styled(meta_col, Style::default().fg(BG).bg(ACCENT)),
                        Span::styled("  ", Style::default().bg(ACCENT)),
                    ]))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(name_col, Style::default().fg(Color::White)),
                        Span::raw("  "),
                        Span::styled(meta_col, Style::default().fg(DIM)),
                        Span::raw("  "),
                    ]))
                }
            })
            .collect();

        let mut state = ListState::default().with_selected(selected_pos);
        frame.render_stateful_widget(
            List::new(items).block(block).highlight_style(Style::default()),
            list_area,
            &mut state,
        );
    }

    render_hints(frame, hints_area, vec![
        hint("↑/↓", "navigate"),
        Span::raw("  "),
        hint("Enter", "select"),
        Span::raw("  "),
        hint("Esc", "clear"),
        Span::raw("  "),
        hint("Ctrl+R", "remote"),
        Span::raw("  "),
        hint("Ctrl+J", "join room"),
        Span::raw("  "),
        hint("Tab", "settings"),
        Span::raw("  "),
        hint("Ctrl+C", "quit"),
        Span::raw("  "),
        hint("F1", "help"),
    ]);

    // ── Join Room dialog ────────────────────────────────────────────────────
    if let Some(ref uuid_input) = app.join_room_input {
        let popup_w = area.width.min(60);
        let popup_h = 5u16;
        let px = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let py = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup_area = Rect { x: px, y: py, width: popup_w, height: popup_h };
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        let block = Block::default()
            .title(Span::styled(" Join Room ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);
        let display = if uuid_input.is_empty() {
            Span::styled("room UUID…", Style::default().fg(DIM))
        } else {
            Span::styled(uuid_input.as_str(), Style::default().fg(Color::White))
        };
        frame.render_widget(Paragraph::new(Line::from(vec![Span::raw(" "), display])), inner);
        if !uuid_input.is_empty() {
            frame.set_cursor_position((inner.x + 1 + uuid_input.len() as u16, inner.y));
        }
    }

    if app.show_help {
        const SECTIONS: &[(&str, &[(&str, &str)])] = &[
            ("Navigation", &[
                ("↑  ↓",  "Navigate models"),
                ("Enter", "Select and open chat"),
            ]),
            ("Search", &[
                ("type",  "Filter by name"),
                ("Esc",   "Clear search"),
            ]),
            ("Rooms (multi-user)", &[
                ("—",      "WS server starts automatically on port 8765"),
                ("Ctrl+J", "Join a room — type UUID (uses last remote URL as base)"),
                ("Ctrl+R", "Connect to a remote server URL (ws://host:port)"),
            ]),
            ("General", &[
                ("Tab",    "Open settings"),
                ("F1",     "Toggle this help"),
                ("Ctrl+C", "Quit"),
            ]),
        ];
        draw_help_popup(frame, app, area, SECTIONS);
    }
}

fn draw_chat(frame: &mut Frame, app: &mut App) {
    let full_area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), full_area);

    // Split horizontally when file panel is open and visible
    let (area, panel_area_opt) = if app.file_panel.is_some() && app.file_panel_visible {
        let [left, right] = Layout::horizontal([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ]).areas(full_area);
        (left, Some(right))
    } else {
        (full_area, None)
    };

    // +2 for borders; account for visual wrapping (area.width - 4 = borders(2) + prefix(2))
    let input_inner_width = area.width.saturating_sub(4);
    let input_height = (visual_line_count(&app.input, input_inner_width) + 2).min(10);
    let warming_up = app.warmup_rx.is_some() || app.ws_warmup_started_at.is_some();
    let (ask_height, hide_input) = match &app.ask {
        None => (0u16, false),
        Some(r) => match &r.kind {
            AskKind::Text    => (0,                                        false),
            AskKind::Confirm => (1,                                        true),
            AskKind::Choice(opts) => (opts.len().min(8) as u16 + 2,       true),
        }
    };
    let pinned_height = if app.pinned_files.is_empty() { 0u16 } else { 1 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                                          // [0] header
            Constraint::Length(pinned_height),                              // [1] pinned bar
            Constraint::Fill(1),                                            // [2] messages
            Constraint::Length(if warming_up { 1 } else { 0 }),            // [3] warmup
            Constraint::Length(ask_height),                                 // [4] ask widget
            Constraint::Length(if hide_input { 0 } else { input_height }), // [5] input
            Constraint::Length(1),                                          // [6] hints
        ])
        .split(area);
    let header_area  = chunks[0];
    let pinned_area  = chunks[1];
    let msg_area_    = chunks[2];
    let warmup_area  = chunks[3];
    let ask_area     = chunks[4];
    let input_area   = chunks[5];
    let hints_area   = chunks[6];

    // --- header ---
    let model_name = app.selected_model.as_deref().unwrap_or("unknown");
    let stream_indicator = match &app.stream_state {
        StreamState::Streaming => Span::styled(" ● ", Style::default().fg(USER_COLOR)),
        StreamState::Error(_) => Span::styled(" ✗ ", Style::default().fg(ERROR_COLOR)),
        StreamState::Idle => Span::styled(" ○ ", Style::default().fg(DIM)),
    };
    let mut header_spans = vec![
        Span::styled("cognilite", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("  ›  ", Style::default().fg(DIM)),
        Span::styled(model_name, Style::default().fg(ASSISTANT_COLOR).add_modifier(Modifier::BOLD)),
        stream_indicator,
    ];
    if let Some(ctx_len) = app.context_length {
        if ctx_len > 0 {
            let ctx_k = ctx_len / 1000;
            if app.used_tokens > 0 {
                let pct = (app.used_tokens as f64 / ctx_len as f64 * 100.0) as u64;
                let color = if pct < 50 {
                    USER_COLOR
                } else if pct < 80 {
                    Color::Rgb(249, 226, 175)
                } else {
                    ERROR_COLOR
                };
                header_spans.push(Span::styled(
                    format!("  ctx {pct}% / {ctx_k}k"),
                    Style::default().fg(color),
                ));
            } else {
                header_spans.push(Span::styled(
                    format!("  ctx {ctx_k}k"),
                    Style::default().fg(DIM),
                ));
            }
        }
    }

    if app.plan_mode {
        header_spans.push(Span::styled("  plan", Style::default().fg(Color::Rgb(249, 226, 175)).add_modifier(Modifier::BOLD)));
    } else if app.auto_accept {
        header_spans.push(Span::styled("  auto✓", Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD)));
    } else {
        header_spans.push(Span::styled("  normal", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)));
    }
    {
        let budget = app.gen_params[3] as u64;
        if budget > 0 {
            header_spans.push(Span::styled(format!("  think:{budget}"), Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD)));
        }
    }
    if let Some(t) = app.copy_notice {
        if t.elapsed().as_secs_f64() < 2.0 {
            header_spans.push(Span::styled("  ✓ copied", Style::default().fg(USER_COLOR)));
        } else {
            app.copy_notice = None;
        }
    }
    if let Some((t, ref msg)) = app.status_notice.clone() {
        if t.elapsed().as_secs_f64() < 3.0 {
            header_spans.push(Span::styled(format!("  ✓ {msg}"), Style::default().fg(USER_COLOR)));
        } else {
            app.status_notice = None;
        }
    }
    frame.render_widget(Paragraph::new(Line::from(header_spans)), header_area);

    // --- pinned files bar ---
    if !app.pinned_files.is_empty() {
        let mut spans = vec![Span::styled("  📎 ", Style::default().fg(DIM))];
        for pf in &app.pinned_files {
            let name = pf.display.split('/').last().unwrap_or(&pf.display);
            if pf.changed {
                spans.push(Span::styled(format!("{name} "), Style::default().fg(ASSISTANT_COLOR)));
                spans.push(Span::styled("⟳  ", Style::default().fg(Color::Rgb(249, 226, 175))));
            } else {
                spans.push(Span::styled(format!("{name} "), Style::default().fg(DIM)));
                spans.push(Span::styled("✓  ", Style::default().fg(USER_COLOR)));
            }
        }
        spans.push(Span::styled("Ctrl+P", Style::default().fg(THINKING_COLOR)));
        frame.render_widget(Paragraph::new(Line::from(spans)), pinned_area);
    }

    // --- messages ---
    let msg_area = msg_area_;
    let inner_width = msg_area.width.saturating_sub(4) as usize; // borders + padding
    let history_mode = app.chat_focus == ChatFocus::History;

    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line: Option<u16> = None;
    lines.push(Line::raw(""));

    for (msg_idx, msg) in app.messages.iter().enumerate() {
        let is_selected = history_mode && msg_idx == app.history_cursor;
        match msg.role {
            Role::User => {
                if is_selected { selected_line = Some(lines.len() as u16); }
                let copy_hint = if is_selected {
                    Span::styled("  ⎘", Style::default().fg(ACCENT))
                } else {
                    Span::raw("")
                };
                let sender = msg.tool_call.as_deref().unwrap_or(app.username.as_str());
                let color = crate::app::username_color(sender);
                let prefix = if is_selected {
                    Span::styled(format!("► {sender}"), Style::default().fg(color).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled(sender.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD))
                };
                lines.push(Line::from(vec![prefix, copy_hint]));
                // show message text (with @refs stripped to just the filename)
                let display = clean_at_refs(&msg.content);
                for text_line in wrap_text(&display, inner_width) {
                    if !text_line.trim().is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("  {text_line}"),
                            Style::default().fg(Color::White),
                        )));
                    }
                }
                // attachment chips
                for (att_idx, att) in msg.attachments.iter().enumerate() {
                    let icon = match att.kind {
                        crate::app::AttachmentKind::Text => "≡",
                        crate::app::AttachmentKind::Image => "⬡",
                    };
                    let size_str = if att.size >= 1024 {
                        format!("{:.1}KB", att.size as f64 / 1024.0)
                    } else {
                        format!("{}B", att.size)
                    };
                    let is_open = app.file_panel.as_ref()
                        .map(|fp| fp.path == att.path)
                        .unwrap_or(false);
                    let is_this_msg_selected = is_selected;
                    let _ = (att_idx, is_this_msg_selected);
                    if is_open {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {icon} "), Style::default().fg(BG).bg(ACCENT)),
                            Span::styled(att.filename.clone(), Style::default().fg(BG).bg(ACCENT).add_modifier(Modifier::BOLD)),
                            Span::styled(format!(" {size_str} "), Style::default().fg(BG).bg(ACCENT)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {icon} "), Style::default().fg(ACCENT)),
                            Span::styled(att.filename.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                            Span::styled(format!("  {size_str}"), Style::default().fg(DIM)),
                            if is_selected && att.kind == crate::app::AttachmentKind::Text {
                                Span::styled("  Enter to view", Style::default().fg(DIM))
                            } else {
                                Span::raw("")
                            },
                        ]));
                    }
                }
            }
            Role::Tool => {
                if is_selected { selected_line = Some(lines.len() as u16); }
                if msg.attachments.is_empty() {
                    if let Some(ref label) = msg.tool_call {
                        lines.push(Line::from(vec![
                            Span::styled(format!("  {label}  "), Style::default().fg(DIM)),
                            Span::styled(msg.content.clone(), Style::default().fg(ACCENT)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  ✗ ", Style::default().fg(ERROR_COLOR)),
                            Span::styled(msg.content.clone(), Style::default().fg(ERROR_COLOR)),
                        ]));
                    }
                } else {
                    let att = &msg.attachments[0];
                    let size_str = if att.size >= 1024 {
                        format!("{:.1} KB", att.size as f64 / 1024.0)
                    } else {
                        format!("{} B", att.size)
                    };
                    let label = msg.tool_call.as_deref().unwrap_or("tool");
                    let toggle = if msg.tool_collapsed { "▶" } else { "▼" };
                    let copy_hint = if is_selected { "  ⎘" } else { "" };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {toggle} ⚙ {label}  "), Style::default().fg(DIM)),
                        Span::styled(att.filename.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("  {size_str}{copy_hint}"), Style::default().fg(DIM)),
                    ]));
                    if !msg.tool_collapsed {
                        for code_line in msg.content.lines() {
                            let truncated: String = code_line.chars().take(inner_width.saturating_sub(4)).collect();
                            lines.push(Line::from(vec![
                                Span::styled("  ▎ ", Style::default().fg(CODE_BORDER)),
                                Span::styled(truncated, Style::default().fg(CODE_FG)),
                            ]));
                        }
                    }
                }
            }
            Role::Assistant => {
                let is_last = msg_idx == app.messages.len() - 1;
                let streaming = is_last && app.stream_state == StreamState::Streaming;

                // intermediate assistant messages that only held a tool call
                // have empty content — skip them entirely, the Tool result below tells the story
                if msg.content.is_empty() && msg.thinking.is_empty() && !streaming {
                    continue;
                }

                if is_selected { selected_line = Some(lines.len() as u16); }
                let copy_hint = if is_selected {
                    Span::styled("  ⎘", Style::default().fg(ACCENT))
                } else {
                    Span::raw("")
                };
                // use the tagged identity (username#id) if present, else build it from selected model + session_id
                let model_label = if let Some(ref tag) = msg.tool_call {
                    tag.clone()
                } else {
                    let base = crate::app::model_display_name(
                        app.selected_model.as_deref().unwrap_or("assistant")
                    );
                    format!("{}#{}", base, app.session_id)
                };
                let label = if is_selected { format!("► {model_label}") } else { model_label.clone() };
                let model_color = crate::app::username_color(
                    model_label.split('#').next().unwrap_or(&model_label)
                );
                lines.push(Line::from(vec![
                    Span::styled(label, Style::default().fg(model_color).add_modifier(Modifier::BOLD)),
                    copy_hint,
                ]));
                if msg.content.is_empty() && msg.thinking.is_empty() {
                    // no tokens yet — model is prefilling the KV cache
                    let label = match app.stream_started_at {
                        Some(started) => format!(
                            "  waiting… {}▋",
                            format_duration(started.elapsed().as_secs_f64())
                        ),
                        None => "  ▋".to_string(),
                    };
                    lines.push(Line::from(Span::styled(label, Style::default().fg(DIM))));
                } else {
                    // use thinking_secs (phase only) for the "thought for X" label;
                    // fall back to wall_secs if there was no content transition captured;
                    // for intermediate messages interrupted by a tool call, use msg.thinking_secs
                    let thought_secs = msg.stats.as_ref().map(|s| {
                        s.thinking_secs.unwrap_or(s.wall_secs)
                    }).or(msg.thinking_secs);
                    render_assistant_content(&mut lines, &msg.thinking, &msg.content, inner_width, streaming, thought_secs);
                }
                if let Some(ref s) = msg.stats {
                    if s.response_tokens > 0 {
                        let stats_str = if s.prompt_eval_count > 0 {
                            format!(
                                "  {} tok/s  ·  {} tokens  ·  {} prompt eval  ·  {}",
                                format!("{:.1}", s.tokens_per_sec),
                                s.response_tokens,
                                s.prompt_eval_count,
                                format_duration(s.wall_secs),
                            )
                        } else {
                            format!(
                                "  {} tok/s  ·  {} tokens  ·  {}",
                                format!("{:.1}", s.tokens_per_sec),
                                s.response_tokens,
                                format_duration(s.wall_secs),
                            )
                        };
                        lines.push(Line::from(Span::styled(
                            stats_str,
                            Style::default().fg(DIM),
                        )));
                    }
                }
            }
        }
        lines.push(Line::raw(""));
    }

    // live tokens from another room participant (remote TUI mode)
    if let Some((ref user, ref tokens)) = app.room_live {
        let user_color = crate::app::username_color(user.split('#').next().unwrap_or(user));
        lines.push(Line::from(Span::styled(
            user.clone(),
            Style::default().fg(user_color).add_modifier(Modifier::BOLD),
        )));
        for text_line in wrap_text(tokens.trim(), inner_width) {
            lines.push(Line::from(Span::raw(format!("  {text_line}"))));
        }
        if let Some(last) = lines.last_mut() {
            last.spans.push(Span::styled("▋", Style::default().fg(DIM)));
        }
        lines.push(Line::raw(""));
    }

    if let StreamState::Error(ref e) = app.stream_state {
        lines.push(Line::from(Span::styled(
            format!("  Error: {e}"),
            Style::default().fg(ERROR_COLOR),
        )));
        lines.push(Line::raw(""));
    }

    // context usage warnings
    if let Some(ctx_len) = app.context_length {
        if ctx_len > 0 && app.used_tokens > 0 {
            let pct = app.used_tokens as f64 / ctx_len as f64 * 100.0;

            if pct >= 100.0 {
                lines.push(Line::from(vec![
                    Span::styled("  ⚠ ", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
                    Span::styled("Context window full. ", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
                    Span::styled("The model can no longer respond.", Style::default().fg(ERROR_COLOR)),
                ]));
                lines.push(Line::from(Span::styled(
                    "  What you can do:",
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(vec![
                    Span::styled("    [Ctrl+L]  ", Style::default().fg(ACCENT)),
                    Span::styled("Clear chat and start fresh", Style::default().fg(Color::White)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("    [Esc]     ", Style::default().fg(ACCENT)),
                    Span::styled("Go back and pick a model with a larger context", Style::default().fg(Color::White)),
                ]));
                lines.push(Line::raw(""));
            } else if pct >= 90.0 {
                lines.push(Line::from(vec![
                    Span::styled("  ⚠ ", Style::default().fg(ERROR_COLOR)),
                    Span::styled(
                        format!("Context at {:.0}% — start a new conversation soon.", pct),
                        Style::default().fg(ERROR_COLOR),
                    ),
                ]));
                lines.push(Line::raw(""));
            } else if pct >= 80.0 {
                lines.push(Line::from(vec![
                    Span::styled("  ⚠ ", Style::default().fg(Color::Rgb(249, 226, 175))),
                    Span::styled(
                        format!("Context at {:.0}%.", pct),
                        Style::default().fg(Color::Rgb(249, 226, 175)),
                    ),
                ]));
                lines.push(Line::raw(""));
            }
        }
    }

    let total_lines = lines.len().min(u16::MAX as usize) as u16;
    app.content_lines = total_lines;

    let visible_height = msg_area.height.saturating_sub(2);
    if app.auto_scroll {
        app.scroll = total_lines.saturating_sub(visible_height);
    } else if let Some(sel) = selected_line {
        // keep selected block in view
        if sel < app.scroll {
            app.scroll = sel.saturating_sub(1);
        } else if sel >= app.scroll + visible_height {
            app.scroll = sel + 1 - visible_height;
        }
        app.scroll = app.scroll.min(total_lines.saturating_sub(1));
    } else {
        app.scroll = app.scroll.min(total_lines.saturating_sub(1));
    }

    let msg_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if history_mode { ACCENT } else { SURFACE }))
        .style(Style::default().bg(BG));

    let messages_widget = Paragraph::new(Text::from(lines))
        .block(msg_block)
        .scroll((app.scroll, 0));
    frame.render_widget(messages_widget, msg_area);

    // --- input ---
    let ask_text_title: Option<String> = app.ask.as_ref().and_then(|a| {
        if let AskKind::Text = &a.kind {
            let q: String = a.question.chars().take(50).collect();
            Some(format!(" ⬡ {q} "))
        } else { None }
    });
    let input_border_color = if app.stream_state == StreamState::Streaming || app.chat_focus != ChatFocus::Input { DIM } else { ACCENT };
    let input_block = {
        let b = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(input_border_color))
            .style(Style::default().bg(BG));
        if let Some(ref t) = ask_text_title {
            b.title(Span::styled(t.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
        } else { b }
    };

    let input_widget = if app.input.is_empty() && app.stream_state == StreamState::Streaming {
        Paragraph::new(Line::from(Span::styled("Waiting for response…", Style::default().fg(DIM))))
            .block(input_block)
    } else {
        // Build visual lines: each logical line (split by \n) is chunked into
        // slices of input_inner_width so long lines wrap rather than getting clipped.
        let w = (input_inner_width as usize).max(1);
        let mut text_lines: Vec<Line> = Vec::new();
        let mut first = true;
        for logical_line in app.input.split('\n') {
            let chars: Vec<char> = logical_line.chars().collect();
            if chars.is_empty() {
                let prefix = if first { Span::styled("> ", Style::default().fg(ACCENT)) }
                             else     { Span::styled("  ", Style::default().fg(ACCENT)) };
                text_lines.push(Line::from(vec![prefix]));
                first = false;
            } else {
                for chunk in chars.chunks(w) {
                    let s: String = chunk.iter().collect();
                    let prefix = if first { Span::styled("> ", Style::default().fg(ACCENT)) }
                                 else     { Span::styled("  ", Style::default().fg(ACCENT)) };
                    let mut spans = vec![prefix];
                    spans.extend(highlight_at_refs(&s));
                    text_lines.push(Line::from(spans));
                    first = false;
                }
            }
        }

        // scroll so cursor visual row is always visible
        let (vcur_row, _) = input_visual_cursor(&app.input, app.cursor_pos, input_inner_width);
        let visible_rows = (input_height - 2) as usize;
        let input_scroll = vcur_row.saturating_sub(visible_rows.saturating_sub(1));
        if input_scroll > 0 {
            text_lines = text_lines.into_iter().skip(input_scroll).collect();
        }

        Paragraph::new(Text::from(text_lines)).block(input_block)
    };
    // --- warmup status bar ---
    if warming_up {
        if let Some(started) = app.ws_warmup_started_at.or(app.warmup_started_at) {
            let elapsed = started.elapsed().as_secs_f64();
            const SPINNER: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
            let frame_i = (elapsed / 0.1) as usize % SPINNER.len();
            let tok_hint = app.warmup_prompt_tokens
                .map(|t| format!("  ~{t} tok"))
                .unwrap_or_default();
            frame.render_widget(Paragraph::new(Line::from(vec![
                Span::styled(format!("  {} ", SPINNER[frame_i]), Style::default().fg(ACCENT)),
                Span::styled("warming up KV cache", Style::default().fg(DIM)),
                Span::styled(tok_hint, Style::default().fg(THINKING_COLOR)),
                Span::styled(format!("  {}", format_duration(elapsed)), Style::default().fg(THINKING_COLOR)),
            ])), warmup_area);
        }
    }

    // --- ask widget ---
    if let Some(ref ask) = app.ask {
        match &ask.kind {
            AskKind::Text => {} // handled via input box title below
            AskKind::Confirm => {
                frame.render_widget(Paragraph::new(Line::from(vec![
                    Span::styled("  ⬡  ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::styled(ask.question.clone(), Style::default().fg(Color::White)),
                ])).style(Style::default().bg(BG)), ask_area);
            }
            AskKind::Choice(options) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ACCENT))
                    .style(Style::default().bg(BG));
                let inner = Rect {
                    x: ask_area.x + 1,
                    y: ask_area.y + 1,
                    width: ask_area.width.saturating_sub(2),
                    height: ask_area.height.saturating_sub(2),
                };
                frame.render_widget(block, ask_area);
                for (i, opt) in options.iter().enumerate().take(8) {
                    let selected = i == app.ask_cursor;
                    let (marker, opt_style) = if selected {
                        ("● ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
                    } else {
                        ("○ ", Style::default().fg(Color::White))
                    };
                    let marker_style = if selected { Style::default().fg(ACCENT) } else { Style::default().fg(DIM) };
                    frame.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(format!("  {marker}"), marker_style),
                        Span::styled(opt.clone(), opt_style),
                    ])), Rect { x: inner.x, y: inner.y + i as u16, width: inner.width, height: 1 });
                }
            }
        }
    }

    frame.render_widget(input_widget, input_area);

    // place cursor in 2D using visual coordinates
    let show_cursor = app.stream_state != StreamState::Streaming
        && !history_mode
        && app.ask.as_ref().map(|a| matches!(&a.kind, AskKind::Text)).unwrap_or(true);
    if show_cursor {
        let (vcur_row, vcur_col) = input_visual_cursor(&app.input, app.cursor_pos, input_inner_width);
        let visible_rows = (input_height - 2) as usize;
        let input_scroll = vcur_row.saturating_sub(visible_rows.saturating_sub(1));
        let visual_row = vcur_row - input_scroll;
        let prefix_len: u16 = 2;
        let x = input_area.x + 1 + prefix_len + vcur_col as u16;
        let y = input_area.y + 1 + visual_row as u16;
        frame.set_cursor_position((x.min(input_area.x + input_area.width - 2), y));
    }

    // --- completion popup ---
    if app.completion.is_some() {
        draw_completion_popup(frame, app, input_area);
    }

    // --- file picker popup ---
    if app.file_picker.is_some() {
        draw_file_picker(frame, app, msg_area);
    }

    // --- file panel (right side) ---
    if let Some(p_area) = panel_area_opt {
        draw_file_panel(frame, app, p_area);
    }

    // --- hints ---
    let hints_line = if let Some(ref ask) = app.ask {
        match &ask.kind {
            AskKind::Text    => Line::from(vec![hint("Enter", "submit"), Span::raw("  "), hint("Esc", "cancel")]),
            AskKind::Confirm => Line::from(vec![hint("y/Enter", "Yes"),  Span::raw("  "), hint("Esc/n", "No")]),
            AskKind::Choice(_) => Line::from(vec![hint("↑/↓", "navigate"), Span::raw("  "), hint("Enter", "select"), Span::raw("  "), hint("Esc", "cancel")]),
        }
    } else if app.chat_focus == ChatFocus::FilePanel {
        Line::from(vec![
            hint("PgUp/Dn", "scroll"),
            Span::raw("  "),
            hint("q/Esc", "close"),
            Span::raw("  "),
            hint("Tab", "→ input"),
        ])
    } else if history_mode {
        if app.file_panel.is_some() && app.file_panel_visible {
            Line::from(vec![
                hint("↑/↓", "navigate"),
                Span::raw("  "),
                hint("Enter", "cycle file"),
                Span::raw("  "),
                hint("PgUp/Dn", "scroll chat"),
                Span::raw("  "),
                hint("q", "close preview"),
                Span::raw("  "),
                hint("Tab", "→ preview  "),
                hint("Esc", "→ input"),
            ])
        } else {
            Line::from(vec![
                hint("↑/↓", "navigate"),
                Span::raw("  "),
                hint("Enter", "open file"),
                Span::raw("  "),
                hint("Ctrl+Y", "copy block"),
                Span::raw("  "),
                hint("Tab/Esc", "→ input"),
            ])
        }
    } else {
        let esc_label = if app.stream_state == StreamState::Streaming { "stop" } else { "models" };
        Line::from(vec![
            hint("Esc", esc_label),
            Span::raw("  "),
            hint("Ctrl+J", "room"),
            Span::raw("  "),
            hint("Enter", "send"),
            Span::raw("  "),
            hint("Ctrl+N", "newline"),
            Span::raw("  "),
            hint("Tab", "browse history"),
            Span::raw("  "),
            hint("Shift+Tab", "mode"),
            Span::raw("  "),
            hint("Ctrl+T", "think budget"),
            Span::raw("  "),
            hint("F1", "help"),
        ])
    };
    let hints = Paragraph::new(hints_line).style(Style::default().fg(DIM));
    frame.render_widget(hints, hints_area);

    // --- help popup ---
    if app.show_help {
        const SECTIONS: &[(&str, &[(&str, &str)])] = &[
            ("Sending", &[
                ("Enter",               "Send message"),
                ("Ctrl+N",              "Insert newline"),
                ("Esc",                 "Stop stream / back to model select"),
            ]),
            ("Cursor movement", &[
                ("←  →",                "Move one character"),
                ("Ctrl+←  →  /  Alt+←  →", "Move one word"),
                ("Ctrl+A  /  Home",     "Beginning of line"),
                ("Ctrl+E  /  End",      "End of line"),
                ("↑  ↓",                "Move lines (multi-line input) / browse send history"),
            ]),
            ("Editing", &[
                ("Backspace",           "Delete character before cursor"),
                ("Delete",              "Delete character after cursor"),
                ("Ctrl+W",              "Delete word before cursor"),
                ("Ctrl+K",              "Delete to end of line"),
                ("Ctrl+U",              "Delete to start of line"),
            ]),
            ("Scrolling", &[
                ("PgUp  /  PgDn",       "Scroll chat (input/history) or file panel (panel focus)"),
                ("Alt+↑  ↓",            "Scroll chat one line"),
                ("Ctrl+End",            "Jump to bottom"),
            ]),
            ("History mode  (Tab)", &[
                ("Tab",                 "Enter history mode — navigate message blocks"),
                ("↑  /  ↓",             "Previous / next message block"),
                ("Enter",               "Open or cycle file attachments in side panel"),
                ("Ctrl+Y",              "Copy selected block to clipboard"),
                ("q",                   "Close file panel"),
                ("Tab",                 "→ file panel (when visible)"),
                ("Esc",                 "→ input"),
            ]),
            ("File panel", &[
                ("Tab (history)",       "Focus file panel"),
                ("PgUp  /  PgDn",       "Scroll file panel"),
                ("Ctrl+B",              "Hide / show file panel (preserves loaded file)"),
                ("q  /  Esc",           "Close file panel"),
                ("Tab",                 "→ input"),
            ]),
            ("Context & files", &[
                ("@path",               "Attach a file or image inline"),
                ("/template",           "Insert a prompt template"),
                ("Ctrl+P",              "Open file picker — pin files to system prompt"),
            ]),
            ("Chat", &[
                ("Ctrl+Y  (input)",     "Copy last assistant response"),
                ("Ctrl+L",              "Clear conversation"),
                ("Ctrl+S",              "Export chat to JSON"),
                ("Ctrl+O",              "Import chat from JSON"),
                ("Ctrl+C",              "Quit"),
                ("F1",                  "Toggle this help"),
            ]),
            ("Modes  (Shift+Tab to cycle)", &[
                ("Shift+Tab",           "Cycle: normal → plan → auto-accept → normal"),
                ("normal",              "Default — asks permission before patches and confirms"),
                ("plan",                "Model describes plan only, no tool or patch execution"),
                ("auto-accept",         "Patches and confirm asks applied automatically"),
            ]),
            ("Rooms (multi-user)", &[
                ("Ctrl+J",              "Show room UUID + shareable URL"),
                ("y  (in popup)",       "Copy URL to clipboard"),
                ("—",                   "Others join via ws://your-ip:8765/id/{uuid}"),
                ("—",                   "Joiners see full history + live token stream"),
                ("—",                   "Each participant shown as  username#id"),
                ("#name#id  /  #all",   "Mention a participant — triggers an AI response"),
                ("▸ username …",        "Live typing indicator while another user streams"),
            ]),
        ];
        draw_help_popup(frame, app, area, SECTIONS);
    }

    // ── Room share popup ────────────────────────────────────────────────────
    if app.show_room_share {
        let popup_w = area.width.min(70);
        let popup_h = 6u16;
        let px = area.x + (area.width.saturating_sub(popup_w)) / 2;
        let py = area.y + (area.height.saturating_sub(popup_h)) / 2;
        let popup_area = Rect { x: px, y: py, width: popup_w, height: popup_h };
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        let block = Block::default()
            .title(Span::styled(" Sala ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let room_id_owned = app.room_id.clone();
        let lines = if let Some(ref room_id) = room_id_owned {
            let share_url = app.room_share_url()
                .unwrap_or_else(|| format!("(connect via --remote first)/id/{room_id}"));
            vec![
                Line::from(vec![
                    Span::styled(" UUID:  ", Style::default().fg(DIM)),
                    Span::styled(room_id.as_str(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled(" URL:   ", Style::default().fg(DIM)),
                    Span::styled(share_url, Style::default().fg(ACCENT)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" y", Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD)),
                    Span::styled(" copy URL   ", Style::default().fg(DIM)),
                    Span::styled(" Esc", Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD)),
                    Span::styled(" close", Style::default().fg(DIM)),
                ]),
            ]
        } else {
            vec![
                Line::from(vec![
                    Span::styled(" Not in a room.", Style::default().fg(DIM)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" Connect to a server with ", Style::default().fg(DIM)),
                    Span::styled("Ctrl+R", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                    Span::styled(" on the model select screen.", Style::default().fg(DIM)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" Esc", Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD)),
                    Span::styled(" close", Style::default().fg(DIM)),
                ]),
            ]
        };
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn draw_file_panel(frame: &mut Frame, app: &App, area: Rect) {
    let fp = match &app.file_panel { Some(f) => f, None => return };

    let show_reloaded = fp.reloaded_at
        .map(|t| t.elapsed() < std::time::Duration::from_secs(2))
        .unwrap_or(false);

    let mut title_spans = vec![
        Span::styled(format!(" {} ", fp.display_path), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ];
    if show_reloaded {
        title_spans.push(Span::styled("↺ ", Style::default().fg(USER_COLOR)));
    }

    let focused = app.chat_focus == crate::app::ChatFocus::FilePanel;
    let border_color = if focused { ACCENT } else { DIM };
    let hint = if focused {
        " PgUp/PgDn scroll  q/Esc close  Tab → input "
    } else {
        " Tab to focus  q close "
    };
    let block = Block::default()
        .title(Line::from(title_spans))
        .title_bottom(Span::styled(hint, Style::default().fg(DIM)))
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Rounded } else { BorderType::Plain })
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(BG));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let start = fp.scroll.min(fp.lines.len());
    let visible = inner.height as usize;
    let lines: Vec<Line> = fp.lines[start..].iter().take(visible).cloned().collect();
    frame.render_widget(Paragraph::new(lines).scroll((0, fp.h_scroll as u16)), inner);
}

fn draw_file_picker(frame: &mut Frame, app: &App, area: Rect) {
    let fp = match &app.file_picker { Some(f) => f, None => return };
    let entries = app.file_picker_visible();

    let popup_width  = (area.width * 9 / 10).min(area.width);
    let popup_height = (area.height * 4 / 5).min(area.height);
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_rect = Rect { x: popup_x, y: popup_y, width: popup_width, height: popup_height };

    let is_remote = app.ws_tx.is_some();
    let title = if is_remote { " 📎 attach file (host) " } else { " 📎 pin files " };
    let hints = if is_remote {
        " ↑↓ navigate  Enter/→ select  ← up  Esc close "
    } else {
        " ↑↓ navigate  Enter/→ enter/pin  ← up  PgUp/PgDn scroll preview  Esc close "
    };

    frame.render_widget(Clear, popup_rect);
    frame.render_widget(
        Block::default()
            .title(Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
            .title_bottom(Span::styled(hints, Style::default().fg(DIM)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG)),
        popup_rect,
    );

    let inner = Rect {
        x: popup_rect.x + 1,
        y: popup_rect.y + 1,
        width: popup_rect.width.saturating_sub(2),
        height: popup_rect.height.saturating_sub(2),
    };

    let [browser_area, div_area, preview_area] = Layout::horizontal([
        Constraint::Percentage(35),
        Constraint::Length(1),
        Constraint::Fill(1),
    ]).areas(inner);

    // ── browser (left) ──────────────────────────────────────────────────────

    let rel_dir = if is_remote {
        fp.current_dir.to_string_lossy().to_string()
    } else {
        fp.current_dir.strip_prefix(&app.working_dir)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    };
    let path_label = if rel_dir.is_empty() || rel_dir == "." { "./".to_string() } else { format!("{rel_dir}/") };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("📁 ", Style::default().fg(ACCENT)),
            Span::styled(path_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ])),
        Rect { x: browser_area.x, y: browser_area.y, width: browser_area.width, height: 1 },
    );

    let filter_line = if fp.query.is_empty() {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(DIM)),
            Span::styled("filter...", Style::default().fg(THINKING_COLOR)),
        ])
    } else {
        Line::from(vec![
            Span::styled("/ ", Style::default().fg(ACCENT)),
            Span::styled(fp.query.clone(), Style::default().fg(Color::White)),
        ])
    };
    frame.render_widget(
        Paragraph::new(filter_line),
        Rect { x: browser_area.x, y: browser_area.y + 1, width: browser_area.width, height: 1 },
    );

    let list_y = browser_area.y + 2;
    let list_h = browser_area.height.saturating_sub(2) as usize;

    if fp.loading {
        frame.render_widget(
            Paragraph::new(Span::styled("loading…", Style::default().fg(DIM))),
            Rect { x: browser_area.x, y: list_y, width: browser_area.width, height: 1 },
        );
    }

    let total  = entries.len();
    let scroll = if fp.cursor >= list_h { fp.cursor + 1 - list_h } else { 0 };

    for (i, entry) in entries[scroll..].iter().take(list_h).enumerate() {
        let global_idx = scroll + i;
        let selected   = global_idx == fp.cursor;
        let row = Rect { x: browser_area.x, y: list_y + i as u16, width: browser_area.width, height: 1 };

        let line = match entry {
            FilePickerEntry::Parent => {
                if selected {
                    Line::from(Span::styled("↑ ../", Style::default().fg(BG).bg(ACCENT).add_modifier(Modifier::BOLD)))
                } else {
                    Line::from(Span::styled("↑ ../", Style::default().fg(DIM)))
                }
            }
            FilePickerEntry::Dir(name) => {
                if selected {
                    Line::from(vec![
                        Span::styled("📁 ", Style::default().fg(BG).bg(ACCENT)),
                        Span::styled(format!("{name}/"), Style::default().fg(BG).bg(ACCENT).add_modifier(Modifier::BOLD)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("📁 ", Style::default().fg(ACCENT)),
                        Span::styled(format!("{name}/"), Style::default().fg(Color::White)),
                    ])
                }
            }
            FilePickerEntry::File(display) => {
                let pinned = app.pinned_files.iter().any(|pf| &pf.display == display);
                let name   = display.split('/').last().unwrap_or(display);
                if selected {
                    Line::from(vec![
                        Span::styled(if pinned { "● " } else { "○ " }, Style::default().fg(BG).bg(ACCENT)),
                        Span::styled(name.to_string(), Style::default().fg(BG).bg(ACCENT).add_modifier(Modifier::BOLD)),
                    ])
                } else if pinned {
                    Line::from(vec![
                        Span::styled("● ", Style::default().fg(USER_COLOR)),
                        Span::styled(name.to_string(), Style::default().fg(Color::White)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("○ ", Style::default().fg(DIM)),
                        Span::styled(name.to_string(), Style::default().fg(Color::White)),
                    ])
                }
            }
        };
        frame.render_widget(Paragraph::new(line), row);
    }

    if total == 0 {
        frame.render_widget(
            Paragraph::new(Span::styled("no files here", Style::default().fg(DIM))),
            Rect { x: browser_area.x, y: list_y, width: browser_area.width, height: 1 },
        );
    }

    // ── divider ─────────────────────────────────────────────────────────────
    for dy in 0..div_area.height {
        frame.render_widget(
            Paragraph::new(Span::styled("│", Style::default().fg(DIM))),
            Rect { x: div_area.x, y: div_area.y + dy, width: 1, height: 1 },
        );
    }

    // ── preview (right) ──────────────────────────────────────────────────────
    if fp.preview.is_empty() {
        let placeholder = if matches!(entries.get(fp.cursor), Some(FilePickerEntry::Dir(_)) | Some(FilePickerEntry::Parent)) {
            "  select a file to preview"
        } else {
            "  no preview"
        };
        let mid = preview_area.y + preview_area.height / 2;
        frame.render_widget(
            Paragraph::new(Span::styled(placeholder, Style::default().fg(DIM))),
            Rect { x: preview_area.x, y: mid, width: preview_area.width, height: 1 },
        );
    } else {
        let visible_lines = preview_area.height as usize;
        let scroll = fp.preview_scroll;
        let lines: Vec<Line> = fp.preview[scroll..]
            .iter()
            .take(visible_lines)
            .cloned()
            .collect();
        frame.render_widget(Paragraph::new(lines), preview_area);
    }
}

fn draw_completion_popup(frame: &mut Frame, app: &App, input_area: Rect) {
    let comp = match &app.completion {
        Some(c) => c,
        None => return,
    };

    const MAX_VISIBLE: usize = 8;
    let total = comp.candidates.len();
    let visible = total.min(MAX_VISIBLE);

    // scroll window so the selected item is always visible
    let scroll = if comp.cursor >= visible {
        comp.cursor + 1 - visible
    } else {
        0
    };

    // compute popup width from longest visible candidate
    let max_name_len = comp.candidates[scroll..scroll + visible]
        .iter()
        .map(|s| display_name(s).chars().count())
        .max()
        .unwrap_or(10);
    let popup_width = (max_name_len as u16 + 6).min(60).max(24).min(input_area.width);
    let popup_height = visible as u16 + 2; // +2 for borders

    // position: above the input box, left-aligned
    let popup_y = input_area.y.saturating_sub(popup_height);
    let popup_rect = Rect {
        x: input_area.x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };

    let is_template = comp.kind == CompletionKind::Template;
    let items: Vec<ListItem> = comp.candidates[scroll..scroll + visible]
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let global_idx = scroll + i;
            let name = if is_template {
                candidate.clone()
            } else {
                display_name(candidate)
            };
            let is_dir = !is_template && candidate.ends_with('/');
            let selected = global_idx == comp.cursor;

            if selected {
                ListItem::new(Line::from(vec![
                    Span::styled(" ", Style::default().bg(ACCENT)),
                    Span::styled(
                        format!("{name} "),
                        Style::default()
                            .fg(BG)
                            .bg(ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
            } else if is_dir {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(name, Style::default().fg(ACCENT)),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(name, Style::default().fg(Color::White)),
                ]))
            }
        })
        .collect();

    let scroll_hint = if total > MAX_VISIBLE {
        format!(" {}/{} ", comp.cursor + 1, total)
    } else {
        String::new()
    };

    let block_title = if is_template { " / templates " } else { "" };
    let block = Block::default()
        .title(Span::styled(block_title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(scroll_hint, Style::default().fg(DIM)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(SURFACE));

    // Clear the area first so the popup doesn't bleed through messages
    frame.render_widget(Clear, popup_rect);
    frame.render_widget(List::new(items).block(block), popup_rect);
}

fn draw_help_popup(frame: &mut Frame, app: &App, area: Rect, sections: &[(&str, &[(&str, &str)])]) {
    // build lines
    let mut lines: Vec<Line> = vec![Line::raw("")];
    for (section, entries) in sections {
        lines.push(Line::from(Span::styled(
            format!("  {section}"),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        for (key, desc) in *entries {
            lines.push(Line::from(vec![
                Span::styled(format!("  {key:<22}", key = key), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(desc.to_string(), Style::default().fg(DIM)),
            ]));
        }
        lines.push(Line::raw(""));
    }

    let ideal_h = (1 + sections.iter().map(|(_, e)| 2 + e.len()).sum::<usize>() + 2) as u16;
    let popup_w = 58u16.min(area.width.saturating_sub(4));
    let popup_h = ideal_h.min(area.height.saturating_sub(4)).max(6);
    let popup_rect = Rect {
        x: area.x + (area.width.saturating_sub(popup_w)) / 2,
        y: area.y + (area.height.saturating_sub(popup_h)) / 2,
        width: popup_w,
        height: popup_h,
    };

    let total_lines = lines.len() as u16;
    let visible = popup_h.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(visible);
    let scroll = app.help_scroll.min(max_scroll);

    let scroll_hint = if total_lines > visible {
        format!(" {}/{} ", scroll + 1, total_lines)
    } else {
        String::new()
    };

    let block = Block::default()
        .title(Span::styled(" Keyboard shortcuts ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
        .title_bottom(Span::styled(
            format!("{scroll_hint}  [F1/Esc] close  [↑↓] scroll"),
            Style::default().fg(DIM),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));

    frame.render_widget(Clear, popup_rect);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(block).scroll((scroll, 0)),
        popup_rect,
    );
}

/// Returns the display name for a completion candidate.
/// For bare names (no slash in the middle) returns as-is.
/// For paths like "src/app.rs" returns just "app.rs" (with dir dimmed separately).
fn display_name(candidate: &str) -> String {
    // strip trailing slash for directory label display
    let c = candidate.trim_end_matches('/');
    let suffix = if candidate.ends_with('/') { "/" } else { "" };
    if let Some(slash) = c.rfind('/') {
        format!("{}{suffix}", &c[slash + 1..])
    } else {
        format!("{c}{suffix}")
    }
}

/// Returns the visual (row, col) of `cursor_pos` within `input`,
/// accounting for both explicit newlines and wrap at `width` columns.
fn input_visual_cursor(input: &str, cursor_pos: usize, width: u16) -> (usize, usize) {
    let w = if width == 0 { return (0, 0); } else { width as usize };
    let byte = input.char_indices().nth(cursor_pos).map(|(b, _)| b).unwrap_or(input.len());
    let before = &input[..byte];
    let logical_lines: Vec<&str> = before.split('\n').collect();
    let last = logical_lines.last().copied().unwrap_or("");
    let mut vrow = 0usize;
    for line in &logical_lines[..logical_lines.len().saturating_sub(1)] {
        let len = line.chars().count();
        vrow += if len == 0 { 1 } else { (len + w - 1) / w };
    }
    let last_len = last.chars().count();
    vrow += last_len / w;
    let vcol = last_len % w;
    (vrow, vcol)
}

/// Counts how many terminal rows the input string will occupy given a fixed column width,
/// accounting for both explicit newlines and visual word-wrap.
fn visual_line_count(input: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    let w = width as usize;
    input
        .split('\n')
        .map(|line| {
            let len = line.chars().count();
            if len == 0 { 1u16 } else { ((len + w - 1) / w) as u16 }
        })
        .sum::<u16>()
        .max(1)
}

fn hint<'a>(key: &'a str, desc: &'a str) -> Span<'a> {
    Span::raw(format!("[{key}] {desc}"))
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    }
}

fn format_duration(secs: f64) -> String {
    if secs < 60.0 {
        format!("{:.1}s", secs)
    } else if secs < 3600.0 {
        let m = secs as u64 / 60;
        let s = secs as u64 % 60;
        format!("{}m {}s", m, s)
    } else {
        let h = secs as u64 / 3600;
        let m = (secs as u64 % 3600) / 60;
        format!("{}h {}m", h, m)
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut result = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_len = 0;
        for word in line.split_whitespace() {
            let word_len = word.chars().count();
            if !current.is_empty() && current_len + 1 + word_len > width {
                result.push(current.clone());
                current.clear();
                current_len = 0;
            }
            if !current.is_empty() {
                current.push(' ');
                current_len += 1;
            }
            current.push_str(word);
            current_len += word_len;
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

fn render_assistant_content(
    lines: &mut Vec<Line>,
    thinking: &str,
    content: &str,
    width: usize,
    streaming: bool,
    thought_secs: Option<f64>,
) {
    // thinking block (only present in models that support it)
    if !thinking.is_empty() {
        let actively_thinking = streaming && content.is_empty();
        let label = if actively_thinking {
            "  thinking…".to_string()
        } else if let Some(secs) = thought_secs {
            format!("  thought for {}", format_duration(secs))
        } else {
            "  thought".to_string()
        };
        lines.push(Line::from(Span::styled(
            label,
            Style::default().fg(THINKING_COLOR).add_modifier(Modifier::ITALIC),
        )));
        // only expand thinking text while actively streaming, or when we have timing info
        if actively_thinking || thought_secs.is_some() {
            for text_line in wrap_text(thinking.trim(), width) {
                lines.push(Line::from(Span::styled(
                    format!("  {text_line}"),
                    Style::default().fg(THINKING_COLOR),
                )));
            }
            if actively_thinking {
                if let Some(last) = lines.last_mut() {
                    last.spans.push(Span::styled("▋", Style::default().fg(THINKING_COLOR)));
                }
            }
        }
        lines.push(Line::raw(""));
    }

    // regular response content — split into text/code segments
    if !content.is_empty() {
        let stripped = strip_think_blocks(content.trim());
        let segments = parse_content_segments(&stripped);
        for seg in &segments {
            match seg {
                ContentSegment::Text(text) => {
                    render_markdown_text(lines, text.trim_matches('\n'), width);
                }
                ContentSegment::Code { lang, content: code } => {
                    lines.extend(render_code_block(lang, code, width));
                }
            }
        }
        if streaming {
            if let Some(last) = lines.last_mut() {
                last.spans.push(Span::styled("▋", Style::default().fg(DIM)));
            }
        }
    } else if thinking.is_empty() {
        lines.push(Line::from(Span::styled("  ▋", Style::default().fg(DIM))));
    }
}

enum ContentSegment {
    Text(String),
    Code { lang: String, content: String },
}

fn strip_think_blocks(content: &str) -> String {
    const PAIRS: &[(&str, &str)] = &[("<think>", "</think>"), ("<thought>", "</thought>")];
    let mut out = String::new();
    let mut rest = content;
    loop {
        let earliest = PAIRS.iter()
            .filter_map(|(open, _)| rest.find(open).map(|i| (i, *open)))
            .min_by_key(|(i, _)| *i);
        match earliest {
            None => { out.push_str(rest); break; }
            Some((start, open)) => {
                out.push_str(&rest[..start]);
                let close = PAIRS.iter().find(|(o, _)| *o == open).map(|(_, c)| *c).unwrap();
                match rest[start..].find(close) {
                    Some(end) => { rest = rest[start + end + close.len()..].trim_start_matches('\n'); }
                    None => break, // unclosed — skip remainder while streaming
                }
            }
        }
    }
    out
}

fn parse_content_segments(content: &str) -> Vec<ContentSegment> {
    let mut segments = Vec::new();
    let mut rest = content;

    while !rest.is_empty() {
        match rest.find("```") {
            Some(start) => {
                if start > 0 {
                    segments.push(ContentSegment::Text(rest[..start].to_string()));
                }
                rest = &rest[start + 3..];
                let lang_end = rest.find('\n').unwrap_or(rest.len());
                let lang = rest[..lang_end].trim().to_string();
                rest = if lang_end < rest.len() { &rest[lang_end + 1..] } else { "" };

                match rest.find("```") {
                    Some(end) => {
                        segments.push(ContentSegment::Code { lang, content: rest[..end].to_string() });
                        rest = &rest[end + 3..];
                        if rest.starts_with('\n') { rest = &rest[1..]; }
                    }
                    None => {
                        // still streaming — unclosed block
                        segments.push(ContentSegment::Code { lang, content: rest.to_string() });
                        rest = "";
                    }
                }
            }
            None => {
                segments.push(ContentSegment::Text(rest.to_string()));
                rest = "";
            }
        }
    }
    segments
}

fn render_code_block(lang: &str, code: &str, width: usize) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    out.push(Line::raw(""));
    let label = if lang.is_empty() { "code" } else { lang };
    out.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(label.to_string(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ]));

    let is_diff = lang == "diff"
        || (code.contains("\n+") && code.contains("\n-") && code.contains("@@"));

    if is_diff {
        for code_line in code.trim_matches('\n').lines() {
            let truncated: String = code_line.chars().take(width.saturating_sub(4)).collect();
            let (gutter, gutter_style, text_style) =
                if code_line.starts_with('+') && !code_line.starts_with("+++") {
                    ("  + ", Style::default().fg(USER_COLOR), Style::default().fg(USER_COLOR))
                } else if code_line.starts_with('-') && !code_line.starts_with("---") {
                    ("  - ", Style::default().fg(ERROR_COLOR), Style::default().fg(ERROR_COLOR))
                } else if code_line.starts_with("@@") {
                    ("  ▎ ", Style::default().fg(ACCENT), Style::default().fg(ACCENT))
                } else if code_line.starts_with("---") || code_line.starts_with("+++") {
                    ("  ▎ ", Style::default().fg(DIM), Style::default().fg(DIM))
                } else {
                    ("  ▎ ", Style::default().fg(CODE_BORDER), Style::default().fg(CODE_FG))
                };
            out.push(Line::from(vec![
                Span::styled(gutter, gutter_style),
                Span::styled(truncated, text_style),
            ]));
        }
    } else {
        for hl_line in crate::app::highlight_code(code.trim_matches('\n'), lang) {
            let mut spans = vec![Span::styled("  ▎ ", Style::default().fg(CODE_BORDER))];
            spans.extend(hl_line.spans);
            out.push(Line::from(spans));
        }
    }

    out.push(Line::raw(""));
    out
}

// Renders a markdown text block (non-code) line by line with inline formatting.
fn render_markdown_text(lines: &mut Vec<Line>, text: &str, width: usize) {
    for orig in text.split('\n') {
        if orig.trim().is_empty() {
            lines.push(Line::raw(""));
            continue;
        }

        // headings
        let (hashes, rest) = count_heading(orig);
        if hashes > 0 {
            let mut spans = vec![Span::raw("  ")];
            spans.extend(inline_md(rest));
            let line = Line::from(spans).style(
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            );
            lines.push(line);
            continue;
        }

        // list items: -, *, •, and numbered (1. 2.)
        let (bullet, item_text) = detect_list_item(orig);
        let indent = if bullet.is_some() { width.saturating_sub(4) } else { width };

        let wrapped = wrap_text(item_text, indent);
        for (i, chunk) in wrapped.iter().enumerate() {
            let mut spans: Vec<Span> = Vec::new();
            if i == 0 {
                if let Some(ref b) = bullet {
                    spans.push(Span::styled(b.clone(), Style::default().fg(ACCENT)));
                } else {
                    spans.push(Span::raw("  "));
                }
            } else {
                // continuation indent
                spans.push(Span::raw(if bullet.is_some() { "      " } else { "  " }));
            }
            spans.extend(inline_md(chunk));
            lines.push(Line::from(spans));
        }
    }
}

fn count_heading(line: &str) -> (usize, &str) {
    let n = line.chars().take_while(|&c| c == '#').count();
    if n > 0 && n <= 3 && line[n..].starts_with(' ') {
        (n, &line[n + 1..])
    } else {
        (0, line)
    }
}

fn detect_list_item(line: &str) -> (Option<String>, &str) {
    if line.starts_with("- ") || line.starts_with("* ") {
        return (Some("  • ".to_string()), &line[2..]);
    }
    if line.starts_with("  - ") || line.starts_with("  * ") {
        return (Some("    ◦ ".to_string()), &line[4..]);
    }
    // numbered: "1. ", "12. " etc.
    let digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let after = &line[digits.len()..];
        if after.starts_with(". ") {
            let num = format!("  {}. ", digits);
            return (Some(num), &after[2..]);
        }
    }
    (None, line)
}

// Replaces @/long/path/to/file.rs with @file.rs for cleaner display
fn clean_at_refs(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            if w.starts_with('@') && w.len() > 1 {
                let path = &w[1..];
                let filename = std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path);
                format!("@{filename}")
            } else {
                w.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// Returns spans with @path tokens highlighted in ACCENT color
fn highlight_at_refs(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    for word in text.split_inclusive(' ') {
        let trimmed = word.trim_end();
        if trimmed.starts_with('@') && trimmed.len() > 1 {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(Color::White)));
                buf.clear();
            }
            let space = if word.ends_with(' ') { " " } else { "" };
            spans.push(Span::styled(
                format!("{trimmed}{space}"),
                Style::default().fg(ACCENT),
            ));
        } else {
            buf.push_str(word);
        }
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, Style::default().fg(Color::White)));
    }
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

// Parses inline markdown: **bold**, *italic*, `code`, __bold__, _italic_
fn inline_md(text: &str) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                spans.push(Span::styled(buf.clone(), Style::default().fg(Color::White)));
                buf.clear();
            }
        };
    }

    while i < chars.len() {
        // **bold** or __bold__
        if i + 1 < chars.len()
            && ((chars[i] == '*' && chars[i + 1] == '*')
                || (chars[i] == '_' && chars[i + 1] == '_'))
        {
            let marker = chars[i];
            flush!();
            i += 2;
            let mut inner = String::new();
            while i < chars.len() {
                if i + 1 < chars.len() && chars[i] == marker && chars[i + 1] == marker {
                    i += 2;
                    break;
                }
                inner.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(
                inner,
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        // *italic* or _italic_  (single)
        if (chars[i] == '*' || chars[i] == '_')
            && i + 1 < chars.len()
            && chars[i + 1] != ' '
        {
            let marker = chars[i];
            flush!();
            i += 1;
            let mut inner = String::new();
            while i < chars.len() && chars[i] != marker {
                inner.push(chars[i]);
                i += 1;
            }
            if i < chars.len() { i += 1; }
            spans.push(Span::styled(
                inner,
                Style::default().fg(Color::White).add_modifier(Modifier::ITALIC),
            ));
            continue;
        }

        // `inline code`
        if chars[i] == '`' {
            flush!();
            i += 1;
            let mut inner = String::new();
            while i < chars.len() && chars[i] != '`' {
                inner.push(chars[i]);
                i += 1;
            }
            if i < chars.len() { i += 1; }
            spans.push(Span::styled(inner, Style::default().fg(CODE_FG)));
            continue;
        }

        // #mention or #name#session_id
        if chars[i] == '#' {
            let start = i + 1;
            let mut end = start;
            let mut seen_inner_hash = false;
            while end < chars.len() {
                let c = chars[end];
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    end += 1;
                } else if c == '#' && !seen_inner_hash && end > start {
                    // allow one embedded # for name#session_id format
                    seen_inner_hash = true;
                    end += 1;
                } else {
                    break;
                }
            }
            // don't render a bare '#' or trailing '#'
            if end > start && chars[end - 1] != '#' {
                flush!();
                let name: String = chars[start..end].iter().collect();
                // color based on base name only so same-named participants share a hue family
                let base = name.split('#').next().unwrap_or(&name);
                let color = crate::app::username_color(&base.to_ascii_lowercase());
                spans.push(Span::styled(
                    format!("#{name}"),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ));
                i = end;
                continue;
            }
        }

        buf.push(chars[i]);
        i += 1;
    }
    flush!();
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}
