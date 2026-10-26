use crate::{branding, ui};

pub fn run() -> eframe::Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let Some(instance) = crate::shell_open::start("DevShop", &args)
        .map_err(|error| eframe::Error::AppCreation(Box::new(error)))?
    else {
        return Ok(());
    };
    let attachment = instance.attachment();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("DevShop")
            .with_decorations(!cfg!(target_os = "windows"))
            .with_transparent(true)
            .with_resizable(true)
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([1024.0, 680.0])
            .with_icon(branding::icon_data()),
        ..Default::default()
    };
    let result = eframe::run_native(
        "DevShop",
        options,
        Box::new(move |cc| {
            attachment.attach_context(&cc.egui_ctx);
            Ok(Box::new(ui::App::new(cc)))
        }),
    );
    drop(instance);
    result
}
