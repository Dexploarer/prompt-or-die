//! glTF 2.0 importer for render-ready mesh + material data.
//!
//! This module intentionally keeps to a small, stable vertex schema that matches
//! the renderer's current native pipeline:
//! - `position`: vec3
//! - `normal`: vec3
//! - `color`: vec4 (from vertex colors, fallback white)

use glam::Vec3;
use gltf::mesh::Mode;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Vertex format extracted from glTF primitives.
#[derive(Debug, Clone, PartialEq)]
pub struct GltfVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

/// One imported mesh primitive from a glTF file.
#[derive(Debug, Clone)]
pub struct GltfMesh {
    pub name: String,
    pub material: Option<usize>,
    pub vertices: Vec<GltfVertex>,
    pub indices: Vec<u32>,
}

/// Minimal material subset used by the renderer.
#[derive(Debug, Clone)]
pub struct GltfMaterial {
    pub index: usize,
    pub name: String,
    pub base_color: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: [f32; 3],
    pub alpha_mode: String,
    pub double_sided: bool,
}

/// Imported glTF payload.
#[derive(Debug, Clone)]
pub struct GltfAsset {
    pub meshes: HashMap<String, GltfMesh>,
    pub materials: Vec<GltfMaterial>,
}

/// Errors produced while reading glTF documents.
#[derive(Debug)]
pub enum GltfImportError {
    Parse(gltf::Error),
    MissingPositionAttribute {
        mesh_name: String,
    },
    UnsupportedPrimitiveMode {
        mesh_name: String,
        mode: String,
    },
    IndexOutOfRange {
        mesh_name: String,
        index: u32,
        vertex_count: usize,
    },
}

impl fmt::Display for GltfImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "Failed to parse glTF document: {err}"),
            Self::MissingPositionAttribute { mesh_name } => {
                write!(
                    f,
                    "glTF mesh primitive is missing POSITION attribute: {mesh_name}"
                )
            }
            Self::UnsupportedPrimitiveMode { mesh_name, mode } => write!(
                f,
                "Unsupported primitive mode for mesh `{mesh_name}`: {mode} (expected TRIANGLES)"
            ),
            Self::IndexOutOfRange {
                mesh_name,
                index,
                vertex_count,
            } => write!(
                f,
                "glTF index out of range in mesh `{mesh_name}`: {index} >= {vertex_count}"
            ),
        }
    }
}

impl std::error::Error for GltfImportError {}

impl From<gltf::Error> for GltfImportError {
    fn from(value: gltf::Error) -> Self {
        Self::Parse(value)
    }
}

