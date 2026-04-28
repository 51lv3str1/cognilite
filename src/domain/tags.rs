// Pure tag parsing for the cognilite protocol. No I/O, no App state — just
// string scanning. The runtime layer interprets the results.

#[derive(Debug, Clone, PartialEq)]
pub enum AskKind {
    Text,            // free text — user types, Enter submits
    Confirm,         // yes/no — y/Enter = Yes, n/Esc = No
    Choice(Vec<String>), // pick one — ↑↓ navigate, Enter selects
}

#[derive(Debug, Clone)]
pub struct InputRequest {
    pub question: String, // shown in UI (empty for Choice — question is in model's preceding text)
    pub kind: AskKind,
}

/// Returns true if `pos` falls inside a triple-backtick code fence in `text`.
pub fn is_in_code_block(text: &str, pos: usize) -> bool {
    text[..pos].matches("```").count() % 2 == 1
}

/// Returns the slice of `content` past any closed `</think>` block. If a
/// `<think>` is open but unclosed, returns None — we're still inside reasoning
/// and tags should be ignored.
fn after_think(content: &str) -> Option<&str> {
    match content.rfind("</think>") {
        Some(i) => Some(&content[i + 8..]),
        None => {
            if content.contains("<think>") { None } else { Some(content) }
        }
    }
}

/// Generic paired-tag extractor: finds `<name>...</name>` outside `<think>`
/// blocks and code fences. Returns the inner content.
pub fn extract_tag<'a>(content: &'a str, name: &str) -> Option<&'a str> {
    let scan = after_think(content)?;
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let tag_pos = scan.find(open.as_str())?;
    if is_in_code_block(scan, tag_pos) { return None; }
    let start = tag_pos + open.len();
    let end = scan.find(close.as_str())?;
    if end >= start { Some(&scan[start..end]) } else { None }
}

/// Strip a complete `<name ...>...</name>` tag (open + body + close) from `content`,
/// leaving the surrounding text. Operates on the part past `</think>`. Tolerates
/// attributes on the opening tag (e.g. `<finding severity="high">`). Returns true
/// if a tag was found and removed.
pub fn strip_tag(content: &mut String, name: &str) -> bool {
    let scan_from = content.rfind("</think>").map(|i| i + 8).unwrap_or(0);
    let prefix = format!("<{name}");
    let close  = format!("</{name}>");

    // find a real opening: `<name` followed by `>`, whitespace, or `/`.
    // skips false matches like `<moody>` when looking for `mood`.
    let mut start = scan_from;
    let abs_open = loop {
        let Some(rel) = content[start..].find(&prefix) else { return false };
        let abs = start + rel;
        let next = content.as_bytes().get(abs + prefix.len()).copied();
        if matches!(next, Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'/')) {
            break abs;
        }
        start = abs + 1;
    };

    let Some(rel_open_end) = content[abs_open..].find('>') else { return false };
    let abs_open_end = abs_open + rel_open_end + 1;
    let Some(rel_close) = content[abs_open_end..].find(&close) else { return false };
    let abs_end = abs_open_end + rel_close + close.len();
    // Preserve byte positions of `before`: do NOT trim_end. Headless prints
    // bytes incrementally and tracks `printed_up_to` against this content; a
    // trim would shift the suffix and split tokens already on stdout. A
    // double-space artifact between `before` and `after` is acceptable —
    // markdown collapses it in TUI render and remains readable in headless.
    content.replace_range(abs_open..abs_end, "");
    true
}

/// Detects a complete `<tool>...</tool>` tag in content, ignoring anything
/// inside `<think>...</think>` blocks (model reasoning) or code fences.
pub fn extract_tool_call(content: &str) -> Option<&str> {
    extract_tag(content, "tool")
}

/// Detects `<ask>`, `<ask type="confirm">`, `<ask type="choice">` tags in content,
/// skipping anything inside `<think>` blocks. Returns (kind, question).
pub fn extract_ask_tag(content: &str) -> Option<(AskKind, String)> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let open = scan.find("<ask")?;
    if is_in_code_block(scan, open) { return None; }
    let tag_close = scan[open..].find('>')?;
    let inner_start = open + tag_close + 1;
    let close = scan.find("</ask>")?;
    if close < inner_start { return None; }

    let tag_str = &scan[open..open + tag_close + 1];
    let inner = scan[inner_start..close].trim();

    if tag_str.contains("type=\"confirm\"") {
        Some((AskKind::Confirm, inner.to_string()))
    } else if tag_str.contains("type=\"choice\"") {
        let options: Vec<String> = inner.split('|')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if options.is_empty() { return None; }
        Some((AskKind::Choice(options), String::new()))
    } else {
        Some((AskKind::Text, inner.to_string()))
    }
}

