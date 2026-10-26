//! The desktop quick entry owns no documents or history; all actions go to App.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use egui::emath::GuiRounding;
use std::sync::{Arc, Mutex, mpsc};

#[cfg(target_os = "windows")]
mod windows;

const BALL_SIZE: f32 = 64.0;
const MENU_SIZE: Vec2 = Vec2::new(232.0, 278.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Open,
    Clipboard,
    History,
    Hide,
    Exit,
}

#[derive(Clone, Copy, Debug)]
struct MenuPlacement {
    origin: [f32; 2],
    ball_offset: Vec2,
}

#[derive(Default)]
struct Geometry {
    // Always the desktop position of the 64px ball, never the expanded menu.
    anchor: Option<[f32; 2]>,
    menu: Option<MenuPlacement>,
    restoring: bool,
}

impl Geometry {
    fn observe(&mut self, rect: Rect) {
        if self.menu.is_some()
            || (rect.width() - BALL_SIZE).abs() > 2.0
            || (rect.height() - BALL_SIZE).abs() > 2.0
        {
            return;
        }
        if self.restoring {
            // Move and resize events can arrive separately after menu closure.
            if let Some(anchor) = self.anchor
                && rect.min.distance(Pos2::from(anchor)) > 2.0
            {
                return;
            }
            self.restoring = false;
        }
        self.anchor = Some([rect.min.x, rect.min.y]);
    }

    fn origin(&self) -> Option<[f32; 2]> {
        self.menu.map(|menu| menu.origin).or(self.anchor)
    }
}

pub struct FloatingBall {
    pub enabled: bool,
    sender: mpsc::Sender<Action>,
    receiver: mpsc::Receiver<Action>,
    geometry: Arc<Mutex<Geometry>>,
}

impl Default for FloatingBall {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            enabled: true,
            sender,
            receiver,
            geometry: Arc::new(Mutex::new(Geometry::default())),
        }
    }
}

impl FloatingBall {
    pub fn position(&self) -> Option<[f32; 2]> {
        self.geometry.lock().ok().and_then(|state| state.anchor)
    }

    pub fn set_position(&mut self, position: Option<[f32; 2]>) {
        if let Ok(mut state) = self.geometry.lock() {
            state.anchor = position.filter(|point| point.iter().all(|value| value.is_finite()));
            state.menu = None;
            state.restoring = state.anchor.is_some();
        }
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        self.receiver.try_iter().collect()
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            if let Ok(mut state) = self.geometry.lock() {
                state.menu = None;
                state.restoring = state.anchor.is_some();
            }
            return;
        }
        let sender = self.sender.clone();
        let geometry = self.geometry.clone();
        let (origin, expanded) = self
            .geometry
            .lock()
            .map(|state| (state.origin(), state.menu.is_some()))
            .unwrap_or((None, false));
        let size = if expanded {
            MENU_SIZE
        } else {
            Vec2::splat(BALL_SIZE)
        };
        let mut builder = egui::ViewportBuilder::default()
            .with_title("DevShop 快捷入口")
            .with_inner_size(size)
            .with_min_inner_size([BALL_SIZE, BALL_SIZE])
            .with_max_inner_size(MENU_SIZE)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_active(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        if let Some(origin) = origin {
            builder = builder.with_position(origin);
        }
        ctx.show_viewport_deferred(
            egui::ViewportId::from_hash_of("devshop-floating-ball"),
            builder,
            move |ctx, class| {
                if let Some(rect) = ctx.input(|input| input.viewport().outer_rect)
                    && let Ok(mut state) = geometry.lock()
                {
                    state.observe(rect);
                }
                if ctx.input(|input| {
                    input.key_pressed(egui::Key::Escape)
                        || input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::WindowFocused(false)))
                }) {
                    set_menu(ctx, &geometry, false);
                }
                if ctx.input(|input| input.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    set_menu(ctx, &geometry, false);
                    dispatch(ctx, &sender, Action::Hide);
                }
                if class == egui::ViewportClass::Embedded {
                    egui::Window::new("DevShop 快捷入口")
                        .resizable(false)
                        .show(ctx, |ui| ball(ui, &sender, &geometry));
                } else {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                        .show(ctx, |ui| ball(ui, &sender, &geometry));
                }
            },
        );
    }
}

