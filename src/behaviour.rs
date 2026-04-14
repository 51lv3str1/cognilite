use std::path::Path;

#[derive(Clone)]
pub enum BehaviourKind {
    Tool { action: String, usage: String, example: String },
}

pub struct Behaviour {
    pub trigger: String,
    pub description: String,
    pub kind: BehaviourKind,
}

const BUILTIN_SOURCES: &[&str] = &[
    include_str!("../behaviours/cat.toml"),
];

pub fn built_ins() -> Vec<Behaviour> {
    BUILTIN_SOURCES
        .iter()
        .filter_map(|src| parse(src))
        .collect()
}

pub fn load_from_dir(dir: &Path) -> Vec<Behaviour> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("toml"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|src| parse(&src))
        .collect()
}

/// Builds the tool context block injected at the start of every conversation.
/// Lists available tools and their few-shot examples so the model knows how to use them.
pub fn build_tool_context(behaviours: &[Behaviour]) -> String {
    let tools: Vec<&Behaviour> = behaviours
        .iter()
        .filter(|b| matches!(b.kind, BehaviourKind::Tool { .. }))
        .collect();

    if tools.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("You have access to the following tools:\n");
    for b in &tools {
        let BehaviourKind::Tool { usage, .. } = &b.kind;
        out.push_str(&format!("- {}: {}\n", usage, b.description));
    }
    out.push_str(
        "\nTo use a tool, output on its own line: <tool>name args</tool>\n\
         You will receive the result under \"Tool result:\" and can then continue.\n",
    );

    let examples: String = tools
        .iter()
        .filter_map(|b| {
            let BehaviourKind::Tool { example, .. } = &b.kind;
            if !example.is_empty() { Some(example.as_str()) } else { None }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !examples.is_empty() {
        out.push('\n');
        out.push_str(&examples);
    }

    out
}

// ── file format ───────────────────────────────────────────────────────────────
//
// key = value header lines, then optional `---` followed by the few-shot example body.
//
// tool:
//   trigger = cat
//   kind = tool
//   action = cat
//   description = Read a file and return its contents
//   usage = cat <path>
//   ---
//   User: ...
//   Assistant: <tool>cat src/main.rs</tool>
//   Tool result:
//   ...
//
// prompt:
//   trigger = explain
//   kind = prompt
//   description = Explain code
//   ---
//   [CODE]
//   example
//   [EXPLANATION]
//   answer
//   [CODE]
//   {{input}}
//   [EXPLANATION]

fn parse(src: &str) -> Option<Behaviour> {
    let (header, body) = match src.find("\n---\n") {
        Some(i) => (&src[..i], src[i + 5..].trim()),
        None    => (src.trim(), ""),
    };

    let mut trigger     = String::new();
    let mut kind_str    = String::new();
    let mut action      = String::new();
    let mut description = String::new();
    let mut usage       = String::new();

    for line in header.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "trigger"     => trigger     = v.trim().to_string(),
                "kind"        => kind_str    = v.trim().to_string(),
                "action"      => action      = v.trim().to_string(),
                "description" => description = v.trim().to_string(),
                "usage"       => usage       = v.trim().to_string(),
                _             => {}
            }
        }
    }

    if trigger.is_empty() {
        return None;
    }

    let kind = match kind_str.as_str() {
        "tool" => BehaviourKind::Tool {
            action,
            usage: if usage.is_empty() { trigger.clone() } else { usage },
            example: body.to_string(),
        },
        _ => return None,
    };

    Some(Behaviour { trigger, description, kind })
}
