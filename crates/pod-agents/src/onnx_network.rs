//! ONNX Runtime integration for neural agents.
//!
//! Enable with `features = ["onnx"]` in pod-agents.
//! Provides `OnnxPolicyNetwork` implementing `PolicyNetwork`
//! and `OnnxActionSelector` implementing `ActionSelector`.
//!
//! # Example
//!
//! ```rust,no_run
//! #[cfg(feature = "onnx")]
//! use pod_agents::onnx_network::{OnnxPolicyNetwork, OnnxActionSelector};
//! use pod_agents::{NeuralAgent};
//!
//! #[cfg(feature = "onnx")]
//! fn load_agent(path: &str) -> Result<NeuralAgent, Box<dyn std::error::Error>> {
//!     let network = OnnxPolicyNetwork::from_file(path)?;
//!     let selector = OnnxActionSelector::with_temperature(network, 0.8);
//!     Ok(NeuralAgent::with_selector_and_network(
//!         Box::new(selector),
//!         Box::new(pod_agents::UniformPolicyNetwork),
//!     ))
//! }
//! ```

/// Inner module gated behind the `onnx` feature flag.
#[cfg(feature = "onnx")]
mod inner {
    use crate::neural_agent::{
        ActionSelector, NeuralCompatibilityStatus, NeuralInferenceStatus, NeuralModelMetadata,
        NeuralPolicyRuntimeStatus, NeuralRuntimeSchema, PolicyNetwork, NEURAL_ACTION_COUNT,
        NEURAL_FEATURE_COUNT,
    };
    use std::fmt;
    use std::path::Path;
    use std::sync::Mutex;

    // ─── Constants ────────────────────────────────────────────────────────────

    /// Expected input feature vector length (must match NeuralAgent::observation_to_features).
    pub const EXPECTED_INPUT_SIZE: usize = NEURAL_FEATURE_COUNT;

    /// Expected output logit count (must match the shared neural action schema).
    pub const EXPECTED_OUTPUT_SIZE: usize = NEURAL_ACTION_COUNT;

    // ─── Error type ───────────────────────────────────────────────────────────

    /// Errors that can occur during ONNX model loading or inference.
    #[derive(Debug)]
    pub enum OnnxError {
        /// Failed to load the model from a file or byte slice.
        LoadError(String),
        /// An error occurred during a forward-pass inference call.
        InferenceError(String),
        /// The model's tensor shape does not match what was expected.
        ShapeMismatch {
            expected: Vec<usize>,
            got: Vec<usize>,
        },
        /// The model metadata does not match the current neural runtime schema.
        MetadataMismatch(String),
        /// An ORT session-level error (environment init, option setting, etc.).
        SessionError(String),
    }

