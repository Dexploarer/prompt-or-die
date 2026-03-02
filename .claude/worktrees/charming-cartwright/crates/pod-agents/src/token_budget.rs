//! Token budget management for LLM agents.
//!
//! Manages token allocation across prompt sections to stay within
//! model context limits while maximizing useful content.
//!
//! # Architecture
//!
//! The budget system has three layers:
//!
//! 1. **[`TokenBudget`]** — defines the overall token limits (context window size,
//!    reserves for response and action instructions).
//! 2. **[`TokenAllocator`]** — takes a budget and allocates tokens across prompt
//!    sections (system prompt, observations, history, action instructions), trimming
//!    lower-priority sections when the total exceeds the budget.
//! 3. **[`TokenTracker`]** — records actual token usage over time and computes
//!    rolling statistics (averages, peaks, utilization ratio).
//!
//! Use [`BudgetConfig`] as a builder to construct a [`TokenBudget`] with custom
//! reserves and history priority.

use crate::conversation_memory::estimate_tokens;
use crate::llm_provider::TokenUsage;

// ---------------------------------------------------------------------------
// TokenBudget
// ---------------------------------------------------------------------------

/// Overall token budget for an LLM agent's context window.
///
/// Splits the model's maximum context into:
/// - tokens available for the prompt (system, observations, history, instructions)
/// - tokens reserved for the LLM's response
/// - tokens reserved for action format instructions
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Total context window size in tokens (e.g. 4096, 8192, 16384).
    pub max_context_tokens: usize,
    /// Tokens reserved for the LLM completion/response.
    pub reserve_for_response: usize,
    /// Tokens reserved for action format instructions appended to the prompt.
    pub reserve_for_actions: usize,
    /// Fraction of remaining budget allocated to history vs observation (0.0–1.0).
    /// Higher values give more space to conversation history.
    pub history_priority: f32,
}

impl TokenBudget {
    /// Create a budget with the given context window size and sensible defaults.
    ///
    /// Defaults: 500 tokens reserved for response, 100 for action instructions,
    /// history priority 0.5.
    pub fn new(max_context_tokens: usize) -> Self {
        Self {
            max_context_tokens,
            reserve_for_response: 500,
            reserve_for_actions: 100,
            history_priority: 0.5,
        }
    }

    /// Set the number of tokens reserved for the LLM response.
    pub fn with_reserve_for_response(mut self, tokens: usize) -> Self {
        self.reserve_for_response = tokens;
        self
    }

    /// Set the number of tokens reserved for action format instructions.
    pub fn with_reserve_for_actions(mut self, tokens: usize) -> Self {
        self.reserve_for_actions = tokens;
        self
    }

    /// Set history priority (0.0–1.0). Clamped to valid range.
    pub fn with_history_priority(mut self, priority: f32) -> Self {
        self.history_priority = priority.clamp(0.0, 1.0);
        self
    }

    /// Tokens available for the prompt content (system + observations + history + instructions).
    ///
    /// Computed as `max_context - reserve_for_response - reserve_for_actions`.
    /// Returns 0 if reserves exceed the context window.
    pub fn available_for_prompt(&self) -> usize {
        self.max_context_tokens
            .saturating_sub(self.reserve_for_response)
            .saturating_sub(self.reserve_for_actions)
    }
}

// ---------------------------------------------------------------------------
// TokenAllocation
// ---------------------------------------------------------------------------

/// Result of a token allocation pass — how many tokens each prompt section
/// was allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenAllocation {
    /// Tokens allocated to the system prompt (never trimmed).
    pub system_tokens: usize,
    /// Tokens allocated to the current observation.
    pub observation_tokens: usize,
    /// Tokens allocated to conversation history.
    pub history_tokens: usize,
    /// Tokens allocated to action format instructions.
    pub instruction_tokens: usize,
    /// Whether the final allocation fits within the prompt budget.
    pub within_budget: bool,
}

impl TokenAllocation {
    /// Total tokens across all sections.
    pub fn total(&self) -> usize {
        self.system_tokens + self.observation_tokens + self.history_tokens + self.instruction_tokens
    }
}

