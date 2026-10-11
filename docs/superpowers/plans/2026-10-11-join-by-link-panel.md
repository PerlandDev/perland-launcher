# Join by Link (Panel) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Execute in a fresh worktree of E:\PerlandPanel based on origin/main, after the Engine plan's PR merges.**

**Goal:** On a server's Invite sheet, while the invite is on and the engine sends a `launcher` block, show a secondary **Open in launcher** link (`perland://join?invite=…`, built by the engine) beside Copy and Open, plus the line "Players need Perland Launcher · Download".

**Architecture:** `toInviteView` (`app/lib/invite.ts`, pure, unit tested) maps the owner `InviteDTO.launcher` to `InviteView.launcher: { joinUrl, downloadUrl } | null`. It drops any block whose `joinUrl` is not `perland://join?invite=…` or whose `downloadUrl` is not https, and maps the new `pack.commit` into the preview. `LinkSlot` in `app/components/invite/invite-dialog.tsx` renders the extra link and line in its "on, with a url" branch. `ViewerLink` renders `LinkSlot`, so viewers get it too. `scripts/mock-engine.mjs` mirrors the engine: `pack.commit` in every invite view, and the `launcher` blocks behind `MOCK_LAUNCHER_DOWNLOAD_URL`.

**Tech Stack:** Next.js 16 (App Router), React 19, TypeScript 5, Tailwind 4 with the panel's `hud-*` classes, Vitest (node environment, `react-dom/server` rendering, no Testing Library), `@perland/api-client` generated from the engine's `docs/swagger.json`.

**Spec:** `docs/superpowers/specs/2026-10-11-join-by-link-design.md` in the perland-launcher repository (`E:\perland-launcher`; on branch `claude/ecstatic-pascal-b946ca` until it merges), §3.2 "Panel" only. §1.1 decisions 3 and 7 and §3.1.2 define the engine fields this plan reads. The Engine plan (PerlandEngine, same spec, §3.1) must be merged first.

## Global Constraints

- Repository: PerlandPanel (`E:\PerlandPanel`). Branch `claude/join-by-link-panel` from `origin/main`, in a fresh worktree. All paths below are relative to the panel repository root unless absolute.
- Engine fields read (from spec §3.1): `PackDTO.commit` (`json:"commit,omitempty"`); public `InviteViewDTO.launcher: { downloadUrl }`; owner `InviteDTO.launcher: { joinUrl, downloadUrl }` (stacks only, only with `LAUNCHER_DOWNLOAD_URL` set, `joinUrl` only with `INVITE_BASE_URL` set), `joinUrl = perland://join?invite=<url-encoded invite url>`.
- "The Engine owns the `perland://` format, so the Panel never builds it." The panel renders `launcher.joinUrl` as given. Only the mock (an engine stand-in) builds it, the way the engine does.
- Show **Open in launcher** and the download line only when `invite.launcher` is present **and** `invite.enabled` is true.
- Engine-first (panel CLAUDE.md): if the merged engine lacks a field this plan reads, stop and ask the owner. Never work around it in the panel.
- No text in components. Every string goes in `app/i18n/dictionaries/en/invite.ts` with its translation in `es/` and `ca/` (the type check fails until all three agree). Spanish is neutral Latin American, `tú`. Catalan is standard central with imperative buttons. "Perland Launcher" is a name and is never translated. Spanish and Catalan keep the noun "launcher".
- UI styling and design-system work go through the `/impeccable` skill. The design system is `DESIGN.md` (its "Invite sheet" section is the brief for this change).
- Never edit or regenerate `E:\PerlandEngine\sdk\out`. It is shared by every panel worktree. Generate the SDK in an engine worktree instead (Task 1).
- Fake tokens only, for example `"A".repeat(43)`. The real Asalith invite token never goes into code, tests, docs, commits, PRs or logs.
- No fuzz, stress or load runs locally. Keep test inputs tiny.
- This change touches no route, layout, data fetching or metadata. If that changes, read `.claude/skills/next-best-practices/SKILL.md` first.
- Panel checks on this machine (`pnpm exec` fails the deps-status check): `node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json`, `node node_modules/eslint/bin/eslint.js app scripts middleware.ts`, `node node_modules/vitest/vitest.mjs run`, `node node_modules/next/dist/bin/next build`. `pnpm install` may rewrite `pnpm-workspace.yaml` with an `allowBuilds` placeholder: revert it before committing (`git checkout -- pnpm-workspace.yaml`).
- Commit messages end with the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- The PR body carries `Roadmap: PerlandDev/PerlandEngine#<n>` on a line of its own (Task 6).

## Review Focus

1. **An engine value that is not the expected link.** A `launcher.joinUrl` that is `javascript:…`, `https://…` or `perland://pack?…`, or a `downloadUrl` that is not https, must never become an `href`. Expected: no launcher row at all. Pinned in Task 2 ("drops a block it would not render as a link").
2. **The invite is turned off while the engine still sends `launcher`.** The engine sends the owner block whenever the env is set, whatever the switch says. Expected: no Open in launcher and no download line, for owners and for viewers. Pinned in Task 4 ("hides both while the invite is off", "ViewerLink … hides it while off").
3. **An engine without a play site address (`INVITE_BASE_URL` unset).** The sheet shows the token, not a link. Expected: no launcher row, even if a malformed engine sent a block. Pinned in Task 4 ("hides both without a play site address").
4. **Replace link.** After a regenerate, the sheet focuses `[data-link-slot] button`. That must still be Copy, never the new link. Expected: the launcher control is an `<a>`, and the link row holds exactly one `<button>`. Pinned in Task 4 ("keeps Copy the only button in the link row").
5. **An older engine with no `launcher` and no `commit`.** Expected: the sheet is exactly as before, and `preview.pack.commit` is `""`. Pinned in Task 2 ("has none without the engine's block", "reads the pack's commit, empty from an older engine").

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `app/lib/invite.ts` | `InviteLauncher`, `InviteView.launcher`, `InvitePack.commit`, `toInviteLauncher`, `toInviteView` | 2 |
| `app/lib/invite.test.ts` | Tests of the mapping | 2 |
| `scripts/mock-engine.mjs` | `pack.commit` in `inviteView`; the `launcher` blocks behind `MOCK_LAUNCHER_DOWNLOAD_URL`; header usage line | 3 |
| `app/components/invite/invite-dialog.tsx` | `LinkSlot` gets the launcher link and line; `LinkSlot` and `ViewerLink` exported for tests | 4 |
| `app/components/invite/invite-dialog.test.tsx` (new) | Server-rendered tests of `LinkSlot` and `ViewerLink` | 4 |
| `app/i18n/dictionaries/{en,es,ca}/invite.ts` | `invite.link.launcher.*` | 4 |
| `DESIGN.md` | "Invite sheet": the launcher link and line | 5 |
| `CLAUDE.md` | "Invites" bullet: the `launcher` block and the mock variable | 5 |

