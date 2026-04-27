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

/// Fetch the chat template string from Ollama. Used to detect template format
/// (ChatML/Llama3/Gemma) for building raw continuation prompts.
pub fn fetch_template(base_url: &str, model: &str) -> Option<String> {
    let url = format!("{}/api/show", base_url);
    let resp = ureq::post(&url)
        .send_json(serde_json::json!({ "model": model }))
        .ok()?;
    let body: serde_json::Value = resp.into_json().ok()?;
    body.get("template")?.as_str().map(String::from)
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

pub fn warmup(base_url: &str, model: String, system_prompt: String, num_ctx: Option<u64>, keep_alive: bool) {
    let url = format!("{}/api/chat", base_url);
    let mut options = serde_json::json!({ "num_predict": 1 });
    if let Some(ctx) = num_ctx {
        options["num_ctx"] = serde_json::json!(ctx);
    }
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user",   "content": " " },
        ],
        "stream": false,
        "options": options,
    });
    if keep_alive {
        body["keep_alive"] = serde_json::json!(-1);
    }
    let _ = ureq::post(&url).send_json(body);
}

pub fn stream_chat(
    base_url: &str,
    model: String,
    messages: Vec<ChatMessage>,
    num_ctx: Option<u64>,
    gen_params: [f64; 4],
    keep_alive: bool,
    thinking: bool,
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

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "options": options,
        "think": thinking,
    });
    if keep_alive {
        body["keep_alive"] = serde_json::json!(-1);
    }

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

/// Raw-prompt streaming via /api/generate with `raw: true`. Used to continue
/// a mid-assistant turn after a tool call — the caller hand-builds the prompt
/// using the model's template so Ollama never re-applies the chat template
/// (which would close the turn and signal "response complete").
/// Response fields differ from /api/chat: we get `response`/`thinking`
/// instead of `message`. We re-shape each chunk into a StreamChunk so the
/// poll_stream consumer doesn't have to branch.
pub fn stream_generate_raw(
    base_url: &str,
    model: String,
    prompt: String,
    num_ctx: Option<u64>,
    gen_params: [f64; 4],
    keep_alive: bool,
    thinking: bool,
    tx: Sender<StreamChunk>,
) {
    #[derive(Deserialize)]
    struct GenerateChunk {
        response: Option<String>,
        thinking: Option<String>,
        done: bool,
        error: Option<String>,
        prompt_eval_count: Option<u64>,
        eval_count: Option<u64>,
        eval_duration: Option<u64>,
    }

    let url = format!("{}/api/generate", base_url);
    let mut options = serde_json::json!({
        "temperature":    gen_params[0],
        "top_p":          gen_params[1],
        "repeat_penalty": gen_params[2],
    });
    if let Some(ctx) = num_ctx {
        options["num_ctx"] = serde_json::json!(ctx);
    }
    let mut body = serde_json::json!({
        "model":  model,
        "prompt": prompt,
        "raw":    true,
        "stream": true,
        "options": options,
        "think":  thinking,
    });
    if keep_alive {
        body["keep_alive"] = serde_json::json!(-1);
    }

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
        let g: GenerateChunk = match serde_json::from_str(&line) {
            Ok(g) => g,
            Err(_) => continue,
        };
        let chunk = StreamChunk {
            message: Some(ChatMessage {
                role:    "assistant".to_string(),
                content: g.response.unwrap_or_default(),
                thinking: g.thinking,
                images:   None,
            }),
            done:              g.done,
            error:             g.error,
            prompt_eval_count: g.prompt_eval_count,
            eval_count:        g.eval_count,
            eval_duration:     g.eval_duration,
        };
        let done = chunk.done;
        if tx.send(chunk).is_err() { break; }
        if done { break; }
    }
}
