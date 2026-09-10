#![windows_subsystem = "windows"]

mod app_logic;
mod cities;

use slint::ComponentHandle;

const FILE_PREFIX: &str = "";

fn main() {
    let (window, _timer) = app_logic::main_window();
    window.run().unwrap();
}
