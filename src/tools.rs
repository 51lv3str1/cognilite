use std::path::{Path, PathBuf};
use std::fs;

const MAX_READ_LINES: usize = 500;
const MAX_GREP_RESULTS: usize = 100;

// ── read_file ─────────────────────────────────────────────────────────────────

/// read_file <path> [start_line [end_line]]
/// Returns line-numbered content. Lines are 1-indexed.
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
            end - cap,
            parts[0],
            cap + 1,
            end
        ));
    }

    format!("{} ({} lines)\n{}", parts[0], total, out)
}

// ── write_file ────────────────────────────────────────────────────────────────

/// write_file <path>\n<content>
/// Creates or overwrites the file. Content starts after the first newline.
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
/// old string (exact, including newlines)
/// <<<REPLACE
/// new string
///
/// Replaces the first occurrence of old string with new string.
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
/// Searches for pattern recursively. Path defaults to ".".
/// Pattern and path are passed as separate args — no shell interpolation.
pub fn grep_files(args: &str, working_dir: &Path) -> String {
    let (pattern, search_path) = args.split_once(' ').unwrap_or((args, "."));
    let search_path = if search_path.trim().is_empty() { "." } else { search_path.trim() };

    let output = std::process::Command::new("grep")
        .args([
            "-rn",
            "--color=never",
            "-m", "5",          // max 5 matches per file
            "--exclude-dir=target",
            "--exclude-dir=.git",
            "--exclude-dir=node_modules",
            pattern,
            search_path,
        ])
        .current_dir(working_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().is_empty() {
                "No matches found.".to_string()
            } else {
                let lines: Vec<&str> = stdout.lines().take(MAX_GREP_RESULTS).collect();
                let mut result = lines.join("\n");
                if lines.len() == MAX_GREP_RESULTS {
                    result.push_str(&format!("\n[truncated at {MAX_GREP_RESULTS} results]"));
                }
                result
            }
        }
        Err(e) => format!("error: {e}"),
    }
}

// ── glob_files ────────────────────────────────────────────────────────────────

/// glob_files <pattern>
/// Lists files matching the pattern. Supports * and **.
/// Examples: glob_files *.rs   glob_files **/*.toml   glob_files src/*.rs
pub fn glob_files(pattern: &str, working_dir: &Path) -> String {
    let pattern = pattern.trim();
    // extract the filename component for -name flag
    let name_pat = pattern.split('/').last().unwrap_or(pattern);

    let output = std::process::Command::new("find")
        .args([
            ".",
            "-name", name_pat,
            "-not", "-path", "*/target/*",
            "-not", "-path", "*/.git/*",
            "-not", "-path", "*/node_modules/*",
        ])
        .current_dir(working_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.trim().is_empty() {
                "No files matched.".to_string()
            } else {
                let mut lines: Vec<&str> = stdout.lines().collect();
                lines.sort();
                lines.join("\n")
            }
        }
        Err(e) => format!("error: {e}"),
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn resolve(path_str: &str, working_dir: &Path) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() { p.to_path_buf() } else { working_dir.join(p) }
}
