#!/usr/bin/env bash
# Reproducible Android build script para FDroid.
#
# Lo invoca el bot de FDroid (o un humano localmente) para producir el APK
# release sin firmar de balchat. La firma la hace fdroidserver con su keystore;
# este script solo se preocupa de generar bytes idénticos en cada build.
#
# Pre-requisitos:
#   - Rust + targets aarch64-linux-android (rustup target add)
#   - Android SDK + NDK (env vars ANDROID_HOME, NDK_HOME)
#   - JDK 21 (env var JAVA_HOME)
#   - Node 20+ con npm
#   - cargo-tauri 2.10+
#
# Variables opcionales:
#   - BALCHAT_TARGETS = "aarch64" (default) | "aarch64,armv7,x86_64,i686"
#
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

echo "[fdroid] root: $ROOT"
echo "[fdroid] commit: $(git rev-parse HEAD 2>/dev/null || echo '(not a git repo)')"

# Sanity checks
for v in ANDROID_HOME NDK_HOME JAVA_HOME; do
  if [ -z "${!v:-}" ]; then
    echo "[fdroid] ERROR: $v no está definida" >&2
    exit 1
  fi
done

if ! command -v cargo-tauri >/dev/null 2>&1 && ! command -v tauri >/dev/null 2>&1; then
  echo "[fdroid] cargo-tauri no encontrado; instalando..."
  cargo install tauri-cli@2.10
fi

TARGETS="${BALCHAT_TARGETS:-aarch64}"
TARGET_ARGS=()
IFS=',' read -ra TGTS <<< "$TARGETS"
for t in "${TGTS[@]}"; do
  TARGET_ARGS+=(--target "$t")
  rustup target add "${t}-linux-android" || true
done

echo "[fdroid] targets: ${TGTS[*]}"

# 1. Frontend (Svelte) — npm ci es determinístico desde package-lock.json.
echo "[fdroid] building Svelte UI..."
pushd crates/balchat-tauri/ui >/dev/null
npm ci
npm run build
popd >/dev/null

# 2. Android scaffolding (idempotente).
echo "[fdroid] tauri android init..."
pushd crates/balchat-tauri >/dev/null
cargo tauri android init --ci 2>/dev/null || true

# 3. Build APK release.
echo "[fdroid] tauri android build (release)..."
cargo tauri android build "${TARGET_ARGS[@]}" --apk

OUTDIR="gen/android/app/build/outputs/apk/universal/release"
APK="$(ls -1 "$OUTDIR"/*.apk 2>/dev/null | head -1 || true)"
popd >/dev/null

if [ -z "$APK" ]; then
  echo "[fdroid] ERROR: APK no encontrado en $OUTDIR" >&2
  exit 1
fi

ABS_APK="$ROOT/crates/balchat-tauri/$APK"
SIZE=$(stat -f%z "$ABS_APK" 2>/dev/null || stat -c%s "$ABS_APK")
SHA=$(shasum -a 256 "$ABS_APK" | awk '{print $1}')

echo "[fdroid] OK"
echo "[fdroid] artefacto: $ABS_APK"
echo "[fdroid] tamaño:    $SIZE bytes"
echo "[fdroid] sha256:    $SHA"
