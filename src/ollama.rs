use serde::{Deserialize, Serialize};
use std::io::BufRead;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    name: String,
    size: Option<u64>, // bytes on disk
    details: Option<ModelDetails>,
}

#[derive(Debug, Deserialize)]
struct ModelDetails {
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub name: String,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>, // base64-encoded, for vision models
}

#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub message: Option<ChatMessage>,
    pub done: bool,
    pub error: Option<String>,
    // present only in the final chunk (done: true)
    pub prompt_eval_count: Option<u64>,
    pub eval_count: Option<u64>,
    pub eval_duration: Option<u64>, // nanoseconds
}

pub fn fetch_context_length(base_url: &str, model: &str) -> Option<u64> {
    let url = format!("{}/api/show", base_url);
    let resp = ureq::post(&url)
        .send_json(serde_json::json!({ "model": model }))
        .ok()?;
    let body: serde_json::Value = resp.into_json().ok()?;
    // model_info keys look like "llama.context_length", "qwen2.context_length", etc.
    let info = body.get("model_info")?.as_object()?;
    for (key, val) in info {
        if key.ends_with(".context_length") {
            return val.as_u64();
        }
    }
    None
}

pub fn list_models(base_url: &str) -> Result<Vec<ModelEntry>, String> {
    let url = format!("{}/api/tags", base_url);
    let response = ureq::get(&url)
        .call()
        .map_err(|e| format!("Cannot connect to Ollama: {e}"))?;
    let tags: TagsResponse = response
        .into_json()
        .map_err(|e| format!("Invalid response: {e}"))?;
    Ok(tags.models.into_iter().map(|m| ModelEntry {
        name: m.name,
        parameter_size: m.details.as_ref().and_then(|d| d.parameter_size.clone()),
        quantization_level: m.details.as_ref().and_then(|d| d.quantization_level.clone()),
        size_bytes: m.size,
    }).collect())
}

pub fn stream_chat(
    base_url: &str,
    model: String,
    messages: Vec<ChatMessage>,
    num_ctx: Option<u64>,
    gen_params: [f64; 3],
    tx: Sender<StreamChunk>,
) {
    let url = format!("{}/api/chat", base_url);

    let mut options = serde_json::json!({
        "temperature":    gen_params[0],
        "top_p":          gen_params[1],
        "repeat_penalty": gen_params[2],
    });
    if let Some(ctx) = num_ctx {
        options["num_ctx"] = serde_json::json!(ctx);
    }

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "options": options,
    });

    let response = match ureq::post(&url).send_json(body) {
        Ok(r) => r,
        Err(e) => {
            tx.send(StreamChunk {
                message: None,
                done: true,
                error: Some(format!("Request failed: {e}")),
                prompt_eval_count: None,
                eval_count: None,
                eval_duration: None,
            })
            .ok();
            return;
        }
    };

    let reader = std::io::BufReader::new(response.into_reader());
    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };
        match serde_json::from_str::<StreamChunk>(&line) {
            Ok(chunk) => {
                let done = chunk.done;
                if tx.send(chunk).is_err() {
                    break;
                }
                if done {
                    break;
                }
            }
            Err(_) => continue,
        }
    }
}
