// The demo's Android app: GameActivity around the Rust cdylib (libcorundum_demo.so), which
// build-rust.sh puts in app/src/main/jniLibs before Gradle packs it.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "CorundumDemo"
include(":app")
