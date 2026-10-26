use super::*;
use actions::kind_name;
use eframe::egui::RichText;

impl App {
    pub(super) fn chrome(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("devshop-title")
            .exact_height(window_chrome::TITLE_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(window_chrome::title_fill(ctx))
                    .inner_margin(egui::Margin::symmetric(6, 0)),
            )
            .show(ctx, |ui| {
                let response = ui.interact(
                    ui.max_rect(),
                    ui.id().with("drag-title"),
                    egui::Sense::click_and_drag(),
                );
                if response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if response.double_clicked() {
                    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal_centered(|ui| {
                    ui.add_space(window_chrome::leading_inset() + 8.0);
                    crate::branding::show_icon(ui, 28.0);
                    ui.label(
                        RichText::new("DevShop")
                            .size(17.0)
                            .strong()
                            .color(ui.visuals().text_color()),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("开发者的本地数据工坊")
                            .size(12.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        window_chrome::controls(ui)
                    });
                });
            });
    }

    pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
        let p = theme::palette(ctx);
        egui::TopBottomPanel::top("devshop-toolbar")
            .frame(
                egui::Frame::new()
                    .fill(p.surface)
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (kind, label) in [
                        (ContentKind::Json, "{ }  JSON 工具"),
                        (ContentKind::Jwt, "JWT  令牌工具"),
                    ] {
                        if ui
                            .selectable_label(
                                self.active == kind,
                                RichText::new(label).strong().size(14.0),
                            )
                            .clicked()
                        {
                            self.active = kind;
                        }
                    }
                    ui.add_space(10.0);
                    ui.separator();
                    if ui.button("新建").on_hover_text("Ctrl / ⌘ + N").clicked() {
                        self.request_action(PendingAction::New(self.active), ctx);
                    }
                    if ui
                        .add(theme::primary("打开文件"))
                        .on_hover_text("Ctrl / ⌘ + O · 支持拖入文件")
                        .clicked()
                    {
                        self.open_dialog(ctx);
                    }
                    if ui
                        .button("读取剪贴板")
                        .on_hover_text("Ctrl / ⌘ + Shift + V · 自动识别 JSON / JWT")
                        .clicked()
                    {
                        self.request_clipboard(ctx);
                    }
                    let can_save = !self.saving
                        && !self.source(self.active).is_empty()
                        && match self.active {
                            ContentKind::Json => !self.json.has_node_draft(),
                            ContentKind::Jwt => self.jwt.can_save(),
                        };
                    if ui
                        .add_enabled(can_save, egui::Button::new("保存"))
                        .on_hover_text("Ctrl / ⌘ + S")
                        .clicked()
                    {
                        self.save_current(false, false, ctx);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("设置", |ui| {
                            if ui.checkbox(&mut self.settings.dark, "深色外观").changed() {
                                self.apply_theme(ctx);
                            }
                            ui.checkbox(&mut self.floating.enabled, "显示置顶悬浮球");
                            ui.checkbox(&mut self.settings.history_enabled, "自动记录处理历史");
                            ui.separator();
                            ui.label(RichText::new("历史保存在本机；JWT 密钥不记录。").small());
                            if let Some(store) = &self.history_store {
                                ui.label(
                                    RichText::new(store.directory().display().to_string()).small(),
                                );
                            }
                            ui.separator();
                            if ui.button("完全退出 DevShop").clicked() {
                                ui.close();
                                self.request_action(PendingAction::Exit, ctx);
                            }
                        });
                        ui.toggle_value(&mut self.settings.show_history, "历史")
                            .on_hover_text("Ctrl / ⌘ + H");
                        if ui
                            .add_enabled(can_save, egui::Button::new("另存为…"))
                            .on_hover_text("Ctrl / ⌘ + Shift + S")
                            .clicked()
                        {
                            self.save_current(true, false, ctx);
                        }
                    });
                });
            });
    }

    pub(super) fn status_bar(&mut self, ctx: &egui::Context) {
        let p = theme::palette(ctx);
        egui::TopBottomPanel::bottom("devshop-status")
            .exact_height(32.0)
            .frame(
                egui::Frame::new()
                    .fill(p.sidebar)
                    .inner_margin(egui::Margin::symmetric(18, 0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.saving || self.file_loading || self.json.busy() {
                        ui.spinner();
                    } else {
                        ui.label(RichText::new("●").color(p.success).size(9.0));
                    }
                    ui.label(RichText::new(&self.status).size(11.0))
                        .on_hover_text(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.is_dirty(self.active) {
                            ui.colored_label(p.accent, RichText::new("● 未保存").size(11.0));
                        } else {
                            ui.weak(RichText::new("无未保存修改").size(11.0));
                        }
                        if let Some(path) = &self.meta(self.active).path {
                            ui.label(
                                RichText::new(
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .size(11.0),
                            );
                        } else {
                            ui.weak(
                                RichText::new(format!(
                                    "未命名.{}",
                                    if self.active == ContentKind::Json {
                                        "json"
                                    } else {
                                        "jwt"
                                    }
                                ))
                                .size(11.0),
                            );
                        }
                    });
                });
            });
    }

    pub(super) fn history_panel(&mut self, ctx: &egui::Context) {
        if !self.settings.show_history {
            return;
        }
        let p = theme::palette(ctx);
        egui::SidePanel::left("devshop-history")
            .resizable(true)
            .default_width(248.0)
            .width_range(210.0..=380.0)
            .frame(egui::Frame::new().fill(p.sidebar).inner_margin(16))
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("处理历史").size(16.0).strong());
                    theme::badge(ui, &self.entries.len().to_string());
                });
                ui.label(
                    RichText::new("每一次处理，都有迹可循。")
                        .small()
                        .color(p.muted),
                );
                ui.add_space(12.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.history_filter)
                        .hint_text("搜索历史记录…")
                        .margin(egui::Margin::symmetric(10, 8))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let can_record = !self.source(self.active).is_empty()
                        && match self.active {
                            ContentKind::Json => !self.json.has_node_draft(),
                            ContentKind::Jwt => !self.jwt.has_editor_draft(),
                        };
                    if ui
                        .add_enabled(can_record, egui::Button::new("记录当前内容").small())
                        .clicked()
                    {
                        self.record(self.active, self.source(self.active), "手动保存历史");
                    }
                    if ui
                        .add(egui::Button::new("清理…").small().frame(false))
                        .clicked()
                    {
                        self.clear_history_confirm = true;
                    }
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                let query = self.history_filter.to_lowercase();
                let entries: Vec<_> = self
                    .entries
                    .iter()
                    .filter(|entry| {
                        query.is_empty()
                            || entry.title.to_lowercase().contains(&query)
                            || entry.origin.to_lowercase().contains(&query)
                            || entry.operation.to_lowercase().contains(&query)
                    })
                    .cloned()
                    .collect();
                let height = (ui.available_height() - 34.0).max(80.0);
                egui::ScrollArea::vertical()
                    .id_salt("history-scroll")
                    .auto_shrink([false, false])
                    .max_height(height)
                    .show_rows(ui, 48.0, entries.len(), |ui, rows| {
                        if entries.is_empty() {
                            ui.add_space(32.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("↺").size(30.0).color(p.muted));
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(if self.entries.is_empty() {
                                        "还没有处理记录"
                                    } else {
                                        "没有找到匹配记录"
                                    })
                                    .strong(),
                                );
                                ui.label(
                                    RichText::new(if self.entries.is_empty() {
                                        "打开或解析内容后，\n历史记录会自动出现在这里。"
                                    } else {
                                        "换个关键词试试。"
                                    })
                                    .small()
                                    .color(p.muted),
                                );
                            });
                        }
                        for index in rows {
                            let entry = &entries[index];
                            ui.push_id(&entry.id, |ui| {
                                let parent_exists = entry
                                    .parent_id
                                    .as_ref()
                                    .is_some_and(|id| self.entries.iter().any(|e| &e.id == id));
                                let current = self.meta(entry.kind).parent_id.as_deref()
                                    == Some(entry.id.as_str());
                                let frame = egui::Frame::new()
                                    .fill(if current { p.accent_soft } else { p.surface })
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        if current {
                                            p.accent.gamma_multiply(0.4)
                                        } else {
                                            p.border
                                        },
                                    ))
                                    .corner_radius(7)
                                    .inner_margin(egui::Margin::symmetric(8, 6));
                                let response = frame
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.set_min_height(36.0);
                                        ui.spacing_mut().interact_size.y = 0.0;
                                        ui.spacing_mut().item_spacing.y = 3.0;
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(kind_name(entry.kind))
                                                    .size(10.0)
                                                    .strong()
                                                    .color(p.accent),
                                            );
                                            if entry.pinned {
                                                ui.label(
                                                    RichText::new("★").size(11.0).color(p.accent),
                                                );
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(&entry.title).size(13.0).strong(),
                                                )
                                                .truncate(),
                                            );
                                        });
                                        ui.horizontal(|ui| {
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        RichText::new(history_time(
                                                            entry.created_ms,
                                                        ))
                                                        .size(10.0)
                                                        .color(p.muted),
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(format!(
                                                                        "{} · {}",
                                                                        short_text(
                                                                            &entry.operation,
                                                                            12
                                                                        ),
                                                                        human_bytes(entry.bytes)
                                                                    ))
                                                                    .size(11.0)
                                                                    .color(p.muted),
                                                                )
                                                                .truncate(),
                                                            );
                                                        },
                                                    );
                                                },
                                            );
                                        });
                                    })
                                    .response;
                                let response = ui.interact(
                                    response.rect,
                                    ui.id().with("open-history"),
                                    egui::Sense::click(),
                                );
                                if response.hovered() {
                                    ui.painter().rect_stroke(
                                        response.rect,
                                        7,
                                        egui::Stroke::new(1.0, p.accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if response.clicked() {
                                    self.open_history(entry.clone(), ctx);
                                }
                                response
                                    .on_hover_text(format!(
                                        "来源：{}\n操作：{}\n右键可收藏、删除或回溯源记录",
                                        entry.origin, entry.operation
                                    ))
                                    .context_menu(|ui| {
                                        if ui.button("打开此记录").clicked() {
                                            self.open_history(entry.clone(), ctx);
                                            ui.close();
                                        }
                                        if ui
                                            .button(if entry.pinned {
                                                "取消收藏"
                                            } else {
                                                "收藏此记录"
                                            })
                                            .clicked()
                                        {
                                            let _ = self.history.sender.send(HistoryCommand::Pin(
                                                entry.id.clone(),
                                                !entry.pinned,
                                            ));
                                            ui.close();
                                        }
                                        if ui
                                            .add_enabled(
                                                parent_exists,
                                                egui::Button::new("回溯上一条来源记录"),
                                            )
                                            .clicked()
                                        {
                                            if let Some(parent) = self
                                                .entries
                                                .iter()
                                                .find(|e| Some(&e.id) == entry.parent_id.as_ref())
                                                .cloned()
                                            {
                                                self.open_history(parent, ctx);
                                            }
                                            ui.close();
                                        }
                                        ui.separator();
                                        if ui.button("删除此记录").clicked() {
                                            let _ = self
                                                .history
                                                .sender
                                                .send(HistoryCommand::Remove(entry.id.clone()));
                                            ui.close();
                                        }
                                    });
                            });
                        }
                    });
                ui.add_space(10.0);
                ui.label(
                    RichText::new("●  内容仅保存在这台设备")
                        .size(11.0)
                        .color(p.muted),
                );
            });
    }

    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(kind) = self.confirm_kind {
            egui::Modal::new(egui::Id::new("unsaved-document")).show(ctx, |ui| {
                ui.set_width(390.0);
                ui.heading("保留当前修改？");
                ui.add_space(8.0);
                ui.label(format!(
                    "{} 工作区有未保存内容。保存完成后再继续操作。",
                    kind_name(kind)
                ));
                if kind == ContentKind::Jwt && self.jwt.has_editor_draft() {
                    ui.colored_label(
                        theme::palette(ui.ctx()).accent,
                        "Header / Payload 已编辑，请先取消本次操作并生成签名，或明确放弃这些修改。",
                    );
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.saving, egui::Button::new("保存后继续"))
                        .clicked()
                    {
                        self.save_current(false, true, ctx);
                    }
                    if ui
                        .add_enabled(!self.saving, egui::Button::new("放弃修改并继续"))
                        .clicked()
                    {
                        self.discard_and_continue(ctx);
                    }
                    if ui
                        .add_enabled(!self.saving, egui::Button::new("取消"))
                        .clicked()
                    {
                        self.confirm_kind = None;
                        self.pending = None;
                    }
                    if self.saving {
                        ui.spinner();
                    }
                });
            });
        }
        if self.clear_history_confirm {
            egui::Modal::new(egui::Id::new("clear-history")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading("清理历史记录？");
                ui.label("将删除未收藏的本地历史快照，已保存的文档文件不受影响。");
                ui.horizontal(|ui| {
                    if ui.button("清理未收藏记录").clicked() {
                        let _ = self.history.sender.send(HistoryCommand::Clear);
                        self.clear_history_confirm = false;
                    }
                    if ui.button("取消").clicked() {
                        self.clear_history_confirm = false;
                    }
                });
            });
        }
    }
}

fn short_text(text: &str, limit: usize) -> String {
    let mut result: String = text.chars().take(limit).collect();
    if text.chars().nth(limit).is_some() {
        result.push('…');
    }
    result
}
fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
fn history_time(milliseconds: u64) -> String {
    let offset = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    chrono::DateTime::from_timestamp_millis(milliseconds.min(i64::MAX as u64) as i64)
        .map(|time| {
            time.with_timezone(&offset)
                .format("%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
