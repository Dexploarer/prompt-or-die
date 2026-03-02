//! LLM provider abstraction for agent decision-making.
//!
//! Provides a unified interface for LLM backends (OpenAI, Anthropic, local models).
//! All providers implement the blocking `LlmProvider` trait, called from
//! the LlmAgent's decision thread.

use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================
// ERROR TYPES
// ============================================================

#[derive(Debug)]
pub enum LlmError {
    Network(String),
    Api { status: u16, message: String },
    Parse(String),
    RateLimited { retry_after_ms: u64 },
    Timeout,
    BudgetExceeded { requested: u32, remaining: u32 },
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::Network(msg) => write!(f, "Network error: {}", msg),
            LlmError::Api { status, message } => write!(f, "API error {}: {}", status, message),
            LlmError::Parse(msg) => write!(f, "Parse error: {}", msg),
            LlmError::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited, retry after {}ms", retry_after_ms)
            }
            LlmError::Timeout => write!(f, "Request timed out"),
            LlmError::BudgetExceeded {
                requested,
                remaining,
            } => write!(
                f,
                "Token budget exceeded: requested {}, remaining {}",
                requested, remaining
            ),
        }
    }
}

impl std::error::Error for LlmError {}

// ============================================================
// REQUEST / RESPONSE TYPES
// ============================================================

#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub usage: TokenUsage,
    pub model: String,
}

// ============================================================
// TOKEN BUDGET (Task 2.5)
// ============================================================

#[derive(Debug, Clone)]
pub struct TokenBudget {
    pub max_tokens_per_tick: u32,
    pub max_tokens_per_minute: u32,
    pub tokens_used_this_tick: u32,
    pub tokens_used_this_minute: u32,
    pub minute_start_tick: u64,
}

impl TokenBudget {
    pub fn new(per_tick: u32, per_minute: u32) -> Self {
        Self {
            max_tokens_per_tick: per_tick,
            max_tokens_per_minute: per_minute,
            tokens_used_this_tick: 0,
            tokens_used_this_minute: 0,
            minute_start_tick: 0,
        }
    }

    pub fn unlimited() -> Self {
        Self::new(u32::MAX, u32::MAX)
    }

    pub fn can_request(&self, estimated_tokens: u32) -> bool {
        self.tokens_used_this_tick + estimated_tokens <= self.max_tokens_per_tick
            && self.tokens_used_this_minute + estimated_tokens <= self.max_tokens_per_minute
    }

    pub fn record_usage(&mut self, usage: &TokenUsage) {
        self.tokens_used_this_tick += usage.total_tokens;
        self.tokens_used_this_minute += usage.total_tokens;
    }

    pub fn reset_tick(&mut self) {
        self.tokens_used_this_tick = 0;
    }

    pub fn reset_minute(&mut self, current_tick: u64) {
        self.tokens_used_this_minute = 0;
        self.minute_start_tick = current_tick;
    }

    pub fn should_reset_minute(&self, current_tick: u64, ticks_per_second: u64) -> bool {
        current_tick - self.minute_start_tick >= ticks_per_second * 60
    }
}

// ============================================================
// PROVIDER TRAIT
// ============================================================

/// Core LLM provider trait. All LLM backends implement this.
///
/// Blocking API — LlmAgent spawns decision threads that call providers synchronously.
pub trait LlmProvider: Send + Sync {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError>;
    fn name(&self) -> &str;

    /// Rough token estimate (~4 chars/token for English)
    fn estimate_tokens(&self, text: &str) -> u32 {
        (text.len() as u32) / 4
    }
}

// ============================================================
// OPENAI-COMPATIBLE PROVIDER
// ============================================================

/// Works with OpenAI, Anthropic, ollama, vLLM, and any OpenAI-compatible API.
pub struct OpenAiProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    base_url: String,
    default_model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, base_url: Option<String>, default_model: Option<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            default_model: default_model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
        }
    }

    pub fn openai(api_key: String) -> Self {
        Self::new(api_key, None, None)
    }

    pub fn anthropic(api_key: String) -> Self {
        Self::new(
            api_key,
            Some("https://api.anthropic.com/v1".to_string()),
            Some("claude-sonnet-4-20250514".to_string()),
        )
    }

    pub fn local(base_url: String) -> Self {
        Self::new(String::new(), Some(base_url), Some("local".to_string()))
    }

    fn complete_anthropic(
        &self,
        request: &CompletionRequest,
        model: &str,
    ) -> Result<CompletionResponse, LlmError> {
        let anthropic_req = AnthropicRequest {
            model: model.to_string(),
            max_tokens: request.max_tokens,
            system: request.system_prompt.clone(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: request.user_prompt.clone(),
            }],
            temperature: request.temperature,
        };

        let url = format!("{}/messages", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_req)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status().as_u16();
        if status == 429 {
            return Err(LlmError::RateLimited {
                retry_after_ms: 1000,
            });
        }
        if status >= 400 {
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Api {
                status,
                message: body,
            });
        }

        let resp: AnthropicResponse = response.json().map_err(|e| LlmError::Parse(e.to_string()))?;

        let content = resp
            .content
            .iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("");

        let usage = resp
            .usage
            .map(|u| {
                let input = u.input_tokens.unwrap_or(0);
                let output = u.output_tokens.unwrap_or(0);
                TokenUsage {
                    prompt_tokens: input,
                    completion_tokens: output,
                    total_tokens: input + output,
                }
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content,
            usage,
            model: resp.model.unwrap_or_else(|| model.to_string()),
        })
    }
}