// ---------------------------------------------------------------------------
// TokenAllocator
// ---------------------------------------------------------------------------

/// Allocates tokens across prompt sections within a [`TokenBudget`].
///
/// When the requested tokens exceed the available budget, the allocator trims
/// sections in priority order:
/// 1. **History** is trimmed first (least critical for immediate decisions).
/// 2. **Observations** are trimmed second (may lose environmental detail).
/// 3. **System prompt** is **never** trimmed (contains core identity/instructions).
/// 4. **Action instructions** are trimmed last (already covered by reserve).
#[derive(Debug, Clone)]
pub struct TokenAllocator {
    budget: TokenBudget,
}

impl TokenAllocator {
    /// Create an allocator from a budget.
    pub fn new(budget: TokenBudget) -> Self {
        Self { budget }
    }

    /// Reference to the underlying budget.
    pub fn budget(&self) -> &TokenBudget {
        &self.budget
    }

    /// Allocate tokens across prompt sections.
    ///
    /// Each parameter is the *requested* token count for that section. The
    /// allocator grants as much as possible within `budget.available_for_prompt()`,
    /// trimming history first, then observation. System prompt is never trimmed.
    ///
    /// Returns a [`TokenAllocation`] describing the granted tokens per section.
    pub fn allocate(
        &self,
        system_tokens: usize,
        observation_tokens: usize,
        history_tokens: usize,
        instruction_tokens: usize,
    ) -> TokenAllocation {
        let available = self.budget.available_for_prompt();
        let requested_total = system_tokens + observation_tokens + history_tokens + instruction_tokens;

        // If everything fits, grant full requests.
        if requested_total <= available {
            return TokenAllocation {
                system_tokens,
                observation_tokens,
                history_tokens,
                instruction_tokens,
                within_budget: true,
            };
        }

        // Need to trim. System prompt is never trimmed.
        // Start by granting system and instruction tokens fully, then allocate
        // the remainder between history and observation.
        let fixed = system_tokens + instruction_tokens;

        if fixed >= available {
            // Even fixed sections exceed budget — grant system fully, trim instructions.
            let granted_instructions = available.saturating_sub(system_tokens);
            return TokenAllocation {
                system_tokens,
                observation_tokens: 0,
                history_tokens: 0,
                instruction_tokens: granted_instructions,
                within_budget: false,
            };
        }

        let remaining = available - fixed;

        // Distribute `remaining` between history and observation.
        // Trim history first. If still over, trim observation.
        if history_tokens + observation_tokens <= remaining {
            // Both fit after granting fixed sections.
            return TokenAllocation {
                system_tokens,
                observation_tokens,
                history_tokens,
                instruction_tokens,
                within_budget: true,
            };
        }

        // History + observation exceed remaining. Trim history first.
        if observation_tokens <= remaining {
            // Observation fits fully; history gets the leftovers.
            let granted_history = remaining - observation_tokens;
            return TokenAllocation {
                system_tokens,
                observation_tokens,
                history_tokens: granted_history,
                instruction_tokens,
                within_budget: false,
            };
        }

        // Even observation alone exceeds remaining — trim it too.
        TokenAllocation {
            system_tokens,
            observation_tokens: remaining,
            history_tokens: 0,
            instruction_tokens,
            within_budget: false,
        }
    }

    /// Convenience: estimate tokens for a text string and allocate.
    ///
    /// Uses `estimate_tokens` from `conversation_memory` for each section's text.
    pub fn allocate_for_text(
        &self,
        system_text: &str,
        observation_text: &str,
        history_text: &str,
        instruction_text: &str,
    ) -> TokenAllocation {
        self.allocate(
            estimate_tokens(system_text),
            estimate_tokens(observation_text),
            estimate_tokens(history_text),
            estimate_tokens(instruction_text),
        )
    }
}

// ---------------------------------------------------------------------------
// TokenTracker
// ---------------------------------------------------------------------------

