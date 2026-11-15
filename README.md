# cooklang-client-android

A native Android app built with Rust and Dioxus 0.7. Dioxus renders the mobile UI in the platform WebView; the app shell and application logic are Rust.

## Run on Android

The development environment setup and install log is in [SETUP.md](SETUP.md). In this environment, source the helper and run:

```sh
source ./android-env.sh
dx serve --android
```

To build a release bundle:

```sh
source ./android-env.sh
dx bundle --android --release --package-types apk
```

The app package identifier is `org.cooklang.client` in `dioxus.toml`.

## Project structure

- `src/main.rs` — Dioxus entry point and initial screen
- `assets/main.css` — app styling
- `dioxus.toml` — Dioxus app and Android bundle configuration
- `android-env.sh` — Android SDK, NDK, Java, and Rust environment for this host
- `SETUP.md` — installed tool versions, steps, and emulator hookup notes
