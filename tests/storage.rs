use devshop::storage::{ContentKind, FileStamp, HistoryStore, read_document, save_document};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("devshop-test-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn save_round_trip_preserves_exact_json_and_rejects_external_changes() {
    let directory = Directory::new();
    let path = directory.0.join("开发数据.json");
    let original = "{\"n\":9007199254740993,\"x\":1,\"x\":2,\"中文\":\"✓\"}";
    let stamp = save_document(&path, original, None).unwrap();
    assert_eq!(read_document(&path).unwrap().0, original);
    assert!(save_document(&path, "{}", None).is_err());
    fs::write(&path, "{\"external\":true}").unwrap();
    assert!(save_document(&path, "{}", Some(&stamp)).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{\"external\":true}");
    let latest = FileStamp::read(&path).unwrap();
    save_document(&path, "[]", Some(&latest)).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "[]");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn utf8_bom_is_accepted_and_invalid_encoding_rejected() {
    let directory = Directory::new();
    let path = directory.0.join("bom.json");
    fs::write(&path, b"\xef\xbb\xbf{\"name\":1}").unwrap();
    assert_eq!(read_document(&path).unwrap().0, "{\"name\":1}");
    fs::write(&path, [0xff, 0xfe, 0]).unwrap();
    assert!(read_document(&path).is_err());
}

#[test]
fn history_survives_restart_and_preserves_provenance() {
    let directory = Directory::new();
    let store = HistoryStore::open(directory.0.clone()).unwrap();
    let first = store
        .append(ContentKind::Json, "{\"x\":1}", "剪贴板", "载入", None)
        .unwrap();
    let second = store
        .append(
            ContentKind::Json,
            "{\"x\":2}",
            "剪贴板",
            "修改节点",
            Some(first.id.clone()),
        )
        .unwrap();
    let reopened = HistoryStore::open(directory.0.clone()).unwrap();
    let entries = reopened.list().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].parent_id.as_ref(), Some(&first.id));
    assert_eq!(reopened.load(&second).unwrap(), "{\"x\":2}");
    assert_eq!(reopened.load(&first).unwrap(), "{\"x\":1}");
}

#[test]
fn history_retention_keeps_pinned_and_shared_blobs() {
    let directory = Directory::new();
    let store = HistoryStore::open(directory.0.clone()).unwrap();
    let pinned = store
        .append(ContentKind::Json, "[1]", "文件", "打开", None)
        .unwrap();
    store.set_pinned(&pinned.id, true).unwrap();
    for _ in 0..105 {
        store
            .append(ContentKind::Json, "[2]", "剪贴板", "打开", None)
            .unwrap();
    }
    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 101);
    assert!(entries.iter().any(|e| e.id == pinned.id && e.pinned));
    assert_eq!(fs::read_dir(directory.0.join("blobs")).unwrap().count(), 2);
    let shared = entries.iter().find(|e| !e.pinned).unwrap().clone();
    store.remove(&shared.id).unwrap();
    assert_eq!(
        store
            .load(store.list().unwrap().iter().find(|e| !e.pinned).unwrap())
            .unwrap(),
        "[2]"
    );
    store.clear_unpinned().unwrap();
    assert_eq!(store.list().unwrap().len(), 1);
    assert_eq!(store.load(&pinned).unwrap(), "[1]");
    assert_eq!(fs::read_dir(directory.0.join("blobs")).unwrap().count(), 1);
}

#[test]
fn corrupt_index_and_blob_are_reported_without_overwriting_originals() {
    let directory = Directory::new();
    let store = HistoryStore::open(directory.0.clone()).unwrap();
    let record = store
        .append(
            ContentKind::Jwt,
            "synthetic.token.fixture",
            "手动输入",
            "记录",
            None,
        )
        .unwrap();
    let blob = fs::read_dir(directory.0.join("blobs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(&blob, "tampered").unwrap();
    assert!(store.load(&record).is_err());
    fs::write(directory.0.join("history.json"), "broken index").unwrap();
    assert!(store.list().is_err());
    assert!(
        store
            .append(ContentKind::Json, "{}", "x", "y", None)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("history.json")).unwrap(),
        "broken index"
    );
}

#[test]
fn malicious_history_paths_never_escape_blob_directory() {
    let directory = Directory::new();
    let store = HistoryStore::open(directory.0.clone()).unwrap();
    store
        .append(ContentKind::Json, "{}", "x", "y", None)
        .unwrap();
    let index = directory.0.join("history.json");
    let mut data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&index).unwrap()).unwrap();
    data[0]["blob"] = "../../outside.txt".into();
    fs::write(index, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(store.list().is_err());
}
