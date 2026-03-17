use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub model: String,
    pub prompt: String,
    pub app_title: String,
    pub http_referer: Option<String>,
}

pub struct OpenRouterClient {
    client: Client,
}

impl OpenRouterClient {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .context("failed to construct OpenRouter client")?;

        Ok(Self { client })
    }

    pub fn transcribe_file(
        &self,
        config: &OpenRouterConfig,
        audio_path: &Path,
        format: &str,
    ) -> Result<String> {
        let audio_bytes = fs::read(audio_path)
            .with_context(|| format!("failed to read {}", audio_path.display()))?;

        let payload = json!({
            "model": config.model,
            "temperature": 0,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": config.prompt
                        },
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": BASE64_STANDARD.encode(audio_bytes),
                                "format": format
                            }
                        }
                    ]
                }
            ]
        });

        let mut request = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .header("X-Title", &config.app_title);

        if let Some(http_referer) = &config.http_referer {
            request = request.header("HTTP-Referer", http_referer);
        }

        let response = request
            .json(&payload)
            .send()
            .context("failed to call OpenRouter")?;

        let status = response.status();
        let body = response
            .text()
            .context("failed to read OpenRouter response body")?;

        if !status.is_success() {
            if let Ok(error_response) = serde_json::from_str::<ErrorEnvelope>(&body) {
                bail!(
                    "OpenRouter error {}: {}",
                    status,
                    error_response.error.message
                );
            }

            bail!("OpenRouter error {}: {}", status, body.trim());
        }

        let parsed: ChatCompletionResponse =
            serde_json::from_str(&body).context("failed to parse OpenRouter response JSON")?;

        parsed
            .extract_text()
            .context("OpenRouter returned a response without transcript text")
    }
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
struct ErrorBody {
    message: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

impl ChatCompletionResponse {
    fn extract_text(&self) -> Option<String> {
        let choice = self.choices.first()?;
        let content = choice.message.content.as_ref()?;

        match content {
            MessageContent::Text(text) => normalize_text(text),
            MessageContent::Parts(parts) => {
                let joined = parts
                    .iter()
                    .filter_map(|part| part.text.as_deref())
                    .collect::<Vec<_>>()
                    .join("");

                normalize_text(&joined)
            }
        }
    }
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: Option<MessageContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Parts(Vec<MessagePart>),
}

#[derive(Deserialize)]
struct MessagePart {
    #[serde(default)]
    text: Option<String>,
}

fn normalize_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::ChatCompletionResponse;

    #[test]
    fn extracts_string_content() {
        let response: ChatCompletionResponse = serde_json::from_str(
            r#"{
                "choices": [
                    {
                        "message": {
                            "content": "hello world"
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(response.extract_text().as_deref(), Some("hello world"));
    }

    #[test]
    fn extracts_part_content() {
        let response: ChatCompletionResponse = serde_json::from_str(
            r#"{
                "choices": [
                    {
                        "message": {
                            "content": [
                                { "type": "text", "text": "hello " },
                                { "type": "text", "text": "world" }
                            ]
                        }
                    }
                ]
            }"#,
        )
        .unwrap();

        assert_eq!(response.extract_text().as_deref(), Some("hello world"));
    }
}
