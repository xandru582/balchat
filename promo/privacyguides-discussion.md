# Privacy Guides — discussion forum prep

Privacy Guides (privacyguides.org) is **the** place for privacy-tool
recommendations. Listing requires meeting strict criteria, including
proven track record + at least one independent security review.

balchat is **not yet ready** for an inclusion request. Don't submit.

## What you should do instead (now)

Open a **discussion** in the forum:

- Forum: <https://discuss.privacyguides.net>
- Category: `Tools` → `Communication` (or `Tool Suggestions` if `Communication` doesn't allow non-listed tools).
- Title: `balchat — early-stage Tor onion-services + MLS messenger, looking for design feedback`

This is **not** an inclusion request. You're asking for technical
feedback from a community that knows messenger threat models cold.

## Body

```
Hi everyone. I'm building balchat (https://github.com/xandru582/balchat)
and would love feedback from this community before I push for any kind
of broader recognition.

Quick summary:

* End-to-end with MLS (RFC 9420) via openmls.
* Each user is a Tor onion service v3 (arti-client embedded in the app).
* Untrusted relay for offline delivery; sees ciphertext + pseudonymous
  queue IDs, no plaintext, no IP.
* No phone, no email, no central account server.
* Apache 2.0, pure Rust + Tauri 2.

The README is honest about the status — 0.1.x prototype, no external
audit yet. I'm not asking for inclusion in the Privacy Guides
recommendations (clearly nowhere near ready). I'm asking for:

1. Feedback on the threat model in SECURITY.md / PRIVACY.md.
2. Specifically: thoughts on using deterministic queue IDs derived from
   onion addresses (SHA-256, domain-separated). The trade is "no need
   to exchange queue out-of-band, peers find each other via just the
   onion" vs "the relay operator can re-derive the queue → onion link
   if it ever learns the onion".
3. Specifically: the default public relay model (we ship a hardcoded
   relay onion in the app as the default mailbox; users can self-host
   in 5 min via `deploy/relay/install.sh`). Is this good UX-vs-security
   balance, or should default be "no relay, online-only"?

Pointers:
- README: https://github.com/xandru582/balchat/blob/main/README.md
- Threat model: https://github.com/xandru582/balchat/blob/main/SECURITY.md#threat-model
- Privacy statement: https://github.com/xandru582/balchat/blob/main/PRIVACY.md
- Relay code: https://github.com/xandru582/balchat/tree/main/crates/balchat-relay

Happy to take harsh feedback. Better to know now.
```

## When to post

- After IzzyOnDroid is live (so people have a real install path).
- After at least one external eyeball has looked at the code (e.g. a
  positive r/privacy thread or a Reddit-cryptography post).

## Long-term Privacy Guides inclusion criteria

Roughly (check the latest at https://www.privacyguides.org/en/about/criteria/):

1. Open source.
2. Cryptography that's been independently reviewed.
3. Audited (independent third-party security audit, public report).
4. Maintained — multiple committers ideal, regular releases.
5. Cross-platform.
6. No known centralization choke points.

Of these, balchat currently has 1, 5, partial 6. Missing: 2, 3, 4
(single committer = me).

The audit is the hard one. Realistic plan:

1. v0.2.0 with at least 1 month of public use + IzzyOnDroid + a couple
   of awesome-lists inclusions.
2. Apply for a free OSTIF audit
   (https://ostif.org/audit-engagement-guide/) or a community-funded
   one via Open Tech Fund.
3. Implement audit fixes; ship v1.0.
4. **Then** apply to Privacy Guides.
```
