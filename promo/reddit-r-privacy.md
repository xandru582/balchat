# r/privacy launch post

Subreddit: <https://reddit.com/r/privacy>
Read the rules first: <https://www.reddit.com/r/privacy/wiki/rules>

The mods are strict about self-promotion. Best to **flair as
"Discussion"** and lead with the technical/threat-model angle, not the
"download my thing" angle.

## Title

```
balchat — open-source E2E messenger that runs over Tor onion services, no phone or email required (Apache-2.0, looking for technical feedback)
```

## Body

```
Hey r/privacy,

I've been building **balchat** (https://github.com/xandru582/balchat),
an open-source 1:1 + group messenger that I think might be interesting
to people here — *not* because it's a Signal replacement, but because of
the specific tradeoffs it makes:

### What it is

* Each user is a Tor onion service v3. Conversations are peer-to-peer
  between two onion addresses.
* End-to-end encryption with MLS (RFC 9420), the same protocol Cisco /
  Wire / Mozilla have been pushing as the next-gen Signal Protocol.
* Untrusted relay holds encrypted blobs when one peer is offline. The
  relay sees only "queue X got N bytes at time T". Queue IDs are
  pseudonymous (SHA-256 of the onion). Default public relay shipped,
  but you can self-host one in 5 minutes on a €5 VPS.
* No phone number, no email, no central registration server. Your
  account is a passphrase + a vault file on your device.
* Open source: Apache-2.0. Pure Rust + Tauri 2. ~10k LOC total.

### What it does NOT protect against

(Mods, I'm leading with this on purpose — too many privacy projects
oversell.)

* Metadata at the relay: if you and your contacts use the same relay,
  the operator can correlate queue activity. Self-host if it matters.
  See PRIVACY.md and SECURITY.md in the repo.
* Endpoint compromise: standard caveat — if your device is rooted,
  game over.
* Forward-secret queue IDs: current design uses deterministic queue
  IDs derived from onion addresses. Trade for usability (no manual
  exchange). Open to suggestions on rotating-queue designs.

### Status

**0.1.x prototype.** The crypto is real (openmls + arti-client +
SQLCipher), but **no external security audit yet**. README has a clear
"don't use for serious threat models until audit" notice. I'd rather
ship the warning prominently than soft-pedal it.

### Asking for

* Eyeballs on the design — does the deterministic-queue tradeoff
  bother you? What would you change?
* Eyeballs on the code, especially `crates/balchat-core/src/conversation.rs`
  (the MLS handshake glue) and `crates/balchat-tauri/src/lib.rs`
  (the Tauri command surface).
* Anyone who wants to run a relay node on a small VPS to make the
  network less centralized — `deploy/relay/install.sh` does the work.

Happy to answer everything in this thread. Will not delete the post if
the questions get hard. :)

Repo: https://github.com/xandru582/balchat
Privacy: https://github.com/xandru582/balchat/blob/main/PRIVACY.md
Security: https://github.com/xandru582/balchat/blob/main/SECURITY.md
Releases: https://github.com/xandru582/balchat/releases/latest
```

## When to post

After IzzyOnDroid inclusion is live (so Android users have a real path
to install), and ideally not on a Friday/weekend.

## Don't do

- Don't post the same content to r/privacytoolsIO, r/privacyguides, etc.
  the same week. Pick one privacy-focused sub at a time.
- Don't link to the Cloudflare website — privacy crowd will side-eye CF
  for being a CDN. Link to GitHub directly.
