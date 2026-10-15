# Perland Launcher: Windows code signing (SignPath Foundation), Design

- **Date:** 2026-10-15
- **Status:** Design approved in chat by the owner (2026-10-15); this written spec awaits review.
- **Parent spec:** `docs/superpowers/specs/2026-10-14-distribution-design.md` (§3.4 release workflow, §3.5 installers and the dormant Azure hook, §3.6 Smart App Control and SmartScreen). This spec replaces the Azure hook.
- **Builds on:** PR #25 (`claude/hardening`): pinned action SHAs, idempotent publish, and `releaseNotes({ channel, signed })` driven by the publish job's `SIGNED` variable.
- **Research:** sourced comparison in the session notes (`windows-code-signing-research.md`). The facts that decided this design are restated in §2.

## 1. Intent

The Windows installer ships unsigned. SmartScreen shows "Windows protected your PC" (passable through **More info** → **Run anyway**). On machines with **Smart App Control** enforcing, the installer is blocked outright, and the owner's own PC is one of them. After this work, every Windows release is Authenticode-signed under the **SignPath Foundation** certificate, so:

- Smart App Control lets the launcher install and run: the signature chains to a CA in Microsoft's Trusted Root Program;
- SmartScreen reputation starts building. The warning still appears on the first signed releases. Since 2024 no certificate type removes it at once (EV no longer does).

### 1.1 Success criteria

- With signing turned on, a Release run produces a Windows installer whose own signature, the launcher `.exe` inside it, and the installed `uninstall.exe` all carry a valid Authenticode signature from SignPath Foundation, timestamped.
- The `.sig` next to the installer is the Ed25519 updater signature **of the signed installer**, from the committed key. The existing manifest key-id check still passes, and installed launchers accept the update.
- A dry run proves this on a GitHub runner: it installs the signed installer silently and checks the installed files' signatures.
- With signing turned off (the default until SignPath approves the project), the Release workflow behaves exactly as today.
- Every release needs two approvals in SignPath. A dry run can skip them by signing with the test certificate.

### 1.2 Out of scope

- Linux signing (AppImage/.deb keep only the updater signature).
- Signing the NSIS plugin DLLs (§3.4).
- Removing SmartScreen warnings before reputation builds; submitting files to Microsoft for analysis.
- macOS.

## 2. Owner decisions and facts (2026-10-15)

- **Provider: SignPath Foundation** (free for OSS). The owner is an individual outside the US and Canada, and PerlandDev is not a registered company, so Azure Artifact Signing is unavailable: Microsoft limits individuals to the US/Canada, and organizations to US, CA, EU, UK, AU, NZ, JP, KR, SG, CH, NO and IL. Paid certificates (SSL.com eSigner, about $250–430/yr; Certum Open Source, €49/yr with no supported CI path) were declined. They would also print the owner's legal name and location on every installer.
- **Publisher shown to players: "SignPath Foundation"**, not Perland. The Foundation vouches that the binary was built from this public repository.
- **Approvals:** the owner accepts manual approval in SignPath for every release-certificate signing request (two per release) and the split build.
- **SignPath Foundation terms that shape the design** (signpath.org/terms):
  - OSI licence (GPL-3.0-only qualifies) and no proprietary code;
  - built from source in a verifiable way, on GitHub-hosted runners;
  - only the project's own binaries are signed, while upstream binaries may ship unsigned;
  - product-name metadata set to the project name and enforced;
  - every release manually approved;
  - all team members use MFA on GitHub and SignPath;
  - a "Code signing policy" section on the project's website.
- **Owner does, not agents:** apply at signpath.org/apply; enable MFA; create the SignPath API token and add it as a GitHub secret; paste the artifact configurations and build policy (§3.6) into SignPath; approve signing requests; any purchase or account step.

## 3. Design

### 3.1 Why two build passes

SignPath signs only files uploaded as a GitHub Actions artifact, and returns them after approval. Tauri 2.12's bundler instead calls `bundle.windows.signCommand` synchronously and expects the file signed **in place** at four points (read from `tauri-bundler` at `tauri-cli-v2.12.1`):

1. The main `.exe`, after Tauri patches it with the bundle type (`bundle.rs`: sign after `patch_binary`, then restore the unsigned, unpatched binary after each package type).
2. The NSIS plugin DLLs, in a per-build copy of the plugin folder (`nsis/mod.rs`, `NSIS_PLUGIN_FILES`).
3. The uninstaller, inside `makensis` through `!uninstfinalize` (`installer.nsi`). The signed uninstaller is embedded in the installer.
4. The finished installer, after `makensis`.

The uninstaller (3) must be signed before the installer (4) is compiled, so a single upload after the build cannot sign everything. The design runs the bundler twice, with a signing command that **collects** files on the first pass and **applies** signed copies on the second, matched by SHA-256.

