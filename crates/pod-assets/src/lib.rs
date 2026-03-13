//! # pod-assets — content-addressed asset pipeline primitives
//!
//! Provides shared types and a content-addressed cache for asset import
//! workflows (glTF, textures, scene sources, and generated runtime assets).

#![allow(clippy::bool_comparison)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::needless_late_init)]
#![allow(clippy::unnecessary_cast)]
#![allow(unused_mut)]

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pod_core::{ColorRect, Sprite, Transform};
use pod_scene::{EntityInstance, Scene};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::SystemTime;

use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

mod unity_import;

pub use unity_import::import_unity_scene;

/// Canonical asset identifier.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub String);

impl AssetId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_filename(&self) -> String {
        self.0.replace(':', "_")
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AssetId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for AssetId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Asset loading state tracked by higher-level import pipelines.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum AssetState {
    /// Metadata exists, content not yet imported.
    Pending,
    /// Import succeeded and runtime data is ready.
    Ready,
    /// Import or validation failed.
    Failed(String),
}

impl Default for AssetState {
    fn default() -> Self {
        Self::Pending
    }
}

impl fmt::Display for AssetState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Ready => write!(f, "ready"),
            Self::Failed(reason) => write!(f, "failed: {reason}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AssetFormat {
    Gltf,
    Obj,
    Png,
    Jpeg,
    Ktx2,
    Svg,
    GodotScene,
    TiledMap,
    UnityScene,
}

impl fmt::Display for AssetFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gltf => write!(f, "gltf"),
            Self::Obj => write!(f, "obj"),
            Self::Png => write!(f, "png"),
            Self::Jpeg => write!(f, "jpeg"),
            Self::Ktx2 => write!(f, "ktx2"),
            Self::Svg => write!(f, "svg"),
            Self::GodotScene => write!(f, "godot_scene"),
            Self::TiledMap => write!(f, "tiled_map"),
            Self::UnityScene => write!(f, "unity_scene"),
        }
    }
}

impl AssetFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension().and_then(|ext| ext.to_str())?;
        match extension.to_lowercase().as_str() {
            "gltf" | "glb" => Some(Self::Gltf),
            "obj" => Some(Self::Obj),
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "ktx2" => Some(Self::Ktx2),
            "svg" => Some(Self::Svg),
            "tscn" => Some(Self::GodotScene),
            "tmj" => Some(Self::TiledMap),
            "unity" | "prefab" => Some(Self::UnityScene),
            _ => None,
        }
    }

    pub fn imported_extension(&self, source_path: &Path) -> String {
        match self {
            Self::Gltf | Self::Jpeg => source_path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_lowercase())
                .unwrap_or_else(|| match self {
                    Self::Gltf => "gltf".to_string(),
                    Self::Jpeg => "jpeg".to_string(),
                    _ => unreachable!("handled by explicit match arm"),
                }),
            Self::Obj => "obj".to_string(),
            Self::Png => "png".to_string(),
            Self::Ktx2 => "ktx2".to_string(),
            Self::Svg => "svg".to_string(),
            Self::GodotScene => "scene.json".to_string(),
            Self::TiledMap => "scene.json".to_string(),
            Self::UnityScene => "scene.json".to_string(),
        }
    }

    pub fn import_prefix(&self) -> &'static str {
        match self {
            Self::Gltf => "gltf",
            Self::Obj => "obj",
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Ktx2 => "ktx2",
            Self::Svg => "svg",
            Self::GodotScene => "scene",
            Self::TiledMap => "scene",
            Self::UnityScene => "scene",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TextureCompressionProfile {
    None,
    Fast,
    Balanced,
    High,
}

impl Default for TextureCompressionProfile {
    fn default() -> Self {
        Self::Balanced
    }
}

impl fmt::Display for TextureCompressionProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Fast => write!(f, "fast"),
            Self::Balanced => write!(f, "balanced"),
            Self::High => write!(f, "high"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TexturePlacement {
    pub id: AssetId,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl TexturePlacement {
    fn right(&self) -> u32 {
        self.x.saturating_add(self.width)
    }

    fn bottom(&self) -> u32 {
        self.y.saturating_add(self.height)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtlasTextureSource {
    pub id: AssetId,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct AtlasPackingOptions {
    pub max_width: u32,
    pub padding: u32,
}

impl Default for AtlasPackingOptions {
    fn default() -> Self {
        Self {
            max_width: 2048,
            padding: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureAtlas {
    pub id: AssetId,
    pub width: u32,
    pub height: u32,
    pub padding: u32,
    pub placements: Vec<TexturePlacement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureCompressionArtifact {
    pub id: AssetId,
    pub format: AssetFormat,
    pub profile: TextureCompressionProfile,
    pub width: u32,
    pub height: u32,
    pub original_size: u64,
    pub compressed_size: u64,
    pub bytes: Vec<u8>,
}

/// Lightweight catalog record for a discovered asset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: AssetId,
    pub source_path: PathBuf,
    pub imported_path: PathBuf,
    pub checksum: String,
    pub byte_len: u64,
    pub state: AssetState,
    pub imported_at_unix: u64,
}

impl AssetRecord {
    pub fn new(
        id: impl Into<AssetId>,
        source_path: PathBuf,
        imported_path: PathBuf,
        checksum: String,
        byte_len: u64,
    ) -> Self {
        Self {
            id: id.into(),
            source_path,
            imported_path,
            checksum,
            byte_len,
            state: AssetState::Pending,
            imported_at_unix: now_unix_seconds(),
        }
    }

    pub fn with_state(mut self, state: AssetState) -> Self {
        self.state = state;
        self
    }
}

/// Errors produced by asset hashing/indexing operations.
#[derive(Debug)]
pub enum AssetCacheError {
    Io(std::io::Error),
    InvalidSourcePath(PathBuf),
}

impl fmt::Display for AssetCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "asset cache io error: {err}"),
            Self::InvalidSourcePath(path) => {
                write!(f, "asset source path could not be canonicalized: {path:?}")
            }
        }
    }
}

impl std::error::Error for AssetCacheError {}

impl From<std::io::Error> for AssetCacheError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[derive(Debug)]
pub enum AssetImportError {
    UnsupportedFormat(PathBuf),
    SceneImport { path: PathBuf, message: String },
    Cache(AssetCacheError),
}

impl fmt::Display for AssetImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedFormat(path) => write!(f, "unsupported asset format: {path:?}"),
            Self::SceneImport { path, message } => {
                write!(f, "scene import failed for {path:?}: {message}")
            }
            Self::Cache(err) => write!(f, "asset import cache error: {err}"),
        }
    }
}

impl std::error::Error for AssetImportError {}

impl From<AssetCacheError> for AssetImportError {
    fn from(err: AssetCacheError) -> Self {
        Self::Cache(err)
    }
}

impl From<std::io::Error> for AssetImportError {
    fn from(err: std::io::Error) -> Self {
        Self::Cache(AssetCacheError::Io(err))
    }
}

#[derive(Debug)]
pub enum RuntimeBundleError {
    InvalidSourcePath(PathBuf),
    MissingImport {
        asset_id: String,
        source_path: PathBuf,
    },
    InvalidRuntimePath(String),
    UnsupportedCompressedTextureFormat {
        asset_id: String,
        format: AssetFormat,
    },
    UnsupportedCompressedMeshFormat {
        asset_id: String,
        format: AssetFormat,
    },
    DuplicateRuntimePath {
        runtime_path: String,
        first_asset_id: String,
        second_asset_id: String,
    },
    Io(std::io::Error),
}

impl fmt::Display for RuntimeBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourcePath(path) => {
                write!(f, "runtime bundle source path could not be canonicalized: {path:?}")
            }
            Self::MissingImport {
                asset_id,
                source_path,
            } => write!(
                f,
                "runtime bundle spec references `{asset_id}` at {:?}, but no staged import matched it",
                source_path
            ),
            Self::InvalidRuntimePath(path) => {
                write!(f, "runtime bundle path is invalid or unsafe: {path}")
            }
            Self::UnsupportedCompressedTextureFormat { asset_id, format } => write!(
                f,
                "runtime bundle compressed texture variant for `{asset_id}` must stage a ktx2 source, got {format}"
            ),
            Self::UnsupportedCompressedMeshFormat { asset_id, format } => write!(
                f,
                "runtime bundle compressed mesh variant for `{asset_id}` must stage a gltf/glb source, got {format}"
            ),
            Self::DuplicateRuntimePath {
                runtime_path,
                first_asset_id,
                second_asset_id,
            } => write!(
                f,
                "runtime bundle output path `{runtime_path}` is declared by both `{first_asset_id}` and `{second_asset_id}`"
            ),
            Self::Io(err) => write!(f, "runtime bundle io error: {err}"),
        }
    }
}

impl std::error::Error for RuntimeBundleError {}

impl From<std::io::Error> for RuntimeBundleError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// File watcher event kind for detected source changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetWatchEventKind {
    Created,
    Modified,
    Removed,
}

/// Event emitted by the watcher when a tracked source file changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetWatchEvent {
    pub path: PathBuf,
    pub kind: AssetWatchEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetHotReloadResult {
    Reprocessed { id: AssetId, import: AssetImport },
    Removed { id: AssetId },
    SkippedUnsupported,
    Failed { message: String },
}

/// Result of processing a single watched asset path.
#[derive(Clone, Debug)]
pub struct AssetHotReloadEvent {
    pub path: PathBuf,
    pub result: AssetHotReloadResult,
}

#[derive(Debug)]
pub enum AssetWatchError {
    Io(std::io::Error),
    Notify(notify::Error),
    LockPoisoned,
    WatchPathMissing(PathBuf),
    CallbackFailed(String),
}

impl fmt::Display for AssetWatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "asset watcher io error: {err}"),
            Self::Notify(err) => write!(f, "asset watcher notify error: {err}"),
            Self::LockPoisoned => write!(f, "asset cache lock is poisoned"),
            Self::WatchPathMissing(path) => {
                write!(f, "asset watch path does not exist: {path:?}")
            }
            Self::CallbackFailed(message) => {
                write!(f, "asset reprocess callback failed: {message}")
            }
        }
    }
}

impl std::error::Error for AssetWatchError {}

impl From<notify::Error> for AssetWatchError {
    fn from(err: notify::Error) -> Self {
        Self::Notify(err)
    }
}

impl From<std::io::Error> for AssetWatchError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

type ReprocessCallback =
    dyn Fn(&mut AssetCache, &Path, &Path) -> Result<AssetImport, AssetImportError> + Send + Sync;

/// Watches source asset paths and triggers re-import when they change on disk.
pub struct AssetHotReloader {
    _watcher: RecommendedWatcher,
    events: mpsc::Receiver<AssetWatchEvent>,
    cache: Arc<Mutex<AssetCache>>,
    output_root: PathBuf,
    reprocess: Arc<ReprocessCallback>,
}

impl AssetHotReloader {
    /// Start a file watcher and route detected source changes through `reprocess`.
    pub fn start<P, I, F>(
        watch_roots: I,
        cache: Arc<Mutex<AssetCache>>,
        output_root: impl AsRef<Path>,
        recursive: bool,
        reprocess: F,
    ) -> Result<Self, AssetWatchError>
    where
        P: AsRef<Path>,
        I: IntoIterator<Item = P>,
        F: Fn(&mut AssetCache, &Path, &Path) -> Result<AssetImport, AssetImportError>
            + Send
            + Sync
            + 'static,
    {
        let (event_tx, event_rx) = mpsc::channel();
        let reprocess = Arc::new(reprocess);
        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| {
                if let Ok(event) = result {
                    let kind = classify_watch_event_kind(&event.kind);
                    if let Some(kind) = kind {
                        for path in &event.paths {
                            if is_supported_asset_path(path) {
                                let _ = event_tx.send(AssetWatchEvent {
                                    path: path.clone(),
                                    kind: kind.clone(),
                                });
                            }
                        }
                    }
                }
            },
            Config::default(),
        )?;
        let watch_mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        let mut has_watch_path = false;
        for watch_path in watch_roots {
            let watch_path = watch_path.as_ref();
            if !watch_path.exists() {
                return Err(AssetWatchError::WatchPathMissing(watch_path.to_path_buf()));
            }
            watcher.watch(watch_path, watch_mode)?;
            has_watch_path = true;
        }
        if !has_watch_path {
            return Err(AssetWatchError::WatchPathMissing(PathBuf::from(".")));
        }

        Ok(Self {
            _watcher: watcher,
            events: event_rx,
            cache,
            output_root: output_root.as_ref().to_path_buf(),
            reprocess,
        })
    }

    /// Process all currently queued file changes up to `max_events`.
    pub fn process_pending(&mut self, max_events: usize) -> Vec<AssetHotReloadEvent> {
        let mut unique_paths = HashSet::new();
        let mut outputs = Vec::new();

        for _ in 0..max_events {
            match self.events.try_recv() {
                Ok(change) => {
                    if unique_paths.insert(change.path.clone()) {
                        outputs.push(self.reprocess_path(change.path.clone()));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => break,
            }
        }

        outputs
    }

    /// Process all queued file changes.
    pub fn process_all_pending(&mut self) -> Vec<AssetHotReloadEvent> {
        self.process_pending(usize::MAX)
    }

    /// Re-run the reprocess callback for a single path.
    pub fn reprocess_path(&self, source_path: impl AsRef<Path>) -> AssetHotReloadEvent {
        let source_path = source_path.as_ref();
        let mut path = source_path.to_path_buf();
        let mut cache_guard = match self.cache.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return AssetHotReloadEvent {
                    path,
                    result: AssetHotReloadResult::Failed {
                        message: AssetWatchError::LockPoisoned.to_string(),
                    },
                };
            }
        };

        if !path.exists() {
            if let Some(record) = cache_guard.get_by_source(&path) {
                let removed_id = record.id.clone();
                cache_guard.remove(&removed_id);
                return AssetHotReloadEvent {
                    path,
                    result: AssetHotReloadResult::Removed { id: removed_id },
                };
            }

            return AssetHotReloadEvent {
                path: path.clone(),
                result: AssetHotReloadResult::Failed {
                    message: format!("source path missing and was not tracked: {path:?}"),
                },
            };
        }

        if !is_supported_asset_path(&path) {
            return AssetHotReloadEvent {
                path,
                result: AssetHotReloadResult::SkippedUnsupported,
            };
        }

        if let Ok(canonical_path) = canonical_source_path(&path) {
            path = canonical_path;
        }

        match (self.reprocess)(&mut cache_guard, &path, &self.output_root) {
            Ok(import) => {
                let id = import.id.clone();
                AssetHotReloadEvent {
                    path,
                    result: AssetHotReloadResult::Reprocessed { id, import },
                }
            }
            Err(err) => AssetHotReloadEvent {
                path,
                result: AssetHotReloadResult::Failed {
                    message: err.to_string(),
                },
            },
        }
    }
}

/// Convenience constructor for the default import callback.
pub fn default_asset_reprocessor(
    cache: &mut AssetCache,
    source_path: &Path,
    output_root: &Path,
) -> Result<AssetImport, AssetImportError> {
    import_asset(cache, source_path, output_root)
}

fn is_supported_asset_path(path: &Path) -> bool {
    AssetFormat::from_path(path).is_some()
}

