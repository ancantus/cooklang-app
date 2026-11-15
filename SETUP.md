# Development environment setup log

Recorded on 2026-11-15 for Fedora Linux 44 (x86_64). Project-local configuration is in `android-env.sh`.

## Installed and used

- Rust stable via rustup: `rustc 1.100.0`, `cargo 1.100.0`
- Rust targets: `aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`, `x86_64-linux-android`
- Dioxus CLI: `dx 0.7.10`
- Android command-line tools under `/home/restep/Android/sdk`
- Android SDK Platform 34 (installed automatically by the Gradle build) and Platform 35
- Android Build Tools 34.0.0 (installed automatically by the Gradle build) and 35.0.0
- Android NDK 25.2.9519653 and CMake 3.22.1
- Android Emulator 37.2.13 and Android 35 Google APIs x86_64 system image
- AVD `Cooklang_API_35` (Pixel 8 profile)
- Eclipse Temurin JDK 17 at `/usr/lib/jvm/java-17-temurin-jdk`; the system also has OpenJDK 25, but that installation lacks `javac` and Gradle could not use it

Rust was installed with rustup's official installer, and `dx` with Dioxus' official installer. Google's command-line tools archive `commandlinetools-linux-15859902_latest.zip` was unpacked into the SDK directory. The Android SDK licenses were accepted for this development setup.

The SDK manager command used was:

```sh
sdkmanager 'platform-tools' 'platforms;android-35' 'build-tools;35.0.0' \
  'ndk;25.2.9519653' 'cmake;3.22.1' 'emulator' \
  'system-images;android-35;google_apis;x86_64'
```

Rust targets installed:

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi \
  i686-linux-android x86_64-linux-android
```

## Use the environment

From the project directory:

```sh
source ./android-env.sh
dx serve --android
```

Build a release APK with:

```sh
source ./android-env.sh
dx bundle --android --release --package-types apk
```

## Build result

A debug APK build completed successfully with:

```sh
dx bundle --android --package-types apk
```

Artifact:

`target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`

## Emulator and T3 Code hookup

The SDK emulator and Android 35 Google APIs x86_64 image are installed, and the `Cooklang_API_35` AVD was created. However, `emulator -accel-check` reports `/dev/kvm` is missing. On Linux, Android's accelerated emulator depends on KVM; software emulation may be very slow or unavailable.

For a usable emulator, enable CPU virtualization in firmware and make `/dev/kvm` available to this user. Then check:

```sh
source ./android-env.sh
emulator -accel-check
emulator -avd Cooklang_API_35
```

Once it is running, `adb devices` should list it as `device`; run `dx serve --android` in the T3 Code terminal. A physical Android device is another option: enable Developer options and USB debugging, connect it to this same machine, authorize the computer on the device, and confirm it appears as `device` in `adb devices`. No Android device was connected during this setup.
