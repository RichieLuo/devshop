use super::*;
use jwt_view::JwtEvent;

impl App {
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        for event in self.json.poll(ctx) {
            let json_view::JsonEvent::Parsed { source, operation } = event;
            self.record(ContentKind::Json, source, &operation);
        }
        while let Ok(reply) = self.history.receiver.try_recv() {
            match reply.entries {
                Ok(entries) => self.entries = entries,
                Err(error) => self.status = format!("历史记录：{error}"),
            }
            if let Some((kind, session, entry)) = reply.recorded
                && self.meta(kind).session == session
            {
                self.meta_mut(kind).parent_id = Some(entry.id);
            }
        }
        while let Ok(reply) = self.files_rx.try_recv() {
            match reply {
                FileReply::Loaded { sequence, text } if sequence == self.file_sequence => {
                    self.file_loading = false;
                    match text {
                        Ok((text, meta)) => {
                            let kind = meta
                                .kind_hint
                                .unwrap_or_else(|| detect_kind(&text, meta.path.as_deref()));
                            let dirty = meta.path.is_none();
                            self.request_action(
                                PendingAction::Install {
                                    kind,
                                    text,
                                    meta,
                                    dirty,
                                },
                                ctx,
                            );
                        }
                        Err(error) => self.status = format!("打开失败：{error}"),
                    }
                }
                FileReply::Loaded { .. } => {}
                FileReply::Saved {
                    kind,
                    source,
                    path,
                    result,
                    continue_pending,
                } => {
                    self.saving = false;
                    match result {
                        Ok(stamp) => {
                            self.meta_mut(kind).path = Some(path.clone());
                            self.meta_mut(kind).stamp = Some(stamp);
                            match kind {
                                ContentKind::Json => self.json.mark_saved(&source),
                                ContentKind::Jwt => self.jwt_saved = source.to_string(),
                            }
                            self.record(kind, source, &format!("保存到 {}", path.display()));
                            self.status = format!("已保存到 {}", path.display());
                            if continue_pending {
                                if self.is_dirty(kind) {
                                    self.status =
                                        "保存期间内容已变化，请再次保存或取消当前操作".into();
                                } else if let Some(action) = self.pending.take() {
                                    self.perform_action(action, ctx);
                                }
                            }
                        }
                        Err(error) => self.status = format!("保存失败：{error}"),
                    }
                }
            }
        }
        match crate::shell_open::poll(ctx) {
            Ok(Some(paths)) => {
                self.show_main(ctx);
                if let Some(path) = paths.into_iter().next() {
                    self.open_path(path, ctx);
                }
            }
            Err(error) => self.status = format!("文件打开请求：{error}"),
            Ok(None) => {}
        }
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.open_path(path, ctx);
        }
        if let Some(requested) = self.paste_requested {
            let paste = ctx.input_mut(|input| {
                let mut pasted = None;
                input.events.retain(|event| {
                    if let egui::Event::Paste(text) = event {
                        pasted = Some(text.clone());
                        false
                    } else {
                        true
                    }
                });
                pasted
            });
            if let Some(text) = paste {
                self.paste_requested = None;
                self.install_clipboard(text, ctx);
            } else if requested.elapsed() > Duration::from_millis(1500) {
                self.paste_requested = None;
                self.status = "剪贴板没有可读取的文本".into();
            }
        }
    }

    pub(super) fn record(&mut self, kind: ContentKind, text: Arc<str>, operation: &str) {
        if (!self.settings.history_enabled && operation != "手动保存历史") || text.trim().is_empty()
        {
            return;
        }
        let command = HistoryCommand::Record {
            kind,
            text,
            meta: self.meta(kind).clone(),
            operation: operation.into(),
        };
        if self.history.sender.send(command).is_err() {
            self.status = "历史记录不可用，请检查本地数据目录".into();
        }
    }

    pub(super) fn jwt_event(&mut self, event: JwtEvent, ctx: &egui::Context) {
        match event {
            JwtEvent::Decoded { token } => {
                self.record(ContentKind::Jwt, Arc::from(token), "解码 JWT")
            }
            JwtEvent::Signed { token } => {
                self.record(ContentKind::Jwt, Arc::from(token), "签名 JWT")
            }
            JwtEvent::RecordRequested { token } => {
                self.record(ContentKind::Jwt, Arc::from(token), "手动保存历史")
            }
            JwtEvent::SaveRequested { text } => {
                if text == self.jwt.save_text() {
                    self.save_current(false, false, ctx);
                } else {
                    self.status = "JWT 内容已变化，请重新保存".into();
                }
            }
            JwtEvent::ReplaceRequested { token, origin } => self.request_action(
                PendingAction::Install {
                    kind: ContentKind::Jwt,
                    text: token,
                    meta: DocumentMeta {
                        origin,
                        ..Default::default()
                    },
                    dirty: true,
                },
                ctx,
            ),
            JwtEvent::NewRequested => {
                self.request_action(PendingAction::New(ContentKind::Jwt), ctx)
            }
        }
    }

    pub(super) fn request_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        if self.saving {
            self.status = "正在保存，请等待完成".into();
            return;
        }
        let dirty = action
            .target()
            .filter(|kind| self.is_dirty(*kind))
            .or_else(|| {
                if matches!(action, PendingAction::Exit) {
                    [ContentKind::Json, ContentKind::Jwt]
                        .into_iter()
                        .find(|kind| self.is_dirty(*kind))
                } else {
                    None
                }
            });
        if let Some(kind) = dirty {
            self.pending = Some(action);
            self.confirm_kind = Some(kind);
        } else {
            self.perform_action(action, ctx);
        }
    }

    pub(super) fn perform_action(&mut self, action: PendingAction, ctx: &egui::Context) {
        self.confirm_kind = None;
        match action {
            PendingAction::Install {
                kind,
                text,
                mut meta,
                dirty,
            } => {
                self.file_sequence += 1;
                meta.session = self.file_sequence;
                self.active = kind;
                *self.meta_mut(kind) = meta;
                match kind {
                    ContentKind::Json => self.json.set_source(text, dirty, "载入 JSON"),
                    ContentKind::Jwt => {
                        self.jwt.set_token(&text);
                        self.jwt_saved = if dirty {
                            String::new()
                        } else {
                            self.jwt.token().to_string()
                        };
                        self.record(kind, Arc::from(self.jwt.token()), "载入 JWT");
                    }
                }
                self.status = format!("来源：{}", self.meta(kind).origin);
            }
            PendingAction::New(kind) => {
                self.active = kind;
                self.file_sequence += 1;
                *self.meta_mut(kind) = DocumentMeta {
                    session: self.file_sequence,
                    origin: format!("新建 {}", kind_name(kind)),
                    ..Default::default()
                };
                match kind {
                    ContentKind::Json => self.json.set_source(String::new(), false, "新建"),
                    ContentKind::Jwt => {
                        self.jwt = JwtWorkspace::default();
                        self.jwt_saved.clear();
                    }
                }
            }
            PendingAction::Exit => {
                // Saving one workspace can still leave the other one dirty.
                if let Some(kind) = [ContentKind::Json, ContentKind::Jwt]
                    .into_iter()
                    .find(|kind| self.is_dirty(*kind))
                {
                    self.pending = Some(PendingAction::Exit);
                    self.confirm_kind = Some(kind);
                } else {
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    pub(super) fn discard_and_continue(&mut self, ctx: &egui::Context) {
        if let Some(kind) = self.confirm_kind {
            match kind {
                ContentKind::Json => {
                    self.json.discard_node_draft();
                    let source = self.json.source();
                    self.json.mark_saved(&source);
                }
                ContentKind::Jwt => {
                    self.jwt.discard_editor_draft();
                    self.jwt_saved = self.jwt.token().to_string();
                }
            }
        }
        if let Some(action) = self.pending.take() {
            self.perform_action(action, ctx);
        }
    }

    pub(super) fn open_path(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.file_sequence += 1;
        self.file_loading = true;
        self.status = format!("正在读取 {}", path.display());
        background::load_file(self.file_sequence, path, self.files_tx.clone(), ctx.clone());
    }

    pub(super) fn open_dialog(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("打开 JSON 或 JWT")
            .add_filter("JSON / JWT", &["json", "jwt", "txt", "token"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        {
            self.open_path(path, ctx);
        }
    }

    pub(super) fn open_history(&mut self, entry: HistoryEntry, ctx: &egui::Context) {
        if let Some(store) = self.history_store.clone() {
            self.file_sequence += 1;
            self.file_loading = true;
            background::load_history(
                self.file_sequence,
                entry,
                store,
                self.files_tx.clone(),
                ctx.clone(),
            );
        }
    }

    pub(super) fn save_current(
        &mut self,
        save_as: bool,
        continue_pending: bool,
        ctx: &egui::Context,
    ) {
        if self.saving {
            return;
        }
        let kind = if continue_pending {
            self.confirm_kind.unwrap_or(self.active)
        } else {
            self.active
        };
        if kind == ContentKind::Json && self.json.has_node_draft() {
            self.status = "JSON 节点有未应用的编辑，请先应用修改或取消节点编辑，再保存".into();
            return;
        }
        if kind == ContentKind::Jwt && !self.jwt.can_save() {
            self.status = "JWT 含未签名的编辑或无效 Token，请先解码或生成并签名，再保存".into();
            return;
        }
        let source = self.source(kind);
        if source.is_empty() {
            self.status = "当前没有可保存的内容".into();
            return;
        }
        let meta = self.meta(kind);
        let (path, expected) = if !save_as && meta.path.is_some() {
            (meta.path.clone().unwrap(), meta.stamp.clone())
        } else {
            let extension = if kind == ContentKind::Json {
                "json"
            } else {
                "jwt"
            };
            let Some(path) = rfd::FileDialog::new()
                .set_title("保存文档")
                .set_file_name(format!("未命名.{extension}"))
                .add_filter(kind_name(kind), &[extension])
                .save_file()
            else {
                return;
            };
            let expected = if path.exists() {
                match devshop::storage::FileStamp::read(&path) {
                    Ok(stamp) => Some(stamp),
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                }
            } else {
                None
            };
            (path, expected)
        };
        self.saving = true;
        self.status = "正在保存…".into();
        background::save_file(
            kind,
            source,
            path,
            expected,
            continue_pending,
            self.files_tx.clone(),
            ctx.clone(),
        );
    }

    pub(super) fn request_clipboard(&mut self, ctx: &egui::Context) {
        self.paste_requested = Some(std::time::Instant::now());
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        self.status = "正在读取剪贴板…".into();
    }

    fn install_clipboard(&mut self, text: String, ctx: &egui::Context) {
        if text.trim().is_empty() {
            self.status = "剪贴板没有文本".into();
            return;
        }
        let kind = detect_kind(&text, None);
        self.request_action(
            PendingAction::Install {
                kind,
                text,
                meta: DocumentMeta {
                    origin: "剪贴板".into(),
                    ..Default::default()
                },
                dirty: true,
            },
            ctx,
        );
    }

    pub(super) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let command = egui::Modifiers::COMMAND;
        let shifted = command | egui::Modifiers::SHIFT;
        if ctx.input_mut(|i| i.consume_key(command, egui::Key::O)) {
            self.open_dialog(ctx);
        }
        if ctx.input_mut(|i| i.consume_key(shifted, egui::Key::S)) {
            self.save_current(true, false, ctx);
        } else if ctx.input_mut(|i| i.consume_key(command, egui::Key::S)) {
            self.save_current(false, false, ctx);
        }
        if ctx.input_mut(|i| i.consume_key(command, egui::Key::N)) {
            self.request_action(PendingAction::New(self.active), ctx);
        }
        if ctx.input_mut(|i| i.consume_key(command, egui::Key::F))
            && self.active == ContentKind::Json
        {
            self.json.focus_search();
        }
        if ctx.input_mut(|i| i.consume_key(command, egui::Key::H)) {
            self.settings.show_history = !self.settings.show_history;
        }
        if ctx.input_mut(|i| i.consume_key(shifted, egui::Key::V)) {
            self.request_clipboard(ctx);
        }
    }
}

fn detect_kind(text: &str, path: Option<&std::path::Path>) -> ContentKind {
    let input = text.trim().strip_prefix("Bearer ").unwrap_or(text.trim());
    if path
        .and_then(|p| p.extension())
        .is_some_and(|x| x.eq_ignore_ascii_case("jwt") || x.eq_ignore_ascii_case("token"))
        || (!input.starts_with(['{', '[', '"'])
            && input.split('.').count() == 3
            && !input.contains(char::is_whitespace))
    {
        ContentKind::Jwt
    } else {
        ContentKind::Json
    }
}

pub(super) fn kind_name(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Json => "JSON",
        ContentKind::Jwt => "JWT",
    }
}
