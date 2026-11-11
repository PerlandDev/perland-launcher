# Perland Launcher

A free, open-source Minecraft launcher for Perland players, and anyone else. Packs stay in sync on their own: press Play and you're on the right version.

> Status: early development. See `docs/superpowers/specs/` for the design and `docs/superpowers/plans/` for the build plans.

## Develop

Requirements: Rust stable (with MSVC build tools on Windows).

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### Desktop app

    pnpm install
    pnpm --filter ui dev:mock      # UI in a browser with a mocked backend
    pnpm tauri dev                 # the real app (PowerShell: $env:PERLAND_LAUNCHER_ROOT="$PWD\target\pl-dev")

Set `PERLAND_SITE_URL` (an https origin, or http on this computer) to read the public server list from another Perland site than `https://play.perland.online`, such as a staging one.

Set `PERLAND_PANEL_URL` (https, or http on this computer) to the Perland panel, and the servers the player owns get **Manage in Perland**, which opens `<panel>/stacks/<name>` in the browser. The app reads it when it starts, else uses the value it was built with; without one there is no button.

Microsoft sign-in uses the official Minecraft Launcher's public client (`00000000402b5328`) by default: Microsoft's page opens in a window of the launcher, which reads the code off `https://login.live.com/oauth20_desktop.srf` (the only redirect that client takes) and closes. That window keeps no cookies, so every sign-in can pick an account. To sign in with Perland's own Azure app instead (the browser and a loopback redirect), build with the repository variable `PERLAND_MS_CLIENT_ID` (a GUID); a dev run (`tauri dev`) can set it in the environment, and shipped builds ignore the runtime value. Refresh tokens belong to the app that issued them, so switching apps signs everyone out once.

### Cloud (Linux)

`scripts/cloud-setup.sh` installs everything CI uses (WebKit build deps, Rust stable with rustfmt/clippy, Node 24, pnpm 11.5.0, Java 21, `gh`) on Ubuntu and warms the caches. Use it as the setup script of a cloud environment; its header lists the network access and the optional `GH_TOKEN` secret it expects. Windows-only checks (installer, SmartScreen, signing, the real app window) still happen on Windows.

## Releasing

Releases come from the manual **Release** workflow: Actions → **Release** → Run workflow. Pick `beta` or `stable`, and untick **Dry run** to publish. Beta may run from any branch; stable only from `main`. Every publish is public, so ask the owner first.

