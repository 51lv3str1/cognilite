use std::path::PathBuf;
use serde::Deserialize;

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

#[derive(Debug, Clone, Deserialize)]
pub struct NeuronPreset {
    pub name: String,
    pub enabled: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ConfigFile {
    ctx_strategy: Option<String>,
    disabled_neurons: Vec<String>,
    on_demand_neurons: Vec<String>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    repeat_penalty: Option<f64>,
    thinking_budget: Option<f64>,
    ctx_pow2: Option<bool>,
    keep_alive: Option<bool>,
    warmup: Option<bool>,
    thinking: Option<bool>,
    neuron_mode: Option<String>,
    neuron_presets: Vec<NeuronPreset>,
    active_preset: Option<String>,
    username: Option<String>,
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
    let file: ConfigFile = config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    Config {
        ctx_strategy: file.ctx_strategy.as_deref().map(CtxStrategy::from_str).unwrap_or(CtxStrategy::Dynamic),
        disabled_neurons: file.disabled_neurons.into_iter().collect(),
        on_demand_neurons: file.on_demand_neurons.into_iter().collect(),
        gen_params: [
            file.temperature.unwrap_or(GEN_PARAMS[0].2),
            file.top_p.unwrap_or(GEN_PARAMS[1].2),
            file.repeat_penalty.unwrap_or(GEN_PARAMS[2].2),
            file.thinking_budget.unwrap_or(GEN_PARAMS[3].2),
        ],
        ctx_pow2: file.ctx_pow2.unwrap_or(true),
        keep_alive: file.keep_alive.unwrap_or(false),
        warmup: file.warmup.unwrap_or(true),
        thinking: file.thinking.unwrap_or(true),
        neuron_mode: file.neuron_mode.as_deref().map(NeuronMode::from_str).unwrap_or(NeuronMode::Manual),
        neuron_presets: file.neuron_presets,
        active_preset: file.active_preset,
        username: file.username.filter(|s| !s.is_empty()).unwrap_or_else(default_username),
    }
}