impl LlmProvider for OpenAiProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        if self.base_url.contains("anthropic.com") {
            return self.complete_anthropic(request, model);
        }

        let chat_req = ChatRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: request.system_prompt.clone(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: request.user_prompt.clone(),
                },
            ],
            temperature: request.temperature,
            max_tokens: request.max_tokens,
        };

        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&chat_req)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::Network(e.to_string())
                }
            })?;

        let status = response.status().as_u16();
        if status == 429 {
            return Err(LlmError::RateLimited {
                retry_after_ms: 1000,
            });
        }
        if status >= 400 {
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Api {
                status,
                message: body,
            });
        }

        let resp: ChatResponse = response.json().map_err(|e| LlmError::Parse(e.to_string()))?;

        let content = resp
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = resp
            .usage
            .map(|u| TokenUsage {
                prompt_tokens: u.prompt_tokens.unwrap_or(0),
                completion_tokens: u.completion_tokens.unwrap_or(0),
                total_tokens: u.total_tokens.unwrap_or(0),
            })
            .unwrap_or_default();

        Ok(CompletionResponse {
            content,
            usage,
            model: resp.model.unwrap_or_else(|| model.to_string()),
        })
    }

    fn name(&self) -> &str {
        "openai-compatible"
    }
}

// ============================================================
// MOCK PROVIDER (for testing)
// ============================================================

pub struct MockProvider {
    responses: std::sync::Mutex<Vec<String>>,
    default_response: String,
}

impl MockProvider {
    pub fn new(default_response: String) -> Self {
        Self {
            responses: std::sync::Mutex::new(Vec::new()),
            default_response,
        }
    }

    pub fn idle() -> Self {
        Self::new(r#"{"actions": ["Idle"], "reasoning": "Observing environment"}"#.to_string())
    }

    pub fn queue_response(&self, response: String) {
        self.responses.lock().unwrap().push(response);
    }
}

impl LlmProvider for MockProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let content = {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                self.default_response.clone()
            } else {
                responses.remove(0)
            }
        };

        let prompt_tokens = self.estimate_tokens(&request.system_prompt)
            + self.estimate_tokens(&request.user_prompt);
        let completion_tokens = self.estimate_tokens(&content);

        Ok(CompletionResponse {
            content,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            model: "mock".to_string(),
        })
    }

    fn name(&self) -> &str {
        "mock"
    }
}

// ============================================================
// SERDE TYPES (OpenAI / Anthropic wire format)
// ============================================================

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ApiUsage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ApiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
    temperature: f32,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    usage: Option<AnthropicUsage>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: Option<String>,
    #[serde(rename = "type")]
    content_type: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_provider_default() {
        let provider = MockProvider::idle();
        let request = CompletionRequest {
            model: "test".to_string(),
            system_prompt: "You are a game agent.".to_string(),
            user_prompt: "What do you do?".to_string(),
            temperature: 0.7,
            max_tokens: 256,
        };
        let response = provider.complete(&request).unwrap();
        assert!(response.content.contains("Idle"));
        assert!(response.usage.total_tokens > 0);
    }

    #[test]
    fn test_mock_provider_queued() {
        let provider = MockProvider::idle();
        provider.queue_response(
            r#"{"actions": ["Attack"], "reasoning": "Enemy nearby"}"#.to_string(),
        );

        let request = CompletionRequest {
            model: "test".to_string(),
            system_prompt: String::new(),
            user_prompt: String::new(),
            temperature: 0.7,
            max_tokens: 256,
        };

        let r1 = provider.complete(&request).unwrap();
        assert!(r1.content.contains("Attack"));

        let r2 = provider.complete(&request).unwrap();
        assert!(r2.content.contains("Idle"));
    }

    #[test]
    fn test_token_budget() {
        let mut budget = TokenBudget::new(1000, 10000);
        assert!(budget.can_request(500));

        budget.record_usage(&TokenUsage {
            prompt_tokens: 300,
            completion_tokens: 200,
            total_tokens: 500,
        });

        assert!(budget.can_request(500));
        assert!(!budget.can_request(501));

        budget.reset_tick();
        assert!(budget.can_request(1000));
        assert_eq!(budget.tokens_used_this_minute, 500);
    }

    #[test]
    fn test_token_estimation() {
        let provider = MockProvider::idle();
        let estimate = provider.estimate_tokens("Hello, world! This is a test.");
        assert!(estimate > 0);
        assert!(estimate < 20);
    }

    #[test]
    fn test_llm_error_display() {
        let err = LlmError::Timeout;
        assert_eq!(format!("{}", err), "Request timed out");

        let err = LlmError::RateLimited {
            retry_after_ms: 5000,
        };
        assert!(format!("{}", err).contains("5000"));
    }
}
