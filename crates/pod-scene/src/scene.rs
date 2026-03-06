use hecs::Entity;
use pod_core::{Parent3D, Transform3D, World};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::binding::{insert_bound_components, NativeComponentBinding};
use crate::prefab::{PrefabComponent, PrefabRegistry};

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

/// Entity instance in a scene with all its component data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityInstance {
    pub id: Uuid,
    pub name: String,
    pub prefab_ref: Option<String>, // Reference to a prefab, if this entity was created from one
    pub components: HashMap<String, PrefabComponent>,
}

impl EntityInstance {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            prefab_ref: None,
            components: HashMap::new(),
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
}

#[derive(Debug, Clone)]
pub struct SceneSpawnResult {
    pub entity_map: HashMap<Uuid, Entity>,
    pub ignored_components: Vec<String>,
}

impl SceneSpawnResult {
    pub fn entity_for(&self, scene_entity_id: Uuid) -> Option<Entity> {
        self.entity_map.get(&scene_entity_id).copied()
    }
}

/// Complete scene representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub metadata: SceneMetadata,
    pub entities: Vec<EntityInstance>,
    pub spawn_points: Vec<SpawnPoint>,
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

    /// Serialize scene to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
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
        self.instantiate_with_prefabs(world, None)
    }

    pub fn instantiate_with_prefabs(
        &self,
        world: &mut World,
        prefabs: Option<&PrefabRegistry>,
    ) -> Result<SceneSpawnResult, String> {
        let mut entity_map = HashMap::new();

        for entity in &self.entities {
            entity_map.insert(entity.id, world.ecs.spawn(()));
        }

        let mut ignored_components = Vec::new();

        for entity in &self.entities {
            let spawned_entity = entity_map
                .get(&entity.id)
                .copied()
                .ok_or_else(|| format!("Scene entity '{}' was not pre-spawned", entity.name))?;

            let resolved_components = self.resolve_entity_components(entity, prefabs)?;
            let ignored =
                insert_bound_components(&resolved_components, &mut world.ecs, spawned_entity)?;
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
        })
    }

    fn resolve_entity_components(
        &self,
        entity: &EntityInstance,
        prefabs: Option<&PrefabRegistry>,
    ) -> Result<HashMap<String, PrefabComponent>, String> {
        let mut components = match entity.prefab_ref.as_deref() {
            Some(prefab_name) => {
                let registry = prefabs.ok_or_else(|| {
                    format!(
                        "Scene '{}' requires prefab registry to resolve '{}'",
                        self.metadata.name, prefab_name
                    )
                })?;
                registry.resolve_prefab(prefab_name)?.components.clone()
            }
            None => HashMap::new(),
        };

        for (name, component) in &entity.components {
            components.insert(name.clone(), component.clone());
        }

        Ok(components)
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
            let child_entity = entity_map.get(&child_id).copied().ok_or_else(|| {
                format!("Missing spawned child entity for scene node {}", child_id)
            })?;
            let parent_entity = entity_map.get(&parent_id).copied().ok_or_else(|| {
                format!("Missing spawned parent entity for scene node {}", parent_id)
            })?;

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
    use pod_core::{Label, Material, Mesh, Parent3D, Sprite, Team, Transform, Transform3D};

    use crate::prefab::Prefab;

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
}
