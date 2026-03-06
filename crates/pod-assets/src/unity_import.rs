use crate::AssetImportError;
use glam::{Vec2, Vec3};
use pod_core::{Sprite, Transform, Transform3D};
use pod_scene::{EntityInstance, Scene};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Default)]
struct UnityDocumentSet {
    game_objects: HashMap<i64, UnityGameObjectRecord>,
    transforms: HashMap<i64, UnityTransformRecord>,
    sprite_renderers: Vec<UnitySpriteRendererRecord>,
}

#[derive(Debug)]
struct UnityGameObjectRecord {
    file_id: i64,
    name: String,
}

#[derive(Debug)]
struct UnityTransformRecord {
    game_object: i64,
    parent_transform: Option<i64>,
    position: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

#[derive(Debug)]
struct UnitySpriteRendererRecord {
    game_object: i64,
    sprite_guid: Option<String>,
    color: [f32; 4],
}

pub fn import_unity_scene(source_path: impl AsRef<Path>) -> Result<Scene, AssetImportError> {
    let source_path = source_path
        .as_ref()
        .canonicalize()
        .map_err(AssetImportError::from)?;
    let document = fs::read_to_string(&source_path)?;
    parse_unity_scene_document(&document, &source_path)
}

fn parse_unity_scene_document(
    document: &str,
    source_path: &Path,
) -> Result<Scene, AssetImportError> {
    let parsed = parse_unity_documents(document);
    let scene_name = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported_scene");
    let mut scene = Scene::new(scene_name);
    scene.metadata.description =
        format!("Imported from Unity text scene {}", source_path.display());
    scene.metadata.tags = vec![
        "imported".to_string(),
        "unity".to_string(),
        "2d".to_string(),
    ];

    let mut game_object_ids = HashMap::<i64, uuid::Uuid>::new();
    let mut game_object_keys: Vec<i64> = parsed.game_objects.keys().copied().collect();
    game_object_keys.sort_unstable();
    for game_object_id in game_object_keys {
        let game_object = &parsed.game_objects[&game_object_id];
        let entity_id = scene.add_entity(EntityInstance::new(game_object.name.clone()));
        game_object_ids.insert(game_object.file_id, entity_id);
    }

    let mut transform_keys: Vec<i64> = parsed.transforms.keys().copied().collect();
    transform_keys.sort_unstable();
    for transform_id in &transform_keys {
        let transform = &parsed.transforms[transform_id];
        let Some(entity_id) = game_object_ids.get(&transform.game_object).copied() else {
            continue;
        };
        let entity = scene
            .get_entity_mut(entity_id)
            .ok_or_else(|| unity_import_error(source_path, "missing Unity scene entity"))?;
        if should_use_transform3d(transform) {
            entity
                .add_native_component(&Transform3D {
                    position: Vec3::new(
                        transform.position[0],
                        transform.position[1],
                        transform.position[2],
                    ),
                    rotation: transform.rotation,
                    scale: Vec3::new(transform.scale[0], transform.scale[1], transform.scale[2]),
                })
                .map_err(|message| unity_import_error(source_path, message))?;
        } else {
            let mut component = Transform::at(transform.position[0], transform.position[1]);
            component.rotation = quaternion_to_2d_radians(transform.rotation);
            component.scale = Vec2::new(transform.scale[0], transform.scale[1]);
            entity
                .add_native_component(&component)
                .map_err(|message| unity_import_error(source_path, message))?;
        }
    }

    for sprite_renderer in &parsed.sprite_renderers {
        let Some(entity_id) = game_object_ids.get(&sprite_renderer.game_object).copied() else {
            continue;
        };
        let entity = scene
            .get_entity_mut(entity_id)
            .ok_or_else(|| unity_import_error(source_path, "missing Unity sprite entity"))?;
        entity
            .add_native_component(&Sprite {
                texture: sprite_renderer
                    .sprite_guid
                    .as_ref()
                    .map(|guid| format!("unity-guid:{guid}"))
                    .unwrap_or_default(),
                frame: 0,
                layer: 0,
                color: sprite_renderer.color,
                visible: sprite_renderer.color[3] > 0.0,
            })
            .map_err(|message| unity_import_error(source_path, message))?;
    }

    for transform_id in transform_keys {
        let transform = &parsed.transforms[&transform_id];
        let Some(parent_transform_id) = transform.parent_transform else {
            continue;
        };
        let Some(parent_transform) = parsed.transforms.get(&parent_transform_id) else {
            continue;
        };
        let Some(child_entity_id) = game_object_ids.get(&transform.game_object).copied() else {
            continue;
        };
        let Some(parent_entity_id) = game_object_ids.get(&parent_transform.game_object).copied()
        else {
            continue;
        };
        scene.graph.set_parent(child_entity_id, parent_entity_id);
    }

    Ok(scene)
}

fn parse_unity_documents(document: &str) -> UnityDocumentSet {
    let mut parsed = UnityDocumentSet::default();
    let mut current_file_id = None::<i64>;
    let mut current_type = String::new();
    let mut current_lines = Vec::<String>::new();

    for raw_line in document.lines() {
        let line = raw_line.trim_end();
        if let Some(file_id) = parse_unity_document_header(line) {
            flush_unity_document(
                &mut parsed,
                current_file_id.take(),
                &current_type,
                &current_lines,
            );
            current_file_id = Some(file_id);
            current_type.clear();
            current_lines.clear();
            continue;
        }

        if current_file_id.is_none() || line.starts_with('%') || line.trim().is_empty() {
            continue;
        }

        if current_type.is_empty() {
            if let Some(object_type) = line.trim().strip_suffix(':') {
                current_type = object_type.to_string();
            }
            continue;
        }

        current_lines.push(line.to_string());
    }

    flush_unity_document(&mut parsed, current_file_id, &current_type, &current_lines);
    parsed
}

fn flush_unity_document(
    parsed: &mut UnityDocumentSet,
    file_id: Option<i64>,
    object_type: &str,
    lines: &[String],
) {
    let Some(file_id) = file_id else {
        return;
    };

    match object_type {
        "GameObject" => {
            let name = lines
                .iter()
                .find_map(|line| line.trim().strip_prefix("m_Name: "))
                .unwrap_or("GameObject")
                .to_string();
            parsed
                .game_objects
                .insert(file_id, UnityGameObjectRecord { file_id, name });
        }
        "Transform" | "RectTransform" => {
            let game_object = lines
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("m_GameObject: ")
                        .and_then(parse_unity_file_id_ref)
                })
                .unwrap_or_default();
            let parent_transform = lines.iter().find_map(|line| {
                line.trim()
                    .strip_prefix("m_Father: ")
                    .and_then(parse_unity_file_id_ref)
                    .filter(|value| *value != 0)
            });
            let position = lines
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("m_LocalPosition: ")
                        .and_then(parse_unity_vec3)
                })
                .unwrap_or([0.0, 0.0, 0.0]);
            let rotation = lines
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("m_LocalRotation: ")
                        .and_then(parse_unity_quaternion)
                })
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let scale = lines
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("m_LocalScale: ")
                        .and_then(parse_unity_vec3)
                })
                .unwrap_or([1.0, 1.0, 1.0]);
            parsed.transforms.insert(
                file_id,
                UnityTransformRecord {
                    game_object,
                    parent_transform,
                    position,
                    rotation,
                    scale,
                },
            );
        }
        "SpriteRenderer" => {
            let game_object = lines
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("m_GameObject: ")
                        .and_then(parse_unity_file_id_ref)
                })
                .unwrap_or_default();
            let sprite_guid = lines.iter().find_map(|line| {
                line.trim()
                    .strip_prefix("m_Sprite: ")
                    .and_then(parse_unity_guid)
            });
            let color = lines
                .iter()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("m_Color: ")
                        .and_then(parse_unity_color)
                })
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            parsed.sprite_renderers.push(UnitySpriteRendererRecord {
                game_object,
                sprite_guid,
                color,
            });
        }
        _ => {}
    }
}

