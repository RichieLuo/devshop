use devshop::storage::{ContentKind, FileStamp, HistoryEntry, HistoryStore};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Default)]
pub(super) struct DocumentMeta {
    pub session: u64,
    pub kind_hint: Option<ContentKind>,
    pub origin: String,
    pub path: Option<PathBuf>,
    pub stamp: Option<FileStamp>,
    pub parent_id: Option<String>,
}

pub(super) enum HistoryCommand {
    Record {
        kind: ContentKind,
        text: Arc<str>,
        meta: DocumentMeta,
        operation: String,
    },
    Pin(String, bool),
    Remove(String),
    Clear,
    Stop,
}

pub(super) struct HistoryReply {
    pub entries: Result<Vec<HistoryEntry>, String>,
    pub recorded: Option<(ContentKind, u64, HistoryEntry)>,
}

pub(super) struct HistoryWorker {
    pub sender: mpsc::Sender<HistoryCommand>,
    pub receiver: mpsc::Receiver<HistoryReply>,
    handle: Option<thread::JoinHandle<()>>,
}

impl HistoryWorker {
    pub fn start(store: Result<HistoryStore, String>, ctx: egui::Context) -> Self {
        let (sender, commands) = mpsc::channel();
        let (replies, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let store = match store {
                Ok(store) => store,
                Err(error) => {
                    let _ = replies.send(HistoryReply {
                        entries: Err(error),
                        recorded: None,
                    });
                    ctx.request_repaint();
                    return;
                }
            };
            let _ = replies.send(HistoryReply {
                entries: store.list(),
                recorded: None,
            });
            ctx.request_repaint();
            let mut parents = std::collections::HashMap::new();
            while let Ok(command) = commands.recv() {
                let mut recorded = None;
                let result = match command {
                    HistoryCommand::Record {
                        kind,
                        text,
                        meta,
                        operation,
                    } => {
                        let key = (kind as u8, meta.session);
                        let parent = parents.get(&key).cloned().or(meta.parent_id);
                        store
                            .append(kind, &text, &meta.origin, &operation, parent)
                            .map(|entry| {
                                parents.insert(key, entry.id.clone());
                                recorded = Some((kind, meta.session, entry));
                            })
                    }
                    HistoryCommand::Pin(id, pinned) => store.set_pinned(&id, pinned),
                    HistoryCommand::Remove(id) => store.remove(&id),
                    HistoryCommand::Clear => store.clear_unpinned(),
                    HistoryCommand::Stop => break,
                };
                let entries = result.and_then(|()| store.list());
                let _ = replies.send(HistoryReply { entries, recorded });
                ctx.request_repaint();
            }
        });
        Self {
            sender,
            receiver,
            handle: Some(handle),
        }
    }
}

impl Drop for HistoryWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(HistoryCommand::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub(super) enum FileReply {
    Loaded {
        sequence: u64,
        text: Result<(String, DocumentMeta), String>,
    },
    Saved {
        kind: ContentKind,
        source: Arc<str>,
        path: PathBuf,
        result: Result<FileStamp, String>,
        continue_pending: bool,
    },
}

pub(super) fn load_file(
    sequence: u64,
    path: PathBuf,
    sender: mpsc::Sender<FileReply>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        let text = devshop::storage::read_document(&path).map(|(text, stamp)| {
            let meta = DocumentMeta {
                session: sequence,
                kind_hint: None,
                origin: path.display().to_string(),
                path: Some(path),
                stamp: Some(stamp),
                parent_id: None,
            };
            (text, meta)
        });
        let _ = sender.send(FileReply::Loaded { sequence, text });
        ctx.request_repaint();
    });
}

pub(super) fn load_history(
    sequence: u64,
    entry: HistoryEntry,
    store: HistoryStore,
    sender: mpsc::Sender<FileReply>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        let text = store.load(&entry).map(|text| {
            let meta = DocumentMeta {
                kind_hint: Some(entry.kind),
                origin: format!("历史 · {}", entry.origin),
                parent_id: Some(entry.id),
                ..Default::default()
            };
            (text, meta)
        });
        let _ = sender.send(FileReply::Loaded { sequence, text });
        ctx.request_repaint();
    });
}

pub(super) fn save_file(
    kind: ContentKind,
    source: Arc<str>,
    path: PathBuf,
    expected: Option<FileStamp>,
    continue_pending: bool,
    sender: mpsc::Sender<FileReply>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        let result = devshop::storage::save_document(&path, &source, expected.as_ref());
        let _ = sender.send(FileReply::Saved {
            kind,
            source,
            path,
            result,
            continue_pending,
        });
        ctx.request_repaint();
    });
}
