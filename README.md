# cooklang-client-android

A native Android app built with Rust and Dioxus 0.7. Dioxus renders the mobile UI in the platform WebView; the app shell and application logic are Rust.

First-time machine setup and the full tool log are in [SETUP.md](SETUP.md). Every command below assumes you are in the project root and have sourced the environment:

```sh
source ./android-env.sh
```

The app package identifier is `org.cooklang.client` (set in `dioxus.toml`).

## Build

Debug APK (default target `x86_64-linux-android`, matches the emulator):

```sh
dx bundle --android --package-types apk
```

Artifact:
`target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk`

Release APK:

```sh
dx bundle --android --release --package-types apk
```

Artifact (verified; note the Gradle packaging variant stays `debug`, only the Rust build is release):
`target/dx/cooklang-client-android/release/android/app/app/build/outputs/apk/debug/app-debug.apk`

For a physical phone you must build the `aarch64-linux-android` target — the default x86_64 APK will not run on it:

```sh
dx bundle --android --target aarch64-linux-android --package-types apk
dx bundle --android --target aarch64-linux-android --release --package-types apk
```

A quick host compile check without bundling:

```sh
cargo check
```

## Install and test on the KVM emulator

1. Start the emulator headless, detached so it survives the shell session:

   ```sh
   setsid nohup emulator -avd Cooklang_API_35 -no-snapshot -no-audio -no-window \
     -gpu swiftshader_indirect >/tmp/cooklang-emulator.log 2>&1 </dev/null &
   ```

2. Wait for boot (~65–90 s) and confirm it is visible to adb:

   ```sh
   adb shell getprop sys.boot_completed   # becomes 1 when booted
   adb devices                            # emulator listed as "device"
   ```

3. Install and launch:

   ```sh
   dx bundle --android --package-types apk
   adb install -r target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
   adb shell am force-stop org.cooklang.client && adb shell monkey -p org.cooklang.client 1
   ```

   For a build-install-relaunch loop, run the three commands again after each change. Wait ~12 s after launch before interacting.

   Alternatively use live reload while the emulator is running — it builds, installs, and opens the app on the emulator:

   ```sh
   dx serve --android
   ```

4. Manual smoke test:

   - Enter the repository on the connect screen. All of these forms are accepted and are unit-tested (`cargo test --no-default-features`):
     `https://github.com/cooklang/recipes`, `https://github.com/cooklang/recipes.git`, `github.com/cooklang/recipes`, and the `cooklang/recipes` shorthand. Prefer the full `https://` URL when typing on a device — phone keyboards are more likely to mangle the short form.
   - Tap Connect and wait for the clone.
   - Browse the recipe list, search by title and tag, and open a recipe.
   - Check ingredients, method steps, and timers render in the detail view.
   - Tap Refresh, then Disconnect, and confirm the empty state appears.
   - Press the system BACK key while a recipe is open: the app returns to the recipe list. Press BACK on the list: the app is backgrounded (not closed).

Known emulator quirks (ANRs, screenshot coordinate scaling, typing quirks, the BACK key abort) are documented in [SETUP.md](SETUP.md).

## Install and test on a physical phone

1. On the phone, enable Developer options, then USB debugging. Connect it by USB to this machine and accept the "allow USB debugging" prompt on the phone.
2. Confirm the phone is visible: `adb devices` must list it as `device`.
3. Build and install the arm64 APK. The output path is the same as the default build; verify it contains `lib/arm64-v8a/libmain.so` (`unzip -l <apk> | grep '\.so'`) before installing:

   ```sh
   dx bundle --android --target aarch64-linux-android --package-types apk
   adb install -r target/dx/cooklang-client-android/debug/android/app/app/build/outputs/apk/debug/app-debug.apk
   adb shell am force-stop org.cooklang.client && adb shell monkey -p org.cooklang.client 1
   ```

   With the phone attached, `dx serve --android --target aarch64-linux-android` also works for live reload.

   If several devices are attached (phone + emulator), pass `-s <serial>` to adb, e.g. `adb -s <serial> install -r ...`. `adb devices` prints the serials.

4. Without USB, transfer the APK to the phone (email, cloud, `adb push` to shared storage), then install it from a file manager after allowing "install unknown apps" for that source. Modern phones are arm64, so always use the `--target aarch64-linux-android` build.
5. Run the same manual smoke test as on the emulator. Network-dependent steps (Connect, Refresh) need the phone online.

## Automated checks

Unit tests for the repository logic (URL normalization, recipe discovery) run on the host without the GUI dependency tree:

```sh
cargo test --no-default-features
```

Plain `cargo test` additionally compiles the Dioxus app for the host, which needs system glib/gtk development headers that are not installed on this machine; use the command above. Verification is the unit tests plus the manual smoke tests above.

## Project structure

- `src/main.rs` — Dioxus entry point and initial screen
- `src/ui.rs` — UI components
- `src/repository.rs` — Git checkout, recipe discovery, and parsing
- `assets/main.css` — app styling
- `dioxus.toml` — Dioxus app and Android bundle configuration
- `android-env.sh` — Android SDK, NDK, Java, and Rust environment for this host
- `SETUP.md` — installed tool versions, steps, emulator hookup notes, and known on-device workarounds