fn parse_unity_document_header(line: &str) -> Option<i64> {
    let file_id = line.trim().strip_prefix("--- !u!")?.split('&').nth(1)?;
    file_id.trim().parse::<i64>().ok()
}

fn parse_unity_file_id_ref(raw_value: &str) -> Option<i64> {
    let value = raw_value
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?
        .split(',')
        .find_map(|segment| segment.trim().strip_prefix("fileID: "))?;
    value.trim().parse::<i64>().ok()
}

fn parse_unity_guid(raw_value: &str) -> Option<String> {
    raw_value
        .trim()
        .strip_prefix('{')?
        .strip_suffix('}')?
        .split(',')
        .find_map(|segment| segment.trim().strip_prefix("guid: "))
        .map(|value| value.trim().to_string())
}

fn parse_unity_vec3(raw_value: &str) -> Option<[f32; 3]> {
    let values = parse_unity_inline_f32_map(raw_value)?;
    Some([*values.get("x")?, *values.get("y")?, *values.get("z")?])
}

fn parse_unity_quaternion(raw_value: &str) -> Option<[f32; 4]> {
    let values = parse_unity_inline_f32_map(raw_value)?;
    Some([
        *values.get("x")?,
        *values.get("y")?,
        *values.get("z")?,
        *values.get("w")?,
    ])
}

