//! # Pod Scene — Scene Management, Asset Pipeline, and Prefab System
//!
//! Provides comprehensive scene management, asset loading, and prefab instantiation
//! for the Prompt or Die engine. Includes hot-reload support, scene serialization,
//! state machine management, and a complete save/load system.

#![allow(unused_variables)]
#![allow(clippy::len_zero)]
#![allow(clippy::new_without_default)]
#![allow(clippy::unwrap_or_default)]

pub mod asset;
pub mod binding;
pub mod prefab;
pub mod save;
pub mod scene;
pub mod state;

pub use asset::{AssetHandle, AssetLoader, AssetManager, AssetState, AssetStore};
pub use binding::{NativeComponent, NativeComponentBinding};
pub use prefab::{
    AppliedPropertyOverride, IgnoredPropertyOverride, Prefab, PrefabComponent, PrefabDiff,
    PrefabMetadataDiff, PrefabRegistry, PropertyOverride, PropertyOverrideReport,
    ResolvedPrefabComponents,
};
pub use save::{SaveData, SaveManager};
pub use scene::{
    EntityInstance, EntityReferenceBinding, EntityReferenceTarget, Scene, SceneGraph, SceneManager,
    SceneRegion, SceneSpawnResult, SceneStreamFocus, SceneStreamPlan, SpawnPoint, StreamingBounds,
};
pub use state::{GameState, StateStack, StateTransition};
