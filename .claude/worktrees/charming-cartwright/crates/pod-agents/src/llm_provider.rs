//! LLM provider abstraction layer — OpenAI-compatible interface for chat completion APIs.
//!
//! Provides a trait-based abstraction over LLM providers so that [`LlmAgent`](super::LlmAgent)
//! can swap backends (OpenAI, Anthropic, local models) without changing agent logic.
//!
//! Included implementations:
//! - [`MockLlmProvider`] — deterministic, configurable responses for testing
//! - [`HttpLlmProvider`] — OpenAI-compatible HTTP provider (requires `llm-http` feature)

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur when calling an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LlmError {
    /// Network-level failure (DNS, TCP, TLS, timeout).
    NetworkError(String),
    /// The provider returned HTTP 429 — caller should back off.
    RateLimited {
        /// Suggested retry delay in milliseconds (0 if unknown).
        retry_after_ms: u64,
    },
    /// The request would exceed the model's context window.
    TokenLimitExceeded,
    /// The response could not be parsed (malformed JSON, missing fields).
    InvalidResponse(String),
    /// API key missing or rejected (HTTP 401/403).
    AuthenticationError,
    /// Catch-all for provider-specific errors.
    Custom(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmError::NetworkError(msg) => write!(f, "network error: {}", msg),
            LlmError::RateLimited { retry_after_ms } => {
                write!(f, "rate limited (retry after {}ms)", retry_after_ms)
            }
            LlmError::TokenLimitExceeded => write!(f, "token limit exceeded"),
            LlmError::InvalidResponse(msg) => write!(f, "invalid response: {}", msg),
            LlmError::AuthenticationError => write!(f, "authentication error"),
            LlmError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for LlmError {}

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

/// Role within a chat conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Request / Response
// ---------------------------------------------------------------------------

/// Request payload for a chat completion call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// Conversation messages (system, user, assistant turns).
    pub messages: Vec<ChatMessage>,
    /// Sampling temperature (0.0 = deterministic, 1.0 = creative). `None` uses model default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens to generate. `None` uses model default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Request JSON-mode output (provider must support it).
    #[serde(default)]
    pub json_mode: bool,
    /// Stop sequences — generation halts when any of these strings is produced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
}

impl Default for ChatCompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            json_mode: false,
            stop: Vec::new(),
        }
    }
}

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    FunctionCall,
}

/// Token usage breakdown for a single request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl TokenUsage {
    pub fn total(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// Response payload from a chat completion call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    /// The generated text content.
    pub content: String,
    /// Why generation stopped.
    pub finish_reason: FinishReason,
    /// Token usage statistics.
    pub usage: TokenUsage,
}

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// Trait for LLM chat completion providers.
///
/// All methods are async and implementations must be `Send + Sync` so they can
/// be shared across the tick pipeline and background request threads.
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    /// Execute a chat completion request and return the response.
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LlmError>;

    /// The underlying model identifier (e.g. `"gpt-4o"`, `"claude-3-5-sonnet"`).
    fn model_name(&self) -> &str;

    /// Maximum context length in tokens for this model.
    fn max_context_tokens(&self) -> usize;

    /// Whether the provider supports JSON-mode output.
    fn supports_json_mode(&self) -> bool {
        false
    }

    /// Whether the provider supports function/tool calling.
    fn supports_function_calling(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// MockLlmProvider
// ---------------------------------------------------------------------------

/// A deterministic LLM provider for testing.
///
/// Returns pre-configured responses in sequence (cycling when exhausted).
/// Tracks call count and stores the last request for test assertions.
pub struct MockLlmProvider {
    model: String,
    responses: Vec<String>,
    call_count: AtomicUsize,
    last_request: Mutex<Option<ChatCompletionRequest>>,
}

impl fmt::Debug for MockLlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MockLlmProvider")
            .field("model", &self.model)
            .field("responses", &self.responses)
            .field("call_count", &self.call_count.load(Ordering::Relaxed))
            .finish()
    }
}

impl MockLlmProvider {
    /// Create a mock that always returns the same response.
    pub fn new(default_response: impl Into<String>) -> Self {
        Self {
            model: "mock-model".to_string(),
            responses: vec![default_response.into()],
            call_count: AtomicUsize::new(0),
            last_request: Mutex::new(None),
        }
    }

    /// Create a mock that cycles through the given responses in order.
    pub fn with_responses(responses: Vec<String>) -> Self {
        assert!(!responses.is_empty(), "MockLlmProvider requires at least one response");
        Self {
            model: "mock-model".to_string(),
            responses,
            call_count: AtomicUsize::new(0),
            last_request: Mutex::new(None),
        }
    }

