mod app;
mod config;
mod decoder;
mod keyboard;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

use app::PreviewApp;
use config::PreviewConfig;

fn main() -> eframe::Result<()> {
    let config = PreviewConfig::parse();
    eframe::run_native(
        "Mocha Keyboard Preview",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 560.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            Ok(Box::new(PreviewApp::new(config)))
        }),
    )
}

fn configure_fonts(ctx: &egui::Context) {
    let Some((name, bytes)) = load_chinese_font() else {
        eprintln!("no CJK font found; Chinese candidates may render incorrectly");
        return;
    };

    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert(name.clone(), FontData::from_owned(bytes));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.clone());
    }
    ctx.set_fonts(fonts);
}

fn load_chinese_font() -> Option<(String, Vec<u8>)> {
    [
        ("msyh", r"C:\Windows\Fonts\msyh.ttc"),
        ("msyhbd", r"C:\Windows\Fonts\msyhbd.ttc"),
        ("simhei", r"C:\Windows\Fonts\simhei.ttf"),
        ("simsun", r"C:\Windows\Fonts\simsun.ttc"),
        ("deng", r"C:\Windows\Fonts\Deng.ttf"),
        ("noto-cjk", r"C:\Windows\Fonts\NotoSansCJK-Regular.ttc"),
    ]
    .into_iter()
    .find_map(|(name, path)| {
        std::fs::read(path)
            .ok()
            .map(|bytes| (name.to_owned(), bytes))
    })
}
