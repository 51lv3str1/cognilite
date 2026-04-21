use std::path::{Path, PathBuf};
use std::fs;

const MAX_READ_LINES: usize = 500;
const MAX_GREP_RESULTS: usize = 100;

// ── read_file ─────────────────────────────────────────────────────────────────

/// read_file <path> [start_line [end_line]]
pub fn read_file(args: &str, working_dir: &Path) -> String {
    let parts: Vec<&str> = args.trim().splitn(3, ' ').collect();
    if parts.is_empty() || parts[0].is_empty() {
        return "error: usage: read_file <path> [start [end]]".to_string();
    }
    let path = resolve(parts[0], working_dir);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return format!("error: {e}"),
    };
    let all: Vec<&str> = content.lines().collect();
    let total = all.len();

    let start = parts.get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1))
        .unwrap_or(0)
        .min(total);
    let end = parts.get(2)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(total)
        .min(total);
    let end = end.max(start);

    let cap = (start + MAX_READ_LINES).min(end);
    let truncated = cap < end;

    let mut out = all[start..cap]
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{:4} | {}", start + i + 1, l))
        .collect::<Vec<_>>()
        .join("\n");

    if truncated {
        out.push_str(&format!(
            "\n[... {} more lines — use: read_file {} {} {}]",
            end - cap, parts[0], cap + 1, end
        ));
    }

    format!("{} ({} lines)\n{}", parts[0], total, out)
}

// ── write_file ────────────────────────────────────────────────────────────────

/// write_file <path>\n<content>
pub fn write_file(args: &str, working_dir: &Path) -> String {
    let (path_str, content) = match args.split_once('\n') {
        Some(p) => (p.0.trim(), p.1),
        None => return "error: usage: write_file <path>\\n<content>".to_string(),
    };
    let path = resolve(path_str, working_dir);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(&path, content) {
        Ok(_) => format!("Written {} bytes to {}", content.len(), path_str),
        Err(e) => format!("error: {e}"),
    }
}

// ── edit_file ─────────────────────────────────────────────────────────────────

/// edit_file <path>
/// <<<FIND
/// old string
/// <<<REPLACE
/// new string
pub fn edit_file(args: &str, working_dir: &Path) -> String {
    let (path_str, rest) = match args.split_once('\n') {
        Some(p) => (p.0.trim(), p.1),
        None => return "error: usage: edit_file <path>\\n<<<FIND\\n<old>\\n<<<REPLACE\\n<new>".to_string(),
    };

    const FIND_MARKER: &str = "<<<FIND\n";
    const REPLACE_MARKER: &str = "<<<REPLACE\n";

    let fi = match rest.find(FIND_MARKER) {
        Some(i) => i + FIND_MARKER.len(),
        None => return "error: missing <<<FIND marker".to_string(),
    };
    let ri = match rest.find(REPLACE_MARKER) {
        Some(i) => i,
        None => return "error: missing <<<REPLACE marker".to_string(),
    };
    if ri < fi {
        return "error: <<<REPLACE must come after <<<FIND".to_string();
    }

    let old_str = &rest[fi..ri];
    let new_str = &rest[ri + REPLACE_MARKER.len()..];

    let path = resolve(path_str, working_dir);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return format!("error reading file: {e}"),
    };
    if !content.contains(old_str) {
        return format!(
            "error: old string not found in {path_str} — verify exact whitespace and newlines"
        );
    }
    let new_content = content.replacen(old_str, new_str, 1);
    match fs::write(&path, &new_content) {
        Ok(_) => format!("Edit applied to {path_str}"),
        Err(e) => format!("error writing file: {e}"),
    }
}

// ── grep_files ────────────────────────────────────────────────────────────────

/// grep_files <pattern> [path]
/// Prefers ripgrep (rg) over grep. Falls back gracefully with a clear error.
pub fn grep_files(args: &str, working_dir: &Path) -> String {
    let (pattern, search_path) = args.split_once(' ').unwrap_or((args, "."));
    let search_path = if search_path.trim().is_empty() { "." } else { search_path.trim() };

    if tool_available("rg") {
        // ripgrep: faster, respects .gitignore, excludes target/ automatically
        match run_tool(
            "rg",
            &["--no-heading", "-n", "--color=never", "-m", "5", pattern, search_path],
            working_dir,
        ) {
            Ok(out) => truncate_results(&out),
            Err(e) => e,
        }
    } else {
        match run_tool(
            "grep",
            &[
                "-rn", "--color=never",
                "-m", "5",
                "--exclude-dir=target",
                "--exclude-dir=.git",
                "--exclude-dir=node_modules",
                pattern,
                search_path,
            ],
            working_dir,
        ) {
            Ok(out) => truncate_results(&out),
            Err(e) => format!("{e}\nhint: install ripgrep (rg) for better results"),
        }
    }
}

// ── glob_files ────────────────────────────────────────────────────────────────

/// glob_files <pattern>
/// Prefers fd over find. Falls back gracefully with a clear error.
pub fn glob_files(pattern: &str, working_dir: &Path) -> String {
    let pattern = pattern.trim();
    let name_pat = pattern.split('/').last().unwrap_or(pattern);

    if tool_available("fd") {
        // fd: faster, respects .gitignore, excludes hidden dirs automatically
        match run_tool("fd", &["--type", "f", "--color", "never", "--glob", name_pat], working_dir) {
            Ok(out) => {
                if out.trim().is_empty() { "No files matched.".to_string() }
                else {
                    let mut lines: Vec<&str> = out.lines().collect();
                    lines.sort();
                    lines.join("\n")
                }
            }
            Err(e) => e,
        }
    } else {
        match run_tool(
            "find",
            &[
                ".", "-name", name_pat,
                "-not", "-path", "*/target/*",
                "-not", "-path", "*/.git/*",
                "-not", "-path", "*/node_modules/*",
            ],
            working_dir,
        ) {
            Ok(out) => {
                if out.trim().is_empty() { "No files matched.".to_string() }
                else {
                    let mut lines: Vec<&str> = out.lines().collect();
                    lines.sort();
                    lines.join("\n")
                }
            }
            Err(e) => format!("{e}\nhint: install fd for better results"),
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Check if a binary exists on PATH without spawning a shell.
pub fn tool_available(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a command with separate args (no shell interpolation).
/// Returns Ok(stdout) or Err("toolname: reason").
fn run_tool(program: &str, args: &[&str], working_dir: &Path) -> Result<String, String> {
    match std::process::Command::new(program)
        .args(args)
        .current_dir(working_dir)
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            if out.status.success() || !stdout.is_empty() {
                Ok(stdout)
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                let msg = if stderr.is_empty() { "exited with error".to_string() } else { stderr.trim().to_string() };
                Err(format!("error ({program}): {msg}"))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("error: '{program}' not found on PATH"))
        }
        Err(e) => Err(format!("error ({program}): {e}")),
    }
}

fn truncate_results(output: &str) -> String {
    if output.trim().is_empty() {
        return "No matches found.".to_string();
    }
    let lines: Vec<&str> = output.lines().take(MAX_GREP_RESULTS).collect();
    let mut result = lines.join("\n");
    if lines.len() == MAX_GREP_RESULTS {
        result.push_str(&format!("\n[truncated at {MAX_GREP_RESULTS} results]"));
    }
    result
}

fn resolve(path_str: &str, working_dir: &Path) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() { p.to_path_buf() } else { working_dir.join(p) }
}
