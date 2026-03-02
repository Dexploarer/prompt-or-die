//! Sliding window conversation memory for LLM agents.
//!
//! Provides bounded memory storage so that LLM agents can maintain context across
//! multiple decision cycles without exceeding token budgets. Two strategies are
//! available:
//!
//! - **`ConversationMemory`** — simple sliding window that evicts the oldest
//!   non-system messages when limits are reached.
//! - **`SummarizingMemory`** — wraps `ConversationMemory` and compresses evicted
//!   messages into a compact summary message rather than discarding them entirely.
//!
//! Both strategies guarantee that system messages are never evicted (unless
//! `preserve_system` is explicitly disabled).

use crate::llm_provider::{ChatMessage, ChatRole};

// ---------------------------------------------------------------------------
// Token estimation
// ---------------------------------------------------------------------------

/// Estimate the token count for a piece of text.
///
/// Uses the common heuristic of ~4 characters per token (which tracks closely
/// with tiktoken for English text). Returns a minimum of 1 to avoid zero-cost
/// messages.
pub fn estimate_tokens(text: &str) -> usize {
    let estimate = text.len() / 4;
    estimate.max(1)
}

/// Estimate token count for a single `ChatMessage` including role overhead.
fn message_tokens(msg: &ChatMessage) -> usize {
    // Account for the role token and message framing (~4 tokens overhead).
    estimate_tokens(&msg.content) + 4
}

// ---------------------------------------------------------------------------
// MemoryConfig builder
// ---------------------------------------------------------------------------

/// Builder for configuring a [`ConversationMemory`].
///
/// # Example
/// ```
/// use pod_agents::conversation_memory::MemoryConfig;
///
/// let memory = MemoryConfig::new()
///     .max_messages(30)
///     .max_tokens(8192)
///     .preserve_system(true)
///     .summarize_on_evict(false)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    max_messages: usize,
    max_tokens: usize,
    preserve_system: bool,
    summarize_on_evict: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_messages: 20,
            max_tokens: 4096,
            preserve_system: true,
            summarize_on_evict: false,
        }
    }
}

impl MemoryConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of messages to retain.
    pub fn max_messages(mut self, n: usize) -> Self {
        self.max_messages = n;
        self
    }

    /// Set the maximum estimated token budget.
    pub fn max_tokens(mut self, n: usize) -> Self {
        self.max_tokens = n;
        self
    }

    /// Whether to preserve system messages during eviction (default: `true`).
    pub fn preserve_system(mut self, yes: bool) -> Self {
        self.preserve_system = yes;
        self
    }

    /// Whether to summarize evicted messages instead of dropping them (default: `false`).
    pub fn summarize_on_evict(mut self, yes: bool) -> Self {
        self.summarize_on_evict = yes;
        self
    }

    /// Build the configured [`ConversationMemory`].
    pub fn build(self) -> ConversationMemory {
        ConversationMemory {
            messages: Vec::new(),
            max_messages: self.max_messages,
            max_tokens: self.max_tokens,
            preserve_system: self.preserve_system,
            summarize_on_evict: self.summarize_on_evict,
        }
    }
}

// ---------------------------------------------------------------------------
// ConversationMemory
// ---------------------------------------------------------------------------

/// Sliding-window conversation memory that keeps the most recent messages within
/// configurable message-count and token-budget limits.
///
/// System messages (role == `System`) are never evicted when `preserve_system` is
/// enabled (default). This guarantees that the agent's system prompt survives
/// even aggressive trimming.
#[derive(Debug, Clone)]
pub struct ConversationMemory {
    messages: Vec<ChatMessage>,
    max_messages: usize,
    max_tokens: usize,
    preserve_system: bool,
    summarize_on_evict: bool,
}

impl Default for ConversationMemory {
    fn default() -> Self {
        MemoryConfig::default().build()
    }
}

