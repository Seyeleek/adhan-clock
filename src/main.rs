// SPDX-License-Identifier: GPL-3.0-only

//! This is the developer documentation for the adhan-clock project at
//! <https://github.com/Seyeleek/adhan-clock>. It is not really useful for end
//! users, but it may make developers' lives easier.
//!
//! Most of the code is in the [`app_logic`] module, so that is the recommended
//! starting point.

#![windows_subsystem = "windows"]

mod app_logic;
mod cities;

use slint::ComponentHandle;

/// A platform-specific directory for program-generated files.
const FILE_PREFIX: &str = "";
/// Whether or not the app can safely prune the generated files.
const SAFELY_PRUNE: bool = false;

/// Runs the app.
fn main() {
    app_logic::main_window().run().unwrap();
}
