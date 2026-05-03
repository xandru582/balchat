# r/rust launch post

Subreddit: <https://reddit.com/r/rust>
Rules: pretty relaxed for Show-style posts as long as Rust is genuinely
the focus. Lead with what you learned writing the thing.

## Title

```
I built balchat: an E2E messenger over Tor in pure Rust (openmls + arti-client + Tauri 2)
```

## Body

```
Spent the last few months on a Rust workspace that ended up shipping
4 binaries from the same codebase: a CLI, an untrusted relay, a
desktop UI, and an Android APK. Sharing in case it's useful as a
reference for other Rust devs touching crypto/networking.

### The crates

```
balchat-core         transport (Arti) + MLS conversation glue
balchat-storage      SQLCipher vault, contacts, message history
balchat-relay        untrusted onion mailbox binary
balchat-relay-proto  CBOR wire types client <-> relay
balchat-cli          CLI: host, connect, send, watch, groups, ...
balchat-tauri        desktop + Android UI (Svelte 5, single bundle)
```

### Things I learned

1. **`arti-client` is genuinely usable now.** Pure-Rust Tor client. You
   can `Endpoint::bootstrap_in(&dir).await` and `endpoint.host_onion(name)`
   in ~10 lines and get a working onion service. No system Tor binary.
2. **`openmls` for MLS** is excellent but the docs assume you've read
   RFC 9420. Reading the spec saved a week of guessing. The
   `openmls_rust_crypto` provider is the path of least resistance.
3. **Tauri 2 for mobile** works better than I expected — single Svelte 5
   codebase, the same `invoke('cmd', { args })` surface on Android and
   macOS. The biggest gotcha was Android NDK setup (need NDK 30
   specifically because of one of the C deps).
4. **Cross-compiling Rust to Linux from macOS** via `cargo zigbuild`
   beats every other option I tried. ~13 minutes for a release build of
   the relay (x86_64 + aarch64 in the same invocation).
5. **SQLCipher + Argon2id** via `rusqlite` with `bundled-sqlcipher` is
   the easiest "encrypted local DB" path I've found. No system OpenSSL
   needed if you use `bundled-sqlcipher-vendored-openssl`.

### Things I still want to figure out

* iOS — Tauri 2 supports it but I don't have Xcode installed yet.
* Reproducible builds (for F-Droid official) — `tauri build` embeds
  timestamps, not stable across machines. Suggestions welcome.
* `arti-client` startup time on Android (~25s on cold boot, mostly
  consensus download). Probably an Android-specific tweak.

### Repo

https://github.com/xandru582/balchat — Apache 2.0, ~10k LOC,
working downloads at https://github.com/xandru582/balchat/releases/latest

Happy to dig into specific implementation details if anyone's curious.
```

## When to post

After r/privacy if you go to both, otherwise immediately. r/rust
generally welcomes "I built X in Rust, here's what I learned" posts.

## Don't do

- Don't focus on the privacy claims here — r/rust cares about the code
  story. Save the threat-model talk for r/privacy and HN.
