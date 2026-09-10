mod app_logic;

use slint::{ComponentHandle,
    android::{
        AndroidApp,
        android_activity::WindowManagerFlags as WMFlags,
    },
};
use crate::app_logic::LogError;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    app.set_window_flags(
        WMFlags::KEEP_SCREEN_ON,
        WMFlags::empty(),
    );
    let file_prefix = "/storage/emulated/0/Android/data/com.example.adhan_clock/files/";
    slint::android::init(app).unwrap_or_log(file_prefix);
    app_logic::main_window(file_prefix).run().unwrap_or_log(file_prefix);
}