    /// Number of times `chat_completion` has been called.
    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::Relaxed)
    }

    /// Clone of the most recent request, if any.
    pub fn last_request(&self) -> Option<ChatCompletionRequest> {
        self.last_request.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockLlmProvider {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let idx = self.call_count.fetch_add(1, Ordering::Relaxed);
        *self.last_request.lock().unwrap() = Some(request);

        let response_text = &self.responses[idx % self.responses.len()];

        Ok(ChatCompletionResponse {
            content: response_text.clone(),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 50,
                completion_tokens: 20,
            },
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn max_context_tokens(&self) -> usize {
        4096
    }

    fn supports_json_mode(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// HttpLlmProvider (native only, behind feature flag)
// ---------------------------------------------------------------------------

/// OpenAI-compatible HTTP provider.
///
/// Targets the `/v1/chat/completions` endpoint. Works with OpenAI, Azure OpenAI,
/// Ollama, vLLM, and any other server that implements the OpenAI chat API.
///
/// Requires the `llm-http` feature flag and is only available on native targets
/// (not wasm32).
#[cfg(not(target_arch = "wasm32"))]
pub struct HttpLlmProvider {
    base_url: String,
    api_key: String,
    model: String,
    max_context: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl fmt::Debug for HttpLlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpLlmProvider")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("max_context", &self.max_context)
            // Intentionally omit api_key
            .finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HttpLlmProvider {
    /// Create a new HTTP provider targeting an OpenAI-compatible endpoint.
    ///
    /// * `base_url` — e.g. `"https://api.openai.com"` (no trailing slash)
    /// * `api_key`  — Bearer token sent in the `Authorization` header
    /// * `model`    — model identifier, e.g. `"gpt-4o"`
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            max_context: 128_000,
        }
    }

    /// Override the default max context token count.
    pub fn with_max_context(mut self, tokens: usize) -> Self {
        self.max_context = tokens;
        self
    }

    /// Build the OpenAI-format JSON body for a chat completion request.
    fn build_request_body(&self, request: &ChatCompletionRequest) -> serde_json::Value {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "role": match msg.role {
                        ChatRole::System => "system",
                        ChatRole::User => "user",
                        ChatRole::Assistant => "assistant",
                    },
                    "content": msg.content,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(max_tok) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tok);
        }
        if request.json_mode {
            body["response_format"] = serde_json::json!({"type": "json_object"});
        }
        if !request.stop.is_empty() {
            body["stop"] = serde_json::json!(request.stop);
        }

        body
    }

    /// Parse an OpenAI-format chat completion response JSON.
    fn parse_response_body(
        body: &serde_json::Value,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let choice = body["choices"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| LlmError::InvalidResponse("no choices in response".into()))?;

        let content = choice["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let finish_reason = match choice["finish_reason"].as_str().unwrap_or("stop") {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "content_filter" => FinishReason::ContentFilter,
            "function_call" | "tool_calls" => FinishReason::FunctionCall,
            _ => FinishReason::Stop,
        };

        let usage = TokenUsage {
            prompt_tokens: body["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: body["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        };

        Ok(ChatCompletionResponse {
            content,
            finish_reason,
            usage,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait::async_trait]
impl LlmProvider for HttpLlmProvider {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let _url = format!("{}/v1/chat/completions", self.base_url);
        let _body = self.build_request_body(&request);
        let _auth = format!("Bearer {}", self.api_key);

        // TODO: Replace with actual reqwest HTTP call once the `llm-http` feature
        // and `reqwest` dependency are wired up. The request body and response
        // parsing are fully implemented — only the transport is stubbed.
        //
        // Example (uncomment when reqwest is available):
        // ```
        // let client = reqwest::Client::new();
        // let res = client
        //     .post(&_url)
        //     .header("Authorization", &_auth)
        //     .header("Content-Type", "application/json")
        //     .json(&_body)
        //     .send()
        //     .await
        //     .map_err(|e| LlmError::NetworkError(e.to_string()))?;
        //
        // let status = res.status();
        // let text = res.text().await.map_err(|e| LlmError::NetworkError(e.to_string()))?;
        //
        // if status == 401 || status == 403 {
        //     return Err(LlmError::AuthenticationError);
        // }
        // if status == 429 {
        //     return Err(LlmError::RateLimited { retry_after_ms: 1000 });
        // }
        // if !status.is_success() {
        //     return Err(LlmError::NetworkError(format!("HTTP {}: {}", status, text)));
        // }
        //
        // let json: serde_json::Value = serde_json::from_str(&text)
        //     .map_err(|e| LlmError::InvalidResponse(e.to_string()))?;
        // Self::parse_response_body(&json)
        // ```

        // Stub: return mock response so the provider is usable for integration tests
        log::warn!(
            "HttpLlmProvider: HTTP transport not yet implemented, returning stub response"
        );

        Ok(ChatCompletionResponse {
            content: r#"{"actions": ["idle"], "reasoning": "stub response from HttpLlmProvider"}"#
                .to_string(),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            },
        })
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn max_context_tokens(&self) -> usize {
        self.max_context
    }

    fn supports_json_mode(&self) -> bool {
        true
    }

    fn supports_function_calling(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_default_response() {
        let provider = MockLlmProvider::new("hello world");
        let req = ChatCompletionRequest::default();
        let resp = provider.chat_completion(req).await.unwrap();

        assert_eq!(resp.content, "hello world");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(provider.call_count(), 1);
    }

    #[tokio::test]
    async fn mock_cycles_through_responses() {
        let provider = MockLlmProvider::with_responses(vec![
            "first".to_string(),
            "second".to_string(),
        ]);
        let req = ChatCompletionRequest::default();

        let r1 = provider.chat_completion(req.clone()).await.unwrap();
        assert_eq!(r1.content, "first");

        let r2 = provider.chat_completion(req.clone()).await.unwrap();
        assert_eq!(r2.content, "second");

        // Cycles back to first
        let r3 = provider.chat_completion(req).await.unwrap();
        assert_eq!(r3.content, "first");

        assert_eq!(provider.call_count(), 3);
    }

    #[tokio::test]
    async fn mock_tracks_last_request() {
        let provider = MockLlmProvider::new("ok");
        let req = ChatCompletionRequest {
            messages: vec![ChatMessage::user("test prompt")],
            temperature: Some(0.5),
            max_tokens: Some(100),
            json_mode: true,
            stop: vec!["STOP".to_string()],
        };

        provider.chat_completion(req).await.unwrap();

        let last = provider.last_request().expect("should have recorded request");
        assert_eq!(last.messages.len(), 1);
        assert_eq!(last.messages[0].content, "test prompt");
        assert_eq!(last.temperature, Some(0.5));
        assert_eq!(last.max_tokens, Some(100));
        assert!(last.json_mode);
    }

    #[test]
    fn chat_message_constructors() {
        let sys = ChatMessage::system("you are helpful");
        assert_eq!(sys.role, ChatRole::System);
        assert_eq!(sys.content, "you are helpful");

        let usr = ChatMessage::user("hello");
        assert_eq!(usr.role, ChatRole::User);

        let asst = ChatMessage::assistant("hi there");
        assert_eq!(asst.role, ChatRole::Assistant);
    }

    #[test]
    fn token_usage_total() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
        };
        assert_eq!(usage.total(), 150);
    }

    #[test]
    fn llm_error_display() {
        assert_eq!(
            LlmError::RateLimited { retry_after_ms: 500 }.to_string(),
            "rate limited (retry after 500ms)"
        );
        assert_eq!(
            LlmError::AuthenticationError.to_string(),
            "authentication error"
        );
        assert_eq!(
            LlmError::TokenLimitExceeded.to_string(),
            "token limit exceeded"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod http_tests {
        use super::super::*;

        #[test]
        fn http_provider_builds_request_body() {
            let provider = HttpLlmProvider::new(
                "https://api.openai.com",
                "sk-test",
                "gpt-4o",
            );

            let req = ChatCompletionRequest {
                messages: vec![
                    ChatMessage::system("you are a game agent"),
                    ChatMessage::user("what do you see?"),
                ],
                temperature: Some(0.7),
                max_tokens: Some(500),
                json_mode: true,
                stop: vec!["\n\n".to_string()],
            };

            let body = provider.build_request_body(&req);

            assert_eq!(body["model"], "gpt-4o");
            assert_eq!(body["messages"].as_array().unwrap().len(), 2);
            assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 0.001);
            assert_eq!(body["max_tokens"], 500);
            assert_eq!(body["response_format"]["type"], "json_object");
            assert_eq!(body["stop"][0], "\n\n");
        }

        #[test]
        fn http_provider_parses_response_body() {
            let json = serde_json::json!({
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "Hello!"
                    },
                    "finish_reason": "stop"
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5,
                    "total_tokens": 15
                }
            });

            let resp = HttpLlmProvider::parse_response_body(&json).unwrap();
            assert_eq!(resp.content, "Hello!");
            assert_eq!(resp.finish_reason, FinishReason::Stop);
            assert_eq!(resp.usage.prompt_tokens, 10);
            assert_eq!(resp.usage.completion_tokens, 5);
        }

        #[test]
        fn http_provider_metadata() {
            let provider = HttpLlmProvider::new("http://localhost:8080", "key", "llama3")
                .with_max_context(8192);

            assert_eq!(provider.model_name(), "llama3");
            assert_eq!(provider.max_context_tokens(), 8192);
            assert!(provider.supports_json_mode());
            assert!(provider.supports_function_calling());
        }

        #[tokio::test]
        async fn http_provider_stub_returns_response() {
            let provider = HttpLlmProvider::new(
                "https://api.openai.com",
                "sk-test",
                "gpt-4o",
            );
            let req = ChatCompletionRequest::default();
            let resp = provider.chat_completion(req).await.unwrap();

            // Stub returns a valid response
            assert!(!resp.content.is_empty());
            assert_eq!(resp.finish_reason, FinishReason::Stop);
        }
    }
}
