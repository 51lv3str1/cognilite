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

/// Detects a complete `<tool>...</tool>` tag in content, ignoring anything
/// inside `<think>...</think>` blocks (model reasoning) or code fences.
pub fn extract_tool_call(content: &str) -> Option<&str> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let tag_pos = scan.find("<tool>")?;
    if is_in_code_block(scan, tag_pos) { return None; }
    let start = tag_pos + 6;
    let end   = scan.find("</tool>")?;
    if end >= start { Some(&scan[start..end]) } else { None }
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
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let tag_pos = scan.find("<patch>")?;
    if is_in_code_block(scan, tag_pos) { return None; }
    let start = tag_pos + 7;
    let end   = scan.find("</patch>")?;
    if end >= start { Some(scan[start..end].to_string()) } else { None }
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
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let tag_pos = scan.find("<mood>")?;
    if is_in_code_block(scan, tag_pos) { return None; }
    let start = tag_pos + 6;
    let end   = scan.find("</mood>")?;
    if end >= start { Some(scan[start..end].trim().to_string()) } else { None }
}

pub fn extract_load_neuron_tag(content: &str) -> Option<String> {
    let scan = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => {
            if content.contains("<think>") { return None; }
            content
        }
    };
    let tag_pos = scan.find("<load_neuron>")?;
    if is_in_code_block(scan, tag_pos) { return None; }
    let start = tag_pos + 13;
    let end   = scan.find("</load_neuron>")?;
    if end >= start { Some(scan[start..end].trim().to_string()) } else { None }
}
