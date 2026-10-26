use super::*;
use eframe::egui::RichText;

const SAMPLE: &str = r#"{
  "name": "DevShop",
  "description": "让数据清晰一点",
  "tools": ["JSON", "JWT"],
  "local": true,
  "ready": 100
}"#;

impl App {
    pub(super) fn welcome(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let p = theme::palette(ctx);
        theme::heading(
            ui,
            "JSON 工作台",
            "浏览结构、查找内容，轻松整理每一段 JSON。",
            "JSON / 本地处理",
        );
        egui::ScrollArea::vertical()
            .id_salt("welcome-scroll")
            .show(ui, |ui| {
                ui.add_space((ui.available_height() * 0.04).clamp(8.0, 28.0));
                ui.vertical_centered(|ui| {
                    crate::branding::show_icon(ui, 74.0);
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("把复杂数据，变得井井有条。")
                            .size(28.0)
                            .strong(),
                    );
                    ui.add_space(5.0);
                    ui.label(
                        RichText::new("打开文件，或粘贴一段 JSON，从这里开始。 ").color(p.muted),
                    );
                    ui.add_space(22.0);

                    let width = ui.available_width().min(680.0);
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                            egui::pos2(ui.max_rect().center().x - width / 2.0, ui.cursor().top()),
                            egui::vec2(width, 0.0),
                        )),
                        |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.horizontal(|ui| {
                                    ui.add_space(((width - 352.0) / 2.0).max(0.0));
                                    if ui
                                        .add_sized([168.0, 42.0], theme::primary("打开 JSON 文件"))
                                        .on_hover_text("Ctrl / ⌘ + O")
                                        .clicked()
                                    {
                                        self.open_dialog(ctx);
                                    }
                                    if ui
                                        .add_sized([168.0, 42.0], egui::Button::new("从剪贴板读取"))
                                        .on_hover_text("Ctrl / ⌘ + Shift + V")
                                        .clicked()
                                    {
                                        self.request_clipboard(ctx);
                                    }
                                });
                                ui.add_space(6.0);
                                ui.vertical_centered(|ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("直接输入 JSON  →")
                                                    .size(12.0)
                                                    .color(p.accent),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        self.json.begin_edit();
                                    }
                                });
                                ui.add_space(22.0);
                                theme::card(ui).show(ui, |ui| {
                                    ui.set_width((width - 32.0).max(100.0));
                                    ui.horizontal(|ui| {
                                        theme::badge(ui, "JSON");
                                        ui.strong("一个小示例");
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    RichText::new("hello-devshop.json")
                                                        .monospace()
                                                        .size(11.0)
                                                        .color(p.muted),
                                                );
                                            },
                                        );
                                    });
                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                    egui::Grid::new("welcome-code")
                                        .min_row_height(20.0)
                                        .spacing([18.0, 3.0])
                                        .show(ui, |ui| {
                                            for (index, line) in SAMPLE.lines().enumerate() {
                                                ui.label(
                                                    RichText::new(format!("{:>2}", index + 1))
                                                        .monospace()
                                                        .size(13.0)
                                                        .color(p.muted),
                                                );
                                                let mut job = egui::text::LayoutJob::default();
                                                if let Some((key, value)) = line.split_once(':') {
                                                    job.append(
                                                        key,
                                                        0.0,
                                                        egui::TextFormat::simple(
                                                            egui::FontId::monospace(14.0),
                                                            p.accent,
                                                        ),
                                                    );
                                                    job.append(
                                                        ":",
                                                        0.0,
                                                        egui::TextFormat::simple(
                                                            egui::FontId::monospace(14.0),
                                                            p.muted,
                                                        ),
                                                    );
                                                    let color = if value.contains('"') {
                                                        p.string
                                                    } else {
                                                        p.number
                                                    };
                                                    job.append(
                                                        value,
                                                        0.0,
                                                        egui::TextFormat::simple(
                                                            egui::FontId::monospace(14.0),
                                                            color,
                                                        ),
                                                    );
                                                } else {
                                                    job.append(
                                                        line,
                                                        0.0,
                                                        egui::TextFormat::simple(
                                                            egui::FontId::monospace(14.0),
                                                            p.muted,
                                                        ),
                                                    );
                                                }
                                                ui.label(job);
                                                ui.end_row();
                                            }
                                        });
                                    ui.add_space(10.0);
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new("试试树形浏览、搜索和节点编辑")
                                                .size(12.0)
                                                .color(p.muted),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            RichText::new("载入示例  →")
                                                                .color(p.accent),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .clicked()
                                                {
                                                    self.request_action(
                                                        PendingAction::Install {
                                                            kind: ContentKind::Json,
                                                            text: SAMPLE.into(),
                                                            meta: DocumentMeta {
                                                                origin: "内置 JSON 示例".into(),
                                                                ..Default::default()
                                                            },
                                                            dirty: true,
                                                        },
                                                        ctx,
                                                    );
                                                }
                                            },
                                        );
                                    });
                                });
                                ui.add_space(18.0);
                                ui.columns(3, |columns| {
                                    for (column, (title, description)) in columns.iter_mut().zip([
                                        ("{ }  结构一目了然", "树形浏览与原文自由切换"),
                                        ("Aa  快速定位", "搜索字段、值和正则表达式"),
                                        ("↺  随时找回", "处理历史自动保存在本机"),
                                    ]) {
                                        column.vertical_centered(|ui| {
                                            ui.label(RichText::new(title).size(13.0).strong());
                                            ui.label(
                                                RichText::new(description)
                                                    .size(11.0)
                                                    .color(p.muted),
                                            );
                                        });
                                    }
                                });
                            });
                        },
                    );
                });
            });
    }
}
