# PR draft: tauri-apps/awesome-tauri

Repo: <https://github.com/tauri-apps/awesome-tauri>
Section to add into: `## Showcase` → category `Communication` (create if missing) or `## Applications`.

## Entry to add

```markdown
- [balchat](https://github.com/xandru582/balchat) - End-to-end-encrypted chat (MLS / RFC 9420) over Tor onion services. Desktop + Android. Pure-Rust Tor client. No accounts, no phone, no central server.
```

## PR title

```
Add balchat: E2E messaging over Tor onion services
```

## PR description

```
## What

Adds [balchat](https://github.com/xandru582/balchat) under "Showcase →
Applications" (or wherever you think fits best — happy to move).

## Why it's relevant to awesome-tauri

balchat is a non-trivial Tauri 2 app exercising several of the framework's
strengths:

- **Single Svelte 5 codebase** auto-selects between a desktop layout
  (Messages.app-style sidebar + chat, vibrancy via `backdrop-filter`,
  macOS title-bar overlay) and a mobile stack-router on Android.
  Detection via `matchMedia('(max-width: 720px), (pointer: coarse)')`.
- **Tauri 2 mobile** Android target shipped (signed APK, 42 MB) — runs
  the same Rust backend that drives the desktop binary.
- Cross-platform binaries verified on macOS arm64, Android arm64,
  Linux x86_64, Windows x86_64 (the last two via `cargo zigbuild`).

The README, code, and download links are all public; no auth required to
test the app.

## Checklist

- [x] Repo is FOSS (Apache-2.0)
- [x] Maintained (latest commit < 1 week)
- [x] Has a working binary download
- [x] README explains what it does
- [x] Entry follows the format of nearby items
```
