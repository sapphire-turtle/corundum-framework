plugins {
    id("com.android.application")
}

android {
    namespace = "io.github.sapphireturtle.corundum.demo"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.sapphireturtle.corundum.demo"
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
    implementation("androidx.appcompat:appcompat:1.7.1")
    // The version android-activity 0.6 is built against. Without prefab: android-activity
    // brings its own native glue, and the upstream C++ one must not be linked.
    implementation("androidx.games:games-activity:4.4.0")
}