/// Import a glTF file into renderer-friendly mesh and material data.
pub fn import_gltf_asset(path: &Path) -> Result<GltfAsset, GltfImportError> {
    let (document, buffers, _) = gltf::import(path)?;

    let materials = document
        .materials()
        .enumerate()
        .map(|(index, material)| {
            let material_name = material
                .name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| format!("material_{index}"));
            let pbr = material.pbr_metallic_roughness();
            let alpha_mode = format!("{:?}", material.alpha_mode());

            GltfMaterial {
                index,
                name: material_name,
                base_color: pbr.base_color_factor(),
                roughness: pbr.roughness_factor(),
                metallic: pbr.metallic_factor(),
                emissive: material.emissive_factor(),
                alpha_mode,
                double_sided: material.double_sided(),
            }
        })
        .collect();

    let mut meshes = HashMap::new();

    for mesh in document.meshes() {
        let mesh_name = mesh
            .name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| "mesh".to_string());

        for (primitive_index, primitive) in mesh.primitives().enumerate() {
            if primitive.mode() != Mode::Triangles {
                return Err(GltfImportError::UnsupportedPrimitiveMode {
                    mesh_name: mesh_name.clone(),
                    mode: format!("{:?}", primitive.mode()),
                });
            }

            let reader = primitive.reader(|buffer| {
                let index = buffer.index();
                Some(&buffers[index])
            });

            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(positions) => positions.collect(),
                None => {
                    return Err(GltfImportError::MissingPositionAttribute {
                        mesh_name: mesh_name.clone(),
                    });
                }
            };

            let normals = reader.read_normals().map_or_else(
                || {
                    let fallback = Vec3::Z;
                    vec![fallback.to_array(); positions.len()]
                },
                |values| {
                    values
                        .map(Vec3::from)
                        .map(|normal| normal.to_array())
                        .collect()
                },
            );

            let colors = reader.read_colors(0).map_or_else(
                || vec![[1.0, 1.0, 1.0, 1.0]; positions.len()],
                |values| values.into_rgba_f32().collect(),
            );

            let indices: Vec<u32> = match reader.read_indices() {
                Some(values) => values.into_u32().collect::<Vec<u32>>(),
                None => (0..positions.len() as u32).collect(),
            };

            for index in &indices {
                if (*index as usize) >= positions.len() {
                    return Err(GltfImportError::IndexOutOfRange {
                        mesh_name: mesh_name.clone(),
                        index: *index,
                        vertex_count: positions.len(),
                    });
                }
            }

            let mut vertices = Vec::with_capacity(positions.len());

            for i in 0..positions.len() {
                vertices.push(GltfVertex {
                    position: positions[i],
                    normal: normals[i],
                    color: colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]),
                });
            }

            meshes.insert(
                format!("{mesh_name}:{primitive_index}"),
                GltfMesh {
                    name: format!("{mesh_name}:{primitive_index}"),
                    material: primitive.material().index().map(|index| index as usize),
                    vertices,
                    indices,
                },
            );
        }
    }

    Ok(GltfAsset { meshes, materials })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn append_f32(buffer: &mut Vec<u8>, value: f32) {
        buffer.extend_from_slice(&value.to_le_bytes());
    }

    fn write_test_gltf_assets() -> GltfAsset {
        let mut folder = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid system time")
            .as_nanos();
        folder.push(format!("pod_render_gltf_{nanos}"));

        fs::create_dir_all(&folder).expect("create test folder");
        let bin_path = folder.join("triangle.bin");
        let gltf_path = folder.join("triangle.gltf");

        let mut binary = Vec::new();
        let positions = [(-0.5f32, 0.0, 0.0), (0.5, 0.0, 0.0), (0.0, 0.5, 0.0)];
        let normals = [(0.0f32, 0.0, 1.0); 3];
        let indices: [u16; 3] = [0, 1, 2];

        for (x, y, z) in positions {
            append_f32(&mut binary, x);
            append_f32(&mut binary, y);
            append_f32(&mut binary, z);
        }

        for (x, y, z) in normals {
            append_f32(&mut binary, x);
            append_f32(&mut binary, y);
            append_f32(&mut binary, z);
        }

        for index in indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }

        while binary.len() % 4 != 0 {
            binary.push(0);
        }

        fs::write(&bin_path, binary).expect("write binary");

        let document = json!({
            "asset": { "version": "2.0", "generator": "pod-render-tests" },
            "scenes": [{ "nodes": [0] }],
            "scene": 0,
            "nodes": [{ "mesh": 0 }],
            "buffers": [{ "uri": "triangle.bin", "byteLength": 80 }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 72, "byteLength": 6 }
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "componentType": 5126,
                    "count": 3,
                    "type": "VEC3",
                    "max": [0.5, 0.5, 0.0],
                    "min": [-0.5, 0.0, 0.0]
                },
                {
                    "bufferView": 1,
                    "componentType": 5126,
                    "count": 3,
                    "type": "VEC3"
                },
                {
                    "bufferView": 2,
                    "componentType": 5123,
                    "count": 3,
                    "type": "SCALAR"
                }
            ],
            "materials": [
                {
                    "name": "test_material",
                    "pbrMetallicRoughness": {
                        "baseColorFactor": [0.1, 0.8, 0.2, 1.0],
                        "roughnessFactor": 0.5,
                        "metallicFactor": 0.1
                    }
                }
            ],
            "meshes": [
                {
                    "name": "triangle_mesh",
                    "primitives": [
                        {
                            "attributes": { "POSITION": 0, "NORMAL": 1 },
                            "indices": 2,
                            "material": 0,
                            "mode": 4
                        }
                    ]
                }
            ]
        });

        let mut gltf_file = fs::File::create(&gltf_path).expect("create gltf");
        gltf_file
            .write_all(serde_json::to_string_pretty(&document).unwrap().as_bytes())
            .expect("write gltf");

        import_gltf_asset(&gltf_path).expect("import test gltf")
    }

    #[test]
    fn imports_triangle_gltf_mesh() {
        let asset = write_test_gltf_assets();

        let mesh_key = "triangle_mesh:0".to_string();
        let mesh = asset.meshes.get(&mesh_key).expect("triangle mesh exists");

        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices, vec![0, 1, 2]);
        assert_eq!(mesh.material, Some(0));
    }

    #[test]
    fn imports_gltf_materials() {
        let asset = write_test_gltf_assets();

        let material = &asset.materials[0];
        assert_eq!(material.name, "test_material");
        assert_eq!(material.base_color[1], 0.8);
        assert_eq!(material.roughness, 0.5);
        assert_eq!(material.metallic, 0.1);
    }
}
