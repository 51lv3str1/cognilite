// Pure prompt-building logic. No App, no I/O.

pub enum RuntimeMode {
    Tui,
    Headless,
    Server    { auto_yes: bool },
    WebSocket { auto_yes: bool },
    /// WebSocket session where the client is the cognilite TUI (--remote mode).
    /// Has identical UI capabilities to the local TUI.
    RemoteTui { auto_yes: bool },
}

pub fn build_runtime_context(model: &str, ctx_len: Option<u64>, mode: RuntimeMode) -> String {
    let ctx_str = ctx_len
        .map(|n| format!("{}k", n / 1024))
        .unwrap_or_else(|| "unknown".to_string());
    match mode {
        RuntimeMode::Tui => format!(
            "## Runtime context\n\nMode: **interactive TUI** · Model: `{model}` · Context window: {ctx_str}\n\n\
             The user is typing in the terminal UI. All features are available:\n\
             - `<ask>` pauses the stream and shows an interactive widget — text input, yes/no, or choice list.\n\
             - `<patch>` renders a colored diff and asks the user to confirm before applying.\n\
             - The file panel on the right shows the currently open file (if any).\n\
             - Pinned files are injected into every request and tracked for changes.\n\
             - The user can switch models and toggle neurons from the settings screen.\n\
             - Thinking blocks are rendered in a muted color with a 'thought for Xs' label."
        ),
        RuntimeMode::Headless => format!(
            "## Runtime context\n\nMode: **headless** (CLI) · Model: `{model}` · Context window: {ctx_str}\n\n\
             Running non-interactively from the shell. Responds once and exits. \
             `<ask>` reads from stdin — use it only when operator input is genuinely required."
        ),
        RuntimeMode::Server { auto_yes } => {
            let note = if auto_yes {
                "All confirmations are auto-accepted (`--yes` is active)."
            } else {
                "`<ask>` prompts go to the server operator's terminal, not the remote client."
            };
            format!(
                "## Runtime context\n\nMode: **server** (HTTP POST /chat) · Model: `{model}` · Context window: {ctx_str}\n\n\
                 The remote client receives your response as a plain-text stream. \
                 Avoid `<ask>` when possible — the client cannot send mid-stream input. \
                 Use tools to gather missing information instead of asking. {note}"
            )
        }
        RuntimeMode::WebSocket { auto_yes } => {
            let note = if auto_yes {
                "All confirmations are auto-accepted (`--yes` is active)."
            } else {
                "`<ask>` prompts are sent to the client as structured frames — the client responds and the conversation continues. \
                 `<patch>` diffs are shown to the client for confirmation before being applied."
            };
            format!(
                "## Runtime context\n\nMode: **WebSocket session** · Model: `{model}` · Context window: {ctx_str}\n\n\
                 The client is connected via WebSocket for a full multi-turn conversation. \
                 You have the same capabilities as the interactive TUI: tool execution, `<ask>`, `<patch>`, \
                 pinned files, and streaming. {note}"
            )
        }
        RuntimeMode::RemoteTui { auto_yes } => {
            let note = if auto_yes {
                "All confirmations are auto-accepted (`--yes` is active)."
            } else {
                ""
            };
            format!(
                "## Runtime context\n\nMode: **remote TUI** (WebSocket) · Model: `{model}` · Context window: {ctx_str}\n\n\
                 The user is running the cognilite terminal UI on a remote machine, connected via WebSocket. \
                 The client renders the full TUI — all interactive features work exactly as in local mode:\n\
                 - `<ask>` pauses the stream and shows an interactive widget — text input, yes/no, or choice list.\n\
                 - `<patch>` renders a colored diff and asks the user to confirm before applying on this server.\n\
                 - Tool results (`<tool>`) are displayed as styled bubbles in the chat.\n\
                 - Thinking blocks are rendered in a muted color with a 'thought for Xs' label.\n\
                 - Pinned files are tracked and changes are sent as diffs on each turn.\n\
                 Use all features freely — the client handles them identically to the local TUI. {note}"
            )
        }
    }
}

// ── Raw continuation prompt (opt B: /api/generate with raw:true) ──────────────
//
// After a tool call, the assistant turn is mid-generation. Sending the history
// back through /api/chat makes Ollama re-apply the chat template, which closes
// the assistant turn — the model sees "I already answered" and stops.
//
// We build the prompt manually here, using the model's own template format,
// and leave the last assistant turn OPEN (no closing token). Ollama then
// continues generating from that point — no re-applied template, no new turn.

#[derive(Debug, Clone, Copy)]
pub enum TemplateFormat {
    ChatML, // qwen, deepseek, nemotron, most recent reasoning models
    Llama3, // llama 3.x
    Gemma,  // gemma 2/3 (role=model)
}

/// Detect format from the Go template string returned by /api/show.
/// Unknown formats return None; callers must fall back to /api/chat.
pub fn detect_template_format(template: &str) -> Option<TemplateFormat> {
    if template.contains("<|im_start|>") {
        Some(TemplateFormat::ChatML)
    } else if template.contains("<|start_header_id|>") {
        Some(TemplateFormat::Llama3)
    } else if template.contains("<start_of_turn>") {
        Some(TemplateFormat::Gemma)
    } else {
        None
    }
}