impl ConversationMemory {
    /// Create a memory with default settings (20 messages, 4096 tokens).
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a message into memory.
    ///
    /// If the memory exceeds `max_messages` or `max_tokens` after insertion,
    /// the oldest non-system messages are evicted (optionally summarized first).
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
        self.enforce_limits();
    }

    /// Get all stored messages.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Get the `n` most recent messages. Returns fewer if memory has less than `n`.
    pub fn recent(&self, n: usize) -> &[ChatMessage] {
        let len = self.messages.len();
        if n >= len {
            &self.messages
        } else {
            &self.messages[len - n..]
        }
    }

    /// Clear all stored messages.
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Estimate the total token count across all stored messages.
    pub fn estimated_tokens(&self) -> usize {
        self.messages.iter().map(message_tokens).sum()
    }

    /// Remove oldest non-system messages until total estimated tokens is within
    /// `token_budget`. System messages are preserved if `preserve_system` is set.
    pub fn trim_to_budget(&mut self, token_budget: usize) {
        while self.estimated_tokens() > token_budget {
            let evict_idx = self.find_oldest_evictable();
            match evict_idx {
                Some(idx) => {
                    self.messages.remove(idx);
                }
                None => break, // nothing left to evict
            }
        }
    }

    /// Current number of stored messages.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether memory is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    // -- internal helpers ---------------------------------------------------

    /// Enforce both message-count and token-budget limits after a push.
    fn enforce_limits(&mut self) {
        // If summarize_on_evict, collect messages that would be evicted and
        // replace them with a summary before dropping.
        if self.summarize_on_evict {
            self.enforce_with_summary();
        } else {
            self.enforce_simple();
        }
    }

    /// Simple eviction: drop oldest non-system messages.
    fn enforce_simple(&mut self) {
        // Message count limit
        while self.messages.len() > self.max_messages {
            match self.find_oldest_evictable() {
                Some(idx) => {
                    self.messages.remove(idx);
                }
                None => break,
            }
        }
        // Token limit
        self.trim_to_budget(self.max_tokens);
    }

    /// Summarizing eviction: compress oldest non-system messages into a summary
    /// before removing them.
    fn enforce_with_summary(&mut self) {
        // Count how many messages need eviction for the message-count limit.
        // Since the summary itself adds 1 message back, we must evict at least
        // `count_over + 1` messages to achieve a net reduction of `count_over`.
        let count_over = self.messages.len().saturating_sub(self.max_messages);
        if count_over > 0 {
            let evict_count = count_over + 1;
            // Find the oldest N evictable messages and summarize them.
            let evictable_indices = self.find_n_oldest_evictable(evict_count);
            if !evictable_indices.is_empty() {
                let summary = Self::build_summary(
                    evictable_indices.iter().map(|&i| &self.messages[i]),
                );
                // Remove in reverse order to preserve indices.
                for &idx in evictable_indices.iter().rev() {
                    self.messages.remove(idx);
                }
                // Insert the summary after the last system message (or at the front).
                let insert_pos = self.last_system_index().map(|i| i + 1).unwrap_or(0);
                self.messages.insert(insert_pos, summary);
            }
        }

        // Token limit — simple eviction after summarization pass.
        while self.estimated_tokens() > self.max_tokens {
            match self.find_oldest_evictable() {
                Some(idx) => {
                    self.messages.remove(idx);
                }
                None => break,
            }
        }
    }

    /// Find the index of the oldest evictable (non-system when preserve_system)
    /// message, or `None` if nothing can be evicted.
    fn find_oldest_evictable(&self) -> Option<usize> {
        if self.preserve_system {
            self.messages
                .iter()
                .position(|m| m.role != ChatRole::System)
        } else {
            if self.messages.is_empty() {
                None
            } else {
                Some(0)
            }
        }
    }

    /// Find the indices of the `n` oldest evictable messages.
    fn find_n_oldest_evictable(&self, n: usize) -> Vec<usize> {
        let mut result = Vec::with_capacity(n);
        for (i, msg) in self.messages.iter().enumerate() {
            if result.len() >= n {
                break;
            }
            if self.preserve_system && msg.role == ChatRole::System {
                continue;
            }
            result.push(i);
        }
        result
    }

    /// Index of the last system message, if any.
    fn last_system_index(&self) -> Option<usize> {
        self.messages
            .iter()
            .rposition(|m| m.role == ChatRole::System)
    }

    /// Build a summary `ChatMessage` from an iterator of messages.
    fn build_summary<'a>(messages: impl Iterator<Item = &'a ChatMessage>) -> ChatMessage {
        let parts: Vec<String> = messages
            .map(|m| {
                let prefix = match m.role {
                    ChatRole::User => "observation",
                    ChatRole::Assistant => "action",
                    ChatRole::System => "system",
                };
                // Truncate long content in summaries.
                let content = if m.content.len() > 80 {
                    format!("{}...", &m.content[..77])
                } else {
                    m.content.clone()
                };
                format!("[{}] {}", prefix, content)
            })
            .collect();

        ChatMessage::user(format!("Previous context: {}", parts.join(" → ")))
    }
}

