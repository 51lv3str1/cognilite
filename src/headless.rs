use std::io::{BufRead, Write};
use std::path::PathBuf;
use crate::app::{
    App, AskKind, AttachmentKind, Attachment, CtxStrategy, InputRequest, Message, NeuronMode,
    Role, StreamState, extract_ask_tag, extract_load_neuron_tag, extract_mood_tag,
    extract_patch_tag, extract_tool_call, resolve_attachments,
};

pub struct HeadlessArgs {
    pub message: Option<String>,
    pub model: Option<String>,
    pub neuron_mode: Option<NeuronMode>,
    pub preset: Option<String>,
    pub no_neurons: Vec<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub repeat_penalty: Option<f64>,
    pub ctx_strategy: Option<CtxStrategy>,
    pub keep_alive: bool,
    pub pin: Vec<String>,
    pub attach: Vec<String>,
    pub yes: bool,     // auto-confirm all <ask type="confirm"> prompts
    pub thinking: bool, // stream thinking content to stderr
}

impl Default for HeadlessArgs {
    fn default() -> Self {
        Self {
            message: None, model: None, neuron_mode: None, preset: None,
            no_neurons: vec![], temperature: None, top_p: None, repeat_penalty: None,
            ctx_strategy: None, keep_alive: false, pin: vec![], attach: vec![],
            yes: false, thinking: false,
        }
    }
}

pub fn run(base_url: &str, args: HeadlessArgs) -> i32 {
    let mut app = App::new(base_url.to_string());

    // apply overrides
    if let Some(mode) = args.neuron_mode {
        app.neuron_mode = mode;
    }
    if let Some(preset) = &args.preset {
        app.neuron_mode = NeuronMode::Presets;
        app.active_preset = Some(preset.clone());
    }
    for name in &args.no_neurons {
        app.disabled_neurons.insert(name.clone());
    }
    if let Some(v) = args.temperature    { app.gen_params[0] = v; }
    if let Some(v) = args.top_p         { app.gen_params[1] = v; }
    if let Some(v) = args.repeat_penalty { app.gen_params[2] = v; }
    if let Some(s) = args.ctx_strategy  { app.ctx_strategy = s; }
    if args.keep_alive { app.keep_alive = true; }
    app.warmup = false; // no warmup in headless

    // model selection
    match crate::ollama::list_models(base_url) {
        Ok(models) => app.models = models,
        Err(e) => { eprintln!("Error: cannot list models — {e}"); return 1; }
    }
    let model_name = match args.model {
        Some(ref m) => {
            if app.models.iter().any(|e| e.name == *m) { m.clone() }
            else { eprintln!("Error: model '{m}' not found"); return 1; }
        }
        None => match app.models.first() {
            Some(e) => e.name.clone(),
            None => { eprintln!("Error: no models available in Ollama"); return 1; }
        }
    };
    eprintln!("[model: {model_name}]");
    app.selected_model = Some(model_name.clone());
    app.context_length = crate::ollama::fetch_context_length(base_url, &model_name);
    app.stream_state = StreamState::Idle;
    app.screen = crate::app::Screen::Chat;

    // pin files
    for path_str in &args.pin {
        app.pin_file(path_str.clone());
    }

    // build message
    let raw_message = if let Some(m) = args.message {
        m
    } else {
        let mut buf = String::new();
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => { buf.push_str(&l); buf.push('\n'); }
                Err(_) => break,
            }
        }
        buf.trim().to_string()
    };
    if raw_message.is_empty() {
        eprintln!("Error: no message provided (pass as argument or via stdin)");
        return 1;
    }
    let full_message = if args.attach.is_empty() {
        raw_message
    } else {
        let refs = args.attach.iter().map(|p| format!("@{p}")).collect::<Vec<_>>().join(" ");
        format!("{raw_message} {refs}")
    };

    // push user message and start stream
    let (display, llm_content, attachments, images) =
        resolve_attachments(&full_message, &app.working_dir, app.context_length, app.used_tokens);
    app.messages.push(Message {
        role: Role::User,
        content: display,
        llm_content,
        images,
        attachments,
        thinking: String::new(),
        thinking_secs: None,
        stats: None,
        tool_call: None,
    });
    app.start_stream();

    run_stream_loop(&mut app, args.yes, args.thinking)
}

