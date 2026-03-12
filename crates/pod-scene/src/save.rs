use crate::json::{stabilize_json_value, to_stable_json_string};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Serializable game snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveData {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub timestamp: String,
    pub version: u32,
    pub playtime_seconds: u64,
    pub metadata: HashMap<String, String>,
    pub world_state: serde_json::Value,
    pub scene_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SaveDataBinary {
    id: Uuid,
    name: String,
    description: String,
    timestamp: String,
    version: u32,
    playtime_seconds: u64,
    metadata: HashMap<String, String>,
    world_state_json: String,
    scene_name: String,
}

impl SaveData {
    pub fn new(name: impl Into<String>, scene_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: String::new(),
            timestamp: Self::get_timestamp(),
            version: 1,
            playtime_seconds: 0,
            metadata: HashMap::new(),
            world_state: serde_json::json!({}),
            scene_name: scene_name.into(),
        }
    }

    /// Set the description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set playtime in seconds
    pub fn with_playtime(mut self, seconds: u64) -> Self {
        self.playtime_seconds = seconds;
        self
    }

    /// Add a metadata key-value pair
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set the world state as JSON
    pub fn with_world_state(mut self, state: serde_json::Value) -> Self {
        self.world_state = state;
        self
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        to_stable_json_string(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to binary
    pub fn to_binary(&self) -> Result<Vec<u8>, bincode::Error> {
        let mut world_state_json = self.world_state.clone();
        stabilize_json_value(&mut world_state_json);
        let binary = SaveDataBinary {
            id: self.id,
            name: self.name.clone(),
            description: self.description.clone(),
            timestamp: self.timestamp.clone(),
            version: self.version,
            playtime_seconds: self.playtime_seconds,
            metadata: self.metadata.clone(),
            world_state_json: world_state_json.to_string(),
            scene_name: self.scene_name.clone(),
        };
        bincode::serialize(&binary)
    }

    /// Deserialize from binary
    pub fn from_binary(data: &[u8]) -> Result<Self, bincode::Error> {
        let binary: SaveDataBinary = bincode::deserialize(data)?;
        let world_state = serde_json::from_str(&binary.world_state_json)
            .map_err(|err| Box::new(bincode::ErrorKind::Custom(err.to_string())))?;
        Ok(Self {
            id: binary.id,
            name: binary.name,
            description: binary.description,
            timestamp: binary.timestamp,
            version: binary.version,
            playtime_seconds: binary.playtime_seconds,
            metadata: binary.metadata,
            world_state,
            scene_name: binary.scene_name,
        })
    }

    fn get_timestamp() -> String {
        use std::time::SystemTime;
        let time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{:020}.{:09}", time.as_secs(), time.subsec_nanos())
    }
}

/// Metadata about a save file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveMetadata {
    pub filename: String,
    pub name: String,
    pub description: String,
    pub timestamp: String,
    pub playtime_seconds: u64,
    pub version: u32,
}

impl SaveMetadata {
    pub fn from_save_data(save_data: &SaveData, filename: impl Into<String>) -> Self {
        Self {
            filename: filename.into(),
            name: save_data.name.clone(),
            description: save_data.description.clone(),
            timestamp: save_data.timestamp.clone(),
            playtime_seconds: save_data.playtime_seconds,
            version: save_data.version,
        }
    }
}

/// Manages saving, loading, and listing save files
pub struct SaveManager {
    save_directory: PathBuf,
    save_extension: String,
    use_binary: bool,
}

impl SaveManager {
    /// Create a new save manager with a save directory
    pub fn new(save_directory: impl AsRef<Path>, use_binary: bool) -> Self {
        let dir = save_directory.as_ref().to_path_buf();
        // Create the directory if it doesn't exist
        let _ = std::fs::create_dir_all(&dir);

        Self {
            save_directory: dir,
            save_extension: if use_binary { "sav" } else { "json" }.to_string(),
            use_binary,
        }
    }

    /// Save game state to file
    pub fn save(&self, save_data: &SaveData) -> Result<PathBuf, String> {
        let filename = format!(
            "{}_{}.{}",
            sanitize_save_stem(&save_data.name),
            save_data.id,
            self.save_extension
        );
        let path = self.save_directory.join(&filename);

        let content = if self.use_binary {
            save_data
                .to_binary()
                .map_err(|e| format!("Failed to serialize save data: {}", e))?
        } else {
            save_data
                .to_json()
                .map_err(|e| format!("Failed to serialize save data: {}", e))?
                .into_bytes()
        };

        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write save file: {}", e))?;

        log::info!("Saved game to {:?}", path);
        Ok(path)
    }

    /// Load a save file by filename
    pub fn load(&self, filename: &str) -> Result<SaveData, String> {
        let filename = self.validate_filename(filename)?;
        let path = self.save_directory.join(filename);

        let content =
            std::fs::read(&path).map_err(|e| format!("Failed to read save file: {}", e))?;

        let save_data = if self.use_binary {
            SaveData::from_binary(&content)
                .map_err(|e| format!("Failed to deserialize save data: {}", e))?
        } else {
            let json_str = String::from_utf8(content)
                .map_err(|e| format!("Invalid UTF-8 in save file: {}", e))?;
            SaveData::from_json(&json_str)
                .map_err(|e| format!("Failed to deserialize save data: {}", e))?
        };

        log::info!("Loaded game from {:?}", path);
        Ok(save_data)
    }

    /// List all available saves with metadata
    pub fn list_saves(&self) -> Result<Vec<SaveMetadata>, String> {
        let mut saves = Vec::new();

        let entries = std::fs::read_dir(&self.save_directory)
            .map_err(|e| format!("Failed to read save directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == self.save_extension.as_str() {
                        if let Some(filename) = path.file_name() {
                            if let Some(filename_str) = filename.to_str() {
                                match self.load(filename_str) {
                                    Ok(save_data) => {
                                        saves.push(SaveMetadata::from_save_data(
                                            &save_data,
                                            filename_str,
                                        ));
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "Failed to load save file {}: {}",
                                            filename_str,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by timestamp (newest first)
        saves.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(saves)
    }

    /// Delete a save file
    pub fn delete(&self, filename: &str) -> Result<(), String> {
        let filename = self.validate_filename(filename)?;
        let path = self.save_directory.join(filename);
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete save file: {}", e))?;
        log::info!("Deleted save file: {:?}", path);
        Ok(())
    }

    /// Check if a save file exists
    pub fn exists(&self, filename: &str) -> bool {
        self.validate_filename(filename)
            .map(|filename| self.save_directory.join(filename).exists())
            .unwrap_or(false)
    }

    /// Get the save directory
    pub fn get_save_directory(&self) -> &Path {
        &self.save_directory
    }

    /// Set the save directory (creates it if it doesn't exist)
    pub fn set_save_directory(&mut self, dir: impl AsRef<Path>) -> Result<(), String> {
        let path = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create save directory: {}", e))?;
        self.save_directory = path;
        Ok(())
    }

    /// Create a snapshot of the current world state
    pub fn create_snapshot(&self, save_name: impl Into<String>) -> SaveData {
        SaveData::new(save_name, "Unknown")
    }

    /// Get save statistics
    pub fn get_statistics(&self) -> Result<SaveStatistics, String> {
        let saves = self.list_saves()?;
        let total_playtime: u64 = saves.iter().map(|s| s.playtime_seconds).sum();

        Ok(SaveStatistics {
            total_saves: saves.len(),
            total_playtime_seconds: total_playtime,
            oldest_save: saves.last().map(|s| s.timestamp.clone()),
            newest_save: saves.first().map(|s| s.timestamp.clone()),
        })
    }

    fn validate_filename<'a>(&self, filename: &'a str) -> Result<&'a str, String> {
        let path = Path::new(filename);
        let mut components = path.components();
        match (components.next(), components.next()) {
            (Some(std::path::Component::Normal(_)), None) => {}
            _ => {
                return Err("Save filename must not contain path separators or traversal segments"
                    .to_string())
            }
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some(self.save_extension.as_str()) {
            return Err(format!(
                "Save filename must use .{} extension",
                self.save_extension
            ));
        }

        Ok(filename)
    }
}

fn sanitize_save_stem(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "save".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Statistics about saved games
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveStatistics {
    pub total_saves: usize,
    pub total_playtime_seconds: u64,
    pub oldest_save: Option<String>,
    pub newest_save: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_data_creation() {
        let save = SaveData::new("TestSave", "TestScene");
        assert_eq!(save.name, "TestSave");
        assert_eq!(save.scene_name, "TestScene");
    }

    #[test]
    fn test_save_data_builder() {
        let save = SaveData::new("Save", "Scene")
            .with_description("Test description")
            .with_playtime(3600)
            .with_metadata("level", "5");

        assert_eq!(save.description, "Test description");
        assert_eq!(save.playtime_seconds, 3600);
        assert_eq!(save.metadata.get("level"), Some(&"5".to_string()));
    }

    #[test]
    fn test_save_data_json_serialization() {
        let save = SaveData::new("Save", "Scene");
        let json = save.to_json().unwrap();
        let loaded = SaveData::from_json(&json).unwrap();

        assert_eq!(loaded.name, save.name);
        assert_eq!(loaded.scene_name, save.scene_name);
    }

    #[test]
    fn test_save_data_binary_serialization() {
        let save = SaveData::new("Save", "Scene");
        let binary = save.to_binary().unwrap();
        let loaded = SaveData::from_binary(&binary).unwrap();

        assert_eq!(loaded.name, save.name);
        assert_eq!(loaded.scene_name, save.scene_name);
    }

    #[test]
    fn test_save_manager_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SaveManager::new(temp_dir.path(), false);

        let save = SaveData::new("TestSave", "TestScene");
        let path = manager.save(&save).unwrap();
        assert!(path.exists());

        let filename = path.file_name().unwrap().to_str().unwrap();
        let loaded = manager.load(filename).unwrap();
        assert_eq!(loaded.name, "TestSave");
        assert_eq!(loaded.scene_name, "TestScene");
    }

    #[test]
    fn test_save_manager_list() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SaveManager::new(temp_dir.path(), false);

        let save1 = SaveData::new("Save1", "Scene1");
        let save2 = SaveData::new("Save2", "Scene2");

        manager.save(&save1).unwrap();
        manager.save(&save2).unwrap();

        let saves = manager.list_saves().unwrap();
        assert_eq!(saves.len(), 2);
    }

    #[test]
    fn test_save_manager_delete() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SaveManager::new(temp_dir.path(), false);

        let save = SaveData::new("ToDelete", "Scene");
        let path = manager.save(&save).unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();

        assert!(manager.exists(filename));
        manager.delete(filename).unwrap();
        assert!(!manager.exists(filename));
    }

    #[test]
    fn test_save_metadata() {
        let save = SaveData::new("Save", "Scene").with_playtime(1000);
        let metadata = SaveMetadata::from_save_data(&save, "save.json");

        assert_eq!(metadata.name, "Save");
        assert_eq!(metadata.playtime_seconds, 1000);
        assert_eq!(metadata.filename, "save.json");
    }

    #[test]
    fn test_save_manager_sanitizes_save_names() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SaveManager::new(temp_dir.path(), false);

        let save = SaveData::new("../unsafe name", "Scene");
        let path = manager.save(&save).unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();

        assert_eq!(path.parent(), Some(temp_dir.path()));
        assert!(!filename.contains(".."));
        assert!(!filename.contains('/'));
        assert!(filename.starts_with("unsafe_name_"));
    }

    #[test]
    fn test_save_manager_rejects_path_traversal_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SaveManager::new(temp_dir.path(), false);

        assert!(manager.load("../escape.json").is_err());
        assert!(manager.delete("../escape.json").is_err());
        assert!(!manager.exists("../escape.json"));
    }

    #[test]
    fn test_save_manager_lists_newest_timestamp_first() {
        let temp_dir = TempDir::new().unwrap();
        let manager = SaveManager::new(temp_dir.path(), false);

        let mut older = SaveData::new("Older", "Scene");
        older.timestamp = "00000000000000000009.000000000".to_string();
        let mut newer = SaveData::new("Newer", "Scene");
        newer.timestamp = "00000000000000000010.000000000".to_string();

        manager.save(&older).unwrap();
        manager.save(&newer).unwrap();

        let saves = manager.list_saves().unwrap();
        assert_eq!(saves[0].name, "Newer");
        assert_eq!(saves[1].name, "Older");
    }
}
