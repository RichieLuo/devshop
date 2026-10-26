use eframe::egui::{self, CursorIcon, Pos2, Rect, ResizeDirection, Sense, Vec2};

mod native;
pub(super) use native::install;

const EDGE: f32 = 6.0;
const CORNER: f32 = 16.0;

pub(super) fn show(ctx: &egui::Context) {
    if ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new("native-window-resize"))
            .unwrap_or(false)
    }) {
        return;
    }
    if ctx.input(|input| {
        input.viewport().maximized.unwrap_or(false) || input.viewport().fullscreen.unwrap_or(false)
    }) {
        return;
    }
    for (direction, rect, cursor) in handles(ctx.viewport_rect()) {
        egui::Area::new(egui::Id::new(("window-resize", direction as u8)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .default_size(rect.size())
            .movable(false)
            .constrain(false)
            .sense(Sense::hover())
            .show(ctx, |ui| {
                let (_, response) = ui.allocate_exact_size(rect.size(), Sense::drag());
                let response = response.on_hover_cursor(cursor);
                if response.is_pointer_button_down_on()
                    && ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Primary))
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            });
    }
}

fn handles(rect: Rect) -> [(ResizeDirection, Rect, CursorIcon); 8] {
    let (left, right, top, bottom) = (rect.left(), rect.right(), rect.top(), rect.bottom());
    let bounds = |x1, y1, x2, y2| Rect::from_min_max(Pos2::new(x1, y1), Pos2::new(x2, y2));
    [
        (
            ResizeDirection::NorthWest,
            Rect::from_min_size(rect.left_top(), Vec2::splat(CORNER)),
            CursorIcon::ResizeNwSe,
        ),
        (
            ResizeDirection::NorthEast,
            bounds(right - CORNER, top, right, top + CORNER),
            CursorIcon::ResizeNeSw,
        ),
        (
            ResizeDirection::SouthWest,
            bounds(left, bottom - CORNER, left + CORNER, bottom),
            CursorIcon::ResizeNeSw,
        ),
        (
            ResizeDirection::SouthEast,
            bounds(right - CORNER, bottom - CORNER, right, bottom),
            CursorIcon::ResizeNwSe,
        ),
        (
            ResizeDirection::North,
            bounds(left + CORNER, top, right - CORNER, top + EDGE),
            CursorIcon::ResizeVertical,
        ),
        (
            ResizeDirection::South,
            bounds(left + CORNER, bottom - EDGE, right - CORNER, bottom),
            CursorIcon::ResizeVertical,
        ),
        (
            ResizeDirection::West,
            bounds(left, top + CORNER, left + EDGE, bottom - CORNER),
            CursorIcon::ResizeHorizontal,
        ),
        (
            ResizeDirection::East,
            bounds(right - EDGE, top + CORNER, right, bottom - CORNER),
            CursorIcon::ResizeHorizontal,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(ctx: &egui::Context, events: Vec<egui::Event>, maximized: bool) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0))),
                viewports: [(
                    egui::ViewportId::ROOT,
                    egui::ViewportInfo {
                        maximized: Some(maximized),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
                events,
                ..Default::default()
            },
            show,
        )
    }

    #[test]
    fn every_edge_and_corner_starts_native_resizing_at_common_scales() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 600.0));
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (direction, rect, cursor) in handles(screen) {
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(scale);
                let pos = rect.center();
                let _ = render(&ctx, vec![], false);
                let _ = render(&ctx, vec![], false);
                let hovered = render(&ctx, vec![egui::Event::PointerMoved(pos)], false);
                assert_eq!(hovered.platform_output.cursor_icon, cursor);
                let output = render(
                    &ctx,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    false,
                );
                assert!(
                    output.viewport_output[&egui::ViewportId::ROOT]
                        .commands
                        .contains(&egui::ViewportCommand::BeginResize(direction)),
                    "{direction:?} at {scale}"
                );
            }
        }
    }

    #[test]
    fn maximized_windows_and_interior_clicks_do_not_start_resizing() {
        for (maximized, pos) in [
            (true, Pos2::new(2.0, 2.0)),
            (false, Pos2::new(450.0, 300.0)),
        ] {
            let ctx = egui::Context::default();
            let _ = render(&ctx, vec![], maximized);
            let _ = render(&ctx, vec![egui::Event::PointerMoved(pos)], maximized);
            let output = render(
                &ctx,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                maximized,
            );
            assert!(
                !output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::BeginResize(_)))
            );
        }
    }
}
