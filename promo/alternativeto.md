# AlternativeTo.net submission

URL to submit: <https://alternativeto.net/software/new>

Account required (free; sign up with email or GitHub OAuth).

## Form fields

### Name

```
balchat
```

### Tagline (~150 chars)

```
End-to-end-encrypted (MLS) chat over Tor onion services. No phone, email, or central server. Pure Rust + Tauri 2.
```

### Description (~600 chars)

```
balchat is a 1:1 and group messenger where every user runs their own
Tor onion service. Conversations happen peer-to-peer between two
.onion addresses, encrypted end-to-end with MLS (RFC 9420). Offline
delivery uses an opt-in untrusted relay that sees only ciphertext and
pseudonymous queue IDs.

No phone number, no email, no central account server. Your "account"
is a passphrase + a vault file on your device.

Open source (Apache 2.0). Pure Rust + Tauri 2. macOS, Android, Linux,
Windows. Default public relay or self-host your own in 5 minutes.
```

### Categories

- `Communication`
- `Internet`
- `Security & Privacy`

### Tags

```
end-to-end-encryption  open-source  tor  rust  messenger
self-hostable          encrypted    no-tracking  decentralized
mls                    onion-services
```

### License

```
Open Source — Apache License 2.0
```

### Platforms

- macOS
- Linux
- Windows
- Android
- (iOS — once shipped)

### URLs

- Website: `https://baluniverse.pages.dev/#balchat`
- Source code: `https://github.com/xandru582/balchat`
- Download: `https://github.com/xandru582/balchat/releases/latest`

### Screenshots

Once we have polished screenshots:
- Mac chat view (light + dark)
- Android chat view (light + dark)
- macOS login / vault unlock
- Settings
- Sidebar with multiple contacts

### "It's an alternative to:" (this is the gold)

Mark these so it shows on the right pages:

- **Signal** — alternative for users who don't want to give a phone number
- **Telegram** — alternative for users who want true E2E by default (not opt-in like secret chats)
- **WhatsApp** — same E2E story without account
- **Briar** — same Tor angle, but with offline-via-relay (Briar requires both online for fresh chats)
- **Cwtch** — closest precursor; same Tor onion design, different stack (Go vs Rust)
- **Session** — same "no phone number" pitch; balchat uses Tor instead of Loki Service Nodes
- **Wire** — same MLS crypto choice, different transport (clearnet vs Tor)
- **Element / Matrix** — federated alternative; balchat is decentralized further
- **OnionShare** — same Tor onion-services-as-endpoint design, but balchat is messaging-first not file-share-first

### After submission

AlternativeTo moderates new entries — usually 1-3 days. Once live,
the URL will be `https://alternativeto.net/software/balchat/`.
Add that URL to:

- Your README badges list
- The IzzyOnDroid request
- The HN comment
