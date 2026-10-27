use devshop::json::SearchTarget;

use super::*;

impl JsonWorkspace {
    pub(super) fn search_bar(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("查找").strong());
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .id(egui::Id::new("json-search-input"))
                    .desired_width(260.0)
                    .hint_text("关键字或正则表达式 · Ctrl+F"),
            );
            if self.focus_search {
                response.request_focus();
                self.focus_search = false;
            }
            changed |= response.changed();
            if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.navigate_match(if ui.input(|input| input.modifiers.shift) {
                    -1
                } else {
                    1
                });
            }
            egui::ComboBox::from_id_salt("json-search-scope")
                .width(85.0)
                .selected_text(match self.search_options.target {
                    SearchTarget::Both => "字段和值",
                    SearchTarget::Keys => "仅字段名",
                    SearchTarget::Values => "仅值",
                })
                .show_ui(ui, |ui| {
                    for (target, title) in [
                        (SearchTarget::Both, "字段和值"),
                        (SearchTarget::Keys, "仅字段名"),
                        (SearchTarget::Values, "仅值"),
                    ] {
                        changed |= ui
                            .selectable_value(&mut self.search_options.target, target, title)
                            .changed();
                    }
                });
            changed |= ui
                .checkbox(&mut self.search_options.case_sensitive, "区分大小写")
                .changed();
            changed |= ui
                .checkbox(&mut self.search_options.regex, "正则")
                .changed();
            if self.search_task.is_some() {
                ui.spinner();
                if ui.small_button("取消搜索").clicked() {
                    self.cancel_search();
                }
            } else if ui
                .add_enabled(
                    self.document.is_some() && !self.query.is_empty(),
                    egui::Button::new("搜索").small(),
                )
                .clicked()
            {
                self.cancel_search();
                self.launch_search();
            }
            let has_matches = self
                .search_result
                .as_ref()
                .is_some_and(|result| !result.matches.is_empty());
            if ui
                .add_enabled(has_matches, egui::Button::new("↑").small())
                .on_hover_text("上一个匹配项")
                .clicked()
            {
                self.navigate_match(-1);
            }
            if ui
                .add_enabled(has_matches, egui::Button::new("↓").small())
                .on_hover_text("下一个匹配项")
                .clicked()
            {
                self.navigate_match(1);
            }
            if let Some(result) = &self.search_result {
                let text = if result.matches.is_empty() {
                    "没有匹配项".into()
                } else if result.truncated {
                    format!(
                        "{} / {} · 共 {} 项，导航前 {} 项",
                        self.active_match + 1,
                        result.matches.len(),
                        result.total,
                        result.matches.len()
                    )
                } else {
                    format!("{} / {} 项", self.active_match + 1, result.total)
                };
                ui.label(
                    RichText::new(text)
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
            if !self.query.is_empty() && ui.small_button("清除").clicked() {
                self.query.clear();
                changed = true;
            }
        });
        if let Some(error) = &self.search_error {
            ui.colored_label(theme::palette(ui.ctx()).danger, error);
        }
        if changed {
            self.schedule_search();
        }
    }

    pub(super) fn schedule_search(&mut self) {
        self.cancel_search();
        if !self.query.is_empty() && self.document.is_some() {
            self.search_due = Some(Instant::now() + Duration::from_millis(280));
            self.context
                .request_repaint_after(Duration::from_millis(280));
        }
    }

    pub(super) fn launch_search(&mut self) {
        let Some(document) = self.document.clone() else {
            return;
        };
        if self.query.is_empty() {
            return;
        }
        let query = self.query.clone();
        let options = self.search_options.clone();
        let ctx = self.context.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = document.search_with_cancel(&query, &options, &worker_cancel);
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = sender.send(result);
                ctx.request_repaint();
            }
        });
        self.search_task = Some(SearchTask {
            receiver,
            cancelled,
        });
    }

    pub(super) fn poll_search(&mut self) {
        if let Some(task) = self.search_task.take() {
            match task.receiver.try_recv() {
                Ok(Ok(result)) => {
                    self.hit_nodes = result.matches.iter().map(|item| item.node).collect();
                    self.active_match = 0;
                    self.search_result = Some(result);
                    self.reveal_match();
                }
                Ok(Err(error)) => self.search_error = Some(error),
                Err(mpsc::TryRecvError::Empty) => self.search_task = Some(task),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.search_error = Some("搜索任务已中断，请修改搜索条件后重试".into())
                }
            }
        }
    }

    fn navigate_match(&mut self, direction: i32) {
        if self.has_node_draft() {
            self.action_error =
                Some("当前节点有未应用的修改，请先应用修改或取消编辑，再切换搜索结果。".into());
            return;
        }
        let Some(result) = &self.search_result else {
            return;
        };
        let count = result.matches.len();
        if count == 0 {
            return;
        }
        self.active_match =
            (self.active_match as i64 + i64::from(direction)).rem_euclid(count as i64) as usize;
        self.reveal_match();
    }

    fn reveal_match(&mut self) {
        if self.has_node_draft() {
            self.action_error =
                Some("搜索已完成；当前节点修改尚未应用，应用或取消编辑后可跳转结果。".into());
            return;
        }
        let Some(result) = &self.search_result else {
            return;
        };
        let Some(item) = result.matches.get(self.active_match) else {
            return;
        };
        let id = item.node;
        let Some(document) = self.document.clone() else {
            return;
        };
        let mut current = document.node(id).parent();
        let mut changed = false;
        while let Some(parent) = current {
            changed |= self.expanded.insert(parent);
            current = document.node(parent).parent();
        }
        self.select_node(id);
        self.view = View::Tree;
        self.scroll_to_node = Some(id);
        if changed {
            self.rebuild_rows();
        }
    }
}
