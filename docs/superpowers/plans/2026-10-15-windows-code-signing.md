# Windows Code Signing (SignPath Foundation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Authenticode-sign the Windows installer, the launcher `.exe` and the uninstaller with the SignPath Foundation certificate inside the Release workflow, keeping the Ed25519 updater signature valid.

**Architecture:** The Windows release leg runs Tauri's bundler twice. Pass 1's `signCommand` (`cli.mjs sign-collect`) copies each own-built binary out by SHA-256. SignPath signs those files (round 1), and pass 2's `signCommand` (`cli.mjs sign-apply`) swaps the signed copies in by hash. The finished installer is signed in round 2. Then `tauri signer sign` writes the updater `.sig` over the signed bytes, and a PowerShell check installs the result and verifies every signature. Everything is dormant until the repository variable `SIGNPATH_SIGNING=true`.

**Tech Stack:** GitHub Actions; Tauri CLI 2.12.1 (`tauri build`, `tauri bundle`, `tauri signer sign`); Node 24 `.mjs` scripts tested with `node:test`; PowerShell 7 on `windows-latest`; `signpath/github-action-submit-signing-request` v3.0.

**Spec:** `docs/superpowers/specs/2026-10-15-windows-code-signing-design.md` (read it first). Parent: `docs/superpowers/specs/2026-10-14-distribution-design.md` §3.4–§3.6.

## Global Constraints

