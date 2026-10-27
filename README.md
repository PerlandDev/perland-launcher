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

Microsoft sign-in needs the Azure app's id: the repository variable `PERLAND_MS_CLIENT_ID` (a GUID) is built into released launchers. A dev run (`tauri dev`) can set `PERLAND_MS_CLIENT_ID` in the environment instead; shipped builds ignore the runtime value. Without an id, signing in with Microsoft says it isn't set up in this build.

### Cloud (Linux)

`scripts/cloud-setup.sh` installs everything CI uses (WebKit build deps, Rust stable with rustfmt/clippy, Node 24, pnpm 11.5.0, Java 21, `gh`) on Ubuntu and warms the caches. Use it as the setup script of a cloud environment; its header lists the network access and the optional `GH_TOKEN` secret it expects. Windows-only checks (installer, SmartScreen, signing, the real app window) still happen on Windows.

## Releasing

Releases come from the manual **Release** workflow: Actions → **Release** → Run workflow. Pick `beta` or `stable`, and untick **Dry run** to publish. Beta may run from any branch; stable only from `main`. Every publish is public, so ask the owner first.

- The version is computed: `<base>-beta.N` for beta, `<base>` for stable. The base is `[workspace.package] version` in `Cargo.toml`; after a stable release it is bumped in a PR.
- The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing. The repository variable `PERLAND_PANEL_URL` is built into released launchers (see Develop); unset, they offer no Manage in Perland. `PERLAND_MS_CLIENT_ID` is built in the same way; unset, Microsoft sign-in is unavailable in that build and nobody could start the game, so a publish stops before building (a dry run only warns). See Azure app registration below. Authenticode signing turns on with the variable `AZURE_TRUSTED_SIGNING=true` plus the `AZURE_*` secrets and variables named in the workflow.
- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.
- A publish needs CI green on the exact commit being released: push it to `main` or to a branch with a pull request and wait for CI first. A dry run only warns.
- The versioned release stays a draft until every installer is uploaded. If a publish fails partway, re-run the failed jobs: the publish job picks up the draft it left, or skips a release that is already out.

### Azure app registration

Microsoft sign-in uses one Azure app (Microsoft Entra ID → App registrations), set up once by the owner. Its Application (client) ID is `PERLAND_MS_CLIENT_ID`.

- **Supported account types:** personal Microsoft accounts only (the launcher signs in through the `consumers` endpoint).
- **Name:** must not contain Mojang, Minecraft, Microsoft, Live, Xbox, Discord or Hypixel (Mojang's app review refuses it).
- **Redirect URI:** `http://127.0.0.1/callback`, as a public-client ("Mobile and desktop applications") redirect. The launcher listens on a random loopback port and sends `http://127.0.0.1:<port>/callback`; Microsoft ignores the port when it matches a loopback redirect, so the one entry covers every port. The portal's Redirect URIs box refuses an `http` loopback address, so add it in the app's **Manifest**: in the Azure AD Graph format (what apps registered with a personal Microsoft account show) as `{ "url": "http://127.0.0.1/callback", "type": "InstalledClient" }` in `replyUrlsWithType`; in the Microsoft Graph format under `publicClient.redirectUris`. It then shows under **Authentication → Mobile and desktop applications**. Don't add the same address again with another port, or as a Web redirect.
- **Allow public client flows:** Yes (Authentication → Advanced settings). There is no client secret.
- **API permissions:** none to add; the launcher asks for `XboxLive.signin offline_access` when the player signs in.
- Then Mojang must approve the client ID for the Minecraft services (their AppID review). Until it does, sign-in stops after Xbox with "waiting on approval".

## License

GPL-3.0-only. See `LICENSE` and `NOTICE`.
