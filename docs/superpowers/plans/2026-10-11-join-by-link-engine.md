# Join by Link (Engine) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Where to execute:** in a fresh worktree of `E:\PerlandEngine` based on `origin/main` (branch suggestion: `claude/join-by-link-engine`). The local `E:\PerlandEngine` checkout is stale and on another branch: never work in it. Run `git fetch origin` first, then create the worktree from `origin/main`. Every path below is relative to that worktree's root. This plan file lives in the launcher repo next to its spec. Read the Engine's `CLAUDE.md` ("Finishing a change", "Docs site") and `.claude/skills/testing/SKILL.md` before Task 1.

**Goal:** Invites tell Perland Launcher which modpack commit a server runs (`pack.commit`). Once the operator sets `LAUNCHER_DOWNLOAD_URL`, invites also offer the launcher: a `launcher` block on the public view and on the owner's invite, and a first "Perland Launcher" tab on the invite page.

**Architecture:**
- `playerpack.Service.Summary` already resolves the pinned commit, so it copies it into the new `PackDTO.Commit`.
- A new `config.LauncherDownloadURL()` validates the env var at boot. `main` passes it to `invites.NewService` through a new `invites.WithLauncherDownloadURL` option.
- The invites service adds the `launcher` blocks for stacks. The public block is set in `view()`, so it is cached with the view; the owner block is set in `dto()`. `invites.JoinURL` owns the `perland://join?invite=` format.
- The invite site:
  - mirrors the fields in `api.ts`;
  - gets the tab's data, the join link and the desktop-only default (`prefersLauncherTab`, `defaultLauncher`) from pure, tested functions in `launchers.ts`;
  - renders the tab in `render.ts`, with styling decided through `/impeccable`.

**Tech Stack:**
- Engine: Go 1.25+, Gin, swag v1.16.6, openapi-generator 7.21.0 (via `make sdk`), stdlib `testing` + `net/http/httptest`.
- Invite site: Astro 7, TypeScript, `node:test`.
- Docs site: Astro, `site/spec/overlay.yaml`.

**Spec:** `E:\perland-launcher\.claude\worktrees\ecstatic-pascal-b946ca\docs\superpowers\specs\2026-10-11-join-by-link-design.md` (launcher repo, branch `claude/ecstatic-pascal-b946ca`, commit `7579b11`). This plan covers only the Engine parts:
- §3.1 "Engine";
- the Engine facts in §1.2;
- the "Not changing" list in §3.3;
- the Engine tests in §3.1 item 4 and §4;
- the Engine row of §5.

The Panel plan and the launcher plan are separate.

**Engine facts verified at `origin/main` f9ae870c (2026-10-11):**
- The invites service lives in `internal/minecraft/invites/`, not `internal/minecraft/services/invites/`.
- The public view is built in `internal/minecraft/invites/view.go` (`(*Service).view`) and cached in `public.go`.
- The owner DTO is built in `service.go` (`(*Service).dto`). It embeds the same view as `preview`.
- `Summary` is in `internal/minecraft/playerpack/summary.go`. The `/pack` ETag is `"` + `playerpack.Pack.Commit` + `"` (`serveBuiltPack` in `internal/minecraft/controllers/player-pack-controller.go`).
- `config.InviteBaseURL()` is in `config/settings.go` (lines 276-291). It is called at boot in `cmd/server/main.go` (around lines 179-185) and passed via `invites.WithBaseURL` (around line 483).
- The invite site's unit tests are `node --test "src/lib/*.test.ts"`. `render.ts` and `main.ts` have no unit tests.
- `sdk/out/` is git-ignored, so `make sdk` only proves the generator still runs. Nothing generated is committed except `docs/docs.go`, `docs/swagger.json` and `docs/swagger.yaml`.

## Global Constraints

**`pack.commit`**
- `PackDTO.Commit string `json:"commit,omitempty"``.
- It is the pinned modpack commit, the same value as the `/pack` ETag without the quotes.
- It is read from the pinned commit that `Summary` already resolves.

**`LAUNCHER_DOWNLOAD_URL`**
- An absolute https URL, validated at boot like `INVITE_BASE_URL`. An invalid value stops boot.
- This plan's reading of "like `INVITE_BASE_URL`":
  - `http` is allowed only for `localhost` or `127.0.0.1`;
  - no user info and no whitespace;
  - at most 512 bytes;
  - a path, a query and a fragment are kept as written.

**`launcher` blocks**
- They are only for `kind: stack`, and only while `LAUNCHER_DOWNLOAD_URL` is set. Networks never carry one.
- The public `InviteViewDTO` gains `launcher: { downloadUrl }`.
- The owner `InviteDTO` gains `launcher: { joinUrl, downloadUrl }`.
  - `joinUrl` = `perland://join?invite=<url-encoded invite url>`.
  - `joinUrl` is present only when `INVITE_BASE_URL` is set and the invite has a token. Otherwise it is absent, never an empty or partial `perland://` link.
- The Engine owns the `perland://` format, so the Panel never builds it.
- On the invite site, the page builds the same link itself from `location.origin + "/" + token`. The Go and TypeScript outputs must match byte for byte.

**Invite site tab**
- **Perland Launcher** tab:
  - first in `invite/src/lib/launchers.ts`;
  - shown only when `view.launcher` is present (stacks only), on every device;
  - also shown for servers without a modpack.
- **Owner decision 2026-10-11: "Default on desktop only."**
  - On a desktop, the Perland Launcher tab is selected by default.
  - On a phone, the previous default stays: the first other tab (Modrinth App for a pack, Minecraft for a plain server).
  - Desktop detection is a pure function in `launchers.ts`, `prefersLauncherTab({ coarsePointer, mobileUA })`, with unit tests. It is a desktop only when the pointer is not coarse and the user agent is not mobile (`/Android|iPhone|iPad|iPod|Mobile/i`).
  - `render.ts` only gathers the env: `matchMedia('(pointer: coarse)')` and `navigator.userAgent`.
- **On a desktop**, the tab's primary button is **Open in Perland Launcher**. It opens `perland://join?invite=<encodeURIComponent(location.origin + "/" + token)>` in the same tab.
- **On a phone**, the tab has no `perland://` button. In its place is the line "Open this page on your computer to play with Perland Launcher." (es: "Abre esta página en tu computadora para jugar con Perland Launcher.").
- Below either: "Don't have it? Download Perland Launcher" (`downloadUrl`). It is always visible on every device, because there is no reliable way to detect whether the launcher is installed.
- The other tabs (Modrinth App, Prism, official launcher, CurseForge) stay.
- Copy is in en and es. The Spanish is neutral Latin American with informal "tú".
- Styling goes through `/impeccable`, against the invite surface brief.

**Not changing:**
- `/pack/zip` and the classic-launcher steps;
- the player-pack link `/api/v1/packs/{token}`;
- whitelist requests;
- networks.

**Error codes:** none are new, so `go run ./cmd/api-catalog` is not needed. Re-check this at the end; run it only if a code or scope changed.

**Token safety**
- Never put a real invite token in code, tests, docs, examples, PR text or screenshots.
- Tests use obviously fake 43-character tokens such as `AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA`, or the test files' existing fakes.
- The owner smoke test with the real Asalith invite is done by the owner, never recorded.

**Testing**
- Go uses stdlib `testing` + `httptest` only, with tests beside the code.
- The invite site uses `node:test` (`pnpm test`), plus `pnpm check` and `pnpm build`.
- **No fuzz, stress or load runs locally.** Keep inputs tiny.

**Windows dev machine**
- `$env:GOTMPDIR="$PWD\.gotmp"`, then `go test -p 2 ...`.
- `pnpm install` in `invite/` or `site/` may fail in esbuild's postinstall step. Retry with `pnpm install --ignore-scripts`.

**Swagger:** `make docs`, or, without `swag` installed, `go run github.com/swaggo/swag/cmd/swag@v1.16.6 init -g cmd/server/main.go -o docs`.

**Finishing a change** (Engine CLAUDE.md):
- changelog item(s) with `areas`;
- a roadmap item set `in-progress`, plus its panel pair;
- overlay examples for the changed public endpoints;
- CLAUDE.md notes;
- regenerated swagger;
- `pnpm test` and `pnpm build` in `site/`;
- PR screenshots of the invite page via the `pr-screenshots` skill (never committed);
- after the PR exists, one more commit setting `pr:` and the roadmap link.

**Every commit message ends with** `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

These are the most likely failure modes that the spec implies but does not test. Each one has a test in the owning task:

1. **The Go and TypeScript join links drift apart** (for example `url.PathEscape` vs `encodeURIComponent`, or a port in the origin).
   - Expected: both give byte-identical `perland://join?invite=https%3A%2F%2Fplay.example%2F<token>` for the same invite URL.
   - Pinned by the same literal strings in `TestJoinURL` (Task 3) and `perlandJoinUrl` (Task 6).
2. **A `downloadUrl` that is not https reaches an `href`** (a misconfigured engine, a `javascript:` value, plain `http://` on a real host).
   - Expected: the engine refuses it at boot (Task 2 table), and the page shows no tab for it (`launcherLinks` test, Task 6).
3. **The owner's invite has no token yet, or `INVITE_BASE_URL` is unset.**
   - Expected: `launcher.downloadUrl` is present and `joinUrl` is absent from the JSON, never `perland://join?invite=` with an empty value.
   - Pinned by `TestLauncher_NoJoinURLWithoutAURL` (Task 3).
4. **The owner regenerates a leaked link.**
   - Expected: the new `joinUrl` carries the new token and differs from the old one.
   - Pinned by `TestLauncher_RegenerateMovesJoinURL` (Task 3).
5. **`LAUNCHER_DOWNLOAD_URL` with surrounding spaces, user info, a space inside, a non-http scheme or over 512 bytes.**
   - Expected: spaces are trimmed; everything else stops boot with a message naming the variable.
   - Pinned by `TestLauncherDownloadURL` (Task 2).

The owner's "default on desktop only" decision brings its own cases, pinned in Task 6:
- an iPad that reports a desktop user agent but a coarse pointer counts as a phone;
- a phone gets no `perland://` link anywhere in its tab data;
- an invite with no `view.launcher` gets no tab on any device.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `internal/minecraft/model/DTOs/invites/invites.go` | Modify | `PackDTO.Commit`; new `LauncherViewDTO`, `InviteLauncherDTO`; `InviteViewDTO.Launcher`, `InviteDTO.Launcher` |
| `internal/minecraft/model/DTOs/invites/invites_test.go` | Create | JSON shape of the new fields (omitempty) |
| `internal/minecraft/playerpack/summary.go` | Modify | Copy the pinned commit into the summary |
| `internal/minecraft/playerpack/summary_test.go` | Modify | Summary commit equals the pinned commit and `Pack.Commit` (the ETag source) |
| `config/settings.go` | Modify | `LauncherDownloadURL()` |
| `config/settings_test.go` | Modify | `TestLauncherDownloadURL` |
| `internal/minecraft/invites/launcher.go` | Create | `JoinURL`, `WithLauncherDownloadURL` |
| `internal/minecraft/invites/service.go` | Modify | `launcherDownload` field; `dto()` sets the owner block |
| `internal/minecraft/invites/view.go` | Modify | `view()` sets the public block |
| `internal/minecraft/invites/service_test.go` | Modify | `newFixture` takes options |
| `internal/minecraft/invites/launcher_test.go` | Create | The launcher block rules |
| `cmd/server/main.go` | Modify | Read and validate the env var at boot; pass the option |
| `internal/minecraft/controllers/invite-controller.go` | Modify | swag descriptions |
| `internal/minecraft/controllers/invite-controller_test.go` | Modify | httptest JSON for `pack.commit` and both `launcher` blocks |
| `docs/docs.go`, `docs/swagger.json`, `docs/swagger.yaml` | Regenerate | `make docs` |
| `site/spec/overlay.yaml` | Modify | Examples with `commit` and `launcher` |
| `site/src/content/docs/guides/invite-players.mdx` | Modify | The launcher section and the env var |
| `site/src/data/changelog.yaml`, `site/src/data/roadmap.yaml` | Modify | PR records |
| `.env.example`, `docs/deployment.md`, `CLAUDE.md` | Modify | Env var and field notes |
| `invite/src/lib/api.ts` | Modify | `pack.commit?`, `launcher?` on `InviteView` |
| `invite/src/lib/launchers.ts` | Modify | `perlandJoinUrl`, `launcherLinks`, the `perland` tab |
| `invite/src/lib/i18n.ts` | Modify | en + es tab copy |
| `invite/src/lib/launchers.test.ts`, `invite/src/lib/i18n.test.ts` | Modify | Tests |
| `invite/src/app/render.ts` | Modify | Render the tab (through `/impeccable`) |
| `invite/src/styles/invite.css` | Modify | Tab styling (decided by `/impeccable`) |
| `invite/scripts/mock-engine.mjs`, `invite/README.md` | Modify | Mock fixtures, docs |