### 3.2 Windows build flow (signing on)

All steps run in one `build` matrix job on `windows-latest`, so pass 2 reuses pass 1's compiled binary and folders:

1. **Pass 1:** `pnpm tauri build --bundles nsis` with `signCommand` = `node <repo>/scripts/release/cli.mjs sign-collect %1`.
   - The command copies each file it should sign into `$SIGN_DIR/unsigned/<sha256>.exe` and makes no change to the file itself.
   - `createUpdaterArtifacts` stays off in both passes.
2. **Upload** `$SIGN_DIR/unsigned/` with `actions/upload-artifact` (v4+, pinned) and keep its `artifact-id`.
3. **Round 1:** `signpath/github-action-submit-signing-request` (pinned SHA), artifact configuration `launcher-binaries`, signing policy per §3.5, `wait-for-completion-timeout-in-seconds: 3600`, `output-artifact-directory: $SIGN_DIR/signed`.
4. **Pass 2:** `pnpm tauri bundle --bundles nsis` (no recompile) with `signCommand` = `... sign-apply %1`.
   - For each file, the command looks up `$SIGN_DIR/signed/<sha256>.exe` and copies it over the file.
   - The final installer is left alone (§3.3).
   - A file whose hash round 1 never saw fails the build: "the build is not reproducible between passes".
5. **Upload** the installer alone and run **round 2** with artifact configuration `launcher-installer`. The signed installer replaces the unsigned one in `target/release/bundle/nsis/`.
6. **Updater signature:** `pnpm tauri signer sign --app-version "$VERSION" <installer>` (the version binding `tauri build` adds on its own), with `TAURI_SIGNING_PRIVATE_KEY` and `_PASSWORD` in `env:`, writes `<installer>.sig` over the **signed** bytes. Ed25519 is computed after Authenticode, as the updater requires.
7. **Verify** (§3.7), then the existing `stage` and `upload-artifact` steps.

The Linux matrix leg is unchanged: one `tauri build` with `createUpdaterArtifacts`. With signing off, the Windows leg is the same single `tauri build` as today.

### 3.3 What the signing command does with each file

`signRole(path)` in `scripts/release/lib.mjs` classifies by path (separators normalised, case-insensitive):

| Role | Rule | collect | apply |
|---|---|---|---|
| `plugin` | path contains `/plugins/x86-unicode/` | no-op | no-op |
| `installer` | under `/bundle/nsis/` and ends in `-setup.exe` | no-op | no-op (signed in round 2) |
| `binary` | anything else: the patched `perland-launcher.exe`, the uninstaller temp file | copy to `<sha256>.exe` | replace with the signed copy, or fail |

`collect` must see exactly the `binary` files that `apply` later sees. The uninstaller does not contain the app `.exe` (it holds only uninstall code and the plugins it calls), and the plugins are byte-identical across passes. So it should be identical in both passes. The dry run proves it. If it fails, the fallback is a third round: pass 2 collects the uninstaller after round 1, and pass 3 applies. That fallback is not built unless needed.

The command writes one log line per file (role and hash) to stdout, which Tauri shows. It never prints secrets, and it needs none.

### 3.4 NSIS plugin DLLs stay unsigned

SignPath Foundation signs only binaries built from this repository's source. `NSISdl.dll`, `StartMenu.dll`, `System.dll`, `nsDialogs.dll` and `nsis_tauri_utils.dll` are upstream binaries that the terms allow to ship unsigned. They are the stock files in every NSIS and Tauri installer, so Smart App Control's cloud reputation for those exact hashes is expected to be good. That is unverified: the owner's install test on a Smart App Control PC (§4) is the check. If Smart App Control blocks a plugin, the follow-up is a separate decision, outside this spec.

### 3.5 Workflow inputs, variables and secrets