// ---------------------------------------------------------------------------
// SummarizingMemory
// ---------------------------------------------------------------------------

/// A memory wrapper that compresses old messages into summaries on demand.
///
/// This is a convenience type around [`ConversationMemory`] that provides
/// explicit summarization methods. For automatic summarization on eviction,
/// use [`MemoryConfig::summarize_on_evict(true)`] instead.
#[derive(Debug, Clone)]
pub struct SummarizingMemory {
    inner: ConversationMemory,
}

impl SummarizingMemory {
    /// Create a summarizing memory with default config.
    pub fn new() -> Self {
        Self {
            inner: MemoryConfig::new().summarize_on_evict(true).build(),
        }
    }

    /// Create from a pre-configured `ConversationMemory`.
    pub fn from_memory(memory: ConversationMemory) -> Self {
        Self { inner: memory }
    }

    /// Push a message.
    pub fn push(&mut self, message: ChatMessage) {
        self.inner.push(message);
    }

    /// Get all stored messages.
    pub fn messages(&self) -> &[ChatMessage] {
        self.inner.messages()
    }

    /// Get N most recent messages.
    pub fn recent(&self, n: usize) -> &[ChatMessage] {
        self.inner.recent(n)
    }

    /// Clear all history.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Estimated total tokens.
    pub fn estimated_tokens(&self) -> usize {
        self.inner.estimated_tokens()
    }

    /// Trim to token budget.
    pub fn trim_to_budget(&mut self, token_budget: usize) {
        self.inner.trim_to_budget(token_budget);
    }

    /// Summarize the `count` oldest non-system messages into a single message
    /// and replace them in memory.
    ///
    /// Returns the generated summary message.
    pub fn summarize_oldest(&mut self, count: usize) -> ChatMessage {
        let evictable = self.inner.find_n_oldest_evictable(count);
        if evictable.is_empty() {
            return ChatMessage::user("Previous context: (none)".to_string());
        }

        let summary = ConversationMemory::build_summary(
            evictable.iter().map(|&i| &self.inner.messages[i]),
        );

        // Remove in reverse to keep indices valid.
        for &idx in evictable.iter().rev() {
            self.inner.messages.remove(idx);
        }

        // Insert after last system message.
        let insert_pos = self
            .inner
            .last_system_index()
            .map(|i| i + 1)
            .unwrap_or(0);
        self.inner.messages.insert(insert_pos, summary.clone());

        summary
    }

    /// Current message count.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether memory is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for SummarizingMemory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::{ChatMessage, ChatRole};

    // -- Token estimation ---------------------------------------------------

    #[test]
    fn token_estimation_accuracy() {
        // "hello world" = 11 chars → 11/4 = 2
        assert_eq!(estimate_tokens("hello world"), 2);
        // Empty string → min 1
        assert_eq!(estimate_tokens(""), 1);
        // 4 chars → 1 token
        assert_eq!(estimate_tokens("abcd"), 1);
        // 100 chars → 25 tokens
        let long = "a".repeat(100);
        assert_eq!(estimate_tokens(&long), 25);
    }

    // -- Push and retrieve --------------------------------------------------

    #[test]
    fn push_and_retrieve_messages() {
        let mut mem = ConversationMemory::new();
        mem.push(ChatMessage::user("hello"));
        mem.push(ChatMessage::assistant("hi there"));

        assert_eq!(mem.messages().len(), 2);
        assert_eq!(mem.messages()[0].role, ChatRole::User);
        assert_eq!(mem.messages()[0].content, "hello");
        assert_eq!(mem.messages()[1].role, ChatRole::Assistant);
        assert_eq!(mem.messages()[1].content, "hi there");
    }

