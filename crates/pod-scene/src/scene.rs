use hecs::Entity;
use pod_core::{Parent3D, Transform3D, World};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::binding::{insert_bound_components, NativeComponentBinding};
use crate::json::to_stable_json_string;
use crate::prefab::{
    set_component_path_value, ComponentProvenance, ComponentProvenanceLayer, PrefabComponent,
    PrefabRegistry, PropertyOverride, PropertyOverrideReport,
};

/// Represents a spawn point in a scene
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub name: String,
    pub position: (f32, f32),
    pub id: Uuid,
}

impl SpawnPoint {
    pub fn new(name: impl Into<String>, x: f32, y: f32) -> Self {
        Self {
            name: name.into(),
            position: (x, y),
            id: Uuid::new_v4(),
        }
    }
}

/// Represents parent-child relationships in the scene hierarchy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneGraph {
    parent_map: HashMap<Uuid, Uuid>,
    children_map: HashMap<Uuid, Vec<Uuid>>,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an entity as a child of another entity
    pub fn set_parent(&mut self, child_id: Uuid, parent_id: Uuid) {
        // Remove from old parent if it exists
        if let Some(old_parent) = self.parent_map.get(&child_id) {
            if let Some(children) = self.children_map.get_mut(old_parent) {
                children.retain(|id| id != &child_id);
            }
        }

        // Add to new parent
        self.parent_map.insert(child_id, parent_id);
        self.children_map
            .entry(parent_id)
            .or_insert_with(Vec::new)
            .push(child_id);
    }

    /// Remove an entity from its parent
    pub fn unset_parent(&mut self, child_id: Uuid) {
        if let Some(parent_id) = self.parent_map.remove(&child_id) {
            if let Some(children) = self.children_map.get_mut(&parent_id) {
                children.retain(|id| id != &child_id);
            }
        }
    }

    /// Get the parent of an entity
    pub fn get_parent(&self, child_id: Uuid) -> Option<Uuid> {
        self.parent_map.get(&child_id).copied()
    }

    /// Get children of an entity
    pub fn get_children(&self, parent_id: Uuid) -> Option<Vec<Uuid>> {
        self.children_map.get(&parent_id).cloned()
    }

    /// Get all descendants of an entity (recursive)
    pub fn get_descendants(&self, entity_id: Uuid) -> Vec<Uuid> {
        let mut result = Vec::new();
        let mut queue = vec![entity_id];

        while let Some(current) = queue.pop() {
            if let Some(children) = self.children_map.get(&current) {
                for child in children {
                    result.push(*child);
                    queue.push(*child);
                }
            }
        }

        result
    }
}

/// Metadata about a scene
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneMetadata {
    pub name: String,
    pub version: u32,
    pub description: String,
    pub author: String,
    pub created_at: String,
    pub tags: Vec<String>,
}

impl Default for SceneMetadata {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: 1,
            description: String::new(),
            author: String::new(),
            created_at: chrono_format_date(),
            tags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StreamingBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl StreamingBounds {
    pub fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }

    pub fn from_center_radius(center: [f32; 3], radius: f32) -> Self {
        let [x, y, z] = center;
        Self {
            min: [x - radius, y - radius, z - radius],
            max: [x + radius, y + radius, z + radius],
        }
    }

