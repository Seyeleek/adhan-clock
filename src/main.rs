#![windows_subsystem = "windows"]

use slint::ComponentHandle;

mod app_logic;

fn main() {
    app_logic::main_window("").run().unwrap();
}
