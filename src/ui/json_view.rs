//! JSON workspace orchestration. Parsing, search and row construction run on
//! workers; the UI paints only the rows intersecting the viewport.

mod actions;
mod details;
mod raw;
mod search;
mod tree;

#[cfg(test)]
mod tests;

use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

use super::theme;
use devshop::json::{self, Document, JsonError, Kind, NodeId, SearchOptions, SearchResult};
use eframe::egui::{self, RichText};

const EDIT_LIMIT: usize = 2 * 1024 * 1024;

pub enum JsonEvent {
    Parsed { source: Arc<str>, operation: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Tree,
    Source,
}

pub struct JsonWorkspace {
    context: egui::Context,
    source: Arc<str>,
    document: Option<Arc<Document>>,
    error: Option<JsonError>,
    action_error: Option<String>,
    dirty: bool,
    edit_text: String,
    view: View,
    work: Option<WorkTask>,
    parse_due: Option<Instant>,
    operation: Option<String>,
    lines: Arc<Vec<usize>>,
    expanded: HashSet<NodeId>,
    rows: Vec<Row>,
    row_task: Option<RowsTask>,
    selected: Option<NodeId>,
    scroll_to_node: Option<NodeId>,
    detail_text: String,
    detail_path: String,
    detail_pointer: String,
    editing_node: bool,
    detail_truncated: bool,
    query: String,
    search_options: SearchOptions,
    search_result: Option<SearchResult>,
    search_error: Option<String>,
    search_task: Option<SearchTask>,
    search_due: Option<Instant>,
    active_match: usize,
    hit_nodes: HashSet<NodeId>,
    focus_search: bool,
    parse_ms: f64,
    source_scroll_line: Option<usize>,
    welcome: bool,
}

struct WorkTask {
    receiver: Receiver<WorkResult>,
    cancelled: Arc<AtomicBool>,
    label: &'static str,
}
enum WorkResult {
    Analysis {
        source: Arc<str>,
        document: Result<Document, JsonError>,
        lines: Vec<usize>,
        operation: Option<String>,
        elapsed_ms: f64,
        dirty: bool,
    },
    Rejected(String),
}
#[derive(Clone, Copy)]
struct Row {
    node: NodeId,
    depth: u16,
}
struct RowsTask {
    receiver: Receiver<Vec<Row>>,
    cancelled: Arc<AtomicBool>,
}
struct SearchTask {
    receiver: Receiver<Result<SearchResult, String>>,
    cancelled: Arc<AtomicBool>,
}

impl JsonWorkspace {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            context: ctx.clone(),
            source: Arc::from(""),
            document: None,
            error: None,
            action_error: None,
            dirty: false,
            edit_text: String::new(),
            view: View::Source,
            work: None,
            parse_due: None,
            operation: Some("编辑".into()),
            lines: Arc::new(vec![0]),
            expanded: HashSet::new(),
            rows: Vec::new(),
            row_task: None,
            selected: None,
            scroll_to_node: None,
            detail_text: String::new(),
            detail_path: String::new(),
            detail_pointer: String::new(),
            editing_node: false,
            detail_truncated: false,
            query: String::new(),
            search_options: SearchOptions::default(),
            search_result: None,
            search_error: None,
            search_task: None,
            search_due: None,
            active_match: 0,
            hit_nodes: HashSet::new(),
            focus_search: false,
            parse_ms: 0.0,
            source_scroll_line: None,
            welcome: true,
        }
    }

    /// `None` restores a snapshot without emitting a new history event.
    pub fn set_source(&mut self, text: String, dirty: bool, operation: Option<&str>) {
        self.welcome = text.is_empty();
        self.cancel_work();
        self.cancel_search();
        self.cancel_rows();
        self.source = Arc::from(text);
        self.dirty = dirty;
        self.operation = operation.map(str::to_owned);
        self.parse_due = None;
        self.document = None;
        self.error = None;
        self.action_error = None;
        self.rows.clear();
        self.selected = None;
        self.scroll_to_node = None;
        self.detail_text.clear();
        self.editing_node = false;
        self.lines = Arc::new(vec![0]);
        self.edit_text = if self.source.len() <= EDIT_LIMIT {
            self.source.to_string()
        } else {
            String::new()
        };
        self.view = if self.source.is_empty() {
            View::Source
        } else {
            View::Tree
        };
        if !self.source.is_empty() {
            self.launch_parse();
        }
    }

    pub fn source(&self) -> Arc<str> {
        Arc::clone(&self.source)
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty || self.has_node_draft()
    }
    pub fn has_node_draft(&self) -> bool {
        self.editing_node
            && self
                .selected
                .zip(self.document.as_ref())
                .is_some_and(|(id, document)| {
                    (id as usize) < document.nodes().len() && self.detail_text != document.raw(id)
                })
    }
    pub fn discard_node_draft(&mut self) {
        if self.editing_node {
            self.cancel_work();
            self.editing_node = false;
            if let Some(id) = self.selected {
                self.select_node(id);
            }
        }
    }
    pub fn mark_saved(&mut self, saved: &str) {
        if self.source.as_ref() == saved {
            self.dirty = false;
        }
    }
    pub fn busy(&self) -> bool {
        self.work.is_some() || self.parse_due.is_some()
    }
    pub fn focus_search(&mut self) {
        self.welcome = false;
        self.focus_search = true;
    }

    pub fn is_welcome(&self) -> bool {
        self.welcome && self.source.is_empty() && !self.busy()
    }

    pub fn begin_edit(&mut self) {
        self.welcome = false;
        self.view = View::Source;
        self.context
            .memory_mut(|memory| memory.request_focus(egui::Id::new("json-source-editor")));
    }

    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<JsonEvent> {
        let mut events = Vec::new();
        if self.parse_due.is_some_and(|due| Instant::now() >= due) {
            self.parse_due = None;
            if !self.source.is_empty() {
                self.launch_parse();
            }
        }
        if let Some(task) = self.work.take() {
            match task.receiver.try_recv() {
                Ok(WorkResult::Analysis {
                    source,
                    document,
                    lines,
                    operation,
                    elapsed_ms,
                    dirty,
                }) => {
                    self.source = source;
                    self.lines = Arc::new(lines);
                    self.dirty = dirty;
                    self.parse_ms = elapsed_ms;
                    self.edit_text = if self.source.len() <= EDIT_LIMIT {
                        self.source.to_string()
                    } else {
                        String::new()
                    };
                    if matches!(operation.as_deref(), Some("格式化" | "压缩")) {
                        self.view = View::Source;
                    }
                    match document {
                        Ok(document) => {
                            self.document = Some(Arc::new(document));
                            self.error = None;
                            self.action_error = None;
                            self.expanded.clear();
                            self.expanded.insert(0);
                            self.selected = None;
                            self.select_node(0);
                            self.scroll_to_node = Some(0);
                            self.rebuild_rows();
                            self.schedule_search();
                            if let Some(operation) = operation {
                                events.push(JsonEvent::Parsed {
                                    source: Arc::clone(&self.source),
                                    operation,
                                });
                            }
                        }
                        Err(error) => {
                            self.document = None;
                            self.error = Some(error);
                            self.rows.clear();
                            self.view = View::Source;
                        }
                    }
                }
                Ok(WorkResult::Rejected(message)) => self.action_error = Some(message),
                Err(mpsc::TryRecvError::Empty) => self.work = Some(task),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.action_error = Some("后台任务已结束，请重试".into())
                }
            }
        }
        self.poll_rows();
        if self.search_due.is_some_and(|due| Instant::now() >= due) {
            self.search_due = None;
            self.launch_search();
        }
        self.poll_search();
        if self.work.is_some() || self.row_task.is_some() || self.search_task.is_some() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        if let Some(due) = self.parse_due.into_iter().chain(self.search_due).min() {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        events
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        theme::heading(
            ui,
            "JSON 工作台",
            "浏览结构、查找内容，轻松整理每一段 JSON。",
            "JSON / 本地处理",
        );
        theme::card(ui)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.toolbar(ui);
                ui.add_space(4.0);
                self.search_bar(ui);
            });
        if self.has_node_draft() {
            ui.label(
                RichText::new("节点修改尚未应用：请在节点详情中应用修改，或取消编辑。")
                    .small()
                    .color(theme::palette(ui.ctx()).accent),
            );
        }
        ui.add_space(14.0);
        if let Some(error) = &self.error {
            egui::Frame::new()
                .fill(theme::palette(ui.ctx()).danger_soft)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("解析失败：{error}"))
                            .color(theme::palette(ui.ctx()).danger),
                    );
                    ui.label("输入内容已保留。修正后会自动重新解析，也可点击“解析”。");
                });
        }
        if let Some(message) = self.action_error.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(theme::palette(ui.ctx()).danger, message);
                if ui.small_button("关闭提示").clicked() {
                    self.action_error = None;
                }
            });
        }
        if self.view == View::Tree {
            let max_details_width = (ui.available_width() - 280.0).clamp(230.0, 650.0);
            egui::SidePanel::right("json-node-details")
                .resizable(true)
                .show_separator_line(false)
                .default_width(330.0)
                .width_range(230.0..=max_details_width)
                .frame(egui::Frame::NONE)
                .show_inside(ui, |ui| {
                    // Split the gap across both cards so resizing never hits a card border.
                    theme::card(ui)
                        .outer_margin(egui::Margin {
                            left: 6,
                            ..Default::default()
                        })
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("json-details-scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.show_details(ui));
                        });
                });
            egui::CentralPanel::default()
                .frame(theme::card(ui).outer_margin(egui::Margin {
                    right: 6,
                    ..Default::default()
                }))
                .show_inside(ui, |ui| self.show_tree(ui));
        } else {
            theme::card(ui).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                self.show_source(ui);
            });
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.view, View::Tree, "树形视图");
            ui.selectable_value(&mut self.view, View::Source, "原文 / 编辑");
            ui.separator();
            let ready = self.document.is_some() && !self.busy() && !self.has_node_draft();
            if ui.add_enabled(ready, egui::Button::new("格式化")).clicked() {
                self.launch_format(true);
            }
            if ui.add_enabled(ready, egui::Button::new("压缩")).clicked() {
                self.launch_format(false);
            }
            if ui
                .add_enabled(
                    !self.source.is_empty() && !self.busy() && !self.has_node_draft(),
                    egui::Button::new("解析"),
                )
                .clicked()
            {
                self.launch_parse();
            }
            if let Some(task) = &self.work {
                ui.spinner();
                ui.label(RichText::new(task.label).color(theme::palette(ui.ctx()).muted));
                if ui.small_button("取消").clicked() {
                    self.cancel_work();
                    self.action_error = Some("任务已取消，当前文本保留".into());
                }
            } else if self.parse_due.is_some() {
                ui.label(
                    RichText::new("文本已修改，等待解析…").color(theme::palette(ui.ctx()).muted),
                );
            } else if let Some(document) = &self.document {
                ui.label(
                    RichText::new(format!(
                        "{}  ·  {} 个节点  ·  {:.0} ms",
                        byte_size(self.source.len()),
                        document.nodes().len(),
                        self.parse_ms
                    ))
                    .small()
                    .color(theme::palette(ui.ctx()).muted),
                );
            } else if !self.source.is_empty() {
                ui.label(
                    RichText::new(byte_size(self.source.len()))
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
        });
    }

    fn cancel_work(&mut self) {
        if let Some(task) = self.work.take() {
            task.cancelled.store(true, Ordering::Relaxed);
        }
    }
    fn cancel_rows(&mut self) {
        if let Some(task) = self.row_task.take() {
            task.cancelled.store(true, Ordering::Relaxed);
        }
    }
    fn cancel_search(&mut self) {
        if let Some(task) = self.search_task.take() {
            task.cancelled.store(true, Ordering::Relaxed);
        }
        self.search_result = None;
        self.hit_nodes.clear();
        self.search_error = None;
        self.search_due = None;
    }
}

impl Drop for JsonWorkspace {
    fn drop(&mut self) {
        self.cancel_work();
        self.cancel_rows();
        self.cancel_search();
    }
}

fn byte_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
