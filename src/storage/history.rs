use super::files::{MAX_DOCUMENT_BYTES, atomic_write};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_ENTRIES: usize = 100;
const MAX_BLOB_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentKind {
    Json,
    Jwt,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub kind: ContentKind,
    pub title: String,
    pub created_ms: u64,
    pub origin: String,
    pub operation: String,
    pub bytes: u64,
    pub parent_id: Option<String>,
    pub pinned: bool,
    blob: String,
}

#[derive(Clone)]
pub struct HistoryStore {
    directory: PathBuf,
}

pub fn default_data_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("DEVSHOP_DATA_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("DevShop");
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path).join("Library/Application Support/DevShop");
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("devshop");
    }
    if let Some(path) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(path).join(".local/share/devshop");
    }
    PathBuf::from(".devshop")
}

impl HistoryStore {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(directory.join("blobs")).map_err(|e| e.to_string())?;
        Ok(Self { directory })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn list(&self) -> Result<Vec<HistoryEntry>, String> {
        let path = self.directory.join("history.json");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
        if metadata.len() > 4 * 1024 * 1024 {
            return Err("历史索引超过安全上限".into());
        }
        let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let entries: Vec<HistoryEntry> = serde_json::from_str(&text)
            .map_err(|e| format!("历史索引无法读取，原文件已保留：{e}"))?;
        if entries
            .iter()
            .any(|e| !valid_blob(&e.blob) || e.bytes > MAX_DOCUMENT_BYTES)
        {
            return Err("历史索引包含无效内容引用，原文件已保留".into());
        }
        Ok(entries)
    }
    pub fn load(&self, entry: &HistoryEntry) -> Result<String, String> {
        if !valid_blob(&entry.blob) {
            return Err("无效的历史引用".into());
        }
        let path = self.directory.join("blobs").join(&entry.blob);
        let (text, _) = super::files::read_document(&path)?;
        if digest(&text) != entry.blob {
            return Err("历史内容校验失败，已停止加载".into());
        }
        Ok(text)
    }
    pub fn append(
        &self,
        kind: ContentKind,
        text: &str,
        origin: &str,
        operation: &str,
        parent_id: Option<String>,
    ) -> Result<HistoryEntry, String> {
        if text.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err("内容超过历史保存上限".into());
        }
        let mut entries = self.list()?;
        let blob = digest(text);
        let path = self.directory.join("blobs").join(&blob);
        if !path.exists() {
            atomic_write(&path, text.as_bytes())?;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let title = origin
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(origin)
            .to_string();
        let entry = HistoryEntry {
            id: format!("{}-{}", now.as_nanos(), std::process::id()),
            kind,
            title,
            created_ms: now.as_millis() as u64,
            origin: origin.into(),
            operation: operation.into(),
            bytes: text.len() as u64,
            parent_id,
            pinned: false,
            blob,
        };
        entries.insert(0, entry.clone());
        prune(&mut entries);
        self.write_index(&entries)?;
        self.collect_blobs(&entries);
        Ok(entry)
    }
    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<(), String> {
        let mut entries = self.list()?;
        let entry = entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or("历史记录不存在")?;
        entry.pinned = pinned;
        self.write_index(&entries)
    }
    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut entries = self.list()?;
        entries.retain(|e| e.id != id);
        self.write_index(&entries)?;
        self.collect_blobs(&entries);
        Ok(())
    }
    pub fn clear_unpinned(&self) -> Result<(), String> {
        let mut entries = self.list()?;
        entries.retain(|e| e.pinned);
        self.write_index(&entries)?;
        self.collect_blobs(&entries);
        Ok(())
    }
    fn write_index(&self, entries: &[HistoryEntry]) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(entries).map_err(|e| e.to_string())?;
        atomic_write(&self.directory.join("history.json"), &bytes)
    }
    fn collect_blobs(&self, entries: &[HistoryEntry]) {
        let live: HashSet<_> = entries.iter().map(|e| e.blob.as_str()).collect();
        if let Ok(files) = fs::read_dir(self.directory.join("blobs")) {
            for file in files.flatten() {
                if let Some(name) = file.file_name().to_str()
                    && valid_blob(name)
                    && !live.contains(name)
                {
                    let _ = fs::remove_file(file.path());
                }
            }
        }
    }
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn valid_blob(blob: &str) -> bool {
    blob.len() == 64 && blob.bytes().all(|b| b.is_ascii_hexdigit())
}
fn prune(entries: &mut Vec<HistoryEntry>) {
    let mut count = 0;
    let mut bytes = 0;
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        if entry.pinned {
            return true;
        }
        count += 1;
        if seen.insert(entry.blob.clone()) {
            bytes += entry.bytes;
        }
        count <= MAX_ENTRIES && (bytes <= MAX_BLOB_BYTES || count == 1)
    });
}
