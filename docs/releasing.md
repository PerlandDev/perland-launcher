# Releasing Perland Launcher

Releases come from the manual **Release** workflow (`.github/workflows/release.yml`): Actions → **Release** → Run workflow. Pick `beta` or `stable`, and untick **Dry run** to publish. Beta may run from any branch; stable only from `main`.

**Every publish is public, so ask the owner first.**

## Before you publish

- A publish needs CI green on the exact commit being released: push it to `main` or to a branch with a pull request and wait for CI first. A dry run only warns.
- Do a dry run first. A dry run's installers are downloadable from the run's artifacts. That replaces the old tester build.

## Version numbers

The version is computed from the date and the tags (`scripts/release/lib.mjs`, `nextVersion`): CalVer `YYYY.M.MICRO`, with the month in UTC and without a leading zero, so it stays valid SemVer for Cargo, the updater and the setup app (`2026.11.0`, `2027.1.2`). `MICRO` counts the month's releases from 0. Beta is `<base>-beta.N`, stable is `<base>`. While a base has betas and no stable, the next beta and the stable keep that base, even in a later month; after its stable, the next release starts a new base. Nothing is bumped by hand: `[workspace.package] version` in `Cargo.toml` is only what dev builds report, and the workflow sets the release version at build time.

## Secrets and variables

The required secrets are `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) must match that signing key, or installed launchers will reject every update. The workflow enforces this: its manifest step (`cli.mjs manifest`) reads the 8-byte minisign key id of every `.sig` and of the committed public key, and stops before anything is published if any differ. After a key rotation, commit the new public key before releasing. The repository variable `PERLAND_PANEL_URL` is built into released launchers (see [Develop](../README.md#develop) in the README); unset, they offer no Manage in Perland. Windows code signing has its own variable and secrets: see [Windows code signing](#windows-code-signing).

## Release notes

Release notes for players live in `release-notes/`, one small file per note (`kind`, `en`, `es`), added by the pull request that makes the change; the `release-notes` skill (`.claude/skills/release-notes/SKILL.md`) says how to write one. Nothing is stamped by hand: the workflow's `plan` job (`cli.mjs notes`) gives each note the first release since which every release has had its file, working from the release tags, and commits nothing. The launcher bundles the result and shows it in **What's new** after an update and from Settings → Launcher updates; the same notes become the GitHub Release's description (`cli.mjs notes-body`) and `latest.json`'s `notes`. A dry run prints the description in its run summary. Design: `docs/superpowers/specs/2026-11-10-whats-new-design.md`.

## What a release publishes

- Each release is a public, versioned GitHub Release. `channel-beta` and `channel-stable` hold `latest.json` and fixed-name installers; perland.online/launcher links those.
- On Windows each release also carries the one-click setup app (`crates/perland-setup`, design in `docs/superpowers/specs/2026-11-04-branded-setup-design.md`): `PerlandLauncher_<version>_x64-installer.exe`, with fixed-name copies `PerlandLauncher-installer.exe` (stable) and `PerlandLauncher-beta-installer.exe` (beta). It wraps that release's NSIS installer and runs it silently; updates keep using the NSIS installer, and `latest.json` never names the setup app. `cargo run -p perland-setup --example render` draws every screen to `target/setup-renders/`.
- The Windows build of a release also bundles the branded uninstaller (`perland-uninstall.exe`, the same crate's second binary) next to the launcher. The Release workflow adds it with a release-only `--config` (the resource and `src-tauri/windows/hooks.nsh`), whose post-install hook points `UninstallString` at it and keeps NSIS's own `uninstall.exe` as `QuietUninstallString`. `pnpm tauri build` without that config still makes the plain NSIS installer and uninstaller.

## Windows code signing

The Windows build is signed with the owner's Certum Open Source Code Signing in the Cloud certificate, through [ssign](https://github.com/Le-Syl21/ssign). Design: `docs/superpowers/specs/2026-11-07-windows-code-signing-design.md`.

- **On or off:** the repository variable `CERTUM_SIGNING=true`. Off, releases build unsigned and their notes say so.
- **When it signs:** every publish, and a dry run with **Sign this dry run's Windows installer too** ticked. Either waits for the owner to approve the `signing` environment.
- **Secrets:** `CERTUM_EMAIL` and `CERTUM_OTP` are environment secrets of `signing` (Settings → Environments → `signing`, with the owner as required reviewer), never repository secrets. `CERTUM_OTP` is the TOTP seed behind the SimplySign QR code: base32, or the whole `otpauth://` URI. It signs as the owner, so treat it like a private key. If it leaks, re-issue the QR code from Certum.
- **What gets signed:** the launcher, both uninstallers (NSIS's and `perland-uninstall.exe`), the NSIS plugin DLLs, the NSIS installer and the setup app (`perland-setup.exe`).
- **What the build checks:** `cli.mjs sign-check` fails if any signing call failed, including the NSIS uninstaller's, whose failures makensis ignores, or if any of those files went unsigned. `verify-authenticode.ps1` checks the setup app and installs the NSIS installer on the runner to check every signature.
- **Upgrading ssign:** change `SSIGN_VERSION` and `SSIGN_SHA256` in `release.yml` together. The SHA-256 is the release asset's digest: `gh api repos/Le-Syl21/ssign/releases/tags/v<version> --jq '.assets[] | select(.name=="ssign-windows-x86_64.zip") | .digest'`.
- **If ssign breaks:** run a dry run unsigned, sign its installer by hand with SimplySign Desktop, and publish that file.

## When a publish fails

The versioned release stays a draft until every installer is uploaded. If a publish fails partway, re-run the failed jobs: the publish job picks up the draft it left, or skips a release that is already out.
