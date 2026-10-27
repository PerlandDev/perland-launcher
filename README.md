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
- The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing. The repository variable `PERLAND_PANEL_URL` is built into released launchers (see Develop); unset, they offer no Manage in Perland. `PERLAND_MS_CLIENT_ID` is built in the same way; unset, Microsoft sign-in is unavailable in that build. Authenticode signing turns on with the variable `AZURE_TRUSTED_SIGNING=true` plus the `AZURE_*` secrets and variables named in the workflow.
- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.
- A publish needs CI green on the exact commit being released: push it to `main` or to a branch with a pull request and wait for CI first. A dry run only warns.
- The versioned release stays a draft until every installer is uploaded. If a publish fails partway, re-run the failed jobs: the publish job picks up the draft it left, or skips a release that is already out.

## License

GPL-3.0-only. See `LICENSE` and `NOTICE`.
