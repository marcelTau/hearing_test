mod app;
mod audio;
mod domain;
mod localization;
mod report;
mod storage;
mod test_engine;
mod theme;

use app::HearingApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Hearing Tracker")
            .with_inner_size([1_050.0, 760.0])
            .with_min_inner_size([850.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "org.personal.hearing-tracker",
        options,
        Box::new(|context| Ok(Box::new(HearingApp::new(context)))),
    )
}
