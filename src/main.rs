#![windows_subsystem = "windows"]

mod app_logic;

use slint::ComponentHandle;

fn main() {
    app_logic::main_window("").run().unwrap();
}
