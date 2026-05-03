# r/TOR post

Subreddit: <https://reddit.com/r/TOR>
**This sub is strict about self-promotion.** Read the rules carefully.
Often you have to wait for an "App Sundays" type thread or DM the mods
first.

## Approach (read first)

1. Check the subreddit rules sidebar for current self-promo policy.
2. Look at the last 3 months of posts — if there are zero "I built X
   that uses Tor" posts, **don't** be the first; instead, comment on a
   relevant existing thread and wait for someone to ask what you've
   built.
3. If self-promo is allowed, follow the format below.

## Title

```
balchat — chat client that runs entirely over onion services v3 (each user is a hidden service, no clearnet at all). Pure Rust, embedded Arti, Apache 2.0.
```

## Body

```
Hi r/TOR,

Wanted to share a project that uses Tor in a slightly unusual way and
might be of interest:

**balchat** (https://github.com/xandru582/balchat) is a 1:1 + group
messenger where **every user runs their own onion service**, and
conversations happen as peer-to-peer connections between two .onion
addresses.

### Tor-specific design choices

* **Pure Rust, no system Tor.** `arti-client` is linked into the app
  binary. No `tor` daemon needed, no torrc to manage. Single executable
  on every platform.
* **No clearnet endpoints anywhere.** The default relay (for offline
  delivery) is also onion-only. If you self-host a relay, it lives on
  your own onion. There's literally no "fallback to HTTPS" — if Tor is
  blocked, the app does not work.
* **Onion services for both directions.** When Alice talks to Bob, both
  apps act as both client and service: Alice connects to Bob's onion to
  send; when Bob is offline, Alice deposits on Bob's relay's onion.

### Threat model honesty

* No external audit yet (0.1.x). I'd rather be loud about that than
  soft-pedal — the README and SECURITY.md both lead with the warning.
* Onion address discovery is out of band — same model as Briar /
  Cwtch / OnionShare. There's no directory.
* Metadata at the relay is the weak point if you use the public one
  (sees queue activity, can correlate two queues if both peers use it).
  Self-hosting is a 5-min `install.sh` away.

### Trying it

Releases at https://github.com/xandru582/balchat/releases/latest
(macOS, Android arm64). Linux/Windows builds available via cargo too.

Happy to answer questions about the Arti integration / MLS / anything.

License: Apache-2.0. Source: https://github.com/xandru582/balchat
```

## Don't do

- Don't crosspost from r/privacy. The mod overlap is significant and
  it'll annoy them.
- Don't post screenshots of the app. r/TOR cares about Tor properties,
  not UI.
