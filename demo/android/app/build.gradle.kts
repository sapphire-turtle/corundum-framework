plugins {
    id("com.android.application")
}

android {
    namespace = "net.sapphire_turtle.corundum_demo"
    compileSdk = 36

    defaultConfig {
        applicationId = "net.sapphire_turtle.corundum_demo"
        // LocaleList (the system's languages, corundum-framework-platform) is API 24.
        minSdk = 24
        targetSdk = 36
        versionCode = 1
        versionName = "0.0.0"
        // The ABIs build-rust.sh builds: phones, and the emulators on x86_64 hosts.
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // games-activity pulls in kotlin-stdlib-jdk7/-jdk8 1.6, whose classes kotlin-stdlib 1.8
    // (from appcompat) also has; the BOM brings them to 1.8, where they are empty.
    implementation(platform("org.jetbrains.kotlin:kotlin-bom:1.8.22"))
    implementation("androidx.appcompat:appcompat:1.7.1")
    // The version android-activity 0.6 is built against. Without prefab: android-activity
    // brings its own native glue, and the upstream C++ one must not be linked.
    implementation("androidx.games:games-activity:4.4.0")
    // Google's code scanner, which corundum's platform::scan_qr opens (`qr-scan`). Play
    // services draws the camera's screen, so the app asks for no camera permission.
    implementation("com.google.android.gms:play-services-code-scanner:16.1.0")
}
