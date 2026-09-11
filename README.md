# Adhan Clock

This app is an adhan clock designed primarily to run on an Android tablet.
However, it is cross-platform and should run on Android, Linux, Windows, MacOS,
and iOS (only the first three are tested and iOS will need hacking).

This app is hacked together and was only designed for my personal use. No
guarantees are made.

This app uses Rust and Slint.

The adhan sounds are from here: <https://archive.org/details/Athan_Mawsoa_mp3>.
They are number 16 (adhan.ogg) and number 22 (fajr-adhan.ogg).

i-slint-backend-android-activity is a folder taken directly from the Slint 
source code (internal/compiler/backends/android-activity) and modified slightly
to remove panics for mousewheel scrolling on Android. In addition,
ui/components.slint, ui/listview.slint, ui/styling.slint, ui/lineedit.slint, and
ui/scrollview.slint are all also taken from the Slint source code and modified
to allow me to customize the styling. The Slint copyright headers have been
retained in all Slint code.
