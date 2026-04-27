use std::path::{Path, PathBuf};
use crate::app::App;
use crate::domain::message::{Attachment, AttachmentKind, Message, Role};
use crate::domain::tags::{AskKind, InputRequest};

impl App {
    pub fn handle_tool_call(&mut self, call: &str) {
        let call = call.trim();
        let cmd = call.split_once(' ').map(|p| p.0).unwrap_or(call);
        let is_builtin = matches!(cmd,
            "read_file" | "write_file" | "edit_file" | "grep_files" | "glob_files");

        // Gate destructive shell passthrough behind a confirm prompt unless
        // auto-accept is on. Built-ins bypass: write_file/edit_file are
        // explicit about the target path so the user already sees the impact;
        // read_file/grep_files/glob_files are read-only.
        if !is_builtin && !self.auto_accept && is_destructive_shell(call) {
            self.pending_tool_call = Some(call.to_string());
            self.ask = Some(InputRequest {
                question: format!("Run destructive shell command? `{call}`"),
                kind: AskKind::Confirm,
            });
            self.ask_cursor = 0;
            return;
        }

        self.execute_tool_call(call);
    }

    pub(crate) fn execute_tool_call(&mut self, call: &str) {
        let (cmd, args) = call.split_once(' ').unwrap_or((call, ""));
        let args = args.trim().trim_start_matches('@');

        // built-in native tools — checked before synapses
        // write_file and edit_file use the full call body (multi-line content after cmd)
        let full_args = call.splitn(2, char::is_whitespace).nth(1).unwrap_or("").trim_start_matches('@');
        let builtin_result = match cmd {
            "read_file"  => Some(crate::adapter::tools_native::read_file(full_args, &self.working_dir)),
            "write_file" => Some(crate::adapter::tools_native::write_file(full_args, &self.working_dir)),
            "edit_file"  => Some(crate::adapter::tools_native::edit_file(full_args, &self.working_dir)),
            "grep_files" => Some(crate::adapter::tools_native::grep_files(full_args, &self.working_dir)),
            "glob_files" => Some(crate::adapter::tools_native::glob_files(full_args, &self.working_dir)),
            _ => None,
        };

        if let Some(result) = builtin_result {
            let tool_label = format!("built-in \u{203a} {cmd}");
            return self.push_tool_result(call, result, tool_label, args);
        }

        // search across all neurons for a matching behaviour
        let found = self.neurons.iter().find_map(|n| {
            n.synapses.iter().find(|s| s.trigger == cmd).map(|s| (n.name.clone(), s.clone()))
        });

        // fall back to shell passthrough if no specific behaviour matched
        let shell_neuron = if found.is_none() {
            self.neurons.iter().find(|n| n.shell)
        } else {
            None
        };

        let result = if let Some((_, b)) = &found {
            let crate::domain::neuron::SynapseKind::Tool { command, .. } = &b.kind;
            execute_command(command, args, &self.working_dir)
        } else if shell_neuron.is_some() {
            execute_command(cmd, args, &self.working_dir)
        } else {
            format!("unknown tool: {cmd}")
        };

        let tool_label = found
            .as_ref()
            .map(|(neuron_name, b)| format!("{} \u{203a} {}", neuron_name, b.trigger))
            .or_else(|| shell_neuron.map(|n| format!("{} \u{203a} {}", n.name, cmd)))
            .unwrap_or_else(|| cmd.to_string());

        self.push_tool_result(call, result, tool_label, args);
    }

    pub(crate) fn push_tool_result(&mut self, call: &str, result: String, tool_label: String, args: &str) {
        let filename = Path::new(args)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| if args.is_empty() { ".".to_string() } else { args.to_string() });
        let size = result.len();

        // Inject result into the last assistant message's llm_content so the model
        // never sees tool output as a user-role message — it's part of its own turn.
        // Fallback: if we reach here without an assistant turn to own the output
        // (rare — means a tool call ran outside the normal stream lifecycle),
        // create a minimal assistant placeholder so the result isn't dropped
        // silently. Empty content keeps it invisible in the UI.
        let owned = matches!(self.messages.last(), Some(m) if m.role == Role::Assistant);
        if !owned {
            self.messages.push(Message {
                role: Role::Assistant,
                content: String::new(),
                llm_content: String::new(),
                images: vec![],
                attachments: vec![],
                thinking: String::new(),
                thinking_secs: None,
                stats: None,
                tool_call: None,
                tool_collapsed: false,
            });
        }
        if let Some(last) = self.messages.last_mut() {
            last.llm_content.push_str(&format!("\n[Tool output for: {call}]\n{result}"));
        }

