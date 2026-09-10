#![windows_subsystem = "windows"]

mod app_logic;
mod cities;

use slint::ComponentHandle;

const FILE_PREFIX: &str = "";

fn main() {
    app_logic::main_window().run().unwrap();
}