fn run_stream_loop(app: &mut App, auto_yes: bool, show_thinking: bool) -> i32 {
    'outer: loop {
        let rx = match app.stream_rx.take() {
            Some(r) => r,
            None => return 0,
        };
        // tracks how many bytes of the current assistant message have been printed;
        // reset each outer iteration so tag stripping from previous turns doesn't bleed over
        let mut printed_up_to: usize = 0;
        let mut thinking_open = false; // whether we've printed the [thinking] header

        loop {
            let chunk = match rx.recv() {
                Ok(c) => c,
                Err(_) => return 0,
            };

            if let Some(e) = chunk.error {
                eprintln!("\nError: {e}");
                return 1;
            }

            if let Some(ref msg) = chunk.message {
                // accumulate tokens
                if let Some(last) = app.messages.last_mut() {
                    if last.role == Role::Assistant {
                        last.content.push_str(&msg.content);
                        last.llm_content.push_str(&msg.content);
                        if let Some(ref t) = msg.thinking {
                            if !t.is_empty() {
                                if show_thinking {
                                    if !thinking_open {
                                        eprintln!("[thinking]");
                                        thinking_open = true;
                                    }
                                    eprint!("{t}");
                                }
                                last.thinking.push_str(t);
                            }
                        }
                    }
                }
                // print only the bytes we haven't printed yet, stopping before any tag prefix
                if let Some(last) = app.messages.last() {
                    if last.role == Role::Assistant {
                        let safe = safe_print_boundary(&last.content, printed_up_to);
                        if safe > printed_up_to {
                            print!("{}", &last.content[printed_up_to..safe]);
                            let _ = std::io::stdout().flush();
                            printed_up_to = safe;
                        }
                    }
                }

                // <tool>
                let maybe_call = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_tool_call(&m.content))
                    .map(|s| s.to_string());
                if let Some(call) = maybe_call {
                    if let Some(last) = app.messages.last_mut() {
                        let tool_pos = match last.content.rfind("</think>") {
                            Some(i) => last.content[i+8..].find("<tool>").map(|p| i+8+p),
                            None    => last.content.find("<tool>"),
                        };
                        if let Some(pos) = tool_pos {
                            last.content.truncate(pos);
                            last.content = last.content.trim_end().to_string();
                        }
                    }
                    println!();
                    eprintln!("[tool: {call}]");
                    app.handle_tool_call(&call);
                    continue 'outer;
                }

                // <load_neuron>
                let load_name = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_load_neuron_tag(&m.content));
                if let Some(name) = load_name {
                    if !app.injected_neurons.contains(&name) {
                        let neuron_content = app.neurons.iter()
                            .find(|n| n.name.eq_ignore_ascii_case(&name))
                            .map(|n| format!("## Neuron: {}\n\n{}", n.name, n.system_prompt));
                        if let Some(content) = neuron_content {
                            if let Some(last) = app.messages.last_mut() {
                                let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                                if let Some(p) = last.content[sf..].find("<load_neuron>") {
                                    let abs = sf + p;
                                    if let Some(end) = last.content[abs..].find("</load_neuron>") {
                                        let tag_end = abs + end + 14;
                                        let before = last.content[..abs].trim_end().to_string();
                                        let after  = last.content[tag_end..].to_string();
                                        last.content = before + &after;
                                    }
                                }
                            }
                            app.injected_neurons.insert(name.clone());
                            let label = format!("Neuron \u{203a} {}", name);
                            let size  = content.len();
                            println!();
                            eprintln!("[loading neuron: {name}]");
                            app.messages.push(Message {
                                role: Role::Tool,
                                content: content.clone(),
                                llm_content: format!("Neuron loaded:\n{content}"),
                                images: vec![],
                                attachments: vec![Attachment {
                                    filename: name,
                                    path: PathBuf::new(),
                                    kind: AttachmentKind::Text,
                                    size,
                                }],
                                thinking: String::new(),
                                thinking_secs: None,
                                stats: None,
                                tool_call: Some(label),
                            });
                            app.start_stream();
                            continue 'outer;
                        }
                    }
                }

                // <ask>
                let ask_info = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_ask_tag(&m.content));
                if let Some((kind, question)) = ask_info {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                        if let Some(p) = last.content[sf..].find("<ask") {
                            last.content.truncate(sf + p);
                            last.content = last.content.trim_end().to_string();
                        }
                    }
                    println!();
                    let response = ask_interactive(&kind, &question, auto_yes);
                    app.ask = Some(InputRequest { question, kind });
                    app.submit_ask(response);
                    continue 'outer;
                }

                // <patch> — auto-apply in headless
                let patch = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_patch_tag(&m.content));
                if let Some(diff) = patch {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                        if let Some(p) = last.content[sf..].find("<patch>") {
                            let abs = sf + p;
                            if let Some(end) = last.content[abs..].find("</patch>") {
                                let tag_end = abs + end + 8;
                                let before = last.content[..abs].trim_end().to_string();
                                let after  = last.content[tag_end..].to_string();
                                let rendered = format!("```diff\n{}\n```", diff.trim());
                                last.content = if before.is_empty() { rendered + &after }
                                               else { format!("{before}\n{rendered}{after}") };
                            }
                        }
                    }
                    println!();
                    eprintln!("[patch proposed — applying automatically]");
                    app.pending_patch = Some(diff);
                    app.ask = Some(InputRequest {
                        question: "Apply this patch?".to_string(),
                        kind: AskKind::Confirm,
                    });
                    app.submit_ask("Yes".to_string());
                    continue 'outer;
                }

                // <mood> — strip and continue
                let mood = app.messages.last()
                    .filter(|m| m.role == Role::Assistant)
                    .and_then(|m| extract_mood_tag(&m.content));
                if let Some(emoji) = mood {
                    if let Some(last) = app.messages.last_mut() {
                        let sf = last.content.rfind("</think>").map(|i| i+8).unwrap_or(0);
                        if let Some(p) = last.content[sf..].find("<mood>") {
                            let abs = sf + p;
                            if let Some(end) = last.content[abs..].find("</mood>") {
                                let tag_end = abs + end + 7;
                                let before = last.content[..abs].trim_end().to_string();
                                let after  = last.content[tag_end..].to_string();
                                last.content = before + &after;
                            }
                        }
                    }
                    app.current_mood = Some(emoji);
                }
            }

            if chunk.done {
                if thinking_open { eprintln!("\n[/thinking]"); }
                println!();
                if let (Some(pt), Some(et), Some(ed)) = (
                    chunk.prompt_eval_count, chunk.eval_count, chunk.eval_duration,
                ) {
                    let tps = et as f64 / (ed as f64 / 1_000_000_000.0);
                    eprintln!("[{tps:.1} tok/s · {et} response tokens · {pt} prompt eval]");
                }
                return 0;
            }
        }
    }
}

