#![cfg_attr(windows, windows_subsystem = "windows")]

use cheatsheets::app::CheatSheetsApp;
use cheatsheets::storage::{self, WindowPlacement};
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    let settings = storage::load_app_settings().unwrap_or_default();
    let window = settings.window;
    let size = window
        .map(|window| [window.width, window.height])
        .unwrap_or([
            WindowPlacement::DEFAULT_WIDTH,
            WindowPlacement::DEFAULT_HEIGHT,
        ]);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("CheatSheets")
        .with_inner_size(size)
        .with_min_inner_size([WindowPlacement::MIN_WIDTH, WindowPlacement::MIN_HEIGHT])
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(true)
        .with_always_on_top();
    if let Some(window) = window {
        viewport = viewport.with_position([window.x, window.y]);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "CheatSheets",
        native_options,
        Box::new(|cc| Ok(Box::new(CheatSheetsApp::new(cc)))),
    )
}
