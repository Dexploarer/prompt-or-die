//! # pod-stdb — SpacetimeDB 2.0 Integration for Prompt or Die
//!
//! This crate serves as both a SpacetimeDB module (compiled to WASM) and
//! a Rust library for the workspace. It defines:
//!
//! - **Tables**: All game state as relational tables (mirrors ECS components)
//! - **Reducers**: Atomic game logic (tick pipeline, actions, entity management)
//! - **Event Tables**: Transient per-tick events (observations, combat, speech)
//!
//! The core invariant: every agent type (Human, LLM, Neural, Scripted, System)
//! goes through the SAME pipeline with IDENTICAL constraints.
//!
//! ## Feature Flags
//!
//! - **`module`**: Enables the SpacetimeDB WASM module code — tables,
//!   reducers, events, observation. This links against WASM host imports and
//!   is intended for wasm32 module builds. Module tests use `spacetime test`.
//!
//! - **`client`**: Enables the native Rust client wrapper for connecting to a
//!   running SpacetimeDB instance. Depends only on `types` (no WASM imports)
//!   and is fully testable on native targets.
//!
//! - **`unstable`**: Enables SpacetimeDB unstable features (RLS filters).
//!
//! ## Building & Testing
//!
//! ```bash
//! # Native default build (client wrapper only):
//! cargo check -p pod-stdb
//!
//! # WASM module build:
//! cargo check -p pod-stdb --no-default-features --features module
//!
//! # Run client tests (no WASM symbols):
//! cargo test -p pod-stdb --no-default-features --features client
//!
//! # Module tests (requires SpacetimeDB runtime):
//! spacetime test
//! ```

// ============================================================
// SHARED TYPES — available on all targets and feature combos
// ============================================================

/// Shared type definitions (AgentType, ActionKind, SpeakVolume, etc.).
///
/// When the `module` feature is enabled, these types derive `SpacetimeType`
/// for BSATN serialization. Without `module`, they are plain Rust enums
/// usable in client code, tests, and other native contexts.
pub mod types;

// ============================================================
// WASM MODULE CODE — tables, reducers, events, observation
// ============================================================
//
// These modules use `#[spacetimedb::table]`, `#[spacetimedb::reducer]`, and
// `spacetimedb::Table` which link against WASM host imports (_datastore_insert_bsatn,
// _table_id_from_name, etc.). They are only compiled when the `module` feature
// is active (which is the default).
//
// To build/test without WASM symbols: --no-default-features --features client

#[cfg(all(feature = "module", target_arch = "wasm32"))]
pub mod events;
#[cfg(all(feature = "module", target_arch = "wasm32"))]
pub mod observation;
#[cfg(all(feature = "module", target_arch = "wasm32"))]
pub mod reducers;
#[cfg(all(feature = "module", target_arch = "wasm32"))]
pub mod tables;

#[cfg(all(feature = "module", target_arch = "wasm32"))]
mod module_entropy {
    use core::sync::atomic::{AtomicU32, Ordering};
    use getrandom::{register_custom_getrandom, Error};

    static MODULE_RANDOM_STATE: AtomicU32 = AtomicU32::new(0xA341_316C);

    fn fill_module_random(dest: &mut [u8]) -> Result<(), Error> {
        let mut state = MODULE_RANDOM_STATE
            .fetch_add(0x9E37_79B9, Ordering::Relaxed)
            .wrapping_add(dest.len() as u32);

        for chunk in dest.chunks_mut(4) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;

            let bytes = state.to_le_bytes();
            for (slot, byte) in chunk.iter_mut().zip(bytes.iter()) {
                *slot = *byte;
            }
        }

        Ok(())
    }

    // Gameplay randomness remains world-seeded and deterministic. This backend
    // only satisfies transitive crates that require `getrandom` in the wasm
    // reducer module path.
    register_custom_getrandom!(fill_module_random);
}

// ============================================================
// NATIVE CLIENT WRAPPER
// ============================================================

/// Client wrapper for connecting to the SpacetimeDB module from native Rust applications.
/// Requires the `client` feature — excluded from the WASM module build (cdylib).
///
/// This module depends only on `types` (no WASM host imports) and is fully
/// testable on native targets via:
/// ```bash
/// cargo test -p pod-stdb --no-default-features --features client
/// ```
#[cfg(feature = "client")]
pub mod client;
