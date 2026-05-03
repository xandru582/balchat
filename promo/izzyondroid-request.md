# IzzyOnDroid inclusion request

Open at: <https://gitlab.com/IzzyOnDroid/repo/-/issues/new>
Title: `Inclusion request: balchat (com.balchat.desktop)`
Labels: `Inclusion request`

---

## Body (paste verbatim)

Hi! Requesting inclusion of **balchat** in the IzzyOnDroid F-Droid repo.

### App

- Name: **balchat**
- Package ID: `com.balchat.desktop`
- Summary: End-to-end-encrypted chat over Tor onion services. No phone number, no email, no central server.
- Source code: <https://github.com/xandru582/balchat>
- License: Apache-2.0
- Author: xandru582 <xandru2222@gmail.com>
- Latest stable release: <https://github.com/xandru582/balchat/releases/latest>
- APK download (always points to the newest signed APK):
  <https://github.com/xandru582/balchat/releases/latest/download/balchat.apk>

### What it does

balchat is a 1:1 and small-group messenger with the following design:

- **End-to-end encryption**: MLS protocol (RFC 9420) via the [`openmls`](https://crates.io/crates/openmls) crate. Forward secrecy + post-compromise security.
- **Network**: Tor onion services v3 via [`arti-client`](https://crates.io/crates/arti-client) — pure-Rust Tor client embedded in the app, no system Tor needed.
- **Offline**: untrusted relay for messages sent while the recipient is offline. Blobs are MLS ciphertext, queue IDs are pseudonymous (SHA-256 of the onion). Default public relay shipped; users can self-host.
- **No accounts, no phone, no email** — registration is just generating a vault locally.
- **Vault**: SQLCipher-encrypted SQLite (Argon2id KDF).

The whole stack is Rust + Svelte 5 (Tauri 2). Android UI is a mobile-first variant of the desktop one (auto-detected via viewport).

### Anti-features (FOSS metadata)

- ✅ No tracking SDKs (no Crashlytics, Firebase, Google Analytics, etc.)
- ✅ No proprietary network services (not even ours — the default relay is FOSS, source in the same repo at `crates/balchat-relay/`).
- ✅ No ads.
- ✅ No proprietary dependencies — all deps Apache-2.0 / MIT / BSD.
- ⚠️ Not yet **reproducible builds** — current APK signed with Android debug keystore + non-deterministic timestamps. We'll work on this for F-Droid official; for IzzyOnDroid the AppVerify hash should be enough.

### Metadata file (F-Droid YAML format)

We ship one in the repo at [`fdroid/metadata/com.balchat.desktop.yml`](https://github.com/xandru582/balchat/blob/main/fdroid/metadata/com.balchat.desktop.yml). Feel free to use it directly or adapt as needed.

### Privacy

[PRIVACY.md](https://github.com/xandru582/balchat/blob/main/PRIVACY.md) explains exactly what balchat does and does not collect (TL;DR: nothing leaves the device except Tor traffic + encrypted blobs to the user-configured relay).

### Status caveat

balchat is currently in **0.1.x prototype** — the cryptography is real (MLS + Tor + SQLCipher), but no external security audit has been performed yet. We'll add a clear notice to the IzzyOnDroid description if you'd like.

Thanks! Happy to answer any questions or adjust whatever's needed.

— xandru582 (xandru2222@gmail.com)
