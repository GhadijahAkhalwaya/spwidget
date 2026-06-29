# Spwidget

A small macOS desktop widget that doesn't just count your Jira story
points — it nudges you toward the next productivity tier.

The number on its face is how many points you've moved to Done in the
last 90 days (or the current month). The widget's appearance shifts as
you cross each tier, giving the count momentum and a visible "next
level" to chase.

Built for the impact.com Product Enablement team but works against any
Jira Cloud or Server / Data Center instance.

![Apple Silicon only](https://img.shields.io/badge/macOS-Apple%20Silicon-blue)

## Productivity tiers

You start at the bottom and earn your way up. Each tier has its own
visual treatment — the widget changes how it looks every time you cross
a threshold, so a glance tells you where you are without reading the
number.

Thresholds are tunable per-period in the settings cog, so each user can
calibrate the tiers to their own pace. And keep grinding past the
obvious ones — the widget might still have a surprise for you. 👀

There's also a **Motivate setting** (toggle in settings) for monthly
mode: it ignores the static threshold and instead computes whether
you're on pace to hit the Thug threshold by month-end based on
working-days-elapsed. When you're projecting on track, you cross into
"Totally locked in" — pastel gradient, sparkle confetti, "in the zone"
subtext. Falling behind drops you back to warming-up mode and the
projection updates after each refresh.

## Install (for end users)

Each release ships a signed-ish, ad-hoc-signed `.app` bundle. Apple
Silicon (M1+) only.

1. Download `Spwidget.zip` from the latest [Release](../../releases).
2. Double-click to unzip → drag `Spwidget.app` into `/Applications`.
3. Open Terminal and run:
   ```bash
   xattr -dr com.apple.quarantine /Applications/Spwidget.app
   ```
   This strips macOS's download quarantine — required because the app is
   not notarized with a paid Apple Developer cert.
4. Launch from Applications. A small widget appears, plus a menu-bar icon
   and a Dock icon — single-click either to wake the widget from
   hibernation.

## First-time setup

The widget opens a setup form asking for:

| Field | Notes |
| --- | --- |
| Jira URL | Pre-filled with `https://impact.atlassian.net` |
| Email | Your work email |
| API token | Create at [id.atlassian.com](https://id.atlassian.com/manage-profile/security/api-tokens) — link is in the form |
| Project key (optional) | e.g. `IRD` — scopes the count to one project. Leave empty to count across all projects. |

The token is stored in the macOS Keychain (`com.local.jira-points-widget`).
Nothing sensitive is written to the app data directory in plaintext.

## Daily use

- **Widget** floats on every Space. Drag it anywhere — position is
  remembered across hibernation and app restarts.
- **Auto-hibernate** after 4 minutes of no widget clicks — widget fades
  and drops behind every normal window so it doesn't overlay your work.
- **Wake**: click the widget, the menu-bar icon, or the Dock icon.
- **Settings cog**: switch between 90-day / monthly mode, Chill vs
  Motivate setting, custom thresholds.
- **× button**: quits the app entirely.

## How the count works

JQL emitted by the app:

```
assignee = currentUser()
AND [project = <KEY>]
AND statusCategory = Done
AND statusCategoryChangedDate >= -90d
ORDER BY statusCategoryChangedDate DESC
```

Notable: we filter by `statusCategoryChangedDate`, NOT `updated` —
otherwise comments/labels/watcher changes on old Done issues would drift
them back into the window.

Story points field is auto-detected on setup (tries `Story Points`, then
`Story point estimate`, then any field containing "story point"). If
multiple candidates match, the app probes a few recent Done issues to
pick the field that actually carries data.

## Building from source

Prerequisites:
- Rust toolchain (`rustup`)
- Node 18+ and npm
- Xcode Command Line Tools

```bash
npm install
npm run tauri build -- --target aarch64-apple-darwin
```

Output: `src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Spwidget.app`

For a shareable zip:

```bash
APP=src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Spwidget.app
codesign --force --deep --sign - --options runtime "$APP"
ditto -c -k --sequesterRsrc --keepParent "$APP" Spwidget.zip
```

## Project layout

- `src/` — TypeScript frontend (Vite + Vanilla TS)
  - `views/widget.ts` — main widget UI, hibernation, theming
  - `views/setup.ts` — first-run + reconfigure form
  - `api.ts` — typed wrappers around Tauri commands
- `src-tauri/src/` — Rust backend
  - `commands.rs` — Tauri command handlers
  - `jira/` — REST client, JQL builder, field detection
  - `state.rs` — shared app context + refresh loop
  - `scheduler.rs` — daily background refresh
  - `secrets.rs` — Keychain integration
- `src-tauri/tauri.conf.json` — window + bundle config

## License

MIT — see [LICENSE](LICENSE).