Invite-site deployment needs no change. The site reads the flag from `view.launcher`, so `LAUNCHER_DOWNLOAD_URL` is set on the engine only. The CSP in `invite/nginx/invite-headers.conf.template` needs no change either: a top-level navigation, including to `perland://`, is not restricted by any CSP directive.

---

### Task 1: `pack.commit` on the invite's pack summary

**Files:**
- Modify: `internal/minecraft/model/DTOs/invites/invites.go` (`PackDTO`)
- Modify: `internal/minecraft/playerpack/summary.go` (`Summary`)
- Create: `internal/minecraft/model/DTOs/invites/invites_test.go`
- Modify: `internal/minecraft/playerpack/summary_test.go` (`TestSummary`, `TestSummary_RemembersFailure`)

**Interfaces:**
- Consumes: `(*playerpack.Service).pinnedPack(ctx, server) (commit, dir string, err error)`, which already exists.
- Produces: `invitedtos.PackDTO.Commit string` (JSON `commit`, omitted when empty). Every `Summary` result that is not nil carries the pinned commit. The placeholder `&PackDTO{State: PackUnavailable}`, built in `invites.view` when `Summary` errors, has none.

- [ ] **Step 1: Write the failing DTO test**

Create `internal/minecraft/model/DTOs/invites/invites_test.go`:

```go
package invitedtos_test

import (
	"encoding/json"
	"strings"
	"testing"

	invitedtos "perland-backend/internal/minecraft/model/DTOs/invites"
)

func TestPackDTO_Commit(t *testing.T) {
	with, err := json.Marshal(invitedtos.PackDTO{Name: "p", Commit: "3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a"})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(with), `"commit":"3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a"`) {
		t.Errorf("with a commit: %s", with)
	}
	without, err := json.Marshal(invitedtos.PackDTO{Name: "p", State: invitedtos.PackUnavailable})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(without), `"commit"`) {
		t.Errorf("without a commit the field must be absent: %s", without)
	}
}
```

- [ ] **Step 2: Extend the summary tests**

In `internal/minecraft/playerpack/summary_test.go`, `TestSummary`, replace this block:

```go
	if sum == nil || sum.State != invitedtos.PackPending || sum.ZipState != invitedtos.PackPending ||
		sum.Name == "" || sum.Minecraft == "" || sum.ModCount == 0 {
		t.Fatalf("first summary: %+v", sum)
	}
	// The pending summary started a build; wait for it through Open, which
	// joins the same build, so none writes after the test's folders go.
	if _, err = f.svc.Open(ctx, tenantA, userA, stack); err != nil {
		t.Fatal(err)
	}
```

with:

```go
	if sum == nil || sum.State != invitedtos.PackPending || sum.ZipState != invitedtos.PackPending ||
		sum.Name == "" || sum.Minecraft == "" || sum.ModCount == 0 {
		t.Fatalf("first summary: %+v", sum)
	}
	if sum.Commit != f.pinned.commit {
		t.Fatalf("summary commit = %q, want the pinned %q", sum.Commit, f.pinned.commit)
	}
	// The pending summary started a build; wait for it through Open, which
	// joins the same build, so none writes after the test's folders go.
	pack, err := f.svc.Open(ctx, tenantA, userA, stack)
	if err != nil {
		t.Fatal(err)
	}
	// /pack's ETag is the quoted pack.Commit: the summary must carry the same value.
	if pack.Commit != sum.Commit {
		t.Fatalf("pack commit %q, summary commit %q", pack.Commit, sum.Commit)
	}
```

In `TestSummary_RemembersFailure`, replace:

```go
	if err != nil || sum == nil || sum.State != invitedtos.PackUnavailable || sum.ZipState != invitedtos.PackUnavailable {
		t.Fatalf("summary = %+v, %v", sum, err)
	}
```

with:

```go
	if err != nil || sum == nil || sum.State != invitedtos.PackUnavailable || sum.ZipState != invitedtos.PackUnavailable {
		t.Fatalf("summary = %+v, %v", sum, err)
	}
	// A failed build still knows which commit it was for.
	if sum.Commit != f.pinned.commit {
		t.Fatalf("summary commit = %q, want %q", sum.Commit, f.pinned.commit)
	}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `go test -p 2 ./internal/minecraft/model/DTOs/invites/ ./internal/minecraft/playerpack/ -run "TestPackDTO_Commit|TestSummary" -count=1`

Expected: a compile FAIL with `unknown field Commit in struct literal of type invitedtos.PackDTO` and `sum.Commit undefined`.

- [ ] **Step 4: Add the field**

In `internal/minecraft/model/DTOs/invites/invites.go`, in `PackDTO`, add this after `ZipState`:

```go
	// Commit is the pinned modpack commit the .mrpack is built from: the
	// /pack ETag without its quotes. A client compares it with the commit it
	// installed to know whether the pack changed. Absent when the engine
	// could not read the stack's modpack.
	Commit string `json:"commit,omitempty" example:"3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a"`
```

- [ ] **Step 5: Set it in `Summary`**

In `internal/minecraft/playerpack/summary.go`, replace:

```go
	sum := &invitedtos.PackDTO{State: invitedtos.PackUnavailable, ZipState: invitedtos.PackUnavailable}
```

with:

```go
	sum := &invitedtos.PackDTO{Commit: commit, State: invitedtos.PackUnavailable, ZipState: invitedtos.PackUnavailable}
