use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use crate::app::{App, CtxStrategy, Role, Screen, StreamState};

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
        Screen::Config      => draw_config(frame, app),
        Screen::ModelSelect => draw_model_select(frame, app),
        Screen::Chat        => draw_chat(frame, app),
    }
}

// Centers a row to max 52 columns — for content boxes and title.
fn centered_panel(row: Rect) -> Rect {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Max(52), Constraint::Fill(1)])
        .split(row)[1]
}

// Renders the shared cognilite title with bottom border.
fn render_title(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled("cogni", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("lite", Style::default().fg(ASSISTANT_COLOR).add_modifier(Modifier::BOLD)),
        Span::styled("  ·  ollama TUI", Style::default().fg(DIM)),
    ]))
    .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(SURFACE)));
    frame.render_widget(title, centered_panel(area));
}

fn render_search_bar(frame: &mut Frame, area: Rect, query: &str) {
    let spans = if query.is_empty() {
        vec![
            Span::styled("  ❯ ", Style::default().fg(DIM)),
            Span::styled("type to filter", Style::default().fg(THINKING_COLOR)),
        ]
    } else {
        vec![
            Span::styled("  ❯ ", Style::default().fg(ACCENT)),
            Span::styled(query.to_owned(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().fg(ACCENT)),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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

    // content height for the active tab (+1 for search bar row)
    let content_h: u16 = match app.config_section {
        0 => 9,
        1 => app.neurons.len().max(1) as u16 + 3,
        2 => crate::app::GEN_PARAMS.len() as u16 + 3,
        _ => 6,
    };

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(3),   // title
            Constraint::Length(1),   // tab bar
            Constraint::Length(1),   // gap
            Constraint::Length(content_h),
            Constraint::Length(1),   // hints
            Constraint::Fill(1),
        ])
        .split(area);

    // ── Title ─────────────────────────────────────────────────────────────────
    render_title(frame, vert[1]);

    // ── Tab bar ───────────────────────────────────────────────────────────────
    let tabs = ["Context", "Neurons", "Generation", "Performance"];
    let mut tab_spans: Vec<Span> = Vec::new();
    for (i, name) in tabs.iter().enumerate() {
        if i > 0 { tab_spans.push(Span::styled("  ·  ", Style::default().fg(THINKING_COLOR))); }
        if i == app.config_section {
            tab_spans.push(Span::styled(*name, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));
        } else {
            tab_spans.push(Span::styled(*name, Style::default().fg(DIM)));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)).alignment(ratatui::layout::Alignment::Center), vert[2]);

    // ── Content box ───────────────────────────────────────────────────────────
    let content_area = centered_panel(vert[4]);
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));
    let inner = Rect {
        x: content_area.x + 1,
        y: content_area.y + 1,
        width: content_area.width.saturating_sub(2),
        height: content_area.height.saturating_sub(2),
    };
    frame.render_widget(content_block, content_area);

    // search bar is always the first row of inner
    render_search_bar(frame, Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 }, &app.config_search);
    let items_y = inner.y + 1;

    match app.config_section {
        0 => {
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
        1 => {
            // ── Neurons ───────────────────────────────────────────────────────
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
                frame.render_widget(Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {marker} "), bg.patch(Style::default().fg(circle_fg))),
                    Span::styled(&neuron.name, name_style),
                    Span::styled(desc, desc_style),
                ])), Rect { x: inner.x, y: items_y + row as u16, width: inner.width, height: 1 });
            }
        }
        2 => {
            // ── Generation params ─────────────────────────────────────────────
            let filtered: Vec<(usize, &(&str, &str, f64, f64, f64, f64))> = crate::app::GEN_PARAMS.iter().enumerate()
                .filter(|(_, (name, _, _, _, _, _))| crate::app::fuzzy_match(&app.config_search, name))
                .collect();
            for (row, (orig_idx, (name, desc, default, _, _, _))) in filtered.iter().enumerate() {
                let cursor = *orig_idx == app.param_cursor;
                let value = app.gen_params[*orig_idx];
                let is_default = (value - default).abs() < 0.001;
                let bg         = if cursor { Style::default().bg(SURFACE) } else { Style::default() };
                let name_style = bg.patch(if cursor { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) });
                let val_style  = bg.patch(if is_default { Style::default().fg(DIM) } else { Style::default().fg(ACCENT).add_modifier(Modifier::BOLD) });
                let dim_style  = bg.patch(Style::default().fg(THINKING_COLOR));
                frame.render_widget(Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {name:<16}"), name_style),
                    Span::styled("← ", dim_style),
                    Span::styled(format!("{value:.2}"), val_style),
                    Span::styled(" →", dim_style),
                    Span::styled(format!("  {desc}"), dim_style),
                ])), Rect { x: inner.x, y: items_y + row as u16, width: inner.width, height: 1 });
            }
        }
        _ => {
            // ── Performance ───────────────────────────────────────────────────
            struct PerfOption<'a> { label: &'a str, desc: &'a str, value: bool }
            let perf_options = [
                PerfOption { label: "Stable num_ctx",   desc: "Round context window to powers of 2 to preserve KV cache", value: app.ctx_pow2   },
                PerfOption { label: "Keep model alive", desc: "Prevent Ollama from unloading the model between requests",  value: app.keep_alive },
                PerfOption { label: "Warm-up cache",    desc: "Pre-fill KV cache with the system prompt on model load",    value: app.warmup     },
            ];
            let filtered: Vec<(usize, &PerfOption)> = perf_options.iter().enumerate()
                .filter(|(_, o)| crate::app::fuzzy_match(&app.config_search, o.label))
                .collect();
            for (row, (orig_idx, opt)) in filtered.iter().enumerate() {
                let cursor = *orig_idx == app.perf_cursor;
                let (marker, circle_fg) = if opt.value { ("●", ACCENT) } else { ("○", DIM) };
                let name_style = if cursor { Style::default().fg(Color::White).bg(SURFACE).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
                let desc_style = if cursor { Style::default().fg(DIM).bg(SURFACE) } else { Style::default().fg(DIM) };
                let bg = if cursor { Style::default().bg(SURFACE) } else { Style::default() };
                frame.render_widget(Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {marker} "), bg.patch(Style::default().fg(circle_fg))),
                    Span::styled(opt.label, name_style),
                    Span::styled(format!("  —  {}", opt.desc), desc_style),
                ])), Rect { x: inner.x, y: items_y + row as u16, width: inner.width, height: 1 });
            }
        }
    }

    // ── Hints ─────────────────────────────────────────────────────────────────
    let action_hint: Vec<Span> = if app.config_section == 2 {
        vec![hint("←/→", "adjust"), Span::raw("  "), hint("r", "reset")]
    } else {
        vec![hint("Enter", "toggle")]
    };
    let mut hint_spans = vec![hint("↑/↓", "navigate"), Span::raw("  ")];
    hint_spans.extend(action_hint);
    hint_spans.extend([Span::raw("  "), hint("type", "filter"), Span::raw("  "), hint("Tab", "next tab"), Span::raw("  "), hint("Esc", "close")]);
    render_hints(frame, vert[5], hint_spans);
}