fn classify_watch_event_kind(event: &EventKind) -> Option<AssetWatchEventKind> {
    match event {
        EventKind::Create(_) => Some(AssetWatchEventKind::Created),
        EventKind::Modify(_) => Some(AssetWatchEventKind::Modified),
        EventKind::Remove(_) => Some(AssetWatchEventKind::Removed),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct TerrainGenerationConfig {
    pub width: usize,
    pub height: usize,
    pub cell_scale_x: f32,
    pub cell_scale_z: f32,
    pub max_height: f32,
    pub base_frequency: f32,
    pub octaves: u8,
    pub persistence: f32,
    pub lacunarity: f32,
    pub seed: u64,
}

impl Default for TerrainGenerationConfig {
    fn default() -> Self {
        Self {
            width: 64,
            height: 64,
            cell_scale_x: 1.0,
            cell_scale_z: 1.0,
            max_height: 8.0,
            base_frequency: 0.05,
            octaves: 4,
            persistence: 0.5,
            lacunarity: 2.0,
            seed: 42,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerrainHeightmap {
    pub width: usize,
    pub height: usize,
    pub cell_scale_x: f32,
    pub cell_scale_z: f32,
    pub max_height: f32,
    pub octaves: u8,
    pub seed: u64,
    pub heights: Vec<f32>,
}

impl TerrainHeightmap {
    pub fn get(&self, x: usize, z: usize) -> f32 {
        self.heights[z * self.width + x]
    }
}

#[derive(Debug)]
pub enum TerrainGenerationError {
    InvalidDimensions { width: usize, height: usize },
    InvalidNoiseConfig(String),
    DegenerateMesh { width: usize, height: usize },
}

impl fmt::Display for TerrainGenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(
                    f,
                    "terrain dimensions must be greater than zero: {width}x{height}"
                )
            }
            Self::InvalidNoiseConfig(message) => {
                write!(f, "invalid terrain noise config: {message}")
            }
            Self::DegenerateMesh { width, height } => {
                write!(
                    f,
                    "terrain mesh requires at least 2x2 samples, got {width}x{height}"
                )
            }
        }
    }
}

impl std::error::Error for TerrainGenerationError {}

/// Create a deterministic procedural terrain heightmap from layered value noise.
pub fn generate_terrain_heightmap(
    config: &TerrainGenerationConfig,
) -> Result<TerrainHeightmap, TerrainGenerationError> {
    if config.width == 0 || config.height == 0 {
        return Err(TerrainGenerationError::InvalidDimensions {
            width: config.width,
            height: config.height,
        });
    }
    if config.max_height < 0.0 {
        return Err(TerrainGenerationError::InvalidNoiseConfig(
            "max_height must be >= 0".to_string(),
        ));
    }
    if config.base_frequency <= 0.0 {
        return Err(TerrainGenerationError::InvalidNoiseConfig(
            "base_frequency must be > 0".to_string(),
        ));
    }
    if config.octaves == 0 {
        return Err(TerrainGenerationError::InvalidNoiseConfig(
            "octaves must be > 0".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&config.persistence) {
        return Err(TerrainGenerationError::InvalidNoiseConfig(
            "persistence must be within 0.0..=1.0".to_string(),
        ));
    }
    if config.lacunarity <= 1.0 {
        return Err(TerrainGenerationError::InvalidNoiseConfig(
            "lacunarity must be > 1".to_string(),
        ));
    }

    let mut heights = Vec::with_capacity(config.width * config.height);
    let width_scale = 1.0 / config.width as f32;
    let height_scale = 1.0 / config.height as f32;

    for z in 0..config.height {
        for x in 0..config.width {
            let noise_x = x as f32 * width_scale;
            let noise_y = z as f32 * height_scale;
            let value = fbm_value_noise(
                noise_x,
                noise_y,
                config.base_frequency,
                config.octaves,
                config.persistence,
                config.lacunarity,
                config.seed,
            );
            let normalized = (value * 0.5 + 0.5).clamp(0.0, 1.0);
            heights.push(normalized * config.max_height);
        }
    }

    Ok(TerrainHeightmap {
        width: config.width,
        height: config.height,
        cell_scale_x: config.cell_scale_x,
        cell_scale_z: config.cell_scale_z,
        max_height: config.max_height,
        octaves: config.octaves,
        seed: config.seed,
        heights,
    })
}

/// Convert a generated terrain heightmap into a triangulated mesh representation.
pub fn generate_terrain_mesh(
    heightmap: &TerrainHeightmap,
) -> Result<TriangleMesh, TerrainGenerationError> {
    if heightmap.width < 2 || heightmap.height < 2 {
        return Err(TerrainGenerationError::DegenerateMesh {
            width: heightmap.width,
            height: heightmap.height,
        });
    }

    let mut vertices = Vec::with_capacity(heightmap.width * heightmap.height);
    for z in 0..heightmap.height {
        for x in 0..heightmap.width {
            let height = heightmap.get(x, z);
            let u = x as f32 / (heightmap.width.saturating_sub(1).max(1) as f32);
            let v = z as f32 / (heightmap.height.saturating_sub(1).max(1) as f32);
            vertices.push(MeshVertex {
                position: [
                    x as f32 * heightmap.cell_scale_x,
                    height,
                    z as f32 * heightmap.cell_scale_z,
                ],
                normal: [0.0, 1.0, 0.0],
                uv: [u, v],
                color: [0.95, 0.95, 0.95, 1.0],
            });
        }
    }

    let mut indices = Vec::with_capacity((heightmap.width - 1) * (heightmap.height - 1) * 6);
    for z in 0..(heightmap.height - 1) {
        for x in 0..(heightmap.width - 1) {
            let top_left = (z * heightmap.width + x) as u32;
            let top_right = top_left + 1;
            let bottom_left = ((z + 1) * heightmap.width + x) as u32;
            let bottom_right = bottom_left + 1;

            indices.extend_from_slice(&[top_left, bottom_left, top_right]);
            indices.extend_from_slice(&[top_right, bottom_left, bottom_right]);
        }
    }

    let mut normals = vec![Vec3::ZERO; vertices.len()];
    for tri in indices.chunks_exact(3) {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        let p0 = Vec3::from_array(vertices[i0].position);
        let p1 = Vec3::from_array(vertices[i1].position);
        let p2 = Vec3::from_array(vertices[i2].position);
        let tri_normal = (p1 - p0).cross(p2 - p0);
        if tri_normal.length_squared() > 0.0 {
            let normalized = tri_normal.normalize();
            normals[i0] += normalized;
            normals[i1] += normalized;
            normals[i2] += normalized;
        }
    }

    for (vertex, normal) in vertices.iter_mut().zip(normals.into_iter()) {
        vertex.normal = if normal.length_squared() > 0.0 {
            normal.normalize().to_array()
        } else {
            [0.0, 1.0, 0.0]
        };
    }

    Ok(TriangleMesh { vertices, indices })
}

fn fbm_value_noise(
    x: f32,
    y: f32,
    base_frequency: f32,
    octaves: u8,
    persistence: f32,
    lacunarity: f32,
    seed: u64,
) -> f32 {
    let mut value = 0.0f32;
    let mut amplitude = 1.0f32;
    let mut frequency = base_frequency;
    let mut max_amplitude = 0.0f32;

    for octave in 0..octaves {
        let octave_scale = 1.0 + octave as f32 * 0.05;
        value += amplitude
            * value_noise(
                x * frequency * octave_scale,
                y * frequency * octave_scale,
                seed + octave as u64,
            );
        max_amplitude += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    if max_amplitude <= 0.0 {
        0.0
    } else {
        value / max_amplitude
    }
}

fn value_noise(x: f32, y: f32, seed: u64) -> f32 {
    let x0 = x.floor() as i64;
    let z0 = y.floor() as i64;
    let x1 = x0 + 1;
    let z1 = z0 + 1;

    let sx = fade((x - x.floor()) as f32);
    let sz = fade((y - y.floor()) as f32);

    let n00 = terrain_hash(x0, z0, seed);
    let n10 = terrain_hash(x1, z0, seed);
    let n01 = terrain_hash(x0, z1, seed);
    let n11 = terrain_hash(x1, z1, seed);

    let ix0 = lerp(n00, n10, sx);
    let ix1 = lerp(n01, n11, sx);
    lerp(ix0, ix1, sz)
}

fn terrain_hash(x: i64, y: i64, seed: u64) -> f32 {
    let mut x = x as u64 ^ seed.rotate_left(21);
    let mut y = y as u64 ^ seed.rotate_left(7);
    x = x.wrapping_mul(0x9e3779b97f4a7c15);
    y = y.wrapping_mul(0xbf58476d1ce4e5b9);
    let mut h = x ^ (y.rotate_left(31));
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000u64 as f32
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoiseTextureConfig {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub seed: u64,
    pub base_frequency: f32,
    pub octaves: u8,
    pub persistence: f32,
    pub lacunarity: f32,
}

impl Default for NoiseTextureConfig {
    fn default() -> Self {
        Self {
            width: 64,
            height: 64,
            channels: 4,
            seed: 42,
            base_frequency: 0.08,
            octaves: 3,
            persistence: 0.55,
            lacunarity: 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GradientDirection {
    Horizontal,
    Vertical,
    Diagonal,
    Radial,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GradientTextureConfig {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub start_color: [u8; 4],
    pub end_color: [u8; 4],
    pub direction: GradientDirection,
}

impl Default for GradientTextureConfig {
    fn default() -> Self {
        Self {
            width: 32,
            height: 32,
            channels: 4,
            start_color: [20, 20, 20, 255],
            end_color: [230, 230, 230, 255],
            direction: GradientDirection::Horizontal,
        }
    }
}

#[derive(Debug)]
pub enum ProceduralTextureError {
    InvalidDimensions { width: u32, height: u32 },
    InvalidChannels(u8),
    InvalidNoiseConfig(String),
}

impl fmt::Display for ProceduralTextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(
                    f,
                    "procedural texture dimensions must be > 0: {width}x{height}"
                )
            }
            Self::InvalidChannels(channels) => {
                write!(f, "texture channel count must be in 1..=4, got {channels}")
            }
            Self::InvalidNoiseConfig(message) => {
                write!(f, "invalid noise texture config: {message}")
            }
        }
    }
}

impl std::error::Error for ProceduralTextureError {}

/// Create deterministic, seeded procedural noise textures (single-channel style data expanded
/// into the requested output channel count).
pub fn generate_noise_texture(
    config: &NoiseTextureConfig,
) -> Result<Vec<u8>, ProceduralTextureError> {
    if config.width == 0 || config.height == 0 {
        return Err(ProceduralTextureError::InvalidDimensions {
            width: config.width,
            height: config.height,
        });
    }
    if !(1..=4).contains(&config.channels) {
        return Err(ProceduralTextureError::InvalidChannels(config.channels));
    }
    if config.base_frequency <= 0.0 {
        return Err(ProceduralTextureError::InvalidNoiseConfig(
            "base_frequency must be > 0".to_string(),
        ));
    }
    if config.octaves == 0 {
        return Err(ProceduralTextureError::InvalidNoiseConfig(
            "octaves must be > 0".to_string(),
        ));
    }
    if !(0.0..=1.0).contains(&config.persistence) {
        return Err(ProceduralTextureError::InvalidNoiseConfig(
            "persistence must be in 0.0..=1.0".to_string(),
        ));
    }
    if config.lacunarity <= 1.0 {
        return Err(ProceduralTextureError::InvalidNoiseConfig(
            "lacunarity must be > 1".to_string(),
        ));
    }

    let width_scale = 1.0 / config.width as f32;
    let height_scale = 1.0 / config.height as f32;
    let mut bytes = Vec::with_capacity(
        config.width as usize * config.height as usize * config.channels as usize,
    );

    for y in 0..config.height {
        for x in 0..config.width {
            let value = fbm_value_noise(
                x as f32 * width_scale,
                y as f32 * height_scale,
                config.base_frequency,
                config.octaves,
                config.persistence,
                config.lacunarity,
                config.seed,
            );
            let intensity = (value * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0;
            let intensity = intensity.round() as u8;
            let pixel = [intensity, intensity, intensity, 255];
            write_rgba_pixel(&mut bytes, &pixel, usize::from(config.channels));
        }
    }

    Ok(bytes)
}

/// Create a deterministic gradient from `start_color` to `end_color` over the requested area.
pub fn generate_gradient_texture(
    config: &GradientTextureConfig,
) -> Result<Vec<u8>, ProceduralTextureError> {
    if config.width == 0 || config.height == 0 {
        return Err(ProceduralTextureError::InvalidDimensions {
            width: config.width,
            height: config.height,
        });
    }
    if !(1..=4).contains(&config.channels) {
        return Err(ProceduralTextureError::InvalidChannels(config.channels));
    }

    let mut bytes = Vec::with_capacity(
        config.width as usize * config.height as usize * config.channels as usize,
    );
    let width_span = (config.width.saturating_sub(1)) as f32;
    let height_span = (config.height.saturating_sub(1)) as f32;
    let max_radial_distance = {
        let cx = (config.width as f32 - 1.0) * 0.5;
        let cy = (config.height as f32 - 1.0) * 0.5;
        let distance = (cx * cx + cy * cy).sqrt();
        if distance == 0.0 {
            1.0
        } else {
            distance
        }
    };

    for y in 0..config.height {
        for x in 0..config.width {
            let t = match config.direction {
                GradientDirection::Horizontal => {
                    if width_span == 0.0 {
                        0.0
                    } else {
                        x as f32 / width_span
                    }
                }
                GradientDirection::Vertical => {
                    if height_span == 0.0 {
                        0.0
                    } else {
                        y as f32 / height_span
                    }
                }
                GradientDirection::Diagonal => {
                    let tx = if width_span == 0.0 {
                        0.0
                    } else {
                        x as f32 / width_span
                    };
                    let ty = if height_span == 0.0 {
                        0.0
                    } else {
                        y as f32 / height_span
                    };
                    ((tx + ty) * 0.5).clamp(0.0, 1.0)
                }
                GradientDirection::Radial => {
                    let cx = (config.width as f32 - 1.0) * 0.5;
                    let cy = (config.height as f32 - 1.0) * 0.5;
                    let nx = x as f32 - cx;
                    let ny = y as f32 - cy;
                    ((nx * nx + ny * ny).sqrt() / max_radial_distance).clamp(0.0, 1.0)
                }
            };

            let mut pixel = [0u8; 4];
            for (channel, (&start_channel, &end_channel)) in config
                .start_color
                .iter()
                .zip(config.end_color.iter())
                .enumerate()
            {
                pixel[channel] = lerp_channel(start_channel, end_channel, t);
            }
            write_rgba_pixel(&mut bytes, &pixel, usize::from(config.channels));
        }
    }

    Ok(bytes)
}

fn write_rgba_pixel(target: &mut Vec<u8>, rgba: &[u8; 4], channels: usize) {
    for channel in 0..channels {
        target.push(rgba[channel.min(3)]);
    }
}

fn lerp_channel(from: u8, to: u8, t: f32) -> u8 {
    ((from as f32 + (to as f32 - from as f32) * t).round()) as u8
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextToAssetMeshRequest {
    pub prompt: String,
    pub seed: Option<u64>,
    pub target_triangle_count: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextToAssetTextureRequest {
    pub prompt: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct TextToAssetTextureArtifact {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum TextToAssetError {
    BackendUnavailable,
    InvalidPrompt(String),
}

impl fmt::Display for TextToAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BackendUnavailable => {
                write!(f, "text-to-asset generator backend is not configured")
            }
            Self::InvalidPrompt(reason) => write!(f, "invalid text-to-asset prompt: {reason}"),
        }
    }
}

impl std::error::Error for TextToAssetError {}

/// Integration point for external AI generation systems.
pub trait TextToAssetGenerator: Send + Sync {
    fn generate_mesh(
        &self,
        request: &TextToAssetMeshRequest,
    ) -> Result<TriangleMesh, TextToAssetError>;

    fn generate_texture(
        &self,
        request: &TextToAssetTextureRequest,
    ) -> Result<TextToAssetTextureArtifact, TextToAssetError>;
}

/// Null implementation signaling that AI generation is disabled until wired.
#[derive(Clone, Debug, Default)]
pub struct NoopTextToAssetGenerator;

impl TextToAssetGenerator for NoopTextToAssetGenerator {
    fn generate_mesh(
        &self,
        _request: &TextToAssetMeshRequest,
    ) -> Result<TriangleMesh, TextToAssetError> {
        Err(TextToAssetError::BackendUnavailable)
    }

    fn generate_texture(
        &self,
        _request: &TextToAssetTextureRequest,
    ) -> Result<TextToAssetTextureArtifact, TextToAssetError> {
        Err(TextToAssetError::BackendUnavailable)
    }
}

pub fn text_to_mesh_from_prompt<G: TextToAssetGenerator>(
    generator: &G,
    prompt: &str,
) -> Result<TriangleMesh, TextToAssetError> {
    let request = TextToAssetMeshRequest {
        prompt: prompt.to_owned(),
        seed: None,
        target_triangle_count: None,
    };
    generator.generate_mesh(&request)
}

pub fn text_to_texture_from_prompt<G: TextToAssetGenerator>(
    generator: &G,
    prompt: &str,
    width: u32,
    height: u32,
) -> Result<TextToAssetTextureArtifact, TextToAssetError> {
    let request = TextToAssetTextureRequest {
        prompt: prompt.to_owned(),
        width,
        height,
    };
    generator.generate_texture(&request)
}

#[derive(Clone, Debug)]
struct BspNode {
    region: DungeonRect,
    left: Option<Box<BspNode>>,
    right: Option<Box<BspNode>>,
    room: Option<DungeonRect>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BspDungeonConfig {
    pub width: usize,
    pub height: usize,
    pub min_leaf_size: usize,
    pub max_leaf_size: usize,
    pub min_room_size: usize,
    pub max_room_size: usize,
    pub seed: u64,
}

impl Default for BspDungeonConfig {
    fn default() -> Self {
        Self {
            width: 64,
            height: 64,
            min_leaf_size: 12,
            max_leaf_size: 30,
            min_room_size: 4,
            max_room_size: 12,
            seed: 1234,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct DungeonRect {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DungeonMap {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<u8>,
    pub rooms: Vec<DungeonRect>,
}

impl DungeonMap {
    pub fn is_floor(&self, x: usize, y: usize) -> bool {
        self.tiles[y * self.width + x] == DungeonTile::Floor as u8
    }

    pub fn floor_count(&self) -> usize {
        self.tiles
            .iter()
            .filter(|tile| **tile == DungeonTile::Floor as u8)
            .count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DungeonTile {
    Wall,
    Floor,
}

#[derive(Debug)]
pub enum DungeonGenerationError {
    InvalidDimensions { width: usize, height: usize },
    InvalidConfig(String),
}

impl fmt::Display for DungeonGenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(
                    f,
                    "dungeon dimensions must be greater than zero: {width}x{height}"
                )
            }
            Self::InvalidConfig(message) => {
                write!(f, "invalid dungeon generation config: {message}")
            }
        }
    }
}

impl std::error::Error for DungeonGenerationError {}

/// Create a deterministic BSP dungeon from simple rectangular leaf splits.
pub fn generate_bsp_dungeon(
    config: &BspDungeonConfig,
) -> Result<DungeonMap, DungeonGenerationError> {
    if config.width == 0 || config.height == 0 {
        return Err(DungeonGenerationError::InvalidDimensions {
            width: config.width,
            height: config.height,
        });
    }
    if config.min_leaf_size == 0 || config.max_leaf_size == 0 {
        return Err(DungeonGenerationError::InvalidConfig(
            "min/max leaf size must be greater than zero".to_string(),
        ));
    }
    if config.min_room_size < 3 || config.max_room_size < config.min_room_size {
        return Err(DungeonGenerationError::InvalidConfig(
            "room sizes are invalid: max must be >= min and min >= 3".to_string(),
        ));
    }
    if config.min_leaf_size < config.min_room_size + 2 {
        return Err(DungeonGenerationError::InvalidConfig(
            "min_leaf_size must be at least min_room_size + 2".to_string(),
        ));
    }
    if config.max_leaf_size <= config.min_leaf_size {
        return Err(DungeonGenerationError::InvalidConfig(
            "max_leaf_size must be larger than min_leaf_size".to_string(),
        ));
    }

    let mut rng = DungeonRng::seeded(config.seed);
    let root_region = DungeonRect {
        x: 0,
        y: 0,
        width: config.width,
        height: config.height,
    };
    let mut root = BspNode {
        region: root_region.clone(),
        left: None,
        right: None,
        room: None,
    };

    split_bsp_node(&mut root, &mut rng, config);

    let mut tiles = vec![DungeonTile::Wall as u8; config.width * config.height];
    let mut rooms = Vec::new();
    carve_node_rooms(
        &mut root,
        &mut rng,
        config,
        &mut rooms,
        &mut tiles,
        config.width,
        config.height,
    );
    connect_node_centers(&root, config.width, config.height, &mut tiles);

    Ok(DungeonMap {
        width: config.width,
        height: config.height,
        tiles,
        rooms,
    })
}

#[derive(Clone, Debug)]
struct DungeonRng(u64);

impl DungeonRng {
    fn seeded(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z ^= z >> 30;
        z = z.wrapping_mul(0xbf58476d1ce4e5b9);
        z ^= z >> 27;
        z = z.wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    fn next_range_inclusive(&mut self, min: usize, max: usize) -> usize {
        if min == max {
            return min;
        }
        let span = max - min + 1;
        min + (self.next_u64() as usize % span)
    }
}

fn split_bsp_node(node: &mut BspNode, rng: &mut DungeonRng, config: &BspDungeonConfig) {
    let mut split_axis_vertical = node.region.width >= node.region.height;
    if node.region.width == node.region.height {
        split_axis_vertical = rng.next_bool();
    }

    let can_split_vertical = node.region.width > config.min_leaf_size.saturating_mul(2);
    let can_split_horizontal = node.region.height > config.min_leaf_size.saturating_mul(2);

    if !can_split_vertical && !can_split_horizontal {
        return;
    }

    if (node.region.width > config.max_leaf_size || node.region.height > config.max_leaf_size)
        == false
    {
        return;
    }

    if split_axis_vertical && !can_split_vertical {
        split_axis_vertical = false;
    } else if !split_axis_vertical && !can_split_horizontal {
        split_axis_vertical = true;
    }

    let split;
    if split_axis_vertical {
        if !can_split_vertical {
            return;
        }
        let max_left = node.region.width - config.min_leaf_size;
        let left_size = max_left.max(config.min_leaf_size);
        split = rng.next_range_inclusive(config.min_leaf_size, left_size);
    } else {
        if !can_split_horizontal {
            return;
        }
        let top_max = node.region.height - config.min_leaf_size;
        let top_size = top_max.max(config.min_leaf_size);
        split = rng.next_range_inclusive(config.min_leaf_size, top_size);
    }

    if node.region.width > config.max_leaf_size || node.region.height > config.max_leaf_size {
        if split_axis_vertical {
            let left_region = DungeonRect {
                x: node.region.x,
                y: node.region.y,
                width: split,
                height: node.region.height,
            };
            let right_region = DungeonRect {
                x: node.region.x + split as i32,
                y: node.region.y,
                width: node.region.width - split,
                height: node.region.height,
            };
            node.left = Some(Box::new(BspNode {
                region: left_region,
                left: None,
                right: None,
                room: None,
            }));
            node.right = Some(Box::new(BspNode {
                region: right_region,
                left: None,
                right: None,
                room: None,
            }));
        } else {
            let top_region = DungeonRect {
                x: node.region.x,
                y: node.region.y,
                width: node.region.width,
                height: split,
            };
            let bottom_region = DungeonRect {
                x: node.region.x,
                y: node.region.y + split as i32,
                width: node.region.width,
                height: node.region.height - split,
            };
            node.left = Some(Box::new(BspNode {
                region: top_region,
                left: None,
                right: None,
                room: None,
            }));
            node.right = Some(Box::new(BspNode {
                region: bottom_region,
                left: None,
                right: None,
                room: None,
            }));
        }

        if let Some(left) = node.left.as_mut() {
            split_bsp_node(left, rng, config);
        }
        if let Some(right) = node.right.as_mut() {
            split_bsp_node(right, rng, config);
        }
    }
}

fn carve_node_rooms(
    node: &mut BspNode,
    rng: &mut DungeonRng,
    config: &BspDungeonConfig,
    rooms: &mut Vec<DungeonRect>,
    tiles: &mut [u8],
    map_width: usize,
    map_height: usize,
) {
    if node.left.is_none() && node.right.is_none() {
        let room = carve_room_in_leaf(node, rng, config);
        if let Some(room_rect) = room {
            carve_filled_rect(
                room_rect.clone(),
                DungeonTile::Floor,
                tiles,
                map_width,
                map_height,
            );
            node.room = Some(room_rect.clone());
            rooms.push(room_rect);
        }
        return;
    }

    if let Some(left) = node.left.as_mut() {
        carve_node_rooms(left, rng, config, rooms, tiles, map_width, map_height);
    }
    if let Some(right) = node.right.as_mut() {
        carve_node_rooms(right, rng, config, rooms, tiles, map_width, map_height);
    }
}

fn carve_room_in_leaf(
    node: &BspNode,
    rng: &mut DungeonRng,
    config: &BspDungeonConfig,
) -> Option<DungeonRect> {
    if node.region.width < config.min_room_size + 2 || node.region.height < config.min_room_size + 2
    {
        return None;
    }
    let max_room_width = (node.region.width - 2)
        .min(config.max_room_size)
        .max(config.min_room_size);
    let max_room_height = (node.region.height - 2)
        .min(config.max_room_size)
        .max(config.min_room_size);
    let room_width = rng.next_range_inclusive(config.min_room_size, max_room_width);
    let room_height = rng.next_range_inclusive(config.min_room_size, max_room_height);

    let x_max_offset = node.region.width - room_width - 1;
    let y_max_offset = node.region.height - room_height - 1;
    let room_x = node.region.x + 1 + rng.next_range_inclusive(0, x_max_offset) as i32;
    let room_y = node.region.y + 1 + rng.next_range_inclusive(0, y_max_offset) as i32;

    Some(DungeonRect {
        x: room_x,
        y: room_y,
        width: room_width,
        height: room_height,
    })
}

fn connect_node_centers(node: &BspNode, map_width: usize, map_height: usize, tiles: &mut [u8]) {
    if let Some(left) = node.left.as_ref() {
        if let Some(left_center) = center_of_room(Some(left)) {
            if let Some(right_center) = center_of_room(node.right.as_deref()) {
                carve_corridor(left_center, right_center, map_width, map_height, tiles);
            }
        }
        connect_node_centers(left, map_width, map_height, tiles);
    }
    if let Some(right) = node.right.as_ref() {
        connect_node_centers(right, map_width, map_height, tiles);
    }
}

fn center_of_room(node: Option<&BspNode>) -> Option<(usize, usize)> {
    node.and_then(|current| {
        if let Some(room) = &current.room {
            Some(room.center_usize())
        } else if let Some(left) = current.left.as_deref() {
            center_of_room(Some(left)).or_else(|| center_of_room(current.right.as_deref()))
        } else {
            None
        }
    })
}

fn carve_corridor(
    from: (usize, usize),
    to: (usize, usize),
    map_width: usize,
    map_height: usize,
    tiles: &mut [u8],
) {
    let (from_x, from_y) = from;
    let (to_x, to_y) = to;

    for x in usize_range(min_max(from_x, to_x).0, min_max(from_x, to_x).1) {
        if x < map_width && from_y < map_height {
            let idx = from_y * map_width + x;
            tiles[idx] = DungeonTile::Floor as u8;
        }
    }

    for y in usize_range(min_max(from_y, to_y).0, min_max(from_y, to_y).1) {
        if to_x < map_width && y < map_height {
            let idx = y * map_width + to_x;
            tiles[idx] = DungeonTile::Floor as u8;
        }
    }
}

fn min_max(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn usize_range(start: usize, end: usize) -> std::ops::RangeInclusive<usize> {
    start..=end
}

fn carve_filled_rect(
    rect: DungeonRect,
    tile: DungeonTile,
    tiles: &mut [u8],
    map_width: usize,
    map_height: usize,
) {
    let end_x = rect.x as usize + rect.width;
    let end_y = rect.y as usize + rect.height;
    for y in rect.y as usize..end_y {
        for x in rect.x as usize..end_x {
            if x < map_width && y < map_height {
                tiles[y * map_width + x] = tile as u8;
            }
        }
    }
}

impl DungeonRect {
    fn center_usize(&self) -> (usize, usize) {
        (
            (self.x as usize) + (self.width / 2),
            (self.y as usize) + (self.height / 2),
        )
    }
}

#[derive(Debug)]
pub enum TextureProcessError {
    NoTexturesToPack,
    InvalidTextureDimensions {
        texture: AssetId,
        width: u32,
        height: u32,
    },
    TextureTooWideForAtlas {
        texture: AssetId,
        width: u32,
        max_width: u32,
    },
}

impl fmt::Display for TextureProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoTexturesToPack => write!(f, "attempted to pack an empty texture list"),
            Self::InvalidTextureDimensions {
                texture,
                width,
                height,
            } => write!(
                f,
                "texture `{texture}` has invalid dimensions: {width}x{height}"
            ),
            Self::TextureTooWideForAtlas {
                texture,
                width,
                max_width,
            } => write!(
                f,
                "texture `{texture}` width {width} exceeds atlas max width {max_width}"
            ),
        }
    }
}

impl std::error::Error for TextureProcessError {}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct AssetImport {
    pub id: AssetId,
    pub source_path: PathBuf,
    pub imported_path: PathBuf,
    pub format: AssetFormat,
    pub checksum: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleOutputRoots {
    pub source: String,
    pub staged: String,
    pub runtime_public: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleMeshLodVariantSpec {
    pub source_path: PathBuf,
    pub runtime_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleAssetSpec {
    pub source_path: PathBuf,
    pub runtime_path: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lod_variants: BTreeMap<String, RuntimeBundleMeshLodVariantSpec>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compressed_lod_variants: BTreeMap<String, RuntimeBundleMeshLodVariantSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleCompressedTextureVariantSpec {
    pub source_path: PathBuf,
    pub runtime_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleSpriteSpec {
    pub source_path: PathBuf,
    pub runtime_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed_variant: Option<RuntimeBundleCompressedTextureVariantSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleSpec {
    pub output_roots: RuntimeBundleOutputRoots,
    pub meshes: BTreeMap<String, RuntimeBundleAssetSpec>,
    pub sprites: BTreeMap<String, RuntimeBundleSpriteSpec>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleCompressedTextureVariantRecord {
    pub source_path: String,
    pub staged_id: String,
    pub staged_format: String,
    pub staged_path: String,
    pub runtime_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleVariantRecord {
    pub source_path: String,
    pub staged_id: String,
    pub staged_format: String,
    pub staged_path: String,
    pub runtime_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleAssetRecord {
    pub source_path: String,
    pub staged_id: String,
    pub staged_format: String,
    pub staged_path: String,
    pub runtime_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed_variant: Option<RuntimeBundleCompressedTextureVariantRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lod_variants: BTreeMap<String, RuntimeBundleVariantRecord>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compressed_lod_variants: BTreeMap<String, RuntimeBundleVariantRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct RuntimeBundleManifest {
    pub version: u32,
    pub output_roots: RuntimeBundleOutputRoots,
    pub meshes: BTreeMap<String, RuntimeBundleAssetRecord>,
    pub sprites: BTreeMap<String, RuntimeBundleAssetRecord>,
}

/// Content-addressed in-memory cache for current-session asset metadata.
#[derive(Default, Debug)]
pub struct AssetCache {
    records: HashMap<AssetId, AssetRecord>,
    source_to_id: HashMap<PathBuf, AssetId>,
    checksum_to_id: HashMap<String, AssetId>,
}

impl AssetCache {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            source_to_id: HashMap::new(),
            checksum_to_id: HashMap::new(),
        }
    }

    /// Index an asset source path into the cache.
    ///
    /// Duplicate binary content reuses the same `AssetId` even when sourced from
    /// different files.
    pub fn index_or_refresh(
        &mut self,
        source_path: impl AsRef<Path>,
        imported_path: impl Into<PathBuf>,
    ) -> Result<AssetId, AssetCacheError> {
        let source_path = canonical_source_path(&source_path)?;
        let imported_path = imported_path.into();
        let checksum = compute_checksum(&source_path)?;
        let byte_len = fs::metadata(&source_path)?.len();
        let id = AssetId::from(format!("sha256:{checksum}"));
        let imported_path_for_record = imported_path;

        if let Some(existing_id) = self.source_to_id.get(&source_path) {
            let existing_id = existing_id.clone();
            if let Some(existing_record) = self.records.get(&existing_id) {
                if existing_record.checksum == checksum {
                    return Ok(existing_id);
                }
                let _ = self.detach_source_from_record(&source_path, &existing_id);
            }
            self.source_to_id.remove(&source_path);
        }

        if let Some(existing_id) = self.checksum_to_id.get(&checksum) {
            let existing_id = existing_id.clone();
            self.source_to_id.insert(source_path, existing_id.clone());
            return Ok(existing_id);
        }

        let record = AssetRecord::new(
            id.clone(),
            source_path.clone(),
            imported_path_for_record,
            checksum.clone(),
            byte_len,
        );
        self.source_to_id.insert(source_path, id.clone());
        self.checksum_to_id.insert(checksum, id.clone());
        self.records.insert(id.clone(), record);

        Ok(id)
    }

    /// Upsert a record directly without recomputing its hash.
    pub fn upsert(&mut self, record: AssetRecord) {
        self.source_to_id
            .insert(record.source_path.clone(), record.id.clone());
        self.checksum_to_id
            .insert(record.checksum.clone(), record.id.clone());
        self.records.insert(record.id.clone(), record);
    }

    pub fn get(&self, id: &AssetId) -> Option<&AssetRecord> {
        self.records.get(id)
    }

    pub fn get_by_source(&self, source_path: impl AsRef<Path>) -> Option<&AssetRecord> {
        let path = source_path.as_ref();
        let source_path = path
            .canonicalize()
            .ok()
            .or_else(|| fallback_canonicalized_source(path))
            .unwrap_or_else(|| path.to_path_buf());
        self.source_to_id
            .get(&source_path)
            .and_then(|id| self.records.get(id))
    }

    pub fn contains(&self, id: &AssetId) -> bool {
        self.records.contains_key(id)
    }

    pub fn remove(&mut self, id: &AssetId) -> Option<AssetRecord> {
        let removed = self.records.remove(id)?;
        self.source_to_id.retain(|_, cached| cached != id);
        self.checksum_to_id.retain(|_, cached| cached != id);
        Some(removed)
    }

    pub fn total(&self) -> usize {
        self.records.len()
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.source_to_id.clear();
        self.checksum_to_id.clear();
    }

    pub fn set_state(&mut self, id: &AssetId, state: AssetState) {
        if let Some(record) = self.records.get_mut(id) {
            record.state = state;
            record.imported_at_unix = now_unix_seconds();
        }
    }

    fn detach_source_from_record(
        &mut self,
        source_path: &Path,
        id: &AssetId,
    ) -> Option<AssetRecord> {
        let removed = self.records.get(id).cloned()?;
        self.source_to_id.remove(source_path);
        let has_aliases = self.source_to_id.values().any(|cached| cached == id);
        if !has_aliases {
            self.records.remove(id);
            if let Some(cached) = self.checksum_to_id.get(&removed.checksum) {
                if cached == id {
                    self.checksum_to_id.remove(&removed.checksum);
                }
            }
        }
        Some(removed)
    }
}

#[derive(Debug, Clone)]
struct GodotNodeRecord {
    name: String,
    node_type: String,
    parent_path: Option<String>,
    full_path: String,
    properties: HashMap<String, String>,
}

enum GodotSection {
    Ignore,
    Node(usize),
}

#[derive(Debug, Deserialize)]
struct TiledMapDocument {
    #[serde(default)]
    name: String,
    width: u32,
    height: u32,
    tilewidth: u32,
    tileheight: u32,
    #[serde(default)]
    infinite: bool,
    #[serde(default)]
    layers: Vec<TiledLayer>,
    #[serde(default)]
    tilesets: Vec<TiledTileset>,
}

#[derive(Debug, Deserialize)]
struct TiledLayer {
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    layer_type: String,
    #[serde(default = "default_visible")]
    visible: bool,
    #[serde(default = "default_opacity")]
    opacity: f32,
    #[serde(default)]
    offsetx: f32,
    #[serde(default)]
    offsety: f32,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    width: u32,
    #[serde(default)]
    height: u32,
    #[serde(default)]
    tintcolor: Option<String>,
    #[serde(default)]
    data: Vec<u32>,
    #[serde(default)]
    objects: Vec<TiledObject>,
    #[serde(default)]
    layers: Vec<TiledLayer>,
}

#[derive(Debug, Deserialize)]
struct TiledObject {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default = "default_visible")]
    visible: bool,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default)]
    width: f32,
    #[serde(default)]
    height: f32,
    #[serde(default)]
    rotation: f32,
    #[serde(default)]
    gid: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TiledTileset {
    firstgid: u32,
    #[serde(default)]
    image: Option<String>,
}

/// Import a Godot text scene (`.tscn`) into a POD-authored `Scene`.
pub fn import_godot_scene(source_path: impl AsRef<Path>) -> Result<Scene, AssetImportError> {
    let source_path = canonical_source_path(source_path)?;
    let document = fs::read_to_string(&source_path)?;
    parse_godot_scene_document(&document, &source_path)
}

/// Import a Tiled JSON map (`.tmj`) into a POD-authored `Scene`.
pub fn import_tiled_scene(source_path: impl AsRef<Path>) -> Result<Scene, AssetImportError> {
    let source_path = canonical_source_path(source_path)?;
    let document = fs::read_to_string(&source_path)?;
    parse_tiled_scene_document(&document, &source_path)
}

fn parse_godot_scene_document(
    document: &str,
    source_path: &Path,
) -> Result<Scene, AssetImportError> {
    let scene_name = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported_scene");
    let mut scene = Scene::new(scene_name);
    scene.metadata.description = format!("Imported from Godot scene {}", source_path.display());
    scene.metadata.tags = vec![
        "imported".to_string(),
        "godot".to_string(),
        "2d".to_string(),
    ];

    let mut ext_resources = HashMap::<String, String>::new();
    let mut nodes = Vec::<GodotNodeRecord>::new();
    let mut section = GodotSection::Ignore;

    for raw_line in document.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let (section_name, attributes) = parse_godot_section_header(line).ok_or_else(|| {
                scene_import_error(
                    source_path,
                    format!("could not parse Godot section header `{line}`"),
                )
            })?;

            section = match section_name.as_str() {
                "ext_resource" => {
                    if let (Some(id), Some(path)) = (attributes.get("id"), attributes.get("path")) {
                        ext_resources.insert(id.clone(), path.clone());
                    }
                    GodotSection::Ignore
                }
                "node" => {
                    let name = attributes
                        .get("name")
                        .cloned()
                        .unwrap_or_else(|| "Node".to_string());
                    let parent_path = attributes.get("parent").cloned();
                    let node_type = attributes
                        .get("type")
                        .cloned()
                        .unwrap_or_else(|| "Node".to_string());
                    let full_path = match parent_path.as_deref() {
                        None => ".".to_string(),
                        Some(".") => name.clone(),
                        Some(parent) => format!("{parent}/{name}"),
                    };

                    if nodes.iter().any(|node| node.full_path == full_path) {
                        return Err(scene_import_error(
                            source_path,
                            format!("duplicate Godot node path `{full_path}`"),
                        ));
                    }

                    nodes.push(GodotNodeRecord {
                        name,
                        node_type,
                        parent_path,
                        full_path,
                        properties: HashMap::new(),
                    });
                    GodotSection::Node(nodes.len() - 1)
                }
                _ => GodotSection::Ignore,
            };
            continue;
        }

        if let GodotSection::Node(index) = section {
            if let Some((key, value)) = line.split_once('=') {
                nodes[index]
                    .properties
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }

    if nodes.is_empty() {
        return Err(scene_import_error(
            source_path,
            "scene does not contain any `[node]` sections",
        ));
    }

    let mut node_ids = HashMap::<String, uuid::Uuid>::new();
    for node in &nodes {
        let mut entity = EntityInstance::new(node.name.clone());

        if should_import_transform(node) {
            entity
                .add_native_component(&build_node_transform(&node.properties))
                .map_err(|message| scene_import_error(source_path, message))?;
        }

        if node.node_type == "Sprite2D" {
            entity
                .add_native_component(&build_sprite_component(&node.properties, &ext_resources))
                .map_err(|message| scene_import_error(source_path, message))?;
        }

        if node.node_type == "ColorRect" {
            entity
                .add_native_component(&build_color_rect_component(&node.properties))
                .map_err(|message| scene_import_error(source_path, message))?;
        }

        let entity_id = scene.add_entity(entity);
        node_ids.insert(node.full_path.clone(), entity_id);
    }

    for node in &nodes {
        let Some(parent_path) = node.parent_path.as_deref() else {
            continue;
        };
        let child_id = *node_ids.get(&node.full_path).ok_or_else(|| {
            scene_import_error(
                source_path,
                format!("missing child scene entity for `{}`", node.full_path),
            )
        })?;
        let parent_id = *node_ids.get(parent_path).ok_or_else(|| {
            scene_import_error(
                source_path,
                format!("missing parent scene entity for `{parent_path}`"),
            )
        })?;
        scene.graph.set_parent(child_id, parent_id);
    }

    Ok(scene)
}

fn parse_tiled_scene_document(
    document: &str,
    source_path: &Path,
) -> Result<Scene, AssetImportError> {
    let map: TiledMapDocument = serde_json::from_str(document)
        .map_err(|err| scene_import_error(source_path, err.to_string()))?;

    if map.infinite {
        return Err(scene_import_error(
            source_path,
            "infinite Tiled maps are not supported by the current importer",
        ));
    }

    let scene_name = if map.name.trim().is_empty() {
        source_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("imported_scene")
            .to_string()
    } else {
        map.name.clone()
    };

    let mut scene = Scene::new(scene_name);
    scene.metadata.description = format!("Imported from Tiled map {}", source_path.display());
    scene.metadata.tags = vec![
        "imported".to_string(),
        "tiled".to_string(),
        "2d".to_string(),
    ];

    let mut layer_depth = 0i32;
    for layer in &map.layers {
        import_tiled_layer(&mut scene, source_path, &map, layer, None, &mut layer_depth)?;
    }

    Ok(scene)
}

fn import_tiled_layer(
    scene: &mut Scene,
    source_path: &Path,
    map: &TiledMapDocument,
    layer: &TiledLayer,
    parent_id: Option<uuid::Uuid>,
    layer_depth: &mut i32,
) -> Result<uuid::Uuid, AssetImportError> {
    let layer_name = if layer.name.trim().is_empty() {
        format!("layer_{}", *layer_depth)
    } else {
        layer.name.clone()
    };

    let mut layer_entity = EntityInstance::new(layer_name);
    let layer_transform = build_tiled_layer_transform(map, layer);
    layer_entity
        .add_native_component(&layer_transform)
        .map_err(|message| scene_import_error(source_path, message))?;
    let layer_id = scene.add_entity(layer_entity);
    if let Some(parent_id) = parent_id {
        scene.graph.set_parent(layer_id, parent_id);
    }

    match layer.layer_type.as_str() {
        "tilelayer" => {
            import_tiled_tile_layer(scene, source_path, map, layer, layer_id, *layer_depth)?;
            *layer_depth += 1;
        }
        "objectgroup" => {
            import_tiled_object_layer(scene, source_path, map, layer, layer_id, *layer_depth)?;
            *layer_depth += 1;
        }
        "group" => {
            for child in &layer.layers {
                import_tiled_layer(scene, source_path, map, child, Some(layer_id), layer_depth)?;
            }
        }
        _ => {}
    }

    Ok(layer_id)
}

fn build_tiled_layer_transform(map: &TiledMapDocument, layer: &TiledLayer) -> Transform {
    Transform::at(
        layer.offsetx + layer.x * map.tilewidth as f32,
        layer.offsety + layer.y * map.tileheight as f32,
    )
}

fn import_tiled_tile_layer(
    scene: &mut Scene,
    source_path: &Path,
    map: &TiledMapDocument,
    layer: &TiledLayer,
    parent_id: uuid::Uuid,
    layer_depth: i32,
) -> Result<(), AssetImportError> {
    let layer_width = if layer.width == 0 {
        map.width
    } else {
        layer.width
    };
    let layer_height = if layer.height == 0 {
        map.height
    } else {
        layer.height
    };
    if layer_width == 0 || layer_height == 0 {
        return Ok(());
    }
    if layer.data.len() != (layer_width as usize).saturating_mul(layer_height as usize) {
        return Err(scene_import_error(
            source_path,
            format!(
                "tile layer `{}` data length {} does not match {}x{}",
                layer.name,
                layer.data.len(),
                layer_width,
                layer_height
            ),
        ));
    }

    let tint = parse_tiled_hex_color(layer.tintcolor.as_deref()).unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let layer_alpha = layer.opacity.clamp(0.0, 1.0);

    for (index, gid_with_flags) in layer.data.iter().enumerate() {
        let gid = strip_tiled_gid_flags(*gid_with_flags);
        if gid == 0 {
            continue;
        }

        let column = (index as u32) % layer_width;
        let row = (index as u32) / layer_width;
        let (texture, frame) = resolve_tiled_tileset(gid, &map.tilesets).ok_or_else(|| {
            scene_import_error(
                source_path,
                format!(
                    "could not resolve tileset image for gid {gid} in `{}`",
                    layer.name
                ),
            )
        })?;

        let mut tile = EntityInstance::new(format!("tile_{column}_{row}"));
        tile.add_native_component(&Transform::at(
            (column as f32 + 0.5) * map.tilewidth as f32,
            (row as f32 + 0.5) * map.tileheight as f32,
        ))
        .map_err(|message| scene_import_error(source_path, message))?;
        tile.add_native_component(&Sprite {
            texture,
            frame,
            layer: layer_depth,
            color: [tint[0], tint[1], tint[2], tint[3] * layer_alpha],
            visible: layer.visible,
        })
        .map_err(|message| scene_import_error(source_path, message))?;
        let tile_id = scene.add_entity(tile);
        scene.graph.set_parent(tile_id, parent_id);
    }

    Ok(())
}

fn import_tiled_object_layer(
    scene: &mut Scene,
    source_path: &Path,
    map: &TiledMapDocument,
    layer: &TiledLayer,
    parent_id: uuid::Uuid,
    layer_depth: i32,
) -> Result<(), AssetImportError> {
    let tint = parse_tiled_hex_color(layer.tintcolor.as_deref()).unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let layer_alpha = layer.opacity.clamp(0.0, 1.0);

    for object in &layer.objects {
        let object_name = if object.name.trim().is_empty() {
            format!("object_{}", object.id)
        } else {
            object.name.clone()
        };

        let mut entity = EntityInstance::new(object_name);
        let mut object_transform = Transform::at(
            object.x + object.width * 0.5,
            object.y + object.height * 0.5,
        );
        object_transform.rotation = object.rotation.to_radians();
        entity
            .add_native_component(&object_transform)
            .map_err(|message| scene_import_error(source_path, message))?;

        if let Some(gid) = object.gid.map(strip_tiled_gid_flags) {
            let (texture, frame) = resolve_tiled_tileset(gid, &map.tilesets).ok_or_else(|| {
                scene_import_error(
                    source_path,
                    format!("could not resolve tileset image for object gid {gid}"),
                )
            })?;
            entity
                .add_native_component(&Sprite {
                    texture,
                    frame,
                    layer: layer_depth,
                    color: [tint[0], tint[1], tint[2], tint[3] * layer_alpha],
                    visible: layer.visible && object.visible,
                })
                .map_err(|message| scene_import_error(source_path, message))?;
        } else if object.width > 0.0 || object.height > 0.0 {
            let mut rect = ColorRect::new(
                object.width.max(map.tilewidth as f32),
                object.height.max(map.tileheight as f32),
                [tint[0], tint[1], tint[2], tint[3] * layer_alpha],
            );
            rect.layer = layer_depth;
            entity
                .add_native_component(&rect)
                .map_err(|message| scene_import_error(source_path, message))?;
        }

        let entity_id = scene.add_entity(entity);
        scene.graph.set_parent(entity_id, parent_id);
    }

    Ok(())
}

fn resolve_tiled_tileset(gid: u32, tilesets: &[TiledTileset]) -> Option<(String, u32)> {
    let tileset = tilesets
        .iter()
        .filter(|tileset| tileset.firstgid <= gid)
        .max_by_key(|tileset| tileset.firstgid)?;
    let texture = normalize_tiled_resource_path(tileset.image.as_deref()?);
    Some((texture, gid.saturating_sub(tileset.firstgid)))
}

fn normalize_tiled_resource_path(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_string()
}

fn strip_tiled_gid_flags(gid: u32) -> u32 {
    gid & 0x1FFF_FFFF
}

fn parse_tiled_hex_color(raw_value: Option<&str>) -> Option<[f32; 4]> {
    let hex = raw_value?.trim().strip_prefix('#')?;
    match hex.len() {
        6 => Some([
            u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0,
            u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0,
            u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0,
            1.0,
        ]),
        8 => Some([
            u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0,
            u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0,
            u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0,
            u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0,
        ]),
        _ => None,
    }
}

fn write_imported_scene_artifact(
    source_path: &Path,
    imported_path: &Path,
    format: &AssetFormat,
) -> Result<(), AssetImportError> {
    let scene = match format {
        AssetFormat::GodotScene => import_godot_scene(source_path)?,
        AssetFormat::TiledMap => import_tiled_scene(source_path)?,
        AssetFormat::UnityScene => import_unity_scene(source_path)?,
        _ => {
            return Err(scene_import_error(
                source_path,
                format!("scene artifact writer does not support `{format}`"),
            ));
        }
    };
    let bytes = serde_json::to_vec_pretty(&scene)
        .map_err(|err| scene_import_error(source_path, err.to_string()))?;
    if let Some(parent) = imported_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(imported_path, bytes)?;
    Ok(())
}

fn write_imported_asset_artifact(
    source_path: &Path,
    imported_path: &Path,
    format: &AssetFormat,
) -> Result<(), AssetImportError> {
    match format {
        AssetFormat::GodotScene | AssetFormat::TiledMap | AssetFormat::UnityScene => {
            write_imported_scene_artifact(source_path, imported_path, format)
        }
        AssetFormat::Gltf
        | AssetFormat::Obj
        | AssetFormat::Png
        | AssetFormat::Jpeg
        | AssetFormat::Ktx2
        | AssetFormat::Svg => {
            if let Some(parent) = imported_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(source_path, imported_path)?;
            Ok(())
        }
    }
}

fn scene_import_error(source_path: &Path, message: impl Into<String>) -> AssetImportError {
    AssetImportError::SceneImport {
        path: source_path.to_path_buf(),
        message: message.into(),
    }
}

fn default_visible() -> bool {
    true
}

fn default_opacity() -> f32 {
    1.0
}

fn parse_godot_section_header(line: &str) -> Option<(String, HashMap<String, String>)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?.trim();
    let split_index = inner.find(char::is_whitespace);
    let (section_name, rest) = match split_index {
        Some(index) => (&inner[..index], inner[index..].trim()),
        None => (inner, ""),
    };
    Some((
        section_name.to_string(),
        parse_godot_header_attributes(rest),
    ))
}

fn parse_godot_header_attributes(input: &str) -> HashMap<String, String> {
    let bytes = input.as_bytes();
    let mut attributes = HashMap::new();
    let mut index = 0;

    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let key_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let key = input[key_start..index].trim();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if key.is_empty() || index >= bytes.len() || bytes[index] != b'=' {
            break;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }

        let value = if bytes[index] == b'"' {
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != b'"' {
                index += 1;
            }
            let parsed = input[value_start..index].to_string();
            if index < bytes.len() {
                index += 1;
            }
            parsed
        } else {
            let value_start = index;
            let mut paren_depth = 0usize;
            while index < bytes.len() {
                match bytes[index] {
                    b'(' => paren_depth += 1,
                    b')' if paren_depth > 0 => paren_depth -= 1,
                    b' ' | b'\t' if paren_depth == 0 => break,
                    _ => {}
                }
                index += 1;
            }
            input[value_start..index].trim().to_string()
        };

        attributes.insert(key.to_string(), value);
    }

    attributes
}

fn should_import_transform(node: &GodotNodeRecord) -> bool {
    node.node_type.ends_with("2D")
        || node.node_type == "ColorRect"
        || node.properties.contains_key("position")
        || node.properties.contains_key("rotation")
        || node.properties.contains_key("rotation_degrees")
        || node.properties.contains_key("scale")
}

fn build_node_transform(properties: &HashMap<String, String>) -> Transform {
    let mut transform = Transform::default();
    if let Some(position) = properties
        .get("position")
        .and_then(|value| parse_vector2(value))
    {
        transform.position = position;
    }
    if let Some(rotation) = properties
        .get("rotation")
        .and_then(|value| value.parse::<f32>().ok())
        .or_else(|| {
            properties
                .get("rotation_degrees")
                .and_then(|value| value.parse::<f32>().ok())
                .map(f32::to_radians)
        })
    {
        transform.rotation = rotation;
    }
    if let Some(scale) = properties
        .get("scale")
        .and_then(|value| parse_vector2(value))
    {
        transform.scale = scale;
    }
    transform
}

fn build_sprite_component(
    properties: &HashMap<String, String>,
    ext_resources: &HashMap<String, String>,
) -> Sprite {
    let mut sprite = Sprite::default();
    if let Some(texture) = properties
        .get("texture")
        .and_then(|value| resolve_godot_resource(value, ext_resources))
    {
        sprite.texture = texture;
    }
    if let Some(frame) = properties
        .get("frame")
        .and_then(|value| value.parse::<u32>().ok())
    {
        sprite.frame = frame;
    }
    if let Some(layer) = properties
        .get("z_index")
        .and_then(|value| value.parse::<i32>().ok())
    {
        sprite.layer = layer;
    }
    if let Some(visible) = properties
        .get("visible")
        .and_then(|value| parse_bool_literal(value))
    {
        sprite.visible = visible;
    }
    if let Some(color) = properties
        .get("modulate")
        .and_then(|value| parse_color(value))
    {
        sprite.color = color;
    }
    sprite
}

fn build_color_rect_component(properties: &HashMap<String, String>) -> ColorRect {
    let size = properties
        .get("size")
        .and_then(|value| parse_vector2(value))
        .unwrap_or(Vec2::ZERO);
    let color = properties
        .get("color")
        .and_then(|value| parse_color(value))
        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let mut rect = ColorRect::new(size.x, size.y, color);
    if let Some(layer) = properties
        .get("z_index")
        .and_then(|value| value.parse::<i32>().ok())
    {
        rect.layer = layer;
    }
    rect
}

fn resolve_godot_resource(
    raw_value: &str,
    ext_resources: &HashMap<String, String>,
) -> Option<String> {
    let trimmed = raw_value.trim();
    if let Some(resource_id) = trimmed
        .strip_prefix("ExtResource(\"")
        .and_then(|value| value.strip_suffix("\")"))
    {
        return ext_resources
            .get(resource_id)
            .map(|path| normalize_godot_resource_path(path));
    }

    Some(normalize_godot_resource_path(trimmed.trim_matches('"')))
}

fn normalize_godot_resource_path(path: &str) -> String {
    path.strip_prefix("res://").unwrap_or(path).to_string()
}

fn parse_bool_literal(raw_value: &str) -> Option<bool> {
    match raw_value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_vector2(raw_value: &str) -> Option<Vec2> {
    let values = parse_f32_call(raw_value, "Vector2")?;
    if values.len() != 2 {
        return None;
    }
    Some(Vec2::new(values[0], values[1]))
}

fn parse_color(raw_value: &str) -> Option<[f32; 4]> {
    if let Some(values) = parse_f32_call(raw_value, "Color") {
        return match values.as_slice() {
            [r, g, b] => Some([*r, *g, *b, 1.0]),
            [r, g, b, a] => Some([*r, *g, *b, *a]),
            _ => None,
        };
    }

    let values = parse_f32_call(raw_value, "Color8")?;
    match values.as_slice() {
        [r, g, b] => Some([*r / 255.0, *g / 255.0, *b / 255.0, 1.0]),
        [r, g, b, a] => Some([*r / 255.0, *g / 255.0, *b / 255.0, *a / 255.0]),
        _ => None,
    }
}

fn parse_f32_call(raw_value: &str, function_name: &str) -> Option<Vec<f32>> {
    let inner = raw_value
        .trim()
        .strip_prefix(function_name)?
        .strip_prefix('(')?
        .strip_suffix(')')?;
    inner
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<_>>>()
}

/// Create a normalized import plan for a single asset source file.
pub fn import_asset(
    cache: &mut AssetCache,
    source_path: impl AsRef<Path>,
    output_root: impl AsRef<Path>,
) -> Result<AssetImport, AssetImportError> {
    let source_path = canonical_source_path(&source_path)?;
    let checksum = compute_checksum(&source_path)?;
    let format = AssetFormat::from_path(&source_path)
        .ok_or_else(|| AssetImportError::UnsupportedFormat(source_path.clone()))?;
    let byte_len = fs::metadata(&source_path)?.len();
    let file_id = AssetId::from(format!("sha256:{checksum}"));
    let imported_path = output_root
        .as_ref()
        .join(format.import_prefix())
        .join(format!(
            "{}.{}",
            file_id.as_filename(),
            format.imported_extension(&source_path)
        ));

    write_imported_asset_artifact(&source_path, &imported_path, &format)?;

    cache.index_or_refresh(&source_path, imported_path.clone())?;
    let record = cache.get(&file_id).cloned().unwrap_or_else(|| {
        AssetRecord::new(
            file_id.clone(),
            source_path.clone(),
            imported_path.clone(),
            checksum.clone(),
            byte_len,
        )
    });

    Ok(AssetImport {
        id: record.id,
        source_path,
        imported_path: record.imported_path,
        format,
        checksum: record.checksum,
        byte_len: record.byte_len,
    })
}

pub fn build_runtime_bundle_manifest(
    spec: &RuntimeBundleSpec,
    imports: &[AssetImport],
    base_dir: impl AsRef<Path>,
) -> Result<RuntimeBundleManifest, RuntimeBundleError> {
    let imports_by_source: HashMap<PathBuf, &AssetImport> = imports
        .iter()
        .map(|import| (import.source_path.clone(), import))
        .collect();
    let base_dir = base_dir.as_ref();

    let manifest = RuntimeBundleManifest {
        version: 1,
        output_roots: spec.output_roots.clone(),
        meshes: build_runtime_bundle_records(&spec.meshes, &imports_by_source, base_dir)?,
        sprites: build_runtime_bundle_sprite_records(&spec.sprites, &imports_by_source, base_dir)?,
    };
    ensure_unique_runtime_bundle_paths(&manifest, base_dir)?;

    Ok(manifest)
}

fn build_runtime_bundle_records(
    spec_entries: &BTreeMap<String, RuntimeBundleAssetSpec>,
    imports_by_source: &HashMap<PathBuf, &AssetImport>,
    base_dir: &Path,
) -> Result<BTreeMap<String, RuntimeBundleAssetRecord>, RuntimeBundleError> {
    spec_entries
        .iter()
        .map(|(asset_id, asset_spec)| {
            let mut record = build_runtime_bundle_record(
                asset_id,
                &asset_spec.source_path,
                &asset_spec.runtime_path,
                imports_by_source,
                base_dir,
            )?;
            for (level, variant_spec) in &asset_spec.lod_variants {
                let variant_record = build_runtime_bundle_variant_record(
                    asset_id,
                    &variant_spec.source_path,
                    &variant_spec.runtime_path,
                    imports_by_source,
                    base_dir,
                )?;
                record.lod_variants.insert(level.clone(), variant_record);
            }
            for (level, variant_spec) in &asset_spec.compressed_lod_variants {
                let variant_import = resolve_runtime_bundle_import(
                    asset_id,
                    &variant_spec.source_path,
                    imports_by_source,
                )?;
                if variant_import.format != AssetFormat::Gltf {
                    return Err(RuntimeBundleError::UnsupportedCompressedMeshFormat {
                        asset_id: asset_id.clone(),
                        format: variant_import.format.clone(),
                    });
                }
                let variant_record = build_runtime_bundle_variant_record(
                    asset_id,
                    &variant_spec.source_path,
                    &variant_spec.runtime_path,
                    imports_by_source,
                    base_dir,
                )?;
                record
                    .compressed_lod_variants
                    .insert(level.clone(), variant_record);
            }
            Ok((asset_id.clone(), record))
        })
        .collect()
}

fn build_runtime_bundle_sprite_records(
    spec_entries: &BTreeMap<String, RuntimeBundleSpriteSpec>,
    imports_by_source: &HashMap<PathBuf, &AssetImport>,
    base_dir: &Path,
) -> Result<BTreeMap<String, RuntimeBundleAssetRecord>, RuntimeBundleError> {
    spec_entries
        .iter()
        .map(|(asset_id, asset_spec)| {
            let mut record = build_runtime_bundle_record(
                asset_id,
                &asset_spec.source_path,
                &asset_spec.runtime_path,
                imports_by_source,
                base_dir,
            )?;
            if let Some(compressed_variant) = asset_spec.compressed_variant.as_ref() {
                let compressed_import = resolve_runtime_bundle_import(
                    asset_id,
                    &compressed_variant.source_path,
                    imports_by_source,
                )?;
                if compressed_import.format != AssetFormat::Ktx2 {
                    return Err(RuntimeBundleError::UnsupportedCompressedTextureFormat {
                        asset_id: asset_id.clone(),
                        format: compressed_import.format.clone(),
                    });
                }
                record.compressed_variant = Some(RuntimeBundleCompressedTextureVariantRecord {
                    source_path: display_path_relative_to(base_dir, &compressed_import.source_path),
                    staged_id: compressed_import.id.to_string(),
                    staged_format: compressed_import.format.to_string(),
                    staged_path: display_path_relative_to(
                        base_dir,
                        &compressed_import.imported_path,
                    ),
                    runtime_path: compressed_variant.runtime_path.clone(),
                });
            }
            Ok((asset_id.clone(), record))
        })
        .collect()
}

fn build_runtime_bundle_record(
    asset_id: &str,
    source_path: &Path,
    runtime_path: &str,
    imports_by_source: &HashMap<PathBuf, &AssetImport>,
    base_dir: &Path,
) -> Result<RuntimeBundleAssetRecord, RuntimeBundleError> {
    let variant = build_runtime_bundle_variant_record(
        asset_id,
        source_path,
        runtime_path,
        imports_by_source,
        base_dir,
    )?;
    Ok(RuntimeBundleAssetRecord {
        source_path: variant.source_path,
        staged_id: variant.staged_id,
        staged_format: variant.staged_format,
        staged_path: variant.staged_path,
        runtime_path: variant.runtime_path,
        compressed_variant: None,
        lod_variants: BTreeMap::new(),
        compressed_lod_variants: BTreeMap::new(),
    })
}

fn build_runtime_bundle_variant_record(
    asset_id: &str,
    source_path: &Path,
    runtime_path: &str,
    imports_by_source: &HashMap<PathBuf, &AssetImport>,
    base_dir: &Path,
) -> Result<RuntimeBundleVariantRecord, RuntimeBundleError> {
    let import = resolve_runtime_bundle_import(asset_id, source_path, imports_by_source)?;
    Ok(RuntimeBundleVariantRecord {
        source_path: display_path_relative_to(base_dir, &import.source_path),
        staged_id: import.id.to_string(),
        staged_format: import.format.to_string(),
        staged_path: display_path_relative_to(base_dir, &import.imported_path),
        runtime_path: runtime_path.to_string(),
    })
}

fn resolve_runtime_bundle_import<'a>(
    asset_id: &str,
    source_path: &Path,
    imports_by_source: &'a HashMap<PathBuf, &'a AssetImport>,
) -> Result<&'a AssetImport, RuntimeBundleError> {
    let canonical_source = canonical_source_path(source_path)
        .map_err(|_| RuntimeBundleError::InvalidSourcePath(source_path.to_path_buf()))?;
    imports_by_source
        .get(&canonical_source)
        .copied()
        .ok_or_else(|| RuntimeBundleError::MissingImport {
            asset_id: asset_id.to_string(),
            source_path: source_path.to_path_buf(),
        })
}

fn ensure_unique_runtime_bundle_paths(
    manifest: &RuntimeBundleManifest,
    base_dir: &Path,
) -> Result<(), RuntimeBundleError> {
    let mut seen_paths = HashMap::<PathBuf, String>::new();

    for (asset_id, record) in &manifest.meshes {
        register_runtime_bundle_path(
            &mut seen_paths,
            base_dir,
            &manifest.output_roots.runtime_public,
            &record.runtime_path,
            asset_id,
        )?;
        for variant_record in record.lod_variants.values() {
            register_runtime_bundle_path(
                &mut seen_paths,
                base_dir,
                &manifest.output_roots.runtime_public,
                &variant_record.runtime_path,
                asset_id,
            )?;
        }
        for variant_record in record.compressed_lod_variants.values() {
            register_runtime_bundle_path(
                &mut seen_paths,
                base_dir,
                &manifest.output_roots.runtime_public,
                &variant_record.runtime_path,
                asset_id,
            )?;
        }
    }

    for (asset_id, record) in &manifest.sprites {
        register_runtime_bundle_path(
            &mut seen_paths,
            base_dir,
            &manifest.output_roots.runtime_public,
            &record.runtime_path,
            asset_id,
        )?;
        if let Some(compressed_variant) = record.compressed_variant.as_ref() {
            register_runtime_bundle_path(
                &mut seen_paths,
                base_dir,
                &manifest.output_roots.runtime_public,
                &compressed_variant.runtime_path,
                asset_id,
            )?;
        }
    }

    Ok(())
}

fn register_runtime_bundle_path(
    seen_paths: &mut HashMap<PathBuf, String>,
    base_dir: &Path,
    runtime_public_root: &str,
    runtime_path: &str,
    asset_id: &str,
) -> Result<(), RuntimeBundleError> {
    let normalized_path = runtime_bundle_output_path(base_dir, runtime_public_root, runtime_path)?;
    if let Some(existing_asset_id) = seen_paths.get(&normalized_path) {
        return Err(RuntimeBundleError::DuplicateRuntimePath {
            runtime_path: runtime_path.to_string(),
            first_asset_id: existing_asset_id.clone(),
            second_asset_id: asset_id.to_string(),
        });
    }
    seen_paths.insert(normalized_path, asset_id.to_string());
    Ok(())
}

pub fn materialize_runtime_bundle_manifest(
    manifest: &RuntimeBundleManifest,
    base_dir: impl AsRef<Path>,
) -> Result<(), RuntimeBundleError> {
    let base_dir = base_dir.as_ref();

    materialize_runtime_bundle_records(&manifest.output_roots, &manifest.meshes, base_dir)?;
    materialize_runtime_bundle_records(&manifest.output_roots, &manifest.sprites, base_dir)?;

    Ok(())
}

fn materialize_runtime_bundle_records(
    output_roots: &RuntimeBundleOutputRoots,
    records: &BTreeMap<String, RuntimeBundleAssetRecord>,
    base_dir: &Path,
) -> Result<(), RuntimeBundleError> {
    for record in records.values() {
        let staged_path = base_dir.join(&record.staged_path);
        let runtime_path = runtime_bundle_output_path(
            base_dir,
            &output_roots.runtime_public,
            &record.runtime_path,
        )?;
        if let Some(parent) = runtime_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(staged_path, runtime_path)?;
        for variant_record in record.lod_variants.values() {
            let variant_staged_path = base_dir.join(&variant_record.staged_path);
            let variant_runtime_path = runtime_bundle_output_path(
                base_dir,
                &output_roots.runtime_public,
                &variant_record.runtime_path,
            )?;
            if let Some(parent) = variant_runtime_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(variant_staged_path, variant_runtime_path)?;
        }
        for variant_record in record.compressed_lod_variants.values() {
            let variant_staged_path = base_dir.join(&variant_record.staged_path);
            let variant_runtime_path = runtime_bundle_output_path(
                base_dir,
                &output_roots.runtime_public,
                &variant_record.runtime_path,
            )?;
            if let Some(parent) = variant_runtime_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(variant_staged_path, variant_runtime_path)?;
        }
        if let Some(compressed_variant) = record.compressed_variant.as_ref() {
            let compressed_staged_path = base_dir.join(&compressed_variant.staged_path);
            let compressed_runtime_path = runtime_bundle_output_path(
                base_dir,
                &output_roots.runtime_public,
                &compressed_variant.runtime_path,
            )?;
            if let Some(parent) = compressed_runtime_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(compressed_staged_path, compressed_runtime_path)?;
        }
    }

    Ok(())
}

fn runtime_bundle_output_path(
    base_dir: &Path,
    runtime_public_root: &str,
    runtime_path: &str,
) -> Result<PathBuf, RuntimeBundleError> {
    let runtime_public_root = base_dir.join(runtime_public_root);
    let runtime_public_prefix = runtime_public_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| RuntimeBundleError::InvalidRuntimePath(runtime_path.to_string()))?;
    let runtime_path = runtime_path.trim();
    if runtime_path.is_empty() {
        return Err(RuntimeBundleError::InvalidRuntimePath(
            runtime_path.to_string(),
        ));
    }

    let mut relative_path = PathBuf::new();
    let mut skipped_public_prefix = false;
    for component in Path::new(runtime_path).components() {
        match component {
            std::path::Component::RootDir | std::path::Component::CurDir => {}
            std::path::Component::Normal(value) => {
                if !skipped_public_prefix && value == runtime_public_prefix {
                    skipped_public_prefix = true;
                    continue;
                }
                skipped_public_prefix = true;
                relative_path.push(value);
            }
            _ => {
                return Err(RuntimeBundleError::InvalidRuntimePath(
                    runtime_path.to_string(),
                ))
            }
        }
    }

    if relative_path.as_os_str().is_empty() {
        return Err(RuntimeBundleError::InvalidRuntimePath(
            runtime_path.to_string(),
        ));
    }

    Ok(runtime_public_root.join(relative_path))
}

fn display_path_relative_to(base_dir: &Path, path: &Path) -> String {
    match path.strip_prefix(base_dir) {
        Ok(relative) => relative.to_string_lossy().to_string(),
        Err(_) => {
            let canonical_base = canonical_source_path(base_dir).ok();
            let canonical_path = canonical_source_path(path).ok();
            match (canonical_base.as_deref(), canonical_path.as_deref()) {
                (Some(base), Some(value)) => value
                    .strip_prefix(base)
                    .map(|relative| relative.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.display().to_string()),
                _ => path.display().to_string(),
            }
        }
    }
}

/// Single vertex record for procedural mesh processing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl Default for MeshVertex {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0; 2],
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// Triangle list mesh data for LOD processing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TriangleMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

impl TriangleMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Mesh processing metadata for a specific LOD level.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshLod {
    pub level: u8,
    pub stride: usize,
    pub mesh: TriangleMesh,
}

/// Ordered chain of simplified mesh levels.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MeshLodChain {
    pub source: AssetId,
    pub levels: Vec<MeshLod>,
}

/// Errors produced during mesh simplification.
#[derive(Debug)]
pub enum MeshProcessError {
    EmptyMesh,
    NonTriangularMesh {
        index_count: usize,
    },
    InvalidIndex {
        level: u8,
        source_index: usize,
        vertex_count: usize,
    },
}

impl fmt::Display for MeshProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMesh => write!(f, "mesh has no triangles"),
            Self::NonTriangularMesh { index_count } => {
                write!(f, "mesh indices must be grouped by 3 triangles, got {index_count}")
            }
            Self::InvalidIndex {
                level,
                source_index,
                vertex_count,
            } => write!(
                f,
                "LOD level {level} references vertex index {source_index}, but vertex count is {vertex_count}"
            ),
        }
    }
}

impl std::error::Error for MeshProcessError {}

/// Generate a deterministic LOD chain from a source mesh.
///
/// Each LOD level drops triangles by a stride that doubles every level:
/// 0 keeps every triangle, level 1 keeps every 2nd triangle, level 2 keeps every
/// 4th triangle, and so on. Vertices are compacted per-level so index buffers stay
/// contiguous and minimal.
pub fn generate_lod_chain(
    source_id: impl Into<AssetId>,
    source: &TriangleMesh,
    levels: u8,
) -> Result<MeshLodChain, MeshProcessError> {
    if source.indices.is_empty() {
        return Err(MeshProcessError::EmptyMesh);
    }
    if source.indices.len() % 3 != 0 {
        return Err(MeshProcessError::NonTriangularMesh {
            index_count: source.indices.len(),
        });
    }
    if source
        .indices
        .iter()
        .any(|index| (*index as usize) >= source.vertices.len())
    {
        let first_invalid = source
            .indices
            .iter()
            .find(|index| (**index as usize) >= source.vertices.len())
            .and_then(|index| (*index).try_into().ok())
            .unwrap_or(0);
        return Err(MeshProcessError::InvalidIndex {
            level: 0,
            source_index: first_invalid,
            vertex_count: source.vertices.len(),
        });
    }

    let mut levels_out = Vec::with_capacity(levels as usize);
    for level in 0..levels {
        let stride = usize::max(1, 1usize << level);
        let sampled = sample_indices_by_stride(&source.indices, stride);
        let mesh = compact_triangle_indices(&sampled, &source.vertices)?;
        levels_out.push(MeshLod {
            level,
            stride,
            mesh,
        });
    }

    Ok(MeshLodChain {
        source: source_id.into(),
        levels: levels_out,
    })
}

pub fn compress_texture(
    id: impl Into<AssetId>,
    format: AssetFormat,
    width: u32,
    height: u32,
    bytes: &[u8],
    profile: TextureCompressionProfile,
) -> Result<TextureCompressionArtifact, TextureProcessError> {
    let id = id.into();
    if width == 0 || height == 0 {
        return Err(TextureProcessError::InvalidTextureDimensions {
            texture: id,
            width,
            height,
        });
    }

    let compressed_bytes = match profile {
        TextureCompressionProfile::None => bytes.to_vec(),
        TextureCompressionProfile::Fast => run_length_compress(bytes),
        TextureCompressionProfile::Balanced => run_length_compress_with_header(bytes),
        TextureCompressionProfile::High => run_length_compress_with_header(bytes),
    };

    Ok(TextureCompressionArtifact {
        id,
        format,
        profile,
        width,
        height,
        original_size: bytes.len() as u64,
        compressed_size: compressed_bytes.len() as u64,
        bytes: compressed_bytes,
    })
}

fn run_length_compress(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(input.len());
    let mut cursor = 0usize;
    while cursor < input.len() {
        let value = input[cursor];
        let mut run = 1usize;
        while cursor + run < input.len() && input[cursor + run] == value && run < u8::MAX as usize {
            run += 1;
        }
        out.push(run as u8);
        out.push(value);
        cursor += run;
    }

    out
}

fn run_length_compress_with_header(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() + 4);
    out.extend_from_slice(b"RLE\0");
    out.extend(run_length_compress(input));
    out
}

pub fn pack_texture_atlas(
    atlas_id: impl Into<AssetId>,
    textures: &[AtlasTextureSource],
    options: &AtlasPackingOptions,
) -> Result<TextureAtlas, TextureProcessError> {
    if textures.is_empty() {
        return Err(TextureProcessError::NoTexturesToPack);
    }

    let mut cursor_x = 0u32;
    let mut cursor_y = 0u32;
    let mut row_height = 0u32;
    let mut used_width = 0u32;
    let mut used_height = 0u32;
    let mut placements = Vec::with_capacity(textures.len());

    for texture in textures {
        if texture.width == 0 || texture.height == 0 {
            return Err(TextureProcessError::InvalidTextureDimensions {
                texture: texture.id.clone(),
                width: texture.width,
                height: texture.height,
            });
        }
        if texture.width > options.max_width {
            return Err(TextureProcessError::TextureTooWideForAtlas {
                texture: texture.id.clone(),
                width: texture.width,
                max_width: options.max_width,
            });
        }

        if cursor_x != 0 && cursor_x.saturating_add(texture.width) > options.max_width {
            cursor_x = 0;
            cursor_y = cursor_y.saturating_add(row_height.saturating_add(options.padding));
            row_height = 0;
        }

        let placement = TexturePlacement {
            id: texture.id.clone(),
            x: cursor_x,
            y: cursor_y,
            width: texture.width,
            height: texture.height,
        };
        cursor_x = cursor_x
            .saturating_add(texture.width)
            .saturating_add(options.padding);
        row_height = row_height.max(texture.height);
        used_width = used_width.max(placement.right());
        used_height = used_height.max(placement.bottom());
        placements.push(placement);
    }

    Ok(TextureAtlas {
        id: atlas_id.into(),
        width: used_width,
        height: used_height,
        padding: options.padding,
        placements,
    })
}

fn sample_indices_by_stride(indices: &[u32], triangle_stride: usize) -> Vec<u32> {
    let mut sampled = Vec::new();
    let triangle_count = indices.len() / 3;
    if triangle_count == 0 {
        return sampled;
    }

    let step = usize::max(1, triangle_stride);
    for triangle_index in (0..triangle_count).step_by(step) {
        let base = triangle_index * 3;
        sampled.extend_from_slice(&indices[base..base + 3]);
    }

    sampled
}

fn compact_triangle_indices(
    indices: &[u32],
    source_vertices: &[MeshVertex],
) -> Result<TriangleMesh, MeshProcessError> {
    let mut remap: HashMap<u32, u32> = HashMap::new();
    let mut vertices = Vec::new();
    let mut compacted_indices = Vec::with_capacity(indices.len());

    for index in indices {
        let target_index = if let Some(index) = remap.get(index) {
            *index
        } else {
            let next = vertices.len() as u32;
            let source_index = *index as usize;
            if source_index >= source_vertices.len() {
                return Err(MeshProcessError::InvalidIndex {
                    level: 0,
                    source_index,
                    vertex_count: source_vertices.len(),
                });
            }
            vertices.push(source_vertices[source_index].clone());
            remap.insert(*index, next);
            next
        };
        compacted_indices.push(target_index);
    }

    Ok(TriangleMesh {
        vertices,
        indices: compacted_indices,
    })
}

/// Deterministic content-address for an asset source path.
///
/// Returns a fallback path-based identifier when hashing fails, preserving
/// backward compatibility for deterministic identifiers in non-file contexts.
pub fn normalize_asset_id(source_path: impl AsRef<Path>) -> AssetId {
    let path = source_path.as_ref();
    let source_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let checksum = compute_checksum(&source_path)
        .or_else(|_| source_to_fallback_id(&source_path))
        .unwrap_or_else(|_| "unknown:asset".to_string());
    AssetId::from(format!("sha256:{checksum}"))
}

fn compute_checksum(source_path: &Path) -> Result<String, AssetCacheError> {
    let bytes = fs::read(source_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hash_to_hex(hasher.finalize().to_vec()))
}

fn source_to_fallback_id(path: &Path) -> Result<String, AssetCacheError> {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    Ok(hash_to_hex(hasher.finalize().to_vec()))
}

fn canonical_source_path(source_path: impl AsRef<Path>) -> Result<PathBuf, AssetCacheError> {
    let source_path = source_path.as_ref();
    source_path
        .canonicalize()
        .map_err(|_| AssetCacheError::InvalidSourcePath(source_path.to_path_buf()))
}

fn hash_to_hex(bytes: Vec<u8>) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fallback_canonicalized_source(path: &Path) -> Option<PathBuf> {
    path.file_name().and_then(|file_name| {
        path.parent()
            .and_then(|parent| parent.canonicalize().ok())
            .map(|parent| parent.join(file_name))
    })
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn temp_file_path(name: &str) -> PathBuf {
        let mut base = std::env::temp_dir();
        let unique = Uuid::new_v4().to_string();
        base.push(format!("{unique}_{name}"));
        base
    }

    #[test]
    fn asset_cache_indexing_is_deduplicated() {
        let mut cache = AssetCache::new();
        let source_a = temp_file_path("same_content_a.bin");
        let source_b = temp_file_path("same_content_b.bin");
        fs::write(&source_a, b"asset-payload").unwrap();
        fs::write(&source_b, b"asset-payload").unwrap();

        let id_a = cache
            .index_or_refresh(&source_a, PathBuf::from("build/assets/asset_a"))
            .expect("indexing should succeed");
        let id_b = cache
            .index_or_refresh(&source_b, PathBuf::from("build/assets/asset_b"))
            .expect("indexing should succeed");

        assert_eq!(id_a, id_b);
        assert_eq!(cache.total(), 1);
        assert!(cache.contains(&id_a));
        assert_eq!(
            cache.get_by_source(&source_a).expect("record exists").state,
            AssetState::Pending
        );
    }

    #[test]
    fn asset_cache_refresh_updates_checksum_changes() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("refresh.bin");
        fs::write(&source, b"first-version").unwrap();
        let first_id = cache
            .index_or_refresh(&source, PathBuf::from("build/assets/refresh"))
            .expect("indexing should succeed");

        fs::write(&source, b"second-version").unwrap();
        let second_id = cache
            .index_or_refresh(&source, PathBuf::from("build/assets/refresh"))
            .expect("indexing should succeed");

        assert_ne!(first_id, second_id);
        assert_eq!(cache.total(), 1);
        assert_eq!(cache.get(&second_id).expect("record exists").byte_len, 14);
        assert!(cache.get(&first_id).is_none());
    }

    #[test]
    fn asset_cache_allows_content_sharing_with_one_modified_source() {
        let mut cache = AssetCache::new();
        let shared_a = temp_file_path("shared-a.bin");
        let shared_b = temp_file_path("shared-b.bin");
        fs::write(&shared_a, b"shared-content").unwrap();
        fs::write(&shared_b, b"shared-content").unwrap();

        let first_id = cache
            .index_or_refresh(&shared_a, PathBuf::from("build/assets/a"))
            .expect("indexing should succeed");
        let second_id = cache
            .index_or_refresh(&shared_b, PathBuf::from("build/assets/b"))
            .expect("indexing should succeed");
        let shared_b_id = cache
            .get_by_source(&shared_b)
            .expect("source b should be indexed")
            .id
            .clone();

        assert_eq!(first_id, second_id);
        assert_eq!(second_id, shared_b_id);
        assert_eq!(cache.total(), 1);

        fs::write(&shared_a, b"unique-content").unwrap();
        let updated_id = cache
            .index_or_refresh(&shared_a, PathBuf::from("build/assets/a"))
            .expect("indexing should succeed");
        let shared_b_after = cache
            .get_by_source(&shared_b)
            .expect("source b should remain")
            .id
            .clone();
        let shared_a_after = cache
            .get_by_source(&shared_a)
            .expect("source a should be reindexed")
            .id
            .clone();

        assert_ne!(updated_id, shared_b_after);
        assert_eq!(shared_a_after, updated_id);
        assert_eq!(shared_b_after, shared_b_id);
        assert_eq!(cache.total(), 2);
    }

    #[test]
    fn normalize_asset_id_is_deterministic() {
        let source = temp_file_path("normalize.bin");
        fs::write(&source, b"normalized").unwrap();

        assert_eq!(
            normalize_asset_id(&source),
            normalize_asset_id(&source),
            "normalize_asset_id should be deterministic"
        );
    }

    #[test]
    fn import_asset_builds_deduplicated_plan_for_known_formats() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("mesh.obj");
        let output_root = temp_file_path("import-root");
        fs::write(&source, b"obj-bytes").unwrap();
        let canonical_source = source.canonicalize().expect("source path canonicalizes");

        let first =
            import_asset(&mut cache, &source, &output_root).expect("import should be scheduled");
        let second =
            import_asset(&mut cache, &source, &output_root).expect("import should be scheduled");

        assert_eq!(first.id, second.id);
        assert_eq!(first.format, AssetFormat::Obj);
        assert_eq!(first.byte_len, 9);
        assert_eq!(first.source_path, canonical_source);
        assert_eq!(cache.total(), 1);
        assert_eq!(second.source_path, canonical_source);
        assert!(first.imported_path.exists());
        assert!(first.imported_path.to_string_lossy().contains("/obj/"));
        assert!(first.imported_path.to_string_lossy().ends_with(".obj"));
        assert_eq!(fs::read(&first.imported_path).unwrap(), b"obj-bytes");
    }

    #[test]
    fn import_asset_preserves_gltf_family_extensions_when_staging_sources() {
        let output_root = temp_file_path("import-root-gltf-family");

        for extension in ["gltf", "glb"] {
            let mut cache = AssetCache::new();
            let source = temp_file_path(&format!("mesh.{extension}"));
            let source_bytes = format!("{extension}-bytes").into_bytes();
            fs::write(&source, &source_bytes).unwrap();

            let import =
                import_asset(&mut cache, &source, &output_root).expect("gltf import should work");

            assert_eq!(import.format, AssetFormat::Gltf);
            assert!(import.imported_path.exists());
            assert!(import.imported_path.to_string_lossy().contains("/gltf/"));
            assert!(import
                .imported_path
                .to_string_lossy()
                .ends_with(&format!(".{extension}")));
            assert_eq!(fs::read(&import.imported_path).unwrap(), source_bytes);
        }
    }

    #[test]
    fn import_asset_preserves_ktx2_extension_when_staging_sources() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("selection-ring.ktx2");
        let output_root = temp_file_path("import-root-ktx2");
        let source_bytes = b"ktx2-bytes";
        fs::write(&source, source_bytes).unwrap();

        let import =
            import_asset(&mut cache, &source, &output_root).expect("ktx2 import should succeed");

        assert_eq!(import.format, AssetFormat::Ktx2);
        assert!(import.imported_path.exists());
        assert!(import.imported_path.to_string_lossy().contains("/ktx2/"));
        assert!(import.imported_path.to_string_lossy().ends_with(".ktx2"));
        assert_eq!(fs::read(&import.imported_path).unwrap(), source_bytes);
    }

    #[test]
    fn import_asset_stages_svg_sources_as_content_addressed_artifacts() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("selection-ring.svg");
        let output_root = temp_file_path("import-root-svg");
        let source_bytes = br#"<svg viewBox="0 0 1 1"></svg>"#;
        fs::write(&source, source_bytes).unwrap();

        let import =
            import_asset(&mut cache, &source, &output_root).expect("svg import should succeed");

        assert_eq!(import.format, AssetFormat::Svg);
        assert!(import.imported_path.exists());
        assert!(import.imported_path.to_string_lossy().contains("/svg/"));
        assert!(import.imported_path.to_string_lossy().ends_with(".svg"));
        assert_eq!(fs::read(&import.imported_path).unwrap(), source_bytes);
    }

    #[test]
    fn build_runtime_bundle_manifest_maps_staged_imports_to_runtime_paths() {
        let mut cache = AssetCache::new();
        let app_root = temp_file_path("bundle-app-root");
        let source_root = app_root.join("artifacts/source-assets");
        let output_root = app_root.join("artifacts/staged-assets");
        fs::create_dir_all(source_root.join("meshes")).unwrap();
        fs::create_dir_all(source_root.join("textures")).unwrap();

        let mesh_source = source_root.join("meshes/hero.glb");
        let mesh_lod1_source = source_root.join("meshes/hero.lod1.glb");
        let mesh_compressed_source = source_root.join("meshes/hero.meshopt.glb");
        let sprite_source = source_root.join("textures/focus-ring.svg");
        let sprite_ktx2_source = source_root.join("textures/focus-ring.ktx2");
        fs::write(&mesh_source, b"glb-bytes").unwrap();
        fs::write(&mesh_lod1_source, b"glb-lod1-bytes").unwrap();
        fs::write(&mesh_compressed_source, b"glb-meshopt-bytes").unwrap();
        fs::write(&sprite_source, br#"<svg></svg>"#).unwrap();
        fs::write(&sprite_ktx2_source, b"ktx2-bytes").unwrap();

        let mesh_import = import_asset(&mut cache, &mesh_source, &output_root).unwrap();
        let mesh_lod1_import = import_asset(&mut cache, &mesh_lod1_source, &output_root).unwrap();
        let mesh_compressed_import =
            import_asset(&mut cache, &mesh_compressed_source, &output_root).unwrap();
        let sprite_import = import_asset(&mut cache, &sprite_source, &output_root).unwrap();
        let sprite_ktx2_import =
            import_asset(&mut cache, &sprite_ktx2_source, &output_root).unwrap();

        let manifest = build_runtime_bundle_manifest(
            &RuntimeBundleSpec {
                output_roots: RuntimeBundleOutputRoots {
                    source: "artifacts/source-assets".to_string(),
                    staged: "artifacts/staged-assets".to_string(),
                    runtime_public: "public/assets".to_string(),
                },
                meshes: BTreeMap::from([(
                    "hero".to_string(),
                    RuntimeBundleAssetSpec {
                        source_path: mesh_source.clone(),
                        runtime_path: "/assets/meshes/hero.glb".to_string(),
                        lod_variants: BTreeMap::from([(
                            "1".to_string(),
                            RuntimeBundleMeshLodVariantSpec {
                                source_path: mesh_lod1_source.clone(),
                                runtime_path: "/assets/meshes/hero.lod1.glb".to_string(),
                            },
                        )]),
                        compressed_lod_variants: BTreeMap::from([(
                            "0".to_string(),
                            RuntimeBundleMeshLodVariantSpec {
                                source_path: mesh_compressed_source.clone(),
                                runtime_path: "/assets/meshes/hero.meshopt.glb".to_string(),
                            },
                        )]),
                    },
                )]),
                sprites: BTreeMap::from([(
                    "focus-ring".to_string(),
                    RuntimeBundleSpriteSpec {
                        source_path: sprite_source.clone(),
                        runtime_path: "/assets/textures/focus-ring.svg".to_string(),
                        compressed_variant: Some(RuntimeBundleCompressedTextureVariantSpec {
                            source_path: sprite_ktx2_source.clone(),
                            runtime_path: "/assets/textures/focus-ring.ktx2".to_string(),
                        }),
                    },
                )]),
            },
            &[
                mesh_import.clone(),
                mesh_lod1_import.clone(),
                mesh_compressed_import.clone(),
                sprite_import.clone(),
                sprite_ktx2_import.clone(),
            ],
            &app_root,
        )
        .unwrap();

        assert_eq!(manifest.version, 1);
        assert_eq!(
            manifest.meshes["hero"],
            RuntimeBundleAssetRecord {
                source_path: "artifacts/source-assets/meshes/hero.glb".to_string(),
                staged_id: mesh_import.id.to_string(),
                staged_format: "gltf".to_string(),
                staged_path: format!(
                    "artifacts/staged-assets/gltf/{}.glb",
                    mesh_import.id.as_filename()
                ),
                runtime_path: "/assets/meshes/hero.glb".to_string(),
                compressed_variant: None,
                lod_variants: BTreeMap::from([(
                    "1".to_string(),
                    RuntimeBundleVariantRecord {
                        source_path: "artifacts/source-assets/meshes/hero.lod1.glb".to_string(),
                        staged_id: mesh_lod1_import.id.to_string(),
                        staged_format: "gltf".to_string(),
                        staged_path: format!(
                            "artifacts/staged-assets/gltf/{}.glb",
                            mesh_lod1_import.id.as_filename()
                        ),
                        runtime_path: "/assets/meshes/hero.lod1.glb".to_string(),
                    },
                )]),
                compressed_lod_variants: BTreeMap::from([(
                    "0".to_string(),
                    RuntimeBundleVariantRecord {
                        source_path: "artifacts/source-assets/meshes/hero.meshopt.glb".to_string(),
                        staged_id: mesh_compressed_import.id.to_string(),
                        staged_format: "gltf".to_string(),
                        staged_path: format!(
                            "artifacts/staged-assets/gltf/{}.glb",
                            mesh_compressed_import.id.as_filename()
                        ),
                        runtime_path: "/assets/meshes/hero.meshopt.glb".to_string(),
                    },
                )]),
            }
        );
        assert_eq!(
            manifest.sprites["focus-ring"],
            RuntimeBundleAssetRecord {
                source_path: "artifacts/source-assets/textures/focus-ring.svg".to_string(),
                staged_id: sprite_import.id.to_string(),
                staged_format: "svg".to_string(),
                staged_path: format!(
                    "artifacts/staged-assets/svg/{}.svg",
                    sprite_import.id.as_filename()
                ),
                runtime_path: "/assets/textures/focus-ring.svg".to_string(),
                compressed_variant: Some(RuntimeBundleCompressedTextureVariantRecord {
                    source_path: "artifacts/source-assets/textures/focus-ring.ktx2".to_string(),
                    staged_id: sprite_ktx2_import.id.to_string(),
                    staged_format: "ktx2".to_string(),
                    staged_path: format!(
                        "artifacts/staged-assets/ktx2/{}.ktx2",
                        sprite_ktx2_import.id.as_filename()
                    ),
                    runtime_path: "/assets/textures/focus-ring.ktx2".to_string(),
                }),
                lod_variants: BTreeMap::new(),
                compressed_lod_variants: BTreeMap::new(),
            }
        );
    }

    #[test]
    fn build_runtime_bundle_manifest_rejects_duplicate_runtime_paths() {
        let mut cache = AssetCache::new();
        let app_root = temp_file_path("duplicate-runtime-path-app-root");
        let source_root = app_root.join("artifacts/source-assets");
        let output_root = app_root.join("artifacts/staged-assets");
        fs::create_dir_all(source_root.join("meshes")).unwrap();
        fs::create_dir_all(source_root.join("textures")).unwrap();

        let mesh_source = source_root.join("meshes/hero.glb");
        let sprite_source = source_root.join("textures/focus-ring.svg");
        fs::write(&mesh_source, b"glb-bytes").unwrap();
        fs::write(&sprite_source, br#"<svg></svg>"#).unwrap();

        let mesh_import = import_asset(&mut cache, &mesh_source, &output_root).unwrap();
        let sprite_import = import_asset(&mut cache, &sprite_source, &output_root).unwrap();

        let error = build_runtime_bundle_manifest(
            &RuntimeBundleSpec {
                output_roots: RuntimeBundleOutputRoots {
                    source: "artifacts/source-assets".to_string(),
                    staged: "artifacts/staged-assets".to_string(),
                    runtime_public: "public/assets".to_string(),
                },
                meshes: BTreeMap::from([(
                    "hero".to_string(),
                    RuntimeBundleAssetSpec {
                        source_path: mesh_source.clone(),
                        runtime_path: "/assets/shared/asset.bin".to_string(),
                        lod_variants: BTreeMap::new(),
                        compressed_lod_variants: BTreeMap::new(),
                    },
                )]),
                sprites: BTreeMap::from([(
                    "focus-ring".to_string(),
                    RuntimeBundleSpriteSpec {
                        source_path: sprite_source.clone(),
                        runtime_path: "/assets/shared/asset.bin".to_string(),
                        compressed_variant: None,
                    },
                )]),
            },
            &[mesh_import, sprite_import],
            &app_root,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            RuntimeBundleError::DuplicateRuntimePath {
                runtime_path,
                first_asset_id,
                second_asset_id
            } if runtime_path == "/assets/shared/asset.bin"
                && first_asset_id == "hero"
                && second_asset_id == "focus-ring"
        ));
    }

    #[test]
    fn build_runtime_bundle_manifest_rejects_non_ktx2_compressed_texture_variant() {
        let mut cache = AssetCache::new();
        let app_root = temp_file_path("invalid-compressed-runtime-app-root");
        let source_root = app_root.join("artifacts/source-assets");
        let output_root = app_root.join("artifacts/staged-assets");
        fs::create_dir_all(source_root.join("textures")).unwrap();

        let sprite_source = source_root.join("textures/focus-ring.svg");
        let invalid_compressed_source = source_root.join("textures/focus-ring.png");
        fs::write(&sprite_source, br#"<svg></svg>"#).unwrap();
        fs::write(&invalid_compressed_source, b"png-bytes").unwrap();

        let sprite_import = import_asset(&mut cache, &sprite_source, &output_root).unwrap();
        let invalid_compressed_import =
            import_asset(&mut cache, &invalid_compressed_source, &output_root).unwrap();

        let error = build_runtime_bundle_manifest(
            &RuntimeBundleSpec {
                output_roots: RuntimeBundleOutputRoots {
                    source: "artifacts/source-assets".to_string(),
                    staged: "artifacts/staged-assets".to_string(),
                    runtime_public: "public/assets".to_string(),
                },
                meshes: BTreeMap::new(),
                sprites: BTreeMap::from([(
                    "focus-ring".to_string(),
                    RuntimeBundleSpriteSpec {
                        source_path: sprite_source.clone(),
                        runtime_path: "/assets/textures/focus-ring.svg".to_string(),
                        compressed_variant: Some(RuntimeBundleCompressedTextureVariantSpec {
                            source_path: invalid_compressed_source.clone(),
                            runtime_path: "/assets/textures/focus-ring.ktx2".to_string(),
                        }),
                    },
                )]),
            },
            &[sprite_import, invalid_compressed_import],
            &app_root,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            RuntimeBundleError::UnsupportedCompressedTextureFormat { asset_id, format }
                if asset_id == "focus-ring" && format == AssetFormat::Png
        ));
    }

    #[test]
    fn build_runtime_bundle_manifest_rejects_non_gltf_compressed_mesh_variant() {
        let mut cache = AssetCache::new();
        let app_root = temp_file_path("invalid-compressed-mesh-runtime-app-root");
        let source_root = app_root.join("artifacts/source-assets");
        let output_root = app_root.join("artifacts/staged-assets");
        fs::create_dir_all(source_root.join("meshes")).unwrap();

        let mesh_source = source_root.join("meshes/hero.glb");
        let invalid_compressed_source = source_root.join("meshes/hero.meshopt.ktx2");
        fs::write(&mesh_source, b"glb-bytes").unwrap();
        fs::write(&invalid_compressed_source, b"ktx2-bytes").unwrap();

        let mesh_import = import_asset(&mut cache, &mesh_source, &output_root).unwrap();
        let invalid_compressed_import =
            import_asset(&mut cache, &invalid_compressed_source, &output_root).unwrap();

        let error = build_runtime_bundle_manifest(
            &RuntimeBundleSpec {
                output_roots: RuntimeBundleOutputRoots {
                    source: "artifacts/source-assets".to_string(),
                    staged: "artifacts/staged-assets".to_string(),
                    runtime_public: "public/assets".to_string(),
                },
                meshes: BTreeMap::from([(
                    "hero".to_string(),
                    RuntimeBundleAssetSpec {
                        source_path: mesh_source.clone(),
                        runtime_path: "/assets/meshes/hero.glb".to_string(),
                        lod_variants: BTreeMap::new(),
                        compressed_lod_variants: BTreeMap::from([(
                            "0".to_string(),
                            RuntimeBundleMeshLodVariantSpec {
                                source_path: invalid_compressed_source.clone(),
                                runtime_path: "/assets/meshes/hero.meshopt.glb".to_string(),
                            },
                        )]),
                    },
                )]),
                sprites: BTreeMap::new(),
            },
            &[mesh_import, invalid_compressed_import],
            &app_root,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            RuntimeBundleError::UnsupportedCompressedMeshFormat { asset_id, format }
                if asset_id == "hero" && format == AssetFormat::Ktx2
        ));
    }

    #[test]
    fn materialize_runtime_bundle_manifest_copies_staged_assets_into_runtime_public_root() {
        let mut cache = AssetCache::new();
        let app_root = temp_file_path("materialized-bundle-app-root");
        let source_root = app_root.join("artifacts/source-assets");
        let output_root = app_root.join("artifacts/staged-assets");
        fs::create_dir_all(source_root.join("meshes")).unwrap();
        fs::create_dir_all(source_root.join("textures")).unwrap();

        let mesh_source = source_root.join("meshes/hero.glb");
        let mesh_lod2_source = source_root.join("meshes/hero.lod2.glb");
        let mesh_compressed_source = source_root.join("meshes/hero.meshopt.glb");
        let sprite_source = source_root.join("textures/focus-ring.svg");
        let sprite_ktx2_source = source_root.join("textures/focus-ring.ktx2");
        let mesh_bytes = b"glb-runtime-bytes";
        let mesh_lod2_bytes = b"glb-runtime-lod2-bytes";
        let mesh_compressed_bytes = b"glb-runtime-meshopt-bytes";
        let sprite_bytes = br#"<svg viewBox="0 0 1 1"></svg>"#;
        let sprite_ktx2_bytes = b"ktx2-runtime-bytes";
        fs::write(&mesh_source, mesh_bytes).unwrap();
        fs::write(&mesh_lod2_source, mesh_lod2_bytes).unwrap();
        fs::write(&mesh_compressed_source, mesh_compressed_bytes).unwrap();
        fs::write(&sprite_source, sprite_bytes).unwrap();
        fs::write(&sprite_ktx2_source, sprite_ktx2_bytes).unwrap();

        let mesh_import = import_asset(&mut cache, &mesh_source, &output_root).unwrap();
        let mesh_lod2_import = import_asset(&mut cache, &mesh_lod2_source, &output_root).unwrap();
        let mesh_compressed_import =
            import_asset(&mut cache, &mesh_compressed_source, &output_root).unwrap();
        let sprite_import = import_asset(&mut cache, &sprite_source, &output_root).unwrap();
        let sprite_ktx2_import =
            import_asset(&mut cache, &sprite_ktx2_source, &output_root).unwrap();

        let manifest = build_runtime_bundle_manifest(
            &RuntimeBundleSpec {
                output_roots: RuntimeBundleOutputRoots {
                    source: "artifacts/source-assets".to_string(),
                    staged: "artifacts/staged-assets".to_string(),
                    runtime_public: "public/assets".to_string(),
                },
                meshes: BTreeMap::from([(
                    "hero".to_string(),
                    RuntimeBundleAssetSpec {
                        source_path: mesh_source.clone(),
                        runtime_path: "/assets/meshes/hero.glb".to_string(),
                        lod_variants: BTreeMap::from([(
                            "2".to_string(),
                            RuntimeBundleMeshLodVariantSpec {
                                source_path: mesh_lod2_source.clone(),
                                runtime_path: "/assets/meshes/hero.lod2.glb".to_string(),
                            },
                        )]),
                        compressed_lod_variants: BTreeMap::from([(
                            "0".to_string(),
                            RuntimeBundleMeshLodVariantSpec {
                                source_path: mesh_compressed_source.clone(),
                                runtime_path: "/assets/meshes/hero.meshopt.glb".to_string(),
                            },
                        )]),
                    },
                )]),
                sprites: BTreeMap::from([(
                    "focus-ring".to_string(),
                    RuntimeBundleSpriteSpec {
                        source_path: sprite_source.clone(),
                        runtime_path: "/assets/textures/focus-ring.svg".to_string(),
                        compressed_variant: Some(RuntimeBundleCompressedTextureVariantSpec {
                            source_path: sprite_ktx2_source.clone(),
                            runtime_path: "/assets/textures/focus-ring.ktx2".to_string(),
                        }),
                    },
                )]),
            },
            &[
                mesh_import,
                mesh_lod2_import,
                mesh_compressed_import,
                sprite_import,
                sprite_ktx2_import,
            ],
            &app_root,
        )
        .unwrap();

        materialize_runtime_bundle_manifest(&manifest, &app_root).unwrap();

        assert_eq!(
            fs::read(app_root.join("public/assets/meshes/hero.glb")).unwrap(),
            mesh_bytes
        );
        assert_eq!(
            fs::read(app_root.join("public/assets/meshes/hero.lod2.glb")).unwrap(),
            mesh_lod2_bytes
        );
        assert_eq!(
            fs::read(app_root.join("public/assets/meshes/hero.meshopt.glb")).unwrap(),
            mesh_compressed_bytes
        );
        assert_eq!(
            fs::read_to_string(app_root.join("public/assets/textures/focus-ring.svg")).unwrap(),
            String::from_utf8(sprite_bytes.to_vec()).unwrap()
        );
        assert_eq!(
            fs::read(app_root.join("public/assets/textures/focus-ring.ktx2")).unwrap(),
            sprite_ktx2_bytes
        );
    }

    #[test]
    fn import_godot_scene_preserves_hierarchy_and_native_components() {
        let source = temp_file_path("demo_level.tscn");
        fs::write(
            &source,
            r#"
[gd_scene load_steps=2 format=3]

[ext_resource type="Texture2D" path="res://art/player.png" id="1_tex"]

[node name="Root" type="Node2D"]
position = Vector2(1, 2)

[node name="Player" type="Sprite2D" parent="."]
position = Vector2(4, 5)
scale = Vector2(1.5, 2.0)
rotation_degrees = 90
texture = ExtResource("1_tex")
frame = 3
z_index = 7
visible = false
modulate = Color(0.25, 0.5, 0.75, 1.0)

[node name="Fog" type="ColorRect" parent="."]
position = Vector2(-2, 6)
size = Vector2(32, 18)
color = Color8(10, 20, 30, 128)
z_index = -2
"#,
        )
        .unwrap();

        let scene = import_godot_scene(&source).expect("scene should import");
        assert!(scene.metadata.name.ends_with("demo_level"));
        assert_eq!(scene.entities.len(), 3);

        let root = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Root")
            .expect("root entity should exist");
        let player = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Player")
            .expect("player entity should exist");
        let fog = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Fog")
            .expect("fog entity should exist");

        let root_transform = root
            .get_native_component::<Transform>()
            .expect("transform should deserialize")
            .expect("root transform should exist");
        assert!(root_transform
            .position
            .abs_diff_eq(Vec2::new(1.0, 2.0), 1e-6));

        let player_transform = player
            .get_native_component::<Transform>()
            .expect("transform should deserialize")
            .expect("player transform should exist");
        assert!(player_transform
            .position
            .abs_diff_eq(Vec2::new(4.0, 5.0), 1e-6));
        assert!(player_transform
            .scale
            .abs_diff_eq(Vec2::new(1.5, 2.0), 1e-6));
        assert!((player_transform.rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-6);

        let player_sprite = player
            .get_native_component::<Sprite>()
            .expect("sprite should deserialize")
            .expect("player sprite should exist");
        assert_eq!(player_sprite.texture, "art/player.png");
        assert_eq!(player_sprite.frame, 3);
        assert_eq!(player_sprite.layer, 7);
        assert!(!player_sprite.visible);
        assert_eq!(player_sprite.color, [0.25, 0.5, 0.75, 1.0]);

        let fog_rect = fog
            .get_native_component::<ColorRect>()
            .expect("color rect should deserialize")
            .expect("fog rect should exist");
        assert_eq!(fog_rect.width, 32.0);
        assert_eq!(fog_rect.height, 18.0);
        assert_eq!(fog_rect.layer, -2);
        assert!((fog_rect.color[0] - (10.0 / 255.0)).abs() < 1e-6);
        assert!((fog_rect.color[1] - (20.0 / 255.0)).abs() < 1e-6);
        assert!((fog_rect.color[2] - (30.0 / 255.0)).abs() < 1e-6);
        assert!((fog_rect.color[3] - (128.0 / 255.0)).abs() < 1e-6);

        assert_eq!(scene.graph.get_parent(player.id), Some(root.id));
        assert_eq!(scene.graph.get_parent(fog.id), Some(root.id));
    }

    #[test]
    fn import_asset_writes_serialized_scene_artifact_for_godot_text_scenes() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("combat_room.tscn");
        let output_root = temp_file_path("import-root-scene");
        fs::write(
            &source,
            r#"
[gd_scene load_steps=1 format=3]

[node name="Room" type="Node2D"]
position = Vector2(0, 0)
"#,
        )
        .unwrap();

        assert_eq!(
            AssetFormat::from_path(&source),
            Some(AssetFormat::GodotScene)
        );

        let import =
            import_asset(&mut cache, &source, &output_root).expect("scene import should succeed");
        assert_eq!(import.format, AssetFormat::GodotScene);
        assert!(import.imported_path.exists());
        assert!(import.imported_path.to_string_lossy().contains("/scene/"));
        assert!(import
            .imported_path
            .to_string_lossy()
            .ends_with(".scene.json"));

        let imported_scene: Scene = serde_json::from_slice(
            &fs::read(&import.imported_path).expect("artifact should exist"),
        )
        .expect("artifact should deserialize to pod_scene::Scene");
        assert!(imported_scene.metadata.name.ends_with("combat_room"));
        assert_eq!(imported_scene.entities.len(), 1);
        assert_eq!(cache.total(), 1);
    }

    #[test]
    fn import_tiled_scene_preserves_layer_hierarchy_and_native_components() {
        let source = temp_file_path("dungeon_floor.tmj");
        fs::write(
            &source,
            r##"
{
  "name": "DungeonFloor",
  "width": 2,
  "height": 2,
  "tilewidth": 16,
  "tileheight": 16,
  "layers": [
    {
      "type": "tilelayer",
      "name": "Ground",
      "width": 2,
      "height": 2,
      "visible": true,
      "opacity": 0.5,
      "tintcolor": "#80FF8040",
      "data": [1, 0, 2, 0]
    },
    {
      "type": "objectgroup",
      "name": "Props",
      "objects": [
        { "id": 1, "name": "Chest", "x": 32, "y": 16, "width": 16, "height": 16 },
        { "id": 2, "name": "Torch", "x": 48, "y": 16, "width": 16, "height": 16, "gid": 1, "rotation": 90 }
      ]
    }
  ],
  "tilesets": [
    { "firstgid": 1, "image": "tiles/dungeon.png" }
  ]
}
"##,
        )
        .unwrap();

        let scene = import_tiled_scene(&source).expect("tiled scene should import");
        assert_eq!(scene.metadata.name, "DungeonFloor");

        let ground = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Ground")
            .expect("ground layer should exist");
        let tile_a = scene
            .entities
            .iter()
            .find(|entity| entity.name == "tile_0_0")
            .expect("first tile should exist");
        let tile_b = scene
            .entities
            .iter()
            .find(|entity| entity.name == "tile_0_1")
            .expect("second tile should exist");
        let chest = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Chest")
            .expect("chest object should exist");
        let torch = scene
            .entities
            .iter()
            .find(|entity| entity.name == "Torch")
            .expect("torch object should exist");

        let tile_a_transform = tile_a
            .get_native_component::<Transform>()
            .expect("tile transform should deserialize")
            .expect("tile transform should exist");
        assert!(tile_a_transform
            .position
            .abs_diff_eq(Vec2::new(8.0, 8.0), 1e-6));

        let tile_b_sprite = tile_b
            .get_native_component::<Sprite>()
            .expect("tile sprite should deserialize")
            .expect("tile sprite should exist");
        assert_eq!(tile_b_sprite.texture, "tiles/dungeon.png");
        assert_eq!(tile_b_sprite.frame, 1);
        assert!((tile_b_sprite.color[3] - (128.0 / 255.0 * 0.5)).abs() < 1e-6);

        let chest_rect = chest
            .get_native_component::<ColorRect>()
            .expect("chest rect should deserialize")
            .expect("chest rect should exist");
        assert_eq!(chest_rect.width, 16.0);
        assert_eq!(chest_rect.height, 16.0);

        let torch_transform = torch
            .get_native_component::<Transform>()
            .expect("torch transform should deserialize")
            .expect("torch transform should exist");
        assert!((torch_transform.rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-6);

        assert_eq!(scene.graph.get_parent(tile_a.id), Some(ground.id));
        assert_eq!(scene.graph.get_parent(tile_b.id), Some(ground.id));
    }

    #[test]
    fn import_asset_writes_serialized_scene_artifact_for_tiled_maps() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("arena.tmj");
        let output_root = temp_file_path("import-root-tiled");
        fs::write(
            &source,
            r##"
{
  "width": 1,
  "height": 1,
  "tilewidth": 16,
  "tileheight": 16,
  "layers": [
    { "type": "tilelayer", "name": "Ground", "width": 1, "height": 1, "data": [1] }
  ],
  "tilesets": [
    { "firstgid": 1, "image": "tiles/arena.png" }
  ]
}
"##,
        )
        .unwrap();

        assert_eq!(AssetFormat::from_path(&source), Some(AssetFormat::TiledMap));

        let import =
            import_asset(&mut cache, &source, &output_root).expect("tiled import should succeed");
        assert_eq!(import.format, AssetFormat::TiledMap);
        assert!(import.imported_path.exists());

        let imported_scene: Scene = serde_json::from_slice(
            &fs::read(&import.imported_path).expect("artifact should exist"),
        )
        .expect("artifact should deserialize to pod_scene::Scene");
        assert!(imported_scene.metadata.name.ends_with("arena"));
        assert!(imported_scene
            .entities
            .iter()
            .any(|entity| entity.name == "tile_0_0"));
        assert_eq!(cache.total(), 1);
    }

    #[test]
    fn import_asset_rejects_unsupported_extension() {
        let mut cache = AssetCache::new();
        let source = temp_file_path("bad.txt");
        let output_root = temp_file_path("import-root-unsupported");
        fs::write(&source, b"bad").unwrap();

        let result = import_asset(&mut cache, &source, &output_root);
        assert!(matches!(
            result,
            Err(AssetImportError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn reprocess_path_reimports_supported_asset() {
        let mut watch_root = temp_file_path("hot-reload");
        fs::create_dir_all(&watch_root).unwrap();
        let output_root = temp_file_path("hot-reload-output");
        let source = watch_root.join("mesh.obj");
        fs::write(&source, b"obj-bytes").unwrap();

        let cache = Arc::new(Mutex::new(AssetCache::new()));
        let mut reloader = AssetHotReloader::start(
            &[&watch_root],
            Arc::clone(&cache),
            &output_root,
            true,
            default_asset_reprocessor,
        )
        .expect("watcher should start");

        let event = reloader.reprocess_path(&source);
        let imported = match event.result {
            AssetHotReloadResult::Reprocessed { id: _, import } => import,
            _ => panic!("supported file should be reprocessed"),
        };
        assert_eq!(imported.format, AssetFormat::Obj);
        assert_eq!(
            cache
                .lock()
                .expect("cache should lock")
                .contains(&imported.id),
            true
        );
    }

    #[test]
    fn reprocess_path_removes_cache_entry_for_deleted_assets() {
        let mut watch_root = temp_file_path("hot-reload-removed");
        fs::create_dir_all(&watch_root).unwrap();
        let output_root = temp_file_path("hot-reload-output-removed");
        let source = watch_root.join("mesh.obj");
        fs::write(&source, b"obj-bytes").unwrap();

        let cache = Arc::new(Mutex::new(AssetCache::new()));
        let mut reloader = AssetHotReloader::start(
            &[&watch_root],
            Arc::clone(&cache),
            &output_root,
            true,
            default_asset_reprocessor,
        )
        .expect("watcher should start");

        let initial = reloader.reprocess_path(&source);
        let imported_id = match initial.result {
            AssetHotReloadResult::Reprocessed { id, .. } => id,
            _ => panic!("supported file should be reprocessed"),
        };

        fs::remove_file(&source).unwrap();
        let removed = reloader.reprocess_path(&source);
        let removed_id = match removed.result {
            AssetHotReloadResult::Removed { id } => id,
            _ => panic!("deleted file should remove cache entry"),
        };
        assert_eq!(removed_id, imported_id);
        assert!(!cache
            .lock()
            .expect("cache should lock")
            .contains(&removed_id));
    }

    #[test]
    fn reprocess_path_skips_unsupported_extension() {
        let mut watch_root = temp_file_path("hot-reload-skipped");
        fs::create_dir_all(&watch_root).unwrap();
        let output_root = temp_file_path("hot-reload-output-skipped");
        let source = watch_root.join("notes.txt");
        fs::write(&source, b"notes").unwrap();

        let cache = Arc::new(Mutex::new(AssetCache::new()));
        let mut reloader = AssetHotReloader::start(
            &[&watch_root],
            Arc::clone(&cache),
            &output_root,
            true,
            default_asset_reprocessor,
        )
        .expect("watcher should start");

        let event = reloader.reprocess_path(&source);
        assert!(matches!(
            event.result,
            AssetHotReloadResult::SkippedUnsupported
        ));
    }

    #[test]
    fn generate_terrain_heightmap_is_deterministic_with_same_seed() {
        let config = TerrainGenerationConfig {
            width: 32,
            height: 18,
            seed: 999,
            max_height: 12.0,
            ..TerrainGenerationConfig::default()
        };

        let first = generate_terrain_heightmap(&config).expect("heightmap should generate");
        let second = generate_terrain_heightmap(&config).expect("heightmap should generate");
        assert_eq!(first.width, config.width);
        assert_eq!(first.height, config.height);
        assert_eq!(first.heights, second.heights);
    }

    #[test]
    fn generate_terrain_heightmap_varies_with_seed_change() {
        let a = TerrainGenerationConfig {
            width: 24,
            height: 12,
            seed: 1,
            max_height: 6.0,
            ..TerrainGenerationConfig::default()
        };
        let b = TerrainGenerationConfig {
            seed: 2,
            ..a.clone()
        };

        let first = generate_terrain_heightmap(&a).expect("heightmap should generate");
        let second = generate_terrain_heightmap(&b).expect("heightmap should generate");
        assert_eq!(first.heights.len(), second.heights.len());
        assert!(
            first
                .heights
                .iter()
                .zip(second.heights.iter())
                .any(|(a, b)| (*a - *b).abs() > 0.0001),
            "different seeds should produce different terrain samples"
        );
    }

    #[test]
    fn generate_terrain_heightmap_bounds_and_dimensions() {
        let config = TerrainGenerationConfig {
            width: 16,
            height: 16,
            max_height: 4.5,
            seed: 42,
            ..TerrainGenerationConfig::default()
        };
        let map = generate_terrain_heightmap(&config).expect("heightmap should generate");
        assert_eq!(map.width, config.width);
        assert_eq!(map.height, config.height);
        assert_eq!(map.heights.len(), config.width * config.height);
        assert!(map.heights.iter().all(|height| *height >= 0.0));
        assert!(map
            .heights
            .iter()
            .all(|height| *height <= config.max_height));
    }

    #[test]
    fn generate_terrain_heightmap_rejects_invalid_parameters() {
        let config = TerrainGenerationConfig {
            width: 0,
            ..TerrainGenerationConfig::default()
        };
        assert!(matches!(
            generate_terrain_heightmap(&config),
            Err(TerrainGenerationError::InvalidDimensions { .. })
        ));

        let config = TerrainGenerationConfig {
            width: 4,
            height: 4,
            base_frequency: 0.0,
            ..TerrainGenerationConfig::default()
        };
        assert!(matches!(
            generate_terrain_heightmap(&config),
            Err(TerrainGenerationError::InvalidNoiseConfig(_))
        ));

        let config = TerrainGenerationConfig {
            width: 4,
            height: 4,
            octaves: 0,
            ..TerrainGenerationConfig::default()
        };
        assert!(matches!(
            generate_terrain_heightmap(&config),
            Err(TerrainGenerationError::InvalidNoiseConfig(_))
        ));
    }

    #[test]
    fn generate_terrain_mesh_from_heightmap_matches_sample_count() {
        let config = TerrainGenerationConfig {
            width: 8,
            height: 9,
            max_height: 2.25,
            ..TerrainGenerationConfig::default()
        };
        let map = generate_terrain_heightmap(&config).expect("heightmap should generate");
        let mesh = generate_terrain_mesh(&map).expect("terrain mesh should generate");

        assert_eq!(mesh.vertices.len(), config.width * config.height);
        assert_eq!(
            mesh.indices.len(),
            (config.width - 1) * (config.height - 1) * 6
        );
        assert_eq!(
            mesh.triangle_count(),
            (config.width - 1) * (config.height - 1) * 2
        );
    }

    #[test]
    fn generate_terrain_mesh_rejects_degenerate_dimensions() {
        let map = TerrainHeightmap {
            width: 1,
            height: 5,
            cell_scale_x: 1.0,
            cell_scale_z: 1.0,
            max_height: 1.0,
            octaves: 1,
            seed: 0,
            heights: vec![0.0; 5],
        };
        assert!(matches!(
            generate_terrain_mesh(&map),
            Err(TerrainGenerationError::DegenerateMesh {
                width: 1,
                height: 5
            })
        ));
    }

    #[test]
    fn generate_bsp_dungeon_is_deterministic_for_same_seed() {
        let config = BspDungeonConfig::default();
        let first = generate_bsp_dungeon(&config).expect("dungeon should generate");
        let second = generate_bsp_dungeon(&config).expect("dungeon should generate");

        assert_eq!(first.width, second.width);
        assert_eq!(first.height, second.height);
        assert_eq!(first.tiles, second.tiles);
        assert_eq!(first.rooms, second.rooms);
    }

    #[test]
    fn generate_bsp_dungeon_varies_with_seed_change() {
        let base = BspDungeonConfig::default();
        let first = generate_bsp_dungeon(&base).expect("dungeon should generate");
        let changed = BspDungeonConfig {
            seed: base.seed + 1,
            ..base
        };
        let second = generate_bsp_dungeon(&changed).expect("dungeon should generate");

        assert_eq!(first.width, second.width);
        assert_eq!(first.height, second.height);
        assert_ne!(first.tiles, second.tiles);
        assert!(!first.rooms.is_empty());
        assert!(!second.rooms.is_empty());
    }

    #[test]
    fn generate_bsp_dungeon_rejects_invalid_config() {
        let invalid_dimensions = BspDungeonConfig {
            width: 0,
            ..BspDungeonConfig::default()
        };
        assert!(matches!(
            generate_bsp_dungeon(&invalid_dimensions),
            Err(DungeonGenerationError::InvalidDimensions { .. })
        ));

        let invalid_rooms = BspDungeonConfig {
            min_room_size: 1,
            ..BspDungeonConfig::default()
        };
        assert!(matches!(
            generate_bsp_dungeon(&invalid_rooms),
            Err(DungeonGenerationError::InvalidConfig(_))
        ));
    }

    #[test]
    fn generate_bsp_dungeon_generates_floor_graph() {
        let config = BspDungeonConfig {
            seed: 42,
            ..BspDungeonConfig::default()
        };
        let dungeon = generate_bsp_dungeon(&config).expect("dungeon should generate");
        let floor_count = dungeon.floor_count();
        assert!(floor_count > 0, "generated dungeon should contain floors");

        for room in &dungeon.rooms {
            assert!(room.width >= config.min_room_size);
            assert!(room.height >= config.min_room_size);
            assert!(room.x >= 0);
            assert!(room.y >= 0);
            let end_x = room.x as usize + room.width;
            let end_y = room.y as usize + room.height;
            assert!(end_x <= dungeon.width);
            assert!(end_y <= dungeon.height);
            assert!(dungeon.is_floor(
                (room.x as usize) + room.width / 2,
                (room.y as usize) + room.height / 2
            ));
        }

        let mut visited = vec![false; dungeon.width * dungeon.height];
        let mut queue = VecDeque::new();
        let mut seen_count = 0usize;

        let first_floor = (0..dungeon.tiles.len())
            .find(|index| dungeon.tiles[*index] == DungeonTile::Floor as u8);
        let Some(first_floor) = first_floor else {
            panic!("expected at least one floor tile");
        };

        visited[first_floor] = true;
        queue.push_back(first_floor);

        while let Some(index) = queue.pop_front() {
            seen_count += 1;
            let x = index % dungeon.width;
            let y = index / dungeon.width;
            let neighbors = [
                (x + 1, y),
                (x.wrapping_sub(1), y),
                (x, y + 1),
                (x, y.wrapping_sub(1)),
            ];

            for &(nx, ny) in &neighbors {
                if nx >= dungeon.width || ny >= dungeon.height {
                    continue;
                }
                let neighbor = ny * dungeon.width + nx;
                if visited[neighbor] || dungeon.tiles[neighbor] != DungeonTile::Floor as u8 {
                    continue;
                }
                visited[neighbor] = true;
                queue.push_back(neighbor);
            }
        }

        assert_eq!(
            seen_count, floor_count,
            "all floors should be connected through corridors"
        );
    }

    #[test]
    fn generate_noise_texture_is_deterministic() {
        let config = NoiseTextureConfig {
            seed: 2026,
            width: 24,
            height: 16,
            channels: 4,
            ..NoiseTextureConfig::default()
        };

        let first = generate_noise_texture(&config).expect("noise texture should generate");
        let second = generate_noise_texture(&config).expect("noise texture should generate");

        assert_eq!(
            first.len(),
            (config.width * config.height * u32::from(config.channels)) as usize
        );
        assert_eq!(first, second);
    }

    #[test]
    fn generate_noise_texture_varies_with_seed() {
        let base = NoiseTextureConfig {
            width: 16,
            height: 16,
            channels: 4,
            ..NoiseTextureConfig::default()
        };
        let alternate = NoiseTextureConfig {
            seed: base.seed + 1,
            ..base
        };

        let first = generate_noise_texture(&base).expect("noise texture should generate");
        let second = generate_noise_texture(&alternate).expect("noise texture should generate");
        assert_ne!(first, second);
    }

    #[test]
    fn generate_noise_texture_rejects_invalid_config() {
        let no_width = NoiseTextureConfig {
            width: 0,
            ..NoiseTextureConfig::default()
        };
        assert!(matches!(
            generate_noise_texture(&no_width),
            Err(ProceduralTextureError::InvalidDimensions { .. })
        ));

        let bad_channels = NoiseTextureConfig {
            channels: 0,
            ..NoiseTextureConfig::default()
        };
        assert!(matches!(
            generate_noise_texture(&bad_channels),
            Err(ProceduralTextureError::InvalidChannels(0))
        ));
    }

    #[test]
    fn generate_gradient_texture_follows_horizontal_axis() {
        let config = GradientTextureConfig {
            width: 2,
            height: 1,
            channels: 3,
            start_color: [0, 0, 255, 255],
            end_color: [255, 255, 0, 255],
            direction: GradientDirection::Horizontal,
        };

        let bytes = generate_gradient_texture(&config).expect("gradient texture should generate");

        assert_eq!(bytes.len(), 6);
        assert_eq!(bytes[0], 0);
        assert_eq!(bytes[1], 0);
        assert_eq!(bytes[2], 255);
        assert_eq!(bytes[3], 255);
        assert_eq!(bytes[4], 255);
        assert_eq!(bytes[5], 0);
        assert_eq!(bytes, vec![0, 0, 255, 255, 255, 0]);
    }

    #[test]
    fn generate_gradient_texture_rejects_invalid_channels() {
        let invalid = GradientTextureConfig {
            channels: 5,
            ..GradientTextureConfig::default()
        };
        assert!(matches!(
            generate_gradient_texture(&invalid),
            Err(ProceduralTextureError::InvalidChannels(5))
        ));
    }

    #[test]
    fn noop_text_to_asset_generator_reports_unavailable() {
        let generator = NoopTextToAssetGenerator;
        let mesh_err = text_to_mesh_from_prompt(&generator, "a tiny robot in the shape of a cube");
        let texture_err = text_to_texture_from_prompt(&generator, "crimson nebula", 8, 8);

        assert!(matches!(
            mesh_err,
            Err(TextToAssetError::BackendUnavailable)
        ));
        assert!(matches!(
            texture_err,
            Err(TextToAssetError::BackendUnavailable)
        ));
    }

    fn sample_mesh() -> TriangleMesh {
        TriangleMesh {
            vertices: vec![
                MeshVertex::default(),
                MeshVertex {
                    position: [1.0, 0.0, 0.0],
                    ..MeshVertex::default()
                },
                MeshVertex {
                    position: [0.0, 1.0, 0.0],
                    ..MeshVertex::default()
                },
                MeshVertex {
                    position: [1.0, 1.0, 0.0],
                    ..MeshVertex::default()
                },
                MeshVertex {
                    position: [0.0, 0.0, 1.0],
                    ..MeshVertex::default()
                },
                MeshVertex {
                    position: [1.0, 0.0, 1.0],
                    ..MeshVertex::default()
                },
            ],
            indices: vec![
                0, 1, 2, // tri 0
                1, 2, 3, // tri 1
                2, 3, 4, // tri 2
                2, 4, 5, // tri 3
            ],
        }
    }

    #[test]
    fn generate_lod_chain_reduces_triangle_counts_deterministically() {
        let mesh = sample_mesh();
        let chain = generate_lod_chain("test_asset", &mesh, 3)
            .expect("mesh should be processed into 3 LODs");

        assert_eq!(chain.levels.len(), 3);
        assert_eq!(chain.levels[0].level, 0);
        assert_eq!(chain.levels[0].stride, 1);
        assert_eq!(chain.levels[0].mesh.triangle_count(), 4);
        assert_eq!(chain.levels[1].level, 1);
        assert_eq!(chain.levels[1].stride, 2);
        assert_eq!(chain.levels[1].mesh.triangle_count(), 2);
        assert_eq!(chain.levels[2].level, 2);
        assert_eq!(chain.levels[2].stride, 4);
        assert_eq!(chain.levels[2].mesh.triangle_count(), 1);

        assert!(chain.levels[0].mesh.vertices.len() >= chain.levels[1].mesh.vertices.len());
        assert!(chain.levels[1].mesh.vertices.len() >= chain.levels[2].mesh.vertices.len());
        assert_eq!(chain.source.to_string(), "test_asset");
    }

    #[test]
    fn generate_lod_chain_rejects_invalid_triangle_layout() {
        let invalid_indices = TriangleMesh {
            vertices: vec![
                MeshVertex::default(),
                MeshVertex {
                    position: [1.0, 1.0, 1.0],
                    ..MeshVertex::default()
                },
            ],
            indices: vec![0, 1, 0, 1],
        };
        let invalid_indices_count = generate_lod_chain("bad", &invalid_indices, 1);
        assert!(matches!(
            invalid_indices_count,
            Err(MeshProcessError::NonTriangularMesh { index_count: 4 })
        ));
    }

    #[test]
    fn generate_lod_chain_rejects_out_of_range_vertex_indices() {
        let invalid_indices = TriangleMesh {
            vertices: vec![MeshVertex::default()],
            indices: vec![0, 1, 2],
        };
        let invalid_index = generate_lod_chain("bad", &invalid_indices, 1);
        assert!(matches!(
            invalid_index,
            Err(MeshProcessError::InvalidIndex {
                level: 0,
                source_index: 1,
                vertex_count: 1
            })
        ));
    }

    #[test]
    fn compress_texture_profile_none_returns_identity() {
        let source = vec![0x11, 0x22, 0x11, 0x33];
        let artifact = compress_texture(
            "texture_none",
            AssetFormat::Png,
            2,
            2,
            &source,
            TextureCompressionProfile::None,
        )
        .expect("compression should succeed");

        assert_eq!(artifact.profile, TextureCompressionProfile::None);
        assert_eq!(artifact.bytes, source);
        assert_eq!(artifact.width, 2);
        assert_eq!(artifact.height, 2);
        assert_eq!(artifact.original_size, 4);
        assert_eq!(artifact.compressed_size, 4);
    }

    #[test]
    fn compress_texture_reduces_repetitive_input_for_fast_profile() {
        let source = vec![7u8, 7, 7, 7, 7, 7, 7, 7, 9, 7];
        let artifact = compress_texture(
            "texture_fast",
            AssetFormat::Png,
            2,
            5,
            &source,
            TextureCompressionProfile::Fast,
        )
        .expect("compression should succeed");

        assert_eq!(artifact.profile, TextureCompressionProfile::Fast);
        assert_eq!(artifact.width, 2);
        assert_eq!(artifact.height, 5);
        assert!(artifact.compressed_size < artifact.original_size);
    }

    #[test]
    fn pack_texture_atlas_uses_row_layout_with_padding() {
        let atlas = pack_texture_atlas(
            "atlas",
            &[
                AtlasTextureSource {
                    id: AssetId::from("tex_a"),
                    width: 4,
                    height: 2,
                },
                AtlasTextureSource {
                    id: AssetId::from("tex_b"),
                    width: 4,
                    height: 3,
                },
                AtlasTextureSource {
                    id: AssetId::from("tex_c"),
                    width: 4,
                    height: 1,
                },
            ],
            &AtlasPackingOptions {
                max_width: 8,
                padding: 1,
            },
        )
        .expect("atlas should pack");

        assert_eq!(atlas.id, AssetId::from("atlas"));
        assert_eq!(atlas.placements.len(), 3);
        assert_eq!(
            atlas.placements[0],
            TexturePlacement {
                id: AssetId::from("tex_a"),
                x: 0,
                y: 0,
                width: 4,
                height: 2,
            }
        );
        assert_eq!(atlas.placements[1].x, 0);
        assert_eq!(atlas.placements[2].y, 7);
        assert_eq!(atlas.height, 8);
    }

    #[test]
    fn pack_texture_atlas_rejects_invalid_size() {
        let result = pack_texture_atlas(
            "atlas_bad",
            &[AtlasTextureSource {
                id: AssetId::from("bad"),
                width: 0,
                height: 1,
            }],
            &AtlasPackingOptions::default(),
        );
        assert!(matches!(
            result,
            Err(TextureProcessError::InvalidTextureDimensions { .. })
        ));
    }
}
