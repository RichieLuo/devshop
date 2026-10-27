use super::*;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const JSON: &str = r#"{"value":1}"#;
const JWT: &str = include_str!("../../examples/sample.jwt");

struct Directory(PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    app: App,
    ctx: egui::Context,
    store: HistoryStore,
    entries: Vec<HistoryEntry>,
    _directory: Directory,
}

impl Fixture {
    fn new() -> Self {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("devshop-ui-{}-{id}", std::process::id()));
        let directory = Directory(path.clone());
        let store = HistoryStore::open(path).unwrap();
        for (kind, text, origin) in [
            (ContentKind::Json, JSON, "sample.json"),
            (ContentKind::Jwt, JWT.trim(), "sample.jwt"),
        ] {
            store.append(kind, text, origin, "载入", None).unwrap();
        }
        let entries = store.list().unwrap();
        let ctx = egui::Context::default();
        let app = App::with_history(&ctx, Settings::default(), Ok(store.clone()));
        Self {
            app,
            ctx,
            store,
            entries,
            _directory: directory,
        }
    }

    fn entry(&self, kind: ContentKind) -> HistoryEntry {
        self.entries
            .iter()
            .find(|entry| entry.kind == kind)
            .unwrap()
            .clone()
    }

    fn wait_until(&mut self, ready: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.app.poll(&self.ctx);
            if ready(&self.app) {
                return;
            }
            assert!(Instant::now() < deadline, "background operation timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn restore(&mut self, kind: ContentKind) {
        let entry = self.entry(kind);
        self.app.open_history(entry.clone(), &self.ctx);
        self.wait_until(|app| !app.file_loading);
        if self.app.pending.is_some() {
            self.app.discard_and_continue(&self.ctx);
        }
        self.wait_until(|app| !app.json.busy());
        assert_eq!(self.app.active, kind);
        assert_eq!(
            self.app.meta(kind).parent_id.as_deref(),
            Some(entry.id.as_str())
        );
        assert_eq!(
            self.app.source(kind).as_ref(),
            if kind == ContentKind::Json {
                JSON
            } else {
                JWT.trim()
            }
        );
    }
}

#[test]
fn repeatedly_opening_json_and_jwt_history_preserves_the_index() {
    let mut fixture = Fixture::new();
    let before = serde_json::to_string(&fixture.entries).unwrap();
    for _ in 0..3 {
        fixture.restore(ContentKind::Json);
        fixture.restore(ContentKind::Jwt);
    }
    // Dropping the worker flushes all queued writes before checking the index.
    drop(fixture.app);
    assert_eq!(
        serde_json::to_string(&fixture.store.list().unwrap()).unwrap(),
        before
    );
}

#[test]
fn history_highlight_follows_only_the_active_workspace() {
    let mut fixture = Fixture::new();
    assert!(
        !fixture
            .entries
            .iter()
            .any(|entry| fixture.app.is_current_history(entry))
    );
    fixture.restore(ContentKind::Json);
    fixture.restore(ContentKind::Jwt);
    for kind in [ContentKind::Json, ContentKind::Jwt, ContentKind::Json] {
        fixture.app.active = kind;
        let selected: Vec<_> = fixture
            .entries
            .iter()
            .filter(|entry| fixture.app.is_current_history(entry))
            .collect();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].kind, kind);
        assert_eq!(selected[0].id, fixture.entry(kind).id);
    }
    fixture
        .app
        .perform_action(PendingAction::New(ContentKind::Json), &fixture.ctx);
    assert!(
        !fixture
            .entries
            .iter()
            .any(|entry| fixture.app.is_current_history(entry))
    );
}

#[test]
fn editing_restored_json_records_a_child_snapshot() {
    let mut fixture = Fixture::new();
    let original = fixture.entry(ContentKind::Json);
    fixture.restore(ContentKind::Json);
    fixture
        .app
        .json
        .set_source(r#"{"value":2}"#.into(), true, Some("编辑"));
    fixture.wait_until(|app| !app.json.busy());
    drop(fixture.app);
    let entries = fixture.store.list().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].parent_id.as_ref(), Some(&original.id));
    assert_eq!(entries[0].operation, "编辑");
    assert_eq!(fixture.store.load(&entries[0]).unwrap(), r#"{"value":2}"#);
}

#[test]
fn manual_recording_after_restoring_jwt_preserves_its_parent() {
    let mut fixture = Fixture::new();
    let original = fixture.entry(ContentKind::Jwt);
    fixture.restore(ContentKind::Jwt);
    fixture.app.jwt_event(
        jwt_view::JwtEvent::RecordRequested {
            token: JWT.trim().into(),
        },
        &fixture.ctx,
    );
    drop(fixture.app);
    let entries = fixture.store.list().unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].kind, ContentKind::Jwt);
    assert_eq!(entries[0].operation, "手动保存历史");
    assert_eq!(entries[0].parent_id.as_ref(), Some(&original.id));
}

#[test]
fn regular_loads_still_record_json_and_jwt_history() {
    let mut fixture = Fixture::new();
    for (kind, text) in [(ContentKind::Json, JSON), (ContentKind::Jwt, JWT.trim())] {
        fixture.restore(kind);
        fixture.app.perform_action(
            PendingAction::Install {
                kind,
                text: text.into(),
                meta: DocumentMeta {
                    origin: "新文件".into(),
                    ..Default::default()
                },
                dirty: false,
            },
            &fixture.ctx,
        );
        fixture.wait_until(|app| !app.json.busy());
    }
    drop(fixture.app);
    let entries = fixture.store.list().unwrap();
    assert_eq!(entries.len(), 4);
    for (kind, operation) in [
        (ContentKind::Json, "载入 JSON"),
        (ContentKind::Jwt, "载入 JWT"),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry.kind == kind && entry.origin == "新文件")
            .unwrap();
        assert_eq!(entry.operation, operation);
        assert!(entry.parent_id.is_none());
    }
}
