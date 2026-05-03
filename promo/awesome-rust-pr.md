# PR draft: rust-unofficial/awesome-rust

Repo: <https://github.com/rust-unofficial/awesome-rust>
Section: `## Applications` → subsection `Communication` (or `Cryptography` if not).

## Entry to add

```markdown
- [balchat](https://github.com/xandru582/balchat) [[balchat-cli](https://github.com/xandru582/balchat/tree/main/crates/balchat-cli)] — End-to-end-encrypted (MLS / RFC 9420) chat over Tor onion services. Pure-Rust Tor stack via arti-client; no system Tor needed. CLI + desktop + Android. [![Apache 2](https://img.shields.io/badge/license-Apache%202-007aff)](https://github.com/xandru582/balchat/blob/main/LICENSE)
```

## PR title

```
Add balchat: pure-Rust E2E messenger over Tor (CLI + Tauri 2)
```

## PR description

```
## Project

[balchat](https://github.com/xandru582/balchat) — an end-to-end-encrypted
1:1 and group messenger built entirely in Rust:

- Crypto: [openmls](https://crates.io/crates/openmls) (MLS / RFC 9420)
- Transport: [arti-client](https://crates.io/crates/arti-client) onion services v3
- Storage: [rusqlite](https://crates.io/crates/rusqlite) with SQLCipher + Argon2id
- UI: [Tauri 2](https://tauri.app/) (desktop + Android)

The workspace ships 4 binaries (CLI, untrusted relay, desktop UI, Android
APK) all built from the same Rust core. No Go, no JS for transport, no
shell-out — just `cargo build --workspace`.

## Where to add

I added it under `Applications → Communication`, but feel free to move
to `Cryptography` or `Network programming` if it fits better. The CLI
is the cleanest entry point for Rust devs evaluating it.

## Maintenance / status

Repository is active (last release a few hours ago, [v0.1.3](https://github.com/xandru582/balchat/releases/tag/v0.1.3)).
Apache-2.0 licensed. CI / reproducible builds in progress.

## Checklist

- [x] Has Cargo.toml at root (workspace)
- [x] Compiles on stable Rust
- [x] FOSS license (Apache-2.0)
- [x] README in English (and Spanish)
- [x] Active maintenance
- [x] Entry follows the alphabetical order of the section
```
