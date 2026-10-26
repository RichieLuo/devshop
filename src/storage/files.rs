use std::{fs, io::Write, path::Path, time::SystemTime};

pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl FileStamp {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("请选择普通文件".into());
        }
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

pub fn read_document(path: &Path) -> Result<(String, FileStamp), String> {
    let before = FileStamp::read(path)?;
    if before.len > MAX_DOCUMENT_BYTES {
        return Err("文件超过当前 256 MiB 上限，请拆分后打开".into());
    }
    use std::io::Read;
    let mut bytes = Vec::with_capacity(before.len as usize);
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_DOCUMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err("读取时文件超过 256 MiB 上限".into());
    }
    let after = FileStamp::read(path)?;
    if before != after {
        return Err("文件在读取时发生了变化，请重试".into());
    }
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        bytes.drain(..3);
    }
    let text =
        String::from_utf8(bytes).map_err(|_| "文件不是 UTF-8 编码，请转换后打开".to_string())?;
    Ok((text, after))
}

/// The expected stamp prevents silently replacing changes made by another editor.
pub fn save_document(
    path: &Path,
    text: &str,
    expected: Option<&FileStamp>,
) -> Result<FileStamp, String> {
    validate_stamp(path, expected)?;
    atomic_write_checked(path, text.as_bytes(), || validate_stamp(path, expected))?;
    FileStamp::read(path)
}

fn validate_stamp(path: &Path, expected: Option<&FileStamp>) -> Result<(), String> {
    if path.exists() {
        if expected != Some(&FileStamp::read(path)?) {
            return Err("文件已在外部变化，为避免覆盖，请重新打开或另存为新文件".into());
        }
    } else if expected.is_some() {
        return Err("原文件已被移动或删除，请另存为".into());
    }
    Ok(())
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write_checked(path, bytes, || Ok(()))
}

fn atomic_write_checked(
    path: &Path,
    bytes: &[u8],
    before_replace: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let parent = path.parent().ok_or("文件没有父目录")?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(".devshop-{}-{stamp}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        before_replace()?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
