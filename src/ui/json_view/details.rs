use super::*;

impl JsonWorkspace {
    pub(super) fn select_node(&mut self, id: NodeId) {
        if self.has_node_draft() {
            self.action_error = Some("当前节点有未应用的修改，请先应用修改或取消编辑。".into());
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        self.selected = Some(id);
        self.editing_node = false;
        self.detail_truncated = document.raw(id).len() > 32 * 1024;
        self.detail_text = if self.detail_truncated {
            json::truncate(document.raw(id), 8000)
        } else {
            document.raw(id).to_owned()
        };
        self.detail_path = document.path(id);
        self.detail_pointer = document.pointer(id);
    }

    pub(super) fn show_details(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.heading(RichText::new("节点详情").size(15.0));
        ui.add_space(7.0);
        let Some(document) = self.document.clone() else {
            ui.label(RichText::new("解析后选择节点查看").color(theme::palette(ui.ctx()).muted));
            return;
        };
        let Some(id) = self.selected else {
            ui.label("选择左侧节点");
            return;
        };
        let node = document.node(id);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(node.kind.label())
                    .strong()
                    .color(theme::palette(ui.ctx()).accent),
            );
            ui.label(
                RichText::new(byte_size(node.range().len()))
                    .small()
                    .color(theme::palette(ui.ctx()).muted),
            );
            if node.kind.is_container() {
                ui.label(
                    RichText::new(format!("{} 项", node.child_count()))
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("JSONPath")
                .small()
                .color(theme::palette(ui.ctx()).muted),
        );
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Label::new(RichText::new(json::truncate(&self.detail_path, 300)).monospace())
                    .wrap(),
            );
            if ui
                .small_button("复制")
                .on_hover_text("复制完整 JSONPath")
                .clicked()
            {
                ui.ctx().copy_text(self.detail_path.clone());
            }
        });
        ui.label(
            RichText::new("JSON Pointer")
                .small()
                .color(theme::palette(ui.ctx()).muted),
        );
        ui.horizontal_wrapped(|ui| {
            let pointer = if self.detail_pointer.is_empty() {
                "（根节点：空指针）"
            } else {
                &self.detail_pointer
            };
            ui.add(
                egui::Label::new(RichText::new(json::truncate(pointer, 300)).monospace()).wrap(),
            );
            if ui
                .small_button("复制")
                .on_hover_text("复制完整 JSON Pointer")
                .clicked()
            {
                ui.ctx().copy_text(self.detail_pointer.clone());
            }
        });
        ui.add_space(7.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui.button("复制完整值").clicked() {
                ui.ctx().copy_text(document.raw(id).to_owned());
            }
            if node.key_range().is_some()
                && ui.button("复制字段名").clicked()
                && let Some(key) = document.key(id)
            {
                ui.ctx().copy_text(key.into_owned());
            }
            if self.source.len() > EDIT_LIMIT && ui.button("定位原文").clicked() {
                self.source_scroll_line = Some(
                    self.lines
                        .partition_point(|&start| start <= node.range().start)
                        .saturating_sub(1),
                );
                self.view = View::Source;
            }
        });
        if self.editing_node {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy(), egui::Button::new("应用修改"))
                    .clicked()
                {
                    self.launch_replace(id, self.detail_text.clone());
                }
                if ui.button("取消编辑").clicked() {
                    self.discard_node_draft();
                }
            });
            ui.label(
                RichText::new("输入完整 JSON 值；字符串需带双引号。应用前会校验。")
                    .small()
                    .color(theme::palette(ui.ctx()).muted),
            );
            let editable = !self.busy();
            egui::ScrollArea::both()
                .id_salt("json-detail-editor-scroll")
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.detail_text)
                            .id(egui::Id::new("json-node-value-editor"))
                            .code_editor()
                            .interactive(editable)
                            .desired_width(f32::INFINITY)
                            .desired_rows(18),
                    );
                });
        } else {
            let can_edit = node.range().len() <= EDIT_LIMIT && !self.busy();
            if ui
                .add_enabled(can_edit, egui::Button::new("编辑此节点"))
                .clicked()
            {
                self.detail_text = document.raw(id).to_owned();
                self.editing_node = true;
            }
            if node.range().len() > EDIT_LIMIT {
                ui.label(
                    RichText::new("此节点超过 2 MiB，请展开后编辑更小的子节点。")
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
            if self.detail_truncated {
                ui.label(
                    RichText::new("以下为截断预览；复制完整值不受预览长度影响。")
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
            ui.add_space(6.0);
            egui::ScrollArea::both()
                .id_salt(("json-detail-preview-scroll", id))
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&self.detail_text).monospace())
                            .selectable(true)
                            .wrap(),
                    );
                });
        }
    }
}
