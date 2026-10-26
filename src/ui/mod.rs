mod actions;
mod background;
pub mod floating;
mod json_view;
mod jwt_view;
mod shell;
mod theme;
mod welcome;
mod window_chrome;

use background::{DocumentMeta, FileReply, HistoryCommand, HistoryWorker};
use devshop::storage::{ContentKind, HistoryEntry, HistoryStore};
use eframe::egui;
use floating::{Action as FloatingAction, FloatingBall};
use json_view::JsonWorkspace;
use jwt_view::JwtWorkspace;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    floating_enabled: bool,
    floating_position: Option<[f32; 2]>,
    dark: bool,
    history_enabled: bool,
    show_history: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            floating_enabled: true,
            floating_position: None,
            dark: false,
            history_enabled: true,
            show_history: true,
        }
    }
}

enum PendingAction {
    Install {
        kind: ContentKind,
        text: String,
        meta: DocumentMeta,
        dirty: bool,
    },
    New(ContentKind),
    Exit,
}

impl PendingAction {
    fn target(&self) -> Option<ContentKind> {
        match self {
            Self::Install { kind, .. } | Self::New(kind) => Some(*kind),
            Self::Exit => None,
        }
    }
}

pub struct App {
    json: JsonWorkspace,
    jwt: JwtWorkspace,
    active: ContentKind,
    json_meta: DocumentMeta,
    jwt_meta: DocumentMeta,
    jwt_saved: String,
    settings: Settings,
    floating: FloatingBall,
    history_store: Option<HistoryStore>,
    history: HistoryWorker,
    entries: Vec<HistoryEntry>,
    history_filter: String,
    files_tx: mpsc::Sender<FileReply>,
    files_rx: mpsc::Receiver<FileReply>,
    file_sequence: u64,
    file_loading: bool,
    saving: bool,
    status: String,
    pending: Option<PendingAction>,
    confirm_kind: Option<ContentKind>,
    clear_history_confirm: bool,
    exiting: bool,
    paste_requested: Option<std::time::Instant>,
    main_hidden: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::branding::install_fonts(&cc.egui_ctx);
        window_chrome::configure_window(cc);
        let settings: Settings = cc
            .storage
            .and_then(|s| eframe::get_value(s, "devshop-settings"))
            .unwrap_or_default();
        let mut floating = FloatingBall::default();
        floating.enabled = settings.floating_enabled;
        floating.set_position(settings.floating_position);
        let store = HistoryStore::open(devshop::storage::default_data_directory());
        let history_store = store.as_ref().ok().cloned();
        let history = HistoryWorker::start(store, cc.egui_ctx.clone());
        let (files_tx, files_rx) = mpsc::channel();
        let mut app = Self {
            json: JsonWorkspace::new(&cc.egui_ctx),
            jwt: JwtWorkspace::default(),
            active: ContentKind::Json,
            json_meta: DocumentMeta {
                origin: "新建 JSON".into(),
                ..Default::default()
            },
            jwt_meta: DocumentMeta {
                origin: "新建 JWT".into(),
                ..Default::default()
            },
            jwt_saved: String::new(),
            settings,
            floating,
            history_store,
            history,
            entries: Vec::new(),
            history_filter: String::new(),
            files_tx,
            files_rx,
            file_sequence: 0,
            file_loading: false,
            saving: false,
            status: "就绪 · 数据仅在本地处理".into(),
            pending: None,
            confirm_kind: None,
            clear_history_confirm: false,
            exiting: false,
            paste_requested: None,
            main_hidden: false,
        };
        app.apply_theme(&cc.egui_ctx);
        app
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        theme::apply(ctx, self.settings.dark);
    }

    fn is_dirty(&self, kind: ContentKind) -> bool {
        match kind {
            ContentKind::Json => self.json.is_dirty(),
            ContentKind::Jwt => self.jwt.token() != self.jwt_saved || self.jwt.has_editor_draft(),
        }
    }
    fn meta(&self, kind: ContentKind) -> &DocumentMeta {
        match kind {
            ContentKind::Json => &self.json_meta,
            ContentKind::Jwt => &self.jwt_meta,
        }
    }
    fn meta_mut(&mut self, kind: ContentKind) -> &mut DocumentMeta {
        match kind {
            ContentKind::Json => &mut self.json_meta,
            ContentKind::Jwt => &mut self.jwt_meta,
        }
    }
    fn source(&self, kind: ContentKind) -> Arc<str> {
        match kind {
            ContentKind::Json => self.json.source(),
            ContentKind::Jwt => Arc::from(self.jwt.token()),
        }
    }

    fn show_main(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.main_hidden = false;
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.exiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.floating.enabled {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.main_hidden = true;
            } else {
                self.request_action(PendingAction::Exit, ctx);
            }
        }
        self.handle_shortcuts(ctx);
        if self.floating.position().is_none()
            && let Some(rect) = ctx.input(|input| input.viewport().outer_rect)
        {
            self.floating
                .set_position(Some([rect.max.x - 90.0, rect.max.y - 130.0]));
        }
        self.floating.show(ctx);
        for action in self.floating.take_actions() {
            match action {
                FloatingAction::Open => self.show_main(ctx),
                FloatingAction::Clipboard => {
                    self.show_main(ctx);
                    self.request_clipboard(ctx);
                }
                FloatingAction::History => {
                    self.settings.show_history = true;
                    self.show_main(ctx);
                }
                FloatingAction::Hide => {
                    self.floating.enabled = false;
                    self.show_main(ctx);
                }
                FloatingAction::Exit => {
                    self.show_main(ctx);
                    self.request_action(PendingAction::Exit, ctx);
                }
            }
        }
        self.chrome(ctx);
        self.toolbar(ctx);
        self.status_bar(ctx);
        self.history_panel(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::palette(ctx).background)
                    .inner_margin(22),
            )
            .show(ctx, |ui| match self.active {
                ContentKind::Json if self.json.is_welcome() => self.welcome(ui, ctx),
                ContentKind::Json => self.json.show(ui),
                ContentKind::Jwt => {
                    if let Some(event) = self.jwt.show(ui) {
                        self.jwt_event(event, ctx);
                    }
                }
            });
        self.dialogs(ctx);
        window_chrome::resize_window(ctx);
        window_chrome::paint_border(ctx);
        // Also services the single-instance inbox while the main window is hidden.
        ctx.request_repaint_after(Duration::from_millis(if self.main_hidden {
            400
        } else {
            200
        }));
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.settings.floating_enabled = self.floating.enabled;
        self.settings.floating_position = self.floating.position();
        eframe::set_value(storage, "devshop-settings", &self.settings);
    }
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
}
