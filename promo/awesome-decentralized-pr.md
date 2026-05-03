# PR draft: gdamdam/awesome-decentralized-systems

Repo: <https://github.com/gdamdam/awesome-decentralized-systems>
(or any of: `awesome-decentralized`, `awesome-p2p`, `awesome-self-hosted`).

Section: `Applications` → `Messaging` / `Chat`.

## Entry to add

```markdown
- [balchat](https://github.com/xandru582/balchat) - 1:1 and group messenger with no central server. Each user runs their own Tor onion service; messages are E2E-encrypted with MLS. Optional untrusted relay for offline delivery (self-hostable, default public one provided). Rust + Tauri 2. Apache-2.0.
```

## Why it's a good fit

Decentralized projects often have to choose between:

1. Federation with admins (XMPP, Matrix) — admins can see metadata.
2. P2P only (Briar) — can't deliver when both peers are offline.
3. Heavy infra (libp2p / IPFS-based) — slow, memory-hungry.

balchat picks a different point: **opt-in untrusted relay** that holds
opaque ciphertext and only learns "queue X received a blob at time T".
The relay is replaceable, self-hostable in 5 minutes on a €5/month VPS,
and the protocol works without it (online-only).

## PR title

```
Add balchat: messenger with no central server, MLS E2E over Tor onion services
```

## PR description

```
balchat (https://github.com/xandru582/balchat) is a 1:1 and group
messenger with the following decentralization properties:

- **No federation, no central directory.** A user is a Tor onion address
  + an MLS signing key. To talk to someone, you exchange "chat codes"
  (the onion + a derived queue ID) out-of-band, the same way you'd
  exchange phone numbers.
- **No mandatory infrastructure.** Online conversations go peer-to-peer
  between two onion services. Offline delivery requires a relay, but
  the relay is untrusted (sees only MLS ciphertext + pseudonymous queue
  IDs), users pick which one to use, and the binary is in the same repo
  for self-hosting (`deploy/relay/install.sh`).
- **Source is small enough to audit by yourself.** ~10k lines of Rust
  total in the workspace. No vendored dependencies for the cryptography
  (uses upstream `openmls`, `arti-client`, `rusqlite` with SQLCipher).

License: Apache-2.0.
Status: 0.1.x prototype, no external audit yet, prominent warning in README.
```
