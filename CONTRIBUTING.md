# Contributing to balchat

Bug reports, fixes, docs, translations, and PRs are welcome — and
necessary, this is a small project. Before you start, read this once.

## Quick rules

1. **Be respectful.** No discrimination, no harassment, period.
2. **Security bugs go to [SECURITY.md](SECURITY.md), not to public issues.**
3. **One concern per PR** — easier to review, easier to revert.
4. **Don't break the build.** `cargo check --workspace` and
   `cargo test --workspace` should pass on `main`.
5. **The license is Apache 2.0** ([LICENSE](LICENSE)). By submitting a
   PR you agree your contribution is licensed under the same terms.

## Setup

You need:

- Rust 1.95+ (or whatever `rust-toolchain.toml` pins, if present).
- Node 22+ (for the Tauri frontend in `crates/balchat-tauri/ui`).
- For Android builds: Android NDK 30, JDK 21, Tauri CLI.
- For Linux/Windows cross-compile from macOS: `cargo zigbuild`.
- For iOS builds: Xcode 16+ (see [docs/ios-build.md](docs/ios-build.md)).

```bash
git clone https://github.com/xandru582/balchat
cd balchat
cargo build --workspace          # runs CLI + relay + desktop sources
cargo test --workspace           # ~30 s on M1
```

For the desktop app:

```bash
cd crates/balchat-tauri/ui && npm install && cd ..
cargo tauri dev
```

## Where things live

```
crates/
  balchat-core/         transport (Arti), MLS conversation, send/recv
  balchat-storage/      SQLCipher vault, contacts, message history
  balchat-relay/        untrusted onion mailbox binary
  balchat-relay-proto/  CBOR wire types between client and relay
  balchat-cli/          balchat CLI (host, connect, send, watch, ...)
  balchat-tauri/        desktop + Android frontend (Svelte 5 UI)
  spike-mls/            standalone MLS handshake spike
  spike-tor/            standalone Tor onion service spike

deploy/relay/           install.sh + systemd unit for VPS self-host
docs/                   build notes per platform
fdroid/                 F-Droid metadata
```

The two crates worth understanding first are:

- `balchat-core/src/conversation.rs` — handshake, MLS group,
  send_app/recv_app.
- `balchat-tauri/src/lib.rs` — Tauri command handlers; entry points
  for everything the UI does.

## PR checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --workspace` is no worse than before
- [ ] Tests added or updated for behaviour you changed
- [ ] No new `unwrap()` in async or in the request path (use `?`)
- [ ] If you touched the wire protocol or the vault schema: bump the
      relevant version constant and document migration
- [ ] If you added a Tauri command: register it in `invoke_handler!`
      and document it in the comment block above the function

## Style

Rust style follows `rustfmt` defaults. Comments should explain *why*
(or what's surprising) — `cargo doc` reads `///`, but most useful
context is for the next maintainer reading the code, so write for them.

UI is plain Svelte 5 + CSS. No Tailwind, no UI library — keep the JS
bundle small (under 150 KB minified).

## Reviewing

We aim for a first response within a week. Don't take silence as
rejection — ping the PR after 7 days if no one has commented. The
maintainer is one person and sometimes life happens.

## Translations

UI strings live inline in the Svelte files. There's no i18n framework
yet — if you want to add Catalan / Galician / Portuguese / French / etc.
support, open an issue first so we can pick a sane structure together.

## Donations

balchat doesn't take donations right now. If you want to support
something, contribute code, file good bug reports, run a relay, or
mirror the project to Codeberg / your own forge.
