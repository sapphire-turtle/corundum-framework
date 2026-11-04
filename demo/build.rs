//! The app icon, from the SVGs in assets/: embedded in the Windows executable, and rendered
//! to the PNGs the phones' projects take (ignored by git) when cargo builds for one.

fn main() {
    corundum_framework_build::embed_icon("assets/app-icon.svg");
    corundum_framework_build::ios_app_icon(
        "assets/app-icon-square.svg",
        "ios/Assets.xcassets/AppIcon.appiconset",
    );
    corundum_framework_build::android_app_icons(
        "assets/app-icon.svg",
        "assets/app-icon-foreground.svg",
        "android/app/src/main/res",
    );
}
