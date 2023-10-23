use app::App;
use eframe::epaint::Vec2;
use sane_scan::{self, Sane};
use tinyfiledialogs::{MessageBoxIcon, message_box_ok};

mod app;
mod commonvals;
mod util;

const DEFAULT_FILE_NAME: &str = "scan.pdf";
const ERR_DIALOG_TITLE: &str = "SlickScan Error";
const LETTER_WIDTH_MM: f32 = 215.9;
const LETTER_HEIGHT_MM: f32 = 279.4;
const LETTER_WIDTH_IN: f32 = 8.5;
const LETTER_HEIGHT_IN: f32 = 11.0;

fn main() {
    env_logger::init();

    let options = eframe::NativeOptions {
        initial_window_size: Some(Vec2::new(1050.0, 850.0)),
        ..Default::default()
    };

    // Initialize SANE components
    let version_code = 0;
    let sane_instance = Sane::init(version_code);

    match sane_instance {
        Ok(sane_instance) => eframe::run_native(
            "SlickScan",
            options,
            Box::new(|cc| Box::new(App::new(cc, sane_instance)))).unwrap(),
        Err(error) => message_box_ok(ERR_DIALOG_TITLE, &format!("Error occurred while setting up SANE scanner interface: {error}"), MessageBoxIcon::Error),
    }
}
