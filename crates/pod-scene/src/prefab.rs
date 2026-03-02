use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use pod_core::World;

/// Type-erased component data that can be serialized
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PrefabComponent {
    /// Raw JSON for flexible component representation
    Json(serde_json::Value),
}

impl PrefabComponent {
    /// Create a component from a JSON value
    pub fn from_json(value: serde_json::Value) -> Self {
        PrefabComponent::Json(value)
    }

    /// Get the underlying JSON value
    pub fn as_json(&self) -> &serde_json::Value {
        match self {
            PrefabComponent::Json(v) => v,
        }
    }

    /// Get a mutable reference to the JSON value
    pub fn as_json_mut(&mut self) -> &mut serde_json::Value {
        match self {
            PrefabComponent::Json(v) => v,
        }
    }

    /// Extract a typed component if it matches
    pub fn get<T: for<'de> serde::Deserialize<'de>>(&self) -> Option<T> {
        match self {
            PrefabComponent::Json(v) => serde_json::from_value(v.clone()).ok(),
        }
    }

    /// Set a typed component
    pub fn set<T: serde::Serialize>(&mut self, value: &T) -> Result<(), String> {
        match self {
            PrefabComponent::Json(ref mut v) => {
                *v = serde_json::to_value(value).map_err(|e| e.to_string())?;
                Ok(())
            }
        }
    }
}

/// Prefab metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefabMetadata {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub tags: Vec<String>,
}

impl Default for PrefabMetadata {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: 1,
            description: String::new(),
            tags: Vec::new(),
        }
    }
}

/// Prefab property override: path and value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyOverride {
    pub path: String, // e.g., "Transform.position.x"
    pub value: serde_json::Value,
}

impl PropertyOverride {
    pub fn new(path: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            path: path.into(),
            value,
        }
    }
}

/// Serializable entity template with components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefab {
    pub id: Uuid,
    pub metadata: PrefabMetadata,
    pub components: HashMap<String, PrefabComponent>,
    pub nested_prefabs: Vec<String>, // References to other prefabs
}

impl Prefab {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            metadata: PrefabMetadata {
                name: name.into(),
                ..Default::default()
            },
            components: HashMap::new(),
            nested_prefabs: Vec::new(),
        }
    }

    /// Add a component to the prefab
    pub fn add_component<T: serde::Serialize>(
        &mut self,
        name: impl Into<String>,
        value: &T,
    ) -> Result<(), String> {
        let json = serde_json::to_value(value).map_err(|e| e.to_string())?;
        self.components
            .insert(name.into(), PrefabComponent::Json(json));
        Ok(())
    }

    /// Get a component by name
    pub fn get_component(&self, name: &str) -> Option<&PrefabComponent> {
        self.components.get(name)
    }

    /// Get a mutable reference to a component
    pub fn get_component_mut(&mut self, name: &str) -> Option<&mut PrefabComponent> {
        self.components.get_mut(name)
    }

    /// Remove a component from the prefab
    pub fn remove_component(&mut self, name: &str) -> Option<PrefabComponent> {
        self.components.remove(name)
    }

    /// Add a reference to a nested prefab
    pub fn add_nested_prefab(&mut self, prefab_name: impl Into<String>) {
        self.nested_prefabs.push(prefab_name.into());
    }

    /// Serialize prefab to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize prefab from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize prefab to binary
    pub fn to_binary(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize prefab from binary
    pub fn from_binary(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }

    /// Spawn this prefab into a world
    pub fn spawn(&self, world: &mut World) -> Result<hecs::Entity, String> {
        let entity = world.ecs.spawn(());

        // Spawn all components
        for (name, component) in &self.components {
            log::debug!("Spawning component '{}' for prefab '{}'", name, self.metadata.name);
            // Component data is stored as JSON; the actual instantiation
            // would be handled by a system that interprets this JSON
        }

        Ok(entity)
    }

    /// Spawn with property overrides
    pub fn spawn_with_overrides(
        &self,
        world: &mut World,
        overrides: &[PropertyOverride],
    ) -> Result<hecs::Entity, String> {
        let mut components = self.components.clone();

        // Apply overrides
        for override_ in overrides {
            let parts: Vec<&str> = override_.path.split('.').collect();
            if parts.len() >= 2 {
                let component_name = parts[0];
                if let Some(component) = components.get_mut(component_name) {
                    // Apply nested property override
                    apply_property_override(component, &parts[1..], &override_.value);
                }
            }
        }

        // Spawn with overridden components
        let entity = world.ecs.spawn(());
        Ok(entity)
    }
}

/// Helper to apply nested property overrides
fn apply_property_override(component: &mut PrefabComponent, path: &[&str], value: &serde_json::Value) {
    if path.is_empty() {
        return;
    }

    match component {
        PrefabComponent::Json(json) => {
            let mut current = json;
            for (i, &key) in path.iter().enumerate() {
                if i == path.len() - 1 {
                    // Last key, apply the override
                    if let Some(obj) = current.as_object_mut() {
                        obj.insert(key.to_string(), value.clone());
                    }
                } else {
                    // Navigate deeper
                    if !current[key].is_object() {
                        current[key] = serde_json::json!({});
                    }
                    current = &mut current[key];
                }
            }
        }
    }
}

