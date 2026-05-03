# Show HN draft

Submit at: <https://news.ycombinator.com/submit>
Best window: Tuesday or Wednesday, 8-10 AM Pacific (16:00-18:00 UTC).

## Title (80-char max)

```
Show HN: Balchat – E2E messenger over Tor onion services in Rust
```

## URL

```
https://github.com/xandru582/balchat
```

(NOT the Cloudflare website — HN values code-first links.)

## First comment (post immediately after)

```
Hey HN — author here.

Balchat is a messenger I've been working on with the goal of "decent
chat without depending on anyone's server, and without having to share
my phone number". The interesting tradeoffs:

* **Crypto**: MLS (RFC 9420) via openmls. End-to-end, with forward
  secrecy and post-compromise security. Group chats up to ~50 members
  before MLS commit-overhead becomes uncomfortable.

* **Transport**: Each user is a Tor onion service v3. Online conversations
  go peer-to-peer between the two onions. Arti (pure-Rust Tor client) is
  embedded in the app — no system Tor required, single binary.

* **Offline delivery**: an untrusted relay holds opaque ciphertext blobs
  in a SQLite. The relay sees only "queue X received N bytes at time T".
  Queue IDs are derived from onion addresses (SHA-256 with domain sep)
  so any peer who knows your onion knows your queue — no out-of-band
  exchange of mailbox info needed. Default public relay shipped, users
  can self-host (`deploy/relay/install.sh`, 5 min on a €5 VPS).

* **Storage**: vault is SQLCipher with Argon2id KDF. No telemetry, no
  analytics, no ads. Privacy statement at PRIVACY.md is exhaustive.

* **Stack**: Rust workspace (~10k LOC) with Tauri 2 for the UI. Single
  Svelte 5 codebase auto-selects between desktop layout (Messages.app
  feel on macOS, vibrancy via backdrop-filter) and mobile stack-router
  on Android.

The `0.1.x` is honest — the crypto is real but no external audit has
happened yet, README has a prominent "don't use for serious threat
models until audit" notice. Looking for: feedback on the protocol
(especially MLS group invitation flow and the queue derivation choice),
people willing to relay-stress-test, and ideally someone who wants to
review the iOS port (currently scaffolded, blocked on me getting Xcode
installed).

Repo: https://github.com/xandru582/balchat
Releases: https://github.com/xandru582/balchat/releases/latest
```

## Be ready to answer

- "Why not Briar / Cwtch / Session?" → Briar requires both peers online
  for fresh chats; Cwtch is the closest precursor (also Tor onion
  services); Session uses its own onion routing variant. Balchat tries
  to be a more conventional UI (Messages.app-like) on top of similar
  crypto, with offline-via-untrusted-relay as the main differentiator
  from Briar and pure-Rust + smaller surface than Cwtch.
- "Why a default relay if you also support self-host?" → 99% of users
  won't host one. Default makes onboarding work; self-host is a 5-min
  escape hatch for people who care about metadata.
- "How is this different from just using Signal?" → No phone number,
  no centralized server, no proprietary protocol. Tradeoff: tiny user
  base, no audit, slower delivery, no group calls. Signal is better
  for almost everyone almost everywhere — balchat is for the people
  who specifically want the "no server in the middle" property and are
  willing to pay UX cost.
- "License?" → Apache-2.0. Patent grant included.
```

## Don't do

- Don't shout "PLEASE UPVOTE" anywhere.
- Don't wait for a quiet day; HN traffic dies if you don't get traction
  within the first hour.
- Don't argue with downvoters in the thread. Answer questions, ignore
  the noise.