- The version is computed from the date and the tags (`scripts/release/lib.mjs`, `nextVersion`): CalVer `YYYY.M.MICRO`, with the month in UTC and without a leading zero, so it stays valid SemVer for Cargo, the updater and the setup app (`2026.11.0`, `2027.1.2`). `MICRO` counts the month's releases from 0. Beta is `<base>-beta.N`, stable is `<base>`. While a base has betas and no stable, the next beta and the stable keep that base, even in a later month; after its stable, the next release starts a new base. Nothing is bumped by hand: `[workspace.package] version` in `Cargo.toml` is only what dev builds report, and the workflow sets the release version at build time.
- The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing. The repository variable `PERLAND_PANEL_URL` is built into released launchers (see Develop); unset, they offer no Manage in Perland. `PERLAND_MS_CLIENT_ID` is built in the same way; unset, the build signs in with the Minecraft Launcher's client. Set, it signs in with that Azure app, which Minecraft turns down until Mojang approves it, so the workflow warns. See Azure app registration below. Authenticode signing turns on with the variable `AZURE_TRUSTED_SIGNING=true` plus the `AZURE_*` secrets and variables named in the workflow.
- Release notes for players live in `release-notes/`, one small file per note (`kind`, `en`, `es`), added by the pull request that makes the change; the `release-notes` skill (`.claude/skills/release-notes/SKILL.md`) says how to write one. Nothing is stamped by hand: the workflow's `plan` job (`cli.mjs notes`) gives each note the first release since which every release has had its file, working from the release tags, and commits nothing. The launcher bundles the result and shows it in **What's new** after an update and from Settings → Launcher updates; the same notes become the GitHub Release's description (`cli.mjs notes-body`) and `latest.json`'s `notes`. A dry run prints the description in its run summary. Design: `docs/superpowers/specs/2026-11-10-whats-new-design.md`.
- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- On Windows each release also carries the one-click setup app (`crates/perland-setup`, design in `docs/superpowers/specs/2026-11-04-branded-setup-design.md`): `PerlandLauncher_<version>_x64-installer.exe`, with fixed-name copies `PerlandLauncher-installer.exe` (stable) and `PerlandLauncher-beta-installer.exe` (beta). It wraps that release's NSIS installer and runs it silently; updates keep using the NSIS installer, and `latest.json` never names the setup app. `cargo run -p perland-setup --example render` draws every screen to `target/setup-renders/`.
- The Windows build of a release also bundles the branded uninstaller (`perland-uninstall.exe`, the same crate's second binary) next to the launcher. The Release workflow adds it with a release-only `--config` (the resource and `src-tauri/windows/hooks.nsh`), whose post-install hook points `UninstallString` at it and keeps NSIS's own `uninstall.exe` as `QuietUninstallString`. `pnpm tauri build` without that config still makes the plain NSIS installer and uninstaller.
- A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.
- A publish needs CI green on the exact commit being released: push it to `main` or to a branch with a pull request and wait for CI first. A dry run only warns.
- The versioned release stays a draft until every installer is uploaded. If a publish fails partway, re-run the failed jobs: the publish job picks up the draft it left, or skips a release that is already out.

### Azure app registration

Only for signing in with Perland's own app instead of the Minecraft Launcher's client. The Azure app (Microsoft Entra ID → App registrations) is set up once by the owner. Its Application (client) ID is `PERLAND_MS_CLIENT_ID`; leave the variable empty until Mojang has approved it.

- **Supported account types:** personal Microsoft accounts only (the launcher signs in through the `consumers` endpoint).
- **Name:** must not contain Mojang, Minecraft, Microsoft, Live, Xbox, Discord or Hypixel (Mojang's app review refuses it).
- **Redirect URI:** `http://127.0.0.1/callback`, as a public-client ("Mobile and desktop applications") redirect. The launcher listens on a random loopback port and sends `http://127.0.0.1:<port>/callback`; Microsoft ignores the port when it matches a loopback redirect, so the one entry covers every port. The portal's Redirect URIs box refuses an `http` loopback address, so add it in the app's **Manifest**: in the Azure AD Graph format (what apps registered with a personal Microsoft account show) as `{ "url": "http://127.0.0.1/callback", "type": "InstalledClient" }` in `replyUrlsWithType`; in the Microsoft Graph format under `publicClient.redirectUris`. It then shows under **Authentication → Mobile and desktop applications**. Don't add the same address again with another port, or as a Web redirect.
- **Allow public client flows:** Yes (Authentication → Advanced settings). There is no client secret.
- **API permissions:** none to add; the launcher asks for `XboxLive.signin offline_access` when the player signs in.
- Then Mojang must approve the client ID for the Minecraft services (their AppID review). Until it does, every sign-in still runs the whole chain (browser, Microsoft, Xbox Live, XSTS, Minecraft) and Minecraft turns it down at its login; the launcher shows each step that passed and says Minecraft hasn't approved it yet.

## What the launcher sends

The launcher sends Perland two small things, so we know how many people use it and what breaks: **error reports** and one **usage ping a day**. Neither says who you are. There is no install ID, account ID or any other identifier, in what is sent or on your computer, and the server keeps totals only: no row per ping and no IP address.

**To turn it off:** Settings → Privacy → **Share anonymous data**. It is on by default. Off takes effect at once and deletes the reports and counts not sent yet. Setting the environment variable `PERLAND_LAUNCHER_TELEMETRY=0` turns it off too, whatever the switch says.

**You are told first.** Nothing is collected or sent until you have been shown that the launcher does this: on the first run's welcome screen for a new install, or in What's new after the update that brings it. Sharing then starts the next time the launcher starts, not before, so if you turn the switch off when you read about it, nothing was ever sent.

**Where it goes:** to Perland's own server (the Engine behind `play.perland.online`, the one the public server list comes from), over HTTPS, with no account or token attached. To find that server the launcher first reads `config.json` from `play.perland.online`, as it does for the server list. No third party is involved.

**It never gets in your way.** Sending happens in the background with a 10-second limit. If it fails, nothing is shown; what could not be sent waits for the next time the launcher starts (at most 50 reports are kept).

### The daily ping

Sent at most once per UTC day, the first time the launcher starts that day (`POST /api/v1/telemetry/launcher/ping`):

| Field | What it is |
|---|---|
| `version` | The launcher's version. |
| `channel` | `stable` or `beta` (the releases this launcher follows); `dev` only from a development build under test. |
| `os`, `arch` | `windows`, `linux` or `macos`, and the processor type (such as `x86_64`). |
| `locale` | The launcher's language (`en` or `es`), or the system's language tag (such as `es-MX`) if you never picked one. |
| `signedIn` | Whether a Perland account is signed in: yes or no, nothing about the account. |
| `firstEver`, `firstThisWeek`, `firstThisMonth` | Whether this is the launcher's first ping ever, this week and this month. The launcher works this out from the date of its last ping, so the server can count weekly and monthly players without telling installs apart. |
| `installWeek` | The week the launcher first ran with this feature, such as `2026-W46`. |
| `counters` | How many times each thing below happened since the last ping. |

The counters say how often something happened, never which server, modpack, skin or account it happened with:

| Counter | Counts |
|---|---|
| `game_launches` | The game started. |
| `game_crashes` | The game closed by itself with an error. |
| `game_repairs` | A repair of the game's files was started (the crash card's Repair). |
| `instances_created` | An instance was created with a Minecraft version picked by hand (New instance, the first run, or along with a new team server). |
| `instances_imported` | An instance was imported from another launcher. |
| `packs_installed` | A modpack was added as a new instance (from Browse, a file or a link). |
| `joins_by_link` | A server was added from an invite link or the server list. |
| `perland_sign_ins` | A sign-in to a Perland account finished. |
| `microsoft_sign_ins` | A sign-in to a Microsoft account finished. |
| `free_servers_created` | A free server was created. |
| `skins_changed` | A skin was put on an account. |
| `updates_installed` | A launcher update was installed. |

### Error reports

Sent shortly after something goes wrong, or at the next start (`POST /api/v1/telemetry/errors`). Each request says the launcher's `version`, `os`, `arch` and `locale` (as above) and carries up to 20 reports. A report has:

| Field | What it is |
|---|---|
| `kind` | `app_error` (an error the launcher showed you), `panic` (the launcher itself crashed), `exception` or `unhandled_rejection` (an error in the window's code), or `game_crash` (the game crashed). |
| `code` | The error's stable name, such as `download_corrupt` or `crash_out_of_memory`. |
| `message` | The error's plain English message, at most 500 characters. |
| `stack` | For a launcher crash or a window error: where in the launcher's code it happened. |
| `context` | A few short facts: the error's category, the launcher command that failed, a cleaned-up technical detail, and safe values such as a Minecraft version, a loader or an HTTP status. |
| `count` | How many times the same report happened. |

A **game crash** report holds only the cause the launcher recognised (or `crash_unknown`), the Minecraft version and the loader. It never holds a line of the game's log, the instance's name or a mod's name.

Every text is cleaned before it is saved or sent. Your home folder becomes `<home>`, the launcher's data folder `<data>`, any other file path `<path>`, web addresses `<url>` (except public services such as Mojang's or Modrinth's, which keep only the host), server addresses, host names and IP addresses `<address>`, emails `<email>`, your computer's user name `<user>`, and every name the launcher knows (accounts, instances, servers, invite links), anything shaped like a token or an account ID, and whatever the error itself marks as private `<redacted>`. Paths written without a drive or a leading slash (`saves\My world\level.dat`) become `<path>` too.

### What is never sent

By how it is built, these are never part of anything the launcher sends:

- An identifier for you or your computer. The server does not store your IP address.
- A sign-in token, a password, or which account is signed in.
- Anything from inside your files, your worlds or the game's log.
- Which server, modpack, skin or account a count is about.

And these are taken out of every error text before it is saved or sent, as described above:

- Your Minecraft, Microsoft and Perland account names, IDs and email.
- Invite links and their tokens, and servers' names and addresses.
- Your computer's user name, your home folder, and file paths on your computer.
- The names of your instances.

An error message can still name a mod or modpack (such as "Sodium needs Fabric API"), and a file's name without its folder (such as `options.txt`). The cleaning is automatic and errs on the side of removing too much; if you ever find something it missed, please open an issue.

### What stays on your computer

One small file, `telemetry.json` in the launcher's data folder: the week of the first run, the day you were shown the notice, the date of the last ping, the counters, and the reports not sent yet. With sharing off, the reports and counters in it are deleted and nothing new is written.

### For developers

A development build never sends anything, and neither does a launcher pointed at a scratch data folder with `PERLAND_LAUNCHER_ROOT`. To test against an Engine of your own, set `PERLAND_LAUNCHER_TELEMETRY_URL` to its address (https, or http on this computer); that is the only way either of those sends. The code is in `crates/launcher-core/src/telemetry` and `crates/launcher-app/src/telemetry.rs`, and the design in `docs/superpowers/specs/2026-11-11-anonymous-usage-and-errors-design.md`.

## License

GPL-3.0-only. See `LICENSE` and `NOTICE`.