fn ask_interactive(kind: &AskKind, question: &str, auto_yes: bool) -> String {
    let mut stderr = std::io::stderr().lock();
    match kind {
        AskKind::Text => {
            let _ = write!(stderr, "[ask] {question}: ");
            let _ = stderr.flush();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).ok();
            line.trim().to_string()
        }
        AskKind::Confirm => {
            if auto_yes {
                let _ = writeln!(stderr, "[confirm] {question} → Yes (--yes)");
                return "Yes".to_string();
            }
            let _ = write!(stderr, "[confirm] {question} [y/N]: ");
            let _ = stderr.flush();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).ok();
            if matches!(line.trim().to_lowercase().as_str(), "y" | "yes") { "Yes".to_string() } else { "No".to_string() }
        }
        AskKind::Choice(options) => {
            let _ = writeln!(stderr, "[choice] {question}");
            for (i, opt) in options.iter().enumerate() {
                let _ = writeln!(stderr, "  {}: {opt}", i + 1);
            }
            let _ = write!(stderr, "Enter number (default 1): ");
            let _ = stderr.flush();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).ok();
            let n: usize = line.trim().parse().unwrap_or(1);
            options.get(n.saturating_sub(1)).cloned().unwrap_or_default()
        }
    }
}

/// Returns the byte offset up to which `content[from..]` can be safely printed,
/// skipping completed `<think>` blocks and stopping before any protocol tag.
fn safe_print_boundary(content: &str, from: usize) -> usize {
    const STOP_TAGS: &[&str] = &[
        "<tool>", "</tool>",
        "<load_neuron>", "</load_neuron>",
        "<ask", "</ask>",
        "<patch>", "</patch>",
        "<mood>", "</mood>",
    ];
    let mut pos = from;
    loop {
        let rest = &content[pos..];
        let Some(rel) = rest.find('<') else { return content.len(); };
        let abs = pos + rel;
        let slice = &content[abs..];
        if slice.starts_with("<think>") {
            match slice.find("</think>") {
                Some(end) => { pos = abs + end + 8; continue; }
                None => return abs,
            }
        }
        if slice.starts_with("<thought>") {
            match slice.find("</thought>") {
                Some(end) => { pos = abs + end + 10; continue; }
                None => return abs,
            }
        }
        if slice.len() < 2 { return abs; }
        let second = &slice[1..2];
        if !matches!(second, "t" | "l" | "a" | "p" | "m" | "/") {
            pos = abs + 1; continue;
        }
        if STOP_TAGS.iter().any(|tag| tag.starts_with(slice) || slice.starts_with(tag)) {
            return abs;
        }
        pos = abs + 1;
    }
}
