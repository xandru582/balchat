# Changelog

All notable changes are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project adheres to [Semantic Versioning](https://semver.org/) once it
hits 1.0; while in 0.x.y, breaking changes can occur in any minor.

## [Unreleased]

## [0.1.3] — 2026-05-03

### Fixed

- **Offline messaging works without manual queue exchange.** Each user's
  queue ID is now derived deterministically from their onion address
  (`SHA-256("balchat-queue-v1\0" || onion)`) instead of being random.
  Any peer who knows your onion now also knows your queue. Resolves the
  `send: no sé el buzón del peer` error that appeared when the recipient
  was offline.
- **Auto-add on inbound handshake** — peers added implicitly via an
  incoming handshake now ship pre-filled with the default relay and
  derived queue, so offline messaging works immediately.
- **Friendlier auto-label** — auto-discovered contacts are labelled
  `Nuevo · xxxxxx` instead of the cryptic `peer-xxxxxxxx`. Editable.

### Added

- Backwards-compatible queue derivation in `send_with_fallback` and
  `add_contact_cmd`: legacy contacts without a stored queue ID derive
  one on the fly.
- `sha2 = "0.10"` dependency.

## [0.1.2] — 2026-05-03

### Added

- **Edit contact** — desktop sidebar shows an ✏️ button on hover next
  to the existing ✕. Mobile chat header has a ⋯ menu that opens a
  full-screen edit screen with a Delete button at the bottom.
- `update_contact_cmd` Tauri command. Onion is read-only (it identifies
  the contact); label, relay, queue, pubkey are mutable.

### Changed

- **Friendlier log strings.** `display_peer(vault, onion)` resolves to
  the contact's label whenever possible. Examples:
  - `dial directo a xxx.onion:1234…` → `conectando con Marta…`
  - `handshake OK con xxx.onion`     → `conexión segura establecida con Marta`
  - `conn entrante de xxx.onion`     → `Marta se ha conectado`
  - `[de xxx] archivo X.pdf`         → `Marta te ha enviado: X.pdf`

## [0.1.1] — 2026-05-03

### Fixed

- **macOS Gatekeeper "está dañado"**. Tauri bundle now uses
  `signingIdentity = "-"` to ad-hoc sign the .app inside the .dmg.
  Gatekeeper still says "desarrollador no identificado" — accept from
  *Privacidad y Seguridad → Abrir igualmente*, or
  `xattr -cr /Applications/balchat.app`.
- **Offline send no longer requires explicit `--relay`** — falls back
  to `DEFAULT_RELAY_ONION` (the bundled public mailbox) when the contact
  has no relay configured.

### Added

- Extended chat code format `onion#queueHex` so peers exchange both
  pieces of info in a single shareable string. *Compartir mi código* now
  copies the extended form. `add_contact_cmd` parses it. Backwards-
  compatible: pasting a bare onion still works for live chat.

## [0.1.0] — 2026-05-03

### Added

- **Non-technical UX rewrite.** Vocabulary throughout the UI swapped
  from cryptography jargon to plain language: vault → cuenta,
  passphrase → contraseña, handshake → conectar, daemon (hidden),
  onion → código de chat, relay → buzón offline.
- **Default public mailbox** hardcoded (`dun4powrdf…ofid.onion`) so
  first-run users have offline messaging working without any setup.
- **Auto-publish KeyPackages** to the relay on first daemon start —
  offline peers can invite you to MLS groups without you doing anything.
- **Native-feel macOS UI** — title-bar overlay (system traffic lights),
  vibrancy on sidebar via `backdrop-filter`, Messages.app-style bubbles,
  auto light/dark from system setting.
- **Mobile UI variants** auto-selected by viewport: stack-router with
  iOS-style slide transitions, full-screen home / chat / settings /
  new contact / edit contact, bottom-safe-area aware composer.
- **Connect command in the UI** (`connect_cmd`) — Initiator-side
  handshake live without dropping to the CLI. Click *Conectar*, done.
- **Contact updated event** (`balchat://contact-updated`) emitted from
  both Initiator and Acceptor sides of the handshake; the receiving
  app no longer stays stuck at "handshake pendiente" until first message.
- **Per-platform icons** generated from the project ICO via `tauri icon`.
- **Public relay deploy bundle** in [`deploy/relay/`](deploy/relay/) —
  cross-compiled binaries for x86_64 + aarch64 Linux + hardened systemd
  unit + installer. 5 minutes to self-host on any VPS.

## [0.0.1] — earlier

Initial codebase: workspace with `balchat-core`, `balchat-storage`,
`balchat-relay`, `balchat-relay-proto`, `balchat-cli`, `balchat-tauri`.
CLI worked end-to-end (host, connect, send, watch, groups, offline via
relay). Desktop and Android Tauri UIs were minimal but functional. iOS
scaffolded but unverified.

[Unreleased]: https://github.com/xandru582/balchat/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/xandru582/balchat/releases/tag/v0.1.3
[0.1.2]: https://github.com/xandru582/balchat/releases/tag/v0.1.2
[0.1.1]: https://github.com/xandru582/balchat/releases/tag/v0.1.1
[0.1.0]: https://github.com/xandru582/balchat/releases/tag/v0.1.0
