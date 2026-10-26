//! Shared colors and surfaces, drawn from the red panda's orange and cream coat.
use eframe::egui::{self, Color32, RichText, Stroke};

pub(super) const BRAND: Color32 = Color32::from_rgb(192, 69, 25);

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub sidebar: Color32,
    pub subtle: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub hover: Color32,
    pub string: Color32,
    pub number: Color32,
    pub literal: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
}

pub(super) fn palette(ctx: &egui::Context) -> Palette {
    if ctx.style().visuals.dark_mode {
        Palette {
            background: Color32::from_rgb(28, 24, 22),
            surface: Color32::from_rgb(36, 31, 28),
            sidebar: Color32::from_rgb(32, 27, 24),
            subtle: Color32::from_rgb(44, 37, 32),
            border: Color32::from_rgb(68, 55, 47),
            text: Color32::from_rgb(244, 231, 218),
            muted: Color32::from_rgb(180, 158, 143),
            accent: Color32::from_rgb(255, 161, 112),
            accent_soft: Color32::from_rgb(78, 43, 29),
            hover: Color32::from_rgb(56, 44, 36),
            string: Color32::from_rgb(159, 199, 139),
            number: Color32::from_rgb(239, 184, 111),
            literal: Color32::from_rgb(196, 165, 222),
            success: Color32::from_rgb(149, 196, 149),
            warning: Color32::from_rgb(236, 186, 111),
            danger: Color32::from_rgb(255, 153, 143),
            danger_soft: Color32::from_rgb(69, 36, 33),
        }
    } else {
        Palette {
            background: Color32::from_rgb(249, 246, 241),
            surface: Color32::from_rgb(255, 253, 249),
            sidebar: Color32::from_rgb(243, 237, 228),
            subtle: Color32::from_rgb(248, 243, 236),
            border: Color32::from_rgb(228, 216, 203),
            text: Color32::from_rgb(59, 43, 34),
            muted: Color32::from_rgb(130, 110, 94),
            accent: BRAND,
            accent_soft: Color32::from_rgb(252, 230, 210),
            hover: Color32::from_rgb(242, 232, 220),
            string: Color32::from_rgb(77, 112, 65),
            number: Color32::from_rgb(161, 92, 28),
            literal: Color32::from_rgb(132, 85, 152),
            success: Color32::from_rgb(74, 114, 74),
            warning: Color32::from_rgb(143, 94, 23),
            danger: Color32::from_rgb(177, 51, 44),
            danger_soft: Color32::from_rgb(255, 235, 227),
        }
    }
}

pub(super) fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    // Reset from the theme defaults so switching appearances cannot retain colors.
    ctx.set_visuals(if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
    let p = palette(ctx);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.window_margin = egui::Margin::same(16);
    for (kind, font) in [
        (egui::TextStyle::Heading, egui::FontId::proportional(22.0)),
        (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
        (egui::TextStyle::Button, egui::FontId::proportional(13.0)),
        (egui::TextStyle::Small, egui::FontId::proportional(12.0)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(14.0)),
    ] {
        style.text_styles.insert(kind, font);
    }
    let v = &mut style.visuals;
    v.panel_fill = p.background;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface;
    v.text_edit_bg_color = Some(p.surface);
    v.faint_bg_color = p.subtle;
    v.code_bg_color = p.subtle;
    v.weak_text_color = Some(p.muted);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = 12.into();
    v.menu_corner_radius = 10.into();
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = 7.into();
        widget.bg_fill = p.subtle;
        widget.weak_bg_fill = p.subtle;
        widget.bg_stroke = Stroke::new(1.0, p.border);
        widget.fg_stroke = Stroke::new(1.0, p.text);
        widget.expansion = 0.0;
    }
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.weak_bg_fill = p.hover;
    v.widgets.hovered.bg_stroke.color = p.accent;
    v.widgets.active.bg_fill = p.accent_soft;
    v.widgets.active.weak_bg_fill = p.accent_soft;
    v.widgets.active.bg_stroke.color = p.accent;
    v.widgets.open = v.widgets.active;
    ctx.set_style(style);
}

pub(super) fn card(ui: &egui::Ui) -> egui::Frame {
    let p = palette(ui.ctx());
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(16)
}

pub(super) fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
        .fill(BRAND)
        .stroke(Stroke::NONE)
        .min_size(egui::vec2(0.0, 34.0))
}

pub(super) fn badge(ui: &mut egui::Ui, label: &str) {
    let p = palette(ui.ctx());
    let galley =
        ui.painter()
            .layout_no_wrap(label.into(), egui::FontId::proportional(11.0), p.accent);
    let (rect, _) =
        ui.allocate_exact_size(galley.size() + egui::vec2(14.0, 8.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 5, p.accent_soft);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, p.accent);
}

pub(super) fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str, badge: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.heading(RichText::new(title).strong());
            ui.label(RichText::new(subtitle).size(12.0).weak());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            self::badge(ui, badge);
        });
    });
    ui.add_space(12.0);
}
