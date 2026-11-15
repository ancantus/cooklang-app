#!/usr/bin/env bash
# Source this file from the project root before using dx or Android tools.
export JAVA_HOME="/usr/lib/jvm/java-17-temurin-jdk"
export ANDROID_HOME="/home/restep/Android/sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export NDK_HOME="$ANDROID_HOME/ndk/25.2.9519653"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