fn parse_unity_color(raw_value: &str) -> Option<[f32; 4]> {
    let values = parse_unity_inline_f32_map(raw_value)?;
    Some([
        *values.get("r")?,
        *values.get("g")?,
        *values.get("b")?,
        *values.get("a")?,
    ])
}

fn parse_unity_inline_f32_map(raw_value: &str) -> Option<HashMap<String, f32>> {
    let inner = raw_value.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut values = HashMap::new();
    for entry in inner.split(',') {
        let (key, value) = entry.trim().split_once(':')?;
        values.insert(key.trim().to_string(), value.trim().parse::<f32>().ok()?);
    }
    Some(values)
}

fn quaternion_to_2d_radians(rotation: [f32; 4]) -> f32 {
    2.0 * rotation[2].atan2(rotation[3])
}

fn should_use_transform3d(transform: &UnityTransformRecord) -> bool {
    transform.position[2].abs() > f32::EPSILON
        || (transform.scale[2] - 1.0).abs() > f32::EPSILON
        || transform.rotation[0].abs() > f32::EPSILON
        || transform.rotation[1].abs() > f32::EPSILON
}

fn unity_import_error(source_path: &Path, message: impl Into<String>) -> AssetImportError {
    AssetImportError::SceneImport {
        path: source_path.to_path_buf(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{import_asset, normalize_asset_id, AssetCache, AssetFormat};
    use std::path::PathBuf;

    fn temp_file_path(name: &str) -> PathBuf {
        let mut base = std::env::temp_dir();
        base.push(format!("{}_{}", uuid::Uuid::new_v4(), name));
        base
    }

    #[test]
    fn import_unity_scene_preserves_hierarchy_and_native_components() {
        let source = temp_file_path("sample_scene.unity");
        fs::write(
            &source,
            r#"%YAML 1.1
%TAG !u! tag:unity3d.com,2011:
--- !u!1 &1000
GameObject:
  m_Name: Player
--- !u!4 &1001
Transform:
  m_GameObject: {fileID: 1000}
  m_LocalRotation: {x: 0, y: 0, z: 0.3826834, w: 0.9238795}
  m_LocalPosition: {x: 1, y: 2, z: 0}
  m_LocalScale: {x: 1.5, y: 2, z: 1}
  m_Father: {fileID: 0}
--- !u!212 &1002
SpriteRenderer:
  m_GameObject: {fileID: 1000}
  m_Sprite: {fileID: 21300000, guid: abc123, type: 3}
  m_Color: {r: 1, g: 0.5, b: 0.25, a: 1}
--- !u!1 &2000
GameObject:
  m_Name: Billboard
--- !u!4 &2001
Transform:
  m_GameObject: {fileID: 2000}
  m_LocalRotation: {x: 0, y: 0.258819, z: 0, w: 0.9659258}
  m_LocalPosition: {x: 5, y: 6, z: 3}
  m_LocalScale: {x: 1, y: 1, z: 1}
  m_Father: {fileID: 1001}
--- !u!212 &2002
SpriteRenderer:
  m_GameObject: {fileID: 2000}
  m_Sprite: {fileID: 21300000, guid: def456, type: 3}
  m_Color: {r: 0.25, g: 0.75, b: 1, a: 0.5}
"#,
        )
        .unwrap();

        let scene = import_unity_scene(&source).expect("unity scene should import");
        let player = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Player")
            .expect("player should exist");
        let billboard = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Billboard")
            .expect("billboard should exist");

        let player_transform = player
            .get_native_component::<Transform>()
            .expect("transform should deserialize")
            .expect("2D transform should exist");
        assert!(player_transform
            .position
            .abs_diff_eq(Vec2::new(1.0, 2.0), 1e-6));
        assert!(player_transform
            .scale
            .abs_diff_eq(Vec2::new(1.5, 2.0), 1e-6));
        assert!((player_transform.rotation - std::f32::consts::FRAC_PI_4).abs() < 1e-4);

        let player_sprite = player
            .get_native_component::<Sprite>()
            .expect("sprite should deserialize")
            .expect("sprite should exist");
        assert_eq!(player_sprite.texture, "unity-guid:abc123");

        let billboard_transform = billboard
            .get_native_component::<Transform3D>()
            .expect("3D transform should deserialize")
            .expect("3D transform should exist");
        assert!(billboard_transform
            .position
            .abs_diff_eq(Vec3::new(5.0, 6.0, 3.0), 1e-6));

        let billboard_sprite = billboard
            .get_native_component::<Sprite>()
            .expect("sprite should deserialize")
            .expect("sprite should exist");
        assert_eq!(billboard_sprite.texture, "unity-guid:def456");
        assert_eq!(scene.graph.get_parent(billboard.id), Some(player.id));
    }

    #[test]
    fn import_asset_writes_serialized_scene_artifact_for_unity_prefabs() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("enemy.prefab");
        let output_root = temp_file_path("import-root-unity");
        fs::write(
            &source,
            r#"%YAML 1.1
%TAG !u! tag:unity3d.com,2011:
--- !u!1 &1000
GameObject:
  m_Name: Enemy
--- !u!4 &1001
Transform:
  m_GameObject: {fileID: 1000}
  m_LocalRotation: {x: 0, y: 0, z: 0, w: 1}
  m_LocalPosition: {x: 2, y: 4, z: 0}
  m_LocalScale: {x: 1, y: 1, z: 1}
  m_Father: {fileID: 0}
--- !u!212 &1002
SpriteRenderer:
  m_GameObject: {fileID: 1000}
  m_Sprite: {fileID: 21300000, guid: feedbeef, type: 3}
  m_Color: {r: 1, g: 1, b: 1, a: 1}
"#,
        )
        .unwrap();

        assert_eq!(
            AssetFormat::from_path(&source),
            Some(AssetFormat::UnityScene)
        );
        let import =
            import_asset(&mut cache, &source, &output_root).expect("import should succeed");
        let imported_scene: Scene =
            serde_json::from_slice(&fs::read(&import.imported_path).unwrap())
                .expect("artifact should deserialize");
        assert!(imported_scene.metadata.name.ends_with("enemy"));
        assert!(imported_scene
            .entities
            .iter()
            .any(|entity| entity.name == "Enemy"));
        assert_eq!(cache.total(), 1);
        assert_eq!(normalize_asset_id(&source), import.id);
    }
}
