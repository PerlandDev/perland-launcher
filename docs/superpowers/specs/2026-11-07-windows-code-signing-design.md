# Windows code signing with a Certum cloud certificate

Date: 2026-11-07. Replaces the SignPath Foundation design of 2026-10-15 (PR #26, never merged) and the dormant Azure hook in the distribution spec (§3.5).

## 1. Why Certum

The owner is an individual outside the US and Canada, and PerlandDev isn't a registered company.

- **Azure Artifact Signing** takes individuals only in the US and Canada, and organisations only in the US, Canada, the EU and the UK. Prism Launcher signs this way.
- **SignPath Foundation** is free. But its application asks for proof that the project is widely used, which a month-old closed beta can't show.
- **EV certificates** no longer skip SmartScreen, so they buy nothing.
- **Certum Open Source Code Signing in the Cloud** (€49 at the time of writing) is issued to a natural person who publishes free or open-source software. The certificate's subject is "Open Source Developer, <owner's name>", so **the owner's legal name is the publisher** players see. The owner accepted this on 2026-11-07. The key is RSA-4096 and stays in Certum's cloud HSM (SimplySign). Usage is limited to 5,000 signatures a month.

A Sectigo certificate kept in Google Cloud KMS and signed with jsign, which is how Modrinth signs (with DigiCert), was the alternative. It costs about five times as much, and it wasn't clear an individual could buy it.

## 2. How the build signs

Tauri's `bundle.windows.signCommand` is `node scripts/release/cli.mjs sign %1`, set through a `--config` file that `cli.mjs sign-config` writes. A file, not inline JSON, so no shell re-quotes it or `%1`. During `tauri build` Tauri calls it, in order, for:

1. the launcher `.exe`;
2. the NSIS plugin DLLs (Tauri's copy of them);
3. the uninstaller (from makensis's `!uninstfinalize`);
4. the finished installer.

Only after that does Tauri write the updater `.sig`, so the `.sig` covers the signed installer. One build pass, no artifact round trips.

`cli.mjs sign`:

- skips a file that already has an Authenticode signature. ssign refuses one, and Tauri passes the plugins along unchecked. Only plugins may come signed.
- otherwise runs `ssign -n "Perland Launcher" -u https://perland.online/launcher <file>`, which signs in place with an RFC 3161 timestamp from Certum;
- checks the file is signed afterwards;
- appends `<role> <outcome> <file>` to `$SIGN_DIR/ledger.log`.

makensis ignores the exit code of the uninstaller's sign command, so a failure there would ship an unsigned uninstaller silently. After the build, `cli.mjs sign-check` fails on any `error` line. It also fails unless the launcher, the uninstaller and the installer were each signed by us. Then `verify-authenticode.ps1` installs the result on the runner and checks that the installer, the launcher and `uninstall.exe` carry a valid Certum signature with a timestamp.

The NSIS plugin DLLs are signed too: they ship inside our installer, and Tauri signs them for that reason.

## 3. ssign

[ssign](https://github.com/Le-Syl21/ssign) (MIT) is an independent client for Certum's SimplySign cloud API. Certum offers no unattended signing of its own: SimplySign Desktop needs a person at the screen. ssign logs in with the account email and a TOTP code it computes from the seed behind the SimplySign QR code. It sends only a SHA-256 digest to Certum, and builds the Authenticode signature locally.

- **Pinned:** the workflow downloads release `v0.1.11`'s Windows zip and checks it against the SHA-256 GitHub recorded for that asset. Upgrading means changing `SSIGN_VERSION` and `SSIGN_SHA256` together.
- **Risk:** a young, single-maintainer project on an unofficial API. If Certum changes the API, signing fails loudly (the build stops), never silently. The fallback is signing a dry run's installer by hand on the owner's PC with SimplySign Desktop, then publishing it.

## 4. Secrets and approval

- The repository variable `CERTUM_SIGNING=true` turns signing on. Until then the workflow builds unsigned, as before.
- `CERTUM_EMAIL` and `CERTUM_OTP` (the TOTP seed, base32 or the full `otpauth://` URI) are **environment secrets of `signing`**, not repository secrets. That environment requires the owner's approval. The seed signs as the owner for as long as it is valid, so a run can read it only after the owner approves. If it leaks, re-issue the SimplySign QR code from Certum.
- The Windows build job runs in `signing` for every publish, and for a dry run with **Sign this dry run** ticked. Other dry runs and the Linux job run without an environment and never see the secrets.
- A publish with signing on can't ship unsigned: any signing failure stops the build before the publish job.

## 5. What signing changes for players

- **Smart App Control** runs a signed app of unknown reputation, so the launcher stops being blocked.
- **SmartScreen** still warns ("Windows protected your PC") until downloads build the certificate's reputation. Reputation is earned per certificate and per file hash.
- The release notes drop "the installers aren't code-signed yet" once `CERTUM_SIGNING` is on.
- perland.online/launcher says who signs the installer, and its code signing section names Certum.
