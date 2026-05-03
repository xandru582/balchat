# balchat

[![Release](https://img.shields.io/github/v/release/xandru582/balchat?style=flat-square&color=b14bff)](https://github.com/xandru582/balchat/releases/latest)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-007aff?style=flat-square)](LICENSE)
[![Rust](https://img.shields.io/badge/built%20with-Rust-dea584?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri 2](https://img.shields.io/badge/UI-Tauri%202-24c8db?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
[![Tor onion services](https://img.shields.io/badge/network-Tor%20onion%20v3-7d4698?style=flat-square)](https://community.torproject.org/onion-services/)
[![MLS RFC 9420](https://img.shields.io/badge/crypto-MLS%20%C2%A7RFC%209420-2dffa6?style=flat-square)](https://datatracker.ietf.org/doc/rfc9420/)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%C2%B7%20Android%20%C2%B7%20Linux%20%C2%B7%20Windows-ffd84a?style=flat-square)](https://github.com/xandru582/balchat/releases/latest)

> Chat 1:1 y grupos n-way **cifrados E2E con MLS**, transportado **sobre Tor onion services v3**,
> con **mensajería offline** vía un relay no-confiable. Sin servidores centrales obligatorios,
> sin números de teléfono, sin email. **Todo en Rust** + UI Tauri 2 (desktop + Android).

**🌐 Web · descargas**: <https://baluniverse.pages.dev/#balchat>
**📦 Última release**: <https://github.com/xandru582/balchat/releases/latest>

> ⚠️ **Estado: prototipo / spike funcional.** Los cuatro binarios principales
> (CLI + relay + desktop + APK) compilan release y arrancan en
> **macOS / Android / Linux / Windows**. Falta auditoría externa de seguridad,
> y **iOS** queda scaffolded pero sin verificar en device (ver
> [docs/ios-build.md](docs/ios-build.md)). No usar en escenarios reales hasta
> que haya audit.

## Quickstart usuario

1. **Descarga** la app para tu plataforma desde [releases/latest](https://github.com/xandru582/balchat/releases/latest):
   - **macOS** (Apple Silicon): `balchat.dmg`
   - **Android** (arm64): `balchat.apk`
2. **macOS**: si Gatekeeper dice *"está dañado"*, abre Terminal y ejecuta `xattr -cr /Applications/balchat.app` después de copiar la app a Aplicaciones.
3. **Android**: permite *fuentes desconocidas* en tu navegador antes de tocar el APK.
4. Crea tu cuenta con una contraseña que solo tú sepas.
5. Espera unos segundos a que se prepare tu *código de chat*.
6. Comparte tu código con tu contacto. Cuando os añadáis mutuamente, uno toca *Conectar* y a chatear.

¿Quieres montar tu propio buzón offline en una VPS? Está en
[`deploy/relay/`](deploy/relay/) — `install.sh` + systemd unit hardened.

| Target | Verificado en esta máquina | Cómo |
|---|---|---|
| macOS aarch64 (Apple Silicon) | ✅ `.app` 35 MB · `.dmg` 62 MB | `cargo tauri build` nativo |
| Android aarch64 (arm64-v8a) | ✅ APK 43 MB firmado (debug.keystore) | `cargo tauri android build --target aarch64 --apk` con NDK 30 |
| Linux x86_64 (`*-unknown-linux-gnu`) | ✅ ELF 26 MB / 21 MB stripped | `cargo zigbuild --target x86_64-unknown-linux-gnu` |
| Windows x86_64 (`*-pc-windows-gnu`) | ✅ PE32+ 32 MB / 30 MB | `cargo zigbuild --target x86_64-pc-windows-gnu` |
| iOS (aarch64-apple-ios) | ⚠️ scaffolded | requiere Xcode.app + device, ver [docs/ios-build.md](docs/ios-build.md) |
| Android multi-ABI (armv7/x86_64/i686) | ⚠️ scaffolded | requiere ajustes de cross-compile OpenSSL, ver [docs/android-multi-abi.md](docs/android-multi-abi.md) |

---

## Tabla de contenidos

- [Qué es](#qué-es)
- [Características](#características)
- [Arquitectura](#arquitectura)
- [Stack tecnológico](#stack-tecnológico)
- [Workspace](#workspace)
- [Build](#build)
- [Uso — CLI](#uso--cli)
- [Uso — Desktop UI](#uso--desktop-ui)
- [Uso — Android](#uso--android)
- [Estado por fases](#estado-por-fases)
- [Limitaciones conocidas](#limitaciones-conocidas)
- [Threat model](#threat-model)
- [Tests](#tests)
- [Roadmap](#roadmap)

---

## Qué es

balchat es una app de mensajería diseñada con tres principios:

1. **Sin punto central de control.** El "servidor" es el otro peer; un relay opcional
   maneja mensajes offline pero ve solo ciphertext y queue ids opacos.
2. **Identidad criptográfica, no fiscal.** Tu cuenta es una signing key MLS + un
   onion service v3. No hay registro, no hay teléfono.
3. **Transporte que oculta metadatos.** Toda conexión peer-to-peer va por circuitos Tor
   con onion services v3 (NAT-traversal y no-IP-leak gratis).

---

## Características

### Cifrado y autenticación

- **MLS (RFC 9420)** vía [`openmls`](https://github.com/openmls/openmls) — forward
  secrecy + post-compromise security, con grupos n-way nativos.
- **Argon2id** para derivar la clave de cifrado del vault desde la passphrase
  (con salt random persistido aparte).
- **SQLCipher** (rusqlite + bundled-sqlcipher-vendored-openssl) para todo lo que
  toca disco: identidad, contactos, grupos, histórico de chat, estado de relay.
- **Cross-sign opcional** (`--pubkey`): podés pinear la signing key esperada de un
  contacto out-of-band; el handshake aborta si el peer presenta otra.

### Transporte

- **Tor onion services v3** vía [`arti-client`](https://crates.io/crates/arti-client)
  (Tor pure-Rust). No hay daemon `tor` externo — el binario hace bootstrap.
- **Wire format** CBOR length-prefixed sobre el stream onion.
- **Send con fallback** directo → relay: si el peer no responde en 45 s, el mismo
  comando deja el blob en el relay para que lo recoja cuando se conecte.

### Mensajería offline

- **Relay no-confiable** (`balchat-relay`): expone su propio onion, almacena blobs
  cifrados indexados por queue_id (256 bits random) hasta `--max-age`. No autentica
  remitentes — la "credencial" es conocer el queue_id, y los blobs son ciphertext
  MLS, así que un atacante con queue_id no lee contenido (pero puede borrar/spammear).
- **KeyPackage pool** (`PutKeyPackage` / `ConsumeKeyPackage`): permite hacer un
  bootstrap 1:1 cuando el invitado está offline — A publica un KeyPackage, B lo
  consume cuando aparece y deriva el grupo MLS sin handshake live.

### Persistencia

- **Vault SQLCipher** con tablas: `kv` (singletons), `contacts`, `groups`,
  `group_members`, `relay_state` (last_seq por queue), `messages` (histórico
  sent/received con timestamps).
- **MLS group state persistido** vía `MemoryStorage` serializado al vault: una
  conversación sobrevive al restart del proceso sin perder ratchet/epoch.
- **Migraciones idempotentes** (`CREATE IF NOT EXISTS` + `ALTER ... ADD COLUMN`):
  vaults de versiones viejas siguen abriendo.

### UI Tauri 2 (desktop + Android)

Single Svelte 5 SPA (`crates/balchat-tauri/ui/`) sobre Tauri 2. Lo que la UI ya hace:

- ✓ Login con passphrase + flujo "crear vault" / "abrir vault" detectado automáticamente.
- ✓ **Auto-arranque del daemon** al unlock — bootstrap Arti + onion service + poll
  relay sin tener que apretar nada.
- ✓ Lista de contactos en el sidebar; **agregar** (form plegable con label /
  onion / relay / queue / pubkey) y **borrar** (con confirm + cascade del
  histórico) desde la UI, sin tocar CLI.
- ✓ Chat panel con **histórico persistido** (carga últimos 200 mensajes del peer
  al seleccionarlo) + **timestamps `[HH:MM]`** locales.
- ✓ **Auto-scroll** al final cuando llegan o se envían mensajes.
- ✓ **Envío de archivos** con file picker nativo (`tauri-plugin-dialog`),
  con **chunking automático para archivos > 12 MiB** (split en chunks de 8 MiB,
  reensamblaje atómico en disco con spool en `inbox/_partial/`).
- ✓ **Notificaciones del sistema** (`tauri-plugin-notification`) cuando llega un
  mensaje o archivo, con label del contacto.
- ✓ **Preview del último mensaje + badge no-leído** en la lista de contactos;
  se ordena por actividad reciente.
- ✓ **Lock del vault** explícito (botón 🔒) y **auto-lock por inactividad
  configurable** desde la UI (default 5 min, 0 = desactivado).
- ✓ **Panel Settings (⚙)**: cambiar mi relay, publicar pool de KeyPackages,
  ajustar auto-lock, exportar backup del vault — todo sin tocar la CLI.
- ✓ Botón **copiar mi onion** al portapapeles para compartir tu dirección.
- ✓ Indicador de status del daemon (`idle / starting / running / error`).

### Android

- APK aarch64 firmable y instalable (probado en Pixel 9 emulator API 36).
- **Foreground service** (`BalchatForegroundService` + notificación persistente)
  que mantiene el proceso vivo mientras la app está en background.
- Permisos `POST_NOTIFICATIONS` (Android 13+), `FOREGROUND_SERVICE`,
  `FOREGROUND_SERVICE_DATA_SYNC`, `WAKE_LOCK` declarados.
- Vault almacenado en `app_local_data_dir` (sandbox de la app).

---

## Arquitectura

```
┌─────────────────────────────────────────────────────────────────────┐
│                       balchat-cli  /  balchat-tauri                 │
│              (CLI binary)            (Svelte UI ↔ Rust commands)    │
└────────┬───────────────────────────────────────────┬────────────────┘
         │                                           │
         ▼                                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          balchat-core                               │
│  Identity (MLS provider + signing key)                              │
│  Conversation<S>  (handshake / Application msgs / Welcomes / Resume)│
│  Endpoint  (Arti bootstrap, dial onion, host onion)                 │
│  RelayClient  (Put / Get / PutKeyPackage / ConsumeKeyPackage)       │
└─────────────┬───────────────────────────────────┬───────────────────┘
              │                                   │
              ▼                                   ▼
   ┌─────────────────────┐             ┌────────────────────────┐
   │  balchat-storage    │             │    balchat-relay-proto │
   │  Vault SQLCipher    │             │  (CBOR enums)          │
   │  · kv, contacts,    │             └──┬─────────────────────┘
   │    groups, members, │                │
   │    messages,        │                ▼
   │    relay_state      │        ┌────────────────────────┐
   │  · Argon2id KDF     │        │   balchat-relay        │
   └─────────────────────┘        │   (untrusted onion)    │
                                  └────────────────────────┘
```

**Data flow (mensaje 1:1 con peer offline):**

```
A: send_text("hola")
   → Conversation.app_message(payload)  → MLS Application message (ciphertext)
   → Endpoint.dial(B.onion) [timeout 45s]  ✗ falla
   → RelayClient.put(B.relay_onion, B.queue_id, blob)  ✓
   → vault.insert_message("sent", "text", "hola")

B: watch loop (cada 30s)
   → RelayClient.get(my_relay, my_queue, last_seq, max=64)
   → MlsGroup::load(group_id) → process_message → AppPayload::Text("hola")
   → resolve contact_for_group_id → vault.insert_message("received", ...)
   → emit balchat://message → notification al sistema
```

---

## Stack tecnológico

| Capa | Crate / lib | Versión |
|---|---|---|
| MLS | `openmls` + `openmls_traits` | 0.8 |
| Tor | `arti-client` + `tor-rtcompat` (rustls) | 0.41 |
| DB cifrada | `rusqlite` con `bundled-sqlcipher-vendored-openssl` | 0.38 |
| KDF | `argon2` | 0.5 |
| Wire codec | `ciborium` (CBOR) | — |
| UI shell | Tauri 2 + plugins (notification / dialog / fs) | 2.x |
| UI frontend | Svelte 5 + Vite | 5 / 5 |
| Async runtime | Tokio | 1.x |
| Mobile | tauri-cli android + Android SDK / NDK | 2.10 / 30.x |

---

## Workspace

```
balchat-storage      vault SQLCipher (Argon2id) + esquema kv/contacts/
                     groups/group_members/messages/relay_state
balchat-relay-proto  tipos del protocolo cliente↔relay (CBOR length-prefixed)
balchat-relay        binario `balchat-relay`: onion service untrusted
balchat-core         transport (Arti) + Identity (MLS) + Conversation +
                     RelayClient + ResumeResolver
balchat-cli          binario `balchat`: 1:1, grupos, file transfer, relay,
                     daemon, host/connect, KeyPackage pool
balchat-tauri        UI Tauri 2 + Svelte 5 (desktop + Android)
spike-mls            Fase 0: handshake MLS + cifrado de mensaje (sin red)
spike-tor            Fase 0: onion service v3 con echo, todo en arti
```

---

## Build

### Workspace completo (CLI + relay + desktop)

```bash
cargo build --workspace --release
```

Binarios:
- `target/release/balchat` — CLI principal
- `target/release/balchat-relay` — relay server
- `target/release/balchat-desktop` — UI desktop (sólo bundle Tauri arma el `.app`)

### Frontend Svelte (necesario antes del bundle Tauri o el APK)

```bash
cd crates/balchat-tauri/ui && npm install && npm run build
```

### Bundle desktop (Tauri 2)

```bash
cd crates/balchat-tauri && cargo tauri build
```

### APK Android (aarch64, requiere SDK 36 + NDK 30)

```bash
export ANDROID_HOME=~/Library/Android/sdk
export NDK_HOME=$ANDROID_HOME/ndk/30.0.14904198      # ajustar a la versión instalada
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
export PATH=$PATH:$ANDROID_HOME/platform-tools

# Symlinks para que el OpenSSL vendored encuentre el toolchain del NDK:
NDK_BIN=$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin
for arch in aarch64-linux-android armv7a-linux-androideabi i686-linux-android x86_64-linux-android; do
  for tool in ar ranlib nm strip; do
    ln -sf "$NDK_BIN/llvm-$tool" "$HOME/.cargo/bin/$arch-$tool"
  done
done

cd crates/balchat-tauri
cargo tauri android build --target aarch64 --apk

# APK queda en gen/android/app/build/outputs/apk/universal/release/
# Hay que zipalign + apksigner antes de instalar:
APK=gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
$ANDROID_HOME/build-tools/37.0.0/zipalign -p -f 4 "$APK" /tmp/balchat-aligned.apk
$ANDROID_HOME/build-tools/37.0.0/apksigner sign \
    --ks ~/.android/debug.keystore --ks-pass pass:android \
    --key-pass pass:android --ks-key-alias androiddebugkey \
    --out /tmp/balchat-signed.apk /tmp/balchat-aligned.apk
adb install -r /tmp/balchat-signed.apk
```

**Verificado en hardware:** APK release aarch64 firmado, 43 MB
(`lib/arm64-v8a/libbalchat_mobile.so` 40 MB stripped).

### Cross-compile a Linux y Windows (desde macOS)

Usamos [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) que
delega a `zig cc` como cross-linker; resuelve el OpenSSL vendored sin instalar
GCC cross-toolchains separados.

```bash
brew install zig                                 # ~120 MB
cargo install cargo-zigbuild                     # ~30 s
rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu

# Linux x86_64 (CLI + relay):
cargo zigbuild --release --target x86_64-unknown-linux-gnu \
    -p balchat-cli -p balchat-relay
# → target/x86_64-unknown-linux-gnu/release/balchat (~26 MB ELF stripped)

# Windows x86_64 (CLI + relay):
cargo zigbuild --release --target x86_64-pc-windows-gnu \
    -p balchat-cli -p balchat-relay
# → target/x86_64-pc-windows-gnu/release/balchat.exe
```

> El bundle Tauri desktop (.deb / .AppImage / .msi) requiere correr el build
> en su SO nativo — webview2 y los plugins de Tauri no cross-compilan. La
> recomendación es CI con runners Linux/Windows, o construir manualmente
> en cada SO. El binario CLI sí cross-compila desde macOS sin problema.

---

## Uso — CLI

### Bootstrap inicial

```bash
balchat init --label me                      # crea ~/.balchat/vault.db
                                             # pide passphrase (mín 4 chars)
balchat my-id                                # imprime ONION + QUEUE + RELAY + PUBKEY
```

### Quickstart 1:1 live

```bash
# Peer A
balchat host                                 # levanta tu .onion y espera UNA conexión

# Peer B
balchat connect xxxxxxxxxxxx.onion:1234      # dial + handshake MLS + REPL chat
```

### Mensajería offline (relay)

```bash
# Operador del relay (cualquiera con un host Tor):
balchat-relay --data-dir ~/.balchat-relay --nickname r1
# imprime [relay] onion: yyyy.onion

# Cada peer:
balchat set-my-relay yyyy.onion:1235

# A invita a B out-of-band (chat, email, QR, etc.):
balchat add-contact bob bob.onion:1234 \
    --queue <hex de 32 bytes> \
    --relay yyyy.onion:1235 \
    --pubkey <hex>            # opcional — verifica MLS signing key

# Envío (intenta directo; cae a relay si peer offline):
balchat send bob "hola desde offline"
balchat send-file bob ~/Documents/foto.jpg

# Daemon que polea relay + acepta conexiones directas:
balchat watch --listen --interval 30
```

### Bootstrap offline 1:1 vía KeyPackage pool

```bash
# B publica un KeyPackage en su relay (no requiere estar online):
balchat publish-kp

# A consume el KeyPackage y crea el grupo MLS sin handshake live:
balchat bootstrap-1to1 bob
# A puede enviar mensajes; B los lee cuando se conecta a su relay.
```

### Grupos n-way

```bash
balchat create-group amigos
balchat invite amigos bob               # B online → handshake live + Welcome
balchat invite amigos carol             # commit auto-diseminado a B
balchat send-group amigos "hola a todos"
balchat groups                          # lista grupos + miembros
```

### Comandos completos

```
balchat init [--label NAME]
balchat my-id
balchat set-my-relay <onion>
balchat add-contact <label> <onion> [--queue HEX] [--relay ONION] [--pubkey HEX]
balchat list-contacts
balchat host [--nickname NAME]
balchat connect <peer-or-onion> [--nickname NAME]
balchat send <peer> <texto>
balchat send-file <peer> <path>
balchat poll [--max N]
balchat watch [--interval SEC] [--listen] [--nickname NAME] [--max N]
balchat publish-kp
balchat bootstrap-1to1 <peer>
balchat create-group <label>
balchat groups
balchat invite <group> <peer>
balchat send-group <group> <texto>
```

---

## Uso — Desktop UI

```bash
cd crates/balchat-tauri/ui && npm install && npm run build
cd .. && cargo tauri dev   # o `cargo tauri build` para .app/.dmg/.AppImage
```

Flujo:

1. Si no hay vault, te pide crear uno (label + passphrase x2).
2. Si ya hay, pedís passphrase. El daemon **se auto-arranca** después del unlock.
3. El sidebar muestra contactos. Tocá `+` para agregar uno, `×` para borrarlo
   (con confirmación, cascade del histórico).
4. Click en un contacto → chat panel con histórico, timestamps, auto-scroll.
5. **Adjuntar archivos** con el botón "Archivo" (file picker del SO).
6. **Notificaciones del sistema** cuando llegan mensajes mientras la app está
   en background.
7. **Lock manual** con el botón 🔒 o **auto-lock** tras 5 min sin interacción.

---

## Uso — Android

Mismo flujo que desktop. Adicionalmente:

- Al lanzarse, arranca el `BalchatForegroundService` con notificación persistente
  ("balchat está corriendo") para que Android no mate el proceso en background.
- Permisos pedidos al primer launch: `POST_NOTIFICATIONS` (Android 13+).
- Vault en `/data/data/com.balchat.desktop/files/vault.db` (sandbox de la app).

Para entornos de dev se incluyó el flujo manual de zipalign + apksigner con un
keystore propio (`~/.balchat/release.keystore`, RSA 4096, 100 años de validez).
**Para producción real habría que firmarlo con un keystore de release controlado
y publicarlo via Play Store o como FDroid build.**

---

## Estado por fases

| Fase | Descripción | Estado |
|---|---|---|
| 0 | spike MLS + spike Tor | ✓ |
| 1a | balchat-core + CLI 1:1 (in-memory) | ✓ |
| 1b | vault SQLCipher + identidad persistente | ✓ |
| 2a | tabla `contacts` + `add-contact` / `list-contacts` | ✓ |
| 2b | MlsGroup persistente + Resume handshake | ✓ |
| 2c | relay no-confiable + send fallback | ✓ |
| 3a | auto-poll (`watch`) | ✓ |
| 3b | `watch --listen` + accept entrantes | ✓ |
| 3c | file transfer (`AppPayload::File`) | ✓ |
| 3d | UI Tauri 2 + Svelte 5 (MVP login + chat) | ✓ |
| 3e | Argon2id + suspicious-mismatch + `--pubkey` | ✓ |
| 3f | Mobile Android — APK firmable + foreground service | ✓ |
| 3g | Grupos n-way (add_members + Commit dissemination) | ✓ |
| 3h | KeyPackage pool para bootstrap offline 1:1 | ✓ |
| 4a | UI: agregar/borrar contactos, auto-arranque daemon, lock | ✓ |
| 4b | UI: histórico persistido + timestamps + auto-scroll | ✓ |
| 4c | UI: notificaciones + file transfer + copiar onion | ✓ |
| 5a | Welcome offline vía relay (grupos con miembros offline) | ✓ |
| 5b | Commit dissemination offline (existing members reciben Add Commit por relay) | ✓ |
| 5c | Settings UI (set-relay, publish KP, auto-lock, export vault) | ✓ |
| 5d | Preview último mensaje + badge no-leído + delete con cleanup MLS | ✓ |
| 5e | Chunking de archivos > 12 MiB (split 8 MiB + spool reassembly) | ✓ |
| 5f | KAT tests del wire format (balchat-core + relay-proto) | ✓ |
| 5g | FDroid build script + metadata (`fdroid/`) | ✓ |
| 5h | Multi-ABI APK (armv7 + x86_64 + x86) | scaffolded — [docs/android-multi-abi.md](docs/android-multi-abi.md) |
| 5i | iOS | scaffolded — [docs/ios-build.md](docs/ios-build.md) |
| 5j | Auditoría externa de protocolo | pendiente |

---

## Limitaciones conocidas

- **Receivers viejos no leen `FileChunk`**: senders con la versión 5e
  arriba siguen mandando archivos chicos (≤ 12 MiB) como `AppPayload::File`,
  pero un peer que todavía corra una versión < 5e va a fallar al recibir
  un chunk de un archivo grande. El sender ve el descifrado fallar en el
  log del peer; no hay re-fall back automático.
- **Relay no autentica:** queue_id es la credencial. Conocerlo permite leer/borrar
  blobs (no descifrarlos). Los queue_ids se distribuyen out-of-band entre peers
  que ya se confían.
- **Cross-sign (`--pubkey`)** sólo lo aplica el lado Initiator del fresh handshake
  hoy. El Acceptor confía en lo que llega vía MLS (que ya verifica integridad
  contra la signing key del KeyPackage, pero no contra una expectativa preestablecida).
- **APK sólo aarch64** (cubre 99 % de Android desde ~2018). Multi-ABI build pendiente.
- **iOS no implementado.**
- **No hay auditoría de seguridad.** El protocolo se apoya en MLS estándar pero la
  capa de transporte y el handling del relay no han sido revisados por nadie
  externo.

---

## Threat model

| Adversario | ¿Qué puede hacer? | ¿Qué NO puede hacer? |
|---|---|---|
| **Network-level** (ISP, gobierno, Wi-Fi público) | Ver que estás usando Tor. | Ver con quién hablás (onion service v3 oculta IP del peer), ver contenido (MLS), correlacionar mensajes con tu IP. |
| **Relay operator** | Ver tamaños de blobs y timing. Borrar/spammear blobs (DoS). | Leer contenido (ciphertext MLS), saber a qué onion pertenece un queue_id. |
| **Atacante con tu queue_id** (filtración OOB) | Leer/borrar tus blobs en ese queue. | Descifrarlos. |
| **Compromiso del dispositivo** (post-quantum) | Leer mensajes futuros desde el comprometido en adelante. | Leer mensajes pasados (forward secrecy de MLS) — siempre que el ratchet haya avanzado. |
| **Compromiso del vault** (acceso al .db) | Nada sin la passphrase. | Forzar la passphrase con menos de 2^N intentos donde N depende de Argon2id (defaults memory=64 MiB, time=3, parallelism=4). |
| **MITM en el primer encuentro** | Si conoce tu signing key esperada — ninguno. Si no la conociste — puede MITM el primer handshake (TOFU). | MITM una conversación ya establecida (todo va por MLS sealed con keys derivadas). |

---

## Tests

```bash
cargo test --workspace                  # storage + core (sin Tor; usan DuplexStream)
```

Cobertura actual (30/30 passing):
- `balchat-storage` — 7 tests: vault create/open, KV roundtrip, contacts upsert,
  passphrases con caracteres raros, vaults legacy sin salt, messages
  insert/list/limit, delete cascade, **preview del último mensaje + unread
  count + mark_contact_read** (5d).
- `balchat-core` — 16 tests: identity roundtrip, KeyPackage tras restore,
  fresh handshake con texto y archivos, cross-sign mismatch aborta handshake,
  **Welcome offline vía KeyPackage pool** (5a), **Commit aplicado vía blob
  avanza el epoch del miembro existente** (5b), **delete_group idempotente
  limpia state MLS huérfano** (5d), **chunking de archivos out-of-order
  reensambla 25 MiB en 4 chunks** (5e), **KAT canónicos del wire format**:
  Frame::Bye 4 bytes exactos, Hello con/sin resume_group_id, KeyPackage,
  AppPayload::Text canonical CBOR, FileChunk roundtrip, max_size enforcement.
- `balchat-relay-proto` — 7 tests **KAT del protocolo cliente↔relay**:
  Put/Get request roundtrip, PutAck/GetReply/ConsumeKeyPackageReply
  (None vs Some) responses, send_recv_frame end-to-end, version pin.

---

## Roadmap

- [x] Welcome offline vía relay (5a)
- [x] Commit dissemination offline (5b)
- [x] Settings UI: set-relay, publish KeyPackages, auto-lock configurable, export vault (5c)
- [x] Preview último mensaje + badge no-leído + cleanup MLS al borrar contacto (5d)
- [x] Chunking de archivos > 12 MiB con spool de reensamblaje en disco (5e)
- [x] KAT tests del wire format en balchat-core y balchat-relay-proto (5f)
- [x] FDroid build script + metadata YAML (5g) → [`fdroid/`](fdroid/)
- [ ] **Multi-ABI APK** (5h) — config y docs listos en
      [docs/android-multi-abi.md](docs/android-multi-abi.md), pendiente verificar
      en máquina con NDK que el cross-compile de OpenSSL funciona para armv7/x86_64/i686.
- [ ] **iOS** (5i) — scaffolding documentado en
      [docs/ios-build.md](docs/ios-build.md); el path crítico es el handler
      de `BGProcessingTask` para poll-relay en background, no probado en device.
- [ ] **Auditoría externa** (5j) — pendiente. Los KATs de wire format
      mitigan regresiones internas pero no reemplazan una review de seguridad
      del protocolo end-to-end.

---

## Licencia

A definir. Por ahora código privado.