/// Build a raw prompt string for /api/generate with raw:true.
///
/// `history` carries API-shaped (role, content) pairs — roles are "user" /
/// "assistant" / "system" (the system slot is handled separately).
///
/// If the last message is an assistant, we leave its turn OPEN (no closing
/// token, no new assistant header). Ollama continues generating right where
/// it left off. Otherwise, we close all previous turns and open a fresh
/// assistant turn.
pub fn build_raw_prompt(
    fmt: TemplateFormat,
    system: Option<&str>,
    history: &[(String, String)],
) -> String {
    let mut s = String::new();
    let last_is_assistant = history.last().is_some_and(|(r, _)| r == "assistant");

    match fmt {
        TemplateFormat::ChatML => {
            if let Some(sys) = system {
                s.push_str(&format!("<|im_start|>system\n{sys}<|im_end|>\n"));
            }
            for (i, (role, content)) in history.iter().enumerate() {
                let is_last = i + 1 == history.len();
                if is_last && last_is_assistant {
                    s.push_str(&format!("<|im_start|>assistant\n{content}"));
                } else {
                    s.push_str(&format!("<|im_start|>{role}\n{content}<|im_end|>\n"));
                }
            }
            if !last_is_assistant {
                s.push_str("<|im_start|>assistant\n");
            }
        }
        TemplateFormat::Llama3 => {
            s.push_str("<|begin_of_text|>");
            if let Some(sys) = system {
                s.push_str(&format!(
                    "<|start_header_id|>system<|end_header_id|>\n\n{sys}<|eot_id|>"
                ));
            }
            for (i, (role, content)) in history.iter().enumerate() {
                let is_last = i + 1 == history.len();
                if is_last && last_is_assistant {
                    s.push_str(&format!(
                        "<|start_header_id|>assistant<|end_header_id|>\n\n{content}"
                    ));
                } else {
                    s.push_str(&format!(
                        "<|start_header_id|>{role}<|end_header_id|>\n\n{content}<|eot_id|>"
                    ));
                }
            }
            if !last_is_assistant {
                s.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
            }
        }
        TemplateFormat::Gemma => {
            // Gemma has no native system role; prepend it to the first user turn
            // if present. Assistant role is emitted as "model".
            let mut system_consumed = system.is_none();
            for (i, (role, content)) in history.iter().enumerate() {
                let is_last = i + 1 == history.len();
                let g_role = if role == "assistant" { "model" } else { "user" };
                let body = if !system_consumed && role == "user" {
                    system_consumed = true;
                    format!("{}\n\n{}", system.unwrap(), content)
                } else {
                    content.clone()
                };
                if is_last && last_is_assistant {
                    s.push_str(&format!("<start_of_turn>model\n{body}"));
                } else {
                    s.push_str(&format!("<start_of_turn>{g_role}\n{body}<end_of_turn>\n"));
                }
            }
            // If no user turn carried the system, prepend it as a bare user turn.
            if !system_consumed {
                let sys = system.unwrap_or("");
                let mut head = format!("<start_of_turn>user\n{sys}<end_of_turn>\n");
                head.push_str(&s);
                s = head;
            }
            if !last_is_assistant {
                s.push_str("<start_of_turn>model\n");
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_chatml() {
        assert!(matches!(detect_template_format("{{ if .System }}<|im_start|>system"), Some(TemplateFormat::ChatML)));
    }

    #[test]
    fn detect_llama3() {
        assert!(matches!(detect_template_format("<|start_header_id|>system"), Some(TemplateFormat::Llama3)));
    }

    #[test]
    fn detect_gemma() {
        assert!(matches!(detect_template_format("<start_of_turn>user"), Some(TemplateFormat::Gemma)));
    }

    #[test]
    fn detect_unknown_returns_none() {
        assert!(detect_template_format("custom template no markers").is_none());
    }

    #[test]
    fn raw_chatml_open_assistant_when_last_is_user() {
        let h = vec![("user".into(), "hi".into())];
        let p = build_raw_prompt(TemplateFormat::ChatML, Some("sys"), &h);
        assert!(p.contains("<|im_start|>system\nsys<|im_end|>"));
        assert!(p.contains("<|im_start|>user\nhi<|im_end|>"));
        assert!(p.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn raw_chatml_keeps_assistant_open_when_last_is_assistant() {
        let h = vec![
            ("user".into(), "hi".into()),
            ("assistant".into(), "partial response".into()),
        ];
        let p = build_raw_prompt(TemplateFormat::ChatML, None, &h);
        // last assistant turn must not be closed
        assert!(p.ends_with("<|im_start|>assistant\npartial response"));
        assert!(!p.contains("partial response<|im_end|>"));
    }

    #[test]
    fn raw_llama3_starts_with_begin_of_text() {
        let h = vec![("user".into(), "hi".into())];
        let p = build_raw_prompt(TemplateFormat::Llama3, None, &h);
        assert!(p.starts_with("<|begin_of_text|>"));
        assert!(p.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn raw_gemma_inlines_system_into_first_user() {
        let h = vec![("user".into(), "hi".into())];
        let p = build_raw_prompt(TemplateFormat::Gemma, Some("sys"), &h);
        // system gets prepended to the first user turn body
        assert!(p.contains("<start_of_turn>user\nsys\n\nhi<end_of_turn>"));
        assert!(p.ends_with("<start_of_turn>model\n"));
    }

    #[test]
    fn raw_gemma_no_user_turn_emits_bare_system() {
        let h: Vec<(String, String)> = vec![];
        let p = build_raw_prompt(TemplateFormat::Gemma, Some("sys"), &h);
        assert!(p.contains("<start_of_turn>user\nsys<end_of_turn>"));
    }

    #[test]
    fn runtime_context_includes_model_and_ctx() {
        let s = build_runtime_context("qwen3:8b", Some(8192), RuntimeMode::Tui);
        assert!(s.contains("qwen3:8b"));
        assert!(s.contains("8k"));
        assert!(s.contains("interactive TUI"));
    }
}
