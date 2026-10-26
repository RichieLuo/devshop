use super::theme;
use devshop::jwt::{self, AlgorithmChoice, DecodedJwt, KeyEncoding, TimeStatus};
use eframe::egui::{self, RichText};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub enum JwtEvent {
    Decoded { token: String },
    Signed { token: String },
    SaveRequested { text: String },
    RecordRequested { token: String },
    ReplaceRequested { token: String, origin: String },
    NewRequested,
}

#[derive(Default)]
enum Verification {
    #[default]
    Unverified,
    Verified,
    Mismatch,
    Error(String),
}

/// Session-only JWT state. This type deliberately implements no serialization:
/// the key must never be included in history or restored on the next launch.
pub struct JwtWorkspace {
    token: String,
    header: String,
    payload: String,
    decoded: Option<DecodedJwt>,
    expected: AlgorithmChoice,
    key: String,
    key_encoding: KeyEncoding,
    show_key: bool,
    verification: Verification,
    error: Option<String>,
    editor_dirty: bool,
}

impl Default for JwtWorkspace {
    fn default() -> Self {
        Self {
            token: String::new(),
            header: "{\n  \"alg\": \"HS256\",\n  \"typ\": \"JWT\"\n}".into(),
            payload: "{\n  \"sub\": \"user-001\",\n  \"name\": \"示例用户\"\n}".into(),
            decoded: None,
            expected: AlgorithmChoice::default(),
            key: String::new(),
            key_encoding: KeyEncoding::Text,
            show_key: false,
            verification: Verification::Unverified,
            error: None,
            editor_dirty: false,
        }
    }
}

impl JwtWorkspace {
    pub fn set_token(&mut self, token: &str) {
        self.token = token.to_owned();
        // Replacing the workspace is coordinated by the outer application's
        // unsaved-changes flow. A previous draft must not leak into this token.
        self.editor_dirty = false;
        self.decode_input();
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Edited JSON must be signed before exporting a JWT; otherwise a save
    /// would silently retain the old payload and signature.
    pub fn can_save(&self) -> bool {
        self.decoded.is_some() && !self.editor_dirty
    }

    pub fn has_editor_draft(&self) -> bool {
        self.editor_dirty
    }

    /// Call only after the user explicitly discards the unsigned JSON draft.
    /// Restore the editors from the original token, then clear the dirty flag
    /// even if the raw token itself is incomplete or invalid.
    pub fn discard_editor_draft(&mut self) {
        self.decode_input();
        self.editor_dirty = false;
    }

    /// A .jwt export contains only the token. In particular, no signing key.
    pub fn save_text(&self) -> String {
        self.decoded
            .as_ref()
            .map(|decoded| decoded.token.clone())
            .unwrap_or_else(|| self.token.trim().to_owned())
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<JwtEvent> {
        let mut event = None;
        egui::ScrollArea::vertical()
            .id_salt("jwt-workspace-scroll")
            .show(ui, |ui| {
                theme::heading(
                    ui,
                    "JWT 工作台",
                    "本地解码、编辑签名与独立验签，让令牌信息清晰可读。",
                    "JWT / 本地处理",
                );
                ui.label(
                    RichText::new("解码不等于认证。密钥只保留在当前会话，不写入历史或文件。")
                        .small()
                        .weak(),
                );
                ui.add_space(10.0);
                self.token_panel(ui, &mut event);
                if let Some(error) = &self.error {
                    ui.colored_label(theme::palette(ui.ctx()).danger, error);
                }
                ui.add_space(10.0);
                self.editors(ui);
                ui.add_space(10.0);
                self.signature_panel(ui, &mut event);
                ui.add_space(10.0);
                self.claims_panel(ui);
            });
        event
    }

    fn token_panel(&mut self, ui: &mut egui::Ui, event: &mut Option<JwtEvent>) {
        theme::card(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Token");
                ui.label(
                    RichText::new(format!("{} 字节", self.token.len()))
                        .small()
                        .weak(),
                );
                if ui
                    .button("载入示例")
                    .on_hover_text("HS256 示例使用公开测试密钥 devshop-demo-key；不会修改当前密钥")
                    .clicked()
                {
                    match Self::sample_token() {
                        Ok(token) => {
                            *event = Some(JwtEvent::ReplaceRequested {
                                token,
                                origin: "内置 JWT 示例".into(),
                            })
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                if ui.button("清空").clicked() {
                    *event = Some(JwtEvent::NewRequested);
                }
            });
            let response = ui.add(
                egui::TextEdit::multiline(&mut self.token)
                    .id_salt("jwt-token-input")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(4)
                    .char_limit(jwt::MAX_TOKEN_BYTES)
                    .hint_text("粘贴 eyJ… JWT，也可直接粘贴 Bearer 令牌；Ctrl+Enter 解码"),
            );
            if response.changed() {
                self.decoded = None;
                self.verification = Verification::Unverified;
                self.error = None;
                self.editor_dirty = true;
            }
            let decode_shortcut = response.has_focus()
                && ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Enter));
            ui.horizontal_wrapped(|ui| {
                if ui.add(theme::primary("解码 Token")).clicked() || decode_shortcut {
                    self.decode_input();
                    if self.decoded.is_some() {
                        *event = Some(JwtEvent::Decoded {
                            token: self.save_text(),
                        });
                    }
                }
                if ui
                    .add_enabled(
                        !self.token.trim().is_empty(),
                        egui::Button::new("复制 Token"),
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(self.save_text());
                }
                if ui
                    .add_enabled(self.can_save(), egui::Button::new("加入历史"))
                    .clicked()
                {
                    *event = Some(JwtEvent::RecordRequested {
                        token: self.save_text(),
                    });
                }
                if ui
                    .add_enabled(self.can_save(), egui::Button::new("保存 .jwt"))
                    .clicked()
                {
                    *event = Some(JwtEvent::SaveRequested {
                        text: self.save_text(),
                    });
                }
                if let Some(decoded) = &self.decoded {
                    ui.label(
                        RichText::new(format!("Header: {}  ·  尚未认证", decoded.algorithm.name()))
                            .color(theme::palette(ui.ctx()).warning),
                    );
                }
            });
        });
    }

