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

## Releasing

Releases come from the manual **Release** workflow: Actions → **Release** → Run workflow. Pick `beta` or `stable`, and untick **Dry run** to publish. Beta may run from any branch; stable only from `main`. Every publish is public, so ask the owner first.

- The version is computed: `<base>-beta.N` for beta, `<base>` for stable. The base is `[workspace.package] version` in `Cargo.toml`; after a stable release it is bumped in a PR.
- The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing.
- Windows installers are Authenticode-signed by SignPath Foundation once the variable `SIGNPATH_SIGNING=true` is set, with the variables `SIGNPATH_ORGANIZATION_ID` and `SIGNPATH_PROJECT_SLUG` and the secret `SIGNPATH_API_TOKEN`. Each signed run asks for two approvals in SignPath: the launcher and uninstaller, then the installer. A dry run signs with SignPath's test certificate unless **Sign with the release certificate** is ticked. Setup and the configuration to paste into SignPath are in `.signpath/README.md`; the design is `docs/superpowers/specs/2026-10-15-windows-code-signing-design.md`.
- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.
- If the publish job fails, use **Re-run failed jobs** on that run; it keeps the same version. The versioned release is created as a draft and only published once every installer is uploaded, and a draft left behind by a failed run is reused by the next run.

## License

GPL-3.0-only. See `LICENSE` and `NOTICE`.
