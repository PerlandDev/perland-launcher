# Releasing Perland Launcher

Releases come from the manual **Release** workflow (`.github/workflows/release.yml`): Actions → **Release** → Run workflow. Pick `beta` or `stable`, and untick **Dry run** to publish. Beta may run from any branch; stable only from `main`.

**Every publish is public, so ask the owner first.**

## Before you publish

- A publish needs CI green on the exact commit being released: push it to `main` or to a branch with a pull request and wait for CI first. A dry run only warns.
- Do a dry run first. A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.

## Version numbers

The version is computed from the date and the tags (`scripts/release/lib.mjs`, `nextVersion`): CalVer `YYYY.M.MICRO`, with the month in UTC and without a leading zero, so it stays valid SemVer for Cargo, the updater and the setup app (`2026.11.0`, `2027.1.2`). `MICRO` counts the month's releases from 0. Beta is `<base>-beta.N`, stable is `<base>`. While a base has betas and no stable, the next beta and the stable keep that base, even in a later month; after its stable, the next release starts a new base. Nothing is bumped by hand: `[workspace.package] version` in `Cargo.toml` is only what dev builds report, and the workflow sets the release version at build time.

## Secrets and variables

The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing. The repository variable `PERLAND_PANEL_URL` is built into released launchers (see [Develop](../README.md#develop) in the README); unset, they offer no Manage in Perland.

## Release notes

Release notes for players live in `release-notes/`, one small file per note (`kind`, `en`, `es`), added by the pull request that makes the change; the `release-notes` skill (`.claude/skills/release-notes/SKILL.md`) says how to write one. Nothing is stamped by hand: the workflow's `plan` job (`cli.mjs notes`) gives each note the first release since which every release has had its file, working from the release tags, and commits nothing. The launcher bundles the result and shows it in **What's new** after an update and from Settings → Launcher updates; the same notes become the GitHub Release's description (`cli.mjs notes-body`) and `latest.json`'s `notes`. A dry run prints the description in its run summary. Design: `docs/superpowers/specs/2026-11-10-whats-new-design.md`.

## What a release publishes

- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- On Windows each release also carries the one-click setup app (`crates/perland-setup`, design in `docs/superpowers/specs/2026-11-04-branded-setup-design.md`): `PerlandLauncher_<version>_x64-installer.exe`, with fixed-name copies `PerlandLauncher-installer.exe` (stable) and `PerlandLauncher-beta-installer.exe` (beta). It wraps that release's NSIS installer and runs it silently; updates keep using the NSIS installer, and `latest.json` never names the setup app. `cargo run -p perland-setup --example render` draws every screen to `target/setup-renders/`.
- The Windows build of a release also bundles the branded uninstaller (`perland-uninstall.exe`, the same crate's second binary) next to the launcher. The Release workflow adds it with a release-only `--config` (the resource and `src-tauri/windows/hooks.nsh`), whose post-install hook points `UninstallString` at it and keeps NSIS's own `uninstall.exe` as `QuietUninstallString`. `pnpm tauri build` without that config still makes the plain NSIS installer and uninstaller.

## When a publish fails

The versioned release stays a draft until every installer is uploaded. If a publish fails partway, re-run the failed jobs: the publish job picks up the draft it left, or skips a release that is already out.
