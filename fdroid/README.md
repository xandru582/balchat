# F-Droid metadata for balchat

Two distribution paths to F-Droid-compatible app stores. Both are free.

## A. IzzyOnDroid (third-party, fast)

The easiest way to reach F-Droid users. Approval typically 1-3 days.

### What we provide

The signed release APK is published as a GitHub Release asset on every
tag (see [`../.github/workflows/`](../.github/workflows/) when added, or
the manual signing flow documented in [README.md](../README.md)).

### What to ask IzzyOnDroid for

Open an issue at <https://gitlab.com/IzzyOnDroid/repo/-/issues> asking
for inclusion. Use the body in [`../promo/izzyondroid-request.md`](../promo/izzyondroid-request.md).

Once accepted, balchat appears at:

- Repo URL: <https://apt.izzysoft.de/fdroid/repo>
- App page: <https://apt.izzysoft.de/fdroid/index/apk/com.balchat.desktop>

Users add that repo to F-Droid Basic / Droidify / Neo-Store.

## B. F-Droid official (slower, ~50k+ users)

Submit a merge request to <https://gitlab.com/fdroid/fdroiddata>
adding the metadata file [`metadata/com.balchat.desktop.yml`](metadata/com.balchat.desktop.yml).

### Status

- Metadata YAML: ✅ committed in this repo, ready to copy.
- Reproducible builds: ⚠️ not yet — current APK is signed with the
  Android debug keystore at build time, which embeds non-deterministic
  timestamps. Need to switch to a stable signing key + lock the build
  environment before F-Droid official will accept.
- Build server compatibility: needs Tauri Android pre-step (`cargo
  tauri android init --ci`) which downloads the NDK. F-Droid's build
  server does provide NDK 30, so this should work but has not been
  tested end-to-end.

The intended next step is to enable A first, then iterate on
reproducibility for B.

## C. Alternatively: F-Droid via your own repo

You can host an F-Droid repo on your own server (e.g. the same Hetzner
VPS that runs the public balchat relay). `fdroidserver` is in Debian's
apt — `apt install fdroidserver`, generate a keystore, drop APKs into
`repo/`, run `fdroid update`, and serve the resulting `repo/` directory
over HTTPS. Users add your URL to their F-Droid app.

This is the fully-self-hosted option but requires you to manage keys
forever. Not recommended unless A and B are both blocked for you.