fn draw_model_select(frame: &mut Frame, app: &App) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        area,
    );

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Length(app.models.len().max(3).min(20) as u16 + 2),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .split(area);

    render_title(frame, vert[1]);

    // model list
    let title_text = if app.model_search.is_empty() {
        " models ".to_string()
    } else {
        format!(" models  ❯ {}_ ", app.model_search)
    };
    let block = Block::default()
        .title(Span::styled(title_text, Style::default().fg(ACCENT)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));

    let list_area = vert[2];

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

    render_hints(frame, vert[3], vec![
        hint("↑/↓", "navigate"),
        Span::raw("  "),
        hint("Enter", "select"),
        Span::raw("  "),
        hint("type", "filter"),
        Span::raw("  "),
        hint("Esc", "clear filter"),
        Span::raw("  "),
        hint("Tab", "settings"),
        Span::raw("  "),
        hint("Ctrl+C", "quit"),
    ]);
}

fn draw_chat(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    // +2 for borders; account for visual wrapping (area.width - 4 = borders(2) + prefix(2))
    let input_inner_width = area.width.saturating_sub(4);
    let input_height = (visual_line_count(&app.input, input_inner_width) + 2).min(10);
    let warming_up = app.warmup_rx.is_some();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                                    // header
            Constraint::Fill(1),                                      // messages
            Constraint::Length(if warming_up { 1 } else { 0 }),      // warmup status bar
            Constraint::Length(input_height),                         // input (dynamic)
            Constraint::Length(1),                                    // hints
        ])
        .split(area);

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

    frame.render_widget(Paragraph::new(Line::from(header_spans)), chunks[0]);

    // --- messages ---
    let msg_area = chunks[1];
    let inner_width = msg_area.width.saturating_sub(4) as usize; // borders + padding

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::raw(""));

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(Span::styled(
                    "You",
                    Style::default().fg(USER_COLOR).add_modifier(Modifier::BOLD),
                )));
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
                // attachment pills
                for att in &msg.attachments {
                    let icon = match att.kind {
                        crate::app::AttachmentKind::Text => "≡",
                        crate::app::AttachmentKind::Image => "⬡",
                    };
                    let size_str = if att.size >= 1024 {
                        format!("{:.1} KB", att.size as f64 / 1024.0)
                    } else {
                        format!("{} B", att.size)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {icon} "), Style::default().fg(ACCENT)),
                        Span::styled(att.filename.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("  {size_str}"), Style::default().fg(DIM)),
                    ]));
                }
            }
            Role::Tool => {
                if msg.attachments.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("  ✗ ", Style::default().fg(ERROR_COLOR)),
                        Span::styled(msg.content.clone(), Style::default().fg(ERROR_COLOR)),
                    ]));
                } else {
                    let att = &msg.attachments[0];
                    let size_str = if att.size >= 1024 {
                        format!("{:.1} KB", att.size as f64 / 1024.0)
                    } else {
                        format!("{} B", att.size)
                    };
                    let label = msg.tool_call.as_deref().unwrap_or("tool");
                    lines.push(Line::from(vec![
                        Span::styled(format!("  ⚙ {label}  "), Style::default().fg(DIM)),
                        Span::styled(att.filename.clone(), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("  {size_str}"), Style::default().fg(DIM)),
                    ]));
                    for code_line in msg.content.lines() {
                        let truncated: String = code_line.chars().take(inner_width.saturating_sub(4)).collect();
                        lines.push(Line::from(vec![
                            Span::styled("  ▎ ", Style::default().fg(CODE_BORDER)),
                            Span::styled(truncated, Style::default().fg(CODE_FG)),
                        ]));
                    }
                }
            }
            Role::Assistant => {
                let is_last = std::ptr::eq(msg, app.messages.last().unwrap());
                let streaming = is_last && app.stream_state == StreamState::Streaming;

                // intermediate assistant messages that only held a tool call
                // have empty content — skip them entirely, the Tool result below tells the story
                if msg.content.is_empty() && msg.thinking.is_empty() && !streaming {
                    continue;
                }

                lines.push(Line::from(Span::styled(
                    "Assistant",
                    Style::default().fg(ASSISTANT_COLOR).add_modifier(Modifier::BOLD),
                )));
                if msg.content.is_empty() && msg.thinking.is_empty() {
                    // only the current streaming message reaches here
                    let label = match app.stream_started_at {
                        Some(started) => format!(
                            "  Processing… {}▋",
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

    let total_lines = lines.len() as u16;
    app.content_lines = total_lines;

    let visible_height = msg_area.height.saturating_sub(2);
    if app.auto_scroll {
        app.scroll = total_lines.saturating_sub(visible_height);
    } else {
        app.scroll = app.scroll.min(total_lines.saturating_sub(1));
    }

    let msg_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE))
        .style(Style::default().bg(BG));

    let messages_widget = Paragraph::new(Text::from(lines))
        .block(msg_block)
        .scroll((app.scroll, 0));
    frame.render_widget(messages_widget, msg_area);

    // --- input ---
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if app.stream_state == StreamState::Streaming {
            DIM
        } else {
            ACCENT
        }))
        .style(Style::default().bg(BG));

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
        if let Some(started) = app.warmup_started_at {
            let elapsed = started.elapsed().as_secs_f64();
            const SPINNER: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
            let frame_i = (elapsed / 0.1) as usize % SPINNER.len();
            frame.render_widget(Paragraph::new(Line::from(vec![
                Span::styled(format!("  {} ", SPINNER[frame_i]), Style::default().fg(ACCENT)),
                Span::styled("warming up KV cache", Style::default().fg(DIM)),
                Span::styled(format!("  {}", format_duration(elapsed)), Style::default().fg(THINKING_COLOR)),
            ])), chunks[2]);
        }
    }

    frame.render_widget(input_widget, chunks[3]);

    // place cursor in 2D using visual coordinates
    if app.stream_state != StreamState::Streaming {
        let (vcur_row, vcur_col) = input_visual_cursor(&app.input, app.cursor_pos, input_inner_width);
        let visible_rows = (input_height - 2) as usize;
        let input_scroll = vcur_row.saturating_sub(visible_rows.saturating_sub(1));
        let visual_row = vcur_row - input_scroll;
        let prefix_len: u16 = 2; // "> " or "  "
        let x = chunks[3].x + 1 + prefix_len + vcur_col as u16;
        let y = chunks[3].y + 1 + visual_row as u16;
        frame.set_cursor_position((x.min(chunks[3].x + chunks[3].width - 2), y));
    }

    // --- completion popup ---
    if app.completion.is_some() {
        draw_completion_popup(frame, app, chunks[3]);
    }

    // --- hints ---
    let esc_label = if app.stream_state == StreamState::Streaming { "stop" } else { "models" };
    let hints = Paragraph::new(Line::from(vec![
        hint("Enter", "send"),
        Span::raw("  "),
        hint("Ctrl+N", "newline"),
        Span::raw("  "),
        hint("Esc", esc_label),
        Span::raw("  "),
        hint("F1", "help"),
    ]))
    .style(Style::default().fg(DIM));
    frame.render_widget(hints, chunks[4]);

    // --- help popup ---
    if app.show_help {
        draw_help_popup(frame, app, area);
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

    let items: Vec<ListItem> = comp.candidates[scroll..scroll + visible]
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let global_idx = scroll + i;
            let name = display_name(candidate);
            let is_dir = candidate.ends_with('/');
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

    let block = Block::default()
        .title(Span::styled(scroll_hint, Style::default().fg(DIM)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(SURFACE));

    // Clear the area first so the popup doesn't bleed through messages
    frame.render_widget(Clear, popup_rect);
    frame.render_widget(List::new(items).block(block), popup_rect);
}

fn draw_help_popup(frame: &mut Frame, app: &App, area: Rect) {
    const SECTIONS: &[(&str, &[(&str, &str)])] = &[
        ("Sending", &[
            ("Enter",        "Send message"),
            ("Ctrl+N",       "Insert newline"),
            ("Esc",          "Stop stream / back to model select"),
        ]),
        ("Cursor movement", &[
            ("←  →",         "Move one character"),
            ("Ctrl+←  →",    "Move one word"),
            ("Ctrl+A",       "Beginning of line"),
            ("Ctrl+E",       "End of line"),
            ("Home / End",   "Beginning / end of line"),
            ("↑  ↓",         "Move between lines (multi-line) / browse history (single-line)"),
        ]),
        ("Editing", &[
            ("Backspace",    "Delete character before cursor"),
            ("Delete",       "Delete character after cursor"),
            ("Ctrl+W",       "Delete word before cursor"),
            ("Ctrl+K",       "Delete to end of line"),
            ("Ctrl+U",       "Delete to beginning of line"),
        ]),
        ("Scrolling", &[
            ("Alt+↑  ↓",     "Scroll messages"),
            ("PageUp / PageDown", "Scroll messages (fast)"),
            ("Ctrl+End",     "Jump to bottom"),
        ]),
        ("Chat", &[
            ("Ctrl+L",       "Clear conversation"),
            ("@path",        "Attach a file or image"),
            ("Ctrl+C",       "Quit"),
            ("F1",           "Toggle this help"),
        ]),
    ];

    // build lines
    let mut lines: Vec<Line> = vec![Line::raw("")];
    for (section, entries) in SECTIONS {
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

    let popup_w = 58u16.min(area.width.saturating_sub(4));
    let popup_h = 24u16.min(area.height.saturating_sub(4));
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
        let label = if streaming && content.is_empty() {
            "  thinking…".to_string()
        } else if let Some(secs) = thought_secs {
            format!("  thought for {}", format_duration(secs))
        } else {
            "  thinking…".to_string()
        };
        lines.push(Line::from(Span::styled(
            label,
            Style::default().fg(THINKING_COLOR).add_modifier(Modifier::ITALIC),
        )));
        for text_line in wrap_text(thinking.trim(), width) {
            lines.push(Line::from(Span::styled(
                format!("  {text_line}"),
                Style::default().fg(THINKING_COLOR),
            )));
        }
        // streaming cursor while still thinking (content not yet started)
        if streaming && content.is_empty() {
            if let Some(last) = lines.last_mut() {
                last.spans.push(Span::styled("▋", Style::default().fg(THINKING_COLOR)));
            }
        }
        lines.push(Line::raw(""));
    }

    // regular response content — split into text/code segments
    if !content.is_empty() {
        let segments = parse_content_segments(content.trim());
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
    let label = if lang.is_empty() { "code".to_string() } else { lang.to_string() };
    out.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(label, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
    ]));
    for code_line in code.trim_matches('\n').lines() {
        let truncated: String = code_line.chars().take(width.saturating_sub(4)).collect();
        out.push(Line::from(vec![
            Span::styled("  ▎ ", Style::default().fg(CODE_BORDER)),
            Span::styled(truncated, Style::default().fg(CODE_FG)),
        ]));
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

        buf.push(chars[i]);
        i += 1;
    }
    flush!();
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}