    // -- Sliding window eviction --------------------------------------------

    #[test]
    fn sliding_window_eviction() {
        let mut mem = MemoryConfig::new()
            .max_messages(3)
            .max_tokens(100_000) // large so only message count triggers
            .build();

        mem.push(ChatMessage::user("msg1"));
        mem.push(ChatMessage::user("msg2"));
        mem.push(ChatMessage::user("msg3"));
        assert_eq!(mem.len(), 3);

        // Adding a 4th should evict the oldest.
        mem.push(ChatMessage::user("msg4"));
        assert_eq!(mem.len(), 3);
        assert_eq!(mem.messages()[0].content, "msg2");
        assert_eq!(mem.messages()[2].content, "msg4");
    }

    // -- System message preservation ----------------------------------------

    #[test]
    fn system_messages_never_evicted() {
        let mut mem = MemoryConfig::new()
            .max_messages(3)
            .max_tokens(100_000)
            .preserve_system(true)
            .build();

        mem.push(ChatMessage::system("you are an agent"));
        mem.push(ChatMessage::user("obs1"));
        mem.push(ChatMessage::user("obs2"));
        // At capacity. Next push should evict oldest non-system (obs1).
        mem.push(ChatMessage::user("obs3"));

        assert_eq!(mem.len(), 3);
        // System message must survive.
        assert_eq!(mem.messages()[0].role, ChatRole::System);
        assert_eq!(mem.messages()[0].content, "you are an agent");
        // Oldest user message was evicted.
        assert_eq!(mem.messages()[1].content, "obs2");
        assert_eq!(mem.messages()[2].content, "obs3");
    }

    #[test]
    fn system_messages_evicted_when_preserve_disabled() {
        let mut mem = MemoryConfig::new()
            .max_messages(2)
            .max_tokens(100_000)
            .preserve_system(false)
            .build();

        mem.push(ChatMessage::system("system"));
        mem.push(ChatMessage::user("user1"));
        mem.push(ChatMessage::user("user2"));

        assert_eq!(mem.len(), 2);
        // System message should have been evicted (it was oldest).
        assert_eq!(mem.messages()[0].content, "user1");
    }

    // -- Token budget trimming ----------------------------------------------

    #[test]
    fn token_budget_trimming() {
        let mut mem = MemoryConfig::new()
            .max_messages(100)
            .max_tokens(100_000) // start large
            .build();

        // Each message: ~4 overhead + content/4 tokens.
        // "a".repeat(40) = 10 content tokens + 4 overhead = 14 per message.
        for i in 0..10 {
            mem.push(ChatMessage::user(format!("{}{}", "a".repeat(40), i)));
        }

        let before = mem.estimated_tokens();
        assert!(before > 100);

        // Trim to a very tight budget.
        mem.trim_to_budget(50);

        assert!(mem.estimated_tokens() <= 50);
        assert!(mem.len() < 10);
        // Most recent messages should survive.
        let last = mem.messages().last().unwrap();
        assert!(last.content.ends_with('9'));
    }

    // -- Recent messages ----------------------------------------------------

    #[test]
    fn recent_messages() {
        let mut mem = ConversationMemory::new();
        mem.push(ChatMessage::user("a"));
        mem.push(ChatMessage::user("b"));
        mem.push(ChatMessage::user("c"));
        mem.push(ChatMessage::user("d"));

        let recent2 = mem.recent(2);
        assert_eq!(recent2.len(), 2);
        assert_eq!(recent2[0].content, "c");
        assert_eq!(recent2[1].content, "d");

        // Requesting more than available returns all.
        let recent10 = mem.recent(10);
        assert_eq!(recent10.len(), 4);
    }

    // -- Clear --------------------------------------------------------------

    #[test]
    fn clear_removes_all_messages() {
        let mut mem = ConversationMemory::new();
        mem.push(ChatMessage::system("sys"));
        mem.push(ChatMessage::user("usr"));
        assert_eq!(mem.len(), 2);

        mem.clear();
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
        assert_eq!(mem.estimated_tokens(), 0);
    }