    pub fn intersects_focus(&self, focus: &SceneStreamFocus) -> bool {
        let [center_x, center_y, center_z] = focus.center;
        let clamped_x = center_x.clamp(self.min[0], self.max[0]);
        let clamped_y = center_y.clamp(self.min[1], self.max[1]);
        let clamped_z = center_z.clamp(self.min[2], self.max[2]);

        let dx = center_x - clamped_x;
        let dy = center_y - clamped_y;
        let dz = center_z - clamped_z;

        dx * dx + dy * dy + dz * dz <= focus.radius * focus.radius
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneStreamFocus {
    pub center: [f32; 3],
    pub radius: f32,
}

impl SceneStreamFocus {
    pub fn new(center: [f32; 3], radius: f32) -> Self {
        Self { center, radius }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneRegion {
    pub id: Uuid,
    pub name: String,
    pub bounds: StreamingBounds,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_ids: Vec<Uuid>,
    #[serde(default)]
    pub always_loaded: bool,
}

impl SceneRegion {
    pub fn new(name: impl Into<String>, bounds: StreamingBounds) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            bounds,
            entity_ids: Vec::new(),
            always_loaded: false,
        }
    }

    pub fn add_entity(&mut self, entity_id: Uuid) {
        if !self.entity_ids.contains(&entity_id) {
            self.entity_ids.push(entity_id);
        }
    }

    pub fn with_always_loaded(mut self, always_loaded: bool) -> Self {
        self.always_loaded = always_loaded;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneStreamPlan {
    pub active_region_ids: Vec<Uuid>,
    pub active_entity_ids: Vec<Uuid>,
}

impl SceneStreamPlan {
    pub fn includes_entity(&self, entity_id: Uuid) -> bool {
        self.active_entity_ids.contains(&entity_id)
    }

    pub fn includes_region(&self, region_id: Uuid) -> bool {
        self.active_region_ids.contains(&region_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntityReferenceTarget {
    ById { id: Uuid },
    ByName { name: String },
}

impl EntityReferenceTarget {
    pub fn by_id(id: Uuid) -> Self {
        Self::ById { id }
    }

    pub fn by_name(name: impl Into<String>) -> Self {
        Self::ByName { name: name.into() }
    }

    fn describe(&self) -> String {
        match self {
            Self::ById { id } => format!("scene entity id {}", id),
            Self::ByName { name } => format!("scene entity name '{}'", name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityReferenceBinding {
    pub component: String,
    pub path: String,
    pub target: EntityReferenceTarget,
}

impl EntityReferenceBinding {
    pub fn new(
        component: impl Into<String>,
        path: impl Into<String>,
        target: EntityReferenceTarget,
    ) -> Self {
        Self {
            component: component.into(),
            path: path.into(),
            target,
        }
    }

    pub fn by_id(component: impl Into<String>, path: impl Into<String>, id: Uuid) -> Self {
        Self::new(component, path, EntityReferenceTarget::by_id(id))
    }

    pub fn by_name(
        component: impl Into<String>,
        path: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self::new(component, path, EntityReferenceTarget::by_name(name))
    }
}

/// Entity instance in a scene with all its component data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInstance {
    pub id: Uuid,
    pub name: String,
    pub prefab_ref: Option<String>, // Reference to a prefab, if this entity was created from one
    pub components: HashMap<String, PrefabComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefab_overrides: Vec<PropertyOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_references: Vec<EntityReferenceBinding>,
}

impl EntityInstance {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            prefab_ref: None,
            components: HashMap::new(),
            prefab_overrides: Vec::new(),
            entity_references: Vec::new(),
        }
    }

    pub fn with_prefab(mut self, prefab_ref: impl Into<String>) -> Self {
        self.prefab_ref = Some(prefab_ref.into());
        self
    }

    pub fn add_component<T: serde::Serialize>(
        &mut self,
        name: impl Into<String>,
        value: &T,
    ) -> Result<(), String> {
        self.components.insert(
            name.into(),
            PrefabComponent::from_json(serde_json::to_value(value).map_err(|err| err.to_string())?),
        );
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

    pub fn get_component(&self, name: &str) -> Option<&PrefabComponent> {
        self.components.get(name)
    }

    pub fn get_native_component<T: NativeComponentBinding>(&self) -> Result<Option<T>, String> {
        self.components
            .get(T::COMPONENT_NAME)
            .map(PrefabComponent::get_native::<T>)
            .transpose()
    }

    pub fn remove_component(&mut self, name: &str) -> Option<PrefabComponent> {
        self.components.remove(name)
    }

    pub fn add_prefab_override(&mut self, override_: PropertyOverride) {
        self.prefab_overrides.push(override_);
    }

    pub fn add_entity_reference(&mut self, reference: EntityReferenceBinding) {
        self.entity_references.push(reference);
    }

    pub fn add_entity_reference_by_id(
        &mut self,
        component: impl Into<String>,
        path: impl Into<String>,
        id: Uuid,
    ) {
        self.add_entity_reference(EntityReferenceBinding::by_id(component, path, id));
    }

    pub fn add_entity_reference_by_name(
        &mut self,
        component: impl Into<String>,
        path: impl Into<String>,
        name: impl Into<String>,
    ) {
        self.add_entity_reference(EntityReferenceBinding::by_name(component, path, name));
    }
}

#[derive(Debug, Clone)]
pub struct SceneSpawnResult {
    pub entity_map: HashMap<Uuid, Entity>,
    pub ignored_components: Vec<String>,
    pub prefab_override_reports: HashMap<Uuid, PropertyOverrideReport>,
    pub component_provenance: HashMap<Uuid, HashMap<String, ComponentProvenance>>,
}

impl SceneSpawnResult {
    pub fn entity_for(&self, scene_entity_id: Uuid) -> Option<Entity> {
        self.entity_map.get(&scene_entity_id).copied()
    }

    pub fn prefab_override_report_for(
        &self,
        scene_entity_id: Uuid,
    ) -> Option<&PropertyOverrideReport> {
        self.prefab_override_reports.get(&scene_entity_id)
    }

    pub fn component_provenance_for(
        &self,
        scene_entity_id: Uuid,
    ) -> Option<&HashMap<String, ComponentProvenance>> {
        self.component_provenance.get(&scene_entity_id)
    }
}

#[derive(Debug, Clone)]
struct ResolvedEntityComponents {
    components: HashMap<String, PrefabComponent>,
    override_report: PropertyOverrideReport,
    component_provenance: HashMap<String, ComponentProvenance>,
}

/// Complete scene representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub metadata: SceneMetadata,
    pub entities: Vec<EntityInstance>,
    pub spawn_points: Vec<SpawnPoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub streaming_regions: Vec<SceneRegion>,
    pub graph: SceneGraph,
    pub id: Uuid,
}

impl Scene {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            metadata: SceneMetadata {
                name: name.into(),
                ..Default::default()
            },
            entities: Vec::new(),
            spawn_points: Vec::new(),
            streaming_regions: Vec::new(),
            graph: SceneGraph::new(),
            id: Uuid::new_v4(),
        }
    }

    /// Add an entity to the scene
    pub fn add_entity(&mut self, entity: EntityInstance) -> Uuid {
        let id = entity.id;
        self.entities.push(entity);
        id
    }

    /// Remove an entity from the scene by ID
    pub fn remove_entity(&mut self, id: Uuid) -> Option<EntityInstance> {
        if let Some(pos) = self.entities.iter().position(|e| e.id == id) {
            Some(self.entities.remove(pos))
        } else {
            None
        }
    }

    /// Get an entity by ID
    pub fn get_entity(&self, id: Uuid) -> Option<&EntityInstance> {
        self.entities.iter().find(|e| e.id == id)
    }

    /// Get a mutable reference to an entity
    pub fn get_entity_mut(&mut self, id: Uuid) -> Option<&mut EntityInstance> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    /// Add a spawn point to the scene
    pub fn add_spawn_point(&mut self, spawn: SpawnPoint) -> Uuid {
        let id = spawn.id;
        self.spawn_points.push(spawn);
        id
    }

    /// Get a spawn point by name
    pub fn get_spawn_point(&self, name: &str) -> Option<&SpawnPoint> {
        self.spawn_points.iter().find(|s| s.name == name)
    }

    pub fn add_streaming_region(&mut self, region: SceneRegion) -> Uuid {
        let id = region.id;
        self.streaming_regions.push(region);
        id
    }

    pub fn get_streaming_region(&self, id: Uuid) -> Option<&SceneRegion> {
        self.streaming_regions.iter().find(|region| region.id == id)
    }

    /// Serialize scene to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        to_stable_json_string(self)
    }

    /// Deserialize scene from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize scene to binary (bincode)
    pub fn to_binary(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserialize scene from binary
    pub fn from_binary(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }

    pub fn instantiate(&self, world: &mut World) -> Result<SceneSpawnResult, String> {
        self.instantiate_subset(world, None, None)
    }

    pub fn instantiate_with_prefabs(
        &self,
        world: &mut World,
        prefabs: Option<&PrefabRegistry>,
    ) -> Result<SceneSpawnResult, String> {
        self.instantiate_subset(world, prefabs, None)
    }

    pub fn build_stream_plan(
        &self,
        focuses: &[SceneStreamFocus],
    ) -> Result<SceneStreamPlan, String> {
        let entity_ids: HashSet<Uuid> = self.entities.iter().map(|entity| entity.id).collect();
        let mut assigned_entities = HashSet::new();
        let mut active_entities = HashSet::new();
        let mut active_region_ids = Vec::new();

        for region in &self.streaming_regions {
            for entity_id in &region.entity_ids {
                if !entity_ids.contains(entity_id) {
                    return Err(format!(
                        "Streaming region '{}' references missing scene entity {}",
                        region.name, entity_id
                    ));
                }
                assigned_entities.insert(*entity_id);
            }

            let is_active = region.always_loaded
                || focuses
                    .iter()
                    .any(|focus| region.bounds.intersects_focus(focus));
            if is_active {
                active_region_ids.push(region.id);
                active_entities.extend(region.entity_ids.iter().copied());
            }
        }

        if self.streaming_regions.is_empty() {
            active_entities.extend(entity_ids.iter().copied());
        } else {
            active_entities.extend(
                self.entities
                    .iter()
                    .filter(|entity| !assigned_entities.contains(&entity.id))
                    .map(|entity| entity.id),
            );
        }

        self.expand_streaming_dependencies(&mut active_entities)?;

        let mut active_entity_ids: Vec<Uuid> = active_entities.into_iter().collect();
        active_region_ids.sort_by_key(Uuid::as_u128);
        active_entity_ids.sort_by_key(Uuid::as_u128);

        Ok(SceneStreamPlan {
            active_region_ids,
            active_entity_ids,
        })
    }

    pub fn instantiate_streamed(
        &self,
        world: &mut World,
        focuses: &[SceneStreamFocus],
        prefabs: Option<&PrefabRegistry>,
    ) -> Result<SceneSpawnResult, String> {
        let plan = self.build_stream_plan(focuses)?;
        let active_entities: HashSet<Uuid> = plan.active_entity_ids.into_iter().collect();
        self.instantiate_subset(world, prefabs, Some(&active_entities))
    }

    fn instantiate_subset(
        &self,
        world: &mut World,
        prefabs: Option<&PrefabRegistry>,
        active_entities: Option<&HashSet<Uuid>>,
    ) -> Result<SceneSpawnResult, String> {
        let mut entity_map = HashMap::new();
        let entity_name_lookup = self.build_entity_name_lookup();
        let mut prefab_override_reports = HashMap::new();
        let mut component_provenance = HashMap::new();

        let scene_entities: Vec<&EntityInstance> = self
            .entities
            .iter()
            .filter(|entity| {
                active_entities
                    .map(|ids| ids.contains(&entity.id))
                    .unwrap_or(true)
            })
            .collect();

        for entity in &scene_entities {
            entity_map.insert(entity.id, world.ecs.spawn(()));
        }

        let mut ignored_components = Vec::new();

        for entity in &scene_entities {
            let spawned_entity = entity_map
                .get(&entity.id)
                .copied()
                .ok_or_else(|| format!("Scene entity '{}' was not pre-spawned", entity.name))?;

            let mut resolved = self.resolve_entity_components(entity, prefabs)?;
            self.apply_entity_references(
                entity,
                &mut resolved.components,
                &mut resolved.component_provenance,
                &entity_map,
                &entity_name_lookup,
            )?;
            if !resolved.override_report.is_empty() {
                prefab_override_reports.insert(entity.id, resolved.override_report.clone());
            }
            if !resolved.component_provenance.is_empty() {
                component_provenance.insert(entity.id, resolved.component_provenance.clone());
            }
            let ignored =
                insert_bound_components(&resolved.components, &mut world.ecs, spawned_entity)?;
            ignored_components.extend(
                ignored
                    .into_iter()
                    .map(|component_name| format!("{}::{}", entity.name, component_name)),
            );
        }

        self.apply_parent_graph(world, &entity_map)?;

        ignored_components.sort();

        Ok(SceneSpawnResult {
            entity_map,
            ignored_components,
            prefab_override_reports,
            component_provenance,
        })
    }

    fn resolve_entity_components(
        &self,
        entity: &EntityInstance,
        prefabs: Option<&PrefabRegistry>,
    ) -> Result<ResolvedEntityComponents, String> {
        let mut resolved = match entity.prefab_ref.as_deref() {
            Some(prefab_name) => {
                let registry = prefabs.ok_or_else(|| {
                    format!(
                        "Scene '{}' requires prefab registry to resolve '{}'",
                        self.metadata.name, prefab_name
                    )
                })?;
                let resolved = registry
                    .resolve_components_with_provenance(prefab_name, &entity.prefab_overrides)?;
                ResolvedEntityComponents {
                    components: resolved.components,
                    override_report: resolved.override_report,
                    component_provenance: resolved.component_provenance,
                }
            }
            None => {
                if !entity.prefab_overrides.is_empty() {
                    return Err(format!(
                        "Scene entity '{}' defines prefab overrides but has no prefab reference",
                        entity.name
                    ));
                }
                ResolvedEntityComponents {
                    components: HashMap::new(),
                    override_report: PropertyOverrideReport::default(),
                    component_provenance: HashMap::new(),
                }
            }
        };

        for (name, component) in &entity.components {
            resolved.components.insert(name.clone(), component.clone());
            resolved
                .component_provenance
                .entry(name.clone())
                .or_default()
                .push(ComponentProvenanceLayer::SceneComponent {
                    entity_id: entity.id,
                    entity_name: entity.name.clone(),
                });
        }

        Ok(resolved)
    }

    fn build_entity_name_lookup(&self) -> HashMap<String, Vec<Uuid>> {
        let mut lookup = HashMap::<String, Vec<Uuid>>::new();
        for entity in &self.entities {
            lookup
                .entry(entity.name.clone())
                .or_default()
                .push(entity.id);
        }
        lookup
    }

    fn expand_streaming_dependencies(
        &self,
        active_entities: &mut HashSet<Uuid>,
    ) -> Result<(), String> {
        let entity_lookup: HashMap<Uuid, &EntityInstance> = self
            .entities
            .iter()
            .map(|entity| (entity.id, entity))
            .collect();
        let entity_name_lookup = self.build_entity_name_lookup();
        let mut queue: Vec<Uuid> = active_entities.iter().copied().collect();

        while let Some(entity_id) = queue.pop() {
            let entity = entity_lookup.get(&entity_id).copied().ok_or_else(|| {
                format!(
                    "Streaming plan references missing scene entity {} while expanding dependencies",
                    entity_id
                )
            })?;

            if let Some(parent_id) = self.graph.get_parent(entity_id) {
                if !entity_lookup.contains_key(&parent_id) {
                    return Err(format!(
                        "Streaming dependency for '{}' points to missing parent entity {}",
                        entity.name, parent_id
                    ));
                }

                if active_entities.insert(parent_id) {
                    queue.push(parent_id);
                }
            }

            for reference in &entity.entity_references {
                let target_id = match &reference.target {
                    EntityReferenceTarget::ById { id } => *id,
                    EntityReferenceTarget::ByName { name } => {
                        let matches = entity_name_lookup.get(name).ok_or_else(|| {
                            format!(
                                "Streaming dependency '{}.{}' on '{}' points to missing {}",
                                reference.component,
                                reference.path,
                                entity.name,
                                reference.target.describe()
                            )
                        })?;

                        if matches.len() != 1 {
                            return Err(format!(
                                "Streaming dependency '{}.{}' on '{}' is ambiguous for {}",
                                reference.component,
                                reference.path,
                                entity.name,
                                reference.target.describe()
                            ));
                        }

                        matches[0]
                    }
                };

                if !entity_lookup.contains_key(&target_id) {
                    return Err(format!(
                        "Streaming dependency '{}.{}' on '{}' points to missing scene entity {}",
                        reference.component, reference.path, entity.name, target_id
                    ));
                }

                if active_entities.insert(target_id) {
                    queue.push(target_id);
                }
            }
        }

        Ok(())
    }

    fn apply_entity_references(
        &self,
        entity: &EntityInstance,
        components: &mut HashMap<String, PrefabComponent>,
        component_provenance: &mut HashMap<String, ComponentProvenance>,
        entity_map: &HashMap<Uuid, Entity>,
        entity_name_lookup: &HashMap<String, Vec<Uuid>>,
    ) -> Result<(), String> {
        for reference in &entity.entity_references {
            let target = self.resolve_entity_reference_target(
                entity,
                reference,
                entity_map,
                entity_name_lookup,
            )?;
            let component = components.get_mut(&reference.component).ok_or_else(|| {
                format!(
                    "Entity reference '{}.{}' on '{}' requires component '{}'",
                    reference.component, reference.path, entity.name, reference.component
                )
            })?;
            let path: Vec<&str> = reference
                .path
                .split('.')
                .filter(|segment| !segment.is_empty())
                .collect();
            if path.is_empty() {
                return Err(format!(
                    "Entity reference on '{}' for component '{}' must specify a non-empty property path",
                    entity.name, reference.component
                ));
            }

            set_component_path_value(component, &path, &serde_json::json!(target.id() as u64))
                .map_err(|err| {
                    format!(
                        "Failed to resolve entity reference '{}.{}' on '{}' in scene '{}': {}",
                        reference.component, reference.path, entity.name, self.metadata.name, err
                    )
                })?;
            component_provenance
                .entry(reference.component.clone())
                .or_default()
                .push(ComponentProvenanceLayer::EntityReference {
                    path: format!("{}.{}", reference.component, reference.path),
                    target: reference.target.describe(),
                });
        }

        Ok(())
    }

    fn resolve_entity_reference_target(
        &self,
        entity: &EntityInstance,
        reference: &EntityReferenceBinding,
        entity_map: &HashMap<Uuid, Entity>,
        entity_name_lookup: &HashMap<String, Vec<Uuid>>,
    ) -> Result<Entity, String> {
        let target_scene_id = match &reference.target {
            EntityReferenceTarget::ById { id } => *id,
            EntityReferenceTarget::ByName { name } => {
                let matches = entity_name_lookup.get(name).ok_or_else(|| {
                    format!(
                        "Entity reference '{}.{}' on '{}' points to missing {}",
                        reference.component,
                        reference.path,
                        entity.name,
                        reference.target.describe()
                    )
                })?;

                if matches.len() != 1 {
                    return Err(format!(
                        "Entity reference '{}.{}' on '{}' is ambiguous for {}",
                        reference.component,
                        reference.path,
                        entity.name,
                        reference.target.describe()
                    ));
                }

                matches[0]
            }
        };

        entity_map.get(&target_scene_id).copied().ok_or_else(|| {
            format!(
                "Entity reference '{}.{}' on '{}' points to unresolved scene entity {}",
                reference.component, reference.path, entity.name, target_scene_id
            )
        })
    }

    fn apply_parent_graph(
        &self,
        world: &mut World,
        entity_map: &HashMap<Uuid, Entity>,
    ) -> Result<(), String> {
        let mut parent_links: Vec<(Uuid, Uuid)> = self
            .graph
            .parent_map
            .iter()
            .map(|(child, parent)| (child.to_owned(), parent.to_owned()))
            .collect();
        parent_links.sort_by_key(|(child, _)| child.as_u128());

        for (child_id, parent_id) in parent_links {
            let Some(child_entity) = entity_map.get(&child_id).copied() else {
                continue;
            };
            let Some(parent_entity) = entity_map.get(&parent_id).copied() else {
                continue;
            };

            if world.ecs.get::<&Transform3D>(child_entity).is_err() {
                continue;
            }

            let _ = world.ecs.remove_one::<Parent3D>(child_entity);
            world
                .ecs
                .insert_one(
                    child_entity,
                    Parent3D {
                        parent: parent_entity.id() as u64,
                    },
                )
                .map_err(|err| err.to_string())?;
        }

        Ok(())
    }
}

/// Manages loading, unloading, and transitioning between scenes
pub struct SceneManager {
    scenes: HashMap<String, Scene>,
    active_scene: Option<String>,
    scene_files: HashMap<String, String>, // Filename cache for hot-reload
}

impl SceneManager {
    pub fn new() -> Self {
        Self {
            scenes: HashMap::new(),
            active_scene: None,
            scene_files: HashMap::new(),
        }
    }

    /// Load a scene from JSON file
    pub fn load_scene(&mut self, name: impl Into<String>, json: &str) -> Result<(), String> {
        let name_str = name.into();
        let scene = Scene::from_json(json).map_err(|e| e.to_string())?;
        self.scene_files.insert(name_str.clone(), json.to_string());
        self.scenes.insert(name_str, scene);
        Ok(())
    }

    /// Load a scene from binary data
    pub fn load_scene_binary(
        &mut self,
        name: impl Into<String>,
        data: &[u8],
    ) -> Result<(), String> {
        let name_str = name.into();
        let scene = Scene::from_binary(data).map_err(|e| e.to_string())?;
        self.scenes.insert(name_str, scene);
        Ok(())
    }

    /// Create a new scene
    pub fn create_scene(&mut self, name: impl Into<String>) -> &mut Scene {
        let name_str = name.into();
        let name_for_new = name_str.clone();
        self.scenes
            .entry(name_str)
            .or_insert_with(|| Scene::new(&name_for_new))
    }

    /// Activate a scene
    pub fn set_active(&mut self, name: &str) -> Result<(), String> {
        if !self.scenes.contains_key(name) {
            return Err(format!("Scene '{}' not found", name));
        }
        self.active_scene = Some(name.to_string());
        Ok(())
    }

    /// Get the active scene
    pub fn get_active_scene(&self) -> Option<&Scene> {
        self.active_scene
            .as_ref()
            .and_then(|name| self.scenes.get(name))
    }

    /// Get a mutable reference to the active scene
    pub fn get_active_scene_mut(&mut self) -> Option<&mut Scene> {
        let name = self.active_scene.clone();
        name.and_then(|n| self.scenes.get_mut(&n))
    }

    /// Get a scene by name
    pub fn get_scene(&self, name: &str) -> Option<&Scene> {
        self.scenes.get(name)
    }

    /// Get a mutable reference to a scene
    pub fn get_scene_mut(&mut self, name: &str) -> Option<&mut Scene> {
        self.scenes.get_mut(name)
    }

    /// Unload a scene
    pub fn unload_scene(&mut self, name: &str) -> Option<Scene> {
        if self
            .active_scene
            .as_ref()
            .map(|n| n == name)
            .unwrap_or(false)
        {
            self.active_scene = None;
        }
        self.scene_files.remove(name);
        self.scenes.remove(name)
    }

    /// Transition from one scene to another
    pub fn transition_scene(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        self.unload_scene(old_name);
        self.set_active(new_name)
    }

    /// List all loaded scenes
    pub fn list_scenes(&self) -> Vec<String> {
        self.scenes.keys().cloned().collect()
    }

    /// Check if a scene is loaded
    pub fn contains_scene(&self, name: &str) -> bool {
        self.scenes.contains_key(name)
    }

    /// Get the active scene name
    pub fn get_active_name(&self) -> Option<&str> {
        self.active_scene.as_deref()
    }

    /// Hot-reload: check if a scene file has changed and reload it
    pub fn hot_reload_scene(&mut self, name: &str, new_json: &str) -> Result<(), String> {
        if let Some(old_json) = self.scene_files.get(name) {
            if old_json != new_json {
                // File changed, reload it
                self.load_scene(name, new_json)?;
                log::info!("Hot-reloaded scene '{}'", name);
            }
        }
        Ok(())
    }

    /// Get scene file content for hot-reload tracking
    pub fn get_scene_file(&self, name: &str) -> Option<&str> {
        self.scene_files.get(name).map(|s| s.as_str())
    }
}

impl Default for SceneManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to get current date/time as string
fn chrono_format_date() -> String {
    use std::time::SystemTime;
    let time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:?}", time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use pod_core::{
        Camera3D, FollowCameraController, Label, Material, Mesh, Parent3D, Sprite, Team, Transform,
        Transform3D,
    };

    use crate::prefab::{Prefab, PropertyOverride};

    #[test]
    fn test_scene_graph_parent_child() {
        let mut graph = SceneGraph::new();
        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        graph.set_parent(child_id, parent_id);

        assert_eq!(graph.get_parent(child_id), Some(parent_id));
        assert_eq!(graph.get_children(parent_id), Some(vec![child_id]));
    }

    #[test]
    fn test_scene_graph_descendants() {
        let mut graph = SceneGraph::new();
        let root = Uuid::new_v4();
        let child1 = Uuid::new_v4();
        let child2 = Uuid::new_v4();
        let grandchild = Uuid::new_v4();

        graph.set_parent(child1, root);
        graph.set_parent(child2, root);
        graph.set_parent(grandchild, child1);

        let descendants = graph.get_descendants(root);
        assert_eq!(descendants.len(), 3);
        assert!(descendants.contains(&child1));
        assert!(descendants.contains(&child2));
        assert!(descendants.contains(&grandchild));
    }

    #[test]
    fn test_scene_serialization() {
        let mut scene = Scene::new("TestScene");
        scene.metadata.author = "Test Author".to_string();
        let entity = EntityInstance::new("Entity1");
        scene.add_entity(entity);

        let json = scene.to_json().unwrap();
        let loaded = Scene::from_json(&json).unwrap();

        assert_eq!(loaded.metadata.name, "TestScene");
        assert_eq!(loaded.metadata.author, "Test Author");
        assert_eq!(loaded.entities.len(), 1);
    }

    #[test]
    fn test_scene_manager_transitions() {
        let mut manager = SceneManager::new();

        manager.create_scene("Scene1");
        manager.create_scene("Scene2");

        manager.set_active("Scene1").unwrap();
        assert_eq!(manager.get_active_name(), Some("Scene1"));

        manager.transition_scene("Scene1", "Scene2").unwrap();
        assert_eq!(manager.get_active_name(), Some("Scene2"));
    }

    #[test]
    fn test_scene_instantiation_ignores_unknown_editor_components() {
        let mut scene = Scene::new("EditorScene");
        let mut entity = EntityInstance::new("EditorOnly");
        entity
            .add_component("EditorMetadata", &serde_json::json!({ "selected": true }))
            .unwrap();
        scene.add_entity(entity);

        let mut world = World::new(1);
        let result = scene.instantiate(&mut world).unwrap();

        assert_eq!(
            result.ignored_components,
            vec!["EditorOnly::EditorMetadata"]
        );
        assert_eq!(world.entity_count(), 1);
    }

    #[test]
    fn test_scene_instantiation_supports_2d_25d_and_3d_entities() {
        let mut registry = PrefabRegistry::new();
        let mut actor_base = Prefab::new("BaseBillboardActor");
        actor_base
            .add_native_component(&Transform3D {
                position: Vec3::new(0.0, 0.5, 1.0),
                ..Default::default()
            })
            .unwrap();
        actor_base
            .add_native_component(&Label {
                name: "ActorBase".to_string(),
                team: Team::Team(2),
            })
            .unwrap();
        registry.register("BaseBillboardActor", actor_base);

        let mut actor_prefab = Prefab::new("BillboardActor").with_base_prefab("BaseBillboardActor");
        actor_prefab.metadata.description = "Derived billboard actor".to_string();
        actor_prefab
            .add_native_component(&Sprite {
                texture: "actor_sprite".to_string(),
                layer: 4,
                ..Default::default()
            })
            .unwrap();
        registry.register("BillboardActor", actor_prefab);

        let mut scene = Scene::new("MixedScene");

        let mut hud = EntityInstance::new("Hud");
        hud.add_native_component(&Transform::at(-4.0, 8.0)).unwrap();
        hud.add_native_component(&Sprite {
            texture: "hud".to_string(),
            layer: 1,
            ..Default::default()
        })
        .unwrap();
        scene.add_entity(hud);

        let mut root_mesh = EntityInstance::new("RootMesh");
        root_mesh
            .add_native_component(&Transform3D {
                position: Vec3::new(10.0, 0.0, 2.0),
                ..Default::default()
            })
            .unwrap();
        root_mesh
            .add_native_component(&Mesh {
                asset_id: "tower".to_string(),
                layer: 3,
                ..Default::default()
            })
            .unwrap();
        root_mesh
            .add_native_component(&Material {
                asset_id: "stone".to_string(),
                ..Default::default()
            })
            .unwrap();
        let root_mesh_id = scene.add_entity(root_mesh);

        let mut actor = EntityInstance::new("Actor").with_prefab("BillboardActor");
        actor
            .add_native_component(&Transform3D {
                position: Vec3::new(2.0, 1.0, 5.0),
                ..Default::default()
            })
            .unwrap();
        let actor_id = scene.add_entity(actor);
        scene.graph.set_parent(actor_id, root_mesh_id);

        let mut world = World::new(9);
        let result = scene
            .instantiate_with_prefabs(&mut world, Some(&registry))
            .unwrap();

        assert!(
            result.ignored_components.is_empty(),
            "native-only scene should not produce ignored components"
        );

        let actor_entity = result
            .entity_for(actor_id)
            .expect("actor should be spawned");
        let root_entity = result
            .entity_for(root_mesh_id)
            .expect("root mesh should be spawned");
        let hud_entity = result
            .entity_map
            .values()
            .copied()
            .find(|entity| {
                world
                    .ecs
                    .get::<&Sprite>(*entity)
                    .map(|sprite| sprite.texture == "hud")
                    .unwrap_or(false)
            })
            .expect("hud entity should be spawned");

        let parent = world
            .ecs
            .get::<&Parent3D>(actor_entity)
            .expect("3D child should receive parent linkage");
        assert_eq!(parent.parent, root_entity.id() as u64);

        let hud_transform = world
            .ecs
            .get::<&Transform>(hud_entity)
            .expect("2D hud should keep Transform");
        let hud_sprite = world
            .ecs
            .get::<&Sprite>(hud_entity)
            .expect("2D hud should keep Sprite");
        assert_eq!(hud_transform.position, Transform::at(-4.0, 8.0).position);
        assert_eq!(hud_sprite.texture, "hud");

        let root_transform = world
            .ecs
            .get::<&Transform3D>(root_entity)
            .expect("3D root should keep Transform3D");
        let root_mesh = world
            .ecs
            .get::<&Mesh>(root_entity)
            .expect("3D root should keep Mesh");
        assert_eq!(root_transform.position, Vec3::new(10.0, 0.0, 2.0));
        assert_eq!(root_mesh.asset_id, "tower");

        let actor_transform = world
            .ecs
            .get::<&Transform3D>(actor_entity)
            .expect("2.5D actor should keep Transform3D");
        let actor_sprite = world
            .ecs
            .get::<&Sprite>(actor_entity)
            .expect("2.5D actor should keep Sprite");
        let actor_label = world
            .ecs
            .get::<&Label>(actor_entity)
            .expect("2.5D actor should inherit Label from base prefab");
        assert_eq!(actor_transform.position, Vec3::new(2.0, 1.0, 5.0));
        assert_eq!(actor_sprite.texture, "actor_sprite");
        assert_eq!(actor_label.name, "ActorBase");
        assert_eq!(actor_label.team, Team::Team(2));
    }

    #[test]
    fn test_scene_instantiation_resolves_entity_references_by_id() {
        let mut scene = Scene::new("EntityRefScene");

        let mut player = EntityInstance::new("Player");
        player
            .add_native_component(&Transform3D {
                position: Vec3::new(4.0, 0.0, 2.0),
                ..Default::default()
            })
            .unwrap();
        let player_id = scene.add_entity(player);

        let mut camera = EntityInstance::new("Camera");
        camera.add_native_component(&Camera3D::default()).unwrap();
        camera
            .add_native_component(&FollowCameraController::default())
            .unwrap();
        camera.add_entity_reference_by_id("FollowCameraController", "target", player_id);
        let camera_id = scene.add_entity(camera);

        let mut world = World::new(5);
        let result = scene.instantiate(&mut world).unwrap();

        let player_entity = result
            .entity_for(player_id)
            .expect("player should be spawned");
        let camera_entity = result
            .entity_for(camera_id)
            .expect("camera should be spawned");
        let controller = world
            .ecs
            .get::<&FollowCameraController>(camera_entity)
            .expect("camera should keep follow controller");
        assert_eq!(controller.target, player_entity.id() as u64);
    }

    #[test]
    fn test_scene_instantiation_resolves_entity_references_by_name_for_prefabs() {
        let mut registry = PrefabRegistry::new();
        let mut camera_prefab = Prefab::new("CameraRig");
        camera_prefab
            .add_native_component(&Camera3D::default())
            .unwrap();
        camera_prefab
            .add_native_component(&FollowCameraController::default())
            .unwrap();
        registry.register("CameraRig", camera_prefab);

        let mut scene = Scene::new("PrefabRefScene");

        let mut player = EntityInstance::new("Player");
        player
            .add_native_component(&Transform3D {
                position: Vec3::new(-2.0, 1.0, 8.0),
                ..Default::default()
            })
            .unwrap();
        let player_id = scene.add_entity(player);

        let mut camera = EntityInstance::new("Camera").with_prefab("CameraRig");
        camera.add_entity_reference_by_name("FollowCameraController", "target", "Player");
        let camera_id = scene.add_entity(camera);

        let mut world = World::new(6);
        let result = scene
            .instantiate_with_prefabs(&mut world, Some(&registry))
            .unwrap();

        let player_entity = result
            .entity_for(player_id)
            .expect("player should be spawned");
        let camera_entity = result
            .entity_for(camera_id)
            .expect("camera should be spawned");
        let controller = world
            .ecs
            .get::<&FollowCameraController>(camera_entity)
            .expect("prefab-backed camera should keep follow controller");
        assert_eq!(controller.target, player_entity.id() as u64);
    }

    #[test]
    fn test_scene_instantiation_rejects_missing_entity_reference_target() {
        let mut scene = Scene::new("MissingEntityRefScene");
        let mut camera = EntityInstance::new("Camera");
        camera.add_native_component(&Camera3D::default()).unwrap();
        camera
            .add_native_component(&FollowCameraController::default())
            .unwrap();
        camera.add_entity_reference_by_name("FollowCameraController", "target", "Missing");
        scene.add_entity(camera);

        let mut world = World::new(7);
        let err = scene.instantiate(&mut world).expect_err(
            "scene instantiation should fail when a named entity reference is unresolved",
        );
        assert!(err.contains("missing scene entity name 'Missing'"));
    }

    #[test]
    fn test_scene_instantiation_rejects_ambiguous_entity_reference_name() {
        let mut scene = Scene::new("AmbiguousEntityRefScene");

        let mut first_player = EntityInstance::new("Player");
        first_player
            .add_native_component(&Transform3D::default())
            .unwrap();
        scene.add_entity(first_player);

        let mut second_player = EntityInstance::new("Player");
        second_player
            .add_native_component(&Transform3D::default())
            .unwrap();
        scene.add_entity(second_player);

        let mut camera = EntityInstance::new("Camera");
        camera.add_native_component(&Camera3D::default()).unwrap();
        camera
            .add_native_component(&FollowCameraController::default())
            .unwrap();
        camera.add_entity_reference_by_name("FollowCameraController", "target", "Player");
        scene.add_entity(camera);

        let mut world = World::new(8);
        let err = scene.instantiate(&mut world).expect_err(
            "scene instantiation should fail when a named entity reference is ambiguous",
        );
        assert!(err.contains("ambiguous"));
    }

    #[test]
    fn test_scene_instantiation_applies_prefab_overrides_and_reports_them() {
        let mut registry = PrefabRegistry::new();
        let mut actor_prefab = Prefab::new("Actor");
        actor_prefab
            .add_native_component(&Sprite {
                texture: "hero".to_string(),
                layer: 2,
                ..Default::default()
            })
            .unwrap();
        registry.register("Actor", actor_prefab);

        let mut scene = Scene::new("PrefabOverrideScene");
        let mut actor = EntityInstance::new("ActorInstance").with_prefab("Actor");
        actor.add_prefab_override(PropertyOverride::new("Sprite.layer", serde_json::json!(9)));
        let actor_id = scene.add_entity(actor);

        let mut world = World::new(12);
        let result = scene
            .instantiate_with_prefabs(&mut world, Some(&registry))
            .unwrap();

        let actor_entity = result.entity_for(actor_id).expect("actor should spawn");
        let sprite = world
            .ecs
            .get::<&Sprite>(actor_entity)
            .expect("actor should keep sprite");
        assert_eq!(sprite.layer, 9);

        let report = result
            .prefab_override_report_for(actor_id)
            .expect("override report should be stored");
        assert_eq!(report.applied.len(), 1);
        assert!(report.ignored.is_empty());
        assert_eq!(report.applied[0].path, "Sprite.layer");
        assert_eq!(report.applied[0].previous_value, Some(serde_json::json!(2)));
        assert_eq!(report.applied[0].value, serde_json::json!(9));
    }

    #[test]
    fn test_scene_instantiation_prefab_override_report_coexists_with_local_component_override() {
        let mut registry = PrefabRegistry::new();
        let mut actor_prefab = Prefab::new("Actor");
        actor_prefab
            .add_native_component(&Sprite {
                texture: "hero".to_string(),
                layer: 2,
                ..Default::default()
            })
            .unwrap();
        registry.register("Actor", actor_prefab);

        let mut scene = Scene::new("PrefabOverridePriorityScene");
        let mut actor = EntityInstance::new("ActorInstance").with_prefab("Actor");
        actor.add_prefab_override(PropertyOverride::new("Sprite.layer", serde_json::json!(9)));
        actor
            .add_native_component(&Sprite {
                texture: "hero_local".to_string(),
                layer: 14,
                ..Default::default()
            })
            .unwrap();
        let actor_id = scene.add_entity(actor);

        let mut world = World::new(13);
        let result = scene
            .instantiate_with_prefabs(&mut world, Some(&registry))
            .unwrap();

        let actor_entity = result.entity_for(actor_id).expect("actor should spawn");
        let sprite = world
            .ecs
            .get::<&Sprite>(actor_entity)
            .expect("local component override should still insert sprite");
        assert_eq!(sprite.texture, "hero_local");
        assert_eq!(sprite.layer, 14);

        let report = result
            .prefab_override_report_for(actor_id)
            .expect("override report should still be stored");
        assert_eq!(report.applied.len(), 1);
        assert_eq!(report.applied[0].value, serde_json::json!(9));
    }

    #[test]
    fn test_scene_instantiation_tracks_component_provenance_across_prefab_and_scene_layers() {
        let mut registry = PrefabRegistry::new();

        let mut base = Prefab::new("BaseActor");
        base.add_native_component(&Transform::at(1.0, 2.0)).unwrap();
        base.add_native_component(&Sprite {
            texture: "base".to_string(),
            layer: 2,
            ..Default::default()
        })
        .unwrap();
        registry.register("BaseActor", base);

        let mut derived = Prefab::new("DerivedActor").with_base_prefab("BaseActor");
        derived
            .add_native_component(&Sprite {
                texture: "derived".to_string(),
                layer: 4,
                ..Default::default()
            })
            .unwrap();
        registry.register("DerivedActor", derived);

        let mut scene = Scene::new("ProvenanceScene");
        let mut actor = EntityInstance::new("ActorInstance").with_prefab("DerivedActor");
        actor.add_prefab_override(PropertyOverride::new("Sprite.layer", serde_json::json!(9)));
        actor
            .add_native_component(&Sprite {
                texture: "scene_local".to_string(),
                layer: 14,
                ..Default::default()
            })
            .unwrap();
        let actor_id = scene.add_entity(actor);

        let mut world = World::new(15);
        let result = scene
            .instantiate_with_prefabs(&mut world, Some(&registry))
            .unwrap();

        let provenance = result
            .component_provenance_for(actor_id)
            .expect("component provenance should be stored");
        assert_eq!(
            provenance["Transform"].layers,
            vec![ComponentProvenanceLayer::PrefabDefinition {
                prefab: "BaseActor".to_string()
            }]
        );
        assert_eq!(
            provenance["Sprite"].layers,
            vec![
                ComponentProvenanceLayer::PrefabDefinition {
                    prefab: "BaseActor".to_string()
                },
                ComponentProvenanceLayer::PrefabDefinition {
                    prefab: "DerivedActor".to_string()
                },
                ComponentProvenanceLayer::PropertyOverride {
                    path: "Sprite.layer".to_string()
                },
                ComponentProvenanceLayer::SceneComponent {
                    entity_id: actor_id,
                    entity_name: "ActorInstance".to_string()
                },
            ]
        );
    }

    #[test]
    fn test_scene_instantiation_tracks_entity_reference_provenance() {
        let mut scene = Scene::new("EntityReferenceProvenanceScene");

        let mut player = EntityInstance::new("Player");
        player
            .add_native_component(&Transform3D {
                position: Vec3::new(4.0, 0.0, 2.0),
                ..Default::default()
            })
            .unwrap();
        let player_id = scene.add_entity(player);

        let mut camera = EntityInstance::new("Camera");
        camera.add_native_component(&Camera3D::default()).unwrap();
        camera
            .add_native_component(&FollowCameraController::default())
            .unwrap();
        camera.add_entity_reference_by_id("FollowCameraController", "target", player_id);
        let camera_id = scene.add_entity(camera);

        let mut world = World::new(16);
        let result = scene.instantiate(&mut world).unwrap();

        let provenance = result
            .component_provenance_for(camera_id)
            .expect("camera provenance should be recorded");
        assert_eq!(
            provenance["FollowCameraController"].layers,
            vec![
                ComponentProvenanceLayer::SceneComponent {
                    entity_id: camera_id,
                    entity_name: "Camera".to_string()
                },
                ComponentProvenanceLayer::EntityReference {
                    path: "FollowCameraController.target".to_string(),
                    target: format!("scene entity id {}", player_id),
                },
            ]
        );
    }

    #[test]
    fn test_scene_instantiation_rejects_prefab_overrides_without_prefab() {
        let mut scene = Scene::new("InvalidPrefabOverrideScene");
        let mut entity = EntityInstance::new("LooseEntity");
        entity.add_prefab_override(PropertyOverride::new("Sprite.layer", serde_json::json!(7)));
        scene.add_entity(entity);

        let mut world = World::new(14);
        let err = scene
            .instantiate(&mut world)
            .expect_err("prefab overrides should require a prefab reference");
        assert!(err.contains("defines prefab overrides but has no prefab reference"));
    }

    #[test]
    fn test_scene_stream_plan_selects_regions_and_unassigned_entities() {
        let mut scene = Scene::new("StreamingScene");

        let mut near = EntityInstance::new("Near");
        near.add_native_component(&Transform::at(0.0, 0.0)).unwrap();
        let near_id = scene.add_entity(near);

        let mut far = EntityInstance::new("Far");
        far.add_native_component(&Transform::at(100.0, 0.0))
            .unwrap();
        let far_id = scene.add_entity(far);

        let mut hud = EntityInstance::new("Hud");
        hud.add_native_component(&Transform::at(-3.0, 8.0)).unwrap();
        let hud_id = scene.add_entity(hud);

        let mut near_region = SceneRegion::new(
            "NearRegion",
            StreamingBounds::from_center_radius([0.0, 0.0, 0.0], 8.0),
        );
        near_region.add_entity(near_id);
        let near_region_id = scene.add_streaming_region(near_region);

        let mut far_region = SceneRegion::new(
            "FarRegion",
            StreamingBounds::from_center_radius([100.0, 0.0, 0.0], 8.0),
        );
        far_region.add_entity(far_id);
        let far_region_id = scene.add_streaming_region(far_region);

        let plan = scene
            .build_stream_plan(&[SceneStreamFocus::new([0.0, 0.0, 0.0], 12.0)])
            .unwrap();

        assert!(plan.includes_region(near_region_id));
        assert!(!plan.includes_region(far_region_id));
        assert!(plan.includes_entity(near_id));
        assert!(plan.includes_entity(hud_id));
        assert!(!plan.includes_entity(far_id));
    }

    #[test]
    fn test_scene_stream_plan_includes_parent_and_reference_dependencies() {
        let mut scene = Scene::new("StreamingDependencies");

        let mut parent = EntityInstance::new("Parent");
        parent
            .add_native_component(&Transform3D {
                position: Vec3::new(20.0, 0.0, 0.0),
                ..Default::default()
            })
            .unwrap();
        let parent_id = scene.add_entity(parent);

        let mut target = EntityInstance::new("Target");
        target
            .add_native_component(&Transform3D {
                position: Vec3::new(22.0, 0.0, 0.0),
                ..Default::default()
            })
            .unwrap();
        let target_id = scene.add_entity(target);

        let mut camera = EntityInstance::new("Camera");
        camera
            .add_native_component(&Transform3D {
                position: Vec3::new(0.0, 0.0, 0.0),
                ..Default::default()
            })
            .unwrap();
        camera.add_native_component(&Camera3D::default()).unwrap();
        camera
            .add_native_component(&FollowCameraController::default())
            .unwrap();
        camera.add_entity_reference_by_id("FollowCameraController", "target", target_id);
        let camera_id = scene.add_entity(camera);

        scene.graph.set_parent(camera_id, parent_id);

        let mut near_region = SceneRegion::new(
            "NearRegion",
            StreamingBounds::from_center_radius([0.0, 0.0, 0.0], 5.0),
        );
        near_region.add_entity(camera_id);
        scene.add_streaming_region(near_region);

        let mut far_region = SceneRegion::new(
            "FarRegion",
            StreamingBounds::from_center_radius([20.0, 0.0, 0.0], 5.0),
        );
        far_region.add_entity(parent_id);
        far_region.add_entity(target_id);
        scene.add_streaming_region(far_region);

        let plan = scene
            .build_stream_plan(&[SceneStreamFocus::new([0.0, 0.0, 0.0], 6.0)])
            .unwrap();

        assert!(plan.includes_entity(camera_id));
        assert!(plan.includes_entity(parent_id));
        assert!(plan.includes_entity(target_id));
    }

    #[test]
    fn test_scene_instantiate_streamed_only_spawns_active_entities() {
        let mut scene = Scene::new("InstantiateStreamedScene");

        let mut near = EntityInstance::new("Near");
        near.add_native_component(&Transform::at(1.0, 1.0)).unwrap();
        let near_id = scene.add_entity(near);

        let mut far = EntityInstance::new("Far");
        far.add_native_component(&Transform::at(50.0, 1.0)).unwrap();
        let far_id = scene.add_entity(far);

        let mut hud = EntityInstance::new("Hud");
        hud.add_native_component(&Transform::at(-4.0, 8.0)).unwrap();
        let hud_id = scene.add_entity(hud);

        let mut near_region = SceneRegion::new(
            "NearRegion",
            StreamingBounds::from_center_radius([0.0, 0.0, 0.0], 10.0),
        );
        near_region.add_entity(near_id);
        scene.add_streaming_region(near_region);

        let mut far_region = SceneRegion::new(
            "FarRegion",
            StreamingBounds::from_center_radius([50.0, 0.0, 0.0], 10.0),
        );
        far_region.add_entity(far_id);
        scene.add_streaming_region(far_region);

        let mut world = World::new(11);
        let result = scene
            .instantiate_streamed(
                &mut world,
                &[SceneStreamFocus::new([0.0, 0.0, 0.0], 12.0)],
                None,
            )
            .unwrap();

        assert_eq!(world.entity_count(), 2);
        assert!(result.entity_for(near_id).is_some());
        assert!(result.entity_for(hud_id).is_some());
        assert!(result.entity_for(far_id).is_none());
    }

    #[test]
    fn test_scene_stream_plan_rejects_missing_region_entities() {
        let mut scene = Scene::new("BrokenStreamingScene");
        let mut region = SceneRegion::new(
            "BrokenRegion",
            StreamingBounds::from_center_radius([0.0, 0.0, 0.0], 5.0),
        );
        region.add_entity(Uuid::new_v4());
        scene.add_streaming_region(region);

        let err = scene
            .build_stream_plan(&[SceneStreamFocus::new([0.0, 0.0, 0.0], 10.0)])
            .expect_err("stream plan should fail when a region references a missing entity");
        assert!(err.contains("BrokenRegion"));
    }
}