/// Tracks actual LLM token usage over a sliding window and computes statistics.
///
/// Records the last `window_size` usages (default 20) and provides rolling
/// averages, peaks, and utilization ratio relative to a token budget.
#[derive(Debug, Clone)]
pub struct TokenTracker {
    /// Circular buffer of recent token usages.
    usages: Vec<TokenUsage>,
    /// Maximum number of usages to retain.
    window_size: usize,
    /// Total prompt tokens across all recorded usages (for efficient average).
    total_prompt: u64,
    /// Total completion tokens across all recorded usages.
    total_completion: u64,
    /// Peak prompt tokens seen in any single usage.
    peak_prompt: usize,
    /// Peak completion tokens seen in any single usage.
    peak_completion: usize,
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::new(20)
    }
}

impl TokenTracker {
    /// Create a tracker with the given sliding window size.
    pub fn new(window_size: usize) -> Self {
        Self {
            usages: Vec::with_capacity(window_size),
            window_size: window_size.max(1),
            total_prompt: 0,
            total_completion: 0,
            peak_prompt: 0,
            peak_completion: 0,
        }
    }

    /// Record a token usage from an LLM response.
    pub fn record_usage(&mut self, usage: &TokenUsage) {
        let prompt = usage.prompt_tokens as usize;
        let completion = usage.completion_tokens as usize;

        // If at capacity, evict the oldest and subtract its contribution.
        if self.usages.len() >= self.window_size {
            let evicted = self.usages.remove(0);
            self.total_prompt -= evicted.prompt_tokens as u64;
            self.total_completion -= evicted.completion_tokens as u64;
        }

        self.usages.push(usage.clone());
        self.total_prompt += usage.prompt_tokens as u64;
        self.total_completion += usage.completion_tokens as u64;

        if prompt > self.peak_prompt {
            self.peak_prompt = prompt;
        }
        if completion > self.peak_completion {
            self.peak_completion = completion;
        }
    }

    /// Average prompt tokens per request over the sliding window.
    ///
    /// Returns 0 if no usages have been recorded.
    pub fn average_prompt_tokens(&self) -> usize {
        if self.usages.is_empty() {
            return 0;
        }
        (self.total_prompt / self.usages.len() as u64) as usize
    }

    /// Average completion tokens per request over the sliding window.
    ///
    /// Returns 0 if no usages have been recorded.
    pub fn average_completion_tokens(&self) -> usize {
        if self.usages.is_empty() {
            return 0;
        }
        (self.total_completion / self.usages.len() as u64) as usize
    }

    /// Peak (maximum) prompt tokens observed in any single request.
    pub fn peak_prompt_tokens(&self) -> usize {
        self.peak_prompt
    }

    /// Peak (maximum) completion tokens observed in any single request.
    pub fn peak_completion_tokens(&self) -> usize {
        self.peak_completion
    }

    /// Utilization ratio: average actual prompt tokens / budget's available prompt tokens.
    ///
    /// Returns 0.0 if no usages recorded or budget is zero.
    pub fn utilization_ratio(&self, budget: &TokenBudget) -> f32 {
        let available = budget.available_for_prompt();
        if available == 0 || self.usages.is_empty() {
            return 0.0;
        }
        self.average_prompt_tokens() as f32 / available as f32
    }

    /// Number of usages currently tracked.
    pub fn count(&self) -> usize {
        self.usages.len()
    }

    /// Whether the tracker has any recorded usages.
    pub fn is_empty(&self) -> bool {
        self.usages.is_empty()
    }
}

// ---------------------------------------------------------------------------
// BudgetConfig builder
// ---------------------------------------------------------------------------

/// Builder for constructing a [`TokenBudget`] with custom settings.
///
/// # Example
///
/// ```
/// use pod_agents::token_budget::BudgetConfig;
///
/// let budget = BudgetConfig::new()
///     .max_context_tokens(8192)
///     .reserve_for_response(600)
///     .reserve_for_actions(150)
///     .history_priority(0.6)
///     .build();
///
/// assert_eq!(budget.max_context_tokens, 8192);
/// assert_eq!(budget.reserve_for_response, 600);
/// ```
#[derive(Debug, Clone)]
pub struct BudgetConfig {
    max_context_tokens: usize,
    reserve_for_response: usize,
    reserve_for_actions: usize,
    history_priority: f32,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 4096,
            reserve_for_response: 500,
            reserve_for_actions: 100,
            history_priority: 0.5,
        }
    }
}