- **Turn on:** repository variable `SIGNPATH_SIGNING=true`. Off by default.
- **Variables:** `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`.
- **Secret:** `SIGNPATH_API_TOKEN`. It is passed only to the SignPath action's `api-token` input and never echoed or interpolated into a `run:` script. `TAURI_SIGNING_*` stay in `env:`.
- **Signing policy:**
  - publish runs: `release-signing`;
  - dry runs: `test-signing` (SignPath's test certificate, no approval);
  - dry runs with the new input **"Sign with the release certificate"** (`release_certificate`, boolean, default false): `release-signing`, which is how the real certificate is verified before the first signed publish.
- `AZURE_TRUSTED_SIGNING`, the `AZURE_*` secrets and variables, and the `trusted-signing-cli` install step are removed.
- The publish job's `SIGNED` becomes `vars.SIGNPATH_SIGNING == 'true'`. It already drives `releaseNotes`, which drops the "not code-signed yet" sentence.
- `permissions: contents: read` stays. The repository is public, so SignPath reads the artifact with the default token. Only `publish` has `contents: write`.
- The Windows build's `timeout-minutes` goes from 60 to 180, to cover two approval waits of up to 60 minutes each.
- SignPath requires every job leading to a signing request to run on GitHub-hosted runners. All jobs already do.

### 3.6 SignPath configuration (owner pastes; files kept in the repo for review)

`.signpath/` holds the text the owner copies into SignPath. SignPath stores its own copy, so these files are documentation:

- `.signpath/artifact-configurations/launcher-binaries.xml`: a `<zip-file>` whose `<pe-file-set>` includes `*.exe` (two to four files) with `product-name="Perland Launcher"`, each Authenticode-signed.
- `.signpath/artifact-configurations/launcher-installer.xml`: a `<zip-file>` with one `*-setup.exe`, `product-name="Perland Launcher"`, Authenticode-signed.
- `.signpath/policies/github-build-policies.yml`: `runners.require_github_hosted: true`.

If the uninstaller's version resource lacks the product name, the dry run shows it (SignPath rejects the request). The plan then sets it through the NSIS template's version keys, or drops the restriction for that file only.

### 3.7 Verification in the workflow

A PowerShell script `scripts/release/verify-authenticode.ps1`, run on Windows when signing is on, after `signer sign`:

- runs `Get-AuthenticodeSignature` on the installer: it must have a signer certificate and a timestamp. When the release certificate was used, it must also be `Valid` with the signer subject containing `SignPath Foundation`;
- installs the installer silently (`/S`, per user) into the runner's `%LOCALAPPDATA%\Perland Launcher`, then checks every `.exe` in that folder (the launcher and `uninstall.exe`; at least two) by the same rules;
- fails the job, naming each file that doesn't pass.

The updater signature is already checked by the manifest step (key id). This spec does not add an Ed25519 verify step: Tauri's `signer sign` uses the same code path as the build, and the end-to-end update test (§4) covers it.

### 3.8 Code signing policy page (PerlandDev-site)

SignPath Foundation requires it. A separate small PR in `PerlandDev/PerlandDev-site`, after the owner's review, adds a "Code signing policy" section (en and es) to `/launcher`. It contains:

- "Free code signing provided by SignPath.io, certificate by SignPath Foundation";
- team roles: committers and reviewers, and approvers (the owner);
- a privacy statement naming the services the launcher contacts (Mojang/Microsoft, Modrinth, CurseForge if used, Perland servers, GitHub for updates) and saying it sends nothing else without the player's action.

The SmartScreen guidance on that page changes from "the installer isn't signed" to "it's signed; Windows may still warn on new versions while reputation builds". This ships only once the first signed release exists.

## 4. Testing

- **Unit (`node:test`, `scripts/release/lib.test.mjs`):**
  - `signRole` for the four kinds of path (main exe, uninstaller temp file, plugin, installer), on both Windows and POSIX separators;
  - `planApply({ role, hash, signed })` returns `skip`/`replace`, and throws for an unknown binary hash.
- **CLI behaviour:** `sign-collect` and `sign-apply` against temp files in `lib.test.mjs` or a small `cli.test.mjs`: copy-by-hash, replace, and fail on unknown.
- **Workflow dry runs.** Run 1 can happen at once. Runs 2 and 3 need SignPath to have accepted the project, because the policies and token only exist after that. The PR merges with run 1 verified and signing dormant, and runs 2 and 3 follow onboarding.
- **The runs** (`gh workflow run release.yml --ref claude/windows-signing -f channel=beta -f dry_run=true [-f release_certificate=true]`):
  1. with `SIGNPATH_SIGNING` unset: unchanged unsigned build (regression);
  2. with signing on and the test certificate: both rounds and the byte-identical uninstaller work, and verification passes the signature-present rules;
  3. with the release certificate: two approvals, then everything is `Valid` from SignPath Foundation.
- **Owner, on the Smart App Control PC:** download run 3's installer artifact, install, launch, uninstall, and confirm Smart App Control allows all three. Then a beta publish, after explicit go-ahead, followed by an auto-update from the previous beta.
- CLAUDE.md's pre-commit checks apply. The Rust and UI ones are unaffected but still run.

## 5. Risks

- **SignPath may decline or delay the application.** Until it approves, nothing changes for players, and the code merges dormant.
- **Uninstaller not byte-identical between passes** (§3.3): caught by `sign-apply` failing loudly; fallback is a third round.
- **Smart App Control and unsigned plugin DLLs** (§3.4): verified on the owner's PC; unknown until then.
- **Approval latency:** each round waits up to 60 minutes, and an unapproved request fails the run. Re-running the workflow is safe, because nothing is published before the build job succeeds.
- **SignPath action or terms change:** the action is pinned by SHA, and the terms are recorded in §2.
