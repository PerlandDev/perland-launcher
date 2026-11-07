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

- The version is computed: `<base>-beta.N` for beta, `<base>` for stable. The base is `[workspace.package] version` in `Cargo.toml`; after a stable release it is bumped in a PR.
- The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing. The repository variable `PERLAND_PANEL_URL` is built into released launchers (see Develop); unset, they offer no Manage in Perland. `PERLAND_MS_CLIENT_ID` is built in the same way; unset, the build signs in with the Minecraft Launcher's client. Set, it signs in with that Azure app, which Minecraft turns down until Mojang approves it, so the workflow warns. See Azure app registration below. Windows code signing is described under Windows code signing below.
- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- On Windows each release also carries the one-click setup app (`crates/perland-setup`, design in `docs/superpowers/specs/2026-11-04-branded-setup-design.md`): `PerlandLauncher_<version>_x64-installer.exe`, with fixed-name copies `PerlandLauncher-installer.exe` (stable) and `PerlandLauncher-beta-installer.exe` (beta). It wraps that release's NSIS installer and runs it silently; updates keep using the NSIS installer, and `latest.json` never names the setup app. `cargo run -p perland-setup --example render` draws every screen to `target/setup-renders/`.
- The Windows build of a release also bundles the branded uninstaller (`perland-uninstall.exe`, the same crate's second binary) next to the launcher. The Release workflow adds it with a release-only `--config` (the resource and `src-tauri/windows/hooks.nsh`), whose post-install hook points `UninstallString` at it and keeps NSIS's own `uninstall.exe` as `QuietUninstallString`. `pnpm tauri build` without that config still makes the plain NSIS installer and uninstaller.
- A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.
- A publish needs CI green on the exact commit being released: push it to `main` or to a branch with a pull request and wait for CI first. A dry run only warns.
- The versioned release stays a draft until every installer is uploaded. If a publish fails partway, re-run the failed jobs: the publish job picks up the draft it left, or skips a release that is already out.

### Windows code signing

The Windows build is signed with the owner's Certum Open Source Code Signing in the Cloud certificate, through [ssign](https://github.com/Le-Syl21/ssign). Design: `docs/superpowers/specs/2026-11-07-windows-code-signing-design.md`.

- **On or off:** the repository variable `CERTUM_SIGNING=true`. Off, releases build unsigned and their notes say so.
- **When it signs:** every publish, and a dry run with **Sign this dry run's Windows installer too** ticked. Either waits for the owner to approve the `signing` environment.
- **Secrets:** `CERTUM_EMAIL` and `CERTUM_OTP` are environment secrets of `signing` (Settings → Environments → `signing`, with the owner as required reviewer), never repository secrets. `CERTUM_OTP` is the TOTP seed behind the SimplySign QR code: base32, or the whole `otpauth://` URI. It signs as the owner, so treat it like a private key. If it leaks, re-issue the QR code from Certum.
- **What gets signed:** the launcher, both uninstallers (NSIS's and `perland-uninstall.exe`), the NSIS plugin DLLs, the NSIS installer and the setup app (`perland-setup.exe`).
- **What the build checks:** `cli.mjs sign-check` fails if any signing call failed, including the NSIS uninstaller's, whose failures makensis ignores, or if any of those files went unsigned. `verify-authenticode.ps1` checks the setup app and installs the NSIS installer on the runner to check every signature.
- **Upgrading ssign:** change `SSIGN_VERSION` and `SSIGN_SHA256` in `release.yml` together. The SHA-256 is the release asset's digest: `gh api repos/Le-Syl21/ssign/releases/tags/v<version> --jq '.assets[] | select(.name=="ssign-windows-x86_64.zip") | .digest'`.
- **If ssign breaks:** run a dry run unsigned, sign its installer by hand with SimplySign Desktop, and publish that file.

### Azure app registration

Only for signing in with Perland's own app instead of the Minecraft Launcher's client. The Azure app (Microsoft Entra ID → App registrations) is set up once by the owner. Its Application (client) ID is `PERLAND_MS_CLIENT_ID`; leave the variable empty until Mojang has approved it.

- **Supported account types:** personal Microsoft accounts only (the launcher signs in through the `consumers` endpoint).
- **Name:** must not contain Mojang, Minecraft, Microsoft, Live, Xbox, Discord or Hypixel (Mojang's app review refuses it).
- **Redirect URI:** `http://127.0.0.1/callback`, as a public-client ("Mobile and desktop applications") redirect. The launcher listens on a random loopback port and sends `http://127.0.0.1:<port>/callback`; Microsoft ignores the port when it matches a loopback redirect, so the one entry covers every port. The portal's Redirect URIs box refuses an `http` loopback address, so add it in the app's **Manifest**: in the Azure AD Graph format (what apps registered with a personal Microsoft account show) as `{ "url": "http://127.0.0.1/callback", "type": "InstalledClient" }` in `replyUrlsWithType`; in the Microsoft Graph format under `publicClient.redirectUris`. It then shows under **Authentication → Mobile and desktop applications**. Don't add the same address again with another port, or as a Web redirect.
- **Allow public client flows:** Yes (Authentication → Advanced settings). There is no client secret.
- **API permissions:** none to add; the launcher asks for `XboxLive.signin offline_access` when the player signs in.
- Then Mojang must approve the client ID for the Minecraft services (their AppID review). Until it does, every sign-in still runs the whole chain (browser, Microsoft, Xbox Live, XSTS, Minecraft) and Minecraft turns it down at its login; the launcher shows each step that passed and says Minecraft hasn't approved it yet.

## License

GPL-3.0-only. See `LICENSE` and `NOTICE`.
