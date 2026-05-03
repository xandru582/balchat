# Privacy

balchat is built around the assumption that you don't trust anybody —
including us. This page lists every piece of data the app touches and
where it goes.

## What balchat does NOT collect

- **No analytics, no telemetry, no crash reporting.** No third-party
  SDK calls home. The app makes outbound network requests only to the
  Tor network (for transport) and to the user-configured relay (for
  offline blob storage).
- **No advertising IDs**, no fingerprinting, no IP logging.
- **No phone number, no email, no real name** required to register.
  You don't even register — you just generate a vault locally.
- **No address book scanning.** balchat does not read your contacts,
  photos, files, location, microphone, or camera unless you explicitly
  pick a file to send via the system file picker.

## What stays on your device

- **Vault** (`~/.balchat/vault.db` on desktop, app sandbox on mobile):
  SQLCipher-encrypted database protected by your passphrase. Contains
  your identity keys, contacts, MLS group state, and message history.
- **Salt file** (`~/.balchat/vault.db.salt`): random salt for the
  Argon2id key derivation. Without both files + passphrase the vault
  is unreadable.
- **Tor state** (`~/.balchat/arti-state/`): Tor consensus, descriptors,
  ephemeral circuit state. Deleted between sessions if you wish
  (re-bootstrap takes 30-90 s).

None of this leaves your device unless you explicitly export it
(*Configuración → Copia de seguridad*).

## What goes over the network

- **Tor traffic** to the public Tor network. Tor itself protects who
  you are and who you're talking to from network observers.
- **Direct peer traffic**: when both you and your peer are online,
  messages flow directly between your two onion services. Only the Tor
  guards/relays involved see ciphertext bytes.
- **Relay traffic** (only if peer is offline or pre-handshake KP fetch):
  encrypted blobs deposited at the user-configured relay (default:
  `dun4powrdfdaw3rtltyqhihxvefvkquoczqy4bqvtityu5sbdpq4ofid.onion`).
  See "Relay" section below.

## What the relay sees

The default relay (or any relay you use) sees:

- That a blob of size `N` arrived for queue `Q` at time `T`.
- That somebody fetched blobs from queue `Q` at time `T'`.
- The byte count and timing pattern of the above.

The relay does **NOT** see:

- The plaintext (blobs are MLS ciphertext).
- Your IP address (everything goes through Tor).
- Who is on the other side of the conversation (queue IDs are
  pseudonymous; correlating them with real identities requires
  knowing the corresponding onion address).
- Your contact list, vault state, or any other metadata.

If you don't want even that minimal metadata visible to a third party,
**run your own relay** (see [`deploy/relay/`](deploy/relay/)). 5 minutes
on any €5/month VPS.

## Cookies, tracking, etc.

The app has no web view that loads third-party content. The only
"cookie-equivalent" state is your vault.

The companion website (https://baluniverse.pages.dev/#balchat) is a
static HTML page hosted on Cloudflare Pages — it sets no cookies of
its own and embeds no analytics or trackers.

## Children

balchat does not knowingly collect data from anyone, of any age.

## Changes to this policy

Whenever this file changes, the change is in the public git history at
<https://github.com/xandru582/balchat/commits/main/PRIVACY.md>. Watch
the repo or check the changelog for `(privacy)` entries.

## Questions

xandru2222@gmail.com
