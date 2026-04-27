use std::path::PathBuf;

/// Generation parameter definitions: (name, description, default, min, max, step)
pub const GEN_PARAMS: &[(&str, &str, f64, f64, f64, f64)] = &[
    ("temperature",     "randomness of output",              0.8,    0.0, 2.0,     0.05),
    ("top_p",           "nucleus sampling cutoff",           0.9,    0.0, 1.0,     0.05),
    ("repeat_penalty",  "repetition penalty",                1.1,    0.5, 2.0,     0.05),
    ("thinking_budget", "max thinking tokens (0=unlimited)", 0.0,    0.0, 32768.0, 512.0),
];

#[derive(Debug, Clone, PartialEq)]
pub enum CtxStrategy {
    Dynamic, // max(8192, used_tokens * 2) — faster, smaller KV cache
    Full,    // model's max context length — slower but never truncates history
}

impl CtxStrategy {
    pub fn index(&self) -> usize {
        match self { CtxStrategy::Dynamic => 0, CtxStrategy::Full => 1 }
    }
    pub fn from_index(i: usize) -> Self {
        match i { 1 => CtxStrategy::Full, _ => CtxStrategy::Dynamic }
    }
    pub fn as_str(&self) -> &'static str {
        match self { CtxStrategy::Dynamic => "dynamic", CtxStrategy::Full => "full" }
    }
    pub fn from_str(s: &str) -> Self {
        match s { "full" => CtxStrategy::Full, _ => CtxStrategy::Dynamic }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NeuronMode { Manual, Smart, Presets }

impl NeuronMode {
    pub fn as_str(&self) -> &'static str {
        match self { NeuronMode::Manual => "manual", NeuronMode::Smart => "smart", NeuronMode::Presets => "presets" }
    }
    pub fn from_str(s: &str) -> Self {
        match s { "smart" => NeuronMode::Smart, "presets" => NeuronMode::Presets, _ => NeuronMode::Manual }
    }
}

#[derive(Debug, Clone)]
pub struct NeuronPreset {
    pub name: String,
    pub enabled: Vec<String>,
}

pub struct Config {
    pub ctx_strategy: CtxStrategy,
    pub disabled_neurons: std::collections::HashSet<String>,
    // Smart mode: neurons that start unloaded and are pulled in via <load_neuron>.
    // Any enabled neuron NOT in this set is included in the initial system prompt.
    pub on_demand_neurons: std::collections::HashSet<String>,
    pub gen_params: [f64; 4],
    pub ctx_pow2: bool,
    pub keep_alive: bool,
    pub warmup: bool,
    pub thinking: bool,
    pub neuron_mode: NeuronMode,
    pub neuron_presets: Vec<NeuronPreset>,
    pub active_preset: Option<String>,
    pub username: String,
}

pub fn config_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config/cognilite/config.json"))
}

pub fn default_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

pub fn load_config() -> Config {
    let default = Config {
        ctx_strategy: CtxStrategy::Dynamic,
        disabled_neurons: Default::default(),
        on_demand_neurons: Default::default(),
        gen_params: [GEN_PARAMS[0].2, GEN_PARAMS[1].2, GEN_PARAMS[2].2, GEN_PARAMS[3].2],
        ctx_pow2: true, keep_alive: false, warmup: true, thinking: true,
        neuron_mode: NeuronMode::Manual, neuron_presets: Vec::new(), active_preset: None,
        username: default_username(),
    };
    let path = match config_path() { Some(p) => p, None => return default };
    let Ok(text) = std::fs::read_to_string(&path) else { return default };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else { return default };
    let ctx_strategy = val.get("ctx_strategy")
        .and_then(|v| v.as_str()).map(CtxStrategy::from_str).unwrap_or(CtxStrategy::Dynamic);
    let disabled_neurons = val.get("disabled_neurons")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let on_demand_neurons = val.get("on_demand_neurons")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let gen_params = [
        val.get("temperature").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[0].2),
        val.get("top_p").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[1].2),
        val.get("repeat_penalty").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[2].2),
        val.get("thinking_budget").and_then(|v| v.as_f64()).unwrap_or(GEN_PARAMS[3].2),
    ];
    let ctx_pow2   = val.get("ctx_pow2").and_then(|v| v.as_bool()).unwrap_or(true);
    let keep_alive = val.get("keep_alive").and_then(|v| v.as_bool()).unwrap_or(false);
    let warmup     = val.get("warmup").and_then(|v| v.as_bool()).unwrap_or(true);
    let thinking   = val.get("thinking").and_then(|v| v.as_bool()).unwrap_or(true);
    let neuron_mode = val.get("neuron_mode").and_then(|v| v.as_str())
        .map(NeuronMode::from_str).unwrap_or(NeuronMode::Manual);
    let neuron_presets = val.get("neuron_presets").and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|p| {
            let name    = p.get("name")?.as_str()?.to_string();
            let enabled = p.get("enabled")?.as_array()?
                .iter().filter_map(|v| v.as_str().map(String::from)).collect();
            Some(NeuronPreset { name, enabled })
        }).collect())
        .unwrap_or_default();
    let active_preset = val.get("active_preset").and_then(|v| v.as_str()).map(String::from);
    let username = val.get("username").and_then(|v| v.as_str())
        .filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(default_username);
    Config { ctx_strategy, disabled_neurons, on_demand_neurons, gen_params, ctx_pow2, keep_alive, warmup, thinking, neuron_mode, neuron_presets, active_preset, username }
}