- The branch `claude/windows-signing` is cut from `origin/claude/hardening` (PR #25). Before opening the PR, rebase onto `origin/main` if #25 has merged. Otherwise open the PR against `claude/hardening` and say so.
- Pin every action by full commit SHA with a `# vX.Y.Z` comment, the style #25 uses. SignPath action: `signpath/github-action-submit-signing-request@f6d04783b4569d051e0c80105fe66e82819d0092 # v3.0`. Upload artifact: `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2` (already used in the file).
- Secrets are never interpolated into `run:` scripts. `TAURI_SIGNING_*` go through `env:`, and `SIGNPATH_API_TOKEN` only into the SignPath action's `api-token:` input.
- Top-level `permissions: contents: read` stays. Only `publish` has `contents: write`.
- Every job runs on GitHub-hosted runners (a SignPath Foundation rule).
- With `vars.SIGNPATH_SIGNING != 'true'`, the Windows leg must run exactly today's single `tauri build` with `createUpdaterArtifacts`.
- Signing policy slugs are `release-signing` (publish, or dry run with `release_certificate`) and `test-signing` (other dry runs). Artifact configuration slugs are `launcher-binaries` and `launcher-installer`.
- NSIS plugin DLLs are never sent to SignPath (Foundation terms: upstream binaries ship unsigned).
- Never run stress or fuzz tests locally (CLAUDE.md). The local tests here are tiny `node:test` cases.
- Every publish is public. Dry runs only, unless the owner explicitly approves a publish.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **The uninstaller differs between pass 1 and pass 2**, so `sign-apply` meets an unknown hash. Expected: the build fails with a message naming the hash and saying the build isn't reproducible, never a silently unsigned uninstaller. Pinned by Task 2's "unknown file fails" test.
2. **Windows paths with backslashes and spaces** (`D:\a\perland-launcher\...\Perland Launcher_0.2.0-beta.3_x64-setup.exe`, `C:\Users\RUNNER~1\AppData\Local\Temp\nst1234.tmp`). Expected: classified correctly whatever the separators and case. Pinned by Task 1's path tests.
3. **`SIGN_DIR` unset, or `sign-apply` running before round 1 delivered anything.** Expected: a clear error, never a no-op that ships unsigned files. Pinned by Task 2.
4. **A dry run with the test certificate** must not fail the verify step for "untrusted root", but a release-certificate run must demand `Valid` and the SignPath Foundation subject. Pinned by Task 3's two modes, exercised in Task 6's runs 2 and 3.
5. **The updater `.sig` made from the unsigned installer** (if `createUpdaterArtifacts` were left on in the passes) would ship a signature that no installed launcher accepts for the signed file. Expected: no `.sig` exists until `signer sign` runs on the signed installer. Pinned by Task 4: the passes run without `createUpdaterArtifacts`, and `stage` fails if `.sig` is missing.

---

## File Structure

- `scripts/release/lib.mjs`: add `signRole(path)` and `planApply({ role, hash, signed })`, both pure.
- `scripts/release/lib.test.mjs`: tests for the two.
- `scripts/release/cli.mjs`: add the `sign-collect` and `sign-apply` commands (file I/O around the pure helpers).
- `scripts/release/cli.test.mjs` (new): runs the two commands on temp files.
- `scripts/release/verify-authenticode.ps1` (new): signature checks and the silent install.
- `.github/workflows/release.yml`: the input, the dormant SignPath leg, removal of Azure, `SIGNED`, timeout.
- `.signpath/artifact-configurations/launcher-binaries.xml`, `.signpath/artifact-configurations/launcher-installer.xml`, `.signpath/policies/github-build-policies.yml`, `.signpath/README.md` (new): the text the owner pastes into SignPath.
- `README.md` (Releasing), `docs/superpowers/specs/2026-10-14-distribution-design.md` (§1.2, §3.5 pointers).
- `E:\PerlandDev-site`: a separate PR with the "Code signing policy" section (Task 7).

---

### Task 1: Classify files and plan the apply pass

**Files:**
- Modify: `scripts/release/lib.mjs` (append after `releaseNotes`)
- Test: `scripts/release/lib.test.mjs`

**Interfaces:**
- Produces: `signRole(path: string): 'plugin' | 'installer' | 'binary'`; `planApply({ role: string, hash: string, signed: Set<string> }): 'skip' | 'replace'` (throws for an unknown `binary` hash).

- [ ] **Step 1: Write the failing tests.** Add `planApply, signRole` to the import list in `lib.test.mjs`, then append:

```js
test('the signing command tells plugins, the installer and our own binaries apart', () => {
  assert.equal(signRole('D:\\a\\perland-launcher\\perland-launcher\\target\\release\\perland-launcher.exe'), 'binary');
  assert.equal(signRole('C:\\Users\\RUNNER~1\\AppData\\Local\\Temp\\nst1234.tmp'), 'binary');
  assert.equal(signRole('D:\\a\\p\\target\\release\\nsis\\x64\\Plugins\\x86-unicode\\System.dll'), 'plugin');
  assert.equal(signRole('D:/a/p/target/release/nsis/x64/plugins/X86-UNICODE/additional/nsis_tauri_utils.dll'), 'plugin');
  assert.equal(signRole('D:\\a\\p\\target\\release\\bundle\\nsis\\Perland Launcher_0.2.0-beta.3_x64-setup.exe'), 'installer');
  assert.equal(signRole('/home/runner/work/p/target/release/bundle/nsis/Perland Launcher_0.2.0_x64-setup.exe'), 'installer');
  // A -setup.exe outside bundle/nsis is not the final installer.
  assert.equal(signRole('D:\\a\\p\\target\\release\\other-setup.exe'), 'binary');
});

test('the apply pass replaces our binaries, skips the rest, and refuses unknown ones', () => {
  const signed = new Set(['aa11']);
  assert.equal(planApply({ role: 'binary', hash: 'aa11', signed }), 'replace');
  assert.equal(planApply({ role: 'plugin', hash: 'ffff', signed }), 'skip');
  assert.equal(planApply({ role: 'installer', hash: 'ffff', signed }), 'skip');
  assert.throws(() => planApply({ role: 'binary', hash: 'bb22', signed }), /bb22.*not reproducible/s);
});
```

- [ ] **Step 2: Run them and see them fail.** Run `pnpm --filter release-scripts test`. Expected: FAIL, `signRole` is not exported.

- [ ] **Step 3: Implement.** Append to `lib.mjs`:

```js
/** What the Windows signCommand does with a file Tauri hands it (signing spec §3.3). NSIS plugins are
 *  upstream binaries and stay unsigned; the final installer is signed in round 2; everything else is
 *  ours (the launcher .exe, the uninstaller) and is signed in round 1. */
export function signRole(path) {
  const p = String(path).replaceAll('\\', '/').toLowerCase();
  if (p.includes('/plugins/x86-unicode/')) return 'plugin';
  if (p.includes('/bundle/nsis/') && p.endsWith('-setup.exe')) return 'installer';
  return 'binary';
}

/** In pass 2, a binary must be one round 1 signed; anything else means the passes built different bytes. */
export function planApply({ role, hash, signed }) {
  if (role !== 'binary') return 'skip';
  if (signed.has(hash)) return 'replace';
  throw new Error(
    `Round 1 never saw this file (sha256 ${hash}), so the build is not reproducible between the two passes. ` +
      'See the signing spec §3.3 for the three-round fallback.',
  );
}
```

- [ ] **Step 4: Run the tests and see them pass.** Run `pnpm --filter release-scripts test`. Expected: all pass.

- [ ] **Step 5: Commit.**

```bash
git add scripts/release/lib.mjs scripts/release/lib.test.mjs
git commit -m "feat(release): classify files for two-pass Windows signing"
```

### Task 2: `sign-collect` and `sign-apply` commands

**Files:**
- Modify: `scripts/release/cli.mjs` (imports and two new `case`s)
- Create: `scripts/release/cli.test.mjs`

**Interfaces:**
- Consumes: `signRole`, `planApply` (Task 1).
- Produces: `node scripts/release/cli.mjs sign-collect <file>` and `... sign-apply <file>`. Both read `SIGN_DIR`. Collect writes `$SIGN_DIR/unsigned/<sha256>.exe`; apply reads `$SIGN_DIR/signed/<sha256>.exe`. Each prints one line: `<command> <role> <sha256> <file>`. The workflow (Task 4) relies on these paths.

- [ ] **Step 1: Write the failing tests** in `scripts/release/cli.test.mjs`:

```js
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const cli = fileURLToPath(new URL('./cli.mjs', import.meta.url));
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const run = (args, env) => execFileSync(process.execPath, [cli, ...args], { env: { ...process.env, ...env }, encoding: 'utf8', stdio: 'pipe' });

function scratch() {
  const dir = mkdtempSync(join(tmpdir(), 'perland-sign-'));
  const exe = join(dir, 'perland-launcher.exe');
  writeFileSync(exe, 'unsigned launcher');
  const plugin = join(dir, 'Plugins', 'x86-unicode', 'System.dll');
  mkdirSync(join(dir, 'Plugins', 'x86-unicode'), { recursive: true });
  writeFileSync(plugin, 'plugin');
  return { dir, exe, plugin, signDir: join(dir, 'sign') };
}

test('collect copies our binaries out by hash and leaves files and plugins untouched', () => {
  const { exe, plugin, signDir } = scratch();
  const out = run(['sign-collect', exe], { SIGN_DIR: signDir });
  run(['sign-collect', plugin], { SIGN_DIR: signDir });
  assert.deepEqual(readdirSync(join(signDir, 'unsigned')), [`${sha('unsigned launcher')}.exe`]);
  assert.equal(readFileSync(exe, 'utf8'), 'unsigned launcher');
  assert.match(out, /sign-collect binary [0-9a-f]{64} /);
});

test('apply swaps in the signed copy with the same hash', () => {
  const { exe, plugin, signDir } = scratch();
  mkdirSync(join(signDir, 'signed'), { recursive: true });
  writeFileSync(join(signDir, 'signed', `${sha('unsigned launcher')}.exe`), 'signed launcher');
  run(['sign-apply', exe], { SIGN_DIR: signDir });
  run(['sign-apply', plugin], { SIGN_DIR: signDir });
  assert.equal(readFileSync(exe, 'utf8'), 'signed launcher');
  assert.equal(readFileSync(plugin, 'utf8'), 'plugin');
});

test('apply fails on a binary round 1 never saw, and both commands need SIGN_DIR', () => {
  const { exe, signDir } = scratch();
  assert.throws(() => run(['sign-apply', exe], { SIGN_DIR: signDir }), /not reproducible/);
  assert.throws(() => run(['sign-collect', exe], { SIGN_DIR: '' }), /SIGN_DIR/);
});
```

- [ ] **Step 2: Run them and see them fail.** Run `pnpm --filter release-scripts test`. Expected: FAIL with "Unknown command sign-collect".

- [ ] **Step 3: Implement.** In `cli.mjs`:
  - add `createHash` from `node:crypto`;
  - add `existsSync` to the `node:fs` import;
  - add `planApply, signRole` to the `./lib.mjs` import;
  - add before `default:`:

```js
  case 'sign-collect':
  case 'sign-apply': {
    // Tauri's bundle.windows.signCommand in the two-pass Windows build (signing spec §3.2–§3.3).
    const [file] = args;
    const dir = process.env.SIGN_DIR;
    if (!dir) throw new Error('SIGN_DIR is not set');
    const role = signRole(file);
    const hash = createHash('sha256').update(readFileSync(file)).digest('hex');
    if (command === 'sign-collect') {
      if (role === 'binary') {
        mkdirSync(join(dir, 'unsigned'), { recursive: true });
        copyFileSync(file, join(dir, 'unsigned', `${hash}.exe`));
      }
    } else {
      const signedDir = join(dir, 'signed');
      const signed = new Set(existsSync(signedDir) ? readdirSync(signedDir).map((n) => n.replace(/\.exe$/i, '')) : []);
      if (planApply({ role, hash, signed }) === 'replace') copyFileSync(join(signedDir, `${hash}.exe`), file);
    }
    console.log(`${command} ${role} ${hash} ${file}`);
    break;
  }
```

- [ ] **Step 4: Run the tests and see them pass.** Run `pnpm --filter release-scripts test`. Expected: all pass. Also run `pnpm -r lint` and `pnpm -r typecheck` (CLAUDE.md), which must stay green.

- [ ] **Step 5: Commit.**

```bash
git add scripts/release/cli.mjs scripts/release/cli.test.mjs
git commit -m "feat(release): sign-collect and sign-apply commands for Tauri's signCommand"
```

### Task 3: Authenticode verification script

**Files:**
- Create: `scripts/release/verify-authenticode.ps1`

**Interfaces:**
- Produces: `pwsh scripts/release/verify-authenticode.ps1 -Installer <path> [-RequireTrusted]`. Exit code 0 when everything passes. Otherwise it prints `::error::` lines and exits 1. Task 4 calls it.

This script runs only on the Windows runner, and it installs software, so it isn't run locally. Its test is the dry runs in Task 6 (Review Focus item 4).

- [ ] **Step 1: Write the script.**

```powershell
# Checks the Authenticode signatures of a built installer and of what it installs (signing spec §3.7).
# Runs on the Windows release runner only: it installs the launcher for the current user.
param(
  [Parameter(Mandatory)] [string] $Installer,
  # Release certificate: the signature must be Valid and from SignPath Foundation. Without it (test
  # certificate) a signature and a timestamp must be present, but the root may be untrusted.
  [switch] $RequireTrusted
)
$ErrorActionPreference = 'Stop'
$failures = @()

function Test-File([string] $Path) {
  $sig = Get-AuthenticodeSignature -LiteralPath $Path
  $name = Split-Path $Path -Leaf
  if (-not $sig.SignerCertificate) { return "$name has no signature" }
  if (-not $sig.TimeStamperCertificate) { return "$name has no timestamp" }
  if ($RequireTrusted) {
    if ($sig.Status -ne 'Valid') { return "$name signature is $($sig.Status): $($sig.StatusMessage)" }
    if ($sig.SignerCertificate.Subject -notmatch 'SignPath Foundation') { return "$name is signed by $($sig.SignerCertificate.Subject)" }
  }
  Write-Host "ok  $name  $($sig.Status)  $($sig.SignerCertificate.Subject)"
  return $null
}

$failures += Test-File $Installer

$installDir = Join-Path $env:LOCALAPPDATA 'Perland Launcher'
$p = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { $failures += "the installer exited with $($p.ExitCode)" }
$exes = @(Get-ChildItem -LiteralPath $installDir -Filter *.exe -ErrorAction SilentlyContinue)
if ($exes.Count -lt 2) { $failures += "expected the launcher and uninstall.exe in $installDir, found $($exes.Count) .exe files" }
foreach ($exe in $exes) { $failures += Test-File $exe.FullName }

$failures = @($failures | Where-Object { $_ })
foreach ($f in $failures) { Write-Host "::error::$f" }
if ($failures.Count -gt 0) { exit 1 }
```

- [ ] **Step 2: Parse-check it locally (no execution).** Run this in PowerShell:

```powershell
$null = [System.Management.Automation.Language.Parser]::ParseFile("$PWD\scripts\release\verify-authenticode.ps1", [ref]$null, [ref]$errs); $errs
```

Expected: no output (no parse errors).

- [ ] **Step 3: Commit.**

```bash
git add scripts/release/verify-authenticode.ps1
git commit -m "feat(release): verify Authenticode signatures of the installer and installed files"
```

### Task 4: Release workflow: dormant SignPath leg

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `cli.mjs sign-collect|sign-apply` (Task 2) and `verify-authenticode.ps1` (Task 3).
- Produces: a workflow input `release_certificate`; repo variables `SIGNPATH_SIGNING`, `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`; secret `SIGNPATH_API_TOKEN`. README (Task 5) documents them.

- [ ] **Step 1: Add the input** under `workflow_dispatch.inputs`, after `dry_run`:

```yaml
      release_certificate:
        description: Sign with the release certificate (asks for two approvals in SignPath). Publishes always do.
        type: boolean
        default: false
```

- [ ] **Step 2: Raise the build timeout.** Set `timeout-minutes: 180` on the `build` job, and add a comment: "two SignPath approval waits of up to 60 minutes each".

- [ ] **Step 3: Replace the Azure steps.** Delete the comment block, the "Install the signing tool" step, the `SIGN_WINDOWS` and `AZURE_*` env entries, and the `if [ "$SIGN_WINDOWS" ...` block. The unsigned build step becomes:

```yaml
      # Windows Authenticode signing (signing spec): off until the repository variable SIGNPATH_SIGNING
      # is true. With it off, every platform builds exactly as before.
      - name: Build and sign the updater artifacts
        if: runner.os != 'Windows' || vars.SIGNPATH_SIGNING != 'true'
        shell: bash
        env:
          PERLAND_BUILD: ${{ inputs.channel == 'beta' && 'beta' || '' }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
          BUNDLES: ${{ matrix.bundles }}
        run: pnpm tauri build --bundles "$BUNDLES" --config '{"bundle":{"createUpdaterArtifacts":true}}'
```

- [ ] **Step 4: Add the signed Windows leg** right after it. Every step has `if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'`. Steps:

```yaml
      - name: "Signing pass 1: build and collect our binaries"
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        shell: bash
        env:
          PERLAND_BUILD: ${{ inputs.channel == 'beta' && 'beta' || '' }}
        run: |
          echo "SIGN_DIR=$(cygpath -m "$RUNNER_TEMP/sign")" >> "$GITHUB_ENV"
          export SIGN_DIR="$(cygpath -m "$RUNNER_TEMP/sign")"
          cli="$(cygpath -m "$GITHUB_WORKSPACE/scripts/release/cli.mjs")"
          pnpm tauri build --bundles nsis --config "{\"bundle\":{\"windows\":{\"signCommand\":{\"cmd\":\"node\",\"args\":[\"$cli\",\"sign-collect\",\"%1\"]}}}}"
          ls "$SIGN_DIR/unsigned"
      - name: Upload our binaries for signing
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        id: unsigned-binaries
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: signing-binaries
          path: ${{ env.SIGN_DIR }}/unsigned/*
          if-no-files-found: error
          retention-days: 1
      - name: "Signing round 1: our binaries (approve in SignPath)"
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        uses: signpath/github-action-submit-signing-request@f6d04783b4569d051e0c80105fe66e82819d0092 # v3.0
        with:
          api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
          organization-id: ${{ vars.SIGNPATH_ORGANIZATION_ID }}
          project-slug: ${{ vars.SIGNPATH_PROJECT_SLUG }}
          signing-policy-slug: ${{ (!inputs.dry_run || inputs.release_certificate) && 'release-signing' || 'test-signing' }}
          artifact-configuration-slug: launcher-binaries
          github-artifact-id: ${{ steps.unsigned-binaries.outputs.artifact-id }}
          wait-for-completion-timeout-in-seconds: 3600
          output-artifact-directory: ${{ env.SIGN_DIR }}/signed
      - name: "Signing pass 2: bundle with the signed binaries"
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        shell: bash
        env:
          PERLAND_BUILD: ${{ inputs.channel == 'beta' && 'beta' || '' }}
        run: |
          cli="$(cygpath -m "$GITHUB_WORKSPACE/scripts/release/cli.mjs")"
          pnpm tauri bundle --bundles nsis --config "{\"bundle\":{\"windows\":{\"signCommand\":{\"cmd\":\"node\",\"args\":[\"$cli\",\"sign-apply\",\"%1\"]}}}}"
      - name: Upload the installer for signing
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        id: unsigned-installer
        uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4.6.2
        with:
          name: signing-installer
          path: target/release/bundle/nsis/*-setup.exe
          if-no-files-found: error
          retention-days: 1
      - name: "Signing round 2: the installer (approve in SignPath)"
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        uses: signpath/github-action-submit-signing-request@f6d04783b4569d051e0c80105fe66e82819d0092 # v3.0
        with:
          api-token: ${{ secrets.SIGNPATH_API_TOKEN }}
          organization-id: ${{ vars.SIGNPATH_ORGANIZATION_ID }}
          project-slug: ${{ vars.SIGNPATH_PROJECT_SLUG }}
          signing-policy-slug: ${{ (!inputs.dry_run || inputs.release_certificate) && 'release-signing' || 'test-signing' }}
          artifact-configuration-slug: launcher-installer
          github-artifact-id: ${{ steps.unsigned-installer.outputs.artifact-id }}
          wait-for-completion-timeout-in-seconds: 3600
          output-artifact-directory: ${{ env.SIGN_DIR }}/installer
      - name: Updater signature over the signed installer
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        shell: bash
        env:
          VERSION: ${{ needs.plan.outputs.version }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        run: |
          shopt -s nullglob
          signed=("$SIGN_DIR"/installer/*-setup.exe)
          test "${#signed[@]}" -eq 1
          target="target/release/bundle/nsis/$(basename "${signed[0]}")"
          test -f "$target"
          cp "${signed[0]}" "$target"
          pnpm tauri signer sign --app-version "$VERSION" "$target"
      - name: Check the Authenticode signatures
        if: runner.os == 'Windows' && vars.SIGNPATH_SIGNING == 'true'
        shell: pwsh
        env:
          TRUSTED: ${{ (!inputs.dry_run || inputs.release_certificate) && 'true' || 'false' }}
        run: |
          $installer = (Get-ChildItem target/release/bundle/nsis/*-setup.exe).FullName
          if ($env:TRUSTED -eq 'true') { ./scripts/release/verify-authenticode.ps1 -Installer $installer -RequireTrusted }
          else { ./scripts/release/verify-authenticode.ps1 -Installer $installer }
```

`stage` and `upload-artifact` stay as they are. `stage` copies the installer and its `.sig`, and fails if the `.sig` is missing (Review Focus item 5).

- [ ] **Step 5: Publish job.** Change `SIGNED: ${{ vars.AZURE_TRUSTED_SIGNING == 'true' }}` to `SIGNED: ${{ vars.SIGNPATH_SIGNING == 'true' }}`.

- [ ] **Step 6: Check the edit.** `grep -n "AZURE\|trusted-signing" .github/workflows/release.yml` must print nothing. If `actionlint` is installed, run `actionlint .github/workflows/release.yml`. Otherwise run 1 in Task 6 is the syntax check, because GitHub rejects an invalid workflow when it's dispatched.

- [ ] **Step 7: Commit.**

```bash
git add .github/workflows/release.yml
git commit -m "build(release): two-pass SignPath signing for the Windows installer, dormant by default"
```

### Task 5: SignPath configuration files and docs

**Files:**
- Create: `.signpath/README.md`, `.signpath/artifact-configurations/launcher-binaries.xml`, `.signpath/artifact-configurations/launcher-installer.xml`, `.signpath/policies/github-build-policies.yml`
- Modify: `README.md` (Releasing bullets), `docs/superpowers/specs/2026-10-14-distribution-design.md` (§1.2 and §3.5 point at the new spec)

- [ ] **Step 1: Artifact configurations.** `launcher-binaries.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<!-- Round 1: the launcher .exe and the uninstaller, named <sha256>.exe by cli.mjs sign-collect. -->
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file-set>
      <include path="*.exe" min-matches="2" max-matches="4" product-name="Perland Launcher" />
      <for-each>
        <authenticode-sign />
      </for-each>
    </pe-file-set>
  </zip-file>
</artifact-configuration>
```

`launcher-installer.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<!-- Round 2: the NSIS installer built around the round-1 binaries. -->
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="*-setup.exe" product-name="Perland Launcher">
      <authenticode-sign />
    </pe-file>
  </zip-file>
</artifact-configuration>
```

- [ ] **Step 2: Build policy.** `.signpath/policies/github-build-policies.yml`:

```yaml
github-build-policies:
  version: '1.0'
  disallow_reruns: false
  runners:
    require_github_hosted: true
```

- [ ] **Step 3: `.signpath/README.md`.** One short page covering:
  - SignPath stores its own copy, so these files are the reviewed source the owner pastes in;
  - project slug, the two policy slugs and the two artifact-configuration slugs, matching `release.yml`;
  - each release asks for two approvals;
  - plugin DLLs stay unsigned on purpose (spec §3.4).

- [ ] **Step 4: README Releasing.** Replace the sentence "Authenticode signing turns on with the variable `AZURE_TRUSTED_SIGNING=true` ..." with a new bullet:

```markdown
- Windows installers are Authenticode-signed by SignPath Foundation once the variable `SIGNPATH_SIGNING=true` is set, with the variables `SIGNPATH_ORGANIZATION_ID` and `SIGNPATH_PROJECT_SLUG` and the secret `SIGNPATH_API_TOKEN`. Each signed run asks for two approvals in SignPath: the launcher and uninstaller, then the installer. A dry run signs with SignPath's test certificate unless **Sign with the release certificate** is ticked. Setup and the configuration to paste into SignPath are in `.signpath/README.md`; the design is `docs/superpowers/specs/2026-10-15-windows-code-signing-design.md`.
```

- [ ] **Step 5: Distribution spec pointers.**
  - In §1.2, change "Authenticode signing. The pipeline has a dormant hook for Azure Trusted Signing (§3.5); builds ship unsigned." to "Authenticode signing: designed separately in `2026-10-15-windows-code-signing-design.md` (SignPath Foundation), which replaced the Azure hook."
  - In §3.5, change the "Authenticode signing (dormant)" bullet the same way.

- [ ] **Step 6: Commit.**

```bash
git add .signpath README.md docs/superpowers/specs/2026-10-14-distribution-design.md
git commit -m "docs(release): SignPath configuration and signing setup"
```

### Task 6: Verify with dry runs, then open the PR

**Files:** none. Uses `gh`.

- [ ] **Step 1: Local checks (CLAUDE.md).** Run `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm -r test`. All must pass. Run the Rust ones in the background, since they take minutes.

- [ ] **Step 2: Push and run 1 (signing off).**

```bash
git push -u origin claude/windows-signing
gh workflow run release.yml --ref claude/windows-signing -f channel=beta -f dry_run=true
```

Watch with `gh run watch <id> --exit-status` in the background.
- Expected: green.
- The Windows log shows the single `tauri build` step, and the signing steps show as skipped.
- The `bundles-Windows` artifact has the installer and its `.sig`.

- [ ] **Step 3: Open the PR.**
  - Base: `main` if #25 has merged (rebase first); otherwise `claude/hardening`.
  - The body says run 1 is verified, and runs 2 and 3 wait for SignPath onboarding (owner steps listed).
  - End the body with the Claude Code attribution line.
  - Bind it with the ccd_pr tools.

- [ ] **Step 4 (after the owner's SignPath onboarding): run 2.** The owner sets `SIGNPATH_SIGNING=true`, the two variables and the secret. Then:

```bash
gh workflow run release.yml --ref claude/windows-signing -f channel=beta -f dry_run=true
```

- Expected: both rounds complete with no approval (test-signing).
- Pass 2 logs `sign-apply binary <hash>` for the launcher and the uninstaller, with no "not reproducible" error.
- The verify step prints `ok` for the installer, the launcher and `uninstall.exe`.
- If pass 2 fails with "not reproducible", stop and report. The three-round fallback needs a design decision (spec §5).

- [ ] **Step 5: Run 3 (release certificate).**

```bash
gh workflow run release.yml --ref claude/windows-signing -f channel=beta -f dry_run=true -f release_certificate=true
```

- The owner approves twice in SignPath.
- Expected: the verify step reports `Valid` and `SignPath Foundation` for all three files.
- Download the artifact and run `Get-AuthenticodeSignature` locally on the `.exe` as a second check. This only reads the signature and runs nothing.

- [ ] **Step 6: Owner's Smart App Control test.** The owner installs run 3's installer on their Smart App Control PC, then launches and uninstalls it. Record the result in the PR. A real publish follows only on the owner's explicit go-ahead.

### Task 7: Code signing policy section on perland.online (separate repo)

**Files:** in `E:\PerlandDev-site` (Astro): the `/launcher` and `/es/launcher` pages. Find them with `Glob` for `**/launcher*.astro` and the i18n strings they use.

- [ ] **Step 1: Branch** `claude/code-signing-policy` from that repo's `main`.
- [ ] **Step 2: Add a "Code signing policy" section** (en and es, informal "tú" in Spanish) with:
  - "Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org)";
  - team roles: committers and reviewers, the PerlandDev GitHub members; approvers, the owner;
  - privacy: the launcher talks to Mojang/Microsoft (game files, sign-in), Modrinth and other mod sources the player picks, Perland servers the player joins, and GitHub (launcher updates), and sends nothing else unless the player asks.
- [ ] **Step 3: Leave the SmartScreen guidance unchanged** until the first signed release is public (spec §3.8). Note that in the PR body.
- [ ] **Step 4: Build the site the way its README says, commit, and show the owner the diff.** Open the PR only after the owner OKs the text. The owner needs this page live before applying to SignPath.
