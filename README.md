# Adhan Clock

This app is an adhan clock designed primarily to run on an Android tablet.
However, it is cross-platform and should run on Android, Linux, Windows, MacOS,
and iOS (only the first three are tested and iOS will need hacking).

This app is hacked together and was only designed for my personal use. No
guarantees are made.

## Building

### For Windows or Linux (or maybe MacOS, untested)

Ensuring you have Rust installed, clone the repository and run the following:

```shell
cargo build --release
```

This will build the project for your operating system and create an executable
at `target/release/adhan-clock` or `target/release/adhan-clock.exe` which you
can then run.

If you want the executable to automatically run, run `cargo run --release`
instead.

You can also cross-compile for different platforms.

### For Android

Follow the instructions here: 
<https://docs.slint.dev/latest/docs/slint/guide/platforms/mobile/android/> to
install the Android SDK and NDK, set up the environment variables, and install
the `aarch64-linux-android` target. Also export the environment variable
`ANDROID_PLATFORM` to at least 30 so that the `i-slint-backend-android-activity`
crate will compile.

You will also need a `.keystore` file to sign your app; mine is named
`debug.keystore`, which is in the `.gitignore`. You can follow the instructions
here: <https://stackoverflow.com/a/15330139> to create one. Set the environment
variables `CARGO_APK_RELEASE_KEYSTORE` (with the path to the `keystore` file and
`CARGO_APK_RELEASE_KEYSTORE_PASSWORD` (with the password you set).

Next, you have to install `cargo-apk` from the instructions here:
<https://github.com/rust-mobile/cargo-apk> or directly from <https://crates.io>.

To build the Android package, run:

```shell
cargo apk build --lib --release
```

The package will be `target/release/apk/adhan-clock.apk`.

## Credits

For a complete list of the Rust packages used, please see `Cargo.toml`.

The adhan sounds are from here: <https://archive.org/details/Athan_Mawsoa_mp3>.
They are number 16 (adhan.ogg) and number 22 (fajr-adhan.ogg).

`i-slint-backend-android-activity` is a folder taken directly from the Slint 
source code (`internal/compiler/backends/android-activity`) and modified
slightly to remove panics for mousewheel scrolling on Android. In addition,
`ui/components.slint`, `ui/listview.slint`, `ui/styling.slint`,
`ui/lineedit.slint`, and `ui/scrollview.slint` are all also taken from the Slint
source code and modified to allow me to customize the styling. The Slint
copyright headers have been retained in all Slint code.

`bg.jpg` and `resources/drawable/icon.png` are modifications of the following
public-domain image: <https://commons.wikimedia.org/wiki/File:Detail_arabesque_Alhambra_Granada_Spain.jpg>.
