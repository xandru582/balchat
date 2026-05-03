# r/selfhosted launch post

Subreddit: <https://reddit.com/r/selfhosted>
Angle: focus on the **relay** that you can self-host, not on the app.
That's what the sub cares about.

## Title

```
balchat-relay: tiny Rust Tor onion mailbox you can self-host in 5 min on any VPS (for E2E messaging without central server)
```

## Body

```
Quick share for r/selfhosted. As part of [balchat](https://github.com/xandru582/balchat)
(an E2E messenger over Tor onion services), I had to write the
"untrusted mailbox" piece — a small server that holds encrypted blobs
when the recipient is offline.

It ended up being small + portable enough that it's worth sharing on
its own. ~300 lines of Rust, single statically-linked binary, no
runtime deps (Arti is embedded so no system Tor needed).

### What it is

* Tor onion service v3 (NAT-traversal "for free", no port forwarding,
  no public IP needed).
* SQLite (via rusqlite) for blob storage. WAL mode.
* CBOR wire protocol — `Put`, `Get`, `PutKeyPackage`, `ConsumeKeyPackage`.
* Stateless beyond the SQLite — restart whenever, peers reconnect.
* Sees only opaque ciphertext + queue IDs. Cannot read messages.

### Specs

| Resource | Idle | Active (10 simultaneous deliveries) |
|----------|------|-------------------------------------|
| RAM      | ~80 MB | ~120 MB |
| CPU      | <1%  | <5% |
| Disk     | grows by ciphertext blob size | (auto-prune to be added) |

Tested on a Hetzner CX22 (4 GB RAM, ~€4.5/month). Probably fine on a
Pi Zero 2 W.

### Deploy bundle

[`deploy/relay/`](https://github.com/xandru582/balchat/tree/main/deploy/relay)
in the repo:

```
balchat-relay-x86_64       cross-compiled binary, 20 MB
balchat-relay-aarch64      same for ARM
balchat-relay.service      systemd unit (hardened — NoNewPrivileges,
                           ProtectSystem=strict, RestrictNamespaces, ...)
install.sh                 60 lines: useradd, install binary, systemd
                           enable, wait for onion to come up, print it
README.md                  step by step
```

Three commands on a fresh Ubuntu 24.04:

```bash
scp -r deploy/relay/ root@TU_VPS:/root/relay-deploy/
ssh root@TU_VPS "cd /root/relay-deploy && chmod +x install.sh && ./install.sh"
# script prints your relay's onion address after Tor bootstraps (~2 min)
```

### Why bother

Default balchat ships a public relay (the one I run on the same Hetzner
box), so users don't *need* to host. But:

* Public relay sees metadata (queue X received N bytes at time T,
  pseudonymous queue IDs derived from onions). Confidentiality of
  message content is still safe (MLS), but if you don't want a third
  party to see when you chat and with whom, run your own.
* Same architecture you'd want for any E2E messaging service —
  storage layer cleanly separated from anything that needs to see
  plaintext.

### Caveats

* No spam control yet (any blob with valid CBOR gets stored). The relay
  is tiny enough that I haven't worried about this; if you grow, you'll
  want size limits + IP-based rate limiting (note: no IPs over Tor —
  needs proof-of-work or per-queue rate limit instead).
* No multi-tenant ACLs. The "buzón público" model assumes any queue ID
  is fair game.

License: Apache-2.0. Repo: https://github.com/xandru582/balchat
Single-binary download: https://github.com/xandru582/balchat/releases/latest
```

## Don't do

- Don't lead with "balchat the app". The sub will downvote anything
  that smells like a product launch. Lead with "relay you can self-host".