No SDK model class is imported: the panel reads invite DTOs structurally (`InviteLike`), so the SDK only needs to carry the fields for the types to line up.

---

### Task 1: Workspace and SDK

No commit. The goal is a panel worktree that type-checks against an SDK generated from the merged engine.

The panel resolves `@perland/api-client` from `../PerlandEngine/sdk/out` (`pnpm-workspace.yaml` overrides it with `file:`). It is **not** a versioned package, so there is no version to bump. CI generates it from the engine's `docs/swagger.json` at `ENGINE_REF`, or engine `main` when that is unset. Every panel worktree reaches `E:\PerlandEngine` through one shared junction, `E:\PerlandPanel\.claude\worktrees\PerlandEngine`. So the SDK is generated in an engine worktree, and only `dist` in this worktree's own pnpm copy is replaced.

**Interfaces:**
- Produces: `ENGINE_PR`, the number of the merged engine PR that added `launcher` and `pack.commit` (used in Tasks 3 and 5). Also the generated model names for the two launcher objects, noted for Task 6's PR body.

- [ ] **Step 1: Confirm the Engine PR has merged (stop-and-ask gate)**

```bash
git -C /e/PerlandEngine fetch origin
MSYS_NO_PATHCONV=1 git -C /e/PerlandEngine grep -n "LAUNCHER_DOWNLOAD_URL" origin/main -- cmd internal config | head -5
MSYS_NO_PATHCONV=1 git -C /e/PerlandEngine show origin/main:docs/swagger.json | grep -n '"joinUrl"\|"downloadUrl"\|"launcher"' | head
gh pr list -R PerlandDev/PerlandEngine --state merged --search "launcher in:title" --limit 5
```

Expected: the env var is in engine code on `origin/main`, the swagger has `launcher`, `joinUrl` and `downloadUrl`, and one merged PR matches. Note its number as `ENGINE_PR`.

**If any of the three is missing, STOP.** Tell the owner the Engine PR (spec §3.1) has not merged, and ask how to proceed. Do not point `ENGINE_REF` at an engine branch, and do not continue without the owner's answer.

Also run `gh variable list -R PerlandDev/PerlandPanel`. If `ENGINE_REF` names a branch other than `main`, tell the owner, since CI would type-check against it. Do not change it yourself.

- [ ] **Step 2: Generate the SDK in an engine worktree on `origin/main`**

```powershell
git -C E:\PerlandEngine worktree add --detach E:\PerlandEngine\.claude\worktrees\join-by-link-sdk origin/main
```

Then, in `E:\PerlandEngine\.claude\worktrees\join-by-link-sdk` (Git Bash):

```bash
npx --yes @openapitools/openapi-generator-cli generate -i docs/swagger.json -g typescript -o sdk/out -c sdk/config.json
cd sdk/out && npm install --no-audit --no-fund && npm run build
```

Expected: `sdk/out/dist` exists.

- [ ] **Step 3: Read the generated names**

In that engine worktree:

```bash
cd sdk/out
ls models | grep -i invitedtos
grep -n "launcher\|commit" models/InvitedtosInviteDTO.ts models/InvitedtosInviteViewDTO.ts models/InvitedtosPackDTO.ts
grep -ln "joinUrl\|downloadUrl" models/*.ts
```

Expected:
- `InvitedtosPackDTO` has an optional `commit?: string`.
- `InvitedtosInviteDTO` and `InvitedtosInviteViewDTO` each have an optional `launcher` typed as a generated class, likely `InvitedtosLauncherDTO` (or two classes, for example `InvitedtosOwnerLauncherDTO` with `joinUrl` and `downloadUrl` and `InvitedtosLauncherDTO` with `downloadUrl`).

Write down the actual names. The panel never imports them: `InviteLike` (Task 2) is structural, and a generated class with optional string fields is assignable to it. If a field is generated as anything other than an optional string or a nested object of optional strings, adjust Task 2's `LauncherLike` / `PackLike` to match before going on.

- [ ] **Step 4: Create the panel worktree**

If the session was not already started in a fresh panel worktree:

```powershell
git -C E:\PerlandPanel fetch origin
git -C E:\PerlandPanel worktree add -b claude/join-by-link-panel E:\PerlandPanel\.claude\worktrees\join-by-link-panel origin/main
Test-Path E:\PerlandPanel\.claude\worktrees\PerlandEngine
```

Expected: `True`. If it is `False`, create the shared junction: `New-Item -ItemType Junction -Path E:\PerlandPanel\.claude\worktrees\PerlandEngine -Target E:\PerlandEngine`.

In the panel worktree: `pnpm install --frozen-lockfile`, then `git checkout -- pnpm-workspace.yaml` if it changed.

- [ ] **Step 5: Swap in the new SDK `dist`, guarded**

In the panel worktree (Git Bash):

```bash
real=$(cd node_modules/@perland/api-client && pwd -P); echo "$real"
case "$real" in *node_modules/.pnpm*) echo "ok: pnpm copy" ;; *) echo "STOP: resolves outside node_modules/.pnpm"; exit 1 ;; esac
rm -rf "$real/dist"
cp -r /e/PerlandEngine/.claude/worktrees/join-by-link-sdk/sdk/out/dist "$real/dist"
grep -rl "joinUrl" "$real/dist/types" | head -3
```

Expected: "ok: pnpm copy", and at least one `.d.ts` that names `joinUrl`. If the guard prints STOP, the package is a link into `E:\PerlandEngine\sdk\out`. Do not delete anything; ask the owner. `rm` comes before `cp` because the files are hard links into the global pnpm store. A later `pnpm install` restores the old SDK, so repeat this step after any install.

- [ ] **Step 6: Baseline checks**

```bash
node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json
node node_modules/vitest/vitest.mjs run app/lib/invite.test.ts
```

Expected: both pass before any change. A type error in files this plan does not touch means the SDK is stale: redo Steps 2 and 5.

---

