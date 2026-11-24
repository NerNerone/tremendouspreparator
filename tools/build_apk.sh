#!/usr/bin/env bash
# Costruisce l'APK Android universale (arm64-v8a e armeabi-v7a) dai sorgenti.
#
#   tools/build_apk.sh [uscita.apk]
#
# Con SPLIT=1 scrive anche un APK per architettura accanto all'universale
# (uscita-arm64-v8a.apk, uscita-armeabi-v7a.apk): stesso versionCode, più leggeri.
#
# Richiede: Rust con i target aarch64-linux-android e armv7-linux-androideabi, cargo-apk 0.10, JDK 17, Android SDK
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
# ANDROID_SDK_ROOT è deprecata (cargo-apk lo segnala): basta ANDROID_HOME
unset ANDROID_SDK_ROOT

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
mapfile -d '' -t JAVA_SRCS < <(find "$ROOT/android/java" -name '*.java' -print0)
javac --release 17 -cp "$JAR" -d "$W/classes" "${JAVA_SRCS[@]}"
mapfile -d '' -t CLASS_FILES < <(find "$W/classes" -name '*.class' -print0)
"$BT/d8" --release --min-api "$MIN_API" --lib "$JAR" --output "$W/dex" "${CLASS_FILES[@]}"
"$BT/aapt" package -f -F "$W/app.unaligned.apk" -M "$W/AndroidManifest.xml" -S "$ROOT/android/res" -I "$JAR"
cp "$W/dex/classes.dex" "$W/apk/"
cp -r "$B/lib" "$W/apk/"
mkdir -p "$(dirname "$OUT")"

# make <uscita> <architetture…>: base (manifest e risorse) + classes.dex + librerie scelte
make_apk() {
  local out="$1"; shift
  local libs=()
  for abi in "$@"; do
    [ -d "$W/apk/lib/$abi" ] || { echo "manca la libreria per $abi"; exit 1; }
    mapfile -t abi_libs < <(cd "$W/apk" && find "lib/$abi" -type f | sort)
    libs+=("${abi_libs[@]}")
  done
  cp "$W/app.unaligned.apk" "$W/one.apk"
  (cd "$W/apk" && "$BT/aapt" add "$W/one.apk" classes.dex "${libs[@]}" >/dev/null)
  "$BT/zipalign" -f -p 4 "$W/one.apk" "$W/one.aligned.apk"
  if [ -n "${KEYSTORE:-}" ] && [ -n "${KEYSTORE_PASSWORD:-}" ]; then
    "$BT/apksigner" sign --ks "$KEYSTORE" --ks-pass env:KEYSTORE_PASSWORD --key-pass env:KEYSTORE_PASSWORD \
      --out "$out" "$W/one.aligned.apk"
    "$BT/apksigner" verify "$out"
    echo "APK firmato: $out"
  else
    cp "$W/one.aligned.apk" "$out"
    echo "APK non firmato: $out"
  fi
  "$BT/aapt" dump badging "$out" | grep -E "^(package|sdkVersion|native-code)"
}

mapfile -t ABIS < <(ls -1 "$W/apk/lib")
make_apk "$OUT" "${ABIS[@]}"
if [ "${SPLIT:-0}" = 1 ] && [ "${#ABIS[@]}" -gt 1 ]; then
  for abi in "${ABIS[@]}"; do
    make_apk "${OUT%.apk}-$abi.apk" "$abi"
  done
fi
