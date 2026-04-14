use std::path::Path;

#[derive(Clone)]
pub enum SynapseKind {
    Tool { command: String, usage: String, example: String },
}

#[derive(Clone)]
pub struct Synapse {
    pub trigger: String,
    pub description: String,
    pub kind: SynapseKind,
}

#[derive(Clone)]
pub struct Neuron {
    pub name: String,
    pub description: String,
    pub shell: bool,           // passthrough: any trigger is executed as a shell command
    pub system_prompt: String, // concatenated thoughts — injected into the system message
    pub example: String,       // few-shot example from neuron.toml body
    pub synapses: Vec<Synapse>,
}

// Built-in neurons embedded at compile time
const BUILTIN_TERMINAL_META: &str         = include_str!("../neurons/terminal/neuron.toml");
const BUILTIN_TERMINAL_THOUGHTS: &[&str]  = &[
    include_str!("../neurons/terminal/thoughts/rules.md"),
];
const BUILTIN_KNOWLEDGE_META: &str        = include_str!("../neurons/knowledge/neuron.toml");
const BUILTIN_KNOWLEDGE_THOUGHTS: &[&str] = &[
    include_str!("../neurons/knowledge/thoughts/knowledge.md"),
    include_str!("../neurons/knowledge/thoughts/transparency.md"),
];

pub fn built_ins() -> Vec<Neuron> {
    vec![
        parse_neuron(BUILTIN_KNOWLEDGE_META,  BUILTIN_KNOWLEDGE_THOUGHTS, &[]),
        parse_neuron(BUILTIN_TERMINAL_META,  BUILTIN_TERMINAL_THOUGHTS, &[]),
    ]
}

/// Scans `base` for subdirectories, each treated as a neuron.
/// Each subdirectory may contain:
///   - `neuron.toml`       — name and description
///   - `thoughts/*.md`     — instructions injected into the system message
///   - `synapses/*.toml`   — synapse (tool) definitions
pub fn load_from_dir(base: &Path) -> Vec<Neuron> {
    let Ok(entries) = std::fs::read_dir(base) else { return vec![] };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let neuron_dir = e.path();
            let meta = std::fs::read_to_string(neuron_dir.join("neuron.toml"))
                .unwrap_or_default();

            // load all .md files from thoughts/ sorted by filename
            let mut thought_entries: Vec<(String, String)> =
                std::fs::read_dir(neuron_dir.join("thoughts"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("md"))
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        let src  = std::fs::read_to_string(e.path()).ok()?;
                        Some((name, src))
                    })
                    .collect();
            thought_entries.sort_by(|a, b| a.0.cmp(&b.0));
            let thought_sources: Vec<String> = thought_entries.into_iter().map(|(_, s)| s).collect();
            let thought_refs: Vec<&str> = thought_sources.iter().map(String::as_str).collect();

            // load all .toml files from synapses/
            let synapse_sources: Vec<String> =
                std::fs::read_dir(neuron_dir.join("synapses"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("toml"))
                    .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                    .collect();
            let synapse_refs: Vec<&str> = synapse_sources.iter().map(String::as_str).collect();

            let mut neuron = parse_neuron(&meta, &thought_refs, &synapse_refs);
            if neuron.name.is_empty() {
                neuron.name = e.file_name().to_string_lossy().to_string();
            }
            Some(neuron)
        })
        .collect()
}

/// Builds the system message injected at the start of every conversation.
pub fn build_tool_context(neurons: &[Neuron]) -> String {
    let active: Vec<&Neuron> = neurons
        .iter()
        .filter(|n| {
            n.shell
                || !n.system_prompt.is_empty()
                || n.synapses.iter().any(|s| matches!(s.kind, SynapseKind::Tool { .. }))
        })
        .collect();

    if active.is_empty() {
        return String::new();
    }

    let names: Vec<&str> = active.iter().map(|n| n.name.as_str()).collect();
    let mut out = format!(
        "Loaded neurons: {}\n\nEach neuron is described below:\n",
        names.join(", ")
    );

    for neuron in &active {
        out.push('\n');
        out.push_str(&neuron.name);
        if !neuron.description.is_empty() {
            out.push_str(" — ");
            out.push_str(&neuron.description);
        }
        out.push_str(":\n");

        if neuron.shell {
            out.push_str("  To run a Linux command, wrap it in a tool tag. For example: <tool>ls</tool> or <tool>cat README.md</tool>\n");
        }
        for s in &neuron.synapses {
            let SynapseKind::Tool { usage, .. } = &s.kind;
            out.push_str(&format!("  - {}: {}\n", usage, s.description));
        }
        if !neuron.system_prompt.is_empty() {
            out.push_str(&format!("  {}\n", neuron.system_prompt.trim()));
        }
    }

    out.push_str(
        "\nWhen you need to run a command, output it on its own line wrapped in tool tags.\n\
         You will receive the result under \"Tool result:\" and can then continue.\n",
    );

    // examples: neuron-level first, then per-synapse
    let mut examples: Vec<&str> = Vec::new();
    for neuron in &active {
        if !neuron.example.is_empty() {
            examples.push(&neuron.example);
        }
        for s in &neuron.synapses {
            let SynapseKind::Tool { example, .. } = &s.kind;
            if !example.is_empty() {
                examples.push(example.as_str());
            }
        }
    }

    if !examples.is_empty() {
        out.push('\n');
        out.push_str(&examples.join("\n"));
    }

    out
}

// ── parsers ───────────────────────────────────────────────────────────────────

fn parse_neuron(meta_src: &str, thoughts: &[&str], synapse_srcs: &[&str]) -> Neuron {
    let (header, body) = match meta_src.find("\n---\n") {
        Some(i) => (&meta_src[..i], meta_src[i + 5..].trim()),
        None    => (meta_src.trim(), ""),
    };

    let mut name        = String::new();
    let mut description = String::new();
    let mut shell       = false;

    for line in header.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "name"        => name        = v.trim().to_string(),
                "description" => description = v.trim().to_string(),
                "shell"       => shell       = v.trim() == "true",
                _ => {}
            }
        }
    }

    let system_prompt = thoughts
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let synapses = synapse_srcs.iter().filter_map(|src| parse_synapse(src)).collect();
    Neuron { name, description, shell, system_prompt, example: body.to_string(), synapses }
}

fn parse_synapse(src: &str) -> Option<Synapse> {
    let (header, body) = match src.find("\n---\n") {
        Some(i) => (&src[..i], src[i + 5..].trim()),
        None    => (src.trim(), ""),
    };

    let mut trigger     = String::new();
    let mut kind_str    = String::new();
    let mut command     = String::new();
    let mut description = String::new();
    let mut usage       = String::new();

    for line in header.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "trigger"     => trigger     = v.trim().to_string(),
                "kind"        => kind_str    = v.trim().to_string(),
                "command"     => command     = v.trim().to_string(),
                "description" => description = v.trim().to_string(),
                "usage"       => usage       = v.trim().to_string(),
                _ => {}
            }
        }
    }

    if trigger.is_empty() {
        return None;
    }

    let kind = match kind_str.as_str() {
        "tool" => SynapseKind::Tool {
            command,
            usage: if usage.is_empty() { trigger.clone() } else { usage },
            example: body.to_string(),
        },
        _ => return None,
    };

    Some(Synapse { trigger, description, kind })
}
