//! ONNX Runtime integration for neural agent policy networks.
//!
//! This module provides an ONNX-backed implementation of the [`PolicyNetwork`] and
//! [`ActionSelector`] traits defined in [`crate::neural_agent`].
//!
//! The ONNX functionality is gated behind the `onnx` Cargo feature. When the feature
//! is disabled, a [`StubOnnxPolicyNetwork`] is available that returns uniform
//! distributions, allowing code to reference the type without requiring the ONNX
//! runtime to be installed.
//!
//! # Feature flag
//!
//! ```toml
//! [dependencies]
//! pod-agents = { path = "../pod-agents", features = ["onnx"] }
//! ```

use crate::neural_agent::{ActionSelector, PolicyNetwork};
use std::fmt;

// ---------------------------------------------------------------------------
// OnnxError
// ---------------------------------------------------------------------------

/// Errors arising from ONNX model loading or inference.
#[derive(Debug, Clone)]
pub enum OnnxError {
    /// The ONNX model file could not be loaded.
    ModelLoadFailed(String),
    /// Inference (forward pass) failed at runtime.
    InferenceFailed(String),
    /// The model produced output with an unexpected shape or type.
    InvalidOutput(String),
}

impl fmt::Display for OnnxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OnnxError::ModelLoadFailed(msg) => write!(f, "ONNX model load failed: {msg}"),
            OnnxError::InferenceFailed(msg) => write!(f, "ONNX inference failed: {msg}"),
            OnnxError::InvalidOutput(msg) => write!(f, "ONNX invalid output: {msg}"),
        }
    }
}

impl std::error::Error for OnnxError {}

// ---------------------------------------------------------------------------
// Softmax utility (used by both gated and non-gated code)
// ---------------------------------------------------------------------------

/// Numerically stable softmax over a slice of logits.
///
/// Returns a `Vec<f32>` of the same length whose elements sum to 1.0.
/// If the input is empty, returns an empty vector.
pub fn softmax(logits: &[f32], temperature: f32) -> Vec<f32> {
    if logits.is_empty() {
        return Vec::new();
    }

    let temp = temperature.max(1e-8); // guard against zero/negative temperature
    let scaled: Vec<f32> = logits.iter().map(|&v| v / temp).collect();
    let max_val = scaled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = scaled.iter().map(|&v| (v - max_val).exp()).collect();
    let sum: f32 = exps.iter().sum();

    if sum == 0.0 || !sum.is_finite() {
        // Fallback: uniform distribution
        let uniform = 1.0 / logits.len() as f32;
        return vec![uniform; logits.len()];
    }

    exps.iter().map(|&e| e / sum).collect()
}

// ---------------------------------------------------------------------------
// OnnxActionSelector (always available — only uses softmax math)
// ---------------------------------------------------------------------------

/// Action selector that applies softmax with temperature then picks argmax.
///
/// This type is always available (no feature gate) because it only performs
/// standard floating-point math and does not depend on the `ort` crate.
pub struct OnnxActionSelector {
    /// Temperature for softmax. Lower values → more greedy, higher → more
    /// exploratory. Default is 1.0.
    pub temperature: f32,
}

impl OnnxActionSelector {
    /// Create a new selector with the given temperature.
    pub fn new(temperature: f32) -> Self {
        Self { temperature }
    }
}

impl Default for OnnxActionSelector {
    fn default() -> Self {
        Self { temperature: 1.0 }
    }
}