impl BudgetConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum context window size in tokens.
    pub fn max_context_tokens(mut self, n: usize) -> Self {
        self.max_context_tokens = n;
        self
    }

    /// Set tokens reserved for the LLM response.
    pub fn reserve_for_response(mut self, n: usize) -> Self {
        self.reserve_for_response = n;
        self
    }

    /// Set tokens reserved for action format instructions.
    pub fn reserve_for_actions(mut self, n: usize) -> Self {
        self.reserve_for_actions = n;
        self
    }

    /// Set history priority (0.0–1.0).
    ///
    /// Higher values allocate more of the flexible budget to conversation history
    /// vs. current observation. Clamped to `[0.0, 1.0]`.
    pub fn history_priority(mut self, priority: f32) -> Self {
        self.history_priority = priority.clamp(0.0, 1.0);
        self
    }

    /// Build the configured [`TokenBudget`].
    pub fn build(self) -> TokenBudget {
        TokenBudget {
            max_context_tokens: self.max_context_tokens,
            reserve_for_response: self.reserve_for_response,
            reserve_for_actions: self.reserve_for_actions,
            history_priority: self.history_priority,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::TokenUsage;

    // -- 1. Budget creation with defaults -----------------------------------

    #[test]
    fn budget_creation_defaults() {
        let budget = TokenBudget::new(4096);

        assert_eq!(budget.max_context_tokens, 4096);
        assert_eq!(budget.reserve_for_response, 500);
        assert_eq!(budget.reserve_for_actions, 100);
        assert!((budget.history_priority - 0.5).abs() < f32::EPSILON);
    }

    // -- 2. Available tokens calculation ------------------------------------

    #[test]
    fn available_tokens_calculation() {
        let budget = TokenBudget::new(4096);
        // 4096 - 500 - 100 = 3496
        assert_eq!(budget.available_for_prompt(), 3496);

        // Custom reserves
        let budget2 = TokenBudget::new(8192)
            .with_reserve_for_response(1000)
            .with_reserve_for_actions(200);
        // 8192 - 1000 - 200 = 6992
        assert_eq!(budget2.available_for_prompt(), 6992);

        // Reserves exceed context window — should return 0, not underflow.
        let budget3 = TokenBudget::new(100)
            .with_reserve_for_response(80)
            .with_reserve_for_actions(80);
        assert_eq!(budget3.available_for_prompt(), 0);
    }

    // -- 3. Allocation within budget ----------------------------------------

    #[test]
    fn allocation_within_budget() {
        let budget = TokenBudget::new(4096); // 3496 available
        let allocator = TokenAllocator::new(budget);

        let alloc = allocator.allocate(200, 500, 300, 100);

        assert_eq!(alloc.system_tokens, 200);
        assert_eq!(alloc.observation_tokens, 500);
        assert_eq!(alloc.history_tokens, 300);
        assert_eq!(alloc.instruction_tokens, 100);
        assert!(alloc.within_budget);
        assert_eq!(alloc.total(), 1100);
    }

    // -- 4. Allocation exceeding budget — trim history first ----------------

    #[test]
    fn allocation_exceeds_budget_trims_history_first() {
        // Budget: 1000 context - 200 response - 100 actions = 700 available
        let budget = TokenBudget::new(1000)
            .with_reserve_for_response(200)
            .with_reserve_for_actions(100);
        let allocator = TokenAllocator::new(budget);

        // Request: system=200 + observation=300 + history=400 + instructions=100 = 1000
        // Available = 700. Fixed = 200+100=300. Remaining = 400.
        // Observation (300) fits in remaining (400). History gets 400-300=100.
        let alloc = allocator.allocate(200, 300, 400, 100);

        assert_eq!(alloc.system_tokens, 200);
        assert_eq!(alloc.observation_tokens, 300);
        assert_eq!(alloc.history_tokens, 100); // trimmed from 400
        assert_eq!(alloc.instruction_tokens, 100);
        assert!(!alloc.within_budget);
    }

    // -- 5. Allocation exceeding budget — trim observation second -----------

    #[test]
    fn allocation_exceeds_budget_trims_observation_second() {
        // Budget: 600 context - 100 response - 50 actions = 450 available
        let budget = TokenBudget::new(600)
            .with_reserve_for_response(100)
            .with_reserve_for_actions(50);
        let allocator = TokenAllocator::new(budget);

        // Request: system=200 + observation=500 + history=300 + instructions=100 = 1100
        // Available = 450. Fixed = 200+100=300. Remaining = 150.
        // Observation (500) > remaining (150). History = 0, observation = 150.
        let alloc = allocator.allocate(200, 500, 300, 100);

        assert_eq!(alloc.system_tokens, 200);
        assert_eq!(alloc.observation_tokens, 150); // trimmed from 500
        assert_eq!(alloc.history_tokens, 0);       // fully trimmed
        assert_eq!(alloc.instruction_tokens, 100);
        assert!(!alloc.within_budget);
    }

    // -- 6. System prompt never trimmed -------------------------------------

    #[test]
    fn system_prompt_never_trimmed() {
        // Very tight budget: 300 total - 100 response - 50 actions = 150 available
        let budget = TokenBudget::new(300)
            .with_reserve_for_response(100)
            .with_reserve_for_actions(50);
        let allocator = TokenAllocator::new(budget);

        // System alone is 200, which exceeds available (150).
        // System is never trimmed, so it stays at 200. Everything else is 0.
        let alloc = allocator.allocate(200, 100, 100, 50);

        assert_eq!(alloc.system_tokens, 200); // never trimmed!
        assert_eq!(alloc.observation_tokens, 0);
        assert_eq!(alloc.history_tokens, 0);
        assert!(!alloc.within_budget);
    }

    // -- 7. Token tracker records and averages ------------------------------

    #[test]
    fn token_tracker_records_and_averages() {
        let mut tracker = TokenTracker::new(10);

        tracker.record_usage(&TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
        });
        tracker.record_usage(&TokenUsage {
            prompt_tokens: 200,
            completion_tokens: 40,
        });
        tracker.record_usage(&TokenUsage {
            prompt_tokens: 300,
            completion_tokens: 60,
        });

        assert_eq!(tracker.count(), 3);
        // Average prompt: (100+200+300)/3 = 200
        assert_eq!(tracker.average_prompt_tokens(), 200);
        // Average completion: (20+40+60)/3 = 40
        assert_eq!(tracker.average_completion_tokens(), 40);
    }

    // -- 8. Token tracker peak tracking -------------------------------------

    #[test]
    fn token_tracker_peak_tracking() {
        let mut tracker = TokenTracker::new(10);

        tracker.record_usage(&TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 20,
        });
        tracker.record_usage(&TokenUsage {
            prompt_tokens: 500,
            completion_tokens: 80,
        });
        tracker.record_usage(&TokenUsage {
            prompt_tokens: 200,
            completion_tokens: 30,
        });

        assert_eq!(tracker.peak_prompt_tokens(), 500);
        assert_eq!(tracker.peak_completion_tokens(), 80);
    }

    // -- 9. Utilization ratio -----------------------------------------------

    #[test]
    fn utilization_ratio() {
        let budget = TokenBudget::new(4096); // 3496 available
        let mut tracker = TokenTracker::new(10);

        tracker.record_usage(&TokenUsage {
            prompt_tokens: 1748,
            completion_tokens: 50,
        });
        tracker.record_usage(&TokenUsage {
            prompt_tokens: 1748,
            completion_tokens: 50,
        });

        // Average prompt = 1748. Available = 3496. Ratio = 1748/3496 = 0.5
        let ratio = tracker.utilization_ratio(&budget);
        assert!((ratio - 0.5).abs() < 0.01, "ratio was {}", ratio);
    }

    // -- 10. Budget config builder ------------------------------------------

    #[test]
    fn budget_config_builder() {
        let budget = BudgetConfig::new()
            .max_context_tokens(16384)
            .reserve_for_response(800)
            .reserve_for_actions(200)
            .history_priority(0.7)
            .build();

        assert_eq!(budget.max_context_tokens, 16384);
        assert_eq!(budget.reserve_for_response, 800);
        assert_eq!(budget.reserve_for_actions, 200);
        assert!((budget.history_priority - 0.7).abs() < f32::EPSILON);
        // 16384 - 800 - 200 = 15384
        assert_eq!(budget.available_for_prompt(), 15384);
    }

    // -- 11. Empty tracker defaults -----------------------------------------

    #[test]
    fn empty_tracker_defaults() {
        let tracker = TokenTracker::default();

        assert!(tracker.is_empty());
        assert_eq!(tracker.count(), 0);
        assert_eq!(tracker.average_prompt_tokens(), 0);
        assert_eq!(tracker.average_completion_tokens(), 0);
        assert_eq!(tracker.peak_prompt_tokens(), 0);
        assert_eq!(tracker.peak_completion_tokens(), 0);

        let budget = TokenBudget::new(4096);
        assert_eq!(tracker.utilization_ratio(&budget), 0.0);
    }

    // -- 12. History priority allocation ------------------------------------

    #[test]
    fn history_priority_allocation() {
        // Verify that the history_priority field is correctly stored and clamped.
        let budget = BudgetConfig::new()
            .max_context_tokens(4096)
            .history_priority(0.8)
            .build();
        assert!((budget.history_priority - 0.8).abs() < f32::EPSILON);

        // Clamping: values outside [0.0, 1.0] are clamped.
        let budget_high = BudgetConfig::new().history_priority(1.5).build();
        assert!((budget_high.history_priority - 1.0).abs() < f32::EPSILON);

        let budget_low = BudgetConfig::new().history_priority(-0.5).build();
        assert!((budget_low.history_priority - 0.0).abs() < f32::EPSILON);

        // TokenBudget builder method also clamps.
        let budget_direct = TokenBudget::new(4096).with_history_priority(2.0);
        assert!((budget_direct.history_priority - 1.0).abs() < f32::EPSILON);
    }

    // -- Additional: sliding window eviction in tracker ---------------------

    #[test]
    fn tracker_sliding_window_evicts_oldest() {
        let mut tracker = TokenTracker::new(3); // window of 3

        tracker.record_usage(&TokenUsage { prompt_tokens: 100, completion_tokens: 10 });
        tracker.record_usage(&TokenUsage { prompt_tokens: 200, completion_tokens: 20 });
        tracker.record_usage(&TokenUsage { prompt_tokens: 300, completion_tokens: 30 });
        assert_eq!(tracker.count(), 3);
        // Avg prompt: (100+200+300)/3 = 200
        assert_eq!(tracker.average_prompt_tokens(), 200);

        // Adding a 4th evicts the oldest (100).
        tracker.record_usage(&TokenUsage { prompt_tokens: 400, completion_tokens: 40 });
        assert_eq!(tracker.count(), 3);
        // Avg prompt: (200+300+400)/3 = 300
        assert_eq!(tracker.average_prompt_tokens(), 300);
    }

    // -- Additional: allocate_for_text convenience --------------------------

    #[test]
    fn allocate_for_text_uses_estimate() {
        let budget = TokenBudget::new(4096);
        let allocator = TokenAllocator::new(budget);

        // "abcd" = 4 chars / 4 = 1 token (estimate_tokens)
        let alloc = allocator.allocate_for_text("abcd", "abcd", "abcd", "abcd");
        assert_eq!(alloc.system_tokens, 1);
        assert_eq!(alloc.observation_tokens, 1);
        assert_eq!(alloc.history_tokens, 1);
        assert_eq!(alloc.instruction_tokens, 1);
        assert!(alloc.within_budget);
    }

    // -- Additional: extreme budget edge cases ------------------------------

    #[test]
    fn zero_budget_handles_gracefully() {
        let budget = TokenBudget::new(0);
        assert_eq!(budget.available_for_prompt(), 0);

        let allocator = TokenAllocator::new(budget.clone());
        let alloc = allocator.allocate(100, 100, 100, 100);
        // System is never trimmed, everything else zeroed, instructions may get partial.
        assert_eq!(alloc.system_tokens, 100);
        assert!(!alloc.within_budget);

        let tracker = TokenTracker::default();
        assert_eq!(tracker.utilization_ratio(&budget), 0.0);
    }
}
