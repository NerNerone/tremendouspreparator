#!/usr/bin/env bash
# Costruisce l'APK Android (arm64) dai sorgenti.
#
#   tools/build_apk.sh [uscita.apk]
#
# Richiede: Rust con il target aarch64-linux-android, cargo-apk 0.10, JDK 17, Android SDK
# (ANDROID_HOME, build-tools e platforms/android-35) e NDK (ANDROID_NDK_ROOT).
#
# Firma: con KEYSTORE e KEYSTORE_PASSWORD (chiave e alias con la stessa password) l'APK esce
# firmato; altrimenti esce non firmato, come serve a chi lo firma da sé (es. F-Droid).
#
# cargo-apk compila la libreria Rust ma non il codice Java né i componenti del manifest
# (receiver, servizio di concentrazione): l'APK finale si ricompone con il suo manifest
# completato da android/patch_manifest.py, la libreria, classes.dex e le risorse.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$(realpath -m "${1:-$ROOT/target/release/apk/TremendousPreparator.apk}")"
: "${ANDROID_HOME:?ANDROID_HOME non impostata}"
: "${ANDROID_NDK_ROOT:?ANDROID_NDK_ROOT non impostata}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"

BT="$ANDROID_HOME/build-tools/$(ls "$ANDROID_HOME/build-tools" | sort -V | tail -1)"
JAR="$ANDROID_HOME/platforms/android-35/android.jar"
[ -f "$JAR" ] || { echo "manca $JAR (platforms;android-35)"; exit 1; }
MIN_API="$(sed -n 's/^min_sdk_version = //p' "$ROOT/crates/prep-app/Cargo.toml")"

W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/classes" "$W/dex" "$W/apk"

# cargo-apk firma sempre il suo APK intermedio: gli basta una chiave usa e getta
TMP_PASS="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
keytool -genkeypair -keystore "$W/tmp.jks" -storepass "$TMP_PASS" -keypass "$TMP_PASS" -alias tmp \
  -keyalg RSA -keysize 2048 -validity 1 -dname "CN=tmp" >/dev/null 2>&1
(
  cd "$ROOT/crates/prep-app"
  CARGO_APK_RELEASE_KEYSTORE="$W/tmp.jks" CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$TMP_PASS" \
    cargo apk build --release --lib
)
B="${CARGO_TARGET_DIR:-$ROOT/target}/release/apk"

python3 "$ROOT/android/patch_manifest.py" "$B/AndroidManifest.xml" "$W/AndroidManifest.xml"
javac --release 17 -cp "$JAR" -d "$W/classes" $(find "$ROOT/android/java" -name '*.java')
"$BT/d8" --release --min-api "$MIN_API" --lib "$JAR" --output "$W/dex" $(find "$W/classes" -name '*.class')
"$BT/aapt" package -f -F "$W/app.unaligned.apk" -M "$W/AndroidManifest.xml" -S "$ROOT/android/res" -I "$JAR"
cp -r "$B/lib" "$W/apk/"
cp "$W/dex/classes.dex" "$W/apk/"
(cd "$W/apk" && "$BT/aapt" add "$W/app.unaligned.apk" classes.dex $(find lib -type f | sort))
"$BT/zipalign" -f -p 4 "$W/app.unaligned.apk" "$W/app.aligned.apk"

mkdir -p "$(dirname "$OUT")"
if [ -n "${KEYSTORE:-}" ] && [ -n "${KEYSTORE_PASSWORD:-}" ]; then
  "$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass env:KEYSTORE_PASSWORD --key-pass env:KEYSTORE_PASSWORD \
    --out "$OUT" "$W/app.aligned.apk"
  "$BT/apksigner" verify "$OUT"
  echo "APK firmato: $OUT"
else
  cp "$W/app.aligned.apk" "$OUT"
  echo "APK non firmato: $OUT"
fi
"$BT/aapt" dump badging "$OUT" | sed -n 1,3p
