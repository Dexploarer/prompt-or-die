use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Asset loading state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetState {
    Unloaded,
    Loading,
    Loaded,
    Failed,
}

/// Reference-counted handle with generation tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetHandle<T> {
    id: Uuid,
    generation: u32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> AssetHandle<T> {
    /// Create a new asset handle
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            generation: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the handle's unique ID
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the generation number
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Increment the generation (used when reloading assets)
    pub fn next_generation(&mut self) {
        self.generation = self.generation.saturating_add(1);
    }

    /// Create a handle with a specific ID
    pub fn with_id(id: Uuid) -> Self {
        Self {
            id,
            generation: 0,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Default for AssetHandle<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PartialEq for AssetHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.generation == other.generation
    }
}

impl<T> Eq for AssetHandle<T> {}

impl<T> std::hash::Hash for AssetHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.generation.hash(state);
    }
}

/// Represents a loaded asset with metadata
#[derive(Debug, Clone)]
struct AssetEntry<T> {
    data: Arc<T>,
    state: AssetState,
    last_modified: u64, // For hot-reload tracking
}

/// Typed storage for loaded assets
pub struct AssetStore<T> {
    assets: HashMap<Uuid, AssetEntry<T>>,
}

impl<T> AssetStore<T> {
    pub fn new() -> Self {
        Self {
            assets: HashMap::new(),
        }
    }

    /// Store an asset
    pub fn store(&mut self, handle: &AssetHandle<T>, asset: T) {
        self.assets.insert(
            handle.id(),
            AssetEntry {
                data: Arc::new(asset),
                state: AssetState::Loaded,
                last_modified: Self::current_timestamp(),
            },
        );
    }

    /// Retrieve an asset
    pub fn get(&self, handle: &AssetHandle<T>) -> Option<Arc<T>> {
        self.assets.get(&handle.id()).map(|entry| Arc::clone(&entry.data))
    }

    /// Get the state of an asset
    pub fn get_state(&self, handle: &AssetHandle<T>) -> Option<AssetState> {
        self.assets.get(&handle.id()).map(|entry| entry.state)
    }

    /// Update the state of an asset
    pub fn set_state(&mut self, handle: &AssetHandle<T>, state: AssetState) {
        if let Some(entry) = self.assets.get_mut(&handle.id()) {
            entry.state = state;
        }
    }

    /// Remove an asset
    pub fn remove(&mut self, handle: &AssetHandle<T>) -> Option<Arc<T>> {
        self.assets.remove(&handle.id()).map(|entry| entry.data)
    }

    /// Check if an asset is loaded
    pub fn is_loaded(&self, handle: &AssetHandle<T>) -> bool {
        self.assets
            .get(&handle.id())
            .map(|e| e.state == AssetState::Loaded)
            .unwrap_or(false)
    }

    /// Get the number of stored assets
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Clear all assets
    pub fn clear(&mut self) {
        self.assets.clear();
    }

    /// Update modified timestamp
    pub fn update_timestamp(&mut self, handle: &AssetHandle<T>) {
        if let Some(entry) = self.assets.get_mut(&handle.id()) {
            entry.last_modified = Self::current_timestamp();
        }
    }

    /// Get last modified timestamp
    pub fn get_timestamp(&self, handle: &AssetHandle<T>) -> Option<u64> {
        self.assets.get(&handle.id()).map(|e| e.last_modified)
    }

    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl<T> Default for AssetStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for pluggable asset loaders
pub trait AssetLoader<T>: Send + Sync {
    fn load(&self, path: &str) -> Result<T, String>;
    fn reload(&self, path: &str, existing: T) -> Result<T, String> {
        self.load(path)
    }
}

/// Default JSON loader for generic assets
pub struct JsonAssetLoader;

impl AssetLoader<serde_json::Value> for JsonAssetLoader {
    fn load(&self, path: &str) -> Result<serde_json::Value, String> {
        std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file {}: {}", path, e))
            .and_then(|content| {
                serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse JSON from {}: {}", path, e))
            })
    }
}

/// Coordinates multi-type asset loading and management
pub struct AssetManager {
    json_loader: Arc<JsonAssetLoader>,
    loading_queue: Vec<(String, Uuid)>, // (path, handle_id)
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            json_loader: Arc::new(JsonAssetLoader),
            loading_queue: Vec::new(),
        }
    }

    /// Queue an asset for loading
    pub fn queue_load(&mut self, path: impl Into<String>, handle_id: Uuid) {
        self.loading_queue.push((path.into(), handle_id));
    }

    /// Get the loading queue
    pub fn get_loading_queue(&self) -> &[(String, Uuid)] {
        &self.loading_queue
    }

    /// Clear the loading queue
    pub fn clear_queue(&mut self) {
        self.loading_queue.clear();
    }

    /// Load a JSON asset synchronously
    pub fn load_json(&self, path: &str) -> Result<serde_json::Value, String> {
        self.json_loader.load(path)
    }

    /// Load a JSON asset from a string
    pub fn load_json_string(&self, json: &str) -> Result<serde_json::Value, String> {
        serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse JSON: {}", e))
    }

    /// Get the JSON loader
    pub fn get_json_loader(&self) -> Arc<JsonAssetLoader> {
        Arc::clone(&self.json_loader)
    }
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in loaders for common asset types
pub mod loaders {
    use super::*;