impl ActionSelector for OnnxActionSelector {
    fn select_action(&self, logits: &[f32]) -> usize {
        let probs = softmax(logits, self.temperature);
        // Argmax
        probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// StubOnnxPolicyNetwork (always available — no ort dependency)
// ---------------------------------------------------------------------------

/// Stub policy network that returns a uniform probability distribution.
///
/// This is always available regardless of whether the `onnx` feature is
/// enabled. It allows code to reference an ONNX-like type without requiring
/// the ONNX Runtime to be installed.
pub struct StubOnnxPolicyNetwork {
    /// Number of actions in the output distribution.
    action_count: usize,
}

impl StubOnnxPolicyNetwork {
    /// Create a stub network that will return `action_count` uniform probabilities.
    pub fn new(action_count: usize) -> Self {
        Self { action_count }
    }
}

impl PolicyNetwork for StubOnnxPolicyNetwork {
    fn forward(&self, _features: &[f32]) -> Vec<f32> {
        let p = 1.0 / self.action_count as f32;
        vec![p; self.action_count]
    }
}

// ---------------------------------------------------------------------------
// OnnxPolicyNetwork (only available with the `onnx` feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "onnx")]
mod onnx_impl {
    use super::*;
    use ndarray::Array2;
    use ort::session::builder::GraphOptimizationLevel;
    use ort::session::Session;
    use ort::value::TensorRef;
    use std::sync::Arc;

    /// ONNX Runtime-backed policy network.
    ///
    /// Loads an `.onnx` model file and runs inference through the ONNX Runtime.
    /// The model is expected to take a 2-D float32 tensor `[1, N]` as input and
    /// produce a 2-D float32 tensor `[1, M]` as output, where `M` is the
    /// number of action logits / probabilities.
    pub struct OnnxPolicyNetwork {
        session: Arc<Session>,
    }

    // Safety: ort::Session is Send+Sync internally (backed by Arc<SharedSessionInner>).
    // We wrap it in Arc for cheap cloning.
    unsafe impl Sync for OnnxPolicyNetwork {}

    impl OnnxPolicyNetwork {
        /// Load an ONNX model from the given file path.
        ///
        /// # Errors
        ///
        /// Returns [`OnnxError::ModelLoadFailed`] if the file cannot be read or
        /// the ONNX Runtime rejects the model.
        pub fn new(model_path: &str) -> Result<Self, OnnxError> {
            let session = Session::builder()
                .map_err(|e| OnnxError::ModelLoadFailed(format!("session builder: {e}")))?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| OnnxError::ModelLoadFailed(format!("optimization level: {e}")))?
                .with_intra_threads(1)
                .map_err(|e| OnnxError::ModelLoadFailed(format!("intra threads: {e}")))?
                .commit_from_file(model_path)
                .map_err(|e| {
                    OnnxError::ModelLoadFailed(format!("loading '{model_path}': {e}"))
                })?;

            Ok(Self {
                session: Arc::new(session),
            })
        }
    }

    impl PolicyNetwork for OnnxPolicyNetwork {
        fn forward(&self, features: &[f32]) -> Vec<f32> {
            match self.run_inference(features) {
                Ok(output) => output,
                Err(e) => {
                    log::warn!("ONNX inference failed, returning uniform distribution: {e}");
                    // Graceful fallback: uniform distribution.
                    // We don't know M at compile time, so guess from the last
                    // successful output shape. Fall back to 10 (ACTION_COUNT).
                    let n = 10;
                    vec![1.0 / n as f32; n]
                }
            }
        }
    }

    impl OnnxPolicyNetwork {
        /// Internal inference helper that propagates errors.
        fn run_inference(&self, features: &[f32]) -> Result<Vec<f32>, OnnxError> {
            let n_features = features.len();

            // Build a [1, N] input tensor from the feature slice.
            let input_array = Array2::from_shape_vec((1, n_features), features.to_vec())
                .map_err(|e| OnnxError::InferenceFailed(format!("input array: {e}")))?;

            let tensor_ref = TensorRef::from_array_view(&input_array)
                .map_err(|e| OnnxError::InferenceFailed(format!("tensor ref: {e}")))?;

            let outputs = self
                .session
                .run(ort::inputs![tensor_ref])
                .map_err(|e| OnnxError::InferenceFailed(format!("session run: {e}")))?;

            // Extract the first output as a flat f32 array.
            let output_tensor: ndarray::ArrayViewD<f32> = outputs[0]
                .try_extract_array()
                .map_err(|e| OnnxError::InvalidOutput(format!("extract array: {e}")))?;

            let output_vec: Vec<f32> = output_tensor.iter().copied().collect();

            if output_vec.is_empty() {
                return Err(OnnxError::InvalidOutput(
                    "model produced empty output".into(),
                ));
            }

            Ok(output_vec)
        }
    }
}

#[cfg(feature = "onnx")]
pub use onnx_impl::OnnxPolicyNetwork;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neural_agent::PolicyNetwork;

    // -- Non-gated tests (always run) --