```

Also update the doc comment of `Summary`: after "and whether the .mrpack is ready", add ", and the pinned commit (`Commit`, the /pack ETag)".

- [ ] **Step 6: Run the tests to verify they pass**

Run: `go test -p 2 ./internal/minecraft/model/DTOs/invites/ ./internal/minecraft/playerpack/ ./internal/minecraft/invites/ -count=1`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add internal/minecraft/model/DTOs/invites/invites.go internal/minecraft/model/DTOs/invites/invites_test.go internal/minecraft/playerpack/summary.go internal/minecraft/playerpack/summary_test.go
git commit -m "feat(invites): pack.commit on the invite view

The pinned modpack commit the .mrpack is built from, the /pack ETag
without its quotes, so Perland Launcher can check for updates with one
view request.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `LAUNCHER_DOWNLOAD_URL` config

**Files:**
- Modify: `config/settings.go` (after `InviteBaseURL`, around line 291)
- Modify: `config/settings_test.go` (after `TestInviteBaseURL`)

**Interfaces:**
- Produces: `config.LauncherDownloadURL() (string, error)`. It returns `"", nil` when unset, or the trimmed URL. The error message starts with `LAUNCHER_DOWNLOAD_URL`.

- [ ] **Step 1: Write the failing test**

Append to `config/settings_test.go`. Add `"strings"` to its imports if it is not there yet.

```go
func TestLauncherDownloadURL(t *testing.T) {
	for raw, want := range map[string]string{
		"":                                    "",
		"   ":                                 "",
		"https://perland.online/launcher":     "https://perland.online/launcher",
		"  https://perland.online/launcher  ": "https://perland.online/launcher",
		"https://github.com/PerlandDev/perland-launcher/releases/latest": "https://github.com/PerlandDev/perland-launcher/releases/latest",
		"https://perland.online/download?os=windows":                     "https://perland.online/download?os=windows",
		"https://perland.online/":                                        "https://perland.online/",
		"http://localhost:4322/launcher":                                 "http://localhost:4322/launcher",
		"http://127.0.0.1:8080/":                                         "http://127.0.0.1:8080/",
	} {
		t.Setenv("LAUNCHER_DOWNLOAD_URL", raw)
		got, err := LauncherDownloadURL()
		if err != nil || got != want {
			t.Errorf("LauncherDownloadURL(%q) = %q, %v; want %q", raw, got, err, want)
		}
	}
	for _, raw := range []string{
		"http://perland.online/launcher",
		"https://user@perland.online/launcher",
		"https://user:pw@perland.online/",
		"ftp://perland.online/launcher",
		"perland.online/launcher",
		"/launcher",
		"https:///launcher",
		"https://perland.online/a b",
		"javascript:alert(1)",
		"perland://join?invite=x",
		"https://perland.online/" + strings.Repeat("a", 512),
	} {
		t.Setenv("LAUNCHER_DOWNLOAD_URL", raw)
		got, err := LauncherDownloadURL()
		if err == nil {
			t.Errorf("LauncherDownloadURL(%q) = %q, want an error", raw, got)
			continue
		}
		if !strings.HasPrefix(err.Error(), "LAUNCHER_DOWNLOAD_URL") {
			t.Errorf("error %q must name the variable", err)
		}
	}
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `go test -p 2 ./config/ -run TestLauncherDownloadURL -count=1`

Expected: a compile FAIL with `undefined: LauncherDownloadURL`.

- [ ] **Step 3: Implement**

In `config/settings.go`, after `InviteBaseURL`, add:

```go
// launcherURLMax bounds LAUNCHER_DOWNLOAD_URL, like an invite's linkUrl.
const launcherURLMax = 512

// LauncherDownloadURL is LAUNCHER_DOWNLOAD_URL: where players download
// Perland Launcher, e.g. https://perland.online/launcher. Set, a stack's
// invite offers the launcher (the public view's launcher.downloadUrl and the
// owner's launcher.joinUrl and downloadUrl); empty, it does not. An absolute
// https URL (http only for localhost or 127.0.0.1) with no user info or
// whitespace, at most 512 bytes; a path, query or fragment is kept as
// written. The owner sets it only once a public launcher build can add an
// account.
func LauncherDownloadURL() (string, error) {
	raw := strings.TrimSpace(GetEnv("LAUNCHER_DOWNLOAD_URL"))
	if raw == "" {
		return "", nil
	}
	u, err := url.Parse(raw)
	if err != nil || u.Host == "" || u.User != nil || len(raw) > launcherURLMax || strings.ContainsAny(raw, " \t\r\n") ||
		(u.Scheme != "https" && (u.Scheme != "http" || (u.Hostname() != "localhost" && u.Hostname() != "127.0.0.1"))) {
		return "", fmt.Errorf("LAUNCHER_DOWNLOAD_URL %q must be an absolute https URL with no user info, at most %d bytes", raw, launcherURLMax)
	}
	return raw, nil
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `go test -p 2 ./config/ -count=1`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add config/settings.go config/settings_test.go
git commit -m "feat(config): LAUNCHER_DOWNLOAD_URL

Where players download Perland Launcher; validated like INVITE_BASE_URL
(https, http only on localhost), an invalid value is an error the boot
turns into a stop.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The `launcher` blocks on stack invites, and boot wiring

**Files:**
- Modify: `internal/minecraft/model/DTOs/invites/invites.go` (new DTOs; `InviteViewDTO.Launcher`, `InviteDTO.Launcher`)
- Modify: `internal/minecraft/model/DTOs/invites/invites_test.go` (JSON shape)
- Create: `internal/minecraft/invites/launcher.go`
- Modify: `internal/minecraft/invites/service.go` (`Service` field, `dto`)
- Modify: `internal/minecraft/invites/view.go` (`view`)
- Modify: `internal/minecraft/invites/service_test.go` (`newFixture` takes options)
- Create: `internal/minecraft/invites/launcher_test.go`
- Modify: `cmd/server/main.go` (boot)

**Interfaces:**
- Consumes:
  - `config.LauncherDownloadURL() (string, error)` (Task 2);
  - `(*invites.Service).URL(token string) string`, which already exists.
- Produces:
  - `invitedtos.LauncherViewDTO{DownloadURL string}` (JSON `{"downloadUrl"}`);
  - `invitedtos.InviteLauncherDTO{JoinURL, DownloadURL string}` (JSON `{"joinUrl"?, "downloadUrl"}`);
  - `InviteViewDTO.Launcher *LauncherViewDTO` (`launcher,omitempty`);
  - `InviteDTO.Launcher *InviteLauncherDTO` (`launcher,omitempty`);
  - `invites.JoinURL(inviteURL string) string`;
  - `invites.WithLauncherDownloadURL(u string) invites.Option`.
- Test helper: `newFixture(t *testing.T, opts ...invites.Option) *fixture`. The options are applied after the defaults (`WithBaseURL("https://play.example")`, `WithClock`).

- [ ] **Step 1: Write the failing DTO JSON test**

Append to `internal/minecraft/model/DTOs/invites/invites_test.go`:

```go
func TestLauncherBlocks_JSON(t *testing.T) {
	view, err := json.Marshal(invitedtos.InviteViewDTO{Kind: "stack"})
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(view), `"launcher"`) {
		t.Errorf("no launcher must mean no field: %s", view)
	}
	view, _ = json.Marshal(invitedtos.InviteViewDTO{Kind: "stack",
		Launcher: &invitedtos.LauncherViewDTO{DownloadURL: "https://perland.online/launcher"}})
	if !strings.Contains(string(view), `"launcher":{"downloadUrl":"https://perland.online/launcher"}`) {
		t.Errorf("public launcher: %s", view)
	}

	owner, _ := json.Marshal(invitedtos.InviteDTO{})
	if strings.Contains(string(owner), `"launcher"`) {
		t.Errorf("no launcher must mean no field: %s", owner)
	}
	owner, _ = json.Marshal(invitedtos.InviteDTO{Launcher: &invitedtos.InviteLauncherDTO{
		JoinURL: "perland://join?invite=x", DownloadURL: "https://perland.online/launcher"}})
	if !strings.Contains(string(owner), `"launcher":{"joinUrl":"perland://join?invite=x","downloadUrl":"https://perland.online/launcher"}`) {
		t.Errorf("owner launcher: %s", owner)
	}
	owner, _ = json.Marshal(invitedtos.InviteDTO{Launcher: &invitedtos.InviteLauncherDTO{DownloadURL: "https://perland.online/launcher"}})
	if strings.Contains(string(owner), `"joinUrl"`) {
		t.Errorf("an empty joinUrl must be absent: %s", owner)
	}
}
```

- [ ] **Step 2: Let `newFixture` take options**

In `internal/minecraft/invites/service_test.go`, change the signature `func newFixture(t *testing.T) *fixture {` to `func newFixture(t *testing.T, opts ...invites.Option) *fixture {`. Then replace:

```go
	f.svc = invites.NewService(f.store, f.facts, f.packs, fakeTeams{},
		invites.WithBaseURL("https://play.example"), invites.WithClock(func() time.Time { return f.now }))
```

with:

```go
	defaults := []invites.Option{invites.WithBaseURL("https://play.example"), invites.WithClock(func() time.Time { return f.now })}
	f.svc = invites.NewService(f.store, f.facts, f.packs, fakeTeams{}, append(defaults, opts...)...)
```

- [ ] **Step 3: Write the failing service tests**

Create `internal/minecraft/invites/launcher_test.go`:

```go
package invites_test

import (
	"context"
	"encoding/json"
	"testing"

	"perland-backend/internal/minecraft/entities"
	"perland-backend/internal/minecraft/invites"
	invitedtos "perland-backend/internal/minecraft/model/DTOs/invites"
)

const launcherDownload = "https://perland.online/launcher"

// fakeToken is an obviously fake 43-character invite token.
const fakeToken = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"

func TestJoinURL(t *testing.T) {
	// The invite site builds the same strings (invite/src/lib/launchers.test.ts, perlandJoinUrl).
	if got, want := invites.JoinURL("https://play.example/"+fakeToken),
		"perland://join?invite=https%3A%2F%2Fplay.example%2F"+fakeToken; got != want {
		t.Errorf("JoinURL = %q, want %q", got, want)
	}
	if got, want := invites.JoinURL("http://localhost:4322/"+fakeToken),
		"perland://join?invite=http%3A%2F%2Flocalhost%3A4322%2F"+fakeToken; got != want {
		t.Errorf("JoinURL with a port = %q, want %q", got, want)
	}
}

func TestLauncher_UnsetIsAbsent(t *testing.T) {
	ctx := context.Background()
	f := newFixture(t)
	got, err := f.svc.Update(ctx, f.tg, f.actor, invitedtos.UpdateRequestDTO{Enabled: ptr(true)})
	if err != nil {
		t.Fatal(err)
	}
	if got.Launcher != nil || got.Preview.Launcher != nil {
		t.Fatalf("launcher without LAUNCHER_DOWNLOAD_URL: %+v, preview %+v", got.Launcher, got.Preview.Launcher)
	}
	v, err := f.svc.View(ctx, got.Token, "203.0.113.1")
	if err != nil {
		t.Fatal(err)
	}
	if v.Launcher != nil {
		t.Fatalf("public launcher %+v", v.Launcher)
	}
}

func TestLauncher_Stack(t *testing.T) {
	ctx := context.Background()
	f := newFixture(t, invites.WithLauncherDownloadURL(launcherDownload))
	got, err := f.svc.Update(ctx, f.tg, f.actor, invitedtos.UpdateRequestDTO{Enabled: ptr(true)})
	if err != nil {
		t.Fatal(err)
	}
	want := invitedtos.InviteLauncherDTO{
		JoinURL:     "perland://join?invite=https%3A%2F%2Fplay.example%2F" + got.Token,
		DownloadURL: launcherDownload,
	}
	if got.Launcher == nil || *got.Launcher != want {
		t.Fatalf("owner launcher = %+v, want %+v", got.Launcher, want)
	}
	if got.Preview.Launcher == nil || got.Preview.Launcher.DownloadURL != launcherDownload {
		t.Fatalf("preview launcher = %+v", got.Preview.Launcher)
	}
	v, err := f.svc.View(ctx, got.Token, "203.0.113.1")
	if err != nil {
		t.Fatal(err)
	}
	if v.Launcher == nil || v.Launcher.DownloadURL != launcherDownload {
		t.Fatalf("public launcher = %+v", v.Launcher)
	}
}

func TestLauncher_StackWithoutModpack(t *testing.T) {
	f := newFixture(t, invites.WithLauncherDownloadURL(launcherDownload))
	f.packs.sum = nil // Summary answering nil, nil: a stack without a modpack
	token := enabled(t, f)
	v, err := f.svc.View(context.Background(), token, "203.0.113.1")
	if err != nil {
		t.Fatal(err)
	}
	if v.Pack != nil || v.Launcher == nil || v.Launcher.DownloadURL != launcherDownload {
		t.Fatalf("a stack without a modpack still offers the launcher: pack %+v, launcher %+v", v.Pack, v.Launcher)
	}
}

func TestLauncher_NetworkNever(t *testing.T) {
	ctx := context.Background()
	f := newFixture(t, invites.WithLauncherDownloadURL(launcherDownload))
	f.tg.Kind = entities.InviteKindNetwork
	got, err := f.svc.Update(ctx, f.tg, f.actor, invitedtos.UpdateRequestDTO{Enabled: ptr(true)})
	if err != nil {
		t.Fatal(err)
	}
	if got.Launcher != nil || got.Preview.Launcher != nil {
		t.Fatalf("network owner launcher %+v, preview %+v", got.Launcher, got.Preview.Launcher)
	}
	v, err := f.svc.View(ctx, got.Token, "203.0.113.1")
	if err != nil {
		t.Fatal(err)
	}
	if v.Launcher != nil {
		t.Fatalf("network public launcher %+v", v.Launcher)
	}
}

func TestLauncher_NoJoinURLWithoutAURL(t *testing.T) {
	ctx := context.Background()
	// No invite yet: no token, so nothing to open.
	f := newFixture(t, invites.WithLauncherDownloadURL(launcherDownload))
	got, err := f.svc.Get(ctx, f.tg)
	if err != nil {
		t.Fatal(err)
	}
	if got.Launcher == nil || got.Launcher.JoinURL != "" || got.Launcher.DownloadURL != launcherDownload {
		t.Fatalf("before a token: %+v", got.Launcher)
	}
	// INVITE_BASE_URL unset: a token but no url.
	f = newFixture(t, invites.WithLauncherDownloadURL(launcherDownload), invites.WithBaseURL(""))
	got, err = f.svc.Update(ctx, f.tg, f.actor, invitedtos.UpdateRequestDTO{Enabled: ptr(true)})
	if err != nil {
		t.Fatal(err)
	}
	if got.URL != "" || got.Launcher == nil || got.Launcher.JoinURL != "" {
		t.Fatalf("without a base URL: url %q, launcher %+v", got.URL, got.Launcher)
	}
	raw, err := json.Marshal(got.Launcher)
	if err != nil {
		t.Fatal(err)
	}
	if string(raw) != `{"downloadUrl":"https://perland.online/launcher"}` {
		t.Fatalf("launcher JSON %s", raw)
	}
}

func TestLauncher_RegenerateMovesJoinURL(t *testing.T) {
	ctx := context.Background()
	f := newFixture(t, invites.WithLauncherDownloadURL(launcherDownload))
	first, err := f.svc.Update(ctx, f.tg, f.actor, invitedtos.UpdateRequestDTO{Enabled: ptr(true)})
	if err != nil {
		t.Fatal(err)
	}
	next, err := f.svc.Regenerate(ctx, f.tg, f.actor)
	if err != nil {
		t.Fatal(err)
	}
	if next.Launcher == nil || next.Launcher.JoinURL != invites.JoinURL("https://play.example/"+next.Token) ||
		next.Launcher.JoinURL == first.Launcher.JoinURL {
		t.Fatalf("after regenerate: %+v (before %+v)", next.Launcher, first.Launcher)
	}
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `go test -p 2 ./internal/minecraft/model/DTOs/invites/ ./internal/minecraft/invites/ -count=1`

Expected: a compile FAIL with `undefined: invitedtos.LauncherViewDTO`, `undefined: invites.JoinURL` and `undefined: invites.WithLauncherDownloadURL`.

- [ ] **Step 5: Add the DTOs**

In `internal/minecraft/model/DTOs/invites/invites.go`:

(a) In `InviteDTO`, after the `Preview` field, add:

```go
	// Launcher offers Perland Launcher; on a stack's invite while the
	// engine's LAUNCHER_DOWNLOAD_URL is set, absent otherwise and for a
	// network.
	Launcher *InviteLauncherDTO `json:"launcher,omitempty"`
```

(b) In `InviteViewDTO`, after the `Team` field, add:

```go
	// Launcher says where players get Perland Launcher; on a stack while the
	// engine's LAUNCHER_DOWNLOAD_URL is set, absent otherwise and for a
	// network.
	Launcher *LauncherViewDTO `json:"launcher,omitempty"`
```

(c) After `TeamDTO`, add:

```go
// LauncherViewDTO is Perland Launcher on the public page.
type LauncherViewDTO struct {
	// DownloadURL is where players download Perland Launcher.
	DownloadURL string `json:"downloadUrl" example:"https://perland.online/launcher"`
}

// InviteLauncherDTO is what the owner shares for Perland Launcher.
type InviteLauncherDTO struct {
	// JoinURL opens the invite in Perland Launcher:
	// perland://join?invite=<the url-encoded invite url>. Absent without a
	// url (INVITE_BASE_URL unset, or no token yet).
	JoinURL string `json:"joinUrl,omitempty" example:"perland://join?invite=https%3A%2F%2Fplay.perland.online%2F2bYc9Q1m..."`
	// DownloadURL is where players download Perland Launcher.
	DownloadURL string `json:"downloadUrl" example:"https://perland.online/launcher"`
}
```

- [ ] **Step 6: Add `JoinURL` and the option**

Create `internal/minecraft/invites/launcher.go`:

```go
package invites

import "net/url"

// JoinURL is the link that opens the invite at inviteURL in Perland
// Launcher: perland://join?invite=<inviteURL, query-escaped>. The engine
// owns this format; the Panel shows it as given, and the invite page builds
// the same string with encodeURIComponent (identical for invite URLs, whose
// tokens are base64url).
func JoinURL(inviteURL string) string {
	return "perland://join?invite=" + url.QueryEscape(inviteURL)
}

// WithLauncherDownloadURL is LAUNCHER_DOWNLOAD_URL: set, a stack's invite
// offers Perland Launcher (the public view's and the owner's launcher
// blocks).
func WithLauncherDownloadURL(u string) Option {
	return func(s *Service) { s.launcherDownload = u }
}
```

- [ ] **Step 7: Set the blocks**

In `internal/minecraft/invites/service.go`:

(a) In `type Service struct`, after `base  string`, add:

```go
	// launcherDownload is LAUNCHER_DOWNLOAD_URL; "" offers no launcher.
	launcherDownload string
```

(b) In `dto`, after `out := &invitedtos.InviteDTO{...}` and before `if !inv.CreatedAt.IsZero() {`, add:

```go
	if t.Kind == entities.InviteKindStack && s.launcherDownload != "" {
		out.Launcher = &invitedtos.InviteLauncherDTO{DownloadURL: s.launcherDownload}
		if u := s.URL(inv.Token); u != "" {
			out.Launcher.JoinURL = JoinURL(u)
		}
	}
```

In `internal/minecraft/invites/view.go`, in `view`, after the block that sets `v.Pack = sum` (the `if t.Kind == entities.InviteKindStack && s.packs != nil { ... }` block), add:

```go
	if t.Kind == entities.InviteKindStack && s.launcherDownload != "" {
		v.Launcher = &invitedtos.LauncherViewDTO{DownloadURL: s.launcherDownload}
	}
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `go test -p 2 ./internal/minecraft/model/DTOs/invites/ ./internal/minecraft/invites/ -count=1`

Expected: PASS, including every existing invites test.

- [ ] **Step 9: Wire it at boot**

In `cmd/server/main.go`, directly after the `inviteBase` block:

```go
	inviteBase, inviteBaseErr := config.InviteBaseURL()
	if inviteBaseErr != nil {
		log.Fatalf("invite base url: %v", inviteBaseErr)
	}
	if inviteBase != "" {
		log.Printf("invite pages: %s/<token>", inviteBase)
	}
```

add:

```go
	launcherDownload, launcherErr := config.LauncherDownloadURL()
	if launcherErr != nil {
		log.Fatalf("launcher download url: %v", launcherErr)
	}
	if launcherDownload != "" {
		log.Printf("stack invites offer Perland Launcher: %s", launcherDownload)
	}
```

Then replace:

```go
			controllers.Invites = invites.NewService(invites.NewGormStore(db.DB),
				invitedocker.New(dockerCli, db.DB), invitePacks, invites.TeamsFrom(mgmtStore),
				invites.WithBaseURL(inviteBase))
```

with:

```go
			controllers.Invites = invites.NewService(invites.NewGormStore(db.DB),
				invitedocker.New(dockerCli, db.DB), invitePacks, invites.TeamsFrom(mgmtStore),
				invites.WithBaseURL(inviteBase), invites.WithLauncherDownloadURL(launcherDownload))
```

`main` has no unit tests, which is the same as for `INVITE_BASE_URL`. "An invalid value stops boot" is covered by `TestLauncherDownloadURL` (the error) plus this `log.Fatalf`. Check it once by hand without starting anything heavy: `$env:LAUNCHER_DOWNLOAD_URL="http://perland.online/x"; go run ./cmd/server`. Expected: it exits at once with `launcher download url: LAUNCHER_DOWNLOAD_URL "http://perland.online/x" must be an absolute https URL ...`. Then `Remove-Item Env:LAUNCHER_DOWNLOAD_URL`.

- [ ] **Step 10: Build, vet and test the touched packages**

Run: `go build ./... ; go vet ./cmd/... ./config/... ./internal/minecraft/... ; go test -p 2 ./config/ ./internal/minecraft/invites/... ./internal/minecraft/model/DTOs/invites/ ./internal/minecraft/playerpack/... -count=1`

Expected: no output from build or vet, and every test package `ok`.

- [ ] **Step 11: Commit**

```bash
git add internal/minecraft/model/DTOs/invites/ internal/minecraft/invites/launcher.go internal/minecraft/invites/launcher_test.go internal/minecraft/invites/service.go internal/minecraft/invites/view.go internal/minecraft/invites/service_test.go cmd/server/main.go
git commit -m "feat(invites): offer Perland Launcher on stack invites

While LAUNCHER_DOWNLOAD_URL is set, a stack's public view carries
launcher.downloadUrl and its owner invite launcher.joinUrl and
downloadUrl; joinUrl is perland://join?invite=<url-encoded invite url>,
absent without a url. Networks never carry it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Controller annotations, httptest JSON checks, swagger and SDK

**Files:**
- Modify: `internal/minecraft/controllers/invite-controller.go` (godoc of `GetInviteView`, `GetStackInvite`, `GetNetworkInvite`)
- Modify: `internal/minecraft/controllers/invite-controller_test.go` (`fakeInvites.invite`, two tests)
- Regenerate: `docs/docs.go`, `docs/swagger.json`, `docs/swagger.yaml`

**Interfaces:**
- Consumes: the DTOs from Tasks 1 and 3.
- Produces: swagger schemas `invitedtos.LauncherViewDTO` and `invitedtos.InviteLauncherDTO`, plus the `commit` property on `invitedtos.PackDTO`. The Panel plan regenerates its SDK from them.

- [ ] **Step 1: Write the failing httptest tests**

In `internal/minecraft/controllers/invite-controller_test.go`:

(a) In `type fakeInvites struct`, add a field after `mods    *invitedtos.InviteModsDTO`:

```go
	invite  *invitedtos.InviteDTO // what Get, Update and Regenerate answer, when set
```

(b) Replace `result`:

```go
func (f *fakeInvites) result() (*invitedtos.InviteDTO, error) {
	if f.err != nil {
		return nil, f.err
	}
	return &invitedtos.InviteDTO{Enabled: f.enabled, Token: inviteToken}, nil
}
```

with:

```go
func (f *fakeInvites) result() (*invitedtos.InviteDTO, error) {
	if f.err != nil {
		return nil, f.err
	}
	if f.invite != nil {
		return f.invite, nil
	}
	return &invitedtos.InviteDTO{Enabled: f.enabled, Token: inviteToken}, nil
}
```

(c) After `TestGetInviteView`, add:

```go
func TestGetInviteView_CommitAndLauncher(t *testing.T) {
	const commit = "3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a"
	f := &fakeInvites{view: &invitedtos.InviteViewDTO{Kind: "stack", Name: "survival", Status: "online",
		Pack:     &invitedtos.PackDTO{Name: "Pack", Minecraft: "1.21.1", State: invitedtos.PackReady, Commit: commit},
		Launcher: &invitedtos.LauncherViewDTO{DownloadURL: "https://perland.online/launcher"}}}
	w := doInvite(t, f, http.MethodGet, "/api/v1/invites/"+inviteToken, "", nil)
	env := decodeEnvelope(t, w)
	pack, _ := env.Data["pack"].(map[string]any)
	launcher, _ := env.Data["launcher"].(map[string]any)
	if w.Code != http.StatusOK || pack["commit"] != commit || launcher["downloadUrl"] != "https://perland.online/launcher" {
		t.Fatalf("view: %d %s", w.Code, w.Body.String())
	}
	if _, ok := launcher["joinUrl"]; ok {
		t.Errorf("the public view never carries joinUrl: %s", w.Body.String())
	}

	// A network, or an engine without LAUNCHER_DOWNLOAD_URL: no launcher, no commit.
	f = &fakeInvites{view: &invitedtos.InviteViewDTO{Kind: "network", Name: "lobby", Status: "online"}}
	w = doInvite(t, f, http.MethodGet, "/api/v1/invites/"+inviteToken, "", nil)
	env = decodeEnvelope(t, w)
	if _, ok := env.Data["launcher"]; ok || w.Code != http.StatusOK {
		t.Fatalf("network view: %d %s", w.Code, w.Body.String())
	}
}

func TestGetStackInvite_Launcher(t *testing.T) {
	join := "perland://join?invite=https%3A%2F%2Fplay.example%2F" + inviteToken
	f := &fakeInvites{invite: &invitedtos.InviteDTO{Enabled: true, Token: inviteToken,
		Launcher: &invitedtos.InviteLauncherDTO{JoinURL: join, DownloadURL: "https://perland.online/launcher"}}}
	w := doInvite(t, f, http.MethodGet, "/api/v1/stacks/survival/invite", "", nil)
	env := decodeEnvelope(t, w)
	launcher, _ := env.Data["launcher"].(map[string]any)
	if w.Code != http.StatusOK || launcher["joinUrl"] != join || launcher["downloadUrl"] != "https://perland.online/launcher" {
		t.Fatalf("owner invite: %d %s", w.Code, w.Body.String())
	}
}
```

- [ ] **Step 2: Run them**

Run: `go test -p 2 ./internal/minecraft/controllers/ -run "TestGetInviteView|TestGetStackInvite_Launcher|TestInviteOwnerHandlers" -count=1`

Expected: PASS. The handlers pass the DTOs through, so these tests pin the wire shape the Panel and launcher rely on rather than drive new handler code. If one fails, the DTO tags from Tasks 1 and 3 are wrong: fix the tags, not the test.

- [ ] **Step 3: Update the swag descriptions**

In `internal/minecraft/controllers/invite-controller.go`:

(a) In `GetInviteView`'s godoc, replace:

```go
// @Description  Public: the token in the path is the credential. What the invite page shows: the server's name, MOTD,
// @Description  icon version, address, game, status, players, whether it is whitelisted, the owner's note and link,
// @Description  the modpack and the team's branding. Never owner names, whitelist names or resources. Cached 15 s per
// @Description  invite; at most 60 requests a minute per client, an IPv6 client counted by its /64 (429). Unknown or
// @Description  disabled token: 404.
```

with:

```go
// @Description  Public: the token in the path is the credential. What the invite page shows: the server's name, MOTD,
// @Description  icon version, address, game, status, players, whether it is whitelisted, the owner's note and link,
// @Description  the modpack (its `commit` is the pinned commit the .mrpack is built from, the /pack ETag without its
// @Description  quotes) and the team's branding; for a stack, while the engine's LAUNCHER_DOWNLOAD_URL is set,
// @Description  `launcher.downloadUrl`, where players get Perland Launcher. Never owner names, whitelist names or
// @Description  resources. Cached 15 s per invite; at most 60 requests a minute per client, an IPv6 client counted by
// @Description  its /64 (429). Unknown or disabled token: 404.
```

(b) In `GetStackInvite`'s godoc, replace:

```go
// @Description  see on the page, built whether or not the invite is on. A stack without an invite answers
// @Description  `enabled: false` and no token.
```

with:

```go
// @Description  see on the page, built whether or not the invite is on. A stack without an invite answers
// @Description  `enabled: false` and no token. While the engine's LAUNCHER_DOWNLOAD_URL is set, `launcher` carries
// @Description  `downloadUrl` and `joinUrl` (`perland://join?invite=<the url-encoded url>`, absent without a `url`),
// @Description  the link that opens the invite in Perland Launcher.
```

(c) In `GetNetworkInvite`'s godoc, replace:

```go
// @Description  Same as the stack invite, for a network: the page shows the proxy's address, MOTD and icon, and the
// @Description  players of its running backends.
```

with:

```go
// @Description  Same as the stack invite, for a network: the page shows the proxy's address, MOTD and icon, and the
// @Description  players of its running backends. A network's invite never carries `launcher`.
```

- [ ] **Step 4: Regenerate swagger**

Run: `make docs`. Without `swag` installed, run `go run github.com/swaggo/swag/cmd/swag@v1.16.6 init -g cmd/server/main.go -o docs` instead.

Then check: `git diff --stat docs/` shows `docs.go`, `swagger.json` and `swagger.yaml`. Run `Select-String -Path docs/swagger.json -Pattern '"invitedtos.InviteLauncherDTO"','"invitedtos.LauncherViewDTO"','"commit"'`. Expected: both schema names are found, and `commit` appears inside `invitedtos.PackDTO`.

- [ ] **Step 5: Regenerate the SDK (a check only; `sdk/out/` is ignored)**

Run: `make sdk`. It needs Node and Java for openapi-generator; it uses `npx` on Windows.

Expected: `OK SDK written to sdk/out/`. Then run `Select-String -Path sdk/out/models/*.ts -Pattern 'downloadUrl','joinUrl' | Select-Object -First 4`. Expected: matches in the generated launcher models. Commit nothing from `sdk/out/`.

- [ ] **Step 6: Run the whole Go suite and lint**

Run: `$env:GOTMPDIR="$PWD\.gotmp"; go vet ./... ; go test -p 2 ./... -count=1`. If `golangci-lint` is installed, also run `golangci-lint run`.

Expected: all `ok`, no lint findings in touched files.

- [ ] **Step 7: Commit**

```bash
git add internal/minecraft/controllers/invite-controller.go internal/minecraft/controllers/invite-controller_test.go docs/docs.go docs/swagger.json docs/swagger.yaml
git commit -m "docs(api): pack.commit and the launcher blocks in the invite routes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Docs, overlay examples, env docs, CLAUDE.md and PR records

**Files:**
- Modify: `site/spec/overlay.yaml` (stack invite `on` and `never_set`, public view `fabric_online` and `sleeping`)
- Modify: `site/src/content/docs/guides/invite-players.mdx` (sections 2 and 5)
- Modify: `.env.example`, `docs/deployment.md`, `CLAUDE.md`
- Modify: `site/src/data/changelog.yaml`, `site/src/data/roadmap.yaml`

**Interfaces:**
- Consumes: the field names `commit`, `launcher.downloadUrl` and `launcher.joinUrl` from Tasks 1 and 3.

- [ ] **Step 1: Overlay examples**

In `site/spec/overlay.yaml`:

(a) Under `# GET /api/v1/stacks/{stackName}/invite`, example `on`:
- in `preview.pack`, add `commit: 3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a`, so the line reads `pack: { name: Asalith Fields, version: 1.0.4, minecraft: 1.21.1, loader: fabric, loaderVersion: 0.16.5, modCount: 142, state: ready, zipState: ready, commit: 3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a }`;
- after `preview.team`, add `launcher: { downloadUrl: https://perland.online/launcher }` at the same indentation as `team`;
- after `preview` (at the indentation of `enabled`), add:

```yaml
              launcher:
                joinUrl: perland://join?invite=https%3A%2F%2Fplay.perland.online%2F<TOKEN>
                downloadUrl: https://perland.online/launcher
```

Replace `<TOKEN>` with the exact 43 characters of the same example's `token:` line (the overlay's existing documentation token). Its alphabet is base64url, so it needs no escaping, and the result must equal `JoinURL(url)` of that example. Leave no `<TOKEN>` text in the file.

(b) Same route, example `never_set`:
- after `preview.team`, add `launcher: { downloadUrl: https://perland.online/launcher }`;
- after `preview` (at the indentation of `enabled`), add `launcher: { downloadUrl: https://perland.online/launcher }`. There is no `joinUrl`: there is no token yet.
- Extend its `summary` to: `A stack that never had an invite; there is no token or url yet, so launcher has no joinUrl`.

(c) Under `# GET /api/v1/invites/{token}`, example `fabric_online`:
- add `commit: 3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a` inside `pack: { ... }`;
- after `team`, add `launcher: { downloadUrl: https://perland.online/launcher }`;
- extend its `summary` with `; the operator set LAUNCHER_DOWNLOAD_URL`.

(d) Same route, example `sleeping` (a stack with no modpack): after `team`, add `launcher: { downloadUrl: https://perland.online/launcher }`.

Leave the network examples unchanged: networks never carry `launcher`.

- [ ] **Step 2: Guide**

In `site/src/content/docs/guides/invite-players.mdx`, after the paragraph in "## 2. What players see" that ends "...past that the answer is 429.", add:

```mdx
When the operator sets `LAUNCHER_DOWNLOAD_URL`, a stack's page also carries `launcher.downloadUrl`, and the page lists Perland Launcher first (selected on a computer; on a phone it says to open the page on a computer): one button opens the invite in the launcher, which adds the server, installs exactly the modpack commit the server runs (`pack.commit`) and joins it. Your own `GET /api/v1/stacks/{stackName}/invite` then carries `launcher.joinUrl`, a `perland://join?invite=...` link to share or put behind a button, and `launcher.downloadUrl`. `joinUrl` needs the invite's `url`, so it is absent while `INVITE_BASE_URL` is unset. Networks do not get the launcher yet.
```

In "## 5. Set up the play domain", after "...keep the connect address in step with `PUBLIC_GAME_HOST`.", add: `` `LAUNCHER_DOWNLOAD_URL` (an https address) turns on the Perland Launcher offer on stack invites; the play site needs no setting of its own for it. ``

- [ ] **Step 3: Env docs**

In `.env.example`, after the `# INVITE_BASE_URL=` line and its blank line, add:

```
# Where players download Perland Launcher, e.g. https://perland.online/launcher
# (an absolute https URL; http only for localhost or 127.0.0.1; no user info;
# at most 512 bytes). Set, a stack's invite offers the launcher: the public view
# carries launcher.downloadUrl and the owner's invite launcher.joinUrl
# (perland://join?invite=<url-encoded invite url>, needs INVITE_BASE_URL) and
# downloadUrl, and the play site shows a Perland Launcher tab. Unset: no offer.
# An invalid value stops the engine at boot.
# LAUNCHER_DOWNLOAD_URL=

```

In `docs/deployment.md`, add a row directly after the `INVITE_BASE_URL` row of the engine env table (line 160):

```
| `LAUNCHER_DOWNLOAD_URL` | Where players download Perland Launcher, e.g. `https://perland.online/launcher` (https; `http` only for `localhost` or `127.0.0.1`; no user info; at most 512 bytes; an invalid value stops boot). Set, stack invites offer the launcher (`launcher` on the public view and the owner's invite) and the play site shows its tab; the play site itself needs no new setting. Set it only once a public launcher build can add an account. Optional: unset, no offer |
```

In the same file, "7. Invite site", step 6 ("**Engine**: set `INVITE_BASE_URL` ..."), append: `` Later, `LAUNCHER_DOWNLOAD_URL` turns on the Perland Launcher tab; nothing changes on the play site. ``

- [ ] **Step 4: CLAUDE.md**

Make four edits. The lines are long, so use exact substring replacements.

(a) Invites route table, `GET /:token` row: replace

`` `pack` (a stack with a modpack: name, version, minecraft, loader, `modCount`, `state` `ready`, `pending` or `unavailable` for the `.mrpack` and `zipState`, the same for the classic zip; else null) ``

with

`` `pack` (a stack with a modpack: name, version, minecraft, loader, `modCount`, `state` `ready`, `pending` or `unavailable` for the `.mrpack` and `zipState`, the same for the classic zip, and `commit`, the pinned commit the `.mrpack` is built from (the `/pack` ETag without quotes; absent when the modpack could not be read); else null), `launcher` (`downloadUrl`; a stack only, while `LAUNCHER_DOWNLOAD_URL` is set) ``

(b) Stacks route table, `GET /:stackName/invite` row: replace

`` `language` and `preview`, the public view built whether or not the invite is on; ``

with

`` `language`, `preview`, the public view built whether or not the invite is on, and `launcher` (while `LAUNCHER_DOWNLOAD_URL` is set: `downloadUrl` and `joinUrl`, `perland://join?invite=<url-encoded url>` from `invites.JoinURL`, absent without a `url`); ``

(c) In the "An invite is a public page per stack or network" design decision, after `` unset, invites work and `url` is absent. ``, insert:

`` `LAUNCHER_DOWNLOAD_URL` (`config.LauncherDownloadURL`: https, `http` for `localhost` or `127.0.0.1`, no user info or whitespace, 512 bytes; an invalid value stops boot) turns on Perland Launcher for stacks: `view` adds `launcher.downloadUrl` (so it is cached with the view and shows in `preview`) and `dto` adds `launcher.joinUrl` and `downloadUrl`; networks never get it. The engine owns the `perland://join?invite=` format (`invites.JoinURL`, `url.QueryEscape` of the invite URL); the Panel shows `joinUrl` as given, and the invite page builds the identical string from `location.origin + "/" + token` (`perlandJoinUrl` in `invite/src/lib/launchers.ts`), and shows a first Perland Launcher tab only when `view.launcher` is present. That tab is the default only on a desktop (`prefersLauncherTab`: no coarse pointer and no mobile user agent; `defaultLauncher`). On a phone the previous default stays, and the tab shows "open this page on your computer" instead of the `perland://` button. ``

(d) In the optional env var list, replace

`` `INVITE_BASE_URL` (the play site that serves invite pages; builds an invite's `url`), ``

with

`` `INVITE_BASE_URL` (the play site that serves invite pages; builds an invite's `url`), `LAUNCHER_DOWNLOAD_URL` (where players download Perland Launcher; set, stack invites offer the launcher), ``

- [ ] **Step 5: Changelog**

In `site/src/data/changelog.yaml`:
- If the first `- date:` under `entries:` is today's date, add these two items at the top of its `items:`.
- Otherwise, insert a new block above it: `  - date: <today, YYYY-MM-DD>` / `    items:`, holding the two items.

```yaml
      - kind: added
        areas: [invites]
        text: "The invite view's `pack` carries `commit`, the pinned modpack commit its `.mrpack` is built from (the `/pack` ETag without its quotes), in `GET /api/v1/invites/{token}` and in the owner invite's `preview`. A client compares it with the commit it installed to know from one view request whether the pack changed. Absent when the engine could not read the stack's modpack."
      - kind: added
        areas: [invites, operators]
        text: "Perland Launcher on invites, behind a new engine setting, `LAUNCHER_DOWNLOAD_URL` (an absolute https URL; `http` only for `localhost` or `127.0.0.1`; no user info; at most 512 bytes; an invalid value stops boot). While it is set, a stack's public `GET /api/v1/invites/{token}` carries `launcher: {downloadUrl}`, and a stack's `GET` and `PUT /api/v1/stacks/{stackName}/invite` and `POST .../invite/regenerate` carry `launcher: {joinUrl, downloadUrl}`. `joinUrl` is `perland://join?invite=<the url-encoded invite url>`, absent while the invite has no `url` (`INVITE_BASE_URL` unset or no token yet). Networks never carry `launcher`. The invite page then lists a Perland Launcher tab first, with a download link. On a computer the tab is selected and holds Open in Perland Launcher; on a phone the usual tab stays selected, and the Perland Launcher tab asks to open the page on a computer."
```

- [ ] **Step 6: Roadmap**

In `site/src/data/roadmap.yaml`:

(a) After the item `- title: Invite pages in English and Spanish` (its last line is `shipped: 2026-10-07`), add:

```yaml
  - title: Join from Perland Launcher
    status: in-progress
    repo: engine
    phase: servers
    summary: Invites tell Perland Launcher the modpack commit a server runs and, once the operator turns it on, offer an Open in Perland Launcher link on the invite page and to owners, so a player goes from the link to the server with exactly its modpack.
```

(b) After the panel item `- title: Invite language picker` (its last line is `shipped: 2026-10-07`), add:

```yaml
  - title: Open in launcher from the invite dialog
    status: planned
    repo: panel
    phase: panel
    summary: An Open in launcher button and a download hint in the invite dialog, shown when the engine offers Perland Launcher. Pairs with the engine's Join from Perland Launcher.
```

- [ ] **Step 7: Build and test the docs site**

Run in `site/`: `pnpm install` (on a failure, retry with `--ignore-scripts`), then `pnpm test`, then `pnpm build`.

Expected: both succeed. The build validates the overlay examples against the regenerated swagger, the records and the guide frontmatter. A "stale overlay target" or "example does not match schema" error means Step 1 or Task 4 Step 4 is wrong: fix it there.

- [ ] **Step 8: Commit**

```bash
git add site/spec/overlay.yaml site/src/content/docs/guides/invite-players.mdx .env.example docs/deployment.md CLAUDE.md site/src/data/changelog.yaml site/src/data/roadmap.yaml
git commit -m "docs: pack.commit, LAUNCHER_DOWNLOAD_URL and the launcher blocks

Overlay examples, the invite guide, env docs, CLAUDE.md, changelog and
roadmap items.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Invite site: the data, the join link and the Perland Launcher tab (logic)

**Files:**
- Modify: `invite/src/lib/api.ts` (`InviteView`)
- Modify: `invite/src/lib/launchers.ts`
- Modify: `invite/src/lib/i18n.ts` (`launch` in `en` and `es`)
- Modify: `invite/src/lib/launchers.test.ts`, `invite/src/lib/i18n.test.ts`

**Interfaces:**
- Consumes:
  - the engine JSON `pack.commit?` and `launcher?: { downloadUrl }` (Tasks 1 and 3);
  - `safeUrl(raw?: string): string | undefined` from `invite/src/lib/mods.ts`, which already exists.
- Produces, for Task 7:
  - `interface LauncherLinks { joinUrl: string; downloadUrl: string }`: what the view offers;
  - `interface LauncherTab { joinUrl?: string; downloadUrl: string }`: what the tab shows. `joinUrl` is absent on a phone;
  - `Launcher.id` gains `'perland'`;
  - `Launcher.open?: LauncherTab`. It is set only on the `perland` tab, whose `steps` is `[]`;
  - `PlayContext.launcher?: LauncherLinks`;
  - `PlayContext.desktop?: boolean`. Absent counts as a desktop; `render.ts` always sets it;
  - `interface DeviceEnv { coarsePointer: boolean; mobileUA: boolean }`;
  - `isMobileUA(userAgent: string): boolean`;
  - `prefersLauncherTab(env: DeviceEnv): boolean`;
  - `defaultLauncher(list: Launcher[], desktop: boolean): Launcher['id']`;
  - `perlandJoinUrl(origin: string, token: string): string`;
  - `launcherLinks(view: Pick<InviteView, 'kind' | 'launcher'>, origin: string, token: string): LauncherLinks | undefined`;
  - `messages(lang).launch.names.perland`, `.openLauncher`, `.onComputer`, `.noLauncher`, `.getLauncher`.

- [ ] **Step 1: Write the failing launcher tests**

In `invite/src/lib/launchers.test.ts`, change the import line to:

```ts
import {
  defaultLauncher, installerUrl, isMobileUA, launcherLinks, launchers, perlandJoinUrl, prefersLauncherTab, profileId,
  type Part, type PlayContext,
} from './launchers.ts'
```

and append:

```ts
const T = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
const L = { joinUrl: `perland://join?invite=https%3A%2F%2Fplay.example%2F${T}`, downloadUrl: 'https://perland.online/launcher' }

test('perlandJoinUrl builds the same link as the engine', () => {
  // internal/minecraft/invites/launcher_test.go TestJoinURL expects these exact strings.
  assert.equal(perlandJoinUrl('https://play.example', T), `perland://join?invite=https%3A%2F%2Fplay.example%2F${T}`)
  assert.equal(perlandJoinUrl('http://localhost:4322', T), `perland://join?invite=http%3A%2F%2Flocalhost%3A4322%2F${T}`)
})

test('launcherLinks: only a stack whose engine sent a safe download URL', () => {
  const view = { kind: 'stack' as const, launcher: { downloadUrl: 'https://perland.online/launcher' } }
  assert.deepEqual(launcherLinks(view, 'https://play.example', T), L)
  assert.equal(launcherLinks({ kind: 'stack' }, 'https://play.example', T), undefined)
  assert.equal(launcherLinks({ ...view, kind: 'network' }, 'https://play.example', T), undefined)
  assert.equal(launcherLinks({ kind: 'stack', launcher: { downloadUrl: 'javascript:alert(1)' } }, 'https://play.example', T), undefined)
  assert.equal(launcherLinks({ kind: 'stack', launcher: { downloadUrl: 'http://perland.online/launcher' } }, 'https://play.example', T), undefined)
  assert.equal(launcherLinks({ kind: 'stack', launcher: { downloadUrl: '' } }, 'https://play.example', T), undefined)
  assert.equal(
    launcherLinks({ kind: 'stack', launcher: { downloadUrl: 'http://localhost:4322/launcher' } }, 'http://localhost:4322', T)?.downloadUrl,
    'http://localhost:4322/launcher',
  )
})

test('the Perland Launcher tab comes first, for packs and plain servers alike', () => {
  const modded = launchers({ ...ctx, launcher: L })
  assert.deepEqual(modded.map((l) => l.id), ['perland', 'modrinth', 'prism', 'official', 'curseforge'])
  assert.equal(modded[0].name, 'Perland Launcher')
  assert.deepEqual(modded[0].open, L)
  assert.deepEqual(modded[0].steps, [])
  assert.ok(modded.slice(1).every((l) => l.open === undefined))
  const plain = launchers({ ...ctx, hasPack: false, hasZip: false, launcher: L })
  assert.deepEqual(plain.map((l) => l.id), ['perland', 'minecraft'])
})

test('without the launcher block there is no Perland Launcher tab, on any device', () => {
  for (const desktop of [true, false]) {
    assert.ok(!launchers({ ...ctx, desktop }).some((l) => l.id === 'perland'), `pack, desktop ${desktop}`)
    assert.ok(!launchers({ ...ctx, hasPack: false, desktop }).some((l) => l.id === 'perland'), `plain, desktop ${desktop}`)
    assert.equal(defaultLauncher(launchers({ ...ctx, desktop }), desktop), 'modrinth')
    assert.equal(defaultLauncher(launchers({ ...ctx, hasPack: false, desktop }), desktop), 'minecraft')
  }
})

test('prefersLauncherTab: a desktop is a fine pointer and a desktop user agent', () => {
  assert.equal(prefersLauncherTab({ coarsePointer: false, mobileUA: false }), true)
  assert.equal(prefersLauncherTab({ coarsePointer: true, mobileUA: false }), false) // an iPad asking for the desktop site
  assert.equal(prefersLauncherTab({ coarsePointer: false, mobileUA: true }), false)
  assert.equal(prefersLauncherTab({ coarsePointer: true, mobileUA: true }), false)
})

test('isMobileUA', () => {
  assert.equal(isMobileUA('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36'), false)
  assert.equal(isMobileUA('Mozilla/5.0 (Macintosh; Intel Mac OS X 14_6) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15'), false)
  assert.equal(isMobileUA('Mozilla/5.0 (X11; Linux x86_64; rv:142.0) Gecko/20100101 Firefox/142.0'), false)
  assert.equal(isMobileUA('Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Mobile Safari/537.36'), true)
  assert.equal(isMobileUA('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'), true)
  assert.equal(isMobileUA('Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1'), true)
  assert.equal(isMobileUA(''), false)
})

test('desktop: the Perland Launcher tab opens selected, with its button', () => {
  const list = launchers({ ...ctx, launcher: L, desktop: true })
  assert.equal(defaultLauncher(list, true), 'perland')
  assert.deepEqual(list[0].open, L)
  const plain = launchers({ ...ctx, hasPack: false, hasZip: false, launcher: L, desktop: true })
  assert.equal(defaultLauncher(plain, true), 'perland')
})

test('phone: the previous default stays, and the tab has no perland:// link', () => {
  const list = launchers({ ...ctx, launcher: L, desktop: false })
  assert.deepEqual(list.map((l) => l.id), ['perland', 'modrinth', 'prism', 'official', 'curseforge'])
  assert.equal(defaultLauncher(list, false), 'modrinth')
  assert.deepEqual(list[0].open, { downloadUrl: L.downloadUrl })
  assert.ok(!JSON.stringify(list).includes('perland://'), 'no perland:// link anywhere on a phone')
  const plain = launchers({ ...ctx, hasPack: false, hasZip: false, launcher: L, desktop: false })
  assert.deepEqual(plain.map((l) => l.id), ['perland', 'minecraft'])
  assert.equal(defaultLauncher(plain, false), 'minecraft')
  assert.ok(!JSON.stringify(plain).includes('perland://'))
})
```

- [ ] **Step 2: Write the failing i18n tests**

In `invite/src/lib/i18n.test.ts`:

(a) Above `test('the Spanish steps hold ...'`, add:

```ts
const LAUNCHER = { joinUrl: 'perland://join?invite=x', downloadUrl: 'https://perland.online/launcher' }
```

(b) In that test's `contexts` array, add three entries:

```ts
    { address: 'play.example:25566', minecraft: '1.21.1', loader: 'fabric', loaderVersion: '0.16.5', hasPack: true, hasZip: true, launcher: LAUNCHER },
    { address: 'play.example', minecraft: '1.21.1', hasPack: false, hasZip: false, launcher: LAUNCHER },
    { address: 'play.example:25566', minecraft: '1.21.1', loader: 'fabric', loaderVersion: '0.16.5', hasPack: true, hasZip: true, launcher: LAUNCHER, desktop: false },
```

Then replace its loop body line:

```ts
    en.forEach((l, i) => assert.deepEqual(shape(es[i].steps), shape(l.steps), `${l.id} ${JSON.stringify(ctx)}`))
```

with:

```ts
    en.forEach((l, i) => {
      assert.deepEqual(shape(es[i].steps), shape(l.steps), `${l.id} ${JSON.stringify(ctx)}`)
      assert.deepEqual(es[i].open, l.open, `${l.id} links`)
    })
```

(c) Append:

```ts
test('the Perland Launcher tab reads in both languages', () => {
  for (const lang of LANGS) {
    const l = MESSAGES[lang].launch
    for (const text of [l.names.perland, l.openLauncher, l.onComputer, l.noLauncher, l.getLauncher]) assert.ok(text.trim(), `${lang}: empty launcher text`)
  }
  assert.equal(MESSAGES.en.launch.openLauncher, 'Open in Perland Launcher')
  assert.equal(MESSAGES.en.launch.onComputer, 'Open this page on your computer to play with Perland Launcher.')
  assert.equal(MESSAGES.en.launch.noLauncher, "Don't have it?")
  assert.equal(MESSAGES.en.launch.getLauncher, 'Download Perland Launcher')
  assert.equal(MESSAGES.es.launch.names.perland, 'Perland Launcher')
  assert.equal(MESSAGES.es.launch.openLauncher, 'Abrir en Perland Launcher')
  assert.equal(MESSAGES.es.launch.onComputer, 'Abre esta página en tu computadora para jugar con Perland Launcher.')
  assert.equal(MESSAGES.es.launch.noLauncher, '¿No lo tienes?')
  assert.equal(MESSAGES.es.launch.getLauncher, 'Descarga Perland Launcher')
})
```

- [ ] **Step 3: Run the tests to verify they fail**

Run in `invite/`: `pnpm install` (on a failure, retry with `--ignore-scripts`), then `pnpm test`.

Expected: FAIL. The cause is `launcherLinks`, `perlandJoinUrl`, `prefersLauncherTab`, `isMobileUA` or `defaultLauncher` not being exported (a SyntaxError on the import), or `names.perland` being undefined.

- [ ] **Step 4: Mirror the engine fields in `api.ts`**

In `invite/src/lib/api.ts`, inside `pack: null | { ... }`, after `zipState?: ...`, add:

```ts
    /** The pinned modpack commit (the /pack ETag without quotes); absent from engines before it. */
    commit?: string
```

and after `team: { displayName: string; accent: string }`, add:

```ts
  /** Perland Launcher: a stack's, while the engine's LAUNCHER_DOWNLOAD_URL is set; absent otherwise. */
  launcher?: { downloadUrl: string }
```

- [ ] **Step 5: Implement in `launchers.ts`**

In `invite/src/lib/launchers.ts`:

(a) Replace the import line `import { messages, type Lang } from './i18n.ts'` with:

```ts
import type { InviteView } from './api.ts'
import { messages, type Lang } from './i18n.ts'
import { safeUrl } from './mods.ts'
```

(b) Replace the `Launcher` interface with:

```ts
/** Perland Launcher's two links: the perland:// link that opens this invite, and the download page. */
export interface LauncherLinks {
  joinUrl: string
  downloadUrl: string
}

/** What the Perland Launcher tab shows: the button's link on a desktop only (a phone cannot run the launcher), and the download. */
export interface LauncherTab {
  joinUrl?: string
  downloadUrl: string
}

export interface Launcher {
  id: 'perland' | 'minecraft' | 'modrinth' | 'prism' | 'official' | 'curseforge'
  name: string
  steps: Part[][]
  /** The Perland Launcher tab: the button (desktop) or a line saying to open the page on a computer (phone), and where to get the launcher; its steps are empty. */
  open?: LauncherTab
}

/** What render reads from the device: a coarse primary pointer, and a mobile user agent. */
export interface DeviceEnv {
  coarsePointer: boolean
  mobileUA: boolean
}
```

(c) In `PlayContext`, after `hasZip: boolean`, add:

```ts
  /** Perland Launcher, when the engine offers it for this server (view.launcher). */
  launcher?: LauncherLinks
  /** The visitor is on a desktop (prefersLauncherTab); absent counts as one. render always sets it. */
  desktop?: boolean
```

(d) After `const enc = encodeURIComponent`, add:

```ts
/** The perland:// link that opens the invite at origin/token in Perland Launcher; the engine builds the same (invites.JoinURL). */
export function perlandJoinUrl(origin: string, token: string): string {
  return `perland://join?invite=${enc(`${origin}/${token}`)}`
}

/** Perland Launcher's links for a view: a stack whose engine sent a safe download URL, else none. */
export function launcherLinks(view: Pick<InviteView, 'kind' | 'launcher'>, origin: string, token: string): LauncherLinks | undefined {
  const downloadUrl = view.kind === 'stack' ? safeUrl(view.launcher?.downloadUrl) : undefined
  return downloadUrl ? { joinUrl: perlandJoinUrl(origin, token), downloadUrl } : undefined
}

const MOBILE_UA = /Android|iPhone|iPad|iPod|Mobile/i

/** Whether a user agent names a phone or tablet. */
export function isMobileUA(userAgent: string): boolean {
  return MOBILE_UA.test(userAgent)
}

/**
 * Whether the Perland Launcher tab opens selected: on a desktop only (owner
 * decision 2026-10-11). A coarse pointer counts as a phone, which also
 * catches an iPad that asks for the desktop site.
 */
export function prefersLauncherTab(env: DeviceEnv): boolean {
  return !env.coarsePointer && !env.mobileUA
}

/** The tab selected first: the list's first on a desktop; on a phone the first that is not Perland Launcher, as before it existed. */
export function defaultLauncher(list: Launcher[], desktop: boolean): Launcher['id'] {
  return (desktop ? list[0] : list.find((l) => l.id !== 'perland') ?? list[0]).id
}
```

(e) In `launchers()`:
- Update the doc comment to start: "The ways to play, for the launcher tabs: Perland Launcher first when the engine offers it."
- Replace:

```ts
  if (!ctx.hasPack) {
    return [{ id: 'minecraft', name: m.names.minecraft, steps: [[m.open(game)], join] }]
  }
```

with:

```ts
  // On a phone the tab keeps its download link but loses the perland:// button.
  const open = ctx.launcher && (ctx.desktop === false ? { downloadUrl: ctx.launcher.downloadUrl } : ctx.launcher)
  const perland: Launcher[] = open ? [{ id: 'perland', name: m.names.perland, steps: [], open }] : []
  if (!ctx.hasPack) {
    return [...perland, { id: 'minecraft', name: m.names.minecraft, steps: [[m.open(game)], join] }]
  }
```

- Replace:

```ts
  const list: Launcher[] = [
    { id: 'modrinth', name: m.names.modrinth, steps: last(m.modrinth) },
```

with:

```ts
  const list: Launcher[] = [
    ...perland,
    { id: 'modrinth', name: m.names.modrinth, steps: last(m.modrinth) },
```

- [ ] **Step 6: Add the copy in `i18n.ts`**

In `invite/src/lib/i18n.ts`, in `en.launch`:
- in `names`, add `perland: 'Perland Launcher',` as the first property;
- after `open: (game: string) => ...,`, add:

```ts
    /** The Perland Launcher tab: its button (desktop), the line in its place (phone), and the line under it that leads to the download. */
    openLauncher: 'Open in Perland Launcher',
    onComputer: 'Open this page on your computer to play with Perland Launcher.',
    noLauncher: "Don't have it?",
    getLauncher: 'Download Perland Launcher',
```

In `es.launch`:
- in `names`, add `perland: 'Perland Launcher',` as the first property;
- after `open: (game) => ...,`, add:

```ts
    openLauncher: 'Abrir en Perland Launcher',
    onComputer: 'Abre esta página en tu computadora para jugar con Perland Launcher.',
    noLauncher: '¿No lo tienes?',
    getLauncher: 'Descarga Perland Launcher',
```

- [ ] **Step 7: Run the tests to verify they pass**

Run in `invite/`: `pnpm test`, then `pnpm check`.

Expected: every test passes, and `astro check` reports 0 errors. If `check` fails in `render.ts` because the new `Launcher.open` is not handled, that is expected; Task 7 fixes it. A type error elsewhere must be fixed here.

- [ ] **Step 8: Commit**

```bash
git add invite/src/lib/api.ts invite/src/lib/launchers.ts invite/src/lib/i18n.ts invite/src/lib/launchers.test.ts invite/src/lib/i18n.test.ts
git commit -m "feat(invite): Perland Launcher tab data and join link

launcherLinks() offers the tab only for a stack whose view carries a safe
launcher.downloadUrl; perlandJoinUrl() builds the same perland:// link
as the engine. The tab comes first on every device but is the default
only on a desktop (prefersLauncherTab, defaultLauncher); on a phone it
says to open the page on a computer instead of the perland:// button.
en and es copy.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Invite site: render the tab, through `/impeccable`

UI styling on the invite site goes through the `/impeccable` skill. Invoke it at the start of this task with:
- the surface brief `invite/.impeccable/surfaces/invite-src-pages-index-astro.md`. The repo root `.impeccable/surfaces/` holds an older copy; let impeccable resolve which one its config uses;
- `invite/DESIGN.md`, and the root `DESIGN.md` (the Director HUD world).

This task fixes the structure, behaviour, copy and tests. Impeccable decides the look: button style, spacing, the line under the button, how the tab sits among the others, the phone layout. It also runs its finish review and records `invite/DESIGN.md`.

**Design tension for impeccable to resolve:** the brief's direction contract says the visitor "copies the address with the one primary button". The spec calls **Open in Perland Launcher** "the primary button" of its tab. Impeccable picks the treatment (for example, primary within the tab panel while Copy address stays the page's primary). It must not make the two compete. Record the decision in `invite/DESIGN.md`.

**Files:**
- Modify: `invite/src/app/render.ts`
- Modify: `invite/src/styles/invite.css` (as impeccable decides)
- Modify: `invite/scripts/mock-engine.mjs`, `invite/README.md`
- Modify: `invite/DESIGN.md` (impeccable's documenter)

**Interfaces:**
- Consumes from Task 6:
  - `launcherLinks`, `LauncherTab`, `Launcher.open`;
  - `DeviceEnv`, `isMobileUA`, `prefersLauncherTab`, `defaultLauncher`;
  - the `launch.openLauncher` / `onComputer` / `noLauncher` / `getLauncher` copy.

**Behaviour contract (impeccable may restyle, never change):**
- The tab exists only when `launcherLinks(view, location.origin, token)` returns links. It is the first tab on every device.
- **Default tab: on desktop only** (owner decision 2026-10-11).
  - When no valid tab is selected yet, `renderLaunchers` picks `defaultLauncher(list, desktop)`, where `desktop = prefersLauncherTab(deviceEnv())`.
  - On a desktop the Perland Launcher tab is selected on first render. On a phone the previous default is selected (Modrinth App, or Minecraft for a plain server).
  - `render.ts` only gathers the env. The decision lives in `launchers.ts` and is unit tested there.
- **Desktop:** **Open in Perland Launcher** is an `<a href="perland://join?invite=...">`.
  - It opens in the same tab, with no `target` and no `download` attribute.
  - Its href is exactly `open.joinUrl`.
- **Phone:** no `perland://` link is rendered. In the button's place is the plain line `launch.onComputer`: "Open this page on your computer to play with Perland Launcher." / "Abre esta página en tu computadora para jugar con Perland Launcher."
- **Don't have it? Download Perland Launcher** is always visible under the button or the line, on every device. Its link is `open.downloadUrl`, with `target="_blank" rel="noopener noreferrer"`.
- The button and the links are keyboard reachable inside the tab panel, and get 44px touch targets on phones (per the brief).
- A language switch re-renders the tab in the new language and keeps it selected. This already works: `playKey` includes `lang`, and `selectedLauncher` survives re-renders.
- Networks never show the tab, and nor do engines without `launcher`. Every other tab and step is unchanged.
- No inline styles or scripts (strict CSP). Use classes only.
- No new copy beyond Task 6's four strings and the tab name unless the owner approves it. The spec and the owner's decision fix the copy.

- [ ] **Step 1: Invoke `/impeccable`**

Invoke `/impeccable` for this change. Give it the behaviour contract and the design tension above, and this task's skeleton (Step 2) as the starting structure.

- [ ] **Step 2: Wire the tab into `render.ts` (structure; impeccable refines classes and styles)**

In `invite/src/app/render.ts`:

(a) Change the launchers import to:

```ts
import {
  defaultLauncher, isMobileUA, launcherLinks, launchers, prefersLauncherTab,
  type DeviceEnv, type Launcher, type LauncherTab, type Part, type PlayContext,
} from '../lib/launchers.ts'
```

(b) After the `let packUrls = ...` line, add:

```ts
/** What the device says about itself, for prefersLauncherTab; the decision lives in launchers.ts. */
function deviceEnv(): DeviceEnv {
  return {
    coarsePointer: typeof window.matchMedia === 'function' && window.matchMedia('(pointer: coarse)').matches,
    mobileUA: isMobileUA(navigator.userAgent),
  }
}
```

In `render()`, in the `const ctx: PlayContext = { ... }` literal, add after `hasZip: ...,`:

```ts
    launcher: launcherLinks(view, location.origin, token),
    desktop: prefersLauncherTab(deviceEnv()),
```

Then replace the call:

```ts
    renderLaunchers($('launcher-tabs'), launchers(ctx, lang), pack?.zipState === 'pending')
```

with:

```ts
    renderLaunchers($('launcher-tabs'), launchers(ctx, lang), pack?.zipState === 'pending', ctx.desktop !== false)
```

(c) Before `function renderLaunchers(`, add:

```ts
/** A tab's content: the Perland Launcher tab's own body, or the numbered steps. */
function tabBody(l: Launcher, zipPending: boolean): HTMLElement {
  return l.open ? openLauncher(l.open) : stepList(l, zipPending)
}

/**
 * The Perland Launcher tab: on a desktop the perland:// link as a button, on
 * a phone (no joinUrl) a line saying to open the page on a computer; then the
 * download link, always shown, since no browser can tell whether the
 * launcher is installed.
 */
function openLauncher(tab: LauncherTab): HTMLElement {
  const launch = m().launch
  const wrap = document.createElement('div')
  wrap.className = 'open-launcher'
  if (tab.joinUrl) {
    const open = document.createElement('a')
    open.className = 'btn btn-solid'
    open.href = tab.joinUrl // same tab: the browser hands perland:// to the launcher
    open.textContent = launch.openLauncher
    wrap.append(open)
  } else {
    const elsewhere = document.createElement('p')
    elsewhere.className = 'open-launcher-elsewhere'
    elsewhere.textContent = launch.onComputer
    wrap.append(elsewhere)
  }
  const get = document.createElement('p')
  get.className = 'open-launcher-get'
  const download = document.createElement('a')
  download.href = tab.downloadUrl
  download.textContent = launch.getLauncher
  download.target = '_blank'
  download.rel = 'noopener noreferrer'
  get.append(`${launch.noLauncher} `, download)
  wrap.append(get)
  return wrap
}
```

(d) In `renderLaunchers`:
- change the signature to `function renderLaunchers(root: HTMLElement, list: Launcher[], zipPending: boolean, desktop: boolean): void {`;
- replace `if (!list.some((l) => l.id === selectedLauncher)) selectedLauncher = list[0].id` with `if (!list.some((l) => l.id === selectedLauncher)) selectedLauncher = defaultLauncher(list, desktop)`;
- replace both `panel.append(stepList(list[0], zipPending))` and `panel.append(stepList(l, zipPending))` with `panel.append(tabBody(list[0], zipPending))` and `panel.append(tabBody(l, zipPending))`.

A choice the visitor made (`selectedLauncher`) still survives refreshes and language switches. The device default applies only while nothing valid is selected.

- [ ] **Step 3: Mock fixtures**

In `invite/scripts/mock-engine.mjs`:

(a) In `pack()`, add `commit: '3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a',` after `zipState: 'ready',` so every pack fixture looks like a current engine.

(b) After `const modded = ...`, add:

```js
// What an engine with LAUNCHER_DOWNLOAD_URL adds to a stack's view.
const launcher = { downloadUrl: 'https://perland.online/launcher' }
```

(c) In `fixtures`, before `network:`, add:

```js
  launcher: { ...base, game: modded, pack: pack(), launcher, team: { displayName: 'Example Team', accent: 'blue' } },
  launchervanilla: { ...base, name: 'Vanilla Friends', launcher },
  launcherspanish: { ...base, name: 'Tierras Altas', language: 'es', game: modded, pack: pack(), launcher },
```

In `invite/README.md`, add `launcher` (a pack, with Perland Launcher offered), `launchervanilla` (no pack, with the launcher) and `launcherspanish` to the fixture list after `forgenopack`. Also add a sentence to the opening paragraph: "When the engine offers Perland Launcher (`launcher` in the view, a stack only), a Perland Launcher tab comes first: on a computer it is selected and a button opens `perland://join?invite=<this page's URL>`; on a phone the usual tab stays selected and the Perland Launcher tab says to open the page on a computer instead. A link to the download is always there."

- [ ] **Step 4: Look at every state in a browser**

Run `pnpm mock` and `pnpm dev` in `invite/`, with `public/config.json` pointed at the mock (`{"engineUrl":"http://localhost:18437"}`; do not commit that change).

Open the mock's printed URLs for `launcher`, `launchervanilla`, `launcherspanish`, `packready` (no tab) and `network` (no tab). Check each against the behaviour contract.

On a desktop browser:
- the tab is first and selected;
- the button href is `perland://join?invite=http%3A%2F%2Flocalhost%3A4322%2F<that fixture's token>`;
- the download link opens in a new tab;
- EN/ES switching keeps the tab.

With phone emulation (a mobile user agent and touch, for example the browser's device mode at 390 wide; reload after switching):
- the Perland Launcher tab is listed first but Modrinth App (`launcher`, `launcherspanish`) or Minecraft (`launchervanilla`) is selected;
- opening the Perland Launcher tab shows the "on your computer" line, no button and no `perland://` link in the DOM, and the download link;
- the layout holds at 390.

These mock tokens are fakes (fixture name padded with `a`).

- [ ] **Step 5: Finish through impeccable**

Let impeccable run its finish review against the brief and apply its material fixes. Its documenter then records the tab in `invite/DESIGN.md`, including the primary-button decision.

- [ ] **Step 6: Test, check and build**

Run in `invite/`: `pnpm test`, then `pnpm check`, then `pnpm build`.

Expected: every test passes, `astro check` reports 0 errors, and the build succeeds.

- [ ] **Step 7: Screenshots for the PR (never committed)**

Use the `pr-screenshots` skill (`.claude/skills/pr-screenshots/`: `capture.mjs`, `publish.sh`) against the built page plus the mock. Capture:
- desktop and phone first views of `launcher` (the desktop with the tab selected, the phone with Modrinth App selected);
- the tab itself on desktop, and the tab on a phone with its "on your computer" line;
- `launchervanilla`;
- `launcherspanish`;
- `packready`, showing the unchanged tabs with no launcher block.

Keep the hosted image URLs for the PR body. Show the same images in the conversation. Revert `public/config.json`.

- [ ] **Step 8: Commit**

```bash
git add invite/src/app/render.ts invite/src/styles/invite.css invite/scripts/mock-engine.mjs invite/README.md invite/DESIGN.md
git commit -m "feat(invite): Perland Launcher tab

First tab when the engine offers the launcher, selected on a desktop
only: Open in Perland Launcher (perland://join?invite=<this page>) on a
desktop, an "open this page on your computer" line on a phone, and an
always-visible download link. Styled through impeccable; mock fixtures launcher, launchervanilla,
launcherspanish.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

If impeccable touched other files under `invite/` (for example `invite/.impeccable/`), add them too after reviewing the diff.

---

### Task 8: Full verification, the PR and the PR records

**Files:**
- Modify (after the PR exists): `site/src/data/changelog.yaml` (`pr:`), `site/src/data/roadmap.yaml` (link)

- [ ] **Step 1: Run the full checks**

Run from the worktree root:
- `$env:GOTMPDIR="$PWD\.gotmp"; go build ./... ; go vet ./... ; go test -p 2 ./... -count=1`
- in `invite/`: `pnpm test; pnpm check; pnpm build`
- in `site/`: `pnpm test; pnpm build`
- `node site/scripts/check-records.mjs audit` (needs `gh`), and fix in this PR what it lists (CLAUDE.md "Catch-up").

Expected: everything is green. `git status` shows no stray files: no `.gotmp/` content, no `sdk/out/`, no `invite/public/config.json` change.

- [ ] **Step 2: Codes and scopes**

No code or scope was added. Confirm with `git diff origin/main -- internal/api/response/ docs/api-catalog.json`. Expected: no changes. If anything shows there, run `go run ./cmd/api-catalog` and commit its output.

- [ ] **Step 3: Token sweep**

Run: `git diff origin/main | Select-String -Pattern '[A-Za-z0-9_-]{43}' | Select-String -NotMatch 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA|abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ|3f9a1c0e5b7d2a4f6c8e0b1d3f5a7c9e1b3d5f7a'`

Expected: only hits from the overlay's existing documentation example token, or other existing fakes. A hit that looks like a real invite token must be removed before pushing.

- [ ] **Step 4: Push and open the PR**

Run: `git push -u origin claude/join-by-link-engine`. Then open the PR with `gh pr create --base main --title "Join by link: pack.commit, LAUNCHER_DOWNLOAD_URL and the Perland Launcher tab"`. The body must include:
- a summary: `pack.commit`; `LAUNCHER_DOWNLOAD_URL` with the public and owner `launcher` blocks (stacks only); the invite page's first Perland Launcher tab;
- a line saying networks are untouched, and so are `/pack/zip` and the player-pack link;
- the test plan (the commands from Step 1);
- the screenshots from Task 7 Step 7;
- the line `Spec: perland-launcher docs/superpowers/specs/2026-10-11-join-by-link-design.md §3.1`;
- the footer `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

The project hook (`check-records.mjs hook`) lets the command through because the changelog and the roadmap are touched.

- [ ] **Step 5: Set the PR records (one more commit)**

Using the number `gh pr create` printed:
- in `site/src/data/changelog.yaml`, add `pr: <number>` under both new items;
- in `site/src/data/roadmap.yaml`, add `links:` with `- label: "#<number>"` / `href: https://github.com/PerlandDev/PerlandEngine/pull/<number>` to "Join from Perland Launcher". It stays `in-progress`, because the Panel and launcher work continue it.

Then run in `site/`: `pnpm test; pnpm build`, and:

```bash
git add site/src/data/changelog.yaml site/src/data/roadmap.yaml
git commit -m "docs(records): link the join-by-link PR

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git push
```

- [ ] **Step 6: Hand back**

Report:
- the PR URL;
- the swagger schema names the Panel plan will see (`invitedtos.InviteLauncherDTO`, `invitedtos.LauncherViewDTO`, `PackDTO.commit`);
- that the Panel plan can start once this merges and the SDK is regenerated.
