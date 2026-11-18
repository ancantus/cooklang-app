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

Artifact (x86_64, for the emulator):

`target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`

The APK contains only the default target's native library (`lib/x86_64/libmain.so`); the default is `x86_64-linux-android`. Building with `--target aarch64-linux-android` writes to the same path and adds `lib/arm64-v8a/libmain.so` (both ABIs can be present after back-to-back builds). For a physical arm64 phone, build with `--target aarch64-linux-android` (see README).

## Machine-local Gradle init script (rustls TLS support)

Dioxus regenerates the Gradle project under `target/dx/.../android/` on every
build, so Gradle settings cannot be edited there directly. The reqwest/rustls
HTTP transport used by `gix` relies on `rustls-platform-verifier`, whose
certificate-verification class (`org.rustls.platformverifier.CertificateVerifier`)
is a Kotlin component that must be bundled into the APK. It is supplied with a
machine-local Gradle init script at `~/.gradle/init.d/cooklang-rustls-verifier.gradle`:

- Adds the Maven repository
  `https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/`
  to all projects.
- Adds the dependency `org.rustls:rustls-platform-verifier:0.2.0` to the `app`
  subproject after evaluation.

The dependency version must track `rustls-platform-verifier-android` in
`Cargo.lock` (currently `0.2.0`); bump the init script if that crate version
changes. Verify the class made it into the APK after a build:

```sh
unzip -l target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk | grep classes
# then grep the dex files for "CertificateVerifier" if in doubt
```

Without this, the app builds but the first HTTPS clone fails with
`ClassNotFoundException: org.rustls.platformverifier.CertificateVerifier`.

## Emulator and T3 Code hookup

The SDK emulator and Android 35 Google APIs x86_64 image are installed, and the `Cooklang_API_35` AVD was created. KVM is available on this host: `/dev/kvm` exists and `emulator -accel-check` reports "KVM (version 12) is installed and usable", so the emulator runs accelerated.

Daily driver (from the project root):

```sh
source ./android-env.sh
setsid nohup emulator -avd Cooklang_API_35 -no-snapshot -no-audio -no-window \
  -gpu swiftshader_indirect >/tmp/cooklang-emulator.log 2>&1 </dev/null &
adb shell getprop sys.boot_completed   # becomes 1 after ~65-90 s
```

Once it is running, `adb devices` lists it as `device`; then run `dx serve --android` or install a bundled APK (see README). The default `dx bundle --android` builds the `x86_64-linux-android` target, which is exactly what the emulator needs.

A physical Android device is another option: enable Developer options and USB debugging, connect it to this same machine, authorize the computer on the device, and confirm it appears as `device` in `adb devices`. For a phone, builds must use `--target aarch64-linux-android` because the default x86_64 APK only contains an x86_64 `libmain.so` and will fail to install on arm64 hardware. See the README "Install and test on a physical phone" section for the full workflow. No Android device was connected during the original setup.

## Emulator workflow notes

- The host has ~7.6 GB RAM and 4 cores; the emulator runs headless with
  software GPU rendering. Launch it fully detached so it survives the shell
  session that started it:

  ```sh
  source ./android-env.sh
  setsid nohup emulator -avd Cooklang_API_35 -no-snapshot -no-audio -no-window \
    -gpu swiftshader_indirect >/tmp/cooklang-emulator.log 2>&1 </dev/null &
  ```

- Boot takes ~65–90 s (`adb shell getprop sys.boot_completed` becomes `1`).
  App data persists across boots; the app stays installed and its saved
  collection (files/connected-repository, files/collections/...) survives
  restarts.
- System UI ANRs ("System UI isn't responding") recur under load. Dismiss
  with Wait, or restart System UI: `adb root && adb shell killall com.android.systemui`.
- Animations are disabled (`window_animation_scale`, `transition_animation_scale`,
  `animator_duration_scale` = 0) and `com.google.android.googlequicksearchbox`
  is disabled to reduce load.
- Rebuild + install loop:

  ```sh
  source ./android-env.sh
  dx bundle --android --package-types apk
  adb install -r target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
  adb shell am force-stop org.cooklang.client && adb shell monkey -p org.cooklang.client 1
  ```

  Wait ~12 s after launch before interacting; the first screenshot after
  launch may show an ANR dialog over the app.
- Screenshot pixel coordinates are 0.83× of the real device coordinates:
  multiply the displayed position by 1.2 for `adb shell input tap`. The
  connect screen's input is around real (540, 1040) and the Connect button
  around (540, 1200).
- Type text into the WebView char-by-char (`adb shell input text "$c"` per
  character with ~0.2 s pauses). Word-sized `input text` commits get mangled
  by the Gboard autocorrect pipeline.
- Do not send `KEYCODE_ESCAPE` (111) or the system BACK key (4) with the IME
  down: ESC reaches the WebView and hits a framework-level FORTIFY
  "pthread_mutex_lock called on a destroyed mutex" abort that kills the
  process. BACK used to be equally fatal when it finished the activity; the
  build-time Kotlin hook in `.cargo/config.toml` now handles BACK safely:
  while a recipe detail screen is open it clicks the in-app "‹ Back to
  recipes" button (returning to the list), otherwise it backgrounds the app
  with `moveTaskToBack`. The activity is never finished, so the crash path
  is never reached. The IME still consumes BACK while it is up (closes the
  keyboard).
- Unit tests exist for the repository logic (`src/repository.rs`,
  exposed through `src/lib.rs`). Run them on the host with
  `cargo test --no-default-features`; plain `cargo test` compiles the
  Dioxus app for the host too and needs glib/gtk development headers that
  are not installed on this machine.
