use pod_core::World;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::binding::{insert_bound_components, NativeComponentBinding};

/// Type-erased component data that can be serialized
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    pub fn from_native<T: NativeComponentBinding>(value: &T) -> Result<Self, String> {
        Ok(Self::Json(value.to_component_value()?))
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

    pub fn get_native<T: NativeComponentBinding>(&self) -> Result<T, String> {
        T::from_component_value(self.as_json())
    }
}

/// Prefab metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefabMetadata {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrefabMetadataDiff {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

impl PrefabMetadataDiff {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.version.is_none()
            && self.description.is_none()
            && self.tags.is_none()
    }

    fn apply_to(&self, metadata: &mut PrefabMetadata) {
        if let Some(name) = &self.name {
            metadata.name = name.clone();
        }
        if let Some(version) = self.version {
            metadata.version = version;
        }
        if let Some(description) = &self.description {
            metadata.description = description.clone();
        }
        if let Some(tags) = &self.tags {
            metadata.tags = tags.clone();
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrefabDiff {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PrefabMetadataDiff>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_prefab: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub added_components: HashMap<String, PrefabComponent>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub changed_components: HashMap<String, PrefabComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_components: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_nested_prefabs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_nested_prefabs: Vec<String>,
}

impl PrefabDiff {
    pub fn is_empty(&self) -> bool {
        self.metadata
            .as_ref()
            .map(PrefabMetadataDiff::is_empty)
            .unwrap_or(true)
            && self.base_prefab.is_none()
            && self.added_components.is_empty()
            && self.changed_components.is_empty()
            && self.removed_components.is_empty()
            && self.added_nested_prefabs.is_empty()
            && self.removed_nested_prefabs.is_empty()
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedPropertyOverride {
    pub path: String,
    pub previous_value: Option<serde_json::Value>,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredPropertyOverride {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PropertyOverrideReport {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<AppliedPropertyOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored: Vec<IgnoredPropertyOverride>,
}

impl PropertyOverrideReport {
    pub fn is_empty(&self) -> bool {
        self.applied.is_empty() && self.ignored.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPrefabComponents {
    pub components: HashMap<String, PrefabComponent>,
    pub override_report: PropertyOverrideReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ComponentProvenanceLayer {
    PrefabDefinition {
        prefab: String,
    },
    PropertyOverride {
        path: String,
    },
    SceneComponent {
        entity_id: Uuid,
        entity_name: String,
    },
    EntityReference {
        path: String,
        target: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentProvenance {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<ComponentProvenanceLayer>,
}

impl ComponentProvenance {
    pub fn from_layer(layer: ComponentProvenanceLayer) -> Self {
        Self {
            layers: vec![layer],
        }
    }

    pub fn push(&mut self, layer: ComponentProvenanceLayer) {
        self.layers.push(layer);
    }

    pub fn current(&self) -> Option<&ComponentProvenanceLayer> {
        self.layers.last()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPrefabComponentsWithProvenance {
    pub components: HashMap<String, PrefabComponent>,
    pub override_report: PropertyOverrideReport,
    pub component_provenance: HashMap<String, ComponentProvenance>,
}

impl From<ResolvedPrefabComponentsWithProvenance> for ResolvedPrefabComponents {
    fn from(value: ResolvedPrefabComponentsWithProvenance) -> Self {
        Self {
            components: value.components,
            override_report: value.override_report,
        }
    }
}

/// Serializable entity template with components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefab {
    pub id: Uuid,
    pub metadata: PrefabMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_prefab: Option<String>,
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
            base_prefab: None,
            components: HashMap::new(),
            nested_prefabs: Vec::new(),
        }
    }

    pub fn with_base_prefab(mut self, prefab_name: impl Into<String>) -> Self {
        self.base_prefab = Some(prefab_name.into());
        self
    }

    pub fn set_base_prefab(&mut self, prefab_name: impl Into<String>) {
        self.base_prefab = Some(prefab_name.into());
    }

    pub fn clear_base_prefab(&mut self) {
        self.base_prefab = None;
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

    pub fn add_native_component<T: NativeComponentBinding>(
        &mut self,
        value: &T,
    ) -> Result<(), String> {
        self.components.insert(
            T::COMPONENT_NAME.to_string(),
            PrefabComponent::from_native(value)?,
        );
        Ok(())
    }

    /// Get a component by name
    pub fn get_component(&self, name: &str) -> Option<&PrefabComponent> {
        self.components.get(name)
    }

    pub fn get_native_component<T: NativeComponentBinding>(&self) -> Result<Option<T>, String> {
        self.components
            .get(T::COMPONENT_NAME)
            .map(PrefabComponent::get_native::<T>)
            .transpose()
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

    pub fn resolved_components(
        &self,
        overrides: &[PropertyOverride],
    ) -> HashMap<String, PrefabComponent> {
        self.resolved_components_with_report(overrides).components
    }

    pub fn resolved_components_with_report(
        &self,
        overrides: &[PropertyOverride],
    ) -> ResolvedPrefabComponents {
        self.resolved_components_with_provenance(overrides).into()
    }

    pub fn resolved_components_with_provenance(
        &self,
        overrides: &[PropertyOverride],
    ) -> ResolvedPrefabComponentsWithProvenance {
        let mut components = self.components.clone();
        let mut component_provenance =
            build_prefab_definition_provenance(self.components.keys(), &self.metadata.name);
        let override_report =
            apply_property_overrides(&mut components, overrides, &mut component_provenance);

        ResolvedPrefabComponentsWithProvenance {
            components,
            override_report,
            component_provenance,
        }
    }

    pub fn diff_against(&self, base: &Prefab) -> PrefabDiff {
        let metadata = diff_metadata(&self.metadata, &base.metadata);

        let mut added_components = HashMap::new();
        let mut changed_components = HashMap::new();
        let mut removed_components = Vec::new();

        for (name, component) in &self.components {
            match base.components.get(name) {
                Some(base_component) if base_component.as_json() == component.as_json() => {}
                Some(_) => {
                    changed_components.insert(name.clone(), component.clone());
                }
                None => {
                    added_components.insert(name.clone(), component.clone());
                }
            }
        }

        for component_name in base.components.keys() {
            if !self.components.contains_key(component_name) {
                removed_components.push(component_name.clone());
            }
        }

        let base_nested: HashSet<&str> = base.nested_prefabs.iter().map(String::as_str).collect();
        let self_nested: HashSet<&str> = self.nested_prefabs.iter().map(String::as_str).collect();

        let mut added_nested_prefabs: Vec<String> = self
            .nested_prefabs
            .iter()
            .filter(|name| !base_nested.contains(name.as_str()))
            .cloned()
            .collect();
        let mut removed_nested_prefabs: Vec<String> = base
            .nested_prefabs
            .iter()
            .filter(|name| !self_nested.contains(name.as_str()))
            .cloned()
            .collect();

        added_nested_prefabs.sort();
        removed_nested_prefabs.sort();
        removed_components.sort();

        PrefabDiff {
            metadata,
            base_prefab: if self.base_prefab != base.base_prefab {
                Some(self.base_prefab.clone())
            } else {
                None
            },
            added_components,
            changed_components,
            removed_components,
            added_nested_prefabs,
            removed_nested_prefabs,
        }
    }

    pub fn apply_diff(&mut self, diff: &PrefabDiff) {
        if let Some(metadata) = &diff.metadata {
            metadata.apply_to(&mut self.metadata);
        }
        if let Some(base_prefab) = &diff.base_prefab {
            self.base_prefab = base_prefab.clone();
        }

        for component_name in &diff.removed_components {
            self.components.remove(component_name);
        }
        for (component_name, component) in &diff.added_components {
            self.components
                .insert(component_name.clone(), component.clone());
        }
        for (component_name, component) in &diff.changed_components {
            self.components
                .insert(component_name.clone(), component.clone());
        }

        let mut nested_prefabs: Vec<String> = self
            .nested_prefabs
            .iter()
            .filter(|name| !diff.removed_nested_prefabs.contains(name))
            .cloned()
            .collect();
        for nested in &diff.added_nested_prefabs {
            if !nested_prefabs.contains(nested) {
                nested_prefabs.push(nested.clone());
            }
        }
        self.nested_prefabs = nested_prefabs;
    }

    /// Spawn this prefab into a world
    pub fn spawn(&self, world: &mut World) -> Result<hecs::Entity, String> {
        let entity = world.ecs.spawn(());
        let ignored = insert_bound_components(&self.components, &mut world.ecs, entity)?;
        for component_name in ignored {
            log::debug!(
                "Ignoring non-native component '{}' while spawning prefab '{}'",
                component_name,
                self.metadata.name
            );
        }
        Ok(entity)
    }

    /// Spawn with property overrides
    pub fn spawn_with_overrides(
        &self,
        world: &mut World,
        overrides: &[PropertyOverride],
    ) -> Result<hecs::Entity, String> {
        let components = self
            .resolved_components_with_provenance(overrides)
            .components;
        let entity = world.ecs.spawn(());
        let ignored = insert_bound_components(&components, &mut world.ecs, entity)?;
        for component_name in ignored {
            log::debug!(
                "Ignoring non-native component '{}' while spawning prefab '{}'",
                component_name,
                self.metadata.name
            );
        }
        Ok(entity)
    }
}

pub(crate) fn set_component_path_value(
    component: &mut PrefabComponent,
    path: &[&str],
    value: &serde_json::Value,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("component path cannot be empty".to_string());
    }

    match component {
        PrefabComponent::Json(json) => set_json_path_value(json, path, value),
    }
}

fn set_json_path_value(
    current: &mut serde_json::Value,
    path: &[&str],
    value: &serde_json::Value,
) -> Result<(), String> {
    if path.is_empty() {
        *current = value.clone();
        return Ok(());
    }

    let key = path[0];
    if path.len() == 1 {
        return set_json_child(current, key, value.clone());
    }

    let next_key = path[1];
    let child = get_or_create_json_child(current, key, next_key)?;
    set_json_path_value(child, &path[1..], value)
}

fn get_component_path_value<'a>(
    component: &'a PrefabComponent,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    match component {
        PrefabComponent::Json(json) => get_json_path_value(json, path),
    }
}

fn get_json_path_value<'a>(
    current: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    if path.is_empty() {
        return Some(current);
    }

    let key = path[0];
    let next = match current {
        serde_json::Value::Object(object) => object.get(key)?,
        serde_json::Value::Array(array) => {
            let index = key_to_index(key)?;
            array.get(index)?
        }
        _ => return None,
    };

    get_json_path_value(next, &path[1..])
}

fn set_json_child(
    target: &mut serde_json::Value,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    if let Some(object) = target.as_object_mut() {
        object.insert(key.to_string(), value);
        return Ok(());
    }

    if let Some(array) = target.as_array_mut() {
        if let Some(index) = key_to_index(key) {
            ensure_array_len(array, index + 1);
            array[index] = value;
            return Ok(());
        }
    }

    Err(format!(
        "cannot assign '{}' on JSON value that is neither an object field nor an addressable array slot",
        key
    ))
}

fn get_or_create_json_child<'a>(
    target: &'a mut serde_json::Value,
    key: &str,
    next_key: &str,
) -> Result<&'a mut serde_json::Value, String> {
    match target {
        serde_json::Value::Object(object) => {
            let entry = object
                .entry(key.to_string())
                .or_insert_with(|| empty_container_for(next_key));
            if entry.is_null() {
                *entry = empty_container_for(next_key);
            } else if !matches_container(entry, next_key) {
                return Err(format!(
                    "cannot descend through '{}' because the existing value has incompatible shape",
                    key
                ));
            }
            Ok(entry)
        }
        serde_json::Value::Array(array) => {
            let index = key_to_index(key)
                .ok_or_else(|| format!("array path segment '{}' is not addressable", key))?;
            ensure_array_len(array, index + 1);
            if array[index].is_null() {
                array[index] = empty_container_for(next_key);
            } else if !matches_container(&array[index], next_key) {
                return Err(format!(
                    "cannot descend through '{}' because the existing array element has incompatible shape",
                    key
                ));
            }
            array
                .get_mut(index)
                .ok_or_else(|| format!("array path segment '{}' is out of bounds", key))
        }
        _ => Err(format!(
            "cannot descend through '{}' on non-container JSON value",
            key
        )),
    }
}

fn key_to_index(key: &str) -> Option<usize> {
    match key {
        "x" | "r" => Some(0),
        "y" | "g" => Some(1),
        "z" | "b" => Some(2),
        "w" | "a" => Some(3),
        _ => key.parse::<usize>().ok(),
    }
}

fn empty_container_for(next_key: &str) -> serde_json::Value {
    if key_to_index(next_key).is_some() {
        serde_json::Value::Array(Vec::new())
    } else {
        serde_json::json!({})
    }
}

fn matches_container(value: &serde_json::Value, next_key: &str) -> bool {
    if key_to_index(next_key).is_some() {
        value.is_array()
    } else {
        value.is_object()
    }
}

fn ensure_array_len(array: &mut Vec<serde_json::Value>, len: usize) {
    while array.len() < len {
        array.push(serde_json::Value::Null);
    }
}

fn build_prefab_definition_provenance<'a>(
    component_names: impl Iterator<Item = &'a String>,
    prefab_name: &str,
) -> HashMap<String, ComponentProvenance> {
    component_names
        .map(|component_name| {
            (
                component_name.clone(),
                ComponentProvenance::from_layer(ComponentProvenanceLayer::PrefabDefinition {
                    prefab: prefab_name.to_string(),
                }),
            )
        })
        .collect()
}

fn apply_property_overrides(
    components: &mut HashMap<String, PrefabComponent>,
    overrides: &[PropertyOverride],
    component_provenance: &mut HashMap<String, ComponentProvenance>,
) -> PropertyOverrideReport {
    let mut override_report = PropertyOverrideReport::default();

    for override_ in overrides {
        let parts: Vec<&str> = override_.path.split('.').collect();
        if parts.len() < 2 {
            override_report.ignored.push(IgnoredPropertyOverride {
                path: override_.path.clone(),
                reason: "override path must include a component name and property path".to_string(),
            });
            continue;
        }

        let component_name = parts[0];
        if let Some(component) = components.get_mut(component_name) {
            let previous_value = get_component_path_value(component, &parts[1..]).cloned();
            match set_component_path_value(component, &parts[1..], &override_.value) {
                Ok(()) => {
                    override_report.applied.push(AppliedPropertyOverride {
                        path: override_.path.clone(),
                        previous_value,
                        value: override_.value.clone(),
                    });
                    component_provenance
                        .entry(component_name.to_string())
                        .or_default()
                        .push(ComponentProvenanceLayer::PropertyOverride {
                            path: override_.path.clone(),
                        });
                }
                Err(reason) => override_report.ignored.push(IgnoredPropertyOverride {
                    path: override_.path.clone(),
                    reason,
                }),
            }
        } else {
            override_report.ignored.push(IgnoredPropertyOverride {
                path: override_.path.clone(),
                reason: format!("component '{}' is not present on prefab", component_name),
            });
        }
    }

    override_report
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

    pub fn resolve_prefab(&self, name: &str) -> Result<Prefab, String> {
        let mut visiting = Vec::new();
        self.resolve_prefab_internal(name, &mut visiting)
    }

    pub fn resolve_components_with_provenance(
        &self,
        name: &str,
        overrides: &[PropertyOverride],
    ) -> Result<ResolvedPrefabComponentsWithProvenance, String> {
        let mut visiting = Vec::new();
        let (prefab, mut component_provenance) =
            self.resolve_prefab_internal_with_provenance(name, &mut visiting)?;
        let mut components = prefab.components.clone();
        let override_report =
            apply_property_overrides(&mut components, overrides, &mut component_provenance);

        Ok(ResolvedPrefabComponentsWithProvenance {
            components,
            override_report,
            component_provenance,
        })
    }

    fn resolve_prefab_internal(
        &self,
        name: &str,
        visiting: &mut Vec<String>,
    ) -> Result<Prefab, String> {
        if visiting.iter().any(|current| current == name) {
            visiting.push(name.to_string());
            return Err(format!(
                "Prefab inheritance cycle detected: {}",
                visiting.join(" -> ")
            ));
        }

        let prefab = self
            .prefabs
            .get(name)
            .ok_or_else(|| format!("Prefab '{}' not found", name))?;

        visiting.push(name.to_string());

        let mut resolved = if let Some(base_name) = &prefab.base_prefab {
            let mut base_prefab = self.resolve_prefab_internal(base_name, visiting)?;
            base_prefab.metadata = prefab.metadata.clone();
            base_prefab.base_prefab = prefab.base_prefab.clone();

            for (component_name, component) in &prefab.components {
                base_prefab
                    .components
                    .insert(component_name.clone(), component.clone());
            }
            for nested in &prefab.nested_prefabs {
                if !base_prefab.nested_prefabs.contains(nested) {
                    base_prefab.nested_prefabs.push(nested.clone());
                }
            }
            base_prefab
        } else {
            prefab.clone()
        };

        visiting.pop();

        resolved.metadata = prefab.metadata.clone();
        resolved.base_prefab = prefab.base_prefab.clone();
        resolved.id = prefab.id;
        Ok(resolved)
    }

    fn resolve_prefab_internal_with_provenance(
        &self,
        name: &str,
        visiting: &mut Vec<String>,
    ) -> Result<(Prefab, HashMap<String, ComponentProvenance>), String> {
        if visiting.iter().any(|current| current == name) {
            visiting.push(name.to_string());
            return Err(format!(
                "Prefab inheritance cycle detected: {}",
                visiting.join(" -> ")
            ));
        }

        let prefab = self
            .prefabs
            .get(name)
            .ok_or_else(|| format!("Prefab '{}' not found", name))?;

        visiting.push(name.to_string());

        let (mut resolved, component_provenance) = if let Some(base_name) = &prefab.base_prefab {
            let (mut base_prefab, mut base_provenance) =
                self.resolve_prefab_internal_with_provenance(base_name, visiting)?;
            base_prefab.metadata = prefab.metadata.clone();
            base_prefab.base_prefab = prefab.base_prefab.clone();

            for (component_name, component) in &prefab.components {
                base_prefab
                    .components
                    .insert(component_name.clone(), component.clone());
                base_provenance
                    .entry(component_name.clone())
                    .or_default()
                    .push(ComponentProvenanceLayer::PrefabDefinition {
                        prefab: name.to_string(),
                    });
            }
            for nested in &prefab.nested_prefabs {
                if !base_prefab.nested_prefabs.contains(nested) {
                    base_prefab.nested_prefabs.push(nested.clone());
                }
            }

            (base_prefab, base_provenance)
        } else {
            (
                prefab.clone(),
                build_prefab_definition_provenance(prefab.components.keys(), name),
            )
        };

        visiting.pop();

        resolved.metadata = prefab.metadata.clone();
        resolved.base_prefab = prefab.base_prefab.clone();
        resolved.id = prefab.id;
        Ok((resolved, component_provenance))
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
        self.resolve_prefab(name)?.spawn(world)
    }

    /// Spawn a prefab with overrides
    pub fn spawn_with_overrides(
        &self,
        name: &str,
        world: &mut World,
        overrides: &[PropertyOverride],
    ) -> Result<hecs::Entity, String> {
        self.resolve_prefab(name)?
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

fn diff_metadata(current: &PrefabMetadata, base: &PrefabMetadata) -> Option<PrefabMetadataDiff> {
    let diff = PrefabMetadataDiff {
        name: (current.name != base.name).then(|| current.name.clone()),
        version: (current.version != base.version).then_some(current.version),
        description: (current.description != base.description).then(|| current.description.clone()),
        tags: (current.tags != base.tags).then(|| current.tags.clone()),
    };
    (!diff.is_empty()).then_some(diff)
}

impl Default for PrefabRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Vec2, Vec3};
    use pod_core::{Mesh, Sprite, Transform, Transform3D};

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
        prefab.add_component("Transform", &value).unwrap();

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
        let override_ = PropertyOverride::new("Transform.position.x", serde_json::json!(42.0));
        assert_eq!(override_.path, "Transform.position.x");
    }

    #[test]
    fn test_resolved_components_with_report_tracks_applied_and_ignored_overrides() {
        let mut prefab = Prefab::new("OverrideReportPrefab");
        prefab
            .add_native_component(&Transform::at(1.0, 2.0))
            .unwrap();

        let resolved = prefab.resolved_components_with_report(&[
            PropertyOverride::new("Transform.position.x", serde_json::json!(42.0)),
            PropertyOverride::new("Sprite.layer", serde_json::json!(3)),
            PropertyOverride::new("Transform.position.foo", serde_json::json!(9.0)),
        ]);

        let transform = resolved.components["Transform"]
            .get_native::<Transform>()
            .expect("resolved transform should deserialize");
        assert_eq!(transform.position, Vec2::new(42.0, 2.0));

        assert_eq!(resolved.override_report.applied.len(), 1);
        assert_eq!(resolved.override_report.ignored.len(), 2);
        assert_eq!(
            resolved.override_report.applied[0].previous_value,
            Some(serde_json::json!(1.0))
        );
        let ignored_reasons = resolved
            .override_report
            .ignored
            .iter()
            .map(|entry| entry.reason.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(
            ignored_reasons.contains("component 'Sprite' is not present on prefab"),
            "unexpected ignored override reasons: {ignored_reasons}"
        );
        assert!(
            ignored_reasons.contains("incompatible shape")
                || ignored_reasons.contains("cannot assign"),
            "unexpected ignored override reasons: {ignored_reasons}"
        );
    }

    #[test]
    fn test_nested_prefabs() {
        let mut prefab = Prefab::new("ParentPrefab");
        prefab.add_nested_prefab("ChildPrefab");

        assert_eq!(prefab.nested_prefabs.len(), 1);
        assert!(prefab.nested_prefabs.contains(&"ChildPrefab".to_string()));
    }

    #[test]
    fn test_native_components_round_trip() {
        let mut prefab = Prefab::new("NativePrefab");
        let transform = Transform {
            position: Vec2::new(2.0, 4.0),
            rotation: 1.5,
            scale: Vec2::new(3.0, 5.0),
        };
        prefab.add_native_component(&transform).unwrap();

        let restored = prefab
            .get_native_component::<Transform>()
            .unwrap()
            .expect("transform should round-trip");

        assert_eq!(restored.position, transform.position);
        assert_eq!(restored.rotation, transform.rotation);
        assert_eq!(restored.scale, transform.scale);
    }

    #[test]
    fn test_prefab_spawn_inserts_native_2d_components() {
        let mut prefab = Prefab::new("SpritePrefab");
        prefab
            .add_native_component(&Transform::at(12.0, 24.0))
            .unwrap();
        prefab
            .add_native_component(&Sprite {
                texture: "hero".to_string(),
                frame: 3,
                layer: 7,
                ..Default::default()
            })
            .unwrap();

        let mut world = World::new(42);
        let entity = prefab.spawn(&mut world).unwrap();

        let transform = world
            .ecs
            .get::<&Transform>(entity)
            .expect("transform should be inserted");
        let sprite = world
            .ecs
            .get::<&Sprite>(entity)
            .expect("sprite should be inserted");

        assert_eq!(transform.position, Vec2::new(12.0, 24.0));
        assert_eq!(sprite.texture, "hero");
        assert_eq!(sprite.layer, 7);
    }

    #[test]
    fn test_prefab_spawn_with_overrides_updates_native_components() {
        let mut prefab = Prefab::new("MeshPrefab");
        prefab
            .add_native_component(&Transform3D {
                position: Vec3::new(1.0, 2.0, 3.0),
                ..Default::default()
            })
            .unwrap();
        prefab
            .add_native_component(&Mesh {
                asset_id: "crate".to_string(),
                layer: 1,
                ..Default::default()
            })
            .unwrap();

        let overrides = [
            PropertyOverride::new("Transform3D.position.z", serde_json::json!(9.0)),
            PropertyOverride::new("Mesh.layer", serde_json::json!(5)),
        ];

        let mut world = World::new(7);
        let entity = prefab.spawn_with_overrides(&mut world, &overrides).unwrap();

        let transform = world
            .ecs
            .get::<&Transform3D>(entity)
            .expect("transform3d should be inserted");
        let mesh = world
            .ecs
            .get::<&Mesh>(entity)
            .expect("mesh should be inserted");

        assert_eq!(transform.position, Vec3::new(1.0, 2.0, 9.0));
        assert_eq!(mesh.layer, 5);
    }

    #[test]
    fn test_prefab_registry_resolves_inherited_components() {
        let mut registry = PrefabRegistry::new();

        let mut base = Prefab::new("BaseActor");
        base.add_native_component(&Transform::at(1.0, 2.0)).unwrap();
        base.add_native_component(&Sprite {
            texture: "base_actor".to_string(),
            layer: 1,
            ..Default::default()
        })
        .unwrap();
        base.add_nested_prefab("WeaponMount");
        registry.register("BaseActor", base);

        let mut derived = Prefab::new("MageActor").with_base_prefab("BaseActor");
        derived
            .add_native_component(&Sprite {
                texture: "mage_actor".to_string(),
                layer: 5,
                ..Default::default()
            })
            .unwrap();
        derived
            .add_native_component(&Mesh {
                asset_id: "staff".to_string(),
                layer: 7,
                ..Default::default()
            })
            .unwrap();
        derived.add_nested_prefab("SpellFx");
        registry.register("MageActor", derived);

        let resolved = registry.resolve_prefab("MageActor").unwrap();

        let transform = resolved
            .get_native_component::<Transform>()
            .unwrap()
            .expect("derived prefab should inherit transform");
        let sprite = resolved
            .get_native_component::<Sprite>()
            .unwrap()
            .expect("derived prefab should override sprite");
        let mesh = resolved
            .get_native_component::<Mesh>()
            .unwrap()
            .expect("derived prefab should add mesh");

        assert_eq!(transform.position, Vec2::new(1.0, 2.0));
        assert_eq!(sprite.texture, "mage_actor");
        assert_eq!(sprite.layer, 5);
        assert_eq!(mesh.asset_id, "staff");
        assert!(resolved.nested_prefabs.contains(&"WeaponMount".to_string()));
        assert!(resolved.nested_prefabs.contains(&"SpellFx".to_string()));
    }

    #[test]
    fn test_prefab_registry_detects_inheritance_cycles() {
        let mut registry = PrefabRegistry::new();
        registry.register("A", Prefab::new("A").with_base_prefab("B"));
        registry.register("B", Prefab::new("B").with_base_prefab("A"));

        let error = registry
            .resolve_prefab("A")
            .expect_err("cycle should be rejected");
        assert!(error.contains("Prefab inheritance cycle detected"));
        assert!(error.contains("A -> B -> A"));
    }

    #[test]
    fn test_prefab_registry_spawn_uses_resolved_inheritance() {
        let mut registry = PrefabRegistry::new();

        let mut base = Prefab::new("BaseMesh");
        base.add_native_component(&Transform3D {
            position: Vec3::new(6.0, 7.0, 8.0),
            ..Default::default()
        })
        .unwrap();
        registry.register("BaseMesh", base);

        let mut derived = Prefab::new("DerivedMesh").with_base_prefab("BaseMesh");
        derived
            .add_native_component(&Mesh {
                asset_id: "tower".to_string(),
                layer: 3,
                ..Default::default()
            })
            .unwrap();
        registry.register("DerivedMesh", derived);

        let mut world = World::new(100);
        let entity = registry.spawn("DerivedMesh", &mut world).unwrap();

        let transform = world
            .ecs
            .get::<&Transform3D>(entity)
            .expect("spawn should include inherited transform");
        let mesh = world
            .ecs
            .get::<&Mesh>(entity)
            .expect("spawn should include derived mesh");

        assert_eq!(transform.position, Vec3::new(6.0, 7.0, 8.0));
        assert_eq!(mesh.asset_id, "tower");
    }

    #[test]
    fn test_prefab_registry_tracks_component_provenance_across_inheritance_and_overrides() {
        let mut registry = PrefabRegistry::new();

        let mut base = Prefab::new("BaseActor");
        base.add_native_component(&Transform::at(1.0, 2.0)).unwrap();
        base.add_native_component(&Sprite {
            texture: "base".to_string(),
            layer: 1,
            ..Default::default()
        })
        .unwrap();
        registry.register("BaseActor", base);

        let mut derived = Prefab::new("MageActor").with_base_prefab("BaseActor");
        derived
            .add_native_component(&Sprite {
                texture: "mage".to_string(),
                layer: 4,
                ..Default::default()
            })
            .unwrap();
        derived
            .add_native_component(&Mesh {
                asset_id: "staff".to_string(),
                layer: 7,
                ..Default::default()
            })
            .unwrap();
        registry.register("MageActor", derived);

        let resolved = registry
            .resolve_components_with_provenance(
                "MageActor",
                &[PropertyOverride::new("Sprite.layer", serde_json::json!(9))],
            )
            .expect("resolved prefab components should be available");

        assert_eq!(
            resolved.component_provenance["Transform"].layers,
            vec![ComponentProvenanceLayer::PrefabDefinition {
                prefab: "BaseActor".to_string()
            }]
        );
        assert_eq!(
            resolved.component_provenance["Sprite"].layers,
            vec![
                ComponentProvenanceLayer::PrefabDefinition {
                    prefab: "BaseActor".to_string()
                },
                ComponentProvenanceLayer::PrefabDefinition {
                    prefab: "MageActor".to_string()
                },
                ComponentProvenanceLayer::PropertyOverride {
                    path: "Sprite.layer".to_string()
                },
            ]
        );
        assert_eq!(
            resolved.component_provenance["Mesh"].layers,
            vec![ComponentProvenanceLayer::PrefabDefinition {
                prefab: "MageActor".to_string()
            }]
        );
    }

    #[test]
    fn test_prefab_diff_round_trip_apply() {
        let mut base = Prefab::new("Base");
        base.metadata.description = "base prefab".to_string();
        base.add_native_component(&Transform::at(1.0, 1.0)).unwrap();
        base.add_native_component(&Sprite {
            texture: "base".to_string(),
            layer: 1,
            ..Default::default()
        })
        .unwrap();
        base.add_nested_prefab("Mount");

        let mut derived = base.clone().with_base_prefab("BasePrefab");
        derived.metadata.description = "derived prefab".to_string();
        derived.remove_component("Transform");
        derived
            .add_native_component(&Sprite {
                texture: "derived".to_string(),
                layer: 4,
                ..Default::default()
            })
            .unwrap();
        derived
            .add_component("EditorMetadata", &serde_json::json!({ "category": "boss" }))
            .unwrap();
        derived.add_nested_prefab("SpellFx");

        let diff = derived.diff_against(&base);
        assert!(!diff.is_empty(), "diff should capture prefab changes");
        assert!(diff.metadata.is_some(), "metadata diff should be recorded");
        assert_eq!(diff.base_prefab, Some(Some("BasePrefab".to_string())));
        assert_eq!(diff.removed_components, vec!["Transform".to_string()]);
        assert!(diff.changed_components.contains_key("Sprite"));
        assert!(diff.added_components.contains_key("EditorMetadata"));
        assert_eq!(diff.added_nested_prefabs, vec!["SpellFx".to_string()]);

        let mut patched = base.clone();
        patched.apply_diff(&diff);

        assert_eq!(patched.metadata.description, "derived prefab");
        assert_eq!(patched.base_prefab, Some("BasePrefab".to_string()));
        assert!(patched.get_component("Transform").is_none());
        assert_eq!(
            patched
                .get_native_component::<Sprite>()
                .unwrap()
                .expect("sprite should exist")
                .texture,
            "derived"
        );
        assert!(patched.get_component("EditorMetadata").is_some());
        assert!(patched.nested_prefabs.contains(&"Mount".to_string()));
        assert!(patched.nested_prefabs.contains(&"SpellFx".to_string()));
    }
}