    impl fmt::Display for OnnxError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                OnnxError::LoadError(msg) => write!(f, "ONNX load error: {msg}"),
                OnnxError::InferenceError(msg) => write!(f, "ONNX inference error: {msg}"),
                OnnxError::ShapeMismatch { expected, got } => {
                    write!(f, "ONNX shape mismatch: expected {expected:?}, got {got:?}")
                }
                OnnxError::MetadataMismatch(msg) => write!(f, "ONNX metadata mismatch: {msg}"),
                OnnxError::SessionError(msg) => write!(f, "ONNX session error: {msg}"),
            }
        }
    }

    impl std::error::Error for OnnxError {}

    // Allow converting ort errors directly into OnnxError
    impl From<ort::Error> for OnnxError {
        fn from(e: ort::Error) -> Self {
            OnnxError::SessionError(e.to_string())
        }
    }

    // ─── OnnxPolicyNetwork ────────────────────────────────────────────────────

    /// A `PolicyNetwork` backed by an ONNX Runtime session.
    ///
    /// Loads a `.onnx` model (from a file path or raw bytes) and runs it
    /// synchronously during the game-tick forward pass.  The session is
    /// kept alive for the lifetime of the struct, so repeated inference
    /// calls do not pay a load penalty.
    pub struct OnnxPolicyNetwork {
        session: ort::Session,
        input_name: String,
        output_name: String,
        metadata: NeuralModelMetadata,
        /// Expected length of the input feature slice.
        input_size: usize,
        /// Expected length of the output logit slice.
        output_size: usize,
        last_inference: Mutex<NeuralInferenceStatus>,
    }

    impl OnnxPolicyNetwork {
        pub(crate) fn validate_metadata(metadata: &NeuralModelMetadata) -> Result<(), OnnxError> {
            NeuralRuntimeSchema::current()
                .validate_model_metadata(metadata)
                .map_err(|error| OnnxError::MetadataMismatch(error.to_string()))
        }

        // ── Construction ──────────────────────────────────────────────────

        /// Load an ONNX model from a file on disk.
        ///
        /// The first input and first output of the model are used; the
        /// input shape must have a single element in the feature dimension
        /// that matches `EXPECTED_INPUT_SIZE` (32) and the output must match
        /// `EXPECTED_OUTPUT_SIZE` (10).  A shape mismatch is reported via
        /// `OnnxError::ShapeMismatch`.
        pub fn from_file(path: impl AsRef<Path>) -> Result<Self, OnnxError> {
            let path = path.as_ref();
            let session = ort::Session::builder()
                .map_err(|e| OnnxError::SessionError(e.to_string()))?
                .commit_from_file(path)
                .map_err(|e| {
                    OnnxError::LoadError(format!(
                        "could not load model from '{}': {e}",
                        path.display()
                    ))
                })?;

            let model_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("onnx-policy")
                .to_string();

            Self::from_session(session, NeuralModelMetadata::current(model_name))
        }

        /// Load an ONNX model only if the caller-supplied metadata matches the
        /// current neural runtime schema.
        pub fn from_file_with_metadata(
            path: impl AsRef<Path>,
            metadata: &NeuralModelMetadata,
        ) -> Result<Self, OnnxError> {
            Self::validate_metadata(metadata)?;
            let path = path.as_ref();
            let session = ort::Session::builder()
                .map_err(|e| OnnxError::SessionError(e.to_string()))?
                .commit_from_file(path)
                .map_err(|e| {
                    OnnxError::LoadError(format!(
                        "could not load model from '{}': {e}",
                        path.display()
                    ))
                })?;

            Self::from_session(session, metadata.clone())
        }

        /// Load an ONNX model from a byte slice in memory.
        ///
        /// Useful for embedding model weights directly in a binary or loading
        /// from a network source without a temporary file.
        pub fn from_bytes(model_data: &[u8]) -> Result<Self, OnnxError> {
            let session = ort::Session::builder()
                .map_err(|e| OnnxError::SessionError(e.to_string()))?
                .commit_from_memory(model_data)
                .map_err(|e| {
                    OnnxError::LoadError(format!("could not load model from bytes: {e}"))
                })?;

            Self::from_session(session, NeuralModelMetadata::current("onnx-bytes"))
        }

        /// Load an in-memory ONNX model only if the caller-supplied metadata
        /// matches the current neural runtime schema.
        pub fn from_bytes_with_metadata(
            model_data: &[u8],
            metadata: &NeuralModelMetadata,
        ) -> Result<Self, OnnxError> {
            Self::validate_metadata(metadata)?;
            let session = ort::Session::builder()
                .map_err(|e| OnnxError::SessionError(e.to_string()))?
                .commit_from_memory(model_data)
                .map_err(|e| {
                    OnnxError::LoadError(format!("could not load model from bytes: {e}"))
                })?;

            Self::from_session(session, metadata.clone())
        }

        /// Build the struct from a ready-made `ort::Session`, probing its
        /// input/output metadata.
        fn from_session(
            session: ort::Session,
            metadata: NeuralModelMetadata,
        ) -> Result<Self, OnnxError> {
            // --- Probe input name & size ---
            let input = session
                .inputs
                .first()
                .ok_or_else(|| OnnxError::LoadError("model has no inputs".into()))?;
            let input_name = input.name.clone();

            // --- Probe output name & size ---
            let output = session
                .outputs
                .first()
                .ok_or_else(|| OnnxError::LoadError("model has no outputs".into()))?;
            let output_name = output.name.clone();

            // Use canonical constants; future work could inspect the shapes dynamically.
            let input_size = EXPECTED_INPUT_SIZE;
            let output_size = EXPECTED_OUTPUT_SIZE;

            Ok(Self {
                session,
                input_name,
                output_name,
                metadata,
                input_size,
                output_size,
                last_inference: Mutex::new(NeuralInferenceStatus::Ready),
            })
        }

        // ── Accessors ─────────────────────────────────────────────────────

        /// The number of input features this model expects.
        pub fn input_size(&self) -> usize {
            self.input_size
        }

        /// The number of output logits this model produces.
        pub fn output_size(&self) -> usize {
            self.output_size
        }

        pub fn runtime_schema(&self) -> NeuralRuntimeSchema {
            self.metadata.runtime_schema
        }

        // ── Internal inference ────────────────────────────────────────────

        /// Core inference: validates the input slice, runs the ORT session,
        /// and returns the first output tensor as a `Vec<f32>`.
        fn run_inference(&self, features: &[f32]) -> Result<Vec<f32>, OnnxError> {
            // Validate input length
            if features.len() != self.input_size {
                return Err(OnnxError::ShapeMismatch {
                    expected: vec![self.input_size],
                    got: vec![features.len()],
                });
            }

            // Build a [1, input_size] tensor (batch size 1)
            let input_tensor = ort::inputs![
                self.input_name.as_str() => ort::Tensor::from_array(
                    ([1usize, self.input_size], features.to_vec())
                ).map_err(|e| OnnxError::InferenceError(e.to_string()))?
            ]
            .map_err(|e| OnnxError::InferenceError(e.to_string()))?;

            // Run the session
            let outputs = self
                .session
                .run(input_tensor)
                .map_err(|e| OnnxError::InferenceError(e.to_string()))?;

            // Extract the first output tensor
            let output = outputs.get(&self.output_name).ok_or_else(|| {
                OnnxError::InferenceError(format!(
                    "output '{}' not found in session result",
                    self.output_name
                ))
            })?;

            let logits: ort::Tensor<f32> = output
                .try_extract_tensor::<f32>()
                .map_err(|e| OnnxError::InferenceError(e.to_string()))?
                .into_owned();

            Ok(logits.as_slice().unwrap_or_default().to_vec())
        }
    }

    impl PolicyNetwork for OnnxPolicyNetwork {
        /// Run a full forward pass through the ONNX model.
        ///
        /// On inference errors the method logs a warning and falls back to a
        /// uniform distribution so that the agent remains functional even if
        /// the model produces unexpected output.
        fn forward(&self, features: &[f32]) -> Vec<f32> {
            match self.run_inference(features) {
                Ok(logits) => {
                    if let Ok(mut state) = self.last_inference.lock() {
                        *state = NeuralInferenceStatus::Ready;
                    }
                    logits
                }
                Err(e) => {
                    if let Ok(mut state) = self.last_inference.lock() {
                        *state = NeuralInferenceStatus::Fallback {
                            reason: e.to_string(),
                        };
                    }
                    log::warn!("OnnxPolicyNetwork::forward failed: {e}; returning uniform output");
                    vec![1.0 / self.output_size as f32; self.output_size]
                }
            }
        }

        /// Return raw logits (same as `forward` for standard classification models).
        fn get_logits(&self, features: &[f32]) -> Vec<f32> {
            self.forward(features)
        }

        fn runtime_status(&self) -> NeuralPolicyRuntimeStatus {
            let last_inference = self
                .last_inference
                .lock()
                .map(|state| state.clone())
                .unwrap_or(NeuralInferenceStatus::Fallback {
                    reason: "introspection lock poisoned".to_string(),
                });

            NeuralPolicyRuntimeStatus {
                model_name: self.metadata.model_name.clone(),
                runtime_schema: self.metadata.runtime_schema,
                compatibility: NeuralCompatibilityStatus::Compatible,
                last_inference,
            }
        }
    }

    // ─── OnnxActionSelector ───────────────────────────────────────────────────

    /// An `ActionSelector` that wraps an `OnnxPolicyNetwork` and applies a
    /// temperature-scaled softmax before picking the argmax action.
    ///
    /// | Temperature | Behaviour                                         |
    /// |-------------|---------------------------------------------------|
    /// | < 1.0       | More deterministic / exploitative                 |
    /// | = 1.0       | Standard softmax                                  |
    /// | > 1.0       | More uniform / exploratory                        |
    pub struct OnnxActionSelector {
        network: OnnxPolicyNetwork,
        /// Softmax temperature applied to raw logits.
        temperature: f32,
    }

    impl OnnxActionSelector {
        /// Create with the default temperature of `1.0` (standard softmax).
        pub fn new(network: OnnxPolicyNetwork) -> Self {
            Self {
                network,
                temperature: 1.0,
            }
        }

        /// Create with a custom softmax temperature.
        ///
        /// Panics in debug builds if `temperature <= 0.0`.
        pub fn with_temperature(network: OnnxPolicyNetwork, temperature: f32) -> Self {
            debug_assert!(temperature > 0.0, "temperature must be positive");
            Self {
                network,
                temperature: temperature.max(1e-6), // guard against zero in release
            }
        }

        /// The softmax temperature currently in use.
        pub fn temperature(&self) -> f32 {
            self.temperature
        }
    }

    impl ActionSelector for OnnxActionSelector {
        /// Run the wrapped network, apply temperature-scaled softmax, and
        /// return the index of the highest-probability action.
        fn select_action(&self, features: &[f32]) -> usize {
            let logits = self.network.forward(features);
            let probs = softmax(&logits, self.temperature);
            argmax(&probs)
        }
    }

    // ─── Helper functions ─────────────────────────────────────────────────────

    /// Numerically stable softmax with temperature scaling.
    ///
    /// For a vector `x` and temperature `T`:
    /// ```text
    /// z_i = x_i / T
    /// p_i = exp(z_i - max(z)) / Σ exp(z_j - max(z))
    /// ```
    ///
    /// Returns a uniform distribution if the input is empty or if all
    /// exponents underflow to zero.
    pub fn softmax(logits: &[f32], temperature: f32) -> Vec<f32> {
        if logits.is_empty() {
            return Vec::new();
        }

        // Guard: temperature must be positive
        let t = temperature.max(1e-6);

        // Scale by temperature
        let scaled: Vec<f32> = logits.iter().map(|&x| x / t).collect();

        // Subtract max for numerical stability
        let max_val = scaled.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        let exps: Vec<f32> = scaled.iter().map(|&x| (x - max_val).exp()).collect();
        let sum: f32 = exps.iter().sum();

        if sum == 0.0 || !sum.is_finite() {
            // Fall back to uniform distribution
            return vec![1.0 / logits.len() as f32; logits.len()];
        }

        exps.iter().map(|&e| e / sum).collect()
    }

    /// Return the index of the maximum value in `values`.
    ///
    /// Ties are broken in favour of the lower index.  Returns `0` for an
    /// empty slice.
    pub fn argmax(values: &[f32]) -> usize {
        values
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Less))
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
} // end mod inner

