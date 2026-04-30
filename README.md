# balchat

> Chat 1:1 y grupos n-way **cifrados E2E con MLS**, transportado **sobre Tor onion services v3**,
> con **mensajería offline** vía un relay no-confiable. Sin servidores centrales obligatorios,
> sin números de teléfono, sin email. **Todo en Rust** + UI Tauri 2 (desktop + Android).

> ⚠️ **Estado: prototipo / spike funcional.** El protocolo está implementado, los binarios
> compilan en macOS/Linux/Android y el flujo end-to-end funciona, pero **no hay auditoría
> de seguridad**, los KAT formales del protocolo no están escritos, y faltan piezas
> (iOS, Welcome offline, multi-ABI APK). No usar en escenarios reales hasta entonces.

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
  hasta 14 MiB por mensaje (límite MLS).
- ✓ **Notificaciones del sistema** (`tauri-plugin-notification`) cuando llega un
  mensaje o archivo, con label del contacto.
- ✓ **Lock del vault** explícito (botón 🔒) **y auto-lock por inactividad**
  (5 min default — configurable en código).
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
export NDK_HOME=$ANDROID_HOME/ndk/30.0.13846066
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
export PATH=$PATH:$ANDROID_HOME/platform-tools

cd crates/balchat-tauri
cargo tauri android build --target aarch64 --apk

# APK queda en gen/android/app/build/outputs/apk/universal/release/
# Hay que zipalign + apksigner antes de instalar:

zipalign -p 4 app-universal-release-unsigned.apk balchat-signed.apk
apksigner sign --ks ~/.balchat/release.keystore --ks-pass pass:... balchat-signed.apk
adb install -r balchat-signed.apk
```

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
| 5a | Welcome offline vía relay (grupos con miembros offline) | pendiente |
| 5b | Multi-ABI APK (armv7 + x86_64 + x86) | pendiente — hoy sólo arm64 |
| 5c | iOS | pendiente |
| 5d | Auditoría de protocolo + KAT formales | pendiente |

---

## Limitaciones conocidas

- **Send-file vía relay limitado a ~14 MiB** (cabe en un solo MLS Application
  message). Files más grandes requieren chunking + reassembly que no está
  implementado.
- **Grupos n-way con miembros offline:** invitar a alguien que no está alcanzable
  deja a ese miembro con el epoch viejo. Welcome via relay (5a) lo resolverá.
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

Cobertura actual:
- `balchat-storage`: 6 tests — vault create/open, KV roundtrip, contacts upsert,
  passphrases con caracteres raros, vaults legacy sin salt, messages
  insert/list/limit, delete cascade.
- `balchat-core`: tests de Conversation con `tokio::io::duplex` (no requiere Tor)
  cubriendo handshake, application messages, Welcome, Resume, group commits.

---

## Roadmap

- [ ] Welcome offline vía relay (5a)
- [ ] Multi-ABI APK (5b)
- [ ] iOS (5c)
- [ ] Auditoría de protocolo + tests KAT (5d)
- [ ] Preview del último mensaje + badge "no leído" en la lista de contactos
- [ ] Setear relay desde la UI (hoy `set-my-relay` solo CLI)
- [ ] Publicar KeyPackage desde la UI (hoy `publish-kp` solo CLI)
- [ ] Limpieza de MLS group state al borrar contacto (fuga menor de storage)
- [ ] Chunking de archivos > 14 MiB
- [ ] Configurabilidad del timeout de auto-lock desde la UI
- [ ] Backup / export del vault encriptado
- [ ] FDroid build script

---

## Licencia

A definir. Por ahora código privado.
