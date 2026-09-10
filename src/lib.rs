mod app_logic;
mod cities;

use slint::{ComponentHandle,
    android::{
        AndroidApp,
        android_activity::WindowManagerFlags as WMFlags,
    },
};

const FILE_PREFIX: &str = "/storage/emulated/0/Android/data/com.example.adhan_clock/files/";

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    app.set_window_flags(
        WMFlags::KEEP_SCREEN_ON,
        WMFlags::empty(),
    );
    slint::android::init(app).unwrap();
    let (window, _timer) = app_logic::main_window();
    window.run().unwrap();
}