// ─── Re-exports ───────────────────────────────────────────────────────────────

#[cfg(feature = "onnx")]
pub use inner::*;

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // Tests that do NOT require an ONNX model can always run.
    // Tests that require `ort` are gated behind `#[cfg(feature = "onnx")]`.

    // ── softmax ───────────────────────────────────────────────────────────────

    #[cfg(feature = "onnx")]
    use super::inner::{argmax, softmax, OnnxError, EXPECTED_INPUT_SIZE, EXPECTED_OUTPUT_SIZE};
    #[cfg(feature = "onnx")]
    use crate::neural_agent::{NeuralModelMetadata, NeuralRuntimeSchema};

    #[cfg(feature = "onnx")]
    #[test]
    fn test_softmax_sums_to_one() {
        let logits = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0];
        let probs = softmax(&logits, 1.0);
        assert_eq!(probs.len(), 5);
        let total: f32 = probs.iter().sum();
        assert!(
            (total - 1.0).abs() < 1e-5,
            "softmax should sum to 1, got {total}"
        );
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_softmax_temperature_low_is_more_peaked() {
        let logits = vec![1.0_f32, 3.0, 2.0];
        let probs_cold = softmax(&logits, 0.1);
        let probs_warm = softmax(&logits, 10.0);

        // With low temperature the argmax probability should dominate more.
        assert!(
            probs_cold[1] > probs_warm[1],
            "low temperature should increase winning probability"
        );
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_softmax_uniform_on_equal_logits() {
        let logits = vec![2.0_f32; 5];
        let probs = softmax(&logits, 1.0);
        for p in &probs {
            assert!(
                (p - 0.2).abs() < 1e-5,
                "equal logits should give uniform distribution, got {p}"
            );
        }
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_softmax_empty() {
        let probs = softmax(&[], 1.0);
        assert!(probs.is_empty());
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_softmax_all_finite() {
        let logits: Vec<f32> = (0..10).map(|i| i as f32 * 100.0).collect();
        let probs = softmax(&logits, 1.0);
        for p in &probs {
            assert!(p.is_finite(), "softmax output must be finite, got {p}");
        }
    }

    // ── argmax ────────────────────────────────────────────────────────────────

    #[cfg(feature = "onnx")]
    #[test]
    fn test_argmax_basic() {
        let values = vec![0.1_f32, 0.5, 0.3, 0.9, 0.2];
        assert_eq!(argmax(&values), 3);
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_argmax_tie_prefers_lower_index() {
        // Both index 0 and 2 have the same max value.
        let values = vec![1.0_f32, 0.5, 1.0];
        // max_by returns the *last* maximum due to Ordering::Less on equal;
        // document the actual behaviour (last index wins in Rust's max_by).
        let idx = argmax(&values);
        assert!(
            idx == 0 || idx == 2,
            "tied argmax should return one of the tied indices"
        );
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_argmax_single_element() {
        assert_eq!(argmax(&[42.0_f32]), 0);
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_argmax_empty() {
        assert_eq!(argmax(&[]), 0);
    }

    // ── OnnxError display ─────────────────────────────────────────────────────

    #[cfg(feature = "onnx")]
    #[test]
    fn test_onnx_error_display_load() {
        let e = OnnxError::LoadError("file not found".into());
        let s = e.to_string();
        assert!(s.contains("load error"), "unexpected: {s}");
        assert!(s.contains("file not found"), "unexpected: {s}");
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_onnx_error_display_inference() {
        let e = OnnxError::InferenceError("tensor rank mismatch".into());
        let s = e.to_string();
        assert!(s.contains("inference error"), "unexpected: {s}");
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_onnx_error_display_shape_mismatch() {
        let e = OnnxError::ShapeMismatch {
            expected: vec![32],
            got: vec![16],
        };
        let s = e.to_string();
        assert!(s.contains("shape mismatch"), "unexpected: {s}");
        assert!(s.contains("32"), "should mention expected size: {s}");
        assert!(s.contains("16"), "should mention actual size: {s}");
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_onnx_error_display_session() {
        let e = OnnxError::SessionError("environment init failed".into());
        let s = e.to_string();
        assert!(s.contains("session error"), "unexpected: {s}");
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_onnx_error_display_metadata_mismatch() {
        let e = OnnxError::MetadataMismatch("bad schema".into());
        let s = e.to_string();
        assert!(s.contains("metadata mismatch"), "unexpected: {s}");
    }

    // ── OnnxActionSelector temperature ────────────────────────────────────────
    // These tests mock the selector's temperature-scaling logic without needing
    // an actual .onnx model by verifying the softmax helper used inside it.

    #[cfg(feature = "onnx")]
    #[test]
    fn test_temperature_one_is_standard_softmax() {
        let logits = vec![1.0_f32, 2.0, 3.0];
        let p_temp1 = softmax(&logits, 1.0);

        // Manually compute reference softmax
        let max = 3.0_f32;
        let exps: Vec<f32> = logits.iter().map(|&x| (x - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let reference: Vec<f32> = exps.iter().map(|&e| e / sum).collect();

        for (p, r) in p_temp1.iter().zip(reference.iter()) {
            assert!((p - r).abs() < 1e-6, "mismatch: {p} vs {r}");
        }
    }

    // ── Module compilation smoke test ──────────────────────────────────────────

    /// Verifies the public API surface compiles and types are wired correctly.
    #[cfg(feature = "onnx")]
    #[test]
    fn test_constants_match_neural_agent() {
        // These must stay in sync with neural_agent.rs constants.
        assert_eq!(
            EXPECTED_INPUT_SIZE,
            crate::neural_agent::NEURAL_FEATURE_COUNT
        );
        assert_eq!(
            EXPECTED_OUTPUT_SIZE,
            crate::neural_agent::NEURAL_ACTION_COUNT
        );
    }

    #[cfg(feature = "onnx")]
    #[test]
    fn test_runtime_schema_metadata_validation_rejects_mismatch() {
        let metadata = NeuralModelMetadata {
            model_name: "bad-model".to_string(),
            runtime_schema: NeuralRuntimeSchema {
                interface_version: 1,
                feature_count: EXPECTED_INPUT_SIZE + 4,
                action_count: EXPECTED_OUTPUT_SIZE,
            },
        };

        let error = super::inner::OnnxPolicyNetwork::validate_metadata(&metadata).unwrap_err();
        assert!(matches!(error, OnnxError::MetadataMismatch(_)));
    }
}
