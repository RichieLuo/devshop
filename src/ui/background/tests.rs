use super::*;

#[test]
fn history_worker_flushes_and_links_rapid_edits_before_shutdown() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("devshop-worker-{}-{id}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let store = HistoryStore::open(path.clone()).unwrap();
    let worker = HistoryWorker::start(Ok(store.clone()), egui::Context::default());
    for text in ["{\"x\":1}", "{\"x\":2}"] {
        worker
            .sender
            .send(HistoryCommand::Record {
                kind: ContentKind::Json,
                text: Arc::from(text),
                meta: DocumentMeta {
                    session: 9,
                    origin: "剪贴板".into(),
                    ..Default::default()
                },
                operation: "修改节点".into(),
            })
            .unwrap();
    }
    drop(worker);
    let entries = store.list().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].parent_id.as_ref(), Some(&entries[1].id));
    assert_eq!(store.load(&entries[0]).unwrap(), "{\"x\":2}");
    assert_eq!(store.load(&entries[1]).unwrap(), "{\"x\":1}");
    std::fs::remove_dir_all(path).unwrap();
}