    fn editors(&mut self, ui: &mut egui::Ui) {
        if ui.available_width() >= 680.0 {
            ui.columns(2, |columns| {
                Self::json_editor(
                    &mut columns[0],
                    "Header",
                    "jwt-header-editor",
                    &mut self.header,
                    &mut self.editor_dirty,
                );
                Self::json_editor(
                    &mut columns[1],
                    "Payload",
                    "jwt-payload-editor",
                    &mut self.payload,
                    &mut self.editor_dirty,
                );
            });
        } else {
            Self::json_editor(
                ui,
                "Header",
                "jwt-header-editor",
                &mut self.header,
                &mut self.editor_dirty,
            );
            Self::json_editor(
                ui,
                "Payload",
                "jwt-payload-editor",
                &mut self.payload,
                &mut self.editor_dirty,
            );
        }
        if self.editor_dirty {
            self.verification = Verification::Unverified;
            ui.label(RichText::new("编辑内容尚未写入 Token。点击“从编辑内容生成并签名”；重新解码会载回 Token 原文。").color(theme::palette(ui.ctx()).warning).small());
        }
    }

    fn json_editor(ui: &mut egui::Ui, label: &str, id: &str, text: &mut String, dirty: &mut bool) {
        theme::card(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(label);
                if ui.small_button("复制 JSON").clicked() {
                    ui.ctx().copy_text(text.clone());
                }
            });
            egui::ScrollArea::both()
                .id_salt((id, "scroll"))
                .max_height(230.0)
                .show(ui, |ui| {
                    if ui
                        .add(
                            egui::TextEdit::multiline(text)
                                .id_salt(id)
                                .font(egui::TextStyle::Monospace)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(10)
                                .char_limit(jwt::MAX_TOKEN_BYTES),
                        )
                        .changed()
                    {
                        *dirty = true;
                    }
                });
        });
    }

    fn signature_panel(&mut self, ui: &mut egui::Ui, event: &mut Option<JwtEvent>) {
        theme::card(ui).show(ui, |ui| {
            ui.strong("签名与验签");
            ui.horizontal_wrapped(|ui| {
                ui.label("明确选择算法");
                let before = self.expected;
                egui::ComboBox::from_id_salt("jwt-expected-algorithm")
                    .selected_text(self.expected.name())
                    .show_ui(ui, |ui| {
                        for algorithm in AlgorithmChoice::ALL {
                            ui.selectable_value(&mut self.expected, algorithm, algorithm.name());
                        }
                    });
                if self.expected != before {
                    self.verification = Verification::Unverified;
                }
                if self.expected.is_hmac() {
                    ui.label("共享密钥格式");
                    let old = self.key_encoding;
                    egui::ComboBox::from_id_salt("jwt-key-encoding")
                        .selected_text(if self.key_encoding == KeyEncoding::Text {
                            "UTF-8 文本"
                        } else {
                            "Base64"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.key_encoding,
                                KeyEncoding::Text,
                                "UTF-8 文本",
                            );
                            ui.selectable_value(
                                &mut self.key_encoding,
                                KeyEncoding::Base64,
                                "Base64",
                            );
                        });
                    if old != self.key_encoding {
                        self.verification = Verification::Unverified;
                    }
                }
                ui.checkbox(&mut self.show_key, "显示密钥");
                if ui.small_button("清除密钥").clicked() {
                    self.key.clear();
                    self.verification = Verification::Unverified;
                }
            });
            let hint = if self.expected.is_hmac() {
                "共享密钥（不会保存）"
            } else {
                "验签粘贴公钥 PEM；签名粘贴私钥 PEM（不会保存）"
            };
            let key_editor = egui::TextEdit::multiline(&mut self.key)
                .id_salt("jwt-session-key")
                .password(!self.show_key)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(if self.expected.is_hmac() { 1 } else { 3 })
                .char_limit(128 * 1024)
                .hint_text(hint);
            let mut key_output = key_editor.show(ui);
            if key_output.response.changed() {
                self.verification = Verification::Unverified;
            }
            // Clearing a key also clears its undo history: Ctrl+Z must not
            // restore a secret the user just removed.
            key_output.state.clear_undoer();
            key_output.state.store(ui.ctx(), key_output.response.id);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.decoded.is_some() && !self.editor_dirty,
                        egui::Button::new("验证原始 Token 签名"),
                    )
                    .clicked()
                {
                    self.verification = match jwt::verify_signature(
                        &self.token,
                        self.expected,
                        &self.key,
                        self.key_encoding,
                    ) {
                        Ok(true) => Verification::Verified,
                        Ok(false) => Verification::Mismatch,
                        Err(error) => Verification::Error(error.to_string()),
                    };
                }
                if ui.add(theme::primary("从编辑内容生成并签名")).clicked() {
                    match jwt::sign(
                        &self.header,
                        &self.payload,
                        self.expected,
                        &self.key,
                        self.key_encoding,
                    ) {
                        Ok(token) => {
                            self.set_token(&token);
                            *event = Some(JwtEvent::Signed { token });
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            });
            ui.add_space(4.0);
            match &self.verification {
                Verification::Unverified => {
                    ui.colored_label(
                        theme::palette(ui.ctx()).warning,
                        "未验签 · 解码内容尚未认证",
                    );
                }
                Verification::Verified => {
                    ui.colored_label(
                        theme::palette(ui.ctx()).success,
                        "签名验证通过 · 时间、签发者、接收者等业务条件需另行判断",
                    );
                }
                Verification::Mismatch => {
                    ui.colored_label(
                        theme::palette(ui.ctx()).danger,
                        "签名不匹配 · 密钥错误或令牌内容已被修改",
                    );
                }
                Verification::Error(error) => {
                    ui.colored_label(theme::palette(ui.ctx()).danger, error);
                }
            }
        });
    }

    fn claims_panel(&self, ui: &mut egui::Ui) {
        theme::card(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong("时间字段解读");
            ui.label(
                RichText::new("按本机当前时间、UTC 秒级时间戳解释；这里的状态与签名结果相互独立。")
                    .small()
                    .weak(),
            );
            let Some(decoded) = &self.decoded else {
                ui.label(RichText::new("解码后显示 exp、nbf 和 iat。").weak());
                return;
            };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let claims = jwt::claim_times(&decoded.payload, now);
            if claims.is_empty() {
                ui.label(
                    RichText::new("Payload 未包含 exp、nbf、iat；这不表示令牌已通过业务校验。")
                        .weak(),
                );
            }
            egui::Grid::new("jwt-time-claims")
                .num_columns(4)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    for claim in claims {
                        ui.monospace(claim.name);
                        ui.monospace(claim.value);
                        ui.monospace(claim.utc);
                        let color = match claim.status {
                            TimeStatus::Expired | TimeStatus::Invalid => {
                                theme::palette(ui.ctx()).danger
                            }
                            TimeStatus::NotYetValid | TimeStatus::IssuedInFuture => {
                                theme::palette(ui.ctx()).warning
                            }
                            _ => theme::palette(ui.ctx()).success,
                        };
                        ui.colored_label(color, claim.status.label());
                        ui.end_row();
                    }
                });
        });
    }

    fn decode_input(&mut self) {
        self.verification = Verification::Unverified;
        match jwt::decode(&self.token) {
            Ok(decoded) => {
                self.token = decoded.token.clone();
                self.header = decoded.header_pretty.clone();
                self.payload = decoded.payload_pretty.clone();
                self.decoded = Some(decoded);
                self.editor_dirty = false;
                self.error = None;
            }
            Err(error) => {
                self.decoded = None;
                self.error = Some(error.to_string());
            }
        }
    }

    fn sample_token() -> Result<String, jwt::JwtError> {
        let header = r#"{"alg":"HS256","typ":"JWT"}"#;
        let payload =
            r#"{"sub":"demo-user","name":"仅供测试的示例用户","iat":1700000000,"exp":4102444800}"#;
        jwt::sign(
            header,
            payload,
            AlgorithmChoice::Hs256,
            "devshop-demo-key",
            KeyEncoding::Text,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discarding_unsigned_edits_restores_the_original_token_payload() {
        let token = jwt::sign(
            r#"{"alg":"HS256"}"#,
            r#"{"sub":"original"}"#,
            AlgorithmChoice::Hs256,
            "synthetic-test-key",
            KeyEncoding::Text,
        )
        .unwrap();
        let mut workspace = JwtWorkspace::default();
        workspace.set_token(&token);
        workspace.payload = r#"{"sub":"unsigned-edit"}"#.into();
        workspace.editor_dirty = true;
        assert!(workspace.has_editor_draft());
        assert!(!workspace.can_save());
        workspace.discard_editor_draft();
        assert!(!workspace.has_editor_draft());
        assert!(workspace.can_save());
        assert_eq!(workspace.token(), token);
        assert!(workspace.payload.contains("original"));
        assert!(!workspace.payload.contains("unsigned-edit"));
    }

    #[test]
    fn replacing_or_discarding_an_invalid_token_does_not_keep_a_previous_draft_dirty() {
        let mut workspace = JwtWorkspace {
            editor_dirty: true,
            ..Default::default()
        };
        workspace.set_token("invalid-token");
        assert!(!workspace.has_editor_draft());
        assert!(!workspace.can_save());
        workspace.editor_dirty = true;
        workspace.discard_editor_draft();
        assert!(!workspace.has_editor_draft());
        assert!(!workspace.can_save());
    }
}
