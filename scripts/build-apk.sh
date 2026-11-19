#!/usr/bin/env bash
# Gera o APK do EMPETO (Rust + Slint + SQLite) sem Gradle, usando apenas SDK/NDK.
#
# Requisitos (seção 5.2):
#   - Rust (stable) com alvos Android:  rustup target add aarch64-linux-android x86_64-linux-android
#   - cargo-ndk:                        cargo install cargo-ndk
#   - Android SDK com build-tools >= 35 e platform android-36  (ANDROID_HOME)
#   - Android NDK r30+ (exigido pelo Skia pré-compilado)                                          (ANDROID_NDK_HOME ou $ANDROID_HOME/ndk/<versão>)
#   - JDK 17+ (javac, keytool)                                  (JAVA_HOME)
#
# Variáveis opcionais:
#   ABIS="arm64-v8a x86_64"   ABIs incluídas no APK (armeabi-v7a exige compilar o Skia do zero: defina ANDROID_NDK)
#   PROFILE=release|debug
#   VERSION_NAME / VERSION_CODE           sobrescrevem o manifesto
#   KEYSTORE, KEYSTORE_PASSWORD, KEY_ALIAS, KEY_PASSWORD   assinatura de release
#     (sem KEYSTORE, gera/usa uma chave de depuração local em build/debug.keystore)
#
# Saída: dist/workout-<versão>-<perfil>.apk

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="${PROFILE:-release}"
ABIS="${ABIS:-arm64-v8a x86_64}"
MIN_SDK=26
TARGET_SDK=35
COMPILE_SDK="${COMPILE_SDK:-36}"

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
[ -n "$SDK" ] && [ -d "$SDK" ] || { echo "Defina ANDROID_HOME"; exit 1; }
BT="${BUILD_TOOLS:-$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)}"
JAR="$SDK/platforms/android-$COMPILE_SDK/android.jar"
[ -f "$JAR" ] || JAR="$(ls -d "$SDK"/platforms/android-* | sort -V | tail -1)/android.jar"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(ls -d "$SDK"/ndk/* | sort -V | tail -1)}"
export ANDROID_HOME="$SDK"
export ANDROID_JAR="$JAR"   # usado pelo build.rs do backend Android do Slint
if [ -z "${JAVA_HOME:-}" ]; then
  JAVA_HOME="$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")"
  export JAVA_HOME
fi

VERSION_NAME="${VERSION_NAME:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
VERSION_CODE="${VERSION_CODE:-$(echo "$VERSION_NAME" | awk -F. '{print $1*10000 + $2*100 + $3}')}"
[ "$VERSION_CODE" -gt 0 ] || VERSION_CODE=1

OUT="$ROOT/build/android"
rm -rf "$OUT"
mkdir -p "$OUT"/{gen,classes,dex,apk/lib} "$ROOT/dist"

echo "==> SDK: $SDK"
echo "==> build-tools: $BT"
echo "==> android.jar: $JAR"
echo "==> NDK: $ANDROID_NDK_HOME"
echo "==> versão: $VERSION_NAME ($VERSION_CODE), perfil $PROFILE, ABIs: $ABIS"

# 1) Biblioteca nativa (Rust + Slint + SQLite)
TARGET_ARGS=()
for abi in $ABIS; do TARGET_ARGS+=(-t "$abi"); done
CARGO_PROFILE_ARGS=()
[ "$PROFILE" = "release" ] && CARGO_PROFILE_ARGS+=(--release)
cargo ndk "${TARGET_ARGS[@]}" -P "$MIN_SDK" -o "$OUT/apk/lib" build --lib "${CARGO_PROFILE_ARGS[@]}"
find "$OUT/apk/lib" -type f ! -name 'libworkout.so' -delete

# 1b) Verificação: nenhuma referência a C++ (libc++) pode ficar sem resolver.
#     O Skia pré-compilado exige a libc++ de NDK recente (r30+); com NDK antigo o link "passa",
#     mas o Android recusa carregar a biblioteca (UnsatisfiedLinkError em dlopen).
NM="$(ls "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/bin/llvm-nm | head -1)"
for so in "$OUT"/apk/lib/*/libworkout.so; do
  BAD="$("$NM" -D --undefined-only "$so" | grep -E '__ndk1|_ZNSt|__cxa_' | grep -v '@LIBC' || true)"
  if [ -n "$BAD" ]; then
    echo "ERRO: $so tem símbolos C++ não resolvidos (NDK incompatível com o Skia; use NDK r30+):"
    echo "$BAD" | head -20
    exit 1
  fi
done
echo "==> verificação de símbolos nativos: OK"
ls -la "$OUT"/apk/lib/*/

# 2) Recursos + manifesto
"$BT/aapt2" compile --dir android/res -o "$OUT/res.zip"
"$BT/aapt2" link -o "$OUT/base.apk" -I "$JAR" \
  --manifest android/AndroidManifest.xml \
  --java "$OUT/gen" \
  --min-sdk-version "$MIN_SDK" --target-sdk-version "$TARGET_SDK" \
  --version-code "$VERSION_CODE" --version-name "$VERSION_NAME" \
  --replace-version \
  "$OUT/res.zip"

# 3) Código Java (ponte Android) -> DEX
find android/java "$OUT/gen" -name '*.java' > "$OUT/sources.txt"
javac -nowarn -Xlint:-options -source 8 -target 8 -encoding UTF-8 \
  -bootclasspath "$JAR" -classpath "$JAR" -d "$OUT/classes" @"$OUT/sources.txt"
find "$OUT/classes" -name '*.class' > "$OUT/classes.txt"
"$BT/d8" --release --min-api "$MIN_SDK" --lib "$JAR" --output "$OUT/dex" @"$OUT/classes.txt"

# 4) Monta o APK (bibliotecas comprimidas; extraídas na instalação — APK menor)
cp "$OUT/base.apk" "$OUT/unsigned.apk"
(cd "$OUT/dex" && zip -q -X "$OUT/unsigned.apk" classes.dex)
(cd "$OUT/apk" && zip -q -X -9 -r "$OUT/unsigned.apk" lib)
"$BT/zipalign" -f -P 16 4 "$OUT/unsigned.apk" "$OUT/aligned.apk"

# 5) Assinatura
if [ -n "${KEYSTORE:-}" ]; then
  KS="$KEYSTORE"; KS_PASS="${KEYSTORE_PASSWORD:?}"; ALIAS="${KEY_ALIAS:?}"; KEY_PASS="${KEY_PASSWORD:-$KS_PASS}"
  SUFFIX="$PROFILE"
else
  KS="$ROOT/build/debug.keystore"; KS_PASS="android"; ALIAS="androiddebugkey"; KEY_PASS="android"
  SUFFIX="$PROFILE-debugsigned"
  if [ ! -f "$KS" ]; then
    keytool -genkeypair -keystore "$KS" -storepass android -keypass android -alias androiddebugkey \
      -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=EMPETO Debug,O=EMPETO,C=BR" >/dev/null 2>&1
  fi
fi
APK="$ROOT/dist/empeto-$VERSION_NAME-$SUFFIX.apk"
"$BT/apksigner" sign --ks "$KS" --ks-pass "pass:$KS_PASS" --ks-key-alias "$ALIAS" --key-pass "pass:$KEY_PASS" \
  --out "$APK" "$OUT/aligned.apk"
"$BT/apksigner" verify --print-certs "$APK" | head -3
rm -f "$APK.idsig"
echo "==> APK gerado: $APK ($(du -h "$APK" | cut -f1))"