    #[test]
    fn test_stub_returns_correct_size() {
        let stub = StubOnnxPolicyNetwork::new(10);
        let output = stub.forward(&[0.0; 32]);
        assert_eq!(output.len(), 10);
    }

    #[test]
    fn test_stub_returns_uniform() {
        let stub = StubOnnxPolicyNetwork::new(5);
        let output = stub.forward(&[1.0, 2.0, 3.0]);
        let expected = 1.0 / 5.0;
        for &val in &output {
            assert!((val - expected).abs() < 1e-6, "Expected uniform {expected}, got {val}");
        }
    }

    #[test]
    fn test_softmax_basic() {
        let logits = vec![1.0, 2.0, 3.0];
        let probs = softmax(&logits, 1.0);
        assert_eq!(probs.len(), 3);

        // Should sum to ~1.0
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "Softmax sum = {sum}");

        // Probabilities should be monotonically increasing for increasing logits
        assert!(probs[0] < probs[1]);
        assert!(probs[1] < probs[2]);
    }

    #[test]
    fn test_softmax_uniform_input() {
        let logits = vec![0.0, 0.0, 0.0, 0.0];
        let probs = softmax(&logits, 1.0);
        let expected = 0.25;
        for &p in &probs {
            assert!((p - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn test_softmax_empty() {
        let probs = softmax(&[], 1.0);
        assert!(probs.is_empty());
    }

    #[test]
    fn test_softmax_temperature_effect() {
        let logits = vec![1.0, 2.0, 3.0];

        // Low temperature → sharper distribution (higher max probability)
        let sharp = softmax(&logits, 0.1);
        // High temperature → flatter distribution (lower max probability)
        let flat = softmax(&logits, 10.0);

        let sharp_max = sharp.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let flat_max = flat.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        assert!(
            sharp_max > flat_max,
            "Low temp max ({sharp_max}) should be > high temp max ({flat_max})"
        );
    }

    #[test]
    fn test_softmax_numerical_stability() {
        // Very large logits should not produce NaN/Inf thanks to max subtraction
        let logits = vec![1000.0, 1001.0, 1002.0];
        let probs = softmax(&logits, 1.0);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-4, "Large logit softmax sum = {sum}");
        for &p in &probs {
            assert!(p.is_finite(), "Softmax produced non-finite value");
        }
    }

    #[test]
    fn test_onnx_action_selector_picks_argmax() {
        let selector = OnnxActionSelector::new(1.0);
        // logits where index 2 is clearly dominant
        let logits = vec![0.0, 0.0, 10.0, 0.0, 0.0];
        let action = selector.select_action(&logits);
        assert_eq!(action, 2);
    }

    #[test]
    fn test_onnx_action_selector_low_temperature_greedy() {
        let selector = OnnxActionSelector::new(0.01);
        let logits = vec![1.0, 1.5, 3.0, 2.0];
        let action = selector.select_action(&logits);
        assert_eq!(action, 2, "Very low temperature should pick the max logit");
    }

    #[test]
    fn test_onnx_action_selector_default_temperature() {
        let selector = OnnxActionSelector::default();
        assert!((selector.temperature - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_onnx_error_display() {
        let e = OnnxError::ModelLoadFailed("file not found".into());
        assert!(e.to_string().contains("model load failed"));
        assert!(e.to_string().contains("file not found"));

        let e = OnnxError::InferenceFailed("bad input shape".into());
        assert!(e.to_string().contains("inference failed"));

        let e = OnnxError::InvalidOutput("empty tensor".into());
        assert!(e.to_string().contains("invalid output"));
    }
}

/// Tests that require the `onnx` feature flag (and therefore the ort crate).
#[cfg(all(test, feature = "onnx"))]
mod onnx_tests {
    use super::*;

    #[test]
    fn test_onnx_policy_network_missing_model() {
        // Loading a nonexistent model should return ModelLoadFailed
        let result = OnnxPolicyNetwork::new("/nonexistent/path/model.onnx");
        assert!(result.is_err());
        match result.unwrap_err() {
            OnnxError::ModelLoadFailed(msg) => {
                assert!(
                    !msg.is_empty(),
                    "Error message should be non-empty"
                );
            }
            other => panic!("Expected ModelLoadFailed, got: {other:?}"),
        }
    }
}