fn dispatch(ctx: &egui::Context, sender: &mpsc::Sender<Action>, action: Action) {
    if action != Action::Hide {
        for command in [
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Minimized(false),
            egui::ViewportCommand::Focus,
        ] {
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, command);
        }
    }
    let _ = sender.send(action);
    ctx.request_repaint_of(egui::ViewportId::ROOT);
}

fn menu_placement(anchor: [f32; 2], work: Rect) -> MenuPlacement {
    let delta = MENU_SIZE - Vec2::splat(BALL_SIZE);
    let position = Pos2::from(anchor);
    let preferred = Pos2::new(
        if position.x - delta.x >= work.left() {
            position.x - delta.x
        } else {
            position.x
        },
        if position.y - delta.y >= work.top() {
            position.y - delta.y
        } else {
            position.y
        },
    );
    let origin = Pos2::new(
        preferred
            .x
            .clamp(work.left(), (work.right() - MENU_SIZE.x).max(work.left())),
        preferred
            .y
            .clamp(work.top(), (work.bottom() - MENU_SIZE.y).max(work.top())),
    );
    MenuPlacement {
        origin: [origin.x, origin.y],
        ball_offset: position - origin,
    }
}

fn set_menu(ctx: &egui::Context, geometry: &Mutex<Geometry>, open: bool) {
    let Ok(mut state) = geometry.lock() else {
        return;
    };
    if state.menu.is_some() == open {
        return;
    }
    if open {
        let anchor = state
            .anchor
            .or_else(|| {
                ctx.input(|input| input.viewport().outer_rect)
                    .map(|rect| [rect.min.x, rect.min.y])
            })
            .unwrap_or([0.0, 0.0]);
        state.anchor = Some(anchor);
        state.menu = Some(menu_placement(anchor, work_area(ctx, anchor)));
        state.restoring = false;
    } else {
        state.menu = None;
        state.restoring = true;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(if open {
        MENU_SIZE
    } else {
        Vec2::splat(BALL_SIZE)
    }));
    if let Some(origin) = state.origin() {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::from(origin)));
    }
    if open {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    drop(state);
    ctx.request_repaint();
    ctx.request_repaint_of(egui::ViewportId::ROOT);
}

fn work_area(ctx: &egui::Context, anchor: [f32; 2]) -> Rect {
    #[cfg(target_os = "windows")]
    if let Some(work) = windows::work_area(ctx, anchor) {
        return work;
    }
    let size = ctx
        .input(|input| input.viewport().monitor_size)
        .unwrap_or(Vec2::new(1920.0, 1080.0))
        .max(MENU_SIZE);
    // Fallback for desktops without a work-area API, including left monitors.
    let origin = Pos2::new(
        (anchor[0] / size.x).floor() * size.x,
        (anchor[1] / size.y).floor() * size.y,
    );
    Rect::from_min_size(origin, size)
}

fn ball(ui: &mut egui::Ui, sender: &mpsc::Sender<Action>, geometry: &Mutex<Geometry>) {
    let menu = geometry.lock().ok().and_then(|state| state.menu);
    let offset = menu.map(|menu| menu.ball_offset).unwrap_or(Vec2::ZERO);
    let rect = Rect::from_min_size(ui.max_rect().min + offset, Vec2::splat(BALL_SIZE));
    let response = ui.interact(
        rect,
        ui.id().with("floating-ball"),
        egui::Sense::click_and_drag(),
    );
    let center = rect.center();
    let colors = super::theme::palette(ui.ctx());
    let painter = ui.painter();
    painter.circle_filled(
        center + Vec2::new(0.0, 2.0),
        24.0,
        Color32::from_black_alpha(24),
    );
    painter.circle_filled(center, 25.0, colors.accent_soft);
    let icon = crate::branding::texture(ui.ctx(), 48.0, crate::branding::IconShape::Circle);
    egui::Image::new(&icon).paint_at(
        ui,
        Rect::from_center_size(center, Vec2::splat(48.0)).round_to_pixels(ui.pixels_per_point()),
    );
    painter.circle_stroke(
        center,
        24.5,
        Stroke::new(
            if response.hovered() { 2.0 } else { 1.0 },
            if response.hovered() {
                colors.accent
            } else {
                colors.border
            },
        ),
    );
    if response.drag_started() {
        set_menu(ui.ctx(), geometry, false);
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if response.clicked() {
        set_menu(ui.ctx(), geometry, false);
        dispatch(ui.ctx(), sender, Action::Open);
    }
    if response.secondary_clicked() {
        set_menu(ui.ctx(), geometry, menu.is_none());
    }
    // A tooltip cannot fit in the 64px viewport and would cover the logo.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            "DevShop：单击打开工作台，拖动移动，右键显示菜单",
        )
    });
    #[cfg(target_os = "windows")]
    let mut popup = None;
    if let Some(menu) = menu {
        let top = if menu.ball_offset.y > 1.0 {
            0.0
        } else {
            BALL_SIZE
        };
        let menu_rect = Rect::from_min_size(
            ui.max_rect().min + Vec2::new(0.0, top),
            Vec2::new(MENU_SIZE.x, MENU_SIZE.y - BALL_SIZE),
        );
        let painted = ui.scope_builder(egui::UiBuilder::new().max_rect(menu_rect), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(10.0)
                .show(ui, |ui| menu_content(ui, sender, geometry))
        });
        #[cfg(target_os = "windows")]
        {
            let corners = ui.visuals().menu_corner_radius;
            let radius = corners.nw.min(corners.ne).min(corners.sw).min(corners.se);
            popup = Some((painted.inner.response.rect, f32::from(radius)));
        }
        #[cfg(not(target_os = "windows"))]
        let _ = painted;
    }
    #[cfg(target_os = "windows")]
    windows::configure_transparency(ui.ctx(), rect, popup);
}

