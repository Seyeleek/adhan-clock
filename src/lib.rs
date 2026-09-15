// SPDX-License-Identifier: GPL-3.0-only

#[cfg(target_os = "android")]
mod app_logic;
#[cfg(target_os = "android")]
mod cities;

#[cfg(target_os = "android")]
use slint::{
    ComponentHandle,
    android::{AndroidApp, android_activity::WindowManagerFlags as WMFlags},
};

#[cfg(target_os = "android")]
const FILE_PREFIX: &str = "/storage/emulated/0/Android/data/io.github.Seyeleek.adhan_clock/files/";
#[cfg(target_os = "android")]
const SAFELY_PRUNE: bool = true;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    app.set_window_flags(WMFlags::KEEP_SCREEN_ON, WMFlags::empty());
    slint::android::init(app).unwrap();
    app_logic::main_window().run().unwrap();
}
