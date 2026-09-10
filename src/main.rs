#![windows_subsystem = "windows"]

mod app_logic;

use slint::ComponentHandle;
use crate::app_logic::LogError;

fn main() {
    app_logic::main_window("").run().unwrap_or_log("");
}