### Task 2: `toInviteView` carries `launcher` and `pack.commit`

**Files:**
- Modify: `app/lib/invite.ts`
- Test: `app/lib/invite.test.ts`

**Interfaces:**
- Consumes: the engine fields from Task 1 (structurally).
- Produces (exported from `@/app/lib/invite`):
  - `type InviteLauncher = { joinUrl: string; downloadUrl: string }`
  - `InviteView.launcher: InviteLauncher | null`
  - `InvitePack.commit: string` (`""` when the engine sends none)
  - `toInviteLauncher(dto: LauncherLike | null | undefined): InviteLauncher | null`
  - `InviteLike.launcher?: LauncherLike | null`, where `type LauncherLike = { joinUrl?: string; downloadUrl?: string }` (exported)

- [ ] **Step 1: Write the failing tests**

In `app/lib/invite.test.ts`, add `toInviteLauncher` to the import list from `@/app/lib/invite`.

In `describe("toInviteView")`, replace the first test's expected object so it carries `launcher: null`:

```ts
  it("reads an invite that was never saved", () => {
    expect(toInviteView({ enabled: false })).toEqual({
      enabled: false,
      token: null,
      url: null,
      fields: EMPTY,
      language: "en",
      preview: null,
      launcher: null,
    });
  });
```

In the test "reads the preview", change the preview's `pack` to carry a commit:

```ts
        pack: { name: "Asalith", version: "1.4.0", minecraft: "1.20.1", loader: "forge", loaderVersion: "47.2.0", modCount: 212, state: "pending", commit: "0123456789abcdef0123456789abcdef01234567" },
```

and its `toMatchObject` pack line to:

```ts
      pack: { name: "Asalith", modCount: 212, state: "pending", commit: "0123456789abcdef0123456789abcdef01234567" },
```

Then add these tests at the end of `describe("toInviteView")`:

```ts
  it("reads the pack's commit, empty from an older engine", () => {
    const pack = { name: "Asalith", version: "1.4.0", modCount: 3, state: "ready" };
    expect(toInviteView({ preview: { kind: "stack", name: "asalith", pack } }).preview?.pack?.commit).toBe("");
    expect(toInviteView({ preview: { kind: "stack", name: "asalith", pack: { ...pack, commit: "  abc123  " } } }).preview?.pack?.commit).toBe("abc123");
  });
```

Add a new `describe` block after `describe("toInviteView")`:

```ts
describe("toInviteLauncher", () => {
  const TOKEN = "A".repeat(43);
  const url = `https://play.example/${TOKEN}`;
  const joinUrl = `perland://join?invite=${encodeURIComponent(url)}`;
  const downloadUrl = "https://perland.example/launcher";

  it("reads the engine's launcher block on the invite", () => {
    expect(toInviteView({ enabled: true, token: TOKEN, url, launcher: { joinUrl, downloadUrl } }).launcher).toEqual({ joinUrl, downloadUrl });
  });

  it("trims what the engine sends", () => {
    expect(toInviteLauncher({ joinUrl: ` ${joinUrl} `, downloadUrl: ` ${downloadUrl}\n` })).toEqual({ joinUrl, downloadUrl });
  });

  it("has none without the engine's block: LAUNCHER_DOWNLOAD_URL unset, a network, or no play site address", () => {
    expect(toInviteView({ enabled: true, token: TOKEN, url }).launcher).toBeNull();
    expect(toInviteView({ launcher: null }).launcher).toBeNull();
    expect(toInviteLauncher({ downloadUrl })).toBeNull();
    expect(toInviteLauncher({ joinUrl })).toBeNull();
    expect(toInviteLauncher({})).toBeNull();
  });

  it("drops a block it would not render as a link", () => {
    expect(toInviteLauncher({ joinUrl: "javascript:alert(1)", downloadUrl })).toBeNull();
    expect(toInviteLauncher({ joinUrl: "https://play.example/x", downloadUrl })).toBeNull();
    expect(toInviteLauncher({ joinUrl: "perland://pack?url=https%3A%2F%2Fx.example%2Fpack.toml", downloadUrl })).toBeNull();
    expect(toInviteLauncher({ joinUrl: "perland://join?invite=", downloadUrl })).toBeNull();
    expect(toInviteLauncher({ joinUrl: "perland://join?invite=a b", downloadUrl })).toBeNull();
    expect(toInviteLauncher({ joinUrl, downloadUrl: "http://perland.example/launcher" })).toBeNull();
    expect(toInviteLauncher({ joinUrl, downloadUrl: "javascript:alert(1)" })).toBeNull();
    expect(toInviteLauncher({ joinUrl, downloadUrl: "https://a:b@perland.example/" })).toBeNull();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node node_modules/vitest/vitest.mjs run app/lib/invite.test.ts`
Expected: FAIL. The never-saved invite lacks `launcher`, the pack lacks `commit`, and `toInviteLauncher` is not exported (`toInviteLauncher is not a function`).

- [ ] **Step 3: Implement**

In `app/lib/invite.ts`:

Add `commit` at the end of `InvitePack`:

```ts
export type InvitePack = {
  name: string;
  version: string;
  minecraft: string;
  loader: string;
  loaderVersion: string;
  modCount: number;
  state: PackState;
  /** The pinned modpack commit the pack is built from; `""` from an engine before join by link. */
  commit: string;
};
```

After `InvitePreview`, add:

```ts
/**
 * Perland Launcher on a stack's invite (join by link): the engine sends it
 * only with LAUNCHER_DOWNLOAD_URL set, and `joinUrl` only with a play site
 * address. The engine owns the `perland://` format; the panel never builds it.
 */
export type InviteLauncher = {
  /** `perland://join?invite=<the invite link, url-encoded>`: opens the launcher on this computer. */
  joinUrl: string;
  /** Where players get Perland Launcher. */
  downloadUrl: string;
};
```

Add `launcher` at the end of `InviteView`:

```ts
  preview: InvitePreview | null;
  /** Open in launcher and its download line; null without the engine's block. */
  launcher: InviteLauncher | null;
};
```

In the structural DTO types, add `commit` to `PackLike`, add `LauncherLike`, and add `launcher` to `InviteLike`:

```ts
type PackLike = { name?: string; version?: string; minecraft?: string; loader?: string; loaderVersion?: string; modCount?: number; state?: string; commit?: string };
export type LauncherLike = { joinUrl?: string; downloadUrl?: string };
```

```ts
export type InviteLike = {
  enabled?: boolean;
  token?: string;
  url?: string;
  note?: string;
  linkLabel?: string;
  linkUrl?: string;
  language?: string;
  preview?: InvitePreviewLike;
  launcher?: LauncherLike | null;
};
```

In `toInvitePreview`, add the commit as the last field of the pack object:

```ts
          state: oneOf(PACK_STATES, pack.state, "unavailable"),
          commit: text(pack.commit),
```

Before `toInviteView`, add:

```ts
/** The engine's join link: its own scheme and route, and an invite value with no space. */
const JOIN_URL = /^perland:\/\/join\?invite=\S+$/;

/**
 * The engine's `launcher` block, or null. Both values become links in the
 * sheet, so anything but the engine's own shapes (a `perland://join` link,
 * an https download page) is dropped, never shown.
 */
export function toInviteLauncher(dto: LauncherLike | null | undefined): InviteLauncher | null {
  const joinUrl = text(dto?.joinUrl);
  const downloadUrl = text(dto?.downloadUrl);
  if (!JOIN_URL.test(joinUrl) || !isHttpsUrl(downloadUrl)) return null;
  return { joinUrl, downloadUrl };
}
```

(`isHttpsUrl` is a function declaration further down the same file, so it is hoisted.)

In `toInviteView`, add the last field:

```ts
    preview: toInvitePreview(dto.preview),
    launcher: toInviteLauncher(dto.launcher),
  };