/// Detects a complete `<patch>...</patch>` tag outside think blocks. Returns the raw diff content.
pub fn extract_patch_tag(content: &str) -> Option<String> {
    extract_tag(content, "patch").map(str::to_string)
}

/// Extract path from `<preview path="..."/>` tag (outside think blocks).
pub fn extract_preview_tag(content: &str) -> Option<String> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let start = scan.find("<preview")?;
    if is_in_code_block(scan, start) { return None; }
    let tag_str = &scan[start..];
    let end = tag_str.find("/>")?;
    let inner = &tag_str[8..end]; // skip "<preview"
    let path_start = inner.find("path=\"")? + 6;
    let path_end = inner[path_start..].find('"')? + path_start;
    Some(inner[path_start..path_end].to_string())
}

/// Detects a complete `<mood>...</mood>` tag outside think blocks. Returns the emoji string.
pub fn extract_mood_tag(content: &str) -> Option<String> {
    extract_tag(content, "mood").map(|s| s.trim().to_string())
}

pub fn extract_load_neuron_tag(content: &str) -> Option<String> {
    extract_tag(content, "load_neuron").map(|s| s.trim().to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub severity: String, // "high" | "med" | "low" — kept as string so unknown values pass through
    pub file:     String, // "path:line" — empty if not provided
    pub category: String, // "security" | "tech-debt" | "bug" | "perf" | "style" | ""
    pub body:     String, // free-form description (trimmed)
}

impl Finding {
    /// Markdown bullet for the consolidated report.
    pub fn to_markdown(&self) -> String {
        let mut head = format!("**[{}]**", self.severity);
        if !self.category.is_empty() { head.push_str(&format!(" *{}* ", self.category)); }
        if !self.file.is_empty()     { head.push_str(&format!(" `{}`",  self.file)); }
        format!("- {head}\n  {}", self.body.replace('\n', "\n  "))
    }
}

/// Extract a single attribute value from an opening tag string.
/// Returns "" if missing. Tolerant of single or double quotes.
fn parse_attr(tag: &str, name: &str) -> String {
    for q in ['"', '\''] {
        let needle = format!("{name}={q}");
        if let Some(s) = tag.find(&needle) {
            let after = s + needle.len();
            if let Some(e) = tag[after..].find(q) {
                return tag[after..after + e].to_string();
            }
        }
    }
    String::new()
}

/// Detects a complete `<finding ...>body</finding>` outside `<think>` and code fences.
/// Reads optional attributes `severity`, `file`, `category`. Returns None until both
/// the opening `>` and the closing `</finding>` are present in the stream.
pub fn extract_finding_tag(content: &str) -> Option<Finding> {
    let scan = after_think(content)?;
    let open = scan.find("<finding")?;
    if is_in_code_block(scan, open) { return None; }
    let tag_close = scan[open..].find('>')?;
    let inner_start = open + tag_close + 1;
    let close = scan[inner_start..].find("</finding>")?;
    let tag_str = &scan[open..open + tag_close + 1];
    let body = scan[inner_start..inner_start + close].trim().to_string();
    Some(Finding {
        severity: parse_attr(tag_str, "severity"),
        file:     parse_attr(tag_str, "file"),
        category: parse_attr(tag_str, "category"),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_tool_skips_inside_think() {
        let c = "<think>plan: <tool>ls</tool></think> ok no tool";
        assert_eq!(extract_tool_call(c), None);
    }

    #[test]
    fn extract_tool_picks_up_after_think_close() {
        let c = "<think>plan</think>\nrunning <tool>ls</tool>";
        assert_eq!(extract_tool_call(c), Some("ls"));
    }

    #[test]
    fn extract_tool_returns_none_when_think_open_unclosed() {
        let c = "<think>still reasoning <tool>ls</tool>";
        assert_eq!(extract_tool_call(c), None);
    }

    #[test]
    fn extract_tool_skips_inside_code_fence() {
        let c = "use ```\n<tool>ls</tool>\n``` to list";
        assert_eq!(extract_tool_call(c), None);
    }

    #[test]
    fn extract_tool_finds_outside_fences() {
        let c = "```\nignored\n```\nrun this: <tool>cat foo</tool>";
        assert_eq!(extract_tool_call(c), Some("cat foo"));
    }

    #[test]
    fn extract_tag_generic_works() {
        assert_eq!(extract_tag("<mood>😊</mood>", "mood"), Some("😊"));
        assert_eq!(extract_tag("<load_neuron>Hippocampus</load_neuron>", "load_neuron"), Some("Hippocampus"));
    }

    #[test]
    fn strip_tag_removes_pair() {
        // strip preserves byte positions — produces a double space between
        // "before " and " after". This is intentional: see strip_tag comment.
        let mut s = "before <mood>😊</mood> after".to_string();
        assert!(strip_tag(&mut s, "mood"));
        assert_eq!(s, "before  after");
    }

    #[test]
    fn strip_tag_returns_false_when_absent() {
        let mut s = "no tags here".to_string();
        assert!(!strip_tag(&mut s, "mood"));
        assert_eq!(s, "no tags here");
    }

    #[test]
    fn extract_ask_text() {
        let (k, q) = extract_ask_tag("<ask>what?</ask>").unwrap();
        assert_eq!(q, "what?");
        assert!(matches!(k, AskKind::Text));
    }

    #[test]
    fn extract_ask_confirm() {
        let (k, q) = extract_ask_tag(r#"<ask type="confirm">go?</ask>"#).unwrap();
        assert_eq!(q, "go?");
        assert!(matches!(k, AskKind::Confirm));
    }

    #[test]
    fn extract_ask_choice() {
        let (k, _) = extract_ask_tag(r#"<ask type="choice">a | b | c</ask>"#).unwrap();
        match k {
            AskKind::Choice(opts) => assert_eq!(opts, vec!["a", "b", "c"]),
            _ => panic!("expected Choice"),
        }
    }

    #[test]
    fn extract_preview_path() {
        assert_eq!(extract_preview_tag(r#"<preview path="src/foo.rs"/>"#), Some("src/foo.rs".to_string()));
    }

    #[test]
    fn extract_mood_trims() {
        assert_eq!(extract_mood_tag("<mood>  😊  </mood>"), Some("😊".to_string()));
    }

    #[test]
    fn extract_finding_full_attrs() {
        let s = r#"prose <finding severity="high" file="src/app.rs:42" category="security">
shell passthrough without a gate.
Fix: match destructive commands in handle_tool_call.
</finding> tail"#;
        let f = extract_finding_tag(s).unwrap();
        assert_eq!(f.severity, "high");
        assert_eq!(f.file, "src/app.rs:42");
        assert_eq!(f.category, "security");
        assert!(f.body.contains("shell passthrough"));
        assert!(f.body.contains("Fix:"));
    }

    #[test]
    fn extract_finding_missing_attrs_ok() {
        let s = "<finding>just a body</finding>";
        let f = extract_finding_tag(s).unwrap();
        assert_eq!(f.severity, "");
        assert_eq!(f.file, "");
        assert_eq!(f.category, "");
        assert_eq!(f.body, "just a body");
    }

    #[test]
    fn extract_finding_skips_inside_think() {
        let s = "<think><finding severity=\"high\">x</finding></think> ok";
        assert!(extract_finding_tag(s).is_none());
    }

    #[test]
    fn extract_finding_returns_none_when_unclosed() {
        let s = "<finding severity=\"high\">still streaming";
        assert!(extract_finding_tag(s).is_none());
    }

    #[test]
    fn strip_tag_handles_attributes() {
        let mut s = r#"a <finding severity="high" file="x:1">body</finding> b"#.to_string();
        assert!(strip_tag(&mut s, "finding"));
        // double space preserved on purpose — see strip_tag comment
        assert_eq!(s, "a  b");
    }

    #[test]
    fn strip_tag_does_not_match_prefix() {
        // "<moody>" must not be stripped when asked to strip "mood"
        let mut s = "<moody>x</moody>".to_string();
        assert!(!strip_tag(&mut s, "mood"));
        assert_eq!(s, "<moody>x</moody>");
    }

    #[test]
    fn finding_to_markdown_includes_attrs() {
        let f = Finding {
            severity: "high".into(),
            file: "src/app.rs:42".into(),
            category: "security".into(),
            body: "first line\nsecond line".into(),
        };
        let md = f.to_markdown();
        assert!(md.contains("[high]"));
        assert!(md.contains("*security*"));
        assert!(md.contains("`src/app.rs:42`"));
        // multiline body must be indented for nested bullet rendering
        assert!(md.contains("first line\n  second line"));
    }
}
