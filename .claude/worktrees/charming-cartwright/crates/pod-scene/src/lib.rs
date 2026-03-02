//! # Pod Scene — Scene Management, Asset Pipeline, and Prefab System
//!
//! Provides comprehensive scene management, asset loading, and prefab instantiation
//! for the Prompt or Die engine. Includes hot-reload support, scene serialization,
//! state machine management, and a complete save/load system.

pub mod scene;
pub mod prefab;
pub mod asset;
pub mod state;
pub mod save;

pub use scene::{Scene, SceneManager, SceneGraph};
pub use prefab::{Prefab, PrefabRegistry, PrefabComponent};
pub use asset::{AssetHandle, AssetStore, AssetManager, AssetLoader, AssetState};
pub use state::{GameState, StateStack, StateTransition};
pub use save::{SaveData, SaveManager};
