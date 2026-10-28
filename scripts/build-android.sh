#!/usr/bin/env bash
set -euo pipefail
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
sdkmanager 'platforms;android-36' 'build-tools;36.0.0' 'ndk;28.2.13676358'
rustup target add aarch64-linux-android armv7-linux-androideabi
cargo install cargo-ndk --version 4.1.2 --locked
cargo ndk -t arm64-v8a -t armeabi-v7a -p 26 -o target/android-jni build --locked -p doremi-app --lib
mkdir -p target/android-assets/licenses target/android-dist
cp LICENSE target/android-assets/licenses/MIT.txt
cp assets/doremi/LICENSE target/android-assets/licenses/Doremi-assets-GPL-3.txt
cp assets/doremi/PROVENANCE.md target/android-assets/licenses/
git rev-parse HEAD > target/android-assets/SOURCE_COMMIT
gradle -p android --no-daemon assembleDebug assembleDebugAndroidTest
cp target/android-gradle/app/outputs/apk/debug/*.apk target/android-dist/
cp target/android-gradle/app/outputs/apk/androidTest/debug/*.apk target/android-dist/
python3 scripts/verify-android.py target/android-dist --create
