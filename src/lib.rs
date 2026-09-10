use slint::{ComponentHandle,
    android::{
        AndroidApp,
        android_activity::WindowManagerFlags as WMFlags,
    },
};

mod app_logic;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    app.set_window_flags(
        WMFlags::KEEP_SCREEN_ON,
        WMFlags::empty(),
    );
    slint::android::init(app).unwrap();
    app_logic::main_window(
        "/storage/emulated/0/Android/data/com.example.adhan_clock/files/"
    ).run().unwrap();
}