```

- [ ] **Step 4: Run the tests and the type check**

Run: `node node_modules/vitest/vitest.mjs run app/lib/invite.test.ts` → PASS.
Run: `node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json` → no errors. `app/lib/actions/invites.ts` passes the SDK's `InvitedtosInviteDTO` to `toInviteView`, so a clean check proves the generated `launcher` type fits `LauncherLike`.

- [ ] **Step 5: Commit**

```bash
git add app/lib/invite.ts app/lib/invite.test.ts
git commit -m "feat(invite): read the engine's launcher block and pack commit

toInviteView maps the owner invite's launcher (joinUrl, downloadUrl) and
drops any block that is not a perland://join link with an https download
page. The preview's pack carries the pinned commit.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Mock engine: `pack.commit` and the launcher blocks

**Files:**
- Modify: `scripts/mock-engine.mjs` (the header usage list near line 67; the `invites` section, around `INVITE_BASE_URL`, `inviteView` and `inviteBody`)

**Interfaces:**
- Consumes: `ENGINE_PR` (Task 1) for the comments; `playerPackOf(x).commit.sha` (existing).
- Produces:
  - `MOCK_LAUNCHER_DOWNLOAD_URL` (unset by default). When set: every stack's `inviteView` carries `launcher: { downloadUrl }` (the public view and the owner preview), and a stack's owner `inviteBody` with a `url` carries `launcher: { joinUrl, downloadUrl }`.
  - `pack.commit` in every stack view that has a pack.

The mock has no unit tests; it is checked with `curl` and lint.

- [ ] **Step 1: Check the gap first**

Run the mock on a side port with the variable set, and read asalith's public view. Its token is the mock's own fake seed, `pQ7rT2vX9yB4nM6kL1sD8fG3hJ5wE0aZcV2bN4mK7qR`.

```bash
MOCK_PORT=18090 MOCK_LAUNCHER_DOWNLOAD_URL=https://perland.test/launcher node scripts/mock-engine.mjs
```

Run it in the background. Then:

```bash
curl -s http://localhost:18090/api/v1/invites/pQ7rT2vX9yB4nM6kL1sD8fG3hJ5wE0aZcV2bN4mK7qR | grep -o '"commit":"[^"]*"\|"launcher":{[^}]*}'
```

Expected: no output, so neither field exists yet. Stop the mock: `Get-NetTCPConnection -LocalPort 18090 | ForEach-Object { Stop-Process -Id $_.OwningProcess }` (PowerShell; `pkill` does not stop it on Windows).

- [ ] **Step 2: Add the setting**

Directly after `const INVITE_PACK = process.env.MOCK_INVITE_PACK ?? "";`, add (replace `<ENGINE_PR>` with the number from Task 1):

```js
// The engine's LAUNCHER_DOWNLOAD_URL (join by link, PerlandEngine#<ENGINE_PR>): unset, invites carry
// no launcher block. Set, a stack's invite view (public and the owner's preview) carries
// launcher.downloadUrl, and its owner invite launcher.joinUrl and downloadUrl while it has a url.
// An invalid value stops the mock, as it stops the engine's boot.
const LAUNCHER_DOWNLOAD_URL = (process.env.MOCK_LAUNCHER_DOWNLOAD_URL ?? "").trim();
if (LAUNCHER_DOWNLOAD_URL && !/^https:\/\/[^/?#\s]+/i.test(LAUNCHER_DOWNLOAD_URL)) {
  throw new Error(`MOCK_LAUNCHER_DOWNLOAD_URL must be an absolute https URL, got "${LAUNCHER_DOWNLOAD_URL}"`);
}
```

- [ ] **Step 3: Carry `pack.commit` and the public `launcher` in `inviteView`**

In `inviteView`, in the stack branch, add `commit` as the last field of `out.pack`. Then, after the `if (built) { … }` block and still inside the stack branch, add the launcher:

```js
        state: INVITE_PACK || (built.reason ? "unavailable" : "ready"),
        commit: built.commit.sha,
      };
    }
    if (LAUNCHER_DOWNLOAD_URL) out.launcher = { downloadUrl: LAUNCHER_DOWNLOAD_URL };
  } else {
```

(The network branch stays as it is: the engine sends no launcher block for a network.)

- [ ] **Step 4: Carry the owner `launcher` in `inviteBody`**

In `inviteBody`, after `if (inv && INVITE_BASE_URL) out.url = …;`, add:

```js
  // The engine builds the join link; the panel only renders it.
  if (kind === "stack" && out.url && LAUNCHER_DOWNLOAD_URL) {
    out.launcher = { joinUrl: `perland://join?invite=${encodeURIComponent(out.url)}`, downloadUrl: LAUNCHER_DOWNLOAD_URL };
  }
```

- [ ] **Step 5: Document it in the header and the section comment**

In the header usage list, directly after the `MOCK_INVITE_PACK=pending …` line, add:

```js
 *   MOCK_LAUNCHER_DOWNLOAD_URL=https://perland.test/launcher node scripts/mock-engine.mjs  # invites: the engine's LAUNCHER_DOWNLOAD_URL (default unset: no launcher block); set, a stack's invite offers Open in launcher
