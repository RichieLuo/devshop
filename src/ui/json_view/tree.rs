use super::*;

const ROW_HEIGHT: f32 = 25.0;

impl JsonWorkspace {
    pub(super) fn rebuild_rows(&mut self) {
        self.cancel_rows();
        let Some(document) = self.document.clone() else {
            return;
        };
        let expanded = self.expanded.clone();
        let ctx = self.context.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancelled);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut rows = Vec::with_capacity(document.nodes().len().min(100_000));
            let mut current = Some(document.root());
            let mut depth = 0_u16;
            while let Some(id) = current {
                if rows.len() & 0xff == 0 && worker_cancel.load(Ordering::Relaxed) {
                    return;
                }
                rows.push(Row { node: id, depth });
                let node = document.node(id);
                if expanded.contains(&id)
                    && let Some(child) = node.first_child()
                {
                    current = Some(child);
                    depth += 1;
                    continue;
                }
                let mut cursor = id;
                loop {
                    if let Some(sibling) = document.node(cursor).next_sibling() {
                        current = Some(sibling);
                        break;
                    }
                    if let Some(parent) = document.node(cursor).parent() {
                        cursor = parent;
                        depth = depth.saturating_sub(1);
                    } else {
                        current = None;
                        break;
                    }
                }
            }
            if !worker_cancel.load(Ordering::Relaxed) {
                let _ = sender.send(rows);
                ctx.request_repaint();
            }
        });
        self.row_task = Some(RowsTask {
            receiver,
            cancelled,
        });
    }

    pub(super) fn poll_rows(&mut self) {
        if let Some(task) = self.row_task.take() {
            match task.receiver.try_recv() {
                Ok(rows) => {
                    self.rows = rows;
                    if let Some(selected) = self.selected {
                        self.scroll_to_row = self
                            .rows
                            .binary_search_by_key(&selected, |row| row.node)
                            .ok();
                    }
                }
                Err(mpsc::TryRecvError::Empty) => self.row_task = Some(task),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.action_error = Some("树形视图构建已中断，请重新展开节点".into())
                }
            }
        }
    }

    pub(super) fn show_tree(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("文档结构").strong());
            if ui
                .add_enabled(
                    self.document.is_some(),
                    egui::Button::new("折叠全部").small(),
                )
                .clicked()
            {
                self.expanded.clear();
                self.rebuild_rows();
            }
            if ui
                .add_enabled(
                    self.document.is_some(),
                    egui::Button::new("展开根节点").small(),
                )
                .clicked()
            {
                self.expanded.insert(0);
                self.rebuild_rows();
            }
            if self.row_task.is_some() {
                ui.spinner();
            }
            ui.label(
                RichText::new(format!("{} 行可见结构", self.rows.len()))
                    .small()
                    .color(theme::palette(ui.ctx()).muted),
            );
        });
        ui.separator();
        let Some(document) = self.document.clone() else {
            ui.vertical_centered(|ui| {
                ui.add_space(70.0);
                if self.work.is_some() {
                    ui.spinner();
                    ui.label("正在建立 JSON 索引…");
                } else {
                    ui.label(
                        RichText::new("粘贴或打开 JSON 后，在这里浏览结构")
                            .color(theme::palette(ui.ctx()).muted),
                    );
                }
            });
            return;
        };
        let mut scroll = egui::ScrollArea::both()
            .id_salt("json-tree-scroll")
            .auto_shrink([false, false]);
        if let Some(row) = self.scroll_to_row.take() {
            let offset = row as f32 * (ROW_HEIGHT + ui.spacing().item_spacing.y)
                - ui.available_height() * 0.4;
            scroll = scroll.vertical_scroll_offset(offset.max(0.0));
        }
        let mut toggle = None;
        let mut select = None;
        scroll.show_rows(ui, ROW_HEIGHT, self.rows.len(), |ui, visible| {
            for index in visible {
                let row = self.rows[index];
                let node = document.node(row.node);
                let selected = self.selected == Some(row.node);
                let key = if row.node == 0 {
                    "$".into()
                } else if let Some(index) = node.array_index() {
                    format!("[{index}]")
                } else {
                    key_preview(&document, row.node)
                };
                let key_galley = ui.painter().layout_no_wrap(
                    key,
                    egui::FontId::monospace(13.0),
                    theme::palette(ui.ctx()).accent,
                );
                let color = match node.kind {
                    Kind::String => theme::palette(ui.ctx()).string,
                    Kind::Number => theme::palette(ui.ctx()).number,
                    Kind::Bool | Kind::Null => theme::palette(ui.ctx()).literal,
                    _ => theme::palette(ui.ctx()).muted,
                };
                let value_galley = ui.painter().layout_no_wrap(
                    document.preview(row.node, 120),
                    egui::FontId::monospace(13.0),
                    color,
                );
                let content_width = 56.0
                    + f32::from(row.depth) * 17.0
                    + key_galley.size().x
                    + value_galley.size().x;
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(content_width), ROW_HEIGHT),
                    egui::Sense::click(),
                );
                if selected || response.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        4.0,
                        if selected {
                            theme::palette(ui.ctx()).accent_soft
                        } else {
                            theme::palette(ui.ctx()).hover
                        },
                    );
                }
                if self.hit_nodes.contains(&row.node) {
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.min.x + 3.0, rect.max.y),
                        ),
                        0.0,
                        theme::palette(ui.ctx()).accent,
                    );
                }
                let left = rect.left() + 8.0 + f32::from(row.depth) * 17.0;
                let arrow_rect = egui::Rect::from_center_size(
                    egui::pos2(left + 7.0, rect.center().y),
                    egui::vec2(18.0, ROW_HEIGHT),
                );
                if node.kind.is_container() && node.child_count() > 0 {
                    let center = arrow_rect.center();
                    let points = if self.expanded.contains(&row.node) {
                        vec![
                            center + egui::vec2(-4.0, -2.0),
                            center + egui::vec2(4.0, -2.0),
                            center + egui::vec2(0.0, 3.0),
                        ]
                    } else {
                        vec![
                            center + egui::vec2(-2.0, -4.0),
                            center + egui::vec2(-2.0, 4.0),
                            center + egui::vec2(3.0, 0.0),
                        ]
                    };
                    ui.painter().add(egui::Shape::convex_polygon(
                        points,
                        theme::palette(ui.ctx()).muted,
                        egui::Stroke::NONE,
                    ));
                }
                let value_x = left + 24.0 + key_galley.size().x + 12.0;
                ui.painter().galley(
                    egui::pos2(left + 24.0, rect.center().y - key_galley.size().y * 0.5),
                    key_galley,
                    theme::palette(ui.ctx()).accent,
                );
                ui.painter().galley(
                    egui::pos2(value_x, rect.center().y - value_galley.size().y * 0.5),
                    value_galley,
                    color,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::SelectableLabel,
                        true,
                        document.preview(row.node, 80),
                    )
                });
                if response.clicked() {
                    if arrow_rect.contains(response.interact_pointer_pos().unwrap_or_default())
                        && node.kind.is_container()
                    {
                        toggle = Some(row.node);
                    } else {
                        select = Some(row.node);
                    }
                }
                if response.double_clicked() && node.kind.is_container() {
                    toggle = Some(row.node);
                }
                response.context_menu(|ui| {
                    if ui.button("复制值").clicked() {
                        ui.ctx().copy_text(document.raw(row.node).to_owned());
                        ui.close();
                    }
                    if ui.button("复制 JSON Pointer").clicked() {
                        ui.ctx().copy_text(document.pointer(row.node));
                        ui.close();
                    }
                    if ui.button("复制 JSONPath").clicked() {
                        ui.ctx().copy_text(document.path(row.node));
                        ui.close();
                    }
                    if node.key_range().is_some()
                        && ui.button("复制字段名").clicked()
                        && let Some(key) = document.key(row.node)
                    {
                        ui.ctx().copy_text(key.into_owned());
                        ui.close();
                    }
                });
            }
        });
        if let Some(id) = toggle {
            if !self.expanded.remove(&id) {
                self.expanded.insert(id);
            }
            self.rebuild_rows();
        }
        if let Some(id) = select {
            self.select_node(id);
        }
    }
}

fn key_preview(document: &Document, id: NodeId) -> String {
    let Some(range) = document.node(id).key_range() else {
        return String::new();
    };
    let raw = &document.source()[range];
    if raw.len() > 4096 {
        json::truncate(&raw[1..raw.len() - 1], 100)
    } else {
        json::truncate(document.key(id).as_deref().unwrap_or(""), 100)
    }
}
