# iOS build (5d, scaffolding)

Documenta el setup necesario para compilar balchat para iOS via Tauri 2.
**No verificado en hardware** todavía — este doc es el plan, no una receta
ya validada como Android.

## Pre-requisitos

- macOS reciente (Sonoma 14+ o Sequoia 15+).
- Xcode 16+ con Command Line Tools (`xcode-select --install`).
- Rust + targets iOS:
  ```bash
  rustup target add aarch64-apple-ios       # device físico (iPhone/iPad arm64)
  rustup target add aarch64-apple-ios-sim   # simulator en Apple Silicon (M1+)
  rustup target add x86_64-apple-ios        # simulator en Mac Intel
  ```
- `tauri-cli` 2.10+ (la misma versión que usamos para Android).
- Cuenta Apple Developer (gratuita alcanza para sideload + simulator;
  para distribución TestFlight/App Store requiere paid $99/year).

## Setup

```bash
cd crates/balchat-tauri
cargo tauri ios init        # genera gen/apple/balchat.xcodeproj
cargo tauri ios build       # produce IPA en gen/apple/build/
```

`tauri ios init` crea:

- `gen/apple/balchat.xcodeproj` — el proyecto Xcode generado.
- `gen/apple/balchat_iOS/Info.plist` — bundle id (`com.balchat.desktop`),
  permisos, capabilities.
- `gen/apple/Sources/balchat/` — sources Swift que invocan al `staticlib`
  Rust (`crate-type = ["staticlib", "cdylib", "rlib"]`).

## Bloqueadores conocidos / dependencias

### TLS stack
Ya estamos en `rustls` (no `native-tls`) por el cambio Android — iOS no
debería pegar contra `Security.framework` en el path del cross-compile.
Si aparece, agregar a `Cargo.toml`:

```toml
[target.'cfg(target_os = "ios")'.dependencies]
arti-client = { workspace = true, default-features = false, features = ["tokio", "rustls"] }
```

### SQLCipher / OpenSSL
`bundled-sqlcipher-vendored-openssl` debería funcionar en iOS — OpenSSL se
compila desde fuente como con Android. Verificar el primer build.

### Background lifecycle (CRÍTICO)
Esta es **la** limitación a la que se hace referencia en el README:

> Para iOS, el ciclo de vida exige relays para mensajería offline (la app
> no puede mantener Tor activo en background indefinidamente).

iOS mata procesos en background tras ~30s salvo capabilities específicas:
- `audio` — para audio playback
- `voip` — para llamadas (no aplica)
- `fetch` — para fetch periódico (sí aplica)
- `processing` (`BGTaskScheduler`) — tareas largas en background, _con_
  budget impredecible (sistema decide cuándo correr)

**Plan operativo balchat en iOS:**

1. **Modo foreground:** Tor activo, `Endpoint::host_onion` levantado,
   `RelayClient::get` cada 30s. Idéntico a desktop/Android.
2. **App va a background:** registramos un `BGProcessingTask` con identifier
   `com.balchat.desktop.poll`. El sistema lo invoca cuando hay batería
   y wifi disponibles (sin garantía de timing).
3. **BGProcessingTask handler:** abrir el vault con la passphrase ya
   cacheada en Keychain (¡con `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`!),
   bootstrap Arti rápido (puede fallar — el sistema no garantiza wifi),
   `RelayClient::get`, persist mensajes, emit notificación local. Cerrar
   Tor antes de que expire el budget (~30s típico).
4. **Push notifications (opcional):** registrar APNs para que peers que
   te quieran contactar notifiquen al device de despertarse — pero APNs
   requiere un servidor Apple-friendly que conozca el device token,
   incompatible con la propiedad "sin servidores centrales". Por ahora
   queda fuera de scope; se puede agregar como opt-in en una fase futura.

### Capabilities a setear en `Info.plist`

```xml
<key>UIBackgroundModes</key>
<array>
    <string>fetch</string>
    <string>processing</string>
</array>
<key>BGTaskSchedulerPermittedIdentifiers</key>
<array>
    <string>com.balchat.desktop.poll</string>
</array>
```

### Vault path
En desktop usamos `~/.balchat/vault.db`; en Android `app_local_data_dir`.
En iOS: `~/Documents/balchat/vault.db` (sandbox de la app, accessible al
usuario via Files.app si setteamos `LSSupportsOpeningDocumentsInPlace=YES`,
útil para backup manual). El `setup` callback de Tauri ya hace esa decisión
en mobile (`#[cfg(mobile)]` rama).

### Firma y distribución

Para sideload via Xcode (free Apple ID): proyecto generado debe abrirse
con `open gen/apple/balchat.xcodeproj`, configurar "Signing & Capabilities"
con tu Apple ID, y `Cmd+R` instala en device USB-conectado. Caduca en 7 días.

Para TestFlight / App Store: necesita paid developer account, certificado
Distribution, y subir IPA via Xcode → Organizer → Distribute App. La
review de Apple puede objetar el uso de Tor (políticas cambian); se ha
visto aprobado para apps que lo usan defensivamente (TorBrowser via
Onion Browser está en App Store).

## Estado por fase del port

- [ ] Tauri ios init OK (no probado)
- [ ] cargo build target aarch64-apple-ios-sim OK (no probado)
- [ ] App levanta en simulator (no probado)
- [ ] Tor bootstrap funciona en simulator (no probado)
- [ ] BGProcessingTask handler implementado
- [ ] Persistencia de passphrase en Keychain
- [ ] Push opcional via APNs (futuro)

## Lo que NO va a funcionar en iOS

- **`watch --listen` continuo**: iOS no permite mantener un proceso
  servidor permanentemente en background. La estrategia es relay-first:
  el peer entrante deja el blob en mi relay, mi BGProcessingTask lo recoge
  cuando el sistema permite. Conexiones live solo mientras la app esté
  en foreground.
- **`balchat host`** como modo principal: solo si el usuario tiene la
  app abierta. Tras cerrar, el onion service se baja.

## Ver también

- [README.md](../README.md) — documentación general.
- [docs/android-multi-abi.md](android-multi-abi.md) — paralelo para Android.
- [Apple BGTaskScheduler docs](https://developer.apple.com/documentation/backgroundtasks/bgtaskscheduler).