        const MAX_DISPLAY_LINES: usize = 15;
        let display_content = {
            let count = result.lines().count();
            if count > MAX_DISPLAY_LINES {
                let head: String = result.lines().take(MAX_DISPLAY_LINES).collect::<Vec<_>>().join("\n");
                format!("{head}\n[... {} more lines — full output sent to model]", count - MAX_DISPLAY_LINES)
            } else {
                result
            }
        };

        // Display-only block (llm_content empty → excluded from API).
        self.messages.push(Message {
            role: Role::Tool,
            content: display_content,
            llm_content: String::new(),
            images: vec![],
            attachments: vec![Attachment {
                filename,
                path: PathBuf::new(),
                kind: AttachmentKind::Text,
                size,
            }],
            thinking: String::new(),
            thinking_secs: None,
            stats: None,
            tool_call: Some(tool_label),
            tool_collapsed: true,
        });

        self.auto_scroll = true;
        self.start_stream();
    }
}

/// Returns true if the shell command line includes a destructive operation
/// that should require explicit user confirmation. Conservative — false
/// positives are preferred to false negatives. Skips `sudo` prefix and
/// resolves `/usr/bin/rm` → `rm` so quoted paths don't slip through.
fn is_destructive_shell(call: &str) -> bool {
    const DESTRUCTIVE: &[&str] = &[
        "rm", "rmdir", "mv", "dd", "mkfs", "shred",
        "truncate", "chmod", "chown", "chgrp",
    ];
    for segment in call.split(|c: char| matches!(c, ';' | '|' | '&')) {
        let mut tokens = segment.split_whitespace();
        let mut first = tokens.next().unwrap_or("");
        if first == "sudo" { first = tokens.next().unwrap_or(""); }
        let basename = first.rsplit('/').next().unwrap_or(first);
        if DESTRUCTIVE.contains(&basename) { return true; }
        if basename == "git" {
            let next = tokens.next().unwrap_or("");
            if matches!(next, "rm" | "mv" | "clean" | "reset" | "checkout") {
                return true;
            }
        }
    }
    false
}

/// Cap shell output so a single `<tool>cat huge.json</tool>` cannot blow the
/// model's context window. 500 lines OR 32KB, whichever is hit first; suffix
/// tells the model how to fetch the rest.
fn truncate_output(out: &str) -> String {
    const MAX_LINES: usize = 500;
    const MAX_BYTES: usize = 32 * 1024;

    let total_bytes = out.len();
    let total_lines = out.lines().count();
    if total_lines <= MAX_LINES && total_bytes <= MAX_BYTES {
        return out.to_string();
    }

    // pick the tighter of the two cuts, then back off to a char boundary
    let line_cut: usize = out.split_inclusive('\n').take(MAX_LINES).map(str::len).sum();
    let mut cut = line_cut.min(MAX_BYTES).min(total_bytes);
    while cut > 0 && !out.is_char_boundary(cut) { cut -= 1; }

    let head = &out[..cut];
    let extra_lines = total_lines.saturating_sub(head.lines().count());
    let extra_bytes = total_bytes - cut;
    format!(
        "{head}\n[... truncated: {extra_lines} more lines, {extra_bytes} more bytes — pipe to head/sed or use read_file with offset]"
    )
}

/// Executes a command via `sh -c` so the full shell syntax is supported:
/// quotes, spaces, redirections, pipes, etc.
/// `command` may include fixed flags (e.g. "grep -rn").
/// `args` are appended after the fixed command string.
/// Runs with `working_dir` as the current directory.
fn execute_command(command: &str, args: &str, working_dir: &Path) -> String {
    if command.is_empty() {
        return "error: empty command".to_string();
    }
    let full = if args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {args}")
    };

    match std::process::Command::new("sh")
        .arg("-c")
        .arg(&full)
        .current_dir(working_dir)
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                let msg = if !stderr.is_empty() { stderr } else { stdout };
                truncate_output(&format!("error: {msg}"))
            } else if stdout.is_empty() {
                "Done.".to_string()
            } else {
                truncate_output(&stdout)
            }
        }
        Err(e) => format!("error: {e}"),
    }
}
