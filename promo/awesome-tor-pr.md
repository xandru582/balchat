# PR draft: ajvb/awesome-tor

Repo: <https://github.com/ajvb/awesome-tor>
Section: `## Applications` (subsection: `Chat / Messaging` if it exists, otherwise just `Applications`).

If `ajvb/awesome-tor` is dormant, fall back to one of these forks:

- <https://github.com/0xn1ux/Awesome-Tor>
- <https://github.com/Daniel-Liu-c0deb0t/awesome-tor>

## Entry to add

```markdown
- [balchat](https://github.com/xandru582/balchat) - End-to-end-encrypted (MLS / RFC 9420) 1:1 and group messenger that runs entirely over onion services v3. Pure-Rust Tor stack via [arti-client](https://gitlab.torproject.org/tpo/core/arti) embedded in the app — no system Tor needed. Desktop (macOS, Linux, Windows) + Android.
```

## PR title

```
Add balchat: E2E messenger over onion services v3 (Rust + arti-client)
```

## PR description

```
## What

[balchat](https://github.com/xandru582/balchat) is a messenger that uses
Tor as its transport — every device runs its own onion service (v3) and
peers connect to each other directly via Tor circuits. Offline messages
go through a user-configurable untrusted relay (also reachable over
onion services).

## Tor angle

- **Embedded Arti**: balchat doesn't shell out to the Tor binary. It
  links [`arti-client`](https://gitlab.torproject.org/tpo/core/arti) and
  brings up its own onion service from inside the app. Single binary,
  no system Tor required.
- **Onion services for both directions**: each user is a hidden service.
  Conversations are peer-to-peer between two onion addresses; the relay
  (also onion-service-only) only ever sees ciphertext blobs and
  pseudonymous queue IDs.
- **No clearnet fallback** — there is literally no clearnet endpoint
  anywhere in the protocol.

## Status

Prototype / 0.1.x — the crypto is real (MLS + Tor + SQLCipher) but no
external security audit yet. README has a clear "don't use for serious
threat models until audited" notice.

## License

Apache-2.0.

## Checklist

- [x] Project actively maintained
- [x] Repo public, FOSS
- [x] Real Tor usage (not just "claims to")
- [x] Documentation explains the threat model
```
