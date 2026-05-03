# Mastodon / Fediverse drafts

Suggested home instance: <https://fosstodon.org> (FOSS-focused) or
<https://hachyderm.io> (tech-focused). Sign up as `@xandru582` on one
of them — match your GitHub handle.

## First toot (introduction)

(Threading: post these as a thread, one per beat.)

### Toot 1 (~280 chars)

```
Just shipped balchat 0.1.3 — a pure-Rust messenger I've been working
on. Each user is a Tor onion service. End-to-end with MLS. No phone,
no email, no central server.

Apache 2.0, source + downloads:
https://github.com/xandru582/balchat

A thread 🧵 with the design choices.
```

### Toot 2

```
1/ The transport is Tor onion services v3 via arti-client, embedded in
the app. No system Tor binary. Each user spins up their own hidden
service; conversations are peer-to-peer between two .onion addresses.

This means NAT traversal is "free" — runs on a phone behind 4G with
no port-forwarding.
```

### Toot 3

```
2/ Crypto is MLS (RFC 9420) via the openmls crate. Forward secrecy +
post-compromise security, group chats with proper key rotation.

The whole crypto stack is upstream Rust crates (openmls, arti-client,
rusqlite + SQLCipher). No custom primitives anywhere.
```

### Toot 4

```
3/ Offline delivery uses an "untrusted relay" — a small Rust binary
that holds opaque ciphertext blobs by pseudonymous queue ID. The
operator sees timing and size metadata but not content, not who's
talking to whom.

Default public relay shipped. Self-hosting is `install.sh` on any VPS.
```

### Toot 5

```
4/ UI is Tauri 2 + Svelte 5. Single codebase auto-detects mobile
viewport and switches to a stack-router layout. Macros app on macOS
with vibrancy + traffic-light overlay; mobile-first stack on Android.

Same Rust commands underneath, all 4 binaries from one cargo workspace.
```

### Toot 6 (call to action)

```
5/ Status is honest 0.1.x prototype — no external audit yet, README
says so loudly. Looking for:

* feedback on the design
* people willing to relay-stress-test
* iOS porters (the scaffold is there, blocked on me getting Xcode)

Pull requests welcome. #Rust #Privacy #Tor
```

## Hashtags

Add to most relevant toot (toot 1 or 6):

```
#Rust #Privacy #Tor #FOSS #Fediverse #Tauri
```

## Boost candidates

Mention or boost:

- @arti — arti-client account if it exists
- @tauri — official Tauri account
- Whoever maintains @torproject@mastodon.social
- Privacy-focused folks who follow you back