    /// Loader for scene files
    pub struct SceneLoader;

    impl AssetLoader<serde_json::Value> for SceneLoader {
        fn load(&self, path: &str) -> Result<serde_json::Value, String> {
            std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read scene file {}: {}", path, e))
                .and_then(|content| {
                    serde_json::from_str(&content)
                        .map_err(|e| format!("Failed to parse scene from {}: {}", path, e))
                })
        }
    }

    /// Loader for prefab files
    pub struct PrefabLoader;

    impl AssetLoader<serde_json::Value> for PrefabLoader {
        fn load(&self, path: &str) -> Result<serde_json::Value, String> {
            std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read prefab file {}: {}", path, e))
                .and_then(|content| {
                    serde_json::from_str(&content)
                        .map_err(|e| format!("Failed to parse prefab from {}: {}", path, e))
                })
        }
    }

    /// Loader for sprite sheets (JSON format)
    pub struct SpriteSheetLoader;

    impl AssetLoader<serde_json::Value> for SpriteSheetLoader {
        fn load(&self, path: &str) -> Result<serde_json::Value, String> {
            std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read sprite sheet {}: {}", path, e))
                .and_then(|content| {
                    serde_json::from_str(&content)
                        .map_err(|e| format!("Failed to parse sprite sheet from {}: {}", path, e))
                })
        }
    }

    /// Loader for raw text assets
    pub struct TextLoader;

    impl AssetLoader<String> for TextLoader {
        fn load(&self, path: &str) -> Result<String, String> {
            std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read text file {}: {}", path, e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_handle_creation() {
        let handle: AssetHandle<String> = AssetHandle::new();
        assert_eq!(handle.generation(), 0);
    }

    #[test]
    fn test_asset_handle_generation() {
        let mut handle: AssetHandle<String> = AssetHandle::new();
        handle.next_generation();
        assert_eq!(handle.generation(), 1);
        handle.next_generation();
        assert_eq!(handle.generation(), 2);
    }

    #[test]
    fn test_asset_store() {
        let mut store: AssetStore<String> = AssetStore::new();
        let handle: AssetHandle<String> = AssetHandle::new();

        store.store(&handle, "test_asset".to_string());

        assert!(store.is_loaded(&handle));
        let asset = store.get(&handle).unwrap();
        assert_eq!(asset.as_ref(), "test_asset");
    }

    #[test]
    fn test_asset_state_management() {
        let mut store: AssetStore<String> = AssetStore::new();
        let handle: AssetHandle<String> = AssetHandle::new();

        store.store(&handle, "asset".to_string());
        assert_eq!(store.get_state(&handle), Some(AssetState::Loaded));

        store.set_state(&handle, AssetState::Failed);
        assert_eq!(store.get_state(&handle), Some(AssetState::Failed));
    }

    #[test]
    fn test_asset_removal() {
        let mut store: AssetStore<String> = AssetStore::new();
        let handle: AssetHandle<String> = AssetHandle::new();

        store.store(&handle, "asset".to_string());
        assert_eq!(store.len(), 1);

        let removed = store.remove(&handle);
        assert!(removed.is_some());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_asset_manager_queue() {
        let mut manager = AssetManager::new();
        let id = Uuid::new_v4();

        manager.queue_load("path/to/asset.json", id);
        assert_eq!(manager.get_loading_queue().len(), 1);

        manager.clear_queue();
        assert_eq!(manager.get_loading_queue().len(), 0);
    }

    #[test]
    fn test_json_loader() {
        let loader = JsonAssetLoader;
        let result = loader.load("/nonexistent/file.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_asset_handle_equality() {
        let id = Uuid::new_v4();
        let h1: AssetHandle<String> = AssetHandle::with_id(id);
        let h2: AssetHandle<String> = AssetHandle::with_id(id);

        assert_eq!(h1, h2);
    }
}
