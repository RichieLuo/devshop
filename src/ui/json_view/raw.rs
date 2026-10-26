use super::*;

impl JsonWorkspace {
    pub(super) fn show_source(&mut self, ui: &mut egui::Ui) {
        if self.source.len() <= EDIT_LIMIT {
            let editable = !self.has_node_draft();
            ui.horizontal(|ui| {
                ui.label(RichText::new("JSON 原文").strong());
                ui.label(
                    RichText::new("编辑后自动解析；原始数字和字段顺序保留")
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            });
            ui.separator();
            if !editable {
                ui.colored_label(
                    theme::palette(ui.ctx()).accent,
                    "节点修改尚未应用。请返回树形视图，应用修改或取消编辑后再编辑原文。",
                );
            }
            let editor_size = ui.available_size().max(egui::vec2(100.0, 160.0));
            let response = egui::ScrollArea::both()
                .id_salt("json-source-edit-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_sized(
                        editor_size,
                        egui::TextEdit::multiline(&mut self.edit_text)
                            .id(egui::Id::new("json-source-editor"))
                            .code_editor()
                            .interactive(editable)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("在这里粘贴 JSON，或使用上方的“打开”“剪贴板”"),
                    )
                })
                .inner;
            if response.changed() {
                self.source_edited();
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("大文件原文").strong());
                ui.label(
                    RichText::new(
                        "仅绘制可见行。单行预览最多 2,000 字符；可在树形视图编辑选中节点。",
                    )
                    .small()
                    .color(theme::palette(ui.ctx()).muted),
                );
            });
            ui.separator();
            if self.lines.len() >= 2_000_000 {
                ui.label(
                    RichText::new("原文预览最多显示前 200 万行；树形浏览和搜索仍覆盖整个文档。")
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
            if self.work.is_some() && self.lines.len() == 1 {
                ui.spinner();
                ui.label("正在建立行索引…");
                return;
            }
            let row_height = 21.0;
            let mut scroll = egui::ScrollArea::both()
                .id_salt("json-large-source-scroll")
                .auto_shrink([false, false]);
            if let Some(line) = self.source_scroll_line.take() {
                scroll = scroll.vertical_scroll_offset(
                    (line as f32 * (row_height + ui.spacing().item_spacing.y)
                        - ui.available_height() * 0.4)
                        .max(0.0),
                );
            }
            scroll.show_rows(ui, row_height, self.lines.len(), |ui, visible| {
                for index in visible {
                    let start = self.lines[index];
                    let end = self
                        .lines
                        .get(index + 1)
                        .copied()
                        .unwrap_or(self.source.len());
                    let bounded = json::truncate(&self.source[start..end], 2000);
                    let preview = bounded.lines().next().unwrap_or("");
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [58.0, row_height],
                            egui::Label::new(
                                RichText::new(format!("{}", index + 1))
                                    .monospace()
                                    .color(theme::palette(ui.ctx()).muted),
                            ),
                        );
                        ui.label(RichText::new(preview).monospace());
                    });
                }
            });
        }
    }
}
