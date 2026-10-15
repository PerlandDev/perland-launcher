# SignPath configuration

Windows releases are Authenticode-signed by [SignPath Foundation](https://signpath.org) through [SignPath.io](https://about.signpath.io). The design is `docs/superpowers/specs/2026-10-15-windows-code-signing-design.md`.

SignPath keeps its own copy of this configuration. The files here are the reviewed source: when one changes, paste it into SignPath by hand.

| SignPath setting | Value | Used by |
|---|---|---|
| Project slug | the repository variable `SIGNPATH_PROJECT_SLUG` | `release.yml` |
| Signing policies | `release-signing` (publishes, and dry runs with **Sign with the release certificate**), `test-signing` (other dry runs) | `release.yml` |
| Artifact configuration `launcher-binaries` | `artifact-configurations/launcher-binaries.xml` | round 1 |
| Artifact configuration `launcher-installer` | `artifact-configurations/launcher-installer.xml` | round 2 |
| GitHub build policy (on both signing policies) | `policies/github-build-policies.yml` | SignPath's GitHub connector |

## Turning it on

The owner does these steps; agents never handle the token.

1. Get the project accepted at [signpath.org/apply](https://signpath.org/apply). The code signing policy section on perland.online/launcher must be live first.
2. Turn on MFA for GitHub and SignPath, for everyone on the team.
3. In SignPath, create the two artifact configurations and set the build policy from the files above.
4. Create a SignPath API token for a CI user that may submit to both policies.
5. In GitHub (Settings → Secrets and variables → Actions):
   - add the secret `SIGNPATH_API_TOKEN`;
   - add the variables `SIGNPATH_ORGANIZATION_ID` and `SIGNPATH_PROJECT_SLUG`;
   - set `SIGNPATH_SIGNING` to `true`.

## What a signed run does

1. Tauri builds the installer. Its `signCommand` (`scripts/release/cli.mjs sign-collect`) copies the launcher `.exe` and the uninstaller out, named by SHA-256.
2. **Round 1:** SignPath signs them. A `release-signing` request waits for an approval.
3. Tauri bundles again. `sign-apply` swaps in the signed copies by hash. `sign-check` then reads `ledger.log`, because makensis ignores the uninstaller sign command's exit code, and fails the step if the second pass produced different bytes.
4. **Round 2:** SignPath signs the installer. This is the second approval.
5. `tauri signer sign` writes the updater `.sig` over the signed installer. `scripts/release/verify-authenticode.ps1` installs it on the runner and checks every signature.

The NSIS plugin DLLs (`System.dll`, `nsDialogs.dll`, `nsis_tauri_utils.dll`, and the rest) are never sent. SignPath Foundation signs only binaries built from this repository, and upstream binaries ship unsigned.
