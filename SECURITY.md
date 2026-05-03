# Security policy

## Reporting a vulnerability

balchat handles end-to-end-encrypted personal communication. Bugs in the
crypto, transport, or app layer can compromise users in ways that are hard
or impossible to undo. **Please report them privately first** — give us a
chance to fix and ship a release before public disclosure.

### How to report

- **Email**: xandru2222@gmail.com — subject line `[balchat security]`.
- **GitHub Security Advisory** (preferred): open a draft advisory at
  <https://github.com/xandru582/balchat/security/advisories/new>.
- **PGP**: not yet published — happy to set one up if you want to encrypt;
  ping by email and we'll arrange.

If you don't get a reply within **7 days**, escalate by re-sending or
opening a public issue marked `[unanswered security report]`.

### What to include

- Affected component (which crate, which CLI command, which UI flow).
- Affected version(s) — `git rev-parse HEAD` or release tag.
- Steps to reproduce (or PoC code if you have it).
- Impact assessment as you understand it.
- Whether you have a fix in mind.

### What happens next

1. Acknowledgement within 72 h.
2. Triage and CVSS-style severity within 7 days.
3. Coordinated patch on a private branch; we may ask for your review.
4. Release + GitHub Security Advisory + CVE if it warrants one.
5. Public credit in the advisory unless you ask to stay anonymous.

We follow a **90-day disclosure window** by default — after that, you're
free to publish regardless of fix status. We'll work with you on extensions
if a fix needs more time, but we won't ask for indefinite silence.

## Scope

In scope:

- All crates in this repository: `balchat-core`, `balchat-storage`,
  `balchat-relay`, `balchat-relay-proto`, `balchat-cli`, `balchat-tauri`.
- Reference relay binary as built from this repo.
- Default public relay operated alongside this project (operational
  compromise of the relay host is in scope, but the relay is *designed*
  to be untrusted — the threat model already assumes the operator can
  see metadata).
- Build/release pipeline (signed APK, .dmg, GitHub Releases).

Out of scope:

- Third-party relays operated by others.
- The user's host operating system, Tor binary outside our process,
  or hardware-level attacks (cold-boot, evil maid before vault unlock).
- Issues that require physical access to an unlocked, in-use device.
- Social-engineering attacks against the user (phishing the "código de
  chat", impersonation outside the app).

## Threat model

balchat's design protects:

- **Message contents** — end-to-end encrypted with MLS (RFC 9420). Even
  the relay operator and the network see only opaque ciphertext.
- **Identity correlation against network observers** — all traffic goes
  through Tor onion services (v3); no IP addresses are visible to peers
  or to the relay.
- **Server-side mass-collection** — there is no central server that holds
  contacts, messages, or identity. The relay is opt-in, holds only encrypted
  blobs by anonymous queue ID, and can be replaced or self-hosted.

balchat's design does **not** protect:

- **Metadata at the relay** — the operator sees that queue `X` received a
  blob of size `N` at time `T`. If sender and receiver use the same relay,
  the operator can correlate the two queues. If both queues are derived
  from public onions, the operator can in principle re-derive both onions
  if it knows them. Mitigation: self-host your relay, or use only direct
  (online-only) chats.
- **Endpoint compromise** — if your device is compromised (malware, root
  kit), the attacker can read your vault, your messages, and impersonate
  you. The vault encryption protects only data at rest while locked.
- **Physical coercion** — passphrase under duress, screen recording, etc.
- **Forward secrecy of pre-shared queue IDs** — queue IDs are derived
  from onion addresses (deterministic). An attacker who later learns
  your onion can identify your historical queue traffic at the relay.

See `README.md` § "Modelo de amenazas" for the full statement.

## Cryptography

- **Messaging**: MLS (RFC 9420) via [`openmls`](https://crates.io/crates/openmls)
  with the default ciphersuite (X25519/AES128-GCM/SHA256/Ed25519).
- **Vault encryption**: SQLCipher (AES-256, Argon2id KDF).
- **Transport**: Tor onion services v3 via [`arti-client`](https://crates.io/crates/arti-client).
- **Queue derivation**: SHA-256 with domain separation
  (`"balchat-queue-v1\0" || onion`).

No custom crypto. If you find balchat rolling its own primitive
anywhere — that's a bug, please report.

## Past advisories

None yet. This file will track them as they happen.