    // -- SummarizingMemory --------------------------------------------------

    #[test]
    fn summarizing_memory_creates_summaries() {
        let mut mem = SummarizingMemory::new();
        mem.push(ChatMessage::system("you are an agent"));
        mem.push(ChatMessage::user("I see a wall"));
        mem.push(ChatMessage::assistant("move north"));
        mem.push(ChatMessage::user("I see a door"));
        mem.push(ChatMessage::assistant("open door"));

        // Summarize the 2 oldest non-system messages.
        let summary = mem.summarize_oldest(2);

        assert!(summary.content.starts_with("Previous context:"));
        assert!(summary.content.contains("[observation] I see a wall"));
        assert!(summary.content.contains("[action] move north"));

        // System message still present.
        assert_eq!(mem.messages()[0].role, ChatRole::System);
        // Summary inserted after system.
        assert!(mem.messages()[1].content.starts_with("Previous context:"));
        // Remaining originals follow.
        assert_eq!(mem.messages()[2].content, "I see a door");
    }

    #[test]
    fn summarizing_memory_on_eviction() {
        let mut mem = MemoryConfig::new()
            .max_messages(4)
            .max_tokens(100_000)
            .summarize_on_evict(true)
            .build();

        mem.push(ChatMessage::system("sys"));
        mem.push(ChatMessage::user("obs1"));
        mem.push(ChatMessage::assistant("act1"));
        mem.push(ChatMessage::user("obs2"));
        // At limit (4). Next push triggers summarization of oldest evictable.
        mem.push(ChatMessage::assistant("act2"));

        // Should be within limits and have a summary somewhere.
        assert!(mem.len() <= 4);
        // System message preserved.
        assert_eq!(mem.messages()[0].role, ChatRole::System);
    }

    // -- MemoryConfig builder -----------------------------------------------

    #[test]
    fn memory_config_builder() {
        let mem = MemoryConfig::new()
            .max_messages(10)
            .max_tokens(2048)
            .preserve_system(false)
            .summarize_on_evict(true)
            .build();

        assert_eq!(mem.max_messages, 10);
        assert_eq!(mem.max_tokens, 2048);
        assert!(!mem.preserve_system);
        assert!(mem.summarize_on_evict);
        assert!(mem.is_empty());
    }

    // -- Empty memory -------------------------------------------------------

    #[test]
    fn empty_memory_handling() {
        let mem = ConversationMemory::new();

        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
        assert_eq!(mem.estimated_tokens(), 0);
        assert_eq!(mem.messages().len(), 0);
        assert_eq!(mem.recent(5).len(), 0);
    }

    // -- Estimated tokens ---------------------------------------------------

    #[test]
    fn estimated_tokens_tracks_content() {
        let mut mem = ConversationMemory::new();
        let initial = mem.estimated_tokens();
        assert_eq!(initial, 0);

        mem.push(ChatMessage::user("hello world")); // 11 chars / 4 = 2 + 4 overhead = 6
        let after_one = mem.estimated_tokens();
        assert!(after_one > 0);

        mem.push(ChatMessage::user("another message here")); // 20 chars / 4 = 5 + 4 = 9
        let after_two = mem.estimated_tokens();
        assert!(after_two > after_one);

        mem.clear();
        assert_eq!(mem.estimated_tokens(), 0);
    }

    // -- SummarizingMemory empty summarize ----------------------------------

    #[test]
    fn summarize_empty_returns_none_context() {
        let mut mem = SummarizingMemory::new();
        let summary = mem.summarize_oldest(5);
        assert!(summary.content.contains("(none)"));
    }

    // -- Large content truncation in summaries ------------------------------

    #[test]
    fn summary_truncates_long_content() {
        let mut mem = SummarizingMemory::new();
        let long_content = "x".repeat(200);
        mem.push(ChatMessage::user(long_content));
        mem.push(ChatMessage::assistant("short"));

        let summary = mem.summarize_oldest(1);
        // The long content should be truncated in the summary.
        assert!(summary.content.len() < 200);
        assert!(summary.content.contains("..."));
    }
}