```

In the comment that opens the `invites` section, change its last sentence from `(PerlandEngine#330); every other server has none yet.` to:

```js
// (PerlandEngine#330); every other server has none yet. A stack's pack carries
// its pinned commit, and MOCK_LAUNCHER_DOWNLOAD_URL adds the launcher blocks.
```

- [ ] **Step 6: Verify with curl, then lint**

Start the mock as in Step 1, in the background. Then:

```bash
curl -s http://localhost:18090/api/v1/invites/pQ7rT2vX9yB4nM6kL1sD8fG3hJ5wE0aZcV2bN4mK7qR | grep -o '"commit":"[^"]*"\|"launcher":{[^}]*}'
```

Expected: one `"commit":"<sha>"` and `"launcher":{"downloadUrl":"https://perland.test/launcher"}`.

Also try the owner route: `curl -s http://localhost:18090/api/v1/stacks/asalith/invite | grep -o '"launcher":{[^}]*}'`.
- If the mock answers without a session, expect two blocks. The owner block is `{"joinUrl":"perland://join?invite=https%3A%2F%2Fplay.perland.test%2FpQ7r…","downloadUrl":"https://perland.test/launcher"}`; the other is the preview's `{"downloadUrl":…}`.
- If it answers 401 or 404, Task 4's browser check covers the owner block instead.

Stop the mock. Then check that an invalid value stops the mock:

```bash
MOCK_PORT=18091 MOCK_LAUNCHER_DOWNLOAD_URL=http://perland.test/launcher node scripts/mock-engine.mjs; echo "exit $?"
```

Expected: it throws "MOCK_LAUNCHER_DOWNLOAD_URL must be an absolute https URL" and exits non-zero. Without the variable, the curl from Step 1 prints `"commit":…` only, with no launcher.

Run: `node node_modules/eslint/bin/eslint.js scripts/mock-engine.mjs` → no errors.

- [ ] **Step 7: Commit**

```bash
git add scripts/mock-engine.mjs
git commit -m "feat(mock): pack commit and launcher blocks on invites

Every stack's invite view carries its pack's pinned commit, and
MOCK_LAUNCHER_DOWNLOAD_URL adds the engine's launcher blocks: downloadUrl
on the view and joinUrl with downloadUrl on the owner invite.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Invite sheet: Open in launcher and the download line

**Files:**
- Modify: `app/components/invite/invite-dialog.tsx` (`LinkSlot` around line 649, `ViewerLink` around line 749)
- Modify: `app/i18n/dictionaries/en/invite.ts`, `app/i18n/dictionaries/es/invite.ts`, `app/i18n/dictionaries/ca/invite.ts`
- Create: `app/components/invite/invite-dialog.test.tsx`

**Interfaces:**
- Consumes: `InviteView.launcher: InviteLauncher | null` and `toInviteView` / `InviteLike` (Task 2). The mock's `MOCK_LAUNCHER_DOWNLOAD_URL` (Task 3) for the visual check.
- Produces:
  - `export function LinkSlot({ invite }: { invite: InviteView })` and `export function ViewerLink({ invite }: { invite: InviteView })` (exported for tests only)
  - dictionary keys `invite.link.launcher.{open, openLabel, need, download, downloadLabel}`

- [ ] **Step 1: Write the failing tests**

Create `app/components/invite/invite-dialog.test.tsx`:

```tsx
import type { ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import type { Locale } from "@/app/i18n/config";
import { I18nProvider } from "@/app/i18n/client";
import { toInviteView, type InviteLike } from "@/app/lib/invite";

import { LinkSlot, ViewerLink } from "./invite-dialog";

vi.mock("next/navigation", () => ({ useRouter: () => ({ refresh: () => {} }) }));
vi.mock("@/app/lib/actions/invites", () => ({ readInviteAction: vi.fn(), saveInviteAction: vi.fn(), regenerateInviteAction: vi.fn() }));

const TOKEN = "A".repeat(43);
const LINK = `https://play.example/${TOKEN}`;
const JOIN = `perland://join?invite=${encodeURIComponent(LINK)}`;
const DOWNLOAD = "https://perland.example/launcher";

const view = (o: InviteLike = {}) => toInviteView({ enabled: true, token: TOKEN, url: LINK, launcher: { joinUrl: JOIN, downloadUrl: DOWNLOAD }, ...o });
const html = (node: ReactNode, locale: Locale = "en") => renderToStaticMarkup(<I18nProvider locale={locale}>{node}</I18nProvider>);
const anchor = (out: string, href: string) => out.match(new RegExp(`<a [^>]*href="${href.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"[^>]*>`))?.[0];

describe("LinkSlot with Perland Launcher", () => {
  it("adds Open in launcher beside Copy and Open, and the download line under them", () => {
    const out = html(<LinkSlot invite={view()} />);
    expect(out).toContain(`href="${JOIN}"`);
    expect(out).toContain(">Open in launcher<");
    expect(out).toContain(`href="${LINK}"`);
    expect(out).toContain("Players need Perland Launcher");
    expect(out).toContain(`href="${DOWNLOAD}"`);
    expect(out).toContain(">Download<");
  });

  it("opens the launcher in the same tab and the download page in a new one", () => {
    const out = html(<LinkSlot invite={view()} />);
    expect(anchor(out, JOIN)).toBeDefined();
    expect(anchor(out, JOIN)).not.toContain("target=");
    expect(anchor(out, DOWNLOAD)).toContain('target="_blank"');
    expect(anchor(out, DOWNLOAD)).toContain('rel="noreferrer"');
  });

  it("keeps Copy the only button in the link row, so Replace link still focuses it", () => {
    const out = html(<LinkSlot invite={view()} />);
    expect(out.match(/<button/g)?.length).toBe(1);
    expect(out).toContain("Copy the invite link");
  });

  it("hides both while the invite is off", () => {
    const out = html(<LinkSlot invite={view({ enabled: false })} />);
    expect(out).not.toContain("perland://");
    expect(out).not.toContain("Perland Launcher");
  });

  it("hides both without the engine's launcher block", () => {
    const out = html(<LinkSlot invite={view({ launcher: undefined })} />);
    expect(out).toContain(`href="${LINK}"`);
    expect(out).not.toContain("perland://");
    expect(out).not.toContain("Perland Launcher");
  });

  it("hides both without a play site address, where the sheet shows the token", () => {
    const out = html(<LinkSlot invite={view({ url: undefined })} />);
    expect(out).toContain("INVITE_BASE_URL");
    expect(out).not.toContain("perland://");
    expect(out).not.toContain("Perland Launcher");
  });

  it("speaks Spanish and Catalan, and keeps the name", () => {
    expect(html(<LinkSlot invite={view()} />, "es")).toContain(">Abrir en el launcher<");
    expect(html(<LinkSlot invite={view()} />, "es")).toContain("Los jugadores necesitan Perland Launcher");
    expect(html(<LinkSlot invite={view()} />, "ca")).toContain(">Obre al launcher<");
    expect(html(<LinkSlot invite={view()} />, "ca")).toContain("Els jugadors necessiten Perland Launcher");
  });
});

describe("ViewerLink with Perland Launcher", () => {
  it("gives viewers Open in launcher while the invite is on", () => {
    const out = html(<ViewerLink invite={view()} />);
    expect(out).toContain(`href="${JOIN}"`);
    expect(out).toContain("Players need Perland Launcher");
  });

  it("hides it while off", () => {
    const out = html(<ViewerLink invite={view({ enabled: false })} />);
    expect(out).not.toContain("perland://");
    expect(out).not.toContain("Perland Launcher");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node node_modules/vitest/vitest.mjs run app/components/invite/invite-dialog.test.tsx`
Expected: FAIL. `LinkSlot` and `ViewerLink` are not exported (`… is not a function` / undefined element).

If the run fails earlier, on an import that needs a request or a browser API, add a `vi.mock` for that module next to the two above (the backups render test does the same), then re-run until the failure is the missing export.

- [ ] **Step 3: Add the strings in all three languages**

`app/i18n/dictionaries/en/invite.ts`: in `link`, directly after the line `    viewerNote: "Owners and members turn the invite on and off, edit it and replace the link.",`, add:

```ts
    /** Join by link: only on a stack whose engine has LAUNCHER_DOWNLOAD_URL. "Perland Launcher" is a name, never translated. */
    launcher: {
      open: "Open in launcher",
      openLabel: "Open this invite in Perland Launcher on this computer",
      need: "Players need Perland Launcher",
      download: "Download",
      downloadLabel: "Download Perland Launcher (opens in a new tab)",
    },
```

`app/i18n/dictionaries/es/invite.ts`: directly after the line `    viewerNote: "Los propietarios y miembros activan y desactivan la invitación, la editan y reemplazan el enlace.",`, add:

```ts
    launcher: {
      open: "Abrir en el launcher",
      openLabel: "Abrir esta invitación en Perland Launcher en esta computadora",
      need: "Los jugadores necesitan Perland Launcher",
      download: "Descargar",
      downloadLabel: "Descargar Perland Launcher (se abre en una pestaña nueva)",
    },
```

`app/i18n/dictionaries/ca/invite.ts`: directly after the line `    viewerNote: "Els propietaris i membres activen i desactiven la invitació, l'editen i en substitueixen l'enllaç.",`, add:

```ts
    launcher: {
      open: "Obre al launcher",
      openLabel: "Obre aquesta invitació a Perland Launcher en aquest ordinador",
      need: "Els jugadors necessiten Perland Launcher",
      download: "Baixa",
      downloadLabel: "Baixa Perland Launcher (s'obre en una pestanya nova)",
    },
```

- [ ] **Step 4: Render the link and the line in `LinkSlot`, and export both components**

In `app/components/invite/invite-dialog.tsx`, change `function LinkSlot(` to `export function LinkSlot(` and `function ViewerLink(` to `export function ViewerLink(`. Add `(exported for its tests)` to the end of each one's doc comment.

Replace the `if (invite.url) { … }` branch of `LinkSlot` (the "on, with a url" branch after the `CopyTextAbsent` one) with:

```tsx
  if (invite.url) {
    const launcher = invite.launcher;
    return (
      <div className="space-y-1.5">
        {/* Open and Open in launcher drop under the address on a phone, so the address keeps the row. */}
        <div data-link-slot className="flex min-w-0 flex-wrap items-center justify-end gap-2">
          <CopyText value={invite.url} label={invite.url} name={w.copyLabel} block className="min-w-[min(100%,18rem)] flex-1" />
          <a href={invite.url} target="_blank" rel="noreferrer" aria-label={w.openLabel} className={buttonClass("secondary", "sm", "shrink-0")}>
            {w.open}
            <IconArrowUpRight size={12} className="-mr-0.5" />
          </a>
          {launcher ? (
            // The engine's perland:// link hands the invite to the launcher on this computer. Same tab: a new one would stay blank.
            <a href={launcher.joinUrl} aria-label={w.launcher.openLabel} className={buttonClass("secondary", "sm", "shrink-0")}>
              {w.launcher.open}
            </a>
          ) : null}
        </div>
        {launcher ? (
          <p className="text-[11px] leading-[1.6] text-muted">
            {w.launcher.need}
            <span aria-hidden> · </span>
            <a
              href={launcher.downloadUrl}
              target="_blank"
              rel="noreferrer"
              aria-label={w.launcher.downloadLabel}
              className="text-accent-text underline-offset-2 hover:underline"
            >
              {w.launcher.download}
            </a>
          </p>
        ) : null}
      </div>
    );
  }
```

The off branch (`invite.url && !invite.enabled`) and the token-only branch stay unchanged, so neither shows the launcher. `ViewerLink` renders `LinkSlot` only while `invite.enabled && invite.token`, so viewers get the same row with no change to `ViewerLink`'s body. `data-link-slot` stays on the row, so the regenerate refocus `[data-link-slot] button` still lands on Copy.

- [ ] **Step 5: Run the tests and the type check**

Run: `node node_modules/vitest/vitest.mjs run app/components/invite/invite-dialog.test.tsx app/lib/invite.test.ts` → PASS.
Run: `node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json` → no errors. A missing key in `es` or `ca` fails here.
Run: `node node_modules/eslint/bin/eslint.js app/components/invite app/i18n/dictionaries` → no errors.

- [ ] **Step 6: Style pass through `/impeccable`**

Invoke the `/impeccable` skill on the Invite sheet's link row (`LinkSlot` in `app/components/invite/invite-dialog.tsx`). Give it as the brief:
- `DESIGN.md` "Invite sheet", "Invite link" bullet;
- the rule that the sheet's one primary is Save, so Open in launcher stays secondary;
- the row: Copy, Open and Open in launcher, plus an 11px muted line with an accent Download link.

Keep whatever it changes inside `LinkSlot`, and never add text outside the dictionaries. Its likely questions:
- whether Open in launcher sits before or after Open;
- whether it takes a glyph;
- how the row wraps at phone width.

If it changes markup, re-run Step 5. Each test pins behaviour (hrefs, targets, one button, visibility), not layout, so a reorder keeps them green.

- [ ] **Step 7: See it in the browser against the mock**

Start the mock: `MOCK_PORT=18090 MOCK_LAUNCHER_DOWNLOAD_URL=https://perland.test/launcher node scripts/mock-engine.mjs` (background). Create `.env.local` with `PERLAND_ENGINE_URL=http://localhost:18090`.

Add a temporary `.claude/launch.json` entry `{"name": "panel-launcher", "runtimeExecutable": "npx", "runtimeArgs": ["next", "dev", "-p", "3210"], "port": 3210}` and start it with the preview tool. Sign in as a mock user who owns asalith, open asalith's Overview, and click Invite. Check:
- Open in launcher sits in the link row, with "Players need Perland Launcher · Download" under it. The Download link points at `https://perland.test/launcher`.
- Turning the switch Off removes both. Turning it On brings them back.
- Replace link leaves focus on Copy, and Open in launcher's `href` carries the new token (the owner block from Task 3).
- At 390px width the row wraps under the address with nothing cut off, in both the light and dark themes.
- With `MOCK_LAUNCHER_DOWNLOAD_URL` unset (restart the mock), the sheet looks exactly as on `main`.
- A network's Invite sheet (survival-net) never shows the launcher.

Then stop the mock and the dev server, revert `.claude/launch.json`, and delete `.env.local`.

- [ ] **Step 8: Commit**

```bash
git add app/components/invite/invite-dialog.tsx app/components/invite/invite-dialog.test.tsx app/i18n/dictionaries/en/invite.ts app/i18n/dictionaries/es/invite.ts app/i18n/dictionaries/ca/invite.ts
git commit -m "feat(invite): Open in Perland Launcher on the invite sheet

While the invite is on and the engine sends its launcher block, the link
row gets a secondary Open in launcher (the engine's perland://join link,
same tab) and the line Players need Perland Launcher · Download. Viewers
get it too. English, Spanish and Catalan.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: DESIGN.md and CLAUDE.md

**Files:**
- Modify: `DESIGN.md` ("Invite sheet" section, the "Invite link" and "Viewer" bullets)
- Modify: `CLAUDE.md` ("Invites" bullet under "Code patterns for this panel")

**Interfaces:**
- Consumes: `ENGINE_PR` (Task 1); the final layout from Task 4 Step 6. If `/impeccable` reordered the row or added a glyph, write what shipped, not the text below.

- [ ] **Step 1: DESIGN.md, "Invite link" bullet**

In `DESIGN.md`, find this text in the "Invite link" bullet of "Invite sheet":

```
with a small secondary "Open" (arrow-out, new tab) beside it, dropping under it on a phone;
```

Replace it with (use the real `ENGINE_PR`):

```
with a small secondary "Open" (arrow-out, new tab) beside it, dropping under it on a phone; with the engine's `launcher` block (a stack whose engine sets LAUNCHER_DOWNLOAD_URL, PerlandEngine#<ENGINE_PR>) a third small secondary "Open in launcher" after Open, the engine's `perland://join` link opened in the same tab (a new tab would stay blank), and under the row an 11px muted line "Players need Perland Launcher · Download", Download an accent link to the launcher's download page in a new tab; neither shows while the invite is off or without a play site address, and Copy stays the row's only button, so a replaced link still focuses it;
```

- [ ] **Step 2: DESIGN.md, "Viewer" bullet**

Find:

```
while on, the link with Copy and Open;
```

Replace with:

```
while on, the link with Copy and Open, and Open in launcher with its line when the engine sends the launcher block;
```

- [ ] **Step 3: CLAUDE.md, "Invites" bullet**

In `CLAUDE.md`, find:

```
On a stack whose `preview.pack` is set it says that the invite hands out the modpack, configs included.
```

Replace with:

```
On a stack whose `preview.pack` is set it says that the invite hands out the modpack, configs included. With the engine's `LAUNCHER_DOWNLOAD_URL` set (join by link, PerlandEngine#<ENGINE_PR>), a stack's owner invite carries `launcher` (`joinUrl`, the engine's `perland://join?invite=…`, which the panel never builds, and `downloadUrl`): while the invite is on, the link row gets "Open in launcher" (same tab) and the line "Players need Perland Launcher · Download", viewers included. `toInviteLauncher` drops a block whose `joinUrl` is not `perland://join?invite=…` or whose `downloadUrl` is not https, so nothing else becomes a link; the preview's `pack` carries the pinned `commit`.
```

Then find:

```
(`MOCK_INVITE_BASE_URL=` for an engine without a play site, `MOCK_INVITE_PACK=pending|unavailable|ready`).
```

Replace with:

```
(`MOCK_INVITE_BASE_URL=` for an engine without a play site, `MOCK_INVITE_PACK=pending|unavailable|ready`, `MOCK_LAUNCHER_DOWNLOAD_URL=https://…` for the launcher blocks; a stack's pack carries its pinned `commit`).
```

- [ ] **Step 4: Commit**

```bash
git add DESIGN.md CLAUDE.md
git commit -m "docs(invite): Open in launcher on the invite sheet

DESIGN.md's Invite sheet and CLAUDE.md's Invites note describe the
launcher link, its download line, when they show, and the mock's
MOCK_LAUNCHER_DOWNLOAD_URL.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Full checks, PR and the roadmap record

**Files:**
- No panel file changes, except fixes the checks ask for.
- Engine repository: `site/src/data/roadmap.yaml` (a separate small PR).

**Interfaces:**
- Consumes: `ENGINE_PR` and the generated names (Task 1).
- Produces: the panel PR, and an engine records PR whose number goes on the panel PR's `Roadmap:` line.

- [ ] **Step 1: Run every CI check locally**

In the panel worktree, with the swapped SDK from Task 1 Step 5 (redo it if `pnpm install` ran since):

```bash
node node_modules/typescript/bin/tsc --noEmit -p tsconfig.json
node node_modules/eslint/bin/eslint.js app scripts middleware.ts
node node_modules/vitest/vitest.mjs run
node node_modules/next/dist/bin/next build
git status --short
```

Expected: all four pass, and `git status` is clean. Revert `pnpm-workspace.yaml`, `.claude/launch.json` and `next-env.d.ts` if a tool touched them, and make sure no `.env.local` is left. Do not run `next build` while a `next dev` from this worktree is still serving.

- [ ] **Step 2: Scan for a real token**

```bash
git diff origin/main --stat
git diff origin/main | grep -nE "[A-Za-z0-9_-]{43}" | grep -v "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\|pQ7rT2vX9yB4nM6kL1sD8fG3hJ5wE0aZcV2bN4mK7qR"
```

Expected: no output. Only the `"A".repeat(43)` fake and the mock's existing fake seed appear. Any other 43-character run gets checked and removed if it is a real token.

- [ ] **Step 3: Push and open the panel PR**

```bash
git push -u origin claude/join-by-link-panel
```

Write the body to a scratchpad file `pr-body.md`:

```markdown
## What

On a server's Invite sheet, while the invite is on and the engine sends its `launcher` block, the link row gets a secondary **Open in launcher** (the engine's `perland://join?invite=…` link, same tab) and the line "Players need Perland Launcher · Download". Viewers get it too.

- `toInviteView` maps `launcher` and drops anything that is not a `perland://join` link with an https download page; the preview's pack carries the pinned `commit`.
- Mock engine: `pack.commit` on every stack view, and the launcher blocks behind `MOCK_LAUNCHER_DOWNLOAD_URL`.
- Strings in English, Spanish and Catalan; DESIGN.md and CLAUDE.md updated.

Pairs with PerlandEngine#<ENGINE_PR> (`PackDTO.commit`, the `launcher` blocks, `LAUNCHER_DOWNLOAD_URL`). Spec: join by link, §3.2 (perland-launcher `docs/superpowers/specs/2026-10-11-join-by-link-design.md`).

## How to check

`MOCK_LAUNCHER_DOWNLOAD_URL=https://perland.test/launcher node scripts/mock-engine.mjs`, open asalith's Invite sheet. Turn the invite off: the launcher row goes. Without the variable the sheet is unchanged.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

```bash
gh pr create -R PerlandDev/PerlandPanel --base main --head claude/join-by-link-panel --title "feat(invite): Open in Perland Launcher on the invite sheet" --body-file <scratchpad>/pr-body.md
```

Note the panel PR number as `PANEL_PR`.

- [ ] **Step 4: Link the panel PR on the engine roadmap (a records PR)**

Panel CLAUDE.md "Finishing a change": the roadmap lives in the engine repository, and a panel PR is paired with a small engine PR that links it.

In a fresh engine worktree from `origin/main`, on a branch such as `claude/join-by-link-panel-roadmap`, open `site/src/data/roadmap.yaml`:
- If the Engine PR (`ENGINE_PR`) already added the `repo: panel` item for this (search `launcher`), set `status: in-progress` and add the link.
- If no such item exists, add this one under the other `repo: panel` items:

```yaml
  - title: Open in Perland Launcher from the invite sheet
    status: in-progress
    repo: panel
    phase: panel
    summary: An Open in launcher button and a download line on a server's invite sheet, shown when the engine offers Perland Launcher. Pairs with the engine's launcher links on invites.
    links:
      - label: "#<PANEL_PR>"
        href: https://github.com/PerlandDev/PerlandPanel/pull/<PANEL_PR>
```

Commit with a message ending in the `Co-Authored-By` line, push, and open it:

```bash
gh pr create -R PerlandDev/PerlandEngine --base main --title "docs(roadmap): link the panel's Open in launcher PR" --body "Links PerlandDev/PerlandPanel#<PANEL_PR> on the roadmap item and sets it in progress.

Changelog: none (roadmap record for a panel PR)

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

The engine's `gh pr create` hook checks the records. A roadmap-only change touches no engine file, so it passes. Note the PR number as `RECORDS_PR`.

- [ ] **Step 5: Add the Roadmap line to the panel PR**

Append a line of its own to `pr-body.md`, above the "Generated with" line:

```
Roadmap: PerlandDev/PerlandEngine#<RECORDS_PR>
```

```bash
gh pr edit <PANEL_PR> -R PerlandDev/PerlandPanel --body-file <scratchpad>/pr-body.md
```

- [ ] **Step 6: Watch CI**

`gh pr checks <PANEL_PR> -R PerlandDev/PerlandPanel --watch`. Expected: "Typecheck, lint, build" passes against engine `main`. A type failure on `launcher` or `commit` means the CI engine ref lacks the Engine PR: check `ENGINE_REF` (Task 1 Step 1) and tell the owner.

Once the panel PR merges, the item needs `status: shipped` and `shipped: <merge day>`. That is the engine's catch-up (`node site/scripts/check-records.mjs audit`), not this plan. Tell the owner.

Clean up: `git -C E:\PerlandEngine worktree remove E:\PerlandEngine\.claude\worktrees\join-by-link-sdk`.

---

## Self-Review

- **Spec coverage (§3.2):**
  - Invite sheet `LinkSlot` / `ViewerLink` with Open in launcher and the hint, shown only with `invite.launcher` and the invite on: Task 4.
  - `toInviteView` and its tests: Task 2.
  - Strings in en, es and ca: Task 4 Step 3.
  - DESIGN.md "Invite sheet": Task 5.
  - Mock `pack.commit` and the launcher blocks behind `MOCK_LAUNCHER_DOWNLOAD_URL`: Task 3.
  - PR body `Roadmap:` line: Task 6.
  - SDK first, with a stop-and-ask: Task 1.
- **Placeholders:** `<ENGINE_PR>`, `<PANEL_PR>` and `<RECORDS_PR>` are PR numbers that exist only at run time; each is produced by a named step (Task 1 Step 1, Task 6 Steps 3 and 4).
- **Type consistency:**
  - `InviteLauncher { joinUrl, downloadUrl }`, `InviteView.launcher`, `InvitePack.commit`, `LauncherLike`, `toInviteLauncher` (Task 2) are used under the same names in Tasks 4 and 5.
  - Dictionary keys are `invite.link.launcher.{open, openLabel, need, download, downloadLabel}` in Task 4's strings, component and tests.
- **Review Focus:** each of the five lines has a named test in Task 2 or Task 4.
