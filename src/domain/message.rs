use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    Tool, // tool results — sent to the model as "user" turns
}

impl Role {
    pub fn to_api_str(&self) -> &'static str {
        match self {
            Role::User | Role::Tool => "user",
            Role::Assistant         => "assistant",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenStats {
    pub response_tokens: u64,
    pub tokens_per_sec: f64,
    pub thinking_secs: Option<f64>, // time until first content token (thinking phase only)
    pub wall_secs: f64,             // total wall-clock time from send to done
    pub prompt_eval_count: u64,     // tokens Ollama actually re-evaluated (0 = cache hit)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttachmentKind {
    Text,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub path: PathBuf,
    pub kind: AttachmentKind,
    pub size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,       // display content (without file bodies)
    pub llm_content: String,   // content sent to model (includes file bodies)
    pub images: Vec<String>,   // base64 images
    pub attachments: Vec<Attachment>,
    pub thinking: String,
    pub thinking_secs: Option<f64>, // set on intermediate messages interrupted by a tool call
    pub stats: Option<TokenStats>,
    pub tool_call: Option<String>,     // "Neuron › trigger" label, set for Role::Tool messages
    pub tool_collapsed: bool,          // tool result body hidden by default
}
