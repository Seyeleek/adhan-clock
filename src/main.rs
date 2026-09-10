// SPDX-License-Identifier: GPL-3.0-only

#![windows_subsystem = "windows"]

mod app_logic;
mod cities;

use slint::ComponentHandle;

const FILE_PREFIX: &str = "";
const SAFELY_PRUNE: bool = false;

fn main() {
    app_logic::main_window().run().unwrap();
}