fn menu_content(ui: &mut egui::Ui, sender: &mpsc::Sender<Action>, geometry: &Mutex<Geometry>) {
    ui.set_width(208.0);
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.label(egui::RichText::new("DevShop").strong());
    ui.separator();
    for (label, action) in [
        ("打开工作台", Action::Open),
        ("读取剪贴板", Action::Clipboard),
        ("历史记录", Action::History),
        ("隐藏悬浮球", Action::Hide),
        ("完全退出", Action::Exit),
    ] {
        if action == Action::Hide {
            ui.separator();
        }
        if ui
            .add_sized([208.0, 25.0], egui::Button::new(label))
            .clicked()
        {
            set_menu(ui.ctx(), geometry, false);
            dispatch(ui.ctx(), sender, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_fit(anchor: [f32; 2], work: Rect) -> MenuPlacement {
        let menu = menu_placement(anchor, work);
        let origin = Pos2::from(menu.origin);
        assert_eq!(origin + menu.ball_offset, Pos2::from(anchor));
        assert!(work.contains_rect(Rect::from_min_size(origin, MENU_SIZE)));
        menu
    }

    #[test]
    fn bottom_right_menu_expands_up_and_left_without_moving_ball() {
        let menu = check_fit(
            [1856.0, 976.0],
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1040.0)),
        );
        assert_eq!(menu.ball_offset, MENU_SIZE - Vec2::splat(BALL_SIZE));
    }

    #[test]
    fn top_left_menu_expands_down_and_right() {
        let menu = check_fit(
            [8.0, 8.0],
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1040.0)),
        );
        assert_eq!(menu.ball_offset, Vec2::ZERO);
    }

    #[test]
    fn left_monitor_uses_its_own_work_area() {
        let work = Rect::from_min_max(Pos2::new(-1920.0, 0.0), Pos2::new(0.0, 1040.0));
        check_fit([-80.0, 960.0], work);
        assert_eq!(check_fit([-1912.0, 8.0], work).ball_offset, Vec2::ZERO);
    }

    #[test]
    fn late_resize_and_move_events_do_not_overwrite_restored_anchor() {
        let anchor = [1856.0, 976.0];
        let menu = menu_placement(
            anchor,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1040.0)),
        );
        let mut state = Geometry {
            anchor: Some(anchor),
            menu: None,
            restoring: true,
        };
        state.observe(Rect::from_min_size(Pos2::from(menu.origin), MENU_SIZE));
        state.observe(Rect::from_min_size(
            Pos2::from(menu.origin),
            Vec2::splat(BALL_SIZE),
        ));
        assert_eq!(state.anchor, Some(anchor));
        assert!(state.restoring);
        state.observe(Rect::from_min_size(
            Pos2::from(anchor),
            Vec2::splat(BALL_SIZE),
        ));
        assert!(!state.restoring);
        state.observe(Rect::from_min_size(
            Pos2::new(1200.0, 800.0),
            Vec2::splat(BALL_SIZE),
        ));
        assert_eq!(state.anchor, Some([1200.0, 800.0]));
    }
}
