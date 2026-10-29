#!/usr/bin/env bash
set -euo pipefail
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/28.2.13676358"
export ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
sdkmanager 'platforms;android-36' 'build-tools;36.0.0' 'ndk;28.2.13676358'
if [[ -n "${DOREMI_NATIVE_RUN:-}" ]]; then
  [[ "$DOREMI_NATIVE_RUN" =~ ^[0-9]+$ ]]
  native_artifact=$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$DOREMI_NATIVE_RUN/artifacts" --jq '.artifacts | map(select(.name | startswith("doremi-android-native-"))) | if length == 1 then .[0].name else error("Expected exactly one native artifact") end')
  # A single exact name extracts into this directory. Pattern downloads nest it.
  gh run download "$DOREMI_NATIVE_RUN" --repo "$GITHUB_REPOSITORY" --name "$native_artifact" --dir target/android-jni
  python3 scripts/android-native-identity.py --check
else
  rustup target add aarch64-linux-android armv7-linux-androideabi
  cargo install cargo-ndk --version 4.1.2 --locked
  cargo ndk -t arm64-v8a -t armeabi-v7a -P 26 -o target/android-jni build --locked -p doremi-app --lib
  python3 scripts/android-native-identity.py
fi
mkdir -p target/android-assets/licenses target/android-dist
cp target/android-jni/native-source.json target/android-assets/
cp LICENSE target/android-assets/licenses/MIT.txt
cp assets/doremi/LICENSE target/android-assets/licenses/Doremi-assets-GPL-3.txt
cp assets/doremi/PROVENANCE.md target/android-assets/licenses/
cp assets/audio/LICENSE.txt target/android-assets/licenses/Audio-CC0.txt
cp assets/audio/manifest.json target/android-assets/licenses/Audio-manifest.json
git rev-parse HEAD > target/android-assets/SOURCE_COMMIT
gradle -p android --no-daemon assembleDebug assembleDebugAndroidTest
cp target/android-gradle/app/outputs/apk/debug/*.apk target/android-dist/
cp target/android-gradle/app/outputs/apk/androidTest/debug/*.apk target/android-dist/
python3 scripts/verify-android.py target/android-dist --create
