# Android multi-ABI build

Hoy el APK release de balchat está construido **solo para aarch64** (ARM64,
cubre ~99% de los devices Android post-2018). Este documento describe cómo
agregar soporte multi-ABI para distribuir un APK universal o splitear por ABI:

- `arm64-v8a` (aarch64) — ya soportado
- `armeabi-v7a` (armv7) — phones pre-2018, smart TVs viejas
- `x86_64` — emuladores Android, Chromebooks, BlueStacks
- `x86` — muy raro hoy en hardware real, pero requerido por Play Store

## Estado al día de hoy (2026-04)

`cargo tauri android build --target aarch64 --apk` funciona end-to-end. Los
otros targets fallan en el cross-compile de OpenSSL (vendored vía
`bundled-sqlcipher-vendored-openssl`).

El error específico observado:

```
make: aarch64-linux-android-ranlib: No such file or directory
```

(o el equivalente para `x86_64`, `i686`, `armv7a`). El NDK provee
`llvm-ranlib` y `llvm-ar`, pero el script de build de OpenSSL invoca
binarios con el nombre tradicional (`<arch>-linux-android-ranlib`). Esto se
puede arreglar con symlinks pero hay que hacerlo para cada arch.

## Setup propuesto

### 1. Symlinks por ABI

Para cada ABI que querés soportar, crear symlinks en `~/.cargo/bin/` que
apunten a los binarios `llvm-*` del NDK:

```bash
NDK_BIN="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
# Para aarch64 (ya existe en setups que funcionan):
ln -sf "$NDK_BIN/llvm-ranlib"   ~/.cargo/bin/aarch64-linux-android-ranlib
ln -sf "$NDK_BIN/llvm-ar"       ~/.cargo/bin/aarch64-linux-android-ar
# Repetir para armv7, x86_64, i686:
for abi in armv7a-linux-androideabi x86_64-linux-android i686-linux-android; do
  ln -sf "$NDK_BIN/llvm-ranlib" "$HOME/.cargo/bin/${abi}-ranlib"
  ln -sf "$NDK_BIN/llvm-ar"     "$HOME/.cargo/bin/${abi}-ar"
done
```

(En Linux: cambiar `darwin-x86_64` por `linux-x86_64`.)

### 2. Targets Rust

```bash
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi
rustup target add x86_64-linux-android
rustup target add i686-linux-android
```

### 3. Build

```bash
cargo tauri android build \
  --target aarch64 \
  --target armv7 \
  --target x86_64 \
  --target i686 \
  --apk
```

`tauri-cli` 2.10+ produce un APK universal con todas las ABIs incluidas, o
bien un APK por ABI (controlable con `--split-per-abi`).

### 4. Tamaños esperados

| ABI | tamaño debug | tamaño release stripped |
|---|---:|---:|
| aarch64 (arm64-v8a) | 871 MB | 40 MB |
| armv7 (armeabi-v7a) | TODO   | TODO   |
| x86_64              | TODO   | TODO   |
| i686                | TODO   | TODO   |

(Los TODOs se completan cuando se valide el build multi-ABI en una máquina
con NDK.)

### 5. APK split vs universal

- **APK universal** (~150 MB): un solo `.apk` con las 4 ABIs. Más simple
  para sideload, pero un usuario aarch64 baja 4× lo que necesita.
- **APK split per-ABI** (~40 MB c/u): el bot de FDroid / Play Store sirve
  el correcto según el device. Recomendado para distribución masiva.

## Bloqueadores conocidos por ABI

### armv7
- OpenSSL cross-compile: además de los symlinks, requiere `--target=armv7a-linux-androideabi-clang`
  (versión NDK), no `armv7-linux-androideabi-clang`. Verificar el path
  exacto en `$NDK_BIN`.

### x86_64 / i686
- Si el device target tiene Android < API 24, los símbolos de ARM no
  funcionan en x86 → no es solo recompilar, hay que verificar runtime.
- Algunos crates en el dependency tree (`tor-*`) usan `cfg(target_os = "android")`
  pero asumen ARM en algunos paths; revisar build logs.

## Verificación

Una vez el APK multi-ABI está construido:

```bash
# Inspeccionar las ABIs presentes:
unzip -l balchat-multi.apk | grep "lib/.*\.so"
# Debe mostrar lib/arm64-v8a/, lib/armeabi-v7a/, lib/x86_64/, lib/x86/

# Test en emulador x86_64:
emulator -avd Pixel_API_34 -no-window &
adb install balchat-multi.apk
adb shell am start -n com.balchat.desktop/.MainActivity
```

## Ver también

- [`fdroid/build.sh`](../fdroid/build.sh) — script de build reproducible
  que hoy hace solo aarch64; cambiar `BALCHAT_TARGETS=aarch64,armv7,x86_64,i686`
  cuando se complete el setup.
- [README.md](../README.md) sección "APK Android" — instrucciones single-ABI.