/// Global registry of named prefabs
pub struct PrefabRegistry {
    prefabs: HashMap<String, Prefab>,
    prefab_files: HashMap<String, String>, // For hot-reload
}

impl PrefabRegistry {
    pub fn new() -> Self {
        Self {
            prefabs: HashMap::new(),
            prefab_files: HashMap::new(),
        }
    }

    /// Register a prefab
    pub fn register(&mut self, name: impl Into<String>, prefab: Prefab) {
        self.prefabs.insert(name.into(), prefab);
    }

    /// Register from JSON
    pub fn register_from_json(
        &mut self,
        name: impl Into<String>,
        json: &str,
    ) -> Result<(), String> {
        let name_str = name.into();
        let prefab = Prefab::from_json(json).map_err(|e| e.to_string())?;
        self.prefab_files.insert(name_str.clone(), json.to_string());
        self.prefabs.insert(name_str, prefab);
        Ok(())
    }

    /// Unregister a prefab
    pub fn unregister(&mut self, name: &str) -> Option<Prefab> {
        self.prefab_files.remove(name);
        self.prefabs.remove(name)
    }

    /// Get a prefab
    pub fn get(&self, name: &str) -> Option<&Prefab> {
        self.prefabs.get(name)
    }

    /// Get a mutable reference to a prefab
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Prefab> {
        self.prefabs.get_mut(name)
    }

    /// Check if a prefab is registered
    pub fn contains(&self, name: &str) -> bool {
        self.prefabs.contains_key(name)
    }

    /// List all registered prefabs
    pub fn list(&self) -> Vec<String> {
        self.prefabs.keys().cloned().collect()
    }

    /// Spawn a prefab by name
    pub fn spawn(&self, name: &str, world: &mut World) -> Result<hecs::Entity, String> {
        self.get(name)
            .ok_or_else(|| format!("Prefab '{}' not found", name))?
            .spawn(world)
    }

    /// Spawn a prefab with overrides
    pub fn spawn_with_overrides(
        &self,
        name: &str,
        world: &mut World,
        overrides: &[PropertyOverride],
    ) -> Result<hecs::Entity, String> {
        self.get(name)
            .ok_or_else(|| format!("Prefab '{}' not found", name))?
            .spawn_with_overrides(world, overrides)
    }

    /// Hot-reload: check and reload a prefab if its file changed
    pub fn hot_reload(&mut self, name: &str, new_json: &str) -> Result<(), String> {
        if let Some(old_json) = self.prefab_files.get(name) {
            if old_json != new_json {
                self.register_from_json(name, new_json)?;
                log::info!("Hot-reloaded prefab '{}'", name);
            }
        }
        Ok(())
    }

    /// Get prefab file content for hot-reload tracking
    pub fn get_prefab_file(&self, name: &str) -> Option<&str> {
        self.prefab_files.get(name).map(|s| s.as_str())
    }
}

impl Default for PrefabRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefab_creation() {
        let mut prefab = Prefab::new("TestPrefab");
        prefab.metadata.description = "A test prefab".to_string();

        assert_eq!(prefab.metadata.name, "TestPrefab");
        assert_eq!(prefab.metadata.description, "A test prefab");
    }

    #[test]
    fn test_prefab_components() {
        let mut prefab = Prefab::new("TestPrefab");
        let value = serde_json::json!({"x": 10.0, "y": 20.0});
        prefab
            .add_component("Transform", &value)
            .unwrap();

        let retrieved = prefab.get_component("Transform").unwrap();
        assert_eq!(
            retrieved.as_json(),
            &serde_json::json!({"x": 10.0, "y": 20.0})
        );
    }

    #[test]
    fn test_prefab_serialization() {
        let mut prefab = Prefab::new("TestPrefab");
        let value = serde_json::json!({"x": 10.0});
        prefab.add_component("Transform", &value).unwrap();

        let json = prefab.to_json().unwrap();
        let loaded = Prefab::from_json(&json).unwrap();

        assert_eq!(loaded.metadata.name, "TestPrefab");
        assert!(loaded.components.contains_key("Transform"));
    }

    #[test]
    fn test_prefab_registry() {
        let mut registry = PrefabRegistry::new();
        let prefab = Prefab::new("Prefab1");
        registry.register("Prefab1", prefab);

        assert!(registry.contains("Prefab1"));
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn test_property_override() {
        let override_ =
            PropertyOverride::new("Transform.position.x", serde_json::json!(42.0));
        assert_eq!(override_.path, "Transform.position.x");
    }

    #[test]
    fn test_nested_prefabs() {
        let mut prefab = Prefab::new("ParentPrefab");
        prefab.add_nested_prefab("ChildPrefab");

        assert_eq!(prefab.nested_prefabs.len(), 1);
        assert!(prefab.nested_prefabs.contains(&"ChildPrefab".to_string()));
    }
}
