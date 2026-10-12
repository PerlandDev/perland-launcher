# Join by Link in the Launcher (Sub-project 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A player pastes a Perland invite link, or clicks **Open in Perland Launcher** on the invite page, and gets a server instance that runs exactly the modpack commit the server has deployed. Play installs or updates it and drops the player straight into that server.

**Architecture:**
- **Core (`launcher-core`):**
  - A new `invite` module parses invite links, finds the Engine through the invite site's `config.json`, and reads the public view through a 15 s shared cache. Errors name the host only.
  - A new `perland` pack source turns the view into the same `DesiredState` every other source returns:
    - modded stacks → the invite's `.mrpack`, stamped with the pinned commit;
    - stacks without a modpack → vanilla, stamped `vanilla:<version>`.
  - Sync, Plan 3b's run phases, the update banner and art keep working unchanged, because they only ever see a `DesiredState`.
  - Joining is generic: `LaunchSpec` gains an optional server address. Quick play is used where the version JSON declares it, and `--server/--port` elsewhere.
- **App (`launcher-app`):**
  - New commands: `join_preview`, `join_add`, `server_status` and `server_relink`.
  - `pack_add` hands invite links to `join_add`.
  - `play` gains `join: bool`. Plan 3b's typed deep link learns `perland://join?invite=…`.
  - Everything goes through the existing `Runner` seam, so app tests use `FakeRunner`.
- **Tauri:** forwards the commands, nothing more.
- **UI (React):**
  - a **Join server** screen, the status pill, the dead-link plate, menu and settings entries, and a rail status mark;
  - Plan 3b's Pack link tab routes invite links to Join server;
  - Plan 3b's banner gets a server variant.
  - Styling goes through `/impeccable`.

**Tech Stack:**
- Rust stable (edition 2021): tokio, serde, reqwest 0.12 (`reqwest::Url` re-exported as `launcher_core::invite::Url`), wiremock 0.6, zip, sha1/sha2.
- `launcher-app` already has `url` 2 and `base64` 0.22, from Plan 3b.
- No new crates.
- UI: React 19, TanStack Query 5, zod 4, i18next, Vitest + Testing Library.
- Fuzzing: cargo-fuzz in CI only.

**Spec:**
- `docs/superpowers/specs/2026-10-11-join-by-link-design.md`. This plan covers §2 and the launcher half of §4.
- Context:
  - `docs/superpowers/specs/2026-10-10-pack-sync-design.md` §4 (the Plan 3b decisions this plan plugs into);
  - `docs/superpowers/specs/2026-10-09-launcher-core-design.md`;
  - `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md`;
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- Plan 3b's own plan (`docs/superpowers/plans/2026-10-11-pack-sync-desktop.md`, on `main` once 3b merges) shows the code this plan extends.

## Global Constraints

- **Order:**
  - This plan runs only after Plan 3b is merged into `main`.
  - Task 0 maps every Plan 3b name the plan assumes to the merged code.
  - Never redo Plan 3b's work. Only extend it as the tasks say.
- **Crate boundaries:**
  - `launcher-core` and `launcher-app` never depend on Tauri or a UI crate.
  - `src-tauri` is wiring only and has no tests.
- **Invite link:**
  - `https://<host>[:port]/<token>[/]`, with the token matching `^[A-Za-z0-9_-]{43}$`.
  - Plain `http` is accepted only for loopback hosts.
  - Extra path segments, a query, a fragment and userinfo are refused.
  - `canonical()` is the lowercased host with no trailing slash.
  - `Debug` and `Display` print `https://<host>/<invite>`.
- **Engine routes (verified against PerlandEngine `origin/main`, `invite/src/lib/api.ts` `inviteUrl`):**
  - The Engine is `<engineUrl>` from `<invite origin>/config.json`.
  - The view is `GET <engine>/api/v1/invites/<token>`.
  - The icon is `GET <engine>/api/v1/invites/<token>/icon`, and the pack is `GET|HEAD <engine>/api/v1/invites/<token>/pack`.
  - The spec's `/{token}/icon` and `/{token}/pack` are shorthand for these.
- **Size caps:** `config.json` ≤ 4 KiB, the view ≤ 64 KiB, the icon ≤ 1 MiB (PNG signature checked).
- **URLs:** `engineUrl` must be https; http is accepted only on loopback. Redirects follow Plan 3b's policy: https→http is refused across the whole chain.
- **Cache:** each `(engine, token)` view is cached in memory for 15 s and shared by every caller.
- **Pending pack:** poll the view every 5 s for up to 3 minutes. The progress task is `server_pack_wait`. After that, `server_pack_pending`.
- **Stamps:**
  - the view's `pack.commit`;
  - else the `/pack` ETag without its quotes;
  - `vanilla:<game.version>` for a stack without a modpack.
- **Update check fallback without `commit`:** `HEAD /pack` with `If-None-Match: "<installed stamp>"`. A 304 means up to date; a 200 with a different ETag means an update is available.
- **Address:**
  - The host must be a DNS name or an IP literal, with no leading `-`, no whitespace and at most 253 characters. The port must be 1–65535.
  - A missing `address.host` falls back to the Engine URL's host.
  - Port 0 or an invalid value gives `address: None`.
- **Policy:** server instances are always `Auto`. Plan 3b's Updates control is hidden for them.
- **Live status:**
  - Poll every 30 s for the selected instance while the window is focused, and every 60 s for the rail's server instances.
  - After a 429, back off 30 s → 60 s → 120 s, and reset after a success.
- **Tokens are credentials:**
  - No `LauncherError` (message, fix, detail, params) ever contains one.
  - `Debug` of `InviteLink`, `SourceRef::Perland`, `PackInput::Perland` and `DeepLink::Join` prints `<invite>`.
  - The Redactor masks the token in game output and the log file.
  - Tests use fake tokens only: 43 × `A`, `B` or `Z`, or `b4sE64url_-b4sE64url_-b4sE64url_-b4sE64url_`.
  - The real Asalith invite never goes into code, docs, PRs, commit messages or logs.
- **Errors:**
  - There are no new categories.
  - Every new `ErrorCode` gets `errors.<code>.message` in `ui/src/i18n/en.json` and `ui/src/i18n/es.json`, plus `.fix` where the spec notes one.
- **Strings:**
  - Every UI string goes in both `en.json` and `es.json`. Spanish is neutral Latin American with informal "tú", and "enlace" for link, as the existing files use.
  - The spec's exact copy:
    - "Server is preparing the modpack…"
    - "Invite only: ask the owner to add *\<name\>*"
    - "This link is for *X*, not *Y*"
    - "This invite link stopped working · **Use a new link**"
    - "Server updated · updates when you play"
    - pill words "Online · 3/20", "Starting", "Sleeping · join to wake it", "Offline", "Unknown"
    - menu items "Play without joining", "Use a new invite link", "Copy invite link", "Copy server address"
- **Contract:**
  - `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
  - The zod schemas in `ui/src/ipc/schema.ts` are strict and must match the Rust serde shapes.
- **Design:**
  - All styling goes through `/impeccable` in the Arcade Cabinet world: one lit button, lime for the selection lamp, blue for progress, and status always in words.
  - This plan fixes structure, behaviour, strings and tests. `/impeccable` decides visuals (Task 20).
- **Heavy tests:** never run fuzzing, stress or load tests on this PC. They run in CI only. Local test inputs stay tiny; the size caps are injectable so cap tests use a few bytes.
- **License:** GPL-3.0-only. No Modrinth branding, no Mojang assets. Nothing is ported, so `NOTICE` doesn't change.
- **Commits:** every commit message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Before each commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - When `ui/` changed, also `pnpm --filter ui build`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm -r test`.
- **Machine notes:**
  - There's no Python on this PC; use Node for scripts.
  - Windows Smart App Control sometimes blocks a fresh unsigned test exe (os error 4551). Rerun.
  - The owner's terminal is PowerShell.

## Review Focus

1. **A stale UI copy of an instance saved after a relink or an Engine move.**
   - **Scenario:** Instance settings was opened before **Use a new link**, then saved.
   - **Expected:** the relink is not undone. `instance_update` keeps `source` and `server` from disk.
   - **Pinned by:** `instance_update_keeps_source_and_server_from_disk` (Task 10).
2. **An Engine view with odd values.**
   - **Inputs:** port 0 or 70000, a host with a space or a leading `-`, an unknown status word, accent `red`, missing optional fields, a huge MOTD.
   - **Expected:** the view still parses. The address is `None` (and refresh keeps the last good one), the status is `None` (shown as "Unknown"), and the accent is `other`.
   - **Pinned by:** `a_view_with_odd_values_still_parses` (Task 3), `addresses_are_validated` and `server_link_follows_the_view_and_keeps_the_last_address` (Task 4).
3. **The same invite pasted in different spellings.**
   - **Inputs:** surrounding spaces, a trailing slash, an uppercase host, `:443`.
   - **Expected:** one canonical invite. **Join server** offers **Open**, and `join_add` returns the existing instance instead of making a second one.
   - **Pinned by:** `join_add_dedupes_case_and_slash_variants` (Task 9).
4. **The owner removes the modpack (modded → vanilla) or adds one.**
   - **Expected:** the next Play's check offers `vanilla:<version>` (or the commit), and the sync that follows treats it like any pack change: the managed mods go or arrive and the loader switches (generic sync behaviour, already tested in Plan 3a).
   - **Pinned by:** `a_server_that_drops_its_modpack_offers_vanilla` (Task 5).
5. **A captive portal or proxy answers the view with HTML or JSON junk.**
   - **Expected:** `meta_unexpected` naming the host. No panic, and no token in the message or detail.
   - **Pinned by:** `a_captive_portal_page_is_unexpected_data` (Task 3) and the token sweep (Task 7).

## File Structure

| File | Responsibility |
|---|---|
| `crates/launcher-core/src/error.rs` | 8 new `ErrorCode`s |
| `crates/launcher-core/src/invite/mod.rs` (new) | module root; `Url` re-export |
| `crates/launcher-core/src/invite/link.rs` (new) | `InviteLink`, `shown_invite`, the shared `invite_link_cases()` table |
| `crates/launcher-core/src/invite/view.rs` (new) | serde mirror of the Engine's `InviteViewDTO` |
| `crates/launcher-core/src/invite/client.rs` (new) | `InviteClient`: `config.json` discovery, cached view, icon, error mapping |
| `crates/launcher-core/src/instances.rs` | `ServerStatus`, `ServerAddress`, `ServerLink`, `Instance.server`, `SourceRef::Perland` + token-free `Debug` |
| `crates/launcher-core/src/sources/perland.rs` (new) | `open`, `resolve`, `check`, the pending poll, stamps, error rewrapping |
| `crates/launcher-core/src/sources/mod.rs` | `PackInput::Perland`, `Sources.invites`, dispatch arms, `resolve_with` |
| `crates/launcher-core/src/sync/mod.rs` | `for_installer` wiring; Plan 3b's `find_update`/`ensure_locked` pass progress to `resolve_with` |
| `crates/launcher-core/src/launch/mod.rs` | `LaunchSpec.join`: quick play or `--server/--port` |
| `crates/launcher-core/src/play.rs` | `StartSpec.join`, `redactor_for` (invite token) |
| `crates/launcher-core/tests/invite.rs` (new), `tests/common/mod.rs` | wiremock tests, the token sweep |
| `crates/perland-cli/src/main.rs` | `StartSpec.join: None`; `pack add` of a server ensures instead of syncing the view-only state |
| `crates/launcher-app/src/servers.rs` (new) | `join_preview`, `join_add`, `server_status`, `server_relink`, `ServerLink` refresh |
| `crates/launcher-app/src/dto.rs` | `JoinPreview`, `ServerStatusView` and parts |
| `crates/launcher-app/src/runner.rs` | `invite_view`, `invite_icon`, `check_update`; `find_update` and `start` gain a sink and `join` |
| `crates/launcher-app/src/runs.rs` | `play_with(id, join)`, join target, `invite_gone` fallbacks |
| `crates/launcher-app/src/packs.rs` | `pack_add` delegation; `pack_status` server arm |
| `crates/launcher-app/src/deep_link.rs` | typed `DeepLink::{Pack, Join}` |
| `crates/launcher-app/src/app.rs` | `instance_update` keeps `source`/`server`, locks Auto; `pending_link: Option<DeepLink>` |
| `crates/launcher-app/src/testing.rs` | `FakeRunner` invite fields; server fixtures |
| `crates/launcher-app/tests/contract.rs` | new fixtures, `invite_links.json` |
| `src-tauri/src/{commands,main}.rs` | the four commands, `play(join)`, typed `deep_link_take` |
| `ui/src/ipc/{schema,commands}.ts`, `ui/src/ipc/contract.test.ts` | schemas and commands |
| `ui/src/ipc/inviteLink.ts` (new) | `inviteCanonical` / `isInviteLink`, the TS mirror of the Rust rule |
| `ui/src/ipc/mockServers.ts` (new), `ui/src/ipc/mock.ts`, `ui/src/main.tsx` | mock server scenarios |
| `ui/src/state/{route,deepLink,queries}.ts`, `ui/src/state/serverPoll.ts` (new) | `joinServer` route, typed deep links, status polling with backoff |
| `ui/src/features/join/{JoinServer,ServerCard}.tsx` (new) | the Join server screen |
| `ui/src/features/servers/{StatusPill,Motd,AddressLine,DeadLinkPlate,ServerPanel}.tsx`, `motd.ts` (new) | server instance view parts |
| `ui/src/features/home/{Home,InstanceMenu,UpdatePlate,labels}.ts(x)`, `ui/src/ui/PlayButton.tsx` | wiring, menu items, banner variant, wait label |
| `ui/src/features/instance-settings/InstanceSettings.tsx` | Server section; Updates hidden |
| `ui/src/features/new-instance/PackSource.tsx` | invite links route to Join server |
| `ui/src/features/rail/Rail.tsx` | server status mark |
| `ui/src/App.tsx` | `joinServer` screen |
| `ui/src/i18n/{en,es}.json` | every string |
| `fuzz/Cargo.toml`, `fuzz/fuzz_targets/{invite_link,join_deep_link}.rs` (new), `.github/workflows/nightly.yml` | CI-only fuzzing |

---

### Task 0: Rebase onto `main` with Plan 3b merged; reconcile names

This plan was written while Plan 3b was in flight in another session (its worktree `eloquent-morse-cc958c`), so 3b's names are taken from:
- the pack-sync spec §4;
- 3b's plan `docs/superpowers/plans/2026-10-11-pack-sync-desktop.md`;
- the commits 3b had made by 2026-10-11 (`find_update`, `ensure_locked`, `Cancel`, error codes, the chain redirect policy).

The merged code is the truth. This task makes the rest of the plan match it before any code is written.

**Files:** none (the plan itself, if names differ).

- [ ] **Step 1: Rebase onto `main` and confirm 3b is in**

```bash
git fetch origin
git rebase origin/main
git log --oneline origin/main | head -20
```

Expected: 3b's merge commit is in the log, and the rebase is clean (this branch only adds the spec and this plan).

- [ ] **Step 2: Baseline**

```bash
cargo test --workspace
pnpm install
pnpm -r test
```

Expected: all green. If not, stop and report: this plan doesn't start on a red `main`.

- [ ] **Step 3: Check every assumed 3b name**

For each row, find the merged name with `git grep`. Then do one of two things:
- Tick it if it matches.
- Otherwise write the real name into the "Merged as" column and replace the assumed name in every later task of this plan (search the plan text) before starting Task 1.

| # | Assumed (3b) | Where | Used by | Merged as |
|---|---|---|---|---|
| 1 | `Syncer::find_update(&self, store: &InstanceStore, instance: &Instance) -> Result<Option<PendingUpdate>>` | `launcher-core/src/sync/mod.rs` | 6 | ✓ matches (`sync/mod.rs:307`). Pinned and never-synced return `Ok(None)`; an `UpdateCheck::Unavailable` is the `Err`; resolving the update is wrapped in `update_check_failed` |
| 2 | `Syncer::ensure_locked(&self, _lock: &InstanceLock, store, id: &str, accept: Option<PendingUpdate>, sink: &EventSink, cancel: &Cancel) -> Result<Ensured>`; its never-synced branch calls `self.sources.resolve(&instance.source)` | same | 6 | ✓ matches (`sync/mod.rs:372`), argument order included. The never-synced branch is exactly `let desired = self.sources.resolve(&instance.source).await?;` |
| 3 | `PendingUpdate { source, desired, changes, kept }`, `Cancel`, `Ensured { outcome, warning }` | same | 6, 11 | ✓ matches. `PendingUpdate` derives `Debug, Clone, PartialEq`; `changes` is `sync::plan::Changes`; `Cancel` is `Clone + Default` with `cancel()`/`is_cancelled()` |
| 4 | `ErrorCode::{SyncCancelled, UpdateDownloadFailed, ModManaged}` exist | `launcher-core/src/error.rs` | 1 | ✓ matches; all three are in the `// packs and sync` group, before `// anything unexpected` |
| 5 | `http::allow_redirect_chain` (10 hops; https→http refused across the chain) | `launcher-core/src/http.rs` | 3 | ✓ matches (`MAX_REDIRECTS = 10`); `http::client(launcher_version)` installs it |
| 6 | `RunStateKind::{Checking, Asking, Syncing}`; `ExitInfo { synced, manual, update_only }`; `AppEvent::RunState { …, update }` | `launcher-app/src/events.rs` | 11, 14 | ✓ matches |
| 7 | `AppEvent::DeepLink` is a **unit** variant, `{"type":"deep_link"}`, meaning "call `deep_link_take`". The pack-sync spec §4.4 wrote `AppEvent::DeepLink { url }`; 3b's plan made it a unit. | same | 12, 15 | ✓ matches: a unit variant serializing as `{"type":"deep_link"}` |
| 8 | `trait Runner { fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>>; fn find_update(&self, instance: Instance) -> BoxFuture<Result<Option<PendingUpdate>>>; fn sync(…); fn import_manual(…); fn prepare(…); fn start(&self, instance, prepared, session, sink) }` | `launcher-app/src/runner.rs` | 9, 11 | ✓ matches. Full shapes: `type Prepared`; `prepare(&self, instance: Instance, sink: EventSink)`; `sync(&self, lock: Arc<InstanceLock>, id: String, accept: Option<PendingUpdate>, cancel: Cancel, sink: EventSink) -> BoxFuture<Result<Ensured>>`; `import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>>` |
| 9 | `CoreRunner { installer, syncer: Arc<Syncer>, launcher_version }`, `CoreRunner::new(installer, syncer: Syncer, launcher_version)` | same | 9 | ✓ matches (fields private; `installer()` accessor) |
| 10 | `FakeRunner` pub fields `instances_dir, added, updates, checks, sync_results, syncs, hold_sync, sync_game, imported, prepared`; helpers `up_to_date()`, `desired(version, files)`, `pending(from, to)`, `pack_instance(core, name, installed)` | `launcher-app/src/testing.rs` | 9, 11 | ✓ matches. `add_pack` creates every instance as `"Sky Islands"` and maps `PackInput` with an exhaustive `match`; `testing` is a `#[cfg(test)] pub(crate) mod` |
| 11 | `AppCore::{pack_add(PackAddRequest), pack_status(id), pack_update(id), play_answer, deep_link(raw), deep_link_take()}`; `pack_instance(id)`; `checks_for_updates(&Instance)`; `start_run(id, instance, play: Option<(Session, bool)>, checks)`; `drive(id, instance, play, checks, cancel)`; `decide_update(id, &Instance, update_only)` | `launcher-app/src/{packs,runs}.rs` | 9, 11, 12 | ✓ matches. `drive` takes `self: Arc<Self>`; `decide_update` returns `Option<PendingUpdate>` (a failed check warns, then `None`); `play` ends with `self.start_run(id, instance, Some((session, minimize)), checks)`, and `drive`'s update-only branch is `let Some((session, minimize)) = play else { … };` |
| 12 | `AppCore.pending_link: Mutex<Option<String>>`; `launcher_app::deep_link::parse(raw) -> Result<String>`; private `invalid()` there | `launcher-app/src/{app,deep_link}.rs` | 12 | ✓ matches. `packs.rs` has its own private `link_invalid()` for `pack_add` |
| 13 | `dto::{PackAddRequest::{Link{url}, File{path}}, PackStatus{installed_version, update, check_error}, PackUpdate{from_version, to_version, added, updated, removed, kept}}` | `launcher-app/src/dto.rs` | 9, 11 | ✓ matches |
| 14 | Tauri: a `deep_link_take` command returning `Option<String>`; `core.deep_link(url)` called from `on_open_url` / `get_current` | `src-tauri/src/{commands,main}.rs` | 13 | ✓ matches; `deep_link` is already `pub mod` in `launcher-app/src/lib.rs` |
| 15 | UI: `commands.{packAdd, packStatus, packUpdate, deepLinkTake}`; `Route` member `{ name: 'newInstance'; link?: string }`; `state/deepLink.ts` `takeDeepLink()`; `handleEvent` `deep_link` branch gated on `firstRunDone`; `qk.packStatus(id)`; `usePackStatus(id, enabled)`; `usePackArt(id, enabled)` | `ui/src/{ipc,state}` | 14, 15 | ✓ matches (`qk.packStatus(id)` is `['pack', 'status', id]`) |
| 16 | UI: `isPack(instance)` in `features/home/labels.ts`; `UpdatePlate` in `features/home/UpdatePlate.tsx`; `PackLinkForm` in `features/new-instance/PackSource.tsx` (its `submit` calls `add.mutate({ kind: 'link', url })`); the Updates fieldset in `InstanceSettings.tsx` rendered under `pack && (…)`; `RailItem` with a `pack` prop; `PlayButton` labels `t('play.checking')` while `checking` | `ui/src/features`, `ui/src/ui` | 17–19 | ✓ matches. `submit` calls `add.mutate({ kind: 'link', url: url.trim() }, { onSuccess })`; `PlayButton` labels by `state === 'checking'` (a `state` prop, no `checking` prop); `RailItem`'s run LED is `.rail-led` |
| 17 | UI mock: `MockOptions` flags, the `sky-islands` pack instance, `exitOf(…)` exported from `ui/src/ipc/mock.ts`, and a `handle(state, cmd, args)` switch with a `default:` branch | `ui/src/ipc/mock.ts` | 16 | ✓ matches. `MockRun` is `{ state, cancelled, updateOnly, answer }`; the play script is `scriptRun(state, instance, updateOnly = false)` (there is no `scriptPlay`); `MockOptions.link` seeds `state.pendingLink`, a string that `deep_link_take` hands back as is |
| 18 | i18n sections `update.*` and `play.checking` exist | `ui/src/i18n/*.json` | 18 | ✓ matches (`update.*` incl. `kept_one`/`kept_other`; `play.checking` in both files) |

- [ ] **Step 4: Stop and ask if 3b's design differs materially**

Stop, and ask the coordinator/owner before continuing, if any of these is true. Name the difference.
- `find_update` or `ensure_locked` is missing, or the run doesn't hold `InstanceLock` from `checking` to game exit.
- A failed update check doesn't fall back to the installed version.
- Deep links don't go through a take-once `deep_link_take`, or Plan 3b has no Pack link tab or no banner.
- `UpdatePolicy` is no longer `Auto | Ask | Pinned`, or `Instance` gained a field that conflicts with `server`.
- `SourceRef` or `PackInput` changed shape (e.g. `PackInput::parse` no longer exists).

Renames alone are not a reason to stop: fix the plan text and go on.

- [ ] **Step 5: Commit only if the plan changed**

```bash
git add docs/superpowers/plans/2026-10-11-join-by-link-launcher.md
git commit -m "docs(plan): join by link, names reconciled with merged Plan 3b

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 1: Error codes and their strings

The codes come first: every later task returns them, and the i18n test fails the moment a code exists without strings.

**Files:**
- Modify: `crates/launcher-core/src/error.rs`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (`errors`)
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Produces `ErrorCode::{InviteLinkInvalid, InviteGone, InviteUnsupported, InviteSiteUnreachable, InviteRateLimited, ServerPackUnavailable, ServerPackPending, ServerAddressUnknown}`. They serialize as `invite_link_invalid`, `invite_gone`, `invite_unsupported`, `invite_site_unreachable`, `invite_rate_limited`, `server_pack_unavailable`, `server_pack_pending` and `server_address_unknown`.
- Categories are chosen where each error is built (later tasks): Pack, Pack, Pack, Network, Network, Pack, Pack, Game.

- [ ] **Step 1: Write the failing test**

In `crates/launcher-core/src/error.rs` `mod tests`, add:

```rust
    #[test]
    fn invite_codes_exist() {
        for (code, wire) in [
            (ErrorCode::InviteLinkInvalid, "invite_link_invalid"),
            (ErrorCode::InviteGone, "invite_gone"),
            (ErrorCode::InviteUnsupported, "invite_unsupported"),
            (ErrorCode::InviteSiteUnreachable, "invite_site_unreachable"),
            (ErrorCode::InviteRateLimited, "invite_rate_limited"),
            (ErrorCode::ServerPackUnavailable, "server_pack_unavailable"),
            (ErrorCode::ServerPackPending, "server_pack_pending"),
            (ErrorCode::ServerAddressUnknown, "server_address_unknown"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p launcher-core --lib error::tests::invite_codes_exist`
Expected: compile error, `no variant named InviteLinkInvalid`.

- [ ] **Step 3: Add the codes**

In the `error_codes!` invocation, add a group just before `// anything unexpected`:

```rust
    // server invites (join by link)
    InviteLinkInvalid, InviteGone, InviteUnsupported, InviteSiteUnreachable, InviteRateLimited,
    ServerPackUnavailable, ServerPackPending, ServerAddressUnknown,
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p launcher-core --lib error::`
Expected: PASS.

- [ ] **Step 5: Add the strings**

In `ui/src/i18n/en.json`, inside `"errors"`, add:

```json
    "invite_link_invalid": {
      "message": "That isn't a Perland invite link.",
      "fix": "Invite links look like https://play.example.com/ followed by a long code."
    },
    "invite_gone": {
      "message": "This invite link doesn't work anymore. Ask the owner for a new one.",
      "fix": "Use a new invite link."
    },
    "invite_unsupported": {
      "message": "Networks can't be joined from the launcher yet. Join with the address {{address}}."
    },
    "invite_site_unreachable": {
      "message": "Couldn't reach the server's Perland site at {{host}}.",
      "fix": "Check the link, or try again later."
    },
    "invite_rate_limited": {
      "message": "Too many requests. Try again in a minute."
    },
    "server_pack_unavailable": {
      "message": "The server's modpack isn't available right now.",
      "fix": "Try again later, or let the server's owner know."
    },
    "server_pack_pending": {
      "message": "The server is still preparing the modpack. Try again in a minute."
    },
    "server_address_unknown": {
      "message": "Couldn't get the server address, so the game starts at the title screen."
    }
```

In `ui/src/i18n/es.json`, inside `"errors"`, add:

```json
    "invite_link_invalid": {
      "message": "Eso no es un enlace de invitación de Perland.",
      "fix": "Los enlaces de invitación se ven como https://play.example.com/ seguido de un código largo."
    },
    "invite_gone": {
      "message": "Este enlace de invitación ya no funciona. Pídele uno nuevo al dueño.",
      "fix": "Usa un nuevo enlace de invitación."
    },
    "invite_unsupported": {
      "message": "Todavía no puedes unirte a redes desde el launcher. Únete con la dirección {{address}}."
    },
    "invite_site_unreachable": {
      "message": "No se pudo conectar con el sitio de Perland del servidor en {{host}}.",
      "fix": "Revisa el enlace o inténtalo más tarde."
    },
    "invite_rate_limited": {
      "message": "Demasiadas solicitudes. Inténtalo de nuevo en un minuto."
    },
    "server_pack_unavailable": {
      "message": "El modpack del servidor no está disponible ahora.",
      "fix": "Inténtalo más tarde o avísale al dueño del servidor."
    },
    "server_pack_pending": {
      "message": "El servidor todavía está preparando el modpack. Inténtalo de nuevo en un minuto."
    },
    "server_address_unknown": {
      "message": "No se pudo obtener la dirección del servidor, así que el juego empieza en la pantalla de título."
    }
```

- [ ] **Step 6: Regenerate the contract and run the UI checks**

```bash
UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract
cargo test -p launcher-app --test contract
pnpm --filter ui test -- i18n contract
```

Expected: `error_codes.json` gains the eight codes, and the i18n test ("translate every error code") passes.

- [ ] **Step 7: Commit**

```bash
git add crates/launcher-core/src/error.rs ui/src/i18n ui/src/ipc/contract/error_codes.json
git commit -m "feat(core): error codes for server invites, in English and Spanish

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `InviteLink` and the shared link table

**Files:**
- Create: `crates/launcher-core/src/invite/mod.rs`, `crates/launcher-core/src/invite/link.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod invite;`)

**Interfaces:**
- Produces in `launcher_core::invite`:
  - `InviteLink::parse(&str) -> Result<InviteLink>` (`invite_link_invalid` on refusal).
  - Accessors: `.origin() -> String`, `.host() -> &str`, `.canonical() -> String`, `.token() -> &str`, `.shown() -> String`.
  - `Debug`/`Display` print `shown()`. `Clone`, `PartialEq`, `Eq` and `Hash` are derived.
  - `pub const TOKEN_LEN: usize = 43`.
  - `pub fn shown_invite(invite: &str) -> String`: any stored invite string without its token, or `<invite>`.
  - `pub struct InviteLinkCase { pub input: String, pub canonical: Option<String> }` (`Serialize`) and `pub fn invite_link_cases() -> Vec<InviteLinkCase>`. Task 14 writes this table to `invite_links.json` and Task 15's TypeScript runs it.
  - `pub type Url = reqwest::Url;`, so `launcher-app` can name Engine URLs without depending on reqwest.

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-core/src/invite/mod.rs`:

```rust
//! Perland server invites (join-by-link spec §2.1): the link, the Engine's public view of it,
//! and the client that reads that view. Invite tokens are credentials: nothing here prints one.
pub mod link;

pub use link::{invite_link_cases, shown_invite, InviteLink, InviteLinkCase, TOKEN_LEN};

/// Engine and invite URLs, re-exported so the app can name them without depending on reqwest.
pub type Url = reqwest::Url;
```

In `crates/launcher-core/src/lib.rs`, add `pub mod invite;` after `pub mod instances;`.

Create `crates/launcher-core/src/invite/link.rs` with only the tests for now:

```rust
//! Invite links: `https://<invite host>/<43-char token>`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[test]
    fn the_shared_table_holds() {
        let cases = invite_link_cases();
        assert!(cases.iter().any(|c| c.canonical.is_some()));
        assert!(cases.iter().any(|c| c.canonical.is_none()));
        for case in cases {
            let got = InviteLink::parse(&case.input).ok().map(|l| l.canonical());
            assert_eq!(got, case.canonical, "{:?}", case.input);
        }
    }

    #[test]
    fn refusals_are_invite_link_invalid() {
        assert_eq!(
            InviteLink::parse("nope").unwrap_err().code,
            ErrorCode::InviteLinkInvalid
        );
    }

    #[test]
    fn debug_and_display_never_print_the_token() {
        let token = "A".repeat(TOKEN_LEN);
        let link = InviteLink::parse(&format!("https://play.example.com/{token}")).unwrap();
        for text in [
            format!("{link:?}"),
            link.to_string(),
            link.shown(),
            shown_invite(&link.canonical()),
        ] {
            assert!(!text.contains(&token), "{text}");
            assert!(text.contains("play.example.com"), "{text}");
        }
        assert_eq!(link.token(), token);
        assert_eq!(link.origin(), "https://play.example.com");
        assert_eq!(link.host(), "play.example.com");
        assert_eq!(shown_invite("junk"), "<invite>");
    }

    #[test]
    fn spellings_of_one_link_share_a_canonical_form() {
        let token = "A".repeat(TOKEN_LEN);
        let a = InviteLink::parse(&format!("https://PLAY.example.com/{token}/")).unwrap();
        let b = InviteLink::parse(&format!(" https://play.example.com:443/{token} ")).unwrap();
        assert_eq!(a.canonical(), b.canonical());
        assert_eq!(a, b);
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib invite::`
Expected: compile errors (`InviteLink`, `invite_link_cases` don't exist).

- [ ] **Step 3: Implement**

Put this above the tests in `link.rs`:

```rust
use std::fmt;

use serde::Serialize;

use crate::error::{ErrorCode, LauncherError, Result};

/// Length of an invite token: 32 random bytes in unpadded base64url.
pub const TOKEN_LEN: usize = 43;

/// Longest input `parse` looks at; anything longer can't be an invite link.
const MAX_INPUT: usize = 512;

/// A parsed invite link. The token is a credential: `Debug` and `Display` print
/// `https://<host>/<invite>` instead.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct InviteLink {
    scheme: &'static str,
    /// Lowercase (as `Url` normalizes it); an IPv6 literal keeps its brackets.
    host: String,
    /// `None` for the scheme's default port.
    port: Option<u16>,
    token: String,
}

pub(crate) fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteLinkInvalid,
        "That isn't a Perland invite link",
    )
    .with_fix("Invite links look like https://play.example.com/ followed by a long code")
}

fn valid_token(s: &str) -> bool {
    s.len() == TOKEN_LEN
        && s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl InviteLink {
    /// `https://<host>[:port]/<token>[/]`, or `http://` to this computer only. Extra path
    /// segments, a query, a fragment and user info are refused.
    pub fn parse(input: &str) -> Result<InviteLink> {
        let raw = input.trim();
        // Checked on the raw text: `Url` hides an empty `?` or `#` and turns `\` into `/`.
        if raw.is_empty()
            || raw.len() > MAX_INPUT
            || raw.chars().any(|c| {
                c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '@' | '\\')
            })
        {
            return Err(link_invalid());
        }
        let url = reqwest::Url::parse(raw).map_err(|_| link_invalid())?;
        let scheme = match url.scheme() {
            "https" => "https",
            "http" if crate::http::is_loopback(&url) => "http",
            _ => return Err(link_invalid()),
        };
        let host = url
            .host_str()
            .filter(|h| !h.is_empty())
            .ok_or_else(link_invalid)?
            .to_ascii_lowercase();
        let token = url
            .path()
            .strip_prefix('/')
            .map(|p| p.strip_suffix('/').unwrap_or(p))
            .filter(|t| valid_token(t))
            .ok_or_else(link_invalid)?
            .to_string();
        Ok(InviteLink {
            scheme,
            host,
            port: url.port(),
            token,
        })
    }

    fn port_suffix(&self) -> String {
        self.port.map(|p| format!(":{p}")).unwrap_or_default()
    }

    /// `https://play.example.com[:port]`: where the invite site's `config.json` lives.
    pub fn origin(&self) -> String {
        format!("{}://{}{}", self.scheme, self.host, self.port_suffix())
    }

    /// The host alone, for messages ("From play.example.com").
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The identity instances are compared by: lowercase host, no trailing slash.
    pub fn canonical(&self) -> String {
        format!("{}/{}", self.origin(), self.token)
    }

    /// The credential. Only for request URLs and the redactor; never log it.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// `https://play.example.com/<invite>`: safe for logs and errors.
    pub fn shown(&self) -> String {
        format!("{}/<invite>", self.origin())
    }
}

impl fmt::Debug for InviteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InviteLink({})", self.shown())
    }
}

impl fmt::Display for InviteLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.shown())
    }
}

/// A stored invite URL without its token, for `Debug` of types that keep it as a string.
pub fn shown_invite(invite: &str) -> String {
    InviteLink::parse(invite)
        .map(|l| l.shown())
        .unwrap_or_else(|_| "<invite>".into())
}

/// One row of the shared link table. `canonical` is `None` for input that must be refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InviteLinkCase {
    pub input: String,
    pub canonical: Option<String>,
}

/// The invite-link rule as data. The Rust tests and the UI's `isInviteLink` both run this
/// table; the contract test writes it to `ui/src/ipc/contract/invite_links.json`. Fake tokens
/// only.
pub fn invite_link_cases() -> Vec<InviteLinkCase> {
    let a = "A".repeat(TOKEN_LEN);
    let a42 = &a[..42];
    let mixed = "b4sE64url_-b4sE64url_-b4sE64url_-b4sE64url_";
    let ok = |input: String, canonical: String| InviteLinkCase {
        input,
        canonical: Some(canonical),
    };
    let bad = |input: String| InviteLinkCase {
        input,
        canonical: None,
    };
    vec![
        ok(
            format!("https://play.example.com/{a}"),
            format!("https://play.example.com/{a}"),
        ),
        ok(
            format!("https://play.example.com/{a}/"),
            format!("https://play.example.com/{a}"),
        ),
        ok(
            format!("  https://PLAY.Example.COM/{mixed}  "),
            format!("https://play.example.com/{mixed}"),
        ),
        ok(
            format!("https://play.example.com:8443/{a}"),
            format!("https://play.example.com:8443/{a}"),
        ),
        ok(
            format!("https://play.example.com:443/{a}"),
            format!("https://play.example.com/{a}"),
        ),
        ok(
            format!("http://127.0.0.1:5173/{a}"),
            format!("http://127.0.0.1:5173/{a}"),
        ),
        ok(
            format!("http://localhost:5173/{a}"),
            format!("http://localhost:5173/{a}"),
        ),
        ok(
            format!("http://[::1]:5173/{a}"),
            format!("http://[::1]:5173/{a}"),
        ),
        bad(format!("http://play.example.com/{a}")),
        bad(format!("http://10.0.0.1/{a}")),
        bad(format!("ftp://play.example.com/{a}")),
        bad(format!("https://play.example.com/{a42}")),
        bad(format!("https://play.example.com/{a}A")),
        bad(format!("https://play.example.com/{a42}=")),
        bad(format!("https://play.example.com/{}%41", &a[..40])),
        bad(format!("https://play.example.com/{a}?ref=discord")),
        bad(format!("https://play.example.com/{a}?")),
        bad(format!("https://play.example.com/{a}#top")),
        bad(format!("https://play.example.com/{a}#")),
        bad(format!("https://player@play.example.com/{a}")),
        bad(format!("https://player:pw@play.example.com/{a}")),
        bad(format!("https://play.example.com/join/{a}")),
        bad(format!("https://play.example.com/{a}/pack")),
        bad(format!("https://play.example.com/{a}//")),
        bad(format!("https://play.example.com\\{a}")),
        bad(format!("https://play.example.com/{} {}", &a[..21], &a[..21])),
        bad(format!("play.example.com/{a}")),
        bad(format!(
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2F{a}"
        )),
        bad("https://perlanddev.github.io/asalith-modpack/pack.toml".into()),
        bad("https://play.example.com/".into()),
        bad(String::new()),
    ]
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-core --lib invite::`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/invite crates/launcher-core/src/lib.rs
git commit -m "feat(core): invite links: strict parsing, canonical form, token-free Debug, shared table

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `InviteClient`: Engine discovery, the shared view, the icon, error mapping

**Files:**
- Create: `crates/launcher-core/src/invite/view.rs`, `crates/launcher-core/src/invite/client.rs`
- Modify: `crates/launcher-core/src/invite/mod.rs`
- Modify: `crates/launcher-core/src/instances.rs` (`ServerStatus` only; the rest of the server types come in Task 4)
- Create: `crates/launcher-core/tests/invite.rs`

**Interfaces:**
- Consumes `InviteLink` (Task 2) and the error codes (Task 1).
- Produces in `launcher_core::instances`:
  - `enum ServerStatus { Online, Starting, Sleeping, Offline }` (serde lowercase).
  - `ServerStatus::parse(&str) -> Option<ServerStatus>`.
- Produces in `launcher_core::invite`:
  - **View types:** `InviteView`, `InviteAddress`, `InviteGame`, `InvitePlayers`, `InvitePack`, `InviteTeam`, `InviteLauncher` (fields below).
  - `InviteView::status() -> Option<ServerStatus>` and `InviteView::is_network() -> bool`.
  - `pub fn non_empty(&Option<String>) -> Option<&str>`.
  - Pack state constants: `PACK_READY`, `PACK_PENDING`, `PACK_UNAVAILABLE`.
  - `pub struct EngineView { pub engine: Url, pub view: InviteView }`.
  - Tuning: `pub struct InviteTiming { pub view_ttl, pub pack_poll_every, pub pack_poll_for }` (defaults 15 s, 5 s and 180 s) and `pub struct InviteLimits { pub config, pub view, pub icon, pub error_body }` (defaults 4 KiB, 64 KiB, 1 MiB and 4 KiB).
  - **`InviteClient`** (`Clone`; clones share one cache):
    - construction: `new(http)`, `with_timing`, `with_limits`, `timing()`, `http()`;
    - `remember_engine(&InviteLink, &str)`;
    - `async engine_for(&InviteLink) -> Result<Url>`;
    - `async view(&InviteLink) -> Result<EngineView>` and `async view_fresh(&InviteLink) -> Result<EngineView>`;
    - `async icon(&InviteLink) -> Option<Vec<u8>>`;
    - `async pack_url(&InviteLink) -> Result<Url>`;
    - `pub(crate) async error_code(Url) -> Option<String>`.
  - Free functions:
    - `pub fn check_engine_url(&str) -> Option<Url>` and `pub fn invite_route(&Url, &InviteLink, suffix: &str) -> Url`;
    - `pub(crate) fn status_error(StatusCode, host) -> Option<LauncherError>`;
    - `pub(crate) fn offline(host, detail)`, `gone(host)`, `rate_limited(host)` and `host_trouble(host, status)`.
- Error mapping (host only, never a URL):

| Answer | Code |
|---|---|
| 404 on the view | `invite_gone` (Pack, fix "Use a new invite link") |
| 429 | `invite_rate_limited` (Network) |
| any other non-2xx | `host_trouble` with `host` and `status` |
| `config.json` missing, unparsable, too large, or its `engineUrl` missing or not https | `invite_site_unreachable` (Network, `host` param) |
| no connection | `offline` (Network, `host` param), and the cached Engine for that site is forgotten |
| a body that isn't the envelope, or is too large | `meta_unexpected` naming the host |

The icon is optional: a 404 there means "no icon", so `icon()` returns `None` and never errors.

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-core/tests/invite.rs`:

```rust
//! Server invites against a wiremock Engine. The mock server is both the invite site
//! (`/config.json`) and the Engine (`/api/v1/invites/…`). Fake tokens only.
mod common;

use std::time::Duration;

use common::hits;
use launcher_core::{
    invite::{InviteClient, InviteLimits, InviteLink, InviteTiming},
    ErrorCategory, ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn invite_url(server: &MockServer) -> String {
    format!("{}/{TOKEN}", server.uri())
}

fn link(server: &MockServer) -> InviteLink {
    InviteLink::parse(&invite_url(server)).unwrap()
}

fn route(suffix: &str) -> String {
    format!("/api/v1/invites/{TOKEN}{suffix}")
}

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn failure(code: Option<&str>) -> Value {
    json!({
        "success": false,
        "data": code.map(|c| json!({ "code": c })),
        "error": "failed",
        "meta": { "requestId": "r" }
    })
}

fn modded_view(commit: Option<&str>, state: &str) -> Value {
    json!({
        "kind": "stack", "name": "survival", "motd": "§6Survival§r\nwith friends",
        "address": { "host": "play.example.com", "port": 25566, "display": "play.example.com:25566" },
        "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric", "loaderVersion": "0.16.14" },
        "status": "online", "players": { "online": 3, "max": 20 }, "whitelisted": false,
        "note": "Be nice.", "language": "en",
        "pack": {
            "name": "Survival Pack", "version": "1.0.4", "minecraft": "1.20.1",
            "loader": "fabric", "loaderVersion": "0.16.14", "modCount": 2,
            "state": state, "zipState": "ready", "commit": commit
        },
        "team": { "displayName": "Acme", "accent": "violet" }
    })
}

fn vanilla_view(version: &str) -> Value {
    json!({
        "kind": "stack", "name": "creative", "motd": "Build",
        "address": { "host": "play.example.com", "port": 25570, "display": "play.example.com:25570" },
        "game": { "type": "vanilla", "version": version },
        "status": "sleeping", "whitelisted": true, "language": "en",
        "pack": null, "team": { "displayName": "", "accent": "" }
    })
}

fn network_view() -> Value {
    json!({
        "kind": "network", "name": "hub", "motd": "Hub",
        "address": { "host": "play.example.com", "port": 25577, "display": "play.example.com:25577" },
        "game": { "type": "velocity" }, "status": "online", "players": { "online": 9 },
        "whitelisted": false, "language": "en", "pack": null,
        "team": { "displayName": "Acme", "accent": "cyan" }
    })
}

async fn mount_config(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })))
        .mount(server)
        .await;
}

async fn mount_view(server: &MockServer, view: Value) {
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(view)))
        .mount(server)
        .await;
}

fn fast() -> InviteTiming {
    InviteTiming {
        view_ttl: Duration::from_secs(15),
        pack_poll_every: Duration::from_millis(10),
        pack_poll_for: Duration::from_millis(300),
    }
}

fn client() -> InviteClient {
    InviteClient::new(launcher_core::http::client("test").unwrap()).with_timing(fast())
}

#[tokio::test]
async fn config_json_finds_the_engine_and_the_view_maps() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c0ffee"), "ready")).await;
    let client = client();
    let ev = client.view(&link(&server)).await.unwrap();
    assert_eq!(ev.engine.as_str().trim_end_matches('/'), server.uri());
    let view = &ev.view;
    assert_eq!(view.name, "survival");
    assert_eq!(view.address.port, 25566);
    assert_eq!(view.players.as_ref().unwrap().max, Some(20));
    let pack = view.pack.as_ref().unwrap();
    assert_eq!(pack.commit.as_deref(), Some("c0ffee"));
    assert_eq!(pack.mod_count, 2);
    assert_eq!(view.team.accent, "violet");
    assert_eq!(view.status(), Some(launcher_core::instances::ServerStatus::Online));
    client.view(&link(&server)).await.unwrap();
    assert_eq!(hits(&server, "/config.json").await, 1, "the Engine is cached per site");
    assert_eq!(hits(&server, &route("")).await, 1, "the view is shared for 15 s");
}

#[tokio::test]
async fn a_site_without_a_usable_config_is_unreachable() {
    let bodies: [Option<&str>; 5] = [
        None,
        Some("not json"),
        Some(r#"{"engineUrl":"http://engine.example.com"}"#),
        Some(r#"{"engineUrl":"https://user:pw@engine.example.com"}"#),
        Some("{}"),
    ];
    for body in bodies {
        let server = MockServer::start().await;
        if let Some(body) = body {
            Mock::given(method("GET"))
                .and(path("/config.json"))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;
        }
        let err = client().engine_for(&link(&server)).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteSiteUnreachable, "{body:?}");
        assert_eq!(err.category, ErrorCategory::Network);
        assert_eq!(err.params["host"], "127.0.0.1");
    }
    // Too large for the cap (a tiny cap keeps the test input tiny).
    let server = MockServer::start().await;
    mount_config(&server).await;
    let small = client().with_limits(InviteLimits {
        config: 16,
        ..InviteLimits::default()
    });
    assert_eq!(
        small.engine_for(&link(&server)).await.unwrap_err().code,
        ErrorCode::InviteSiteUnreachable
    );
}

#[tokio::test]
async fn view_answers_map_to_codes_that_name_the_host() {
    for (status, code) in [
        (404u16, ErrorCode::InviteGone),
        (429, ErrorCode::InviteRateLimited),
        (500, ErrorCode::HostTrouble),
        (503, ErrorCode::HostTrouble),
    ] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(route("")))
            .respond_with(ResponseTemplate::new(status).set_body_json(failure(None)))
            .mount(&server)
            .await;
        let err = client().view(&link(&server)).await.unwrap_err();
        assert_eq!(err.code, code, "{status}");
        assert_eq!(err.params["host"], "127.0.0.1");
    }
}

#[tokio::test]
async fn the_view_is_shared_for_its_ttl_and_fresh_reads_skip_it() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = client().with_timing(InviteTiming {
        view_ttl: Duration::from_millis(50),
        ..fast()
    });
    let l = link(&server);
    client.view(&l).await.unwrap();
    client.view(&l).await.unwrap();
    assert_eq!(hits(&server, &route("")).await, 1);
    tokio::time::sleep(Duration::from_millis(80)).await;
    client.view(&l).await.unwrap();
    assert_eq!(hits(&server, &route("")).await, 2);
    client.view_fresh(&l).await.unwrap();
    assert_eq!(hits(&server, &route("")).await, 3);
}

#[tokio::test]
async fn callers_at_the_same_moment_share_one_request() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = client();
    let other = client.clone();
    let (l1, l2) = (link(&server), link(&server));
    let (a, b) = tokio::join!(client.view(&l1), other.view(&l2));
    assert_eq!(a.unwrap().view, b.unwrap().view);
    assert_eq!(hits(&server, &route("")).await, 1);
}

#[tokio::test]
async fn the_icon_is_a_small_png_or_nothing() {
    let png = b"\x89PNG\r\n\x1a\nabcd".to_vec();
    let small = |c: InviteClient| {
        c.with_limits(InviteLimits {
            icon: 16,
            ..InviteLimits::default()
        })
    };
    for (status, body, want) in [
        (200u16, png.clone(), Some(png.clone())),
        (200, b"<html>no</html>".to_vec(), None),
        (200, [png.clone(), b"0123456789".to_vec()].concat(), None),
        (404, Vec::new(), None),
    ] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(route("/icon")))
            .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
            .mount(&server)
            .await;
        assert_eq!(small(client()).icon(&link(&server)).await, want, "{status}");
    }
}

#[tokio::test]
async fn a_view_with_odd_values_still_parses() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let motd = "§".repeat(2000);
    mount_view(
        &server,
        json!({
            "kind": "stack", "name": "odd", "motd": motd, "status": "exploding",
            "address": { "host": "-bad host", "port": 70000 },
            "game": { "type": "vanilla", "version": "1.21.1" },
            "team": { "accent": "red" }, "somethingNew": { "a": 1 }
        }),
    )
    .await;
    let view = client().view(&link(&server)).await.unwrap().view;
    assert_eq!(view.status(), None);
    assert_eq!(view.address.port, 70000);
    assert_eq!(view.team.accent, "red");
    assert!(view.players.is_none() && view.pack.is_none() && !view.whitelisted);
}

#[tokio::test]
async fn a_captive_portal_page_is_unexpected_data() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!("<html>Sign in to Wi-Fi to reach /{TOKEN}</html>")),
        )
        .mount(&server)
        .await;
    let err = client().view(&link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
    let text = format!("{err:?} {}", serde_json::to_string(&err).unwrap());
    assert!(!text.contains(TOKEN), "{text}");

    // A view larger than the cap is unexpected too.
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let small = client().with_limits(InviteLimits {
        view: 64,
        ..InviteLimits::default()
    });
    assert_eq!(
        small.view(&link(&server)).await.unwrap_err().code,
        ErrorCode::MetaUnexpected
    );
}

#[tokio::test]
async fn an_unreachable_engine_is_forgotten_and_found_again() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = client();
    let l = link(&server);
    // An instance saved an Engine that has since moved (nothing listens on port 1).
    client.remember_engine(&l, "http://127.0.0.1:1");
    assert_eq!(client.view(&l).await.unwrap_err().code, ErrorCode::Offline);
    assert_eq!(client.view(&l).await.unwrap().view.name, "survival");
    assert_eq!(hits(&server, "/config.json").await, 1);
}
```

`vanilla_view` and `network_view` are used from Task 5 on. Until then, add `#![allow(dead_code)]` as the file's first line, and remove it in Task 5.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test invite`
Expected: compile errors (`InviteClient`, `InviteTiming`, `ServerStatus` don't exist).

- [ ] **Step 3: `ServerStatus`**

In `crates/launcher-core/src/instances.rs`, after `UpdatePolicy`:

```rust
/// A server's state, as its invite view last said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Online,
    Starting,
    Sleeping,
    Offline,
}

impl ServerStatus {
    /// The Engine's word; anything else (a newer Engine's state) is `None`, shown as "Unknown".
    pub fn parse(s: &str) -> Option<ServerStatus> {
        match s {
            "online" => Some(ServerStatus::Online),
            "starting" => Some(ServerStatus::Starting),
            "sleeping" => Some(ServerStatus::Sleeping),
            "offline" => Some(ServerStatus::Offline),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: The view mirror**

Create `crates/launcher-core/src/invite/view.rs`:

```rust
//! A serde mirror of the Engine's public invite view (`InviteViewDTO`). Unknown fields are
//! ignored, and every field the Engine may omit has a default, so an Engine that adds or drops
//! optional fields still parses.
use serde::Deserialize;

use crate::instances::ServerStatus;

/// The Engine's pack states.
pub const PACK_READY: &str = "ready";
pub const PACK_PENDING: &str = "pending";
pub const PACK_UNAVAILABLE: &str = "unavailable";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    /// `stack` or `network`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    /// May hold `§` colour codes.
    #[serde(default)]
    pub motd: String,
    #[serde(default)]
    pub icon_version: Option<String>,
    #[serde(default)]
    pub address: InviteAddress,
    #[serde(default)]
    pub game: InviteGame,
    /// `online | starting | sleeping | offline`; see [`InviteView::status`].
    #[serde(default)]
    pub status: String,
    /// Present only while online.
    #[serde(default)]
    pub players: Option<InvitePlayers>,
    #[serde(default)]
    pub whitelisted: bool,
    #[serde(default)]
    pub note: Option<String>,
    /// `None` for networks and for stacks without a modpack.
    #[serde(default)]
    pub pack: Option<InvitePack>,
    #[serde(default)]
    pub team: InviteTeam,
    /// Present when the Engine advertises the launcher (`LAUNCHER_DOWNLOAD_URL`).
    #[serde(default)]
    pub launcher: Option<InviteLauncher>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InviteAddress {
    /// Absent without the Engine's `PUBLIC_GAME_HOST`.
    #[serde(default)]
    pub host: Option<String>,
    /// May be 0. Kept wide so an out-of-range value still parses and is refused later.
    #[serde(default)]
    pub port: i64,
    #[serde(default)]
    pub display: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteGame {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InvitePlayers {
    #[serde(default)]
    pub online: u32,
    /// Absent for networks.
    #[serde(default)]
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitePack {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub mod_count: u32,
    /// `ready | pending | unavailable`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub zip_state: Option<String>,
    /// The pinned modpack commit, the `/pack` ETag without quotes. Absent on older Engines.
    #[serde(default)]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteTeam {
    #[serde(default)]
    pub display_name: String,
    /// Empty, or `cyan | blue | violet | magenta`.
    #[serde(default)]
    pub accent: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteLauncher {
    #[serde(default)]
    pub download_url: Option<String>,
}

/// `Some` only for a non-blank string, trimmed.
pub fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl InviteView {
    pub fn status(&self) -> Option<ServerStatus> {
        ServerStatus::parse(&self.status)
    }

    pub fn is_network(&self) -> bool {
        self.kind == "network"
    }
}
```

- [ ] **Step 5: The client**

Create `crates/launcher-core/src/invite/client.rs`:

```rust
//! Talks to a server's Perland invite. It finds the Engine through the invite site's
//! `config.json`, reads the public view (shared by every caller for 15 s), and fetches the
//! icon. There is one per process, owned by `Sources`. Every error names the host only: the
//! token is in each request path, so no URL ever reaches a `LauncherError`.
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;

use super::{InviteLink, InviteView, Url};
use crate::error::{ErrorCode, LauncherError, Result};

#[derive(Debug, Clone, Copy)]
pub struct InviteTiming {
    /// How long one view read is shared by every caller.
    pub view_ttl: Duration,
    /// How often a pending pack is polled, and for how long.
    pub pack_poll_every: Duration,
    pub pack_poll_for: Duration,
}

impl Default for InviteTiming {
    fn default() -> Self {
        Self {
            view_ttl: Duration::from_secs(15),
            pack_poll_every: Duration::from_secs(5),
            pack_poll_for: Duration::from_secs(180),
        }
    }
}

/// Body size caps, in bytes. Overridable so tests stay tiny.
#[derive(Debug, Clone, Copy)]
pub struct InviteLimits {
    pub config: usize,
    pub view: usize,
    pub icon: usize,
    pub error_body: usize,
}

impl Default for InviteLimits {
    fn default() -> Self {
        Self {
            config: 4 << 10,
            view: 64 << 10,
            icon: 1 << 20,
            error_body: 4 << 10,
        }
    }
}

/// The view together with the Engine it came from (an address without a host falls back to
/// the Engine's host).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineView {
    pub engine: Url,
    pub view: InviteView,
}

type Slot = Arc<tokio::sync::Mutex<Option<(Instant, InviteView)>>>;

#[derive(Default)]
struct State {
    /// Invite site origin → its Engine.
    engines: Mutex<HashMap<String, Url>>,
    /// (Engine, token) → the last view read. Memory only.
    views: Mutex<HashMap<(String, String), Slot>>,
}

#[derive(Clone)]
pub struct InviteClient {
    http: reqwest::Client,
    timing: InviteTiming,
    limits: InviteLimits,
    state: Arc<State>,
}

// Manual Debug: the cache keys hold tokens.
impl std::fmt::Debug for InviteClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InviteClient")
            .field("timing", &self.timing)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct SiteConfig {
    #[serde(rename = "engineUrl", default)]
    engine_url: Option<String>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

#[derive(Deserialize)]
struct ErrorData {
    #[serde(default)]
    code: Option<String>,
}

pub(crate) fn offline(host: &str, detail: impl Display) -> LauncherError {
    LauncherError::network(ErrorCode::Offline, "You seem to be offline")
        .with_param("host", host)
        .with_fix("Check your internet connection and try again")
        .with_detail(detail)
}

fn site_unreachable(host: &str, why: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::InviteSiteUnreachable,
        format!("Couldn't reach the server's Perland site at {host}"),
    )
    .with_param("host", host)
    .with_fix("Check the link, or try again later")
    .with_detail(why)
}

pub(crate) fn gone(host: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteGone,
        "This invite link doesn't work anymore. Ask the owner for a new one.",
    )
    .with_param("host", host)
    .with_fix("Use a new invite link")
}

pub(crate) fn rate_limited(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::InviteRateLimited,
        "Too many requests. Try again in a minute.",
    )
    .with_param("host", host)
}

pub(crate) fn host_trouble(host: &str, status: impl Display) -> LauncherError {
    LauncherError::network(
        ErrorCode::HostTrouble,
        format!("{host} is having trouble ({status})"),
    )
    .with_param("host", host)
    .with_param("status", status)
    .with_fix("Try again in a few minutes")
}

fn unexpected(host: &str, why: &str) -> LauncherError {
    LauncherError::loader(
        ErrorCode::MetaUnexpected,
        format!("Unexpected data from {host}"),
    )
    .with_param("host", host)
    .with_detail(why)
}

/// A request that never got an answer. `without_url` keeps the token out of the detail.
fn request_failed(host: &str, e: reqwest::Error) -> LauncherError {
    if e.is_redirect() {
        host_trouble(host, "redirect")
    } else {
        offline(host, e.without_url())
    }
}

/// What an invite route's status means; `None` for success.
pub(crate) fn status_error(status: reqwest::StatusCode, host: &str) -> Option<LauncherError> {
    match status.as_u16() {
        200..=299 | 304 => None,
        404 => Some(gone(host)),
        429 => Some(rate_limited(host)),
        s => Some(host_trouble(host, s)),
    }
}

/// At most `cap` bytes of the body; `None` when it is longer.
async fn read_capped(
    mut response: reqwest::Response,
    cap: usize,
) -> std::result::Result<Option<Vec<u8>>, reqwest::Error> {
    if response.content_length().is_some_and(|n| n > cap as u64) {
        return Ok(None);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > cap {
            return Ok(None);
        }
    }
    Ok(Some(body))
}

/// An `engineUrl` the launcher will call: https (or http on this computer), with a host and no
/// user info, query or fragment.
pub fn check_engine_url(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    let scheme_ok = url.scheme() == "https"
        || (url.scheme() == "http" && crate::http::is_loopback(&url));
    (scheme_ok
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

/// `<engine>/api/v1/invites/<token><suffix>` (Engine `invite/src/lib/api.ts` `inviteUrl`).
pub fn invite_route(engine: &Url, link: &InviteLink, suffix: &str) -> Url {
    let base = engine.as_str().trim_end_matches('/');
    Url::parse(&format!("{base}/api/v1/invites/{}{suffix}", link.token()))
        .expect("a checked Engine URL plus a base64url token and a fixed suffix is a URL")
}

fn host_of(engine: &Url) -> String {
    engine.host_str().unwrap_or("the server").to_string()
}

impl InviteClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            timing: InviteTiming::default(),
            limits: InviteLimits::default(),
            state: Arc::default(),
        }
    }

    pub fn with_timing(mut self, timing: InviteTiming) -> Self {
        self.timing = timing;
        self
    }

    pub fn with_limits(mut self, limits: InviteLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn timing(&self) -> InviteTiming {
        self.timing
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Seed the Engine for `link`'s site from what an instance saved, so a Play needs no
    /// `config.json`. An unusable value is ignored, and a known Engine is kept.
    pub fn remember_engine(&self, link: &InviteLink, engine: &str) {
        if let Some(url) = check_engine_url(engine) {
            self.state
                .engines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(link.origin())
                .or_insert(url);
        }
    }

    fn forget_engine(&self, link: &InviteLink) {
        self.state
            .engines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&link.origin());
    }

    /// The Engine behind `link`'s invite site: `<origin>/config.json` → `engineUrl`, cached
    /// per origin.
    pub async fn engine_for(&self, link: &InviteLink) -> Result<Url> {
        let known = self
            .state
            .engines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&link.origin())
            .cloned();
        if let Some(url) = known {
            return Ok(url);
        }
        let host = link.host();
        let response = self
            .http
            .get(format!("{}/config.json", link.origin()))
            .send()
            .await
            .map_err(|e| {
                if e.is_redirect() {
                    site_unreachable(host, "config.json redirected somewhere unsafe")
                } else {
                    offline(host, e.without_url())
                }
            })?;
        if !response.status().is_success() {
            return Err(site_unreachable(
                host,
                &format!("config.json answered {}", response.status().as_u16()),
            ));
        }
        let body = read_capped(response, self.limits.config)
            .await
            .map_err(|e| offline(host, e.without_url()))?
            .ok_or_else(|| site_unreachable(host, "config.json is too large"))?;
        let config: SiteConfig = serde_json::from_slice(&body)
            .map_err(|_| site_unreachable(host, "config.json isn't valid JSON"))?;
        let engine = config
            .engine_url
            .as_deref()
            .and_then(check_engine_url)
            .ok_or_else(|| site_unreachable(host, "config.json has no https engineUrl"))?;
        self.state
            .engines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(link.origin(), engine.clone());
        Ok(engine)
    }

    /// The view, shared by every caller for `view_ttl` (status polls, `pack_status`, Play).
    pub async fn view(&self, link: &InviteLink) -> Result<EngineView> {
        self.view_inner(link, false).await
    }

    /// The view from the Engine now, skipping the shared copy, which it then replaces (the
    /// pending-pack poll).
    pub async fn view_fresh(&self, link: &InviteLink) -> Result<EngineView> {
        self.view_inner(link, true).await
    }

    async fn view_inner(&self, link: &InviteLink, fresh: bool) -> Result<EngineView> {
        let engine = self.engine_for(link).await?;
        let slot = {
            let mut views = self.state.views.lock().unwrap_or_else(|e| e.into_inner());
            views
                .entry((engine.to_string(), link.token().to_string()))
                .or_default()
                .clone()
        };
        // Held across the request: callers that arrive meanwhile wait and share its answer.
        let mut cached = slot.lock().await;
        if !fresh {
            if let Some((at, view)) = cached.as_ref() {
                if at.elapsed() < self.timing.view_ttl {
                    return Ok(EngineView {
                        engine,
                        view: view.clone(),
                    });
                }
            }
        }
        let view = match self.fetch_view(&engine, link).await {
            Ok(view) => view,
            Err(e) => {
                // The Engine may have moved: ask the invite site again next time.
                if e.code == ErrorCode::Offline {
                    self.forget_engine(link);
                }
                return Err(e);
            }
        };
        *cached = Some((Instant::now(), view.clone()));
        Ok(EngineView { engine, view })
    }

    async fn fetch_view(&self, engine: &Url, link: &InviteLink) -> Result<InviteView> {
        let host = host_of(engine);
        let response = self
            .http
            .get(invite_route(engine, link, ""))
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        if let Some(e) = status_error(response.status(), &host) {
            return Err(e);
        }
        let body = read_capped(response, self.limits.view)
            .await
            .map_err(|e| request_failed(&host, e))?
            .ok_or_else(|| unexpected(&host, "the invite view is too large"))?;
        // The parser's own message can quote the body, so it is never shown.
        let envelope: Envelope<InviteView> = serde_json::from_slice(&body)
            .map_err(|_| unexpected(&host, "the invite view isn't valid"))?;
        envelope
            .data
            .ok_or_else(|| unexpected(&host, "the invite view is empty"))
    }

    /// The server's icon PNG (PNG signature checked, size capped), or `None`: no icon, or
    /// anything going wrong. Never an error.
    pub async fn icon(&self, link: &InviteLink) -> Option<Vec<u8>> {
        let engine = self.engine_for(link).await.ok()?;
        let response = self
            .http
            .get(invite_route(&engine, link, "/icon"))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body = read_capped(response, self.limits.icon).await.ok()??;
        body.starts_with(b"\x89PNG\r\n\x1a\n").then_some(body)
    }

    /// `<engine>/api/v1/invites/<token>/pack`: the `.mrpack` of the commit the server runs.
    pub async fn pack_url(&self, link: &InviteLink) -> Result<Url> {
        Ok(invite_route(&self.engine_for(link).await?, link, "/pack"))
    }

    /// The Engine's `data.code` from a failed pack answer (`modrinth_unavailable`, …), kept
    /// only if it is a short snake_case word.
    pub(crate) async fn error_code(&self, url: Url) -> Option<String> {
        let response = self.http.get(url).send().await.ok()?;
        let body = read_capped(response, self.limits.error_body).await.ok()??;
        let envelope: Envelope<ErrorData> = serde_json::from_slice(&body).ok()?;
        envelope.data?.code.filter(|c| {
            (1..=64).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        })
    }
}
```

Replace `crates/launcher-core/src/invite/mod.rs` with:

```rust
//! Perland server invites (join-by-link spec §2.1): the link, the Engine's public view of it,
//! and the client that reads that view. Invite tokens are credentials: nothing here prints one.
pub mod client;
pub mod link;
pub mod view;

pub use client::{
    check_engine_url, invite_route, EngineView, InviteClient, InviteLimits, InviteTiming,
};
pub use link::{invite_link_cases, shown_invite, InviteLink, InviteLinkCase, TOKEN_LEN};
pub use view::{
    non_empty, InviteAddress, InviteGame, InviteLauncher, InvitePack, InvitePlayers, InviteTeam,
    InviteView, PACK_PENDING, PACK_READY, PACK_UNAVAILABLE,
};

/// Engine and invite URLs, re-exported so the app can name them without depending on reqwest.
pub type Url = reqwest::Url;
```

- [ ] **Step 6: Run them to verify they pass**

Run: `cargo test -p launcher-core --test invite && cargo test -p launcher-core --lib`
Expected: PASS. The "unreachable Engine" test may take about 2 s on Windows: a refused localhost connection is retried by the OS.

- [ ] **Step 7: Commit**

```bash
git add crates/launcher-core/src/invite crates/launcher-core/src/instances.rs crates/launcher-core/tests/invite.rs
git commit -m "feat(core): invite client: config.json discovery, a 15 s shared view, icon, size caps, host-only errors

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `ServerAddress`, `ServerLink` and `Instance.server`

**Files:**
- Modify: `crates/launcher-core/src/instances.rs`
- Modify: `crates/launcher-app/tests/contract.rs` (`server: None` in the two instance literals)
- Regenerate: `ui/src/ipc/contract/instance.json`, `ui/src/ipc/contract/instance_listing.json`
- Modify: `ui/src/ipc/schema.ts` (`serverStatus`, `serverAddress`, `serverLink`, `instance.server`), `ui/src/ipc/mock.ts` (`server: null` on created instances), and every other TypeScript `Instance` literal `pnpm -r typecheck` points at

**Interfaces:**
- Consumes `InviteView`, `InviteAddress`, `non_empty` and `Url` (Task 3).
- Produces in `launcher_core::instances`:
  - `pub struct ServerAddress { pub host: String, pub port: u16 }` (serde `{host, port}`).
    - `ServerAddress::new(host: &str, port: i64) -> Option<ServerAddress>`.
    - `ServerAddress::from_view(&InviteAddress, engine: &Url) -> Option<ServerAddress>`.
    - `.display() -> String`: `host:port`, with an IPv6 literal in brackets.
  - `pub fn valid_host(&str) -> bool`.
  - `pub struct ServerLink { address, name, accent, whitelisted, last_status, last_seen, gone }` (camelCase).
    - `ServerLink::from_view(&InviteView, &Url, now: i64) -> ServerLink`.
    - `.refresh(&InviteView, &Url, now)` keeps the last good address when the new one is invalid.
    - `.mark_gone()`.
  - `pub fn accent_of(&str) -> Option<String>` and `pub fn unix_now() -> i64`.
  - `Instance` gains `#[serde(default)] pub server: Option<ServerLink>`. `create` sets `None`.

- [ ] **Step 1: Write the failing tests**

In `instances.rs` `mod tests`, add:

```rust
    use crate::invite::{InviteAddress, InviteTeam, InviteView, Url};

    fn engine() -> Url {
        Url::parse("https://api.example.com").unwrap()
    }

    #[test]
    fn addresses_are_validated() {
        let long = format!("{}.example.com", "a".repeat(250));
        for (host, port, want) in [
            ("play.example.com", 25565, Some(("play.example.com", 25565))),
            ("PLAY.Example.com", 1, Some(("play.example.com", 1))),
            ("10.0.0.5", 25565, Some(("10.0.0.5", 25565))),
            ("[2001:db8::1]", 25565, Some(("2001:db8::1", 25565))),
            ("2001:db8::1", 65535, Some(("2001:db8::1", 65535))),
            ("play.example.com.", 25565, Some(("play.example.com.", 25565))),
            ("-evil.example", 25565, None),
            ("--server", 1, None),
            ("play example.com", 25565, None),
            ("play.example.com\n", 25565, None),
            ("x_y.example.com", 25565, None),
            ("a..b", 25565, None),
            ("", 25565, None),
            (long.as_str(), 25565, None),
            ("play.example.com", 0, None),
            ("play.example.com", 65536, None),
            ("play.example.com", -1, None),
        ] {
            let got = ServerAddress::new(host, port);
            let want = want.map(|(h, p): (&str, u16)| ServerAddress {
                host: h.into(),
                port: p,
            });
            assert_eq!(got, want, "{host:?}:{port}");
        }
        let v6 = ServerAddress::new("2001:db8::1", 25565).unwrap();
        assert_eq!(v6.display(), "[2001:db8::1]:25565");
        assert_eq!(
            ServerAddress::new("play.example.com", 25566).unwrap().display(),
            "play.example.com:25566"
        );
    }

    #[test]
    fn the_address_falls_back_to_the_engine_host() {
        let none = InviteAddress {
            host: None,
            port: 25566,
            display: None,
        };
        assert_eq!(
            ServerAddress::from_view(&none, &engine()).unwrap().display(),
            "api.example.com:25566"
        );
        let blank = InviteAddress {
            host: Some("  ".into()),
            port: 25566,
            display: None,
        };
        assert_eq!(
            ServerAddress::from_view(&blank, &engine()).unwrap().host,
            "api.example.com"
        );
    }

    fn view(port: i64, status: &str) -> InviteView {
        InviteView {
            kind: "stack".into(),
            name: " survival ".into(),
            status: status.into(),
            whitelisted: true,
            address: InviteAddress {
                host: Some("play.example.com".into()),
                port,
                display: None,
            },
            team: InviteTeam {
                display_name: "Acme".into(),
                accent: "violet".into(),
            },
            ..InviteView::default()
        }
    }

    #[test]
    fn server_link_follows_the_view_and_keeps_the_last_address() {
        let mut link = ServerLink::from_view(&view(25566, "online"), &engine(), 100);
        assert_eq!(link.name, "survival");
        assert_eq!(link.accent.as_deref(), Some("violet"));
        assert!(link.whitelisted && !link.gone);
        assert_eq!(link.last_status, Some(ServerStatus::Online));
        assert_eq!(link.last_seen, Some(100));
        let address = link.address.clone().unwrap();

        link.refresh(&view(0, "sleeping"), &engine(), 200);
        assert_eq!(link.address, Some(address.clone()), "a bad port keeps the last good one");
        assert_eq!(link.last_status, Some(ServerStatus::Sleeping));
        assert_eq!(link.last_seen, Some(200));

        link.mark_gone();
        assert!(link.gone);
        assert_eq!(link.address, Some(address));
        assert_eq!(link.last_seen, Some(200));
        link.refresh(&view(25566, "online"), &engine(), 300);
        assert!(!link.gone, "a link that works again isn't dead");

        let fresh = ServerLink::from_view(&view(0, "exploding"), &engine(), 1);
        assert_eq!((fresh.address, fresh.last_status), (None, None));
    }

    #[test]
    fn accents_are_known_or_other() {
        assert_eq!(accent_of("cyan").as_deref(), Some("cyan"));
        assert_eq!(accent_of("red").as_deref(), Some("other"));
        assert_eq!(accent_of(" "), None);
    }

    #[test]
    fn instance_files_without_a_server_still_load() {
        let json = r#"{"id":"a","name":"A","source":{"kind":"none"},"game":{"minecraft":"1.20.1"}}"#;
        let instance: Instance = serde_json::from_str(json).unwrap();
        assert_eq!(instance.server, None);
        let back = serde_json::to_value(&instance).unwrap();
        assert!(back["server"].is_null());
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib instances::`
Expected: compile errors (`ServerAddress`, `ServerLink`, `Instance.server` don't exist).

- [ ] **Step 3: Implement**

In `instances.rs`:
- Add the imports `use std::time::{SystemTime, UNIX_EPOCH};` and `use crate::invite::{non_empty, InviteAddress, InviteView, Url};`.
- Below `ServerStatus`, add:

```rust
/// Where the game connects: a validated host (a DNS name or an IP literal) and a port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerAddress {
    /// Lowercase; an IPv6 literal without brackets.
    pub host: String,
    pub port: u16,
}

/// A DNS name or an IP literal: no leading `-`, no whitespace, at most 253 characters.
pub fn valid_host(host: &str) -> bool {
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('-')
        || host.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    if bare.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let name = host.strip_suffix('.').unwrap_or(host);
    !name.is_empty()
        && name.split('.').all(|label| {
            (1..=63).contains(&label.len())
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

impl ServerAddress {
    /// `None` unless `host` passes [`valid_host`] and `port` is 1–65535. The address reaches the
    /// game's arguments, so nothing else gets through.
    pub fn new(host: &str, port: i64) -> Option<ServerAddress> {
        let port = u16::try_from(port).ok().filter(|p| *p != 0)?;
        if !valid_host(host) {
            return None;
        }
        let bare = host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host);
        Some(ServerAddress {
            host: bare.to_ascii_lowercase(),
            port,
        })
    }

    /// From the view: its host, or the Engine's host when it has none (as the invite page does).
    pub fn from_view(address: &InviteAddress, engine: &Url) -> Option<ServerAddress> {
        let host = non_empty(&address.host).or(engine.host_str())?;
        ServerAddress::new(host, address.port)
    }

    /// `host:port`, with brackets around an IPv6 literal: what quick play takes and players type.
    pub fn display(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]:{}", self.host, self.port)
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

const ACCENTS: [&str; 4] = ["cyan", "blue", "violet", "magenta"];

/// The team accent: one of the Engine's four, `other` for anything else, `None` when unset.
pub fn accent_of(raw: &str) -> Option<String> {
    let accent = raw.trim();
    if accent.is_empty() {
        None
    } else if ACCENTS.contains(&accent) {
        Some(accent.to_string())
    } else {
        Some("other".to_string())
    }
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// What a server instance knows about its server, from the last invite view read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerLink {
    /// Last known and validated. Kept when the link stops working.
    #[serde(default)]
    pub address: Option<ServerAddress>,
    /// The view's name at the last read.
    pub name: String,
    /// `cyan | blue | violet | magenta | other`.
    #[serde(default)]
    pub accent: Option<String>,
    #[serde(default)]
    pub whitelisted: bool,
    #[serde(default)]
    pub last_status: Option<ServerStatus>,
    /// Unix seconds of the last successful view read.
    #[serde(default)]
    pub last_seen: Option<i64>,
    /// The last read said `invite_gone`.
    #[serde(default)]
    pub gone: bool,
}

impl ServerLink {
    pub fn from_view(view: &InviteView, engine: &Url, now: i64) -> ServerLink {
        ServerLink {
            address: ServerAddress::from_view(&view.address, engine),
            name: view.name.trim().chars().take(MAX_NAME_CHARS).collect(),
            accent: accent_of(&view.team.accent),
            whitelisted: view.whitelisted,
            last_status: view.status(),
            last_seen: Some(now),
            gone: false,
        }
    }

    /// A successful read. Everything follows the view, except that an invalid address keeps
    /// the last good one.
    pub fn refresh(&mut self, view: &InviteView, engine: &Url, now: i64) {
        let last = self.address.take();
        *self = ServerLink::from_view(view, engine, now);
        if self.address.is_none() {
            self.address = last;
        }
    }

    /// The read said `invite_gone`: only `gone` changes.
    pub fn mark_gone(&mut self) {
        self.gone = true;
    }
}
```

- In `struct Instance`, after `java`, add:

```rust
    /// Set for server instances (`SourceRef::Perland`): what the invite view last said.
    #[serde(default)]
    pub server: Option<ServerLink>,
```

- In `InstanceStore::create`, add `server: None,` to the literal.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-core`
Expected: PASS. Fix any other `Instance { … }` literal the compiler points at by adding `server: None`.

- [ ] **Step 5: The contract and the UI schema**

In `crates/launcher-app/tests/contract.rs`, add `server: None,` to `fabric_instance()` and `vanilla_instance()`. Then:

```bash
UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract
```

In `ui/src/ipc/schema.ts`, above `instance`:

```ts
export const serverStatus = z.enum(['online', 'starting', 'sleeping', 'offline']);
export const serverAddress = z.strictObject({ host: z.string(), port: z.number().int() });
export const serverLink = z.strictObject({
  address: serverAddress.nullable(),
  name: z.string(),
  accent: z.string().nullable(),
  whitelisted: z.boolean(),
  lastStatus: serverStatus.nullable(),
  lastSeen: z.number().int().nullable(),
  gone: z.boolean(),
});
```

Add `server: serverLink.nullable(),` as the last field of `instance`, and these types at the bottom:

```ts
export type ServerStatus = z.infer<typeof serverStatus>;
export type ServerAddress = z.infer<typeof serverAddress>;
export type ServerLink = z.infer<typeof serverLink>;
```

In `ui/src/ipc/mock.ts` `instance_create`, add `server: null` to `created`. Add `server: null` to any other `Instance` literal that `pnpm -r typecheck` reports.

- [ ] **Step 6: Run everything**

Run: `cargo test --workspace && pnpm -r typecheck && pnpm -r test`
Expected: PASS. The contract test parses the new `"server": null`.

- [ ] **Step 7: Commit**

```bash
git add crates/launcher-core/src/instances.rs crates/launcher-app/tests/contract.rs ui/src
git commit -m "feat(core): validated server addresses and the ServerLink a server instance keeps

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The `perland` source: `SourceRef`/`PackInput` variants, resolve with the pending poll, check with the HEAD fallback

**Files:**
- Modify: `crates/launcher-core/src/instances.rs` (`SourceRef::Perland`, manual `Debug`)
- Create: `crates/launcher-core/src/sources/perland.rs`
- Modify: `crates/launcher-core/src/sources/mod.rs` (`pub mod perland;`, `PackInput::Perland`, `Sources.invites`, the `open`/`resolve`/`check` arms)
- Modify: `crates/launcher-core/src/sync/mod.rs` (`for_installer` builds the `InviteClient`)
- Modify: `crates/launcher-core/tests/common/mod.rs` (`Sources.invites` with fast timing)
- Modify: `crates/perland-cli/src/main.rs` (`pack add` of a server ensures instead of syncing the view-only state)
- Test: `crates/launcher-core/tests/invite.rs`

**Interfaces:**
- Consumes `InviteClient`, `EngineView`, `invite_route` and the helpers from Task 3, plus `ServerAddress` from Task 4.
- Produces:
  - `SourceRef::Perland { invite: String, engine: String }`.
    - Serde: `{"kind":"perland","invite":…,"engine":…}`.
    - `invite` is `InviteLink::canonical()`, and `engine` is the Engine URL string.
    - `Debug` prints `shown_invite(invite)`.
  - `PackInput::Perland { link: InviteLink }`. `PackInput::parse` tries `InviteLink::parse` before its "any URL is packwiz" rule.
  - `Sources` gains `pub invites: InviteClient`. Every `Sources { … }` literal adds it.
  - In `launcher_core::sources::perland`:
    - `pub const VANILLA_PREFIX: &str = "vanilla:"`
    - `pub fn stamp_of(&InviteView) -> Option<String>` and `pub fn version_of(&InviteView) -> String`
    - `pub fn supported(&EngineView) -> Result<()>`: `invite_unsupported` with an `address` param
    - `pub async fn open(&Sources, &InviteLink) -> Result<(SourceRef, DesiredState)>`: view only, no download
    - `pub async fn resolve(&Sources, &InviteLink, &EventSink) -> Result<DesiredState>`
    - `pub async fn resolve_ref(&Sources, invite: &str, engine: &str, &EventSink) -> Result<DesiredState>`
    - `pub async fn check(&Sources, invite: &str, engine: &str, installed: &DesiredState) -> UpdateCheck`
    - `pub(crate) fn scrub(LauncherError, token: &str) -> LauncherError`
- Behaviour:
  - **`open` reads only the view.** Plan 3b's `add_pack` keeps only the name, game, policy and art of what `open` returns, and the first Play resolves the real pack. So Add never downloads, and never waits on a pack that is still building. That wait happens at Play, with its progress label.
  - **Errors:** each one passes through `scrub`, so a token in any downloader detail becomes `<invite>`.
  - **`check` never fails:**
    - `invite_gone` passes through unchanged (the app marks the link dead);
    - every other failure becomes `update_check_failed`, with the original message as detail.

- [ ] **Step 1: Write the failing unit tests**

In `instances.rs` `mod tests`:

```rust
    #[test]
    fn perland_sources_round_trip_and_hide_their_token() {
        let token = "A".repeat(43);
        let source = SourceRef::Perland {
            invite: format!("https://play.example.com/{token}"),
            engine: "https://api.example.com/".into(),
        };
        let json = serde_json::to_value(&source).unwrap();
        assert_eq!(json["kind"], "perland");
        assert_eq!(json["engine"], "https://api.example.com/");
        let back: SourceRef = serde_json::from_value(json).unwrap();
        assert_eq!(back, source);
        let shown = format!("{source:?}");
        assert!(!shown.contains(&token), "{shown}");
        assert!(shown.contains("play.example.com/<invite>"), "{shown}");
        assert_eq!(format!("{:?}", SourceRef::None), "None");
    }
```

In `sources/mod.rs` `mod tests`:

```rust
    #[test]
    fn pack_input_reads_invite_links_before_packwiz() {
        let token = "A".repeat(43);
        let input = PackInput::parse(&format!(" https://play.example.com/{token}/ ")).unwrap();
        let PackInput::Perland { link } = &input else {
            panic!("{input:?}");
        };
        assert_eq!(link.canonical(), format!("https://play.example.com/{token}"));
        assert!(!format!("{input:?}").contains(&token));
        assert!(matches!(
            PackInput::parse("http://127.0.0.1:8080/pack.toml"),
            Ok(PackInput::Packwiz { .. })
        ));
    }
```

- [ ] **Step 2: Write the failing integration tests**

Remove the `#![allow(dead_code)]` line from `crates/launcher-core/tests/invite.rs`. Extend its imports:

```rust
use std::sync::{Arc, Mutex};

use common::{sources, write_zip};
use launcher_core::{
    events::{noop_sink, Event, EventSink},
    instances::SourceRef,
    meta::loaders::LoaderKind,
    sources::{perland, DesiredState, PackInput, UpdateCheck},
};
use wiremock::matchers::header;
```

Add the helpers and tests:

```rust
/// A tiny `.mrpack` for Fabric 1.20.1 with one override file.
fn mrpack(dir: &std::path::Path, version: &str) -> Vec<u8> {
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": version, "name": "Survival Pack (index)",
        "files": [], "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.16.14" }
    }))
    .unwrap();
    let path = dir.join(format!("{version}.mrpack"));
    write_zip(
        &path,
        &[
            ("modrinth.index.json", &index[..]),
            ("overrides/config/server.txt", &b"motd=hi"[..]),
        ],
    );
    std::fs::read(path).unwrap()
}

async fn mount_pack(server: &MockServer, bytes: Vec<u8>, etag: &str) {
    for verb in ["GET", "HEAD"] {
        Mock::given(method(verb))
            .and(path(route("/pack")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", format!("\"{etag}\"").as_str())
                    .set_body_bytes(bytes.clone()),
            )
            .mount(server)
            .await;
    }
}

fn collect() -> (EventSink, Arc<Mutex<Vec<Event>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    (Arc::new(move |e| s2.lock().unwrap().push(e)), seen)
}

fn source(server: &MockServer) -> SourceRef {
    SourceRef::Perland {
        invite: invite_url(server),
        engine: server.uri(),
    }
}

fn installed(stamp: &str) -> DesiredState {
    DesiredState {
        name: "survival".into(),
        pack_version: "1.0.3".into(),
        minecraft: "1.20.1".into(),
        loader: None,
        files: vec![],
        stamp: stamp.into(),
        about: Default::default(),
        archive: None,
    }
}

#[tokio::test]
async fn adding_reads_only_the_view_and_resolving_downloads_the_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;
    let src = sources(&server, tmp.path());

    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (source, shell) = src.open(&input).await.unwrap();
    assert_eq!(
        source,
        SourceRef::Perland {
            invite: invite_url(&server),
            engine: format!("{}/", server.uri()),
        }
    );
    assert_eq!(shell.name, "survival");
    assert_eq!(shell.minecraft, "1.20.1");
    assert_eq!(shell.loader.as_ref().unwrap().kind, LoaderKind::Fabric);
    assert!(shell.files.is_empty());
    assert_eq!(hits(&server, &route("/pack")).await, 0, "Add downloads nothing");

    let desired = src.resolve(&source).await.unwrap();
    assert_eq!(desired.stamp, "c1");
    assert_eq!(desired.name, "survival");
    assert_eq!(desired.pack_version, "1.0.4");
    assert_eq!(desired.loader.as_ref().unwrap().version, "0.16.14");
    let paths: Vec<_> = desired.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["config/server.txt"]);
    assert!(desired.archive.is_some());
    assert_eq!(desired.about.policy, Some(launcher_core::instances::UpdatePolicy::Auto));
    assert_eq!(desired.about.description.as_deref(), Some("Be nice."));
    assert!(desired.about.art.icon.as_deref().unwrap().ends_with(&route("/icon")));
}

#[tokio::test]
async fn without_a_commit_the_stamp_is_the_pack_etag() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(None, "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "e7a9").await;
    let desired = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap();
    assert_eq!(desired.stamp, "e7a9");
}

#[tokio::test]
async fn a_vanilla_server_resolves_to_vanilla_and_follows_its_version() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("1.21.1")).await;
    let desired = sources(&server, tmp.path())
        .resolve(&source(&server))
        .await
        .unwrap();
    assert_eq!(desired.stamp, "vanilla:1.21.1");
    assert_eq!((desired.minecraft.as_str(), desired.pack_version.as_str()), ("1.21.1", "1.21.1"));
    assert!(desired.loader.is_none() && desired.files.is_empty() && desired.archive.is_none());

    // Same version: up to date. The server moves to 1.21.4: an update (fresh sources, so the
    // 15 s shared view doesn't hide the change).
    let src = sources(&server, tmp.path());
    assert_eq!(src.check(&source(&server), &desired).await, UpdateCheck::UpToDate);
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("1.21.4")).await;
    match sources(&server, tmp.path()).check(&source(&server), &desired).await {
        UpdateCheck::Available { stamp, version, .. } => {
            assert_eq!((stamp.as_str(), version.as_str()), ("vanilla:1.21.4", "1.21.4"))
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_server_that_drops_its_modpack_offers_vanilla() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("1.20.1")).await;
    match sources(&server, tmp.path()).check(&source(&server), &installed("c1")).await {
        UpdateCheck::Available { stamp, .. } => assert_eq!(stamp, "vanilla:1.20.1"),
        other => panic!("{other:?}"),
    }
    // And back: a vanilla instance whose server gained a modpack.
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c2"), "ready")).await;
    match sources(&server, tmp.path())
        .check(&source(&server), &installed("vanilla:1.20.1"))
        .await
    {
        UpdateCheck::Available { stamp, source: s, .. } => {
            assert_eq!(stamp, "c2");
            assert!(matches!(s, SourceRef::Perland { .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_update_check_compares_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let src = sources(&server, tmp.path());
    assert_eq!(src.check(&source(&server), &installed("c1")).await, UpdateCheck::UpToDate);
    match src.check(&source(&server), &installed("c0")).await {
        UpdateCheck::Available { stamp, version, .. } => {
            assert_eq!((stamp.as_str(), version.as_str()), ("c1", "1.0.4"))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(hits(&server, &route("/pack")).await, 0, "the commit answers it; no HEAD");
}

#[tokio::test]
async fn without_a_commit_the_check_asks_head_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(None, "ready")).await;
    Mock::given(method("HEAD"))
        .and(path(route("/pack")))
        .and(header("if-none-match", "\"c1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .and(path(route("/pack")))
        .respond_with(ResponseTemplate::new(200).insert_header("etag", "\"c2\""))
        .mount(&server)
        .await;
    let src = sources(&server, tmp.path());
    assert_eq!(src.check(&source(&server), &installed("c1")).await, UpdateCheck::UpToDate);
    match src.check(&source(&server), &installed("c0")).await {
        UpdateCheck::Available { stamp, .. } => assert_eq!(stamp, "c2"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_check_never_fails_and_a_dead_link_stays_recognizable() {
    for (status, code) in [(404u16, ErrorCode::InviteGone), (500, ErrorCode::UpdateCheckFailed)] {
        let tmp = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(route("")))
            .respond_with(ResponseTemplate::new(status).set_body_json(failure(None)))
            .mount(&server)
            .await;
        match sources(&server, tmp.path()).check(&source(&server), &installed("c1")).await {
            UpdateCheck::Unavailable(e) => assert_eq!(e.code, code, "{status}"),
            other => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn pack_failures_map_to_codes_with_the_engine_reason() {
    for (status, engine_code, want) in [
        (502u16, Some("modrinth_unavailable"), ErrorCode::ServerPackUnavailable),
        (413, Some("export_too_large"), ErrorCode::ServerPackUnavailable),
        (400, Some("invalid_modpack"), ErrorCode::ServerPackUnavailable),
        (404, None, ErrorCode::InviteGone),
        (429, None, ErrorCode::InviteRateLimited),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_view(&server, modded_view(Some("c1"), "ready")).await;
        Mock::given(method("GET"))
            .and(path(route("/pack")))
            .respond_with(ResponseTemplate::new(status).set_body_json(failure(engine_code)))
            .mount(&server)
            .await;
        let err = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
            .await
            .unwrap_err();
        assert_eq!(err.code, want, "{status}");
        if let Some(c) = engine_code {
            assert_eq!(err.detail.as_deref(), Some(c), "{status}");
        }
    }
}

#[tokio::test]
async fn an_unavailable_pack_is_refused_before_downloading() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "unavailable")).await;
    let err = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerPackUnavailable);
    assert_eq!(hits(&server, &route("/pack")).await, 0);
}

#[tokio::test]
async fn networks_are_refused_with_their_address() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, network_view()).await;
    let src = sources(&server, tmp.path());
    let err = perland::open(&src, &link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteUnsupported);
    assert_eq!(err.params["address"], "play.example.com:25577");
    let err = perland::resolve(&src, &link(&server), &noop_sink()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteUnsupported);
}

#[tokio::test]
async fn a_pending_pack_is_polled_until_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(modded_view(Some("c1"), "pending"))))
        .with_priority(1)
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;
    let (sink, seen) = collect();
    let desired = perland::resolve(&sources(&server, tmp.path()), &link(&server), &sink)
        .await
        .unwrap();
    assert_eq!(desired.stamp, "c1");
    assert!(hits(&server, &route("")).await >= 3);
    let waits = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|e| matches!(e, Event::Progress { task, .. } if task.key == "server_pack_wait"))
        .count();
    assert!(waits >= 1, "the run can say the server is preparing the modpack");
}

#[tokio::test]
async fn a_pack_still_building_after_the_limit_is_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "pending")).await;
    let started = std::time::Instant::now();
    let err = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerPackPending);
    assert!(started.elapsed() < Duration::from_secs(5), "the poll limit is injectable");
    assert_eq!(hits(&server, &route("/pack")).await, 0);
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib && cargo test -p launcher-core --test invite`
Expected: compile errors (`SourceRef::Perland`, `PackInput::Perland`, `perland` don't exist).

- [ ] **Step 4: `SourceRef::Perland` with a token-free `Debug`**

In `instances.rs`:
- Change `SourceRef`'s derive to `#[derive(Clone, PartialEq, Serialize, Deserialize)]`, dropping `Debug`.
- Add the variant after `Modrinth`:

```rust
    /// A Perland server's invite. `invite` is the canonical invite URL (a credential: `Debug`
    /// hides its token); `engine` is the Engine it was last found at.
    Perland { invite: String, engine: String },
```

- Then add:

```rust
// Manual Debug: a Perland invite URL holds its token.
impl std::fmt::Debug for SourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceRef::None => f.write_str("None"),
            SourceRef::Packwiz { url } => f.debug_struct("Packwiz").field("url", url).finish(),
            SourceRef::Mrpack { name, sha1 } => f
                .debug_struct("Mrpack")
                .field("name", name)
                .field("sha1", sha1)
                .finish(),
            SourceRef::Modrinth {
                project_id,
                version_id,
            } => f
                .debug_struct("Modrinth")
                .field("project_id", project_id)
                .field("version_id", version_id)
                .finish(),
            SourceRef::Perland { invite, engine } => f
                .debug_struct("Perland")
                .field("invite", &crate::invite::shown_invite(invite))
                .field("engine", engine)
                .finish(),
        }
    }
}
```

- [ ] **Step 5: Wire `Sources` and `PackInput`**

In `sources/mod.rs`:
- Add `pub mod perland;`.
- Import `crate::{events::noop_sink, invite::{InviteClient, InviteLink}}`.
- Add the variant to `PackInput`:

```rust
    /// A Perland server's invite link.
    Perland {
        link: InviteLink,
    },
```

- In `PackInput::parse`, right after the `modrinth:` branch and before the `http(s)://` branch:

```rust
        // Before the packwiz rule: an invite is also an https URL, but never ends in pack.toml.
        if let Ok(link) = InviteLink::parse(s) {
            return Ok(PackInput::Perland { link });
        }
```

- Add `pub invites: InviteClient,` to `Sources` (doc comment: "Server invites: one per process, so every caller shares its 15 s view cache.").
- Add the dispatch arms:
  - `open`: `PackInput::Perland { link } => perland::open(self, link).await,`
  - `resolve`: `SourceRef::Perland { invite, engine } => perland::resolve_ref(self, invite, engine, &noop_sink()).await,`
  - `check`: `SourceRef::Perland { invite, engine } => perland::check(self, invite, engine, installed).await,`

In `sync/mod.rs` `for_installer`, build the client first in the `Sources` literal, before `http` moves:

```rust
        let sources = Sources {
            invites: crate::invite::InviteClient::new(http.clone()),
            http,
            meta: installer.meta.clone(),
            downloader: installer.downloader.clone(),
            cache: ObjectCache::new(installer.dirs.objects()),
            config: SourceConfig::default(),
        };
```

In `crates/launcher-core/tests/common/mod.rs`:
- Import `launcher_core::invite::{InviteClient, InviteTiming}`.
- Add to the `Sources` literal in `sources()`, before `http: http.clone(),`:

```rust
        invites: InviteClient::new(http.clone()).with_timing(InviteTiming {
            view_ttl: Duration::from_secs(15),
            pack_poll_every: Duration::from_millis(10),
            pack_poll_for: Duration::from_millis(300),
        }),
```

Add `invites` to any other `Sources { … }` literal the compiler points at.

- [ ] **Step 6: The source**

Create `crates/launcher-core/src/sources/perland.rs`:

```rust
//! Perland server invites as a pack source (join-by-link spec §2.1).
//! - A modded stack resolves to the invite's `.mrpack`, built from the commit the server runs.
//! - A stack without a modpack resolves to vanilla at the server's Minecraft version.
//! - Networks are refused.
//! Every error names the host only: the token is part of every invite URL.
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

use sha1::Digest;

use super::{game_from, invalid, mrpack, DesiredState, PackAbout, PackArt, Sources, UpdateCheck};
use crate::{
    download::{hash::sha1_hex, DownloadRequest, Hashes},
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
    instances::{LoaderSpec, ServerAddress, SourceRef, UpdatePolicy},
    invite::{
        client::{gone, host_trouble, offline, rate_limited, status_error},
        invite_route, non_empty, EngineView, InviteLink, InviteView, Url, PACK_READY,
        PACK_UNAVAILABLE,
    },
};

/// The stamp of a stack without a modpack: `vanilla:<minecraft version>`.
pub const VANILLA_PREFIX: &str = "vanilla:";

fn game_version(view: &InviteView) -> Option<&str> {
    non_empty(&view.game.version)
}

/// What the update check compares: the pinned commit, or `vanilla:<version>`. `None` for a
/// modded stack on an Engine that doesn't send the commit yet.
pub fn stamp_of(view: &InviteView) -> Option<String> {
    match &view.pack {
        Some(pack) => non_empty(&pack.commit).map(str::to_string),
        None => game_version(view).map(|v| format!("{VANILLA_PREFIX}{v}")),
    }
}

/// The version players see: the pack's, else the server's Minecraft version.
pub fn version_of(view: &InviteView) -> String {
    view.pack
        .as_ref()
        .and_then(|p| non_empty(&p.version))
        .or_else(|| game_version(view))
        .unwrap_or_default()
        .to_string()
}

/// Refuse what the launcher can't join yet: networks, and stacks with neither a modpack nor a
/// Minecraft version. The error carries the address so the player can still join by hand.
pub fn supported(ev: &EngineView) -> Result<()> {
    let view = &ev.view;
    if !view.is_network() && (view.pack.is_some() || game_version(view).is_some()) {
        return Ok(());
    }
    let address = ServerAddress::from_view(&view.address, &ev.engine)
        .map(|a| a.display())
        .or_else(|| non_empty(&view.address.display).map(|d| d.chars().take(260).collect()))
        .unwrap_or_else(|| ev.engine.host_str().unwrap_or_default().to_string());
    Err(LauncherError::pack(
        ErrorCode::InviteUnsupported,
        format!("Networks can't be joined from the launcher yet. Join with the address {address}."),
    )
    .with_param("address", address))
}

/// A Minecraft version id the launcher can look up and use as a folder name.
fn valid_version(v: &str) -> bool {
    (1..=64).contains(&v.len())
        && !v.contains("..")
        && v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+ ".contains(&b))
}

fn unusable_version() -> LauncherError {
    invalid("the server's invite", "no usable Minecraft version")
}

fn display_name(view: &InviteView) -> String {
    view.name.trim().chars().take(64).collect()
}

fn engine_host(ev: &EngineView) -> String {
    ev.engine.host_str().unwrap_or("the server").to_string()
}

fn source_of(link: &InviteLink, ev: &EngineView) -> SourceRef {
    SourceRef::Perland {
        invite: link.canonical(),
        engine: ev.engine.to_string(),
    }
}

fn about(link: &InviteLink, ev: &EngineView) -> PackAbout {
    PackAbout {
        description: non_empty(&ev.view.note).map(str::to_string),
        art: PackArt {
            // Fetched by `art::fetch`, which never fails and reports nothing, so the token in
            // this URL can't leave it.
            icon: Some(invite_route(&ev.engine, link, "/icon").to_string()),
            banner: None,
        },
        policy: Some(UpdatePolicy::Auto),
    }
}

/// The loader the view names, if any. `vanilla`, or a loader without a version, means none.
fn loader_from(view: &InviteView, minecraft: &str) -> Result<Option<LoaderSpec>> {
    let Some(pack) = &view.pack else {
        return Ok(None);
    };
    let (Some(kind), Some(version)) = (non_empty(&pack.loader), non_empty(&pack.loader_version))
    else {
        return Ok(None);
    };
    if kind.eq_ignore_ascii_case("vanilla") {
        return Ok(None);
    }
    let versions = BTreeMap::from([
        ("minecraft".to_string(), minecraft.to_string()),
        (kind.to_ascii_lowercase(), version.to_string()),
    ]);
    Ok(game_from(&versions)?.1)
}

/// Replace the token wherever it appears in an error. The downloader's details keep URL paths,
/// and the token is in the path.
pub(crate) fn scrub(mut e: LauncherError, token: &str) -> LauncherError {
    let clean = |s: &str| s.replace(token, "<invite>");
    e.message = clean(&e.message);
    e.fix = e.fix.map(|f| clean(&f));
    e.detail = e.detail.map(|d| clean(&d));
    for value in e.params.values_mut() {
        *value = clean(value);
    }
    e
}

fn pack_unavailable(reason: Option<String>) -> LauncherError {
    let e = LauncherError::pack(
        ErrorCode::ServerPackUnavailable,
        "The server's modpack isn't available right now",
    )
    .with_fix("Try again later, or let the server's owner know");
    match reason {
        Some(r) => e.with_detail(r),
        None => e,
    }
}

fn pack_pending() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ServerPackPending,
        "The server is still preparing the modpack. Try again in a minute.",
    )
}

/// A failed check: a dead link stays `invite_gone` (the app shows its plate); anything else is
/// the usual "Couldn't check for updates" warning.
fn check_failed(e: LauncherError) -> LauncherError {
    if e.code == ErrorCode::InviteGone {
        return e;
    }
    LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
        .with_fix("You can still play the version you have")
        .with_detail(e.message)
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

/// A new server instance's source, and the pack as the view describes it. Nothing is
/// downloaded (`files` is empty and `stamp` is blank). The first sync resolves the real pack
/// (3b §4.1 decision 4), so a pack that is still building never holds up Add.
pub async fn open(src: &Sources, link: &InviteLink) -> Result<(SourceRef, DesiredState)> {
    open_inner(src, link).await.map_err(|e| scrub(e, link.token()))
}

async fn open_inner(src: &Sources, link: &InviteLink) -> Result<(SourceRef, DesiredState)> {
    let ev = src.invites.view(link).await?;
    supported(&ev)?;
    let view = &ev.view;
    let minecraft = view
        .pack
        .as_ref()
        .map(|p| p.minecraft.trim())
        .filter(|m| !m.is_empty())
        .or_else(|| game_version(view))
        .filter(|m| valid_version(m))
        .ok_or_else(unusable_version)?
        .to_string();
    let desired = DesiredState {
        name: display_name(view),
        pack_version: version_of(view),
        loader: loader_from(view, &minecraft)?,
        minecraft,
        files: Vec::new(),
        stamp: String::new(),
        about: about(link, &ev),
        archive: None,
    };
    Ok((source_of(link, &ev), desired))
}

/// `resolve` for a saved source: seeds the Engine it last used, so no `config.json` is needed.
pub async fn resolve_ref(
    src: &Sources,
    invite: &str,
    engine: &str,
    sink: &EventSink,
) -> Result<DesiredState> {
    let link = InviteLink::parse(invite)?;
    src.invites.remember_engine(&link, engine);
    resolve(src, &link, sink).await
}

/// Exactly what the server runs now. `sink` hears `server_pack_wait` while the Engine is still
/// building the pack.
pub async fn resolve(src: &Sources, link: &InviteLink, sink: &EventSink) -> Result<DesiredState> {
    resolve_inner(src, link, sink)
        .await
        .map_err(|e| scrub(e, link.token()))
}

async fn resolve_inner(
    src: &Sources,
    link: &InviteLink,
    sink: &EventSink,
) -> Result<DesiredState> {
    let ev = src.invites.view(link).await?;
    supported(&ev)?;
    let ev = wait_for_pack(src, link, ev, sink).await?;
    let Some(pack) = ev.view.pack.clone() else {
        return vanilla(link, &ev);
    };
    let (path, hashes, stamp) = download_pack(src, link, &ev).await?;
    let mut desired = tokio::task::spawn_blocking(move || {
        mrpack::read_archive(&path, stamp, hashes, mrpack::Limits::default())
    })
    .await
    .map_err(join_error)??;
    let name = display_name(&ev.view);
    if !name.is_empty() {
        desired.name = name;
    }
    if let Some(version) = non_empty(&pack.version) {
        desired.pack_version = version.to_string();
    }
    desired.about = about(link, &ev);
    Ok(desired)
}

/// A stack without a modpack: vanilla at the server's Minecraft version. When the server moves
/// to a new version, the stamp changes and sync installs it.
fn vanilla(link: &InviteLink, ev: &EngineView) -> Result<DesiredState> {
    let version = game_version(&ev.view)
        .filter(|v| valid_version(v))
        .ok_or_else(unusable_version)?
        .to_string();
    Ok(DesiredState {
        name: display_name(&ev.view),
        pack_version: version.clone(),
        minecraft: version.clone(),
        loader: None,
        files: Vec::new(),
        stamp: format!("{VANILLA_PREFIX}{version}"),
        about: about(link, ev),
        archive: None,
    })
}

/// While the Engine builds the pack, poll the view every `pack_poll_every` for up to
/// `pack_poll_for`, telling `sink` (`server_pack_wait`).
async fn wait_for_pack(
    src: &Sources,
    link: &InviteLink,
    mut ev: EngineView,
    sink: &EventSink,
) -> Result<EngineView> {
    let timing = src.invites.timing();
    let deadline = tokio::time::Instant::now() + timing.pack_poll_for;
    loop {
        let state = ev
            .view
            .pack
            .as_ref()
            .map_or(PACK_READY, |p| p.state.as_str());
        match state {
            PACK_READY => return Ok(ev),
            PACK_UNAVAILABLE => return Err(pack_unavailable(None)),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(pack_pending());
        }
        sink(Event::Progress {
            task: TaskId::new("server_pack_wait").with("name", display_name(&ev.view)),
            done: 0,
            total: 0,
            bytes: 0,
        });
        tokio::time::sleep(timing.pack_poll_every).await;
        ev = src.invites.view_fresh(link).await?;
        supported(&ev)?;
    }
}

/// Download the pack into the object cache. Returns where it is, its hashes and its stamp.
async fn download_pack(
    src: &Sources,
    link: &InviteLink,
    ev: &EngineView,
) -> Result<(PathBuf, Hashes, String)> {
    let host = engine_host(ev);
    let url = invite_route(&ev.engine, link, "/pack");
    let stamp = match stamp_of(&ev.view) {
        Some(stamp) => stamp,
        None => etag_stamp(src, &url, &host).await?,
    };
    // Named by stamp (the commit), so an interrupted download resumes with Range and a
    // finished one is reused: one commit is always one file.
    let dest = incoming_path(src, &stamp);
    let request = DownloadRequest::new(url.as_str(), &dest, Hashes::default(), None);
    if let Err(e) = src.downloader.fetch(&request).await {
        return Err(pack_download_error(src, url, &host, e).await);
    }
    let hashes = hash_file(&dest).await?;
    let cached = src.cache.insert_file(&dest, &hashes).await?;
    let _ = tokio::fs::remove_file(&dest).await;
    Ok((cached, hashes, stamp))
}

/// The pack's ETag, for an Engine that doesn't send the commit.
async fn etag_stamp(src: &Sources, url: &Url, host: &str) -> Result<String> {
    let response = src
        .invites
        .http()
        .head(url.clone())
        .send()
        .await
        .map_err(|e| offline(host, e.without_url()))?;
    if let Some(e) = pack_status_error(response.status(), host) {
        return Err(e);
    }
    etag_of(response.headers()).ok_or_else(|| pack_unavailable(Some("no ETag".into())))
}

/// An ETag without its quotes or `W/`.
pub(crate) fn etag_of(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let raw = headers.get(reqwest::header::ETAG)?.to_str().ok()?.trim();
    let raw = raw.strip_prefix("W/").unwrap_or(raw);
    let tag = raw.trim_matches('"');
    (!tag.is_empty() && tag.len() <= 128).then(|| tag.to_string())
}

/// `/pack`'s own failures (a failed build) first, then the usual invite answers.
fn pack_status_error(status: reqwest::StatusCode, host: &str) -> Option<LauncherError> {
    match status.as_u16() {
        s @ (400 | 413 | 502) => Some(pack_unavailable(Some(format!("status {s}")))),
        _ => status_error(status, host),
    }
}

/// What a downloader failure on `/pack` means for the player. A failed build (502, 413 or 400)
/// carries the Engine's `data.code`, read with one small extra request.
async fn pack_download_error(src: &Sources, url: Url, host: &str, e: LauncherError) -> LauncherError {
    let status: Option<u16> = e.params.get("status").and_then(|s| s.parse().ok());
    match (e.code, status) {
        (_, Some(404)) => gone(host),
        (_, Some(429)) => rate_limited(host),
        (_, Some(s @ (400 | 413 | 502))) => {
            let reason = src.invites.error_code(url).await;
            pack_unavailable(Some(reason.unwrap_or_else(|| format!("status {s}"))))
        }
        (ErrorCode::HostUnreachable, _) => offline(host, "the pack download couldn't connect"),
        (ErrorCode::DownloadRejected | ErrorCode::HostTrouble, Some(s)) => host_trouble(host, s),
        _ => e,
    }
}

/// `cache/incoming/perland-<stamp>.mrpack`, next to the object store.
fn incoming_path(src: &Sources, stamp: &str) -> PathBuf {
    let plain = (1..=80).contains(&stamp.len())
        && !stamp.starts_with('.')
        && stamp
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    let name = if plain {
        stamp.to_string()
    } else {
        sha1_hex(stamp.as_bytes())
    };
    src.cache
        .dir()
        .with_file_name("incoming")
        .join(format!("perland-{name}.mrpack"))
}

/// sha1 and sha512 of a file, read once on the blocking pool.
async fn hash_file(path: &Path) -> Result<Hashes> {
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<Hashes> {
        let mut file = std::fs::File::open(&p)?;
        let (mut s1, mut s512) = (sha1::Sha1::new(), sha2::Sha512::new());
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            s1.update(&buf[..n]);
            s512.update(&buf[..n]);
        }
        Ok(Hashes {
            sha1: Some(hex::encode(s1.finalize())),
            sha512: Some(hex::encode(s512.finalize())),
            ..Hashes::default()
        })
    })
    .await
    .map_err(join_error)?
    .map_err(LauncherError::from)
}

/// The cheap check against what is installed, using the shared view. It never fails: a failure
/// is `Unavailable`, and the installed version plays.
pub async fn check(
    src: &Sources,
    invite: &str,
    engine: &str,
    installed: &DesiredState,
) -> UpdateCheck {
    let link = match InviteLink::parse(invite) {
        Ok(link) => link,
        Err(e) => return UpdateCheck::Unavailable(e),
    };
    src.invites.remember_engine(&link, engine);
    match check_inner(src, &link, installed).await {
        Ok(check) => check,
        Err(e) => UpdateCheck::Unavailable(check_failed(scrub(e, link.token()))),
    }
}

async fn check_inner(
    src: &Sources,
    link: &InviteLink,
    installed: &DesiredState,
) -> Result<UpdateCheck> {
    let ev = src.invites.view(link).await?;
    supported(&ev)?;
    let stamp = match stamp_of(&ev.view) {
        Some(stamp) => stamp,
        None => {
            // An Engine without `pack.commit`: ask whether the pack's ETag moved on.
            let host = engine_host(&ev);
            let response = src
                .invites
                .http()
                .head(invite_route(&ev.engine, link, "/pack"))
                .header(
                    reqwest::header::IF_NONE_MATCH,
                    format!("\"{}\"", installed.stamp),
                )
                .send()
                .await
                .map_err(|e| offline(&host, e.without_url()))?;
            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(UpdateCheck::UpToDate);
            }
            if let Some(e) = pack_status_error(response.status(), &host) {
                return Err(e);
            }
            match etag_of(response.headers()) {
                Some(tag) => tag,
                None => return Ok(UpdateCheck::UpToDate),
            }
        }
    };
    if stamp == installed.stamp {
        return Ok(UpdateCheck::UpToDate);
    }
    Ok(UpdateCheck::Available {
        source: source_of(link, &ev),
        stamp,
        version: version_of(&ev.view),
    })
}
```

- [ ] **Step 7: The CLI's `pack add`**

`perland::open` returns a view-only `DesiredState`, so `perland-cli pack add` must not sync that state directly. In `crates/perland-cli/src/main.rs`, in the `PackCmd::Add` arm, replace the `let outcome = syncer.sync(…).await?;` statement with:

```rust
            // A server's pack is resolved by its first sync (perland::open reads only the invite).
            let outcome = if matches!(instance.source, SourceRef::Perland { .. }) {
                syncer
                    .ensure(&store, &instance.id, &progress_sink())
                    .await?
                    .outcome
            } else {
                syncer
                    .sync(&store, &instance.id, &desired, &progress_sink())
                    .await?
            };
```

- [ ] **Step 8: Run them to verify they pass**

Run: `cargo test -p launcher-core && cargo build -p perland-cli`
Expected: PASS and OK. Clippy may flag `too_many_lines`: it isn't enabled in this workspace, so nothing to do.

- [ ] **Step 9: Commit**

```bash
git add crates/launcher-core crates/perland-cli/src/main.rs
git commit -m "feat(core): the perland pack source: a server's commit or vanilla, pending poll, ETag and HEAD fallbacks

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Pass the wait to the run: `resolve_with` through Plan 3b's `find_update` and `ensure_locked`

Plan 3b's `Syncer::find_update` and the never-synced branch of `ensure_locked` call `Sources::resolve`, which has no sink. This task adds a sink-taking path, so a run that waits on a building pack can say so. The callers' signatures stay as they are; only additive APIs are added. It ends with the whole core flow against wiremock.

**Files:**
- Modify: `crates/launcher-core/src/sources/mod.rs` (`resolve_with`)
- Modify: `crates/launcher-core/src/sync/mod.rs` (`find_update_with`; `ensure_locked`'s never-synced branch)
- Test: `crates/launcher-core/tests/invite.rs`

**Interfaces:**
- Consumes `perland::resolve_ref` (Task 5), plus Plan 3b's `find_update` and `ensure_locked` (Task 0 rows 1 and 2).
- Produces:
  - `Sources::resolve_with(&self, source: &SourceRef, sink: &EventSink) -> Result<DesiredState>`.
  - `Syncer::find_update_with(&self, store: &InstanceStore, instance: &Instance, sink: &EventSink) -> Result<Option<PendingUpdate>>`. Plan 3b's `find_update(store, instance)` becomes `find_update_with(…, &noop_sink())`.
  - `ensure_locked` resolves a never-synced source with its own `sink`.

- [ ] **Step 1: Write the failing test**

Add to `crates/launcher-core/tests/invite.rs`. Imports: `launcher_core::{instances::{GameSpec, InstanceStore, UpdatePolicy}, sync::{BoxFuture, GameInstall, SyncOutcome, Syncer}, sources::Sources}` and `std::path::Path` (`InviteClient` and `InviteTiming` are imported since Task 3).

```rust
/// Installs nothing: these tests are about which pack lands, not about Minecraft.
struct NoInstall;

impl GameInstall for NoInstall {
    fn install_game<'a>(
        &'a self,
        _game: &'a GameSpec,
        _sink: &'a EventSink,
    ) -> BoxFuture<'a, launcher_core::Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// The wiremock sources with no view sharing, so a redeploy is seen at once.
fn unshared(server: &MockServer, root: &Path) -> Sources {
    let base = sources(server, root);
    Sources {
        invites: InviteClient::new(base.http.clone()).with_timing(InviteTiming {
            view_ttl: Duration::ZERO,
            ..fast()
        }),
        ..base
    }
}

#[tokio::test]
async fn a_server_instance_installs_on_first_play_and_follows_a_redeploy() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    // The first view says the pack is still building; then it is ready.
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(modded_view(Some("c1"), "pending"))))
        .with_priority(1)
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;

    let syncer = Syncer::new(std::sync::Arc::new(NoInstall), unshared(&server, tmp.path()), tmp.path());
    let store = InstanceStore::new(tmp.path().join("instances"));
    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (instance, _) = syncer.add_pack(&store, &input).await.unwrap();
    assert_eq!(instance.update_policy, UpdatePolicy::Auto);
    assert!(matches!(instance.source, SourceRef::Perland { .. }));
    assert_eq!(hits(&server, &route("/pack")).await, 0, "Add downloads nothing");

    // First Play: the never-synced branch resolves with the run's sink.
    let (sink, seen) = collect();
    let ensured = syncer.ensure(&store, &instance.id, &sink).await.unwrap();
    assert!(matches!(ensured.outcome, SyncOutcome::Done(_)));
    assert!(store.game_dir(&instance.id).join("config/server.txt").is_file());
    assert_eq!(syncer.installed(&store, &instance.id).await.unwrap().stamp, "c1");
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Progress { task, .. } if task.key == "server_pack_wait")));

    // The owner redeploys: a new commit and a new pack.
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c2"), "ready")).await;
    let dir = tempfile::tempdir().unwrap();
    mount_pack(&server, mrpack(dir.path(), "1.0.5"), "c2").await;
    let fresh = store.load(&instance.id).await.unwrap();
    let update = syncer
        .find_update_with(&store, &fresh, &noop_sink())
        .await
        .unwrap()
        .expect("the new commit is an update");
    assert_eq!(update.desired.stamp, "c2");
    assert_eq!(update.desired.pack_version, "1.0.5");
    syncer.ensure(&store, &instance.id, &noop_sink()).await.unwrap();
    assert_eq!(syncer.installed(&store, &instance.id).await.unwrap().stamp, "c2");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p launcher-core --test invite a_server_instance_installs`
Expected: compile error (`find_update_with` doesn't exist). Without Step 3, the progress assertion would also fail, because `resolve` passes no sink.

- [ ] **Step 3: Implement**

In `sources/mod.rs`, inside `impl Sources`, after `resolve`:

```rust
    /// `resolve`, telling `sink` about long waits (a server pack the Engine is still building).
    pub async fn resolve_with(&self, source: &SourceRef, sink: &EventSink) -> Result<DesiredState> {
        match source {
            SourceRef::Perland { invite, engine } => {
                perland::resolve_ref(self, invite, engine, sink).await
            }
            other => self.resolve(other).await,
        }
    }
```

Import `crate::events::EventSink` there.

In `sync/mod.rs` (Plan 3b's code):
- Rename the body of `find_update` into a new method, `find_update_with(&self, store, instance, sink: &EventSink)`, in which the resolve line becomes:

```rust
        let desired = self
            .sources
            .resolve_with(&source, sink)
            .await
            .map_err(update_check_failed)?;
```

- `find_update` becomes:

```rust
    /// `find_update_with` without progress.
    pub async fn find_update(
        &self,
        store: &InstanceStore,
        instance: &Instance,
    ) -> Result<Option<PendingUpdate>> {
        self.find_update_with(store, instance, &crate::events::noop_sink())
            .await
    }
```

- In `ensure_locked`'s never-synced branch, change `self.sources.resolve(&instance.source).await?` to `self.sources.resolve_with(&instance.source, sink).await?`.

`find_update`'s `update_check_failed` wrapper would hide `invite_gone` when resolving an *update* fails. That is acceptable: the check that came first already succeeded, so the link was alive a moment ago. The plain check path (`Sources::check`) keeps `invite_gone`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-core`
Expected: PASS, including Plan 3b's own `tests/sync.rs`.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core
git commit -m "feat(sync): resolve_with passes a server pack's wait to Play; find_update_with

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: The token sweep

One test runs every failure path the launcher can hit with an invite and checks that the token appears in none of them: not in the JSON the UI gets, not in `Debug`, not in `Display`.

**Files:**
- Test: `crates/launcher-core/tests/invite.rs`

**Interfaces:**
- Consumes everything from Tasks 2–6. Produces nothing new. A failure here means a code path from those tasks leaks; fix it there, usually by passing it through `scrub` or replacing a URL with the host.

- [ ] **Step 1: Write the test**

```rust
fn assert_token_free(label: &str, e: &launcher_core::LauncherError) {
    for text in [
        serde_json::to_string(e).unwrap(),
        format!("{e:?}"),
        e.to_string(),
    ] {
        assert!(!text.contains(TOKEN), "{label} leaks the token: {text}");
    }
}

/// A fresh server with config.json, plus whatever `mount` adds.
async fn engine_with<F, Fut>(mount: F) -> MockServer
where
    F: FnOnce(MockServer) -> Fut,
    Fut: std::future::Future<Output = MockServer>,
{
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount(server).await
}

async fn answer(server: MockServer, at: String, status: u16, body: Value) -> MockServer {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn no_failure_path_leaks_the_token() {
    let tmp = tempfile::tempdir().unwrap();
    let mut errors: Vec<(String, launcher_core::LauncherError)> = Vec::new();

    // config.json
    for body in ["nope", r#"{"engineUrl":"http://engine.example.com"}"#] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/config.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        errors.push((format!("config {body}"), client().view(&link(&server)).await.unwrap_err()));
    }
    let empty = MockServer::start().await;
    errors.push(("config 404".into(), client().view(&link(&empty)).await.unwrap_err()));

    // The view.
    for status in [404, 429, 500] {
        let server = engine_with(|s| answer(s, route(""), status, failure(None))).await;
        errors.push((format!("view {status}"), client().view(&link(&server)).await.unwrap_err()));
    }
    let offline = InviteLink::parse(&format!("http://127.0.0.1:1/{TOKEN}")).unwrap();
    errors.push(("offline".into(), client().view(&offline).await.unwrap_err()));

    // Networks and unusable views.
    let server = engine_with(|s| async move {
        mount_view(&s, network_view()).await;
        s
    })
    .await;
    errors.push((
        "network".into(),
        perland::open(&sources(&server, tmp.path()), &link(&server)).await.unwrap_err(),
    ));

    // The pack.
    for (status, code) in [
        (404, None),
        (400, Some("invalid_modpack")),
        (413, Some("export_too_large")),
        (429, None),
        (502, Some("mod_download_failed")),
        (503, None),
    ] {
        let server = engine_with(|s| async move {
            mount_view(&s, modded_view(Some("c1"), "ready")).await;
            answer(s, route("/pack"), status, failure(code)).await
        })
        .await;
        errors.push((
            format!("pack {status}"),
            perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
                .await
                .unwrap_err(),
        ));
    }
    for state in ["pending", "unavailable"] {
        let server = engine_with(|s| async move {
            mount_view(&s, modded_view(Some("c1"), state)).await;
            s
        })
        .await;
        errors.push((
            format!("pack {state}"),
            perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
                .await
                .unwrap_err(),
        ));
    }

    // Checks.
    for status in [404, 500] {
        let server = engine_with(|s| answer(s, route(""), status, failure(None))).await;
        match sources(&server, tmp.path()).check(&source(&server), &installed("c0")).await {
            UpdateCheck::Unavailable(e) => errors.push((format!("check {status}"), e)),
            other => panic!("{other:?}"),
        }
    }

    assert!(errors.len() >= 18, "{}", errors.len());
    for (label, e) in &errors {
        assert_token_free(label, e);
    }

    // Debug of every type that holds the invite.
    let l = InviteLink::parse(&format!("https://play.example.com/{TOKEN}")).unwrap();
    let source = SourceRef::Perland {
        invite: l.canonical(),
        engine: "https://api.example.com/".into(),
    };
    for text in [
        format!("{l:?}"),
        l.to_string(),
        format!("{source:?}"),
        format!("{:?}", PackInput::Perland { link: l.clone() }),
        format!("{:?}", client()),
        format!(
            "{:?}",
            UpdateCheck::Available {
                source: source.clone(),
                stamp: "c1".into(),
                version: "1".into()
            }
        ),
    ] {
        assert!(!text.contains(TOKEN), "{text}");
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p launcher-core --test invite no_failure_path_leaks_the_token`
Expected: PASS. If a path leaks, the message names it. Route that path's error through `scrub` (Task 5), or name the host instead of the URL (Task 3), then rerun.

- [ ] **Step 3: Commit**

```bash
git add crates/launcher-core/tests/invite.rs
git commit -m "test(core): sweep every invite failure path for the token

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Joining in the launch command, and redacting the invite token

**Files:**
- Modify: `crates/launcher-core/src/launch/mod.rs` (`LaunchSpec.join`, `declares_feature`, `QUICK_PLAY_MULTIPLAYER`, `build_command`)
- Modify: `crates/launcher-core/src/play.rs` (`StartSpec.join`, `redactor_for`)
- Modify: `crates/launcher-app/src/runner.rs` (`join: None` in `StartSpec`; Task 11 passes the real address)
- Modify: `crates/perland-cli/src/main.rs` (`join: None` in `StartSpec`)

**Interfaces:**
- Consumes `ServerAddress` (Task 4), `SourceRef::Perland` (Task 5) and `InviteLink::token` (Task 2).
- Produces:
  - `LaunchSpec.join: Option<&'a ServerAddress>`.
  - `pub const QUICK_PLAY_MULTIPLAYER: &str = "is_quick_play_multiplayer"`.
  - `pub fn declares_feature(json: &VersionJson, feature: &str) -> bool`.
  - `StartSpec.join: Option<&'a ServerAddress>`.
  - `pub fn redactor_for(spec: &StartSpec) -> Redactor`: masks the access token and, for a `Perland` source, the invite token. `play::start` uses it.
- Argument style, chosen from the version JSON:
  - **It declares an `is_quick_play_multiplayer` rule (1.20+):** that feature is enabled in the `RuleContext`, and `${quickPlayMultiplayer}` = `ServerAddress::display()`.
  - **It doesn't (1.13–1.19, legacy `minecraftArguments`):** `--server <host> --port <port>` is appended after the game arguments.
  - Loader children inherit vanilla's rules through `meta::merge`.

- [ ] **Step 1: Write the failing tests**

In `launch/mod.rs` `mod tests`:
- Add the imports `crate::{instances::ServerAddress, meta::mojang::ArgValue, test_support::forge_resolved}`.
- Change the existing `build` helper's body to call a new helper, so the existing tests are untouched:

```rust
    fn build(resolved: &ResolvedVersion, platform: Platform) -> LaunchCommand {
        build_joining(resolved, platform, None)
    }

    fn build_joining(
        resolved: &ResolvedVersion,
        platform: Platform,
        join: Option<&ServerAddress>,
    ) -> LaunchCommand {
        let dirs = LauncherDirs::new("/pl");
        let session = offline_account("Steve").unwrap().offline_session();
        build_command(&LaunchSpec {
            dirs: &dirs,
            resolved,
            game_dir: Path::new("/pl/instances/x/minecraft"),
            java: Path::new("/pl/java/x/bin/java"),
            session: &session,
            memory_mb: 4096,
            platform,
            launcher_version: "0.1.0",
            join,
        })
        .unwrap()
    }

    fn server() -> ServerAddress {
        ServerAddress::new("play.example.com", 25566).unwrap()
    }

    fn vanilla(file: &str, id: &str) -> ResolvedVersion {
        ResolvedVersion {
            vanilla_id: id.into(),
            json: version(file),
            forge: None,
        }
    }

    #[test]
    fn quick_play_joins_on_1_20_1() {
        let cmd = build_joining(&vanilla("1.20.1.json", "1.20.1"), WINDOWS_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--quickPlayMultiplayer"), "play.example.com:25566");
        assert!(!cmd.args.contains(&"--server".to_string()));
        assert!(
            !cmd.args.contains(&"--quickPlayPath".to_string()),
            "only the multiplayer feature is on"
        );
        assert!(!cmd.args.iter().any(|a| a.contains("${")), "{:?}", cmd.args);
    }

    #[test]
    fn quick_play_brackets_an_ipv6_address() {
        let v6 = ServerAddress::new("2001:db8::1", 25565).unwrap();
        let cmd = build_joining(&vanilla("1.20.1.json", "1.20.1"), WINDOWS_X64, Some(&v6));
        assert_eq!(arg_after(&cmd.args, "--quickPlayMultiplayer"), "[2001:db8::1]:25565");
    }

    #[test]
    fn legacy_versions_join_with_server_and_port() {
        let cmd = build_joining(&vanilla("1.12.2.json", "1.12.2"), LINUX_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--server"), "play.example.com");
        assert_eq!(arg_after(&cmd.args, "--port"), "25566");
        assert!(!cmd.args.iter().any(|a| a.starts_with("--quickPlay")));
        let forge = ResolvedVersion {
            vanilla_id: "1.12.2".into(),
            json: merge(
                version("1.12.2.json"),
                version("forge-1.12.2-14.23.5.2860-version.json"),
            ),
            forge: None,
        };
        let cmd = build_joining(&forge, LINUX_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--server"), "play.example.com");
    }

    #[test]
    fn modern_versions_without_quick_play_join_with_server_and_port() {
        let mut json = version("1.20.1.json");
        json.arguments.as_mut().unwrap().game.retain(|arg| {
            !matches!(arg, ArgValue::Conditional { rules, .. }
                if rules.iter().any(|r| r.features.as_ref().is_some_and(|f| f.keys().any(|k| k.contains("quick_play")))))
        });
        assert!(!declares_feature(&json, QUICK_PLAY_MULTIPLAYER));
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json,
            forge: None,
        };
        let cmd = build_joining(&resolved, WINDOWS_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--server"), "play.example.com");
        assert_eq!(arg_after(&cmd.args, "--port"), "25566");
        assert!(!cmd.args.contains(&"--quickPlayMultiplayer".to_string()));
    }

    #[test]
    fn loaders_inherit_quick_play_through_the_merge() {
        let fabric = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: merge(
                version("1.20.1.json"),
                version("fabric-1.20.1-0.16.14.json"),
            ),
            forge: None,
        };
        let cmd = build_joining(&fabric, WINDOWS_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--quickPlayMultiplayer"), "play.example.com:25566");
        let cmd = build_joining(&forge_resolved(), WINDOWS_X64, Some(&server()));
        assert_eq!(arg_after(&cmd.args, "--quickPlayMultiplayer"), "play.example.com:25566");
    }

    #[test]
    fn without_a_server_nothing_is_added() {
        for resolved in [vanilla("1.20.1.json", "1.20.1"), vanilla("1.12.2.json", "1.12.2")] {
            let cmd = build(&resolved, WINDOWS_X64);
            assert!(!cmd.args.contains(&"--server".to_string()));
            assert!(!cmd.args.iter().any(|a| a.starts_with("--quickPlay")));
        }
    }
```

In `play.rs` `mod tests`, add the import `crate::instances::SourceRef`, then:

```rust
    #[tokio::test]
    async fn the_redactor_masks_a_server_instances_invite_token() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        let store = InstanceStore::new(dirs.instances());
        let mut instance = store
            .create(
                "Survival",
                GameSpec {
                    minecraft: "1.20.1".into(),
                    loader: None,
                },
            )
            .await
            .unwrap();
        let token = "B".repeat(43);
        let invite = format!("https://play.example.com/{token}");
        instance.source = SourceRef::Perland {
            invite: invite.clone(),
            engine: "https://api.example.com/".into(),
        };
        let prepared = Prepared {
            resolved: ResolvedVersion {
                vanilla_id: "1.20.1".into(),
                json: version("1.20.1.json"),
                forge: None,
            },
            java: tmp.path().join("java.exe"),
        };
        let session = offline_account("Steve").unwrap().offline_session();
        let game_dir = store.game_dir(&instance.id);
        let spec = StartSpec {
            dirs: &dirs,
            game_dir: &game_dir,
            instance: &instance,
            prepared: &prepared,
            session: &session,
            platform: Platform::current(),
            launcher_version: "0.1.0",
            join: None,
        };
        let line = redactor_for(&spec).redact(&format!("[Server] joined via {invite}"));
        assert!(!line.contains(&token), "{line}");
        assert!(line.contains("<redacted>"), "{line}");
    }
```

Add `join: None,` to the two existing `StartSpec { … }` literals in `play.rs`'s tests.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib launch:: play::`
Expected: compile errors (`join`, `declares_feature`, `redactor_for` don't exist).

- [ ] **Step 3: Implement the arguments**

In `launch/mod.rs`:
- Import `crate::instances::ServerAddress` and `crate::meta::mojang::{ArgValue, VersionJson}` (next to `LEGACY_JVM_ARGS`).
- Add the field to `LaunchSpec`:

```rust
    /// Join this server once the game starts: quick play where the version supports it,
    /// `--server`/`--port` elsewhere.
    pub join: Option<&'a ServerAddress>,
```

- Add above `build_command`:

```rust
/// The feature a version JSON declares when it takes `--quickPlayMultiplayer` (1.20 and later).
pub const QUICK_PLAY_MULTIPLAYER: &str = "is_quick_play_multiplayer";

/// Does the version (with its loader merged in) declare `feature` in a game-argument rule?
pub fn declares_feature(json: &VersionJson, feature: &str) -> bool {
    json.arguments.as_ref().is_some_and(|a| {
        a.game.iter().any(|arg| match arg {
            ArgValue::Conditional { rules, .. } => rules
                .iter()
                .any(|r| r.features.as_ref().is_some_and(|f| f.contains_key(feature))),
            ArgValue::Plain(_) => false,
        })
    })
}
```

- In `build_command`:
  - Replace `let ctx = RuleContext::new(spec.platform);` and `let json = &spec.resolved.json;` with:

```rust
    let json = &spec.resolved.json;
    let quick_play = spec.join.is_some() && declares_feature(json, QUICK_PLAY_MULTIPLAYER);
    let ctx = if quick_play {
        RuleContext::new(spec.platform).with_feature(QUICK_PLAY_MULTIPLAYER)
    } else {
        RuleContext::new(spec.platform)
    };
```

  - Make `vars` mutable (`let mut vars: HashMap<&str, String> = …`) and right after it add:

```rust
    if let (true, Some(address)) = (quick_play, spec.join) {
        vars.insert("quickPlayMultiplayer", address.display());
    }
```

  - After the `match (&json.arguments, &json.minecraft_arguments) { … }` that adds the game arguments, and before `Ok(LaunchCommand { … })`, add:

```rust
    if let (false, Some(address)) = (quick_play, spec.join) {
        // 1.13–1.19 and legacy `minecraftArguments`: the older flags every version still reads.
        args.extend([
            "--server".to_string(),
            address.host.clone(),
            "--port".to_string(),
            address.port.to_string(),
        ]);
    }
```

- [ ] **Step 4: Implement `StartSpec.join` and the redactor**

In `play.rs`:
- Import `crate::{instances::{ServerAddress, SourceRef}, invite::InviteLink}`.
- Add to `StartSpec`:

```rust
    /// The server to join, if any (a server instance played with "join").
    pub join: Option<&'a ServerAddress>,
```

- In `command`, pass `join: spec.join,` in the `LaunchSpec` literal.
- Add:

```rust
/// Scrubs the game's output and log: the access token, the home folder, and a server
/// instance's invite token.
pub fn redactor_for(spec: &StartSpec) -> Redactor {
    let mut redactor =
        Redactor::for_current_user().with_secret(spec.session.access_token.clone());
    if let SourceRef::Perland { invite, .. } = &spec.instance.source {
        if let Ok(link) = InviteLink::parse(invite) {
            redactor = redactor.with_secret(link.token().to_string());
        }
    }
    redactor
}
```

- In `start`, replace `let redactor = Redactor::for_current_user().with_secret(spec.session.access_token.clone());` with `let redactor = redactor_for(spec);`.

Add `join: None,` to the `StartSpec` literals in `crates/launcher-app/src/runner.rs` and `crates/perland-cli/src/main.rs`.

- [ ] **Step 5: Run them to verify they pass**

Run: `cargo test --workspace`
Expected: PASS. The existing `modern_command_on_windows`, `legacy_command_on_linux` and `fabric_command_uses_knot` still pass unchanged.

- [ ] **Step 6: Commit**

```bash
git add crates/launcher-core/src/launch/mod.rs crates/launcher-core/src/play.rs crates/launcher-app/src/runner.rs crates/perland-cli/src/main.rs
git commit -m "feat(launch): join a server with quick play or --server/--port; redact a server instance's invite token

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: App: preview and add a server, and `pack_add` hands invite links over

**Files:**
- Create: `crates/launcher-app/src/servers.rs`
- Modify: `crates/launcher-app/src/lib.rs` (`mod servers;`)
- Modify: `crates/launcher-app/src/dto.rs` (`JoinPreview` and parts)
- Modify: `crates/launcher-app/src/runner.rs` (`invite_view`, `invite_icon`)
- Modify: `crates/launcher-app/src/testing.rs` (`FakeRunner` invite fields, the `Perland` arm of `add_pack`, fixtures)
- Modify: `crates/launcher-app/src/packs.rs` (`pack_add` delegation)

**Interfaces:**
- Consumes Plan 3b's `Runner::add_pack`, `FakeRunner` and `pack_add` (Task 0 rows 8, 10, 11), plus `EngineView`, `InviteLink`, `ServerLink` and `perland::supported` (Tasks 2–5).
- Produces on `trait Runner`:

```rust
    /// The invite's view and its Engine, shared for 15 s by every caller. `engine` seeds the
    /// Engine an instance saved, so no `config.json` is needed.
    fn invite_view(&self, link: InviteLink, engine: Option<String>) -> BoxFuture<Result<EngineView>>;
    /// The invite's icon PNG, or `None`.
    fn invite_icon(&self, link: InviteLink) -> BoxFuture<Option<Vec<u8>>>;
```

- Produces in `launcher_app::dto` (all `Serialize`, camelCase):
  - `ServerPlayers { online: u32, max: Option<u32> }` with `From<&InvitePlayers>`.
  - `ServerGame { version, loader, loader_version: Option<String> }`.
  - `ServerPack { name: String, version: Option<String>, mod_count: u32, state: String }`.
  - `ServerTeam { display_name: String, accent: Option<String> }`.
  - `JoinPreview { host, name, motd, kind, status: Option<ServerStatus>, players: Option<ServerPlayers>, game: ServerGame, pack: Option<ServerPack>, whitelisted: bool, team: Option<ServerTeam>, address: Option<ServerAddress>, icon: Option<String>, existing: Option<String> }`, with `JoinPreview::new(&InviteLink, &EngineView, icon, existing)`.
- Produces on `AppCore` (in `servers.rs`):
  - `async join_preview(&self, link: &str) -> Result<JoinPreview>`: writes nothing to disk.
  - `async join_add(&self, link: &str) -> Result<Instance>`: dedupes by canonical invite, otherwise `add_pack(PackInput::Perland)` plus `ServerLink`, with the policy `Auto`.
  - `pub(crate) async fn instance_for_invite(&self, &InviteLink) -> Result<Option<Instance>>`.
  - `pub(crate) fn server_source(&Instance) -> Option<(InviteLink, String)>` (a free function).
- `pack_add(PackAddRequest::Link { url })` with an invite link returns `join_add(url)`.
- Test fixtures in `testing.rs`:
  - `pub(crate) fn invite(c: char) -> String`: `https://play.example.com/` + 43 × `c`.
  - `pub(crate) fn server_view(name: &str) -> EngineView`: a modded, online server at `play.example.com:25566`, team accent violet, commit `c1`.
  - `pub(crate) fn serve(runner: &FakeRunner, invite: &str, view: Result<EngineView>)`.
- New `FakeRunner` pub fields:
  - `views: Mutex<HashMap<String, Result<EngineView>>>`, keyed by canonical invite; a missing one answers `invite_gone`;
  - `view_reads: AtomicUsize`;
  - `engine_hints: Mutex<Vec<Option<String>>>`;
  - `icon: Mutex<Option<Vec<u8>>>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-app/src/servers.rs` with the module doc and the tests:

```rust
//! Joining Perland servers by invite link (join-by-link spec §2.2): previewing an invite,
//! adding it as an instance, its live status, and moving it to a new link. Tauri only forwards
//! these commands.

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::{ServerStatus, SourceRef, UpdatePolicy},
        ErrorCode,
    };

    use crate::{
        app::tests::core,
        testing::{invite, serve, server_view},
    };

    #[tokio::test]
    async fn join_preview_reads_the_invite_and_writes_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        *runner.icon.lock().unwrap() = Some(b"\x89PNG\r\n\x1a\nicon".to_vec());
        let preview = core.join_preview(&invite('A')).await.unwrap();
        assert_eq!(preview.host, "play.example.com");
        assert_eq!(preview.name, "survival");
        assert_eq!(preview.kind, "stack");
        assert_eq!(preview.status, Some(ServerStatus::Online));
        assert_eq!(preview.players.as_ref().unwrap().max, Some(20));
        assert_eq!(preview.pack.as_ref().unwrap().mod_count, 2);
        assert_eq!(preview.team.as_ref().unwrap().accent.as_deref(), Some("violet"));
        assert_eq!(
            preview.address.as_ref().unwrap().display(),
            "play.example.com:25566"
        );
        assert!(preview
            .icon
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert_eq!(preview.existing, None);
        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert!(runner.added.lock().unwrap().is_empty(), "nothing is added");
    }

    #[tokio::test]
    async fn join_preview_refuses_networks_with_their_address() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut network = server_view("hub");
        network.view.kind = "network".into();
        network.view.pack = None;
        network.view.address.port = 25577;
        serve(&runner, &invite('A'), Ok(network));
        let err = core.join_preview(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteUnsupported);
        assert_eq!(err.params["address"], "play.example.com:25577");
    }

    #[tokio::test]
    async fn junk_is_not_an_invite() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        for bad in ["https://play.example.com/short", "modrinth:sky", ""] {
            assert_eq!(
                core.join_preview(bad).await.unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
            assert_eq!(
                core.join_add(bad).await.unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn join_add_creates_an_automatic_server_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let instance = core.join_add(&invite('A')).await.unwrap();
        assert_eq!(instance.update_policy, UpdatePolicy::Auto);
        assert!(
            matches!(&instance.source, SourceRef::Perland { invite: i, .. } if *i == invite('A'))
        );
        let server = instance.server.clone().unwrap();
        assert_eq!(server.name, "survival");
        assert_eq!(server.accent.as_deref(), Some("violet"));
        assert_eq!(server.last_status, Some(ServerStatus::Online));
        assert_eq!(server.address.unwrap().display(), "play.example.com:25566");
        assert!(!server.gone);
        assert_eq!(core.instances.load(&instance.id).await.unwrap(), instance);
        let preview = core.join_preview(&invite('A')).await.unwrap();
        assert_eq!(preview.existing.as_deref(), Some(instance.id.as_str()));
    }

    #[tokio::test]
    async fn join_add_dedupes_case_and_slash_variants() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let first = core.join_add(&invite('A')).await.unwrap();
        let shouted = format!(
            "  {}/  ",
            invite('A').replace("play.example.com", "PLAY.Example.com:443")
        );
        let second = core.join_add(&shouted).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(runner.added.lock().unwrap().len(), 1);
    }
}
```

In `packs.rs` `mod tests`, add the import `crate::testing::{invite, serve, server_view}`, then:

```rust
    #[tokio::test]
    async fn pack_add_hands_invite_links_to_join_add() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let added = core
            .pack_add(PackAddRequest::Link { url: invite('A') })
            .await
            .unwrap();
        assert!(matches!(added.source, SourceRef::Perland { .. }));
        assert!(added.server.is_some());
        let again = core
            .pack_add(PackAddRequest::Link { url: invite('A') })
            .await
            .unwrap();
        assert_eq!(again.id, added.id);
        assert_eq!(runner.added.lock().unwrap().len(), 1);
    }
```

Add `mod servers;` to `lib.rs`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib servers:: packs::`
Expected: compile errors (`join_preview`, `serve`, `server_view` don't exist).

- [ ] **Step 3: The seam**

In `runner.rs`:
- Import `launcher_core::invite::{EngineView, InviteLink}`.
- Add the two trait methods from Interfaces.
- Implement them for `CoreRunner`:

```rust
    fn invite_view(
        &self,
        link: InviteLink,
        engine: Option<String>,
    ) -> BoxFuture<Result<EngineView>> {
        let invites = self.syncer.sources.invites.clone();
        Box::pin(async move {
            if let Some(engine) = engine {
                invites.remember_engine(&link, &engine);
            }
            invites.view(&link).await
        })
    }

    fn invite_icon(&self, link: InviteLink) -> BoxFuture<Option<Vec<u8>>> {
        let invites = self.syncer.sources.invites.clone();
        Box::pin(async move { invites.icon(&link).await })
    }
```

`CoreRunner`'s `syncer` is Plan 3b's `Arc<Syncer>`, and `Syncer.sources.invites` is the process-wide client (Task 5). So `AppCore`, Play's check and `server_status` all share one 15 s cache.

- [ ] **Step 4: The fake and the fixtures**

In `testing.rs`:
- Imports: `launcher_core::{instances::UpdatePolicy, invite::{EngineView, InviteAddress, InviteGame, InviteLink, InvitePack, InvitePlayers, InviteTeam, InviteView, Url}}`.
- Add the four fields from Interfaces to `FakeRunner`, initialized empty (`AtomicUsize::new(0)`) in `FakeRunner::new`.
- In Plan 3b's `add_pack`:
  - create the instance as `"Survival"` for a server: `store.create(if matches!(input, PackInput::Perland { .. }) { "Survival" } else { "Sky Islands" }, …)`;
  - add this arm to the `instance.source = match input { … }`:

```rust
                PackInput::Perland { link } => SourceRef::Perland {
                    invite: link.canonical(),
                    engine: "https://api.example.com/".into(),
                },
```

  - after the match, before `store.save`:

```rust
            if matches!(instance.source, SourceRef::Perland { .. }) {
                instance.update_policy = UpdatePolicy::Auto;
            }
```

- The two seam methods:

```rust
    fn invite_view(
        &self,
        link: InviteLink,
        engine: Option<String>,
    ) -> BoxFuture<Result<EngineView>> {
        self.view_reads.fetch_add(1, Ordering::SeqCst);
        self.engine_hints.lock().unwrap().push(engine);
        let answer = self
            .views
            .lock()
            .unwrap()
            .get(&link.canonical())
            .cloned()
            .unwrap_or_else(|| Err(LauncherError::pack(ErrorCode::InviteGone, "gone")));
        Box::pin(async move { answer })
    }

    fn invite_icon(&self, _link: InviteLink) -> BoxFuture<Option<Vec<u8>>> {
        let icon = self.icon.lock().unwrap().clone();
        Box::pin(async move { icon })
    }
```

- The fixtures:

```rust
/// An invite link with a fake token of 43 × `c`.
pub(crate) fn invite(c: char) -> String {
    format!("https://play.example.com/{}", c.to_string().repeat(43))
}

/// A modded server that is online with 3 of 20 players, at play.example.com:25566.
pub(crate) fn server_view(name: &str) -> EngineView {
    EngineView {
        engine: Url::parse("https://api.example.com").unwrap(),
        view: InviteView {
            kind: "stack".into(),
            name: name.into(),
            motd: "§6Survival§r with friends".into(),
            address: InviteAddress {
                host: Some("play.example.com".into()),
                port: 25566,
                display: Some("play.example.com:25566".into()),
            },
            game: InviteGame {
                kind: "fabric".into(),
                version: Some("1.20.1".into()),
                loader: Some("fabric".into()),
                loader_version: Some("0.16.14".into()),
            },
            status: "online".into(),
            players: Some(InvitePlayers {
                online: 3,
                max: Some(20),
            }),
            pack: Some(InvitePack {
                name: "Survival Pack".into(),
                version: Some("1.0.4".into()),
                minecraft: "1.20.1".into(),
                loader: Some("fabric".into()),
                loader_version: Some("0.16.14".into()),
                mod_count: 2,
                state: "ready".into(),
                zip_state: Some("ready".into()),
                commit: Some("c1".into()),
            }),
            team: InviteTeam {
                display_name: "Acme".into(),
                accent: "violet".into(),
            },
            ..InviteView::default()
        },
    }
}

/// What `invite_view` answers for `invite` from now on.
pub(crate) fn serve(runner: &FakeRunner, invite: &str, view: Result<EngineView>) {
    let key = InviteLink::parse(invite).unwrap().canonical();
    runner.views.lock().unwrap().insert(key, view);
}
```

- [ ] **Step 5: The DTOs**

In `dto.rs`, import `launcher_core::{instances::{accent_of, ServerAddress, ServerStatus}, invite::{non_empty, EngineView, InviteLink, InvitePlayers}}` and add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPlayers {
    pub online: u32,
    /// Absent for networks.
    pub max: Option<u32>,
}

impl From<&InvitePlayers> for ServerPlayers {
    fn from(p: &InvitePlayers) -> Self {
        Self {
            online: p.online,
            max: p.max,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerGame {
    pub version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPack {
    pub name: String,
    pub version: Option<String>,
    pub mod_count: u32,
    /// `ready | pending | unavailable`.
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTeam {
    pub display_name: String,
    /// `cyan | blue | violet | magenta | other`: a small mark, never a second lit element.
    pub accent: Option<String>,
}

/// What Join server shows before anything is written. Holds no token: the host is enough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinPreview {
    /// The invite's host ("From play.example.com").
    pub host: String,
    pub name: String,
    /// May hold `§` colour codes.
    pub motd: String,
    /// `stack` or `network`.
    pub kind: String,
    pub status: Option<ServerStatus>,
    pub players: Option<ServerPlayers>,
    pub game: ServerGame,
    pub pack: Option<ServerPack>,
    pub whitelisted: bool,
    pub team: Option<ServerTeam>,
    pub address: Option<ServerAddress>,
    /// The icon as a `data:image/png` URL (the CSP allows no remote images).
    pub icon: Option<String>,
    /// The instance that already joins this server, if any: the screen offers Open.
    pub existing: Option<String>,
}

fn owned(s: &Option<String>) -> Option<String> {
    non_empty(s).map(str::to_string)
}

impl JoinPreview {
    pub fn new(
        link: &InviteLink,
        ev: &EngineView,
        icon: Option<String>,
        existing: Option<String>,
    ) -> Self {
        let view = &ev.view;
        let display_name = view.team.display_name.trim().to_string();
        let accent = accent_of(&view.team.accent);
        let team = (!display_name.is_empty() || accent.is_some()).then_some(ServerTeam {
            display_name,
            accent,
        });
        JoinPreview {
            host: link.host().to_string(),
            name: view.name.trim().to_string(),
            motd: view.motd.chars().take(512).collect(),
            kind: view.kind.clone(),
            status: view.status(),
            players: view.players.as_ref().map(ServerPlayers::from),
            game: ServerGame {
                version: owned(&view.game.version),
                loader: owned(&view.game.loader),
                loader_version: owned(&view.game.loader_version),
            },
            pack: view.pack.as_ref().map(|p| ServerPack {
                name: p.name.trim().to_string(),
                version: owned(&p.version),
                mod_count: p.mod_count,
                state: p.state.clone(),
            }),
            whitelisted: view.whitelisted,
            team,
            address: ServerAddress::from_view(&view.address, &ev.engine),
            icon,
            existing,
        }
    }
}
```

- [ ] **Step 6: The commands**

In `servers.rs`, above the tests:

```rust
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    instances::{unix_now, Instance, ServerLink, SourceRef, UpdatePolicy},
    invite::InviteLink,
    sources::{perland, PackInput},
    Result,
};

use crate::{app::AppCore, dto::JoinPreview, events::Host, runner::Runner};

/// A server instance's saved invite, and the Engine it last used.
pub(crate) fn server_source(instance: &Instance) -> Option<(InviteLink, String)> {
    match &instance.source {
        SourceRef::Perland { invite, engine } => InviteLink::parse(invite)
            .ok()
            .map(|link| (link, engine.clone())),
        _ => None,
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The instance that already joins `link`'s server, compared by canonical invite.
    pub(crate) async fn instance_for_invite(&self, link: &InviteLink) -> Result<Option<Instance>> {
        let canonical = link.canonical();
        Ok(self.instances.list().await?.into_iter().find(|instance| {
            server_source(instance).is_some_and(|(saved, _)| saved.canonical() == canonical)
        }))
    }

    /// What Join server shows: the view and the icon. Nothing else is downloaded, and nothing
    /// is written.
    pub async fn join_preview(&self, link: &str) -> Result<JoinPreview> {
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        perland::supported(&ev)?;
        let icon = self
            .runner
            .invite_icon(link.clone())
            .await
            .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)));
        let existing = self.instance_for_invite(&link).await?.map(|i| i.id);
        Ok(JoinPreview::new(&link, &ev, icon, existing))
    }

    /// Add the server as an instance, or return the one that already joins it. Nothing
    /// installs: the first Play does that.
    pub async fn join_add(&self, link: &str) -> Result<Instance> {
        let link = InviteLink::parse(link)?;
        if let Some(existing) = self.instance_for_invite(&link).await? {
            return Ok(existing);
        }
        let created = self
            .runner
            .add_pack(PackInput::Perland { link: link.clone() })
            .await?;
        let mut instance = self.instances.load(&created.id).await?;
        instance.update_policy = UpdatePolicy::Auto;
        // The view was just read by `add_pack`, so this is the shared copy.
        if let Ok(ev) = self.runner.invite_view(link, None).await {
            instance.server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now()));
        }
        self.instances.save(&instance).await?;
        Ok(instance)
    }
}
```

In `packs.rs` `pack_add`, change the `Link` arm to:

```rust
            PackAddRequest::Link { url } => match PackInput::parse(&url)? {
                PackInput::MrpackFile { .. } => return Err(link_invalid()),
                // The UI routes invite links to Join server first; this is the same thing.
                PackInput::Perland { .. } => return self.join_add(&url).await,
                input => input,
            },
```

- [ ] **Step 7: Run them to verify they pass**

Run: `cargo test -p launcher-app`
Expected: PASS, Plan 3b's tests included.

- [ ] **Step 8: Commit**

```bash
git add crates/launcher-app
git commit -m "feat(app): join_preview and join_add; pack_add hands invite links to join_add

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: App: live status, relinking, and what `instance_update` may change

**Files:**
- Modify: `crates/launcher-app/src/servers.rs`
- Modify: `crates/launcher-app/src/dto.rs` (`ServerStatusView`)
- Modify: `crates/launcher-app/src/app.rs` (`instance_update`)

**Interfaces:**
- Consumes `server_source`, `instance_for_invite`, the fixtures and the `FakeRunner` fields from Task 9.
- Produces:
  - `dto::ServerStatusView { status: Option<ServerStatus>, players: Option<ServerPlayers>, whitelisted: bool, pack_state: Option<String>, gone: bool, address: Option<ServerAddress> }` (camelCase).
  - `AppCore::server_status(&self, id) -> Result<ServerStatusView>`:
    - it reads the view through the shared cache, seeded with the saved Engine, and takes no lock;
    - a success refreshes `ServerLink`;
    - `invite_gone` returns `Ok` with `gone: true` and the last known address;
    - any other failure (`invite_rate_limited`, `offline`, `host_trouble`) is `Err`, and the UI backs off.
  - `AppCore::server_relink(&self, id, link) -> Result<Instance>`: rewrites `source` and `ServerLink`, keeps the files, sets the policy to `Auto`. The next Play syncs.
  - `pub(crate) async fn update_server(&self, id, change: impl FnOnce(&mut Option<ServerLink>)) -> Result<Option<ServerLink>>`: reloads, changes, and saves only when different.
  - `pub(crate) async fn mark_server_gone(&self, id)` and `pub(crate) fn apply_view(&mut Option<ServerLink>, &EngineView)`.
  - `instance_update` keeps `source` and `server` from disk. A server instance stays `Auto` whatever the UI sends.

- [ ] **Step 1: Write the failing tests**

In `servers.rs` `mod tests`, extend the imports with `launcher_core::LauncherError` and `crate::dto::NewInstance`, then add:

```rust
    #[tokio::test]
    async fn server_status_refreshes_the_saved_link() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let mut asleep = server_view("survival");
        asleep.view.status = "sleeping".into();
        asleep.view.players = None;
        serve(&runner, &invite('A'), Ok(asleep));
        let status = core.server_status(&id).await.unwrap();
        assert_eq!(status.status, Some(ServerStatus::Sleeping));
        assert!(status.players.is_none() && !status.gone);
        assert_eq!(status.pack_state.as_deref(), Some("ready"));
        assert_eq!(
            core.instances.load(&id).await.unwrap().server.unwrap().last_status,
            Some(ServerStatus::Sleeping)
        );
        assert_eq!(
            runner.engine_hints.lock().unwrap().last().cloned().flatten().as_deref(),
            Some("https://api.example.com/"),
            "the saved Engine seeds the shared client"
        );
    }

    #[tokio::test]
    async fn a_dead_link_is_marked_and_keeps_its_address() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        serve(
            &runner,
            &invite('A'),
            Err(LauncherError::pack(ErrorCode::InviteGone, "gone")),
        );
        let status = core.server_status(&id).await.unwrap();
        assert!(status.gone && status.status.is_none());
        assert_eq!(status.address.unwrap().display(), "play.example.com:25566");
        assert!(core.instances.load(&id).await.unwrap().server.unwrap().gone);
        // The owner turned the invite back on.
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        assert!(!core.server_status(&id).await.unwrap().gone);
    }

    #[tokio::test]
    async fn a_rate_limit_is_passed_on_and_changes_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let before = core.instances.load(&id).await.unwrap();
        serve(
            &runner,
            &invite('A'),
            Err(LauncherError::network(ErrorCode::InviteRateLimited, "slow down")),
        );
        assert_eq!(
            core.server_status(&id).await.unwrap_err().code,
            ErrorCode::InviteRateLimited
        );
        assert_eq!(core.instances.load(&id).await.unwrap(), before);
    }

    #[tokio::test]
    async fn relinking_moves_the_instance_and_keeps_its_files() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let world = core.instances.game_dir(&id).join("saves/home/level.dat");
        std::fs::create_dir_all(world.parent().unwrap()).unwrap();
        std::fs::write(&world, b"blocks").unwrap();
        core.update_server(&id, |s| s.as_mut().unwrap().mark_gone())
            .await
            .unwrap();
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let moved = core.server_relink(&id, &invite('B')).await.unwrap();
        assert!(matches!(&moved.source, SourceRef::Perland { invite: i, .. } if *i == invite('B')));
        assert!(!moved.server.as_ref().unwrap().gone);
        assert_eq!(moved.update_policy, UpdatePolicy::Auto);
        assert!(world.is_file(), "the files stay; the next Play syncs");
        assert_eq!(core.instances.load(&id).await.unwrap(), moved);
    }

    #[tokio::test]
    async fn only_server_instances_have_a_status_or_a_link_to_move() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(core.server_status(&plain.id).await.unwrap_err().code, ErrorCode::NoPack);
        assert_eq!(
            core.server_relink(&plain.id, &invite('B')).await.unwrap_err().code,
            ErrorCode::NoPack
        );
    }

    #[tokio::test]
    async fn instance_update_keeps_source_and_server_from_disk() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let stale = core.join_add(&invite('A')).await.unwrap();
        core.server_relink(&stale.id, &invite('B')).await.unwrap();
        // Instance settings was opened before the relink and saves now, with a new name.
        let mut edited = stale.clone();
        edited.name = "Our server".into();
        let saved = core.instance_update(edited).await.unwrap();
        assert_eq!(saved.name, "Our server");
        assert!(matches!(&saved.source, SourceRef::Perland { invite: i, .. } if *i == invite('B')));
        assert_eq!(core.instances.load(&stale.id).await.unwrap(), saved);
    }

    #[tokio::test]
    async fn instance_update_keeps_servers_automatic() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let mut instance = core.join_add(&invite('A')).await.unwrap();
        instance.update_policy = UpdatePolicy::Pinned;
        let saved = core.instance_update(instance).await.unwrap();
        assert_eq!(saved.update_policy, UpdatePolicy::Auto);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib servers::`
Expected: compile errors (`server_status`, `server_relink`, `update_server` don't exist).

- [ ] **Step 3: Implement**

In `dto.rs`:

```rust
/// A server instance's live status, for the pill, the address line and the dead-link plate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatusView {
    /// `None` when unknown (a dead link).
    pub status: Option<ServerStatus>,
    pub players: Option<ServerPlayers>,
    pub whitelisted: bool,
    pub pack_state: Option<String>,
    pub gone: bool,
    /// The last known address.
    pub address: Option<ServerAddress>,
}
```

In `servers.rs`:
- Extend the imports: `launcher_core::{invite::EngineView, ErrorCode, LauncherError}` and `crate::dto::{ServerPlayers, ServerStatusView}`.
- Add:

```rust
fn not_a_server() -> LauncherError {
    LauncherError::pack(
        ErrorCode::NoPack,
        "This instance doesn't come from a server invite",
    )
}

/// A successful read: refresh the link, or start one for an instance that has none yet.
pub(crate) fn apply_view(server: &mut Option<ServerLink>, ev: &EngineView) {
    match server {
        Some(link) => link.refresh(&ev.view, &ev.engine, unix_now()),
        None => *server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now())),
    }
}
```

Inside the `impl` block:

```rust
    /// Change a server instance's `ServerLink` on disk and return it. The instance is reloaded
    /// first, so a rename or a sync made meanwhile stays. It is saved only when it changed.
    pub(crate) async fn update_server(
        &self,
        id: &str,
        change: impl FnOnce(&mut Option<ServerLink>),
    ) -> Result<Option<ServerLink>> {
        let mut instance = self.instances.load(id).await?;
        let before = instance.server.clone();
        change(&mut instance.server);
        if instance.server != before {
            self.instances.save(&instance).await?;
        }
        Ok(instance.server)
    }

    /// The last read said `invite_gone`: only `gone` changes; the address stays for joining.
    pub(crate) async fn mark_server_gone(&self, id: &str) {
        let _ = self
            .update_server(id, |server| {
                if let Some(link) = server {
                    link.mark_gone();
                }
            })
            .await;
    }

    /// The live status, through the shared 15 s view. It takes no lock, so it works while the
    /// game runs. A dead link is an answer (`gone`); anything else failing is an error, after
    /// which the UI polls less often.
    pub async fn server_status(&self, id: &str) -> Result<ServerStatusView> {
        let instance = self.instances.load(id).await?;
        let (link, engine) = server_source(&instance).ok_or_else(not_a_server)?;
        match self.runner.invite_view(link, Some(engine)).await {
            Ok(ev) => {
                let saved = self.update_server(id, |s| apply_view(s, &ev)).await?;
                Ok(ServerStatusView {
                    status: ev.view.status(),
                    players: ev.view.players.as_ref().map(ServerPlayers::from),
                    whitelisted: ev.view.whitelisted,
                    pack_state: ev.view.pack.as_ref().map(|p| p.state.clone()),
                    gone: false,
                    address: saved.and_then(|s| s.address),
                })
            }
            Err(e) if e.code == ErrorCode::InviteGone => {
                self.mark_server_gone(id).await;
                let last = self.instances.load(id).await?.server;
                Ok(ServerStatusView {
                    status: None,
                    players: None,
                    whitelisted: last.as_ref().is_some_and(|s| s.whitelisted),
                    pack_state: None,
                    gone: true,
                    address: last.and_then(|s| s.address),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// "Use a new link": point the instance at another invite. The files stay; the next Play
    /// checks the new link and syncs.
    pub async fn server_relink(&self, id: &str, link: &str) -> Result<Instance> {
        let mut instance = self.instances.load(id).await?;
        server_source(&instance).ok_or_else(not_a_server)?;
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        perland::supported(&ev)?;
        instance.source = SourceRef::Perland {
            invite: link.canonical(),
            engine: ev.engine.to_string(),
        };
        instance.server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now()));
        instance.update_policy = UpdatePolicy::Auto;
        self.instances.save(&instance).await?;
        Ok(instance)
    }
```

In `app.rs`, replace `instance_update` with the version below. 3b's already takes `mut instance` and keeps `source` from disk (`instance.source = current.source;`, pinned by its `a_settings_save_keeps_the_saved_pack_source`); this adds `server` and the `Auto` lock:

```rust
    /// Version and Java changes wait until the game stops; other edits are fine while it runs.
    /// Where an instance comes from (`source`) and its server link (`server`) change only through
    /// their own commands, so a stale copy from the UI can't undo a relink. Server instances
    /// always follow their server (`Auto`).
    pub async fn instance_update(&self, instance: Instance) -> Result<Instance> {
        let current = self.instances.load(&instance.id).await?;
        if self.is_busy(&instance.id)
            && (current.game != instance.game || current.java != instance.java)
        {
            return Err(busy(&instance.id));
        }
        let mut instance = instance;
        instance.source = current.source.clone();
        instance.server = current.server.clone();
        if matches!(current.source, SourceRef::Perland { .. }) {
            instance.update_policy = UpdatePolicy::Auto;
        }
        self.instances.update(&instance).await
    }
```

Add `SourceRef` and `UpdatePolicy` to the `launcher_core::instances::{…}` import in `app.rs` (3b's imports neither outside `mod tests`).

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-app`
Expected: PASS. 3b's `a_settings_save_keeps_the_saved_pack_source` still passes: a packwiz instance keeps the policy the form sends, and its source already came from disk. 3b's helpers (`pack_instance`) save through `core.instances`, so none of them depends on `instance_update` changing `source`.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-app
git commit -m "feat(app): server_status and server_relink; instance_update keeps source and server, and servers automatic

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: App: Play joins the server, with fallbacks for failed checks and dead links

**Files:**
- Modify: `crates/launcher-app/src/runner.rs` (`find_update` gains a sink; `check_update`; `start` gains `join`)
- Modify: `crates/launcher-app/src/runs.rs` (`PlayArgs`, `play_with`, `join_target`, `invite_gone` handling)
- Modify: `crates/launcher-app/src/packs.rs` (`pack_status` server arm; the `find_update` call passes `noop_sink()`)
- Modify: `crates/launcher-app/src/testing.rs` (the fake's `start`, `find_update`, `check_update`)

**Interfaces:**
- Consumes Plan 3b's run machinery (Task 0 row 11), `find_update_with` (Task 6), `update_server`, `mark_server_gone` and `apply_view` (Task 10), and `server_source` (Task 9).
- `trait Runner` changes:

```rust
    fn find_update(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<Option<PendingUpdate>>>;
    /// The cheap update check only (no pack download): a server's banner.
    fn check_update(&self, instance: Instance) -> BoxFuture<UpdateCheck>;
    fn start(
        &self,
        instance: Instance,
        prepared: Self::Prepared,
        session: Session,
        join: Option<ServerAddress>,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>>;
```

- Produces:
  - `AppCore::play_with(self: &Arc<Self>, id: &str, join: bool) -> Result<()>`. `play(id)` stays and means `play_with(id, true)`.
  - `pub(crate) struct PlayArgs { session: Session, minimize: bool, join: bool }`. It replaces 3b's `(Session, bool)` tuple in `start_run` and `drive`.
- New `FakeRunner` pub fields:
  - `started_joins: Mutex<Vec<Option<ServerAddress>>>`;
  - `update_checks: Mutex<HashMap<String, UpdateCheck>>` (default `UpToDate`).
- Behaviour:
  - A server run is `checking → queued → syncing → preparing → running`. It never asks, because the policy is `Auto`.
  - **The check fails:**
    - with `invite_gone`: no warning toast; `ServerLink.gone` is set (the plate shows), and the installed version plays.
    - otherwise: Plan 3b's warning, and the installed version plays.
  - **A never-synced server's resolve fails:** the run ends with that error. If it was `invite_gone`, `gone` is set too.
  - **Joining:**
    - `join: true` and a known address: the address goes to `start`.
    - No known address: one more view read (shared cache). If there is still none, a `server_address_unknown` warning, and the game starts at the title screen.
    - `join: false`, or a non-server instance: nothing is joined, and there is no warning.
  - **`pack_status` for a server** uses `check_update` (the view's commit), never `find_update`, so the banner never downloads a pack. It reports `{ fromVersion: installed, toVersion, added/updated/removed/kept: [] }`. On `invite_gone` it marks the link dead.

- [ ] **Step 1: Write the failing tests**

In `runs.rs` `mod tests`:
- Add the imports `launcher_core::{instances::ServerAddress, sync::lockfile}` and `crate::testing::{invite, serve, server_view}`. `desired` and `pending` are Plan 3b's.
- Then:

```rust
    /// A server instance from invite A that installed 1.0.4 before.
    async fn server(core: &TestCore, runner: &FakeRunner) -> String {
        serve(runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        lockfile::save_pack(&core.instances.perland_dir(&id), &desired("1.0.4", vec![]))
            .await
            .unwrap();
        id
    }

    fn address() -> Option<ServerAddress> {
        ServerAddress::new("play.example.com", 25566)
    }

    #[tokio::test]
    async fn a_server_run_updates_without_asking_and_joins() {
        let (core, runner, host, _t) = ready(true).await;
        let id = server(&core, &runner).await;
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.4", "1.0.5"))));
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Preparing, Running],
            "asking never happens"
        );
        assert_eq!(runner.syncs.lock().unwrap()[0].1.as_deref(), Some("1.0.5"));
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![address()]);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn play_without_joining_starts_at_the_title_screen() {
        let (core, runner, host, _t) = ready(true).await;
        let id = server(&core, &runner).await;
        core.play_with(&id, false).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![None]);
        assert!(!host.warnings().contains(&ErrorCode::ServerAddressUnknown));
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn no_known_address_warns_and_starts_at_the_title_screen() {
        let (core, runner, host, _t) = ready(true).await;
        let mut portless = server_view("survival");
        portless.view.address.port = 0;
        serve(&runner, &invite('A'), Ok(portless));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![None]);
        assert!(host.warnings().contains(&ErrorCode::ServerAddressUnknown));
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn a_failed_check_plays_the_installed_version_and_still_joins() {
        let (core, runner, host, _t) = ready(true).await;
        let id = server(&core, &runner).await;
        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::network(ErrorCode::UpdateCheckFailed, "offline")),
        );
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert!(host.warnings().contains(&ErrorCode::UpdateCheckFailed));
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![address()]);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn a_dead_link_plays_the_installed_version_and_raises_the_plate() {
        let (core, runner, host, _t) = ready(true).await;
        let id = server(&core, &runner).await;
        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::pack(ErrorCode::InviteGone, "gone")),
        );
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert!(!host.warnings().contains(&ErrorCode::InviteGone), "the plate says it, not a toast");
        assert!(core.instances.load(&id).await.unwrap().server.unwrap().gone);
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        assert_eq!(
            *runner.started_joins.lock().unwrap(),
            vec![address()],
            "the last known address"
        );
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn a_never_synced_server_whose_pack_fails_ends_the_run_with_it() {
        let (core, runner, host, _t) = ready(true).await;
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        runner
            .sync_results
            .lock()
            .unwrap()
            .push_back(Err(LauncherError::pack(ErrorCode::InviteGone, "gone")));
        core.play(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(host.states(&id)[0], Queued, "nothing installed yet, so nothing to check");
        assert_eq!(
            host.last_exit(&id).unwrap().error.unwrap().code,
            ErrorCode::InviteGone
        );
        assert!(core.instances.load(&id).await.unwrap().server.unwrap().gone);
        assert!(runner.prepared.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn plain_instances_never_join() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(*runner.started_joins.lock().unwrap(), vec![None]);
        assert!(!host.warnings().contains(&ErrorCode::ServerAddressUnknown));
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }
```

In `packs.rs` `mod tests`, add the imports `launcher_core::{sources::UpdateCheck, sync::lockfile}` and `std::sync::atomic::Ordering` (if not already there), then:

```rust
    #[tokio::test]
    async fn a_servers_pack_status_comes_from_its_view() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        lockfile::save_pack(&core.instances.perland_dir(&id), &desired("1.0.4", vec![]))
            .await
            .unwrap();
        runner.update_checks.lock().unwrap().insert(
            id.clone(),
            UpdateCheck::Available {
                source: SourceRef::None,
                stamp: "c2".into(),
                version: "1.0.5".into(),
            },
        );
        let checks = runner.checks.load(Ordering::SeqCst);
        let update = core.pack_status(&id).await.unwrap().update.unwrap();
        assert_eq!(
            (update.from_version.as_deref(), update.to_version.as_str()),
            (Some("1.0.4"), "1.0.5")
        );
        assert!(update.added.is_empty() && update.updated.is_empty() && update.removed.is_empty());
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "no find_update: the banner never downloads a pack"
        );
        runner.update_checks.lock().unwrap().insert(
            id.clone(),
            UpdateCheck::Unavailable(LauncherError::pack(ErrorCode::InviteGone, "gone")),
        );
        assert_eq!(
            core.pack_status(&id).await.unwrap().check_error.unwrap().code,
            ErrorCode::InviteGone
        );
        assert!(core.instances.load(&id).await.unwrap().server.unwrap().gone);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib runs:: packs::`
Expected: compile errors (`play_with`, `started_joins`, `update_checks` don't exist).

- [ ] **Step 3: The seam**

In `runner.rs`:
- Import `launcher_core::{instances::ServerAddress, sources::UpdateCheck}`.
- Change `find_update` and `start` as in Interfaces, and add `check_update`.
- `CoreRunner`:

```rust
    fn find_update(
        &self,
        instance: Instance,
        sink: EventSink,
    ) -> BoxFuture<Result<Option<PendingUpdate>>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { syncer.find_update_with(&store, &instance, &sink).await })
    }

    fn check_update(&self, instance: Instance) -> BoxFuture<UpdateCheck> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { syncer.check(&store, &instance).await })
    }
```

- In `CoreRunner::start`, take `join: Option<ServerAddress>` and pass `join: join.as_ref(),` in the `StartSpec` literal (replacing Task 8's `join: None`).

In `testing.rs`:
- Add the two fields from Interfaces, initialized empty.
- The fake's `find_update` takes `_sink: EventSink` and is otherwise unchanged.
- The fake's `start` takes `join: Option<ServerAddress>` and records it first: `self.started_joins.lock().unwrap().push(join);`.
- Add:

```rust
    fn check_update(&self, instance: Instance) -> BoxFuture<UpdateCheck> {
        let answer = self
            .update_checks
            .lock()
            .unwrap()
            .get(&instance.id)
            .cloned()
            .unwrap_or(UpdateCheck::UpToDate);
        Box::pin(async move { answer })
    }
```

- [ ] **Step 4: The run**

In `runs.rs`:
- Import `launcher_core::instances::ServerAddress` and `crate::servers::{apply_view, server_source}`.
- Add:

```rust
/// What a game run needs besides the instance: who plays, whether the window hides, and
/// whether the game joins the instance's server.
pub(crate) struct PlayArgs {
    pub(crate) session: Session,
    pub(crate) minimize: bool,
    pub(crate) join: bool,
}

fn address_unknown() -> LauncherError {
    LauncherError::game(
        ErrorCode::ServerAddressUnknown,
        "Couldn't get the server address, so the game starts at the title screen",
    )
}
```

- Rename 3b's `play` to `play_with(self: &Arc<Self>, id: &str, join: bool)`, with its final call `self.start_run(id, instance, Some(PlayArgs { session, minimize, join }), checks)`. Then add:

```rust
    /// Queue a run that joins the instance's server (if it has one). Everything after this
    /// arrives as events.
    pub async fn play(self: &Arc<Self>, id: &str) -> Result<()> {
        self.play_with(id, true).await
    }
```

- In `start_run` and `drive`, change the parameter type `Option<(Session, bool)>` to `Option<PlayArgs>`. In `drive`:
  - 3b's update-only branch `let Some((session, minimize)) = play else { … };` becomes `let Some(args) = play else { … };` (body unchanged).
  - Right after that `let … else` and before `if !self.set_phase(&id, RunStateKind::Preparing) {`, add:

```rust
        let join = if args.join {
            self.join_target(&id, &instance).await
        } else {
            None
        };
```

  - The start call becomes `self.runner.start(instance, prepared, args.session, join, sink.clone())`.
  - 3b reads `minimize` twice after the game starts (`if minimize { self.host.minimize(); }` and `if minimize || crashed { self.host.restore(); }`); both read `args.minimize`. `args.session` has moved into `start` by then, but `args.minimize` is a `bool` field, so it is still usable.
  - In the sync result's generic `Err(error) => { … }` arm, before finishing, add:

```rust
                    if error.code == ErrorCode::InviteGone {
                        self.mark_server_gone(&id).await;
                    }
```

- `decide_update` takes `sink: &EventSink` (pass `&sink` from `drive`). Its call becomes `self.runner.find_update(instance.clone(), sink.clone())`. Its `Err(error)` branch becomes:

```rust
            Err(error) if error.code == ErrorCode::InviteGone => {
                // The dead-link plate says it; the installed version plays.
                self.mark_server_gone(id).await;
                return None;
            }
            Err(error) => {
                self.host.emit(AppEvent::Warning {
                    instance: Some(id.to_string()),
                    error,
                });
                return None;
            }
```

- Add the join target:

```rust
    /// Where this run's game connects: a server instance's last known address. Without one,
    /// the view is asked once more (the shared copy); if there is still none, the game starts
    /// at the title screen with a warning.
    async fn join_target(&self, id: &str, instance: &Instance) -> Option<ServerAddress> {
        let (link, engine) = server_source(instance)?;
        if let Some(address) = instance.server.as_ref().and_then(|s| s.address.clone()) {
            return Some(address);
        }
        if let Ok(ev) = self.runner.invite_view(link, Some(engine)).await {
            if let Ok(Some(server)) = self.update_server(id, |s| apply_view(s, &ev)).await {
                if let Some(address) = server.address {
                    return Some(address);
                }
            }
        }
        self.host.emit(AppEvent::Warning {
            instance: Some(id.to_string()),
            error: address_unknown(),
        });
        None
    }
```

In `packs.rs` `pack_status`:
- Plan 3b's `self.runner.find_update(instance)` becomes `self.runner.find_update(instance, launcher_core::events::noop_sink())`.
- Add this arm before the general `Some(_)` one:

```rust
            // A server's check is the commit in its view, so the banner never downloads a pack.
            Some(current) if matches!(instance.source, SourceRef::Perland { .. }) => {
                match self.runner.check_update(instance).await {
                    UpdateCheck::UpToDate => (None, None),
                    UpdateCheck::Available { version, .. } => (
                        Some(PackUpdate {
                            from_version: Some(current.pack_version.clone()),
                            to_version: version,
                            added: vec![],
                            updated: vec![],
                            removed: vec![],
                            kept: vec![],
                        }),
                        None,
                    ),
                    UpdateCheck::Unavailable(error) => {
                        if error.code == ErrorCode::InviteGone {
                            self.mark_server_gone(id).await;
                        }
                        (None, Some(error))
                    }
                }
            }
```

Import `launcher_core::sources::UpdateCheck` there.

- [ ] **Step 5: Run every app test**

Run: `cargo test -p launcher-app && cargo build -p perland-launcher`
Expected: PASS. Plan 3b's run tests still produce their phase lists. `perland-launcher` fails to build until Task 13 updates the `play` command; if so, check only with `cargo test -p launcher-app` here.

- [ ] **Step 6: Commit**

```bash
git add crates/launcher-app
git commit -m "feat(app): Play joins the server; dead links and failed checks play the installed version

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Typed deep links: `perland://join?invite=…`

**Files:**
- Modify: `crates/launcher-app/src/deep_link.rs` (Plan 3b's)
- Modify: `crates/launcher-app/src/app.rs` (`pending_link: Mutex<Option<DeepLink>>`)
- Modify: `crates/launcher-app/src/packs.rs` (`deep_link`, `deep_link_take`)

**Interfaces:**
- Consumes 3b's `deep_link::parse` and `AppCore::{deep_link, deep_link_take}` (Task 0 rows 11 and 12), and `InviteLink` (Task 2).
- Produces:
  - `pub enum DeepLink { Pack { url: String }, Join { invite: String } }`: `Serialize`, tagged `kind`, snake_case, so `{"kind":"pack","url":…}` or `{"kind":"join","invite":…}`.
    - `Debug` hides the invite's token.
    - `invite` is `InviteLink::canonical()`.
  - `deep_link::parse(raw) -> Result<DeepLink>`. The `pack` behaviour is unchanged.
  - `perland://join?invite=<url-encoded>` is accepted only when:
    - `invite` is the only parameter, and it appears once;
    - the decoded value starts with `https://` (a web page never hands over plain http, like 3b's pack links);
    - the value passes `InviteLink::parse`.
  - A bad join link is `invite_link_invalid`; any other bad link stays 3b's `pack_link_invalid`.
  - `AppCore::deep_link_take(&self) -> Option<DeepLink>`.
  - `AppEvent::DeepLink` stays 3b's unit signal ("a link is waiting; call `deep_link_take`"). If Task 0 row 7 found it carrying `{ url }`, change it to `{ link: DeepLink }` here and update its contract sample in Task 14.

- [ ] **Step 1: Write the failing tests**

In `deep_link.rs` `mod tests`:
- Update 3b's `deep_links_accept_only_https_pack_links`:
  - the good cases compare with `DeepLink::Pack { url: asalith.into() }` instead of a string;
  - remove the row `"perland://join?url=https://a.example/pack.toml"`, which is now a join link (covered below as `invite_link_invalid`).
- Add:

```rust
    #[test]
    fn join_links_carry_a_canonical_invite() {
        let token = "A".repeat(43);
        let canonical = format!("https://play.example.com/{token}");
        for good in [
            format!("perland://join?invite=https%3A%2F%2FPLAY.example.com%2F{token}%2F"),
            format!("perland://join/?invite=https://play.example.com/{token}"),
            format!(" perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token} "),
        ] {
            assert_eq!(
                parse(&good).unwrap(),
                DeepLink::Join {
                    invite: canonical.clone()
                },
                "{good}"
            );
        }
        for bad in [
            format!("perland://join?invite=http%3A%2F%2Fplay.example.com%2F{token}"),
            format!("perland://join?invite=http%3A%2F%2F127.0.0.1%3A5173%2F{token}"),
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}&invite=https%3A%2F%2Fplay.example.com%2F{token}"),
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}&url=x"),
            format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}%3Fref%3Dx"),
            format!("perland://join/extra?invite=https%3A%2F%2Fplay.example.com%2F{token}"),
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2Fshort".to_string(),
            "perland://join?url=https://a.example/pack.toml".to_string(),
            "perland://join".to_string(),
        ] {
            assert_eq!(
                parse(&bad).unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
        }
        assert_eq!(
            parse("perland://joinx?invite=x").unwrap_err().code,
            ErrorCode::PackLinkInvalid
        );
    }

    #[test]
    fn debug_hides_the_invite_token_and_json_is_tagged() {
        let token = "A".repeat(43);
        let link = DeepLink::Join {
            invite: format!("https://play.example.com/{token}"),
        };
        assert!(!format!("{link:?}").contains(&token));
        let json = serde_json::to_value(&link).unwrap();
        assert_eq!(json["kind"], "join");
        assert_eq!(
            serde_json::to_value(DeepLink::Pack { url: "https://x.example/pack.toml".into() })
                .unwrap()["kind"],
            "pack"
        );
    }
```

In `packs.rs` `mod tests`:
- Update 3b's `a_good_deep_link_waits_for_the_ui`: its take assertion becomes `assert_eq!(core.deep_link_take(), Some(DeepLink::Pack { url: "https://x.example/pack.toml".into() }));` (import `crate::deep_link::DeepLink`).
- Add:

```rust
    #[tokio::test]
    async fn a_join_link_waits_typed_and_a_bad_one_warns() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        let token = "A".repeat(43);
        core.deep_link(&format!(
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}"
        ));
        assert_eq!(
            core.deep_link_take(),
            Some(DeepLink::Join {
                invite: format!("https://play.example.com/{token}")
            })
        );
        core.deep_link("perland://join?invite=https%3A%2F%2Fplay.example.com%2Fshort");
        assert_eq!(host.warnings(), vec![ErrorCode::InviteLinkInvalid]);
        assert_eq!(core.deep_link_take(), None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib deep_link:: packs::`
Expected: compile errors (`DeepLink` doesn't exist).

- [ ] **Step 3: Implement the parser**

In `deep_link.rs`:
- Update the module doc to name both forms.
- Import `std::fmt`, `serde::Serialize` and `launcher_core::invite::{shown_invite, InviteLink}`.
- Keep 3b's `invalid()`, and add:

```rust
/// A link the OS handed the launcher, after checking.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeepLink {
    /// `perland://pack?url=`: an https packwiz link for New instance.
    Pack { url: String },
    /// `perland://join?invite=`: a canonical invite link for Join server.
    Join { invite: String },
}

// Manual Debug: an invite holds its token.
impl fmt::Debug for DeepLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeepLink::Pack { url } => f.debug_struct("Pack").field("url", url).finish(),
            DeepLink::Join { invite } => f
                .debug_struct("Join")
                .field("invite", &shown_invite(invite))
                .finish(),
        }
    }
}

fn join_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteLinkInvalid,
        "That isn't a Perland invite link",
    )
}

/// The one value of `key`, if it appears exactly once.
fn only_param(link: &url::Url, key: &str) -> Option<String> {
    let mut values = link
        .query_pairs()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned());
    match (values.next(), values.next()) {
        (Some(value), None) => Some(value),
        _ => None,
    }
}

/// `perland://join?invite=…` → the canonical invite. Only an https invite gets through.
fn parse_join(link: &url::Url) -> Result<String> {
    if link.query_pairs().any(|(k, _)| k != "invite") {
        return Err(join_invalid());
    }
    let invite = only_param(link, "invite").ok_or_else(join_invalid)?;
    if !invite.trim_start().starts_with("https://") {
        return Err(join_invalid());
    }
    InviteLink::parse(&invite)
        .map(|l| l.canonical())
        .map_err(|_| join_invalid())
}
```

Rework 3b's `parse` into a dispatcher, and keep 3b's pack checks in a helper:

```rust
/// A `perland://pack?url=…` or `perland://join?invite=…` link.
pub fn parse(raw: &str) -> Result<DeepLink> {
    let link = url::Url::parse(raw.trim()).map_err(|_| invalid())?;
    if link.scheme() != "perland" {
        return Err(invalid());
    }
    let bare = matches!(link.path(), "" | "/");
    match link.host_str() {
        Some("pack") if bare => parse_pack(&link).map(|url| DeepLink::Pack { url }),
        Some("join") if bare => parse_join(&link).map(|invite| DeepLink::Join { invite }),
        Some("join") => Err(join_invalid()),
        _ => Err(invalid()),
    }
}

/// 3b's pack rule, unchanged: one `url`, https, and it must parse as packwiz.
fn parse_pack(link: &url::Url) -> Result<String> {
    let pack = only_param(link, "url").ok_or_else(invalid)?;
    if !pack.starts_with("https://") {
        return Err(invalid());
    }
    match PackInput::parse(&pack).map_err(|_| invalid())? {
        PackInput::Packwiz { url } => Ok(url),
        _ => Err(invalid()),
    }
}
```

- [ ] **Step 4: Store and take it typed**

In `app.rs`, change `pending_link` to `Mutex<Option<DeepLink>>` (import `crate::deep_link::DeepLink`).

In `packs.rs`:
- `deep_link` stores the `DeepLink` from `parse` (`Ok(link) => { *pending = Some(link); self.host.emit(AppEvent::DeepLink); }`).
- `deep_link_take` returns `Option<DeepLink>`.

- [ ] **Step 5: Run them to verify they pass**

Run: `cargo test -p launcher-app`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/launcher-app
git commit -m "feat(app): typed deep links; perland://join?invite= carries a canonical https invite

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Tauri wiring

`src-tauri` has no tests (`test = false`). This task is verified by building, clippy, and the owner's smoke test (Task 22).

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Consumes `AppCore::{play_with, join_preview, join_add, server_status, server_relink, deep_link_take}` (Tasks 9–12).
- Produces these Tauri commands (JS argument names in parentheses):
  - `play(id, join?)`: `join` defaults to `true`;
  - `join_preview(link)`, `join_add(link)`;
  - `server_status(id)`, `server_relink(id, link)`;
  - `deep_link_take()`, which now returns `DeepLink | null`.

- [ ] **Step 1: Commands**

In `src-tauri/src/commands.rs`:
- Import `launcher_app::{deep_link::DeepLink, dto::{JoinPreview, ServerStatusView}}`.
- Replace `play`:

```rust
#[tauri::command]
pub async fn play(core: CoreState<'_>, id: String, join: Option<bool>) -> Res<()> {
    core.inner().play_with(&id, join.unwrap_or(true)).await
}
```

- Change 3b's `deep_link_take` to return `Option<DeepLink>`.
- Append:

```rust
#[tauri::command]
pub async fn join_preview(core: CoreState<'_>, link: String) -> Res<JoinPreview> {
    core.join_preview(&link).await
}

#[tauri::command]
pub async fn join_add(core: CoreState<'_>, link: String) -> Res<Instance> {
    core.join_add(&link).await
}

#[tauri::command]
pub async fn server_status(core: CoreState<'_>, id: String) -> Res<ServerStatusView> {
    core.server_status(&id).await
}

#[tauri::command]
pub async fn server_relink(core: CoreState<'_>, id: String, link: String) -> Res<Instance> {
    core.server_relink(&id, &link).await
}
```

`deep_link` is already `pub mod` in `crates/launcher-app/src/lib.rs` (3b), so `launcher_app::deep_link::DeepLink` resolves as is.

- [ ] **Step 2: Register them**

In `src-tauri/src/main.rs`, add `commands::join_preview`, `commands::join_add`, `commands::server_status` and `commands::server_relink` to `generate_handler!`, after `commands::deep_link_take`.

- [ ] **Step 3: Build and lint**

Run: `cargo build -p perland-launcher && cargo clippy --workspace --all-targets -- -D warnings`
Expected: OK, no warnings.

- [ ] **Step 4: Commit**

```bash
git add src-tauri
git commit -m "feat(tauri): join_preview, join_add, server_status, server_relink, play(join), typed deep_link_take

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: Contract fixtures, zod schemas and commands

**Files:**
- Modify: `crates/launcher-app/tests/contract.rs`
- Regenerate: `ui/src/ipc/contract/*.json`. New: `server_instance.json`, `join_preview.json`, `server_status.json`, `deep_links.json`, `invite_links.json`.
- Modify: `ui/src/ipc/schema.ts`, `ui/src/ipc/contract.test.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts` (`deep_link_take`), `ui/src/state/deepLink.ts`
- Test: `ui/src/features/home/Home.test.tsx`, `ui/src/features/home/ManualDownloads.test.tsx` (`play` now sends `join`)

**Interfaces:**
- Consumes every new Rust type: `SourceRef::Perland`, `ServerLink`, `JoinPreview`, `ServerStatusView`, `DeepLink` and `invite_link_cases()`.
- Produces in `schema.ts`:
  - `sourceRef` gains the `perland` member;
  - new schemas `serverPlayers`, `serverGame`, `serverPack`, `serverTeam`, `joinPreview`, `serverStatusView`, `deepLink` and `inviteLinkCase`, plus their types.
- Produces in `commands.ts`:
  - `play: (id: string, join = true) => invoke<null>('play', { id, join })`;
  - `joinPreview(link)`, `joinAdd(link)`;
  - `serverStatus(id)`, `serverRelink(id, link)`;
  - `deepLinkTake()` returns `Promise<DeepLink | null>`.

- [ ] **Step 1: The fixtures**

In `crates/launcher-app/tests/contract.rs`, import `launcher_app::{deep_link::DeepLink, dto::{JoinPreview, ServerGame, ServerPack, ServerPlayers, ServerStatusView, ServerTeam}}` and `launcher_core::{instances::{ServerAddress, ServerLink, ServerStatus}, invite::invite_link_cases}`. Add:

```rust
/// A fake invite (43 × A): fixtures never hold a real token.
fn sample_invite() -> String {
    format!("https://play.example.com/{}", "A".repeat(43))
}

fn sample_address() -> ServerAddress {
    ServerAddress {
        host: "play.example.com".into(),
        port: 25566,
    }
}

fn server_instance() -> Instance {
    Instance {
        id: "survival".into(),
        name: "Survival".into(),
        source: SourceRef::Perland {
            invite: sample_invite(),
            engine: "https://api.example.com/".into(),
        },
        game: GameSpec {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        },
        update_policy: UpdatePolicy::Auto,
        optional_choices: BTreeMap::new(),
        java: JavaSettings {
            path: None,
            memory_mb: 4096,
        },
        server: Some(ServerLink {
            address: Some(sample_address()),
            name: "survival".into(),
            accent: Some("violet".into()),
            whitelisted: false,
            last_status: Some(ServerStatus::Online),
            last_seen: Some(1_791_720_000),
            gone: false,
        }),
    }
}
```

In `contract_fixtures_are_current`, add:

```rust
    check("server_instance", server_instance());
    check(
        "join_preview",
        vec![
            JoinPreview {
                host: "play.example.com".into(),
                name: "survival".into(),
                motd: "§6Survival§r\nwith friends".into(),
                kind: "stack".into(),
                status: Some(ServerStatus::Online),
                players: Some(ServerPlayers {
                    online: 3,
                    max: Some(20),
                }),
                game: ServerGame {
                    version: Some("1.20.1".into()),
                    loader: Some("fabric".into()),
                    loader_version: Some("0.16.14".into()),
                },
                pack: Some(ServerPack {
                    name: "Survival Pack".into(),
                    version: Some("1.0.4".into()),
                    mod_count: 142,
                    state: "ready".into(),
                }),
                whitelisted: false,
                team: Some(ServerTeam {
                    display_name: "Acme".into(),
                    accent: Some("violet".into()),
                }),
                address: Some(sample_address()),
                icon: Some("data:image/png;base64,iVBORw0KGgo=".into()),
                existing: None,
            },
            JoinPreview {
                host: "play.example.com".into(),
                name: "creative".into(),
                motd: "Build".into(),
                kind: "stack".into(),
                status: Some(ServerStatus::Sleeping),
                players: None,
                game: ServerGame {
                    version: Some("1.21.1".into()),
                    loader: None,
                    loader_version: None,
                },
                pack: None,
                whitelisted: true,
                team: None,
                address: Some(ServerAddress {
                    host: "play.example.com".into(),
                    port: 25570,
                }),
                icon: None,
                existing: Some("creative".into()),
            },
        ],
    );
    check(
        "server_status",
        vec![
            ServerStatusView {
                status: Some(ServerStatus::Online),
                players: Some(ServerPlayers {
                    online: 3,
                    max: Some(20),
                }),
                whitelisted: false,
                pack_state: Some("ready".into()),
                gone: false,
                address: Some(sample_address()),
            },
            ServerStatusView {
                status: None,
                players: None,
                whitelisted: true,
                pack_state: None,
                gone: true,
                address: Some(sample_address()),
            },
        ],
    );
    check(
        "deep_links",
        vec![
            DeepLink::Pack {
                url: "https://perlanddev.github.io/asalith-modpack/pack.toml".into(),
            },
            DeepLink::Join {
                invite: sample_invite(),
            },
        ],
    );
    check("invite_links", invite_link_cases());
```

Run: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract && cargo test -p launcher-app --test contract`
Expected: five new JSON files, and the check passes.

- [ ] **Step 2: Write the failing UI contract cases**

In `ui/src/ipc/contract.test.ts`, import the new fixtures and add rows to `cases`:

```ts
import deepLinksFx from './contract/deep_links.json';
import inviteLinksFx from './contract/invite_links.json';
import joinPreviewFx from './contract/join_preview.json';
import serverInstanceFx from './contract/server_instance.json';
import serverStatusFx from './contract/server_status.json';
// …
  ['server_instance', s.instance, serverInstanceFx],
  ['join_preview', s.joinPreview.array(), joinPreviewFx],
  ['server_status', s.serverStatusView.array(), serverStatusFx],
  ['deep_links', s.deepLink.array(), deepLinksFx],
  ['invite_links', s.inviteLinkCase.array(), inviteLinksFx],
```

Run: `pnpm --filter ui test -- contract`
Expected: FAIL (`s.joinPreview` etc. don't exist, and `perland` isn't a `sourceRef`).

- [ ] **Step 3: The schemas**

In `ui/src/ipc/schema.ts`:
- Add the `sourceRef` member `z.strictObject({ kind: z.literal('perland'), invite: z.string(), engine: z.string() }),`.
- After `serverLink` (Task 4), add:

```ts
export const serverPlayers = z.strictObject({ online: z.number().int(), max: z.number().int().nullable() });
export const serverGame = z.strictObject({
  version: z.string().nullable(),
  loader: z.string().nullable(),
  loaderVersion: z.string().nullable(),
});
export const serverPack = z.strictObject({
  name: z.string(),
  version: z.string().nullable(),
  modCount: z.number().int(),
  state: z.string(),
});
export const serverTeam = z.strictObject({ displayName: z.string(), accent: z.string().nullable() });
export const joinPreview = z.strictObject({
  host: z.string(),
  name: z.string(),
  motd: z.string(),
  kind: z.string(),
  status: serverStatus.nullable(),
  players: serverPlayers.nullable(),
  game: serverGame,
  pack: serverPack.nullable(),
  whitelisted: z.boolean(),
  team: serverTeam.nullable(),
  address: serverAddress.nullable(),
  icon: z.string().nullable(),
  existing: z.string().nullable(),
});
export const serverStatusView = z.strictObject({
  status: serverStatus.nullable(),
  players: serverPlayers.nullable(),
  whitelisted: z.boolean(),
  packState: z.string().nullable(),
  gone: z.boolean(),
  address: serverAddress.nullable(),
});
export const deepLink = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('pack'), url: z.string() }),
  z.strictObject({ kind: z.literal('join'), invite: z.string() }),
]);
export const inviteLinkCase = z.strictObject({ input: z.string(), canonical: z.string().nullable() });
```

And the types:

```ts
export type ServerPlayers = z.infer<typeof serverPlayers>;
export type JoinPreview = z.infer<typeof joinPreview>;
export type ServerStatusView = z.infer<typeof serverStatusView>;
export type DeepLink = z.infer<typeof deepLink>;
```

- [ ] **Step 4: The commands**

In `ui/src/ipc/commands.ts`:
- Import `DeepLink`, `JoinPreview` and `ServerStatusView`.
- Replace `play` with `play: (id: string, join = true) => invoke<null>('play', { id, join }),`.
- Change `deepLinkTake` to `invoke<DeepLink | null>('deep_link_take')`.
- Add:

```ts
  joinPreview: (link: string) => invoke<JoinPreview>('join_preview', { link }),
  joinAdd: (link: string) => invoke<Instance>('join_add', { link }),
  serverStatus: (id: string) => invoke<ServerStatusView>('server_status', { id }),
  serverRelink: (id: string, link: string) => invoke<Instance>('server_relink', { id, link }),
```

Plan 3b's `state/deepLink.ts` reads the result as a string. Task 15 changes it; until then, make it typecheck by navigating only for `link?.kind === 'pack'` with `link.url`.

Plan 3b's mock hands back its pending link as a bare string: `MockOptions.link` (`?link=` in `main.tsx`) seeds `state.pendingLink`, and the `deep_link_take` case returns it as is. In `ui/src/ipc/mock.ts`, make that case return `link ? { kind: 'pack', url: link } : null` (typed `DeepLink | null`), so `dev:mock ?link=…` keeps opening New instance. Task 16 adds join links to it.

Existing tests that assert `sent(calls, 'play')` equals `[{ id: … }]` now see `{ id, join: true }`. Update those expectations: `features/home/Home.test.tsx` (two `toEqual([{ id: 'asalith-fields' }])`) and `features/home/ManualDownloads.test.tsx` (`toEqual([{ id: 'sky-islands' }])`). Then:

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-app/tests/contract.rs ui/src
git commit -m "feat(contract): server instances, join previews, server status, typed deep links, the invite link table

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: UI state: the invite rule in TypeScript, the Join server route, typed deep links, status polling

**Files:**
- Create: `ui/src/ipc/inviteLink.ts`, `ui/src/ipc/inviteLink.test.ts`
- Create: `ui/src/state/serverPoll.ts`, `ui/src/state/serverPoll.test.ts`
- Modify: `ui/src/state/route.ts`, `ui/src/state/deepLink.ts` (Plan 3b's), `ui/src/state/queries.ts`, `ui/src/state/handleEvent.ts`
- Modify: `ui/src/features/home/labels.ts` (+ `ui/src/features/home/labels.test.ts`, new)
- Test: `ui/src/state/handleEvent.test.ts`

**Interfaces:**
- Consumes the Task 14 schemas and commands, plus Plan 3b's `takeDeepLink`, `handleEvent` and `qk.packStatus` (Task 0 row 15).
- Produces:
  - In `ipc/inviteLink.ts`:
    - `inviteCanonical(input: string): string | null`, the mirror of `InviteLink::parse` + `canonical()`;
    - `isInviteLink(input): boolean`;
    - `inviteHost(invite): string | null`.
  - **Route:** `| { name: 'joinServer'; invite: string; relinkFor?: string }`. An empty `invite` means "ask for a link".
  - **Deep links:** `takeDeepLink()` routes `{kind:'pack'}` to New instance (as in 3b) and `{kind:'join'}` to `joinServer`.
  - **In `state/serverPoll.ts`:**
    - `SELECTED_POLL = 30_000` and `RAIL_POLL = 60_000`;
    - `nextPoll(base, limited): number`;
    - `strikesOf(id): number`;
    - `pollServerStatus(id): Promise<ServerStatusView>`, which counts consecutive `invite_rate_limited` failures and clears them on success;
    - `resetServerPolls()`.
  - **Queries:**
    - `qk.joinPreview(invite)` and `qk.serverStatus(id)`;
    - `useJoinPreview(invite)`, enabled only for a valid invite;
    - `useServerStatus(id, enabled, base)`, polling by `nextPoll`, paused in the background;
    - `useJoinAdd(opts)` and `useServerRelink(id, opts)`.
  - **Labels:** `isServer(instance)`, `inviteOf(instance): string | null` and `addressText(address): string` (IPv6 in brackets).
  - `handleEvent` also invalidates `qk.serverStatus(id)` when a run ends.

- [ ] **Step 1: Write the failing tests**

`ui/src/ipc/inviteLink.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import cases from './contract/invite_links.json';
import { inviteCanonical, inviteHost, isInviteLink } from './inviteLink';

describe('invite links', () => {
  it.each(cases.map((c) => [c.input, c.canonical] as const))('%j → %j, exactly as Rust decides', (input, canonical) => {
    expect(inviteCanonical(input)).toBe(canonical);
    expect(isInviteLink(input)).toBe(canonical !== null);
  });

  it('names only the host', () => {
    expect(inviteHost(`https://play.example.com/${'A'.repeat(43)}`)).toBe('play.example.com');
    expect(inviteHost('nope')).toBeNull();
  });
});
```

`ui/src/state/serverPoll.test.ts`:

```ts
import { mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';
import { nextPoll, pollServerStatus, resetServerPolls, strikesOf } from './serverPoll';

const limited = { category: 'network', code: 'invite_rate_limited', message: 'x', fix: null, detail: null, params: {} };
const ok = { status: 'online', players: null, whitelisted: false, packState: 'ready', gone: false, address: null };

describe('server status polling', () => {
  afterEach(resetServerPolls);

  it('backs off 30 → 60 → 120 s after rate limits', () => {
    expect([0, 1, 2, 3].map((n) => nextPoll(30_000, n))).toEqual([30_000, 60_000, 120_000, 120_000]);
    expect(nextPoll(60_000, 1)).toBe(120_000);
  });

  it('counts rate limits in a row and clears them after a success', async () => {
    let answer: unknown = limited;
    mockIPC((cmd) => {
      if (cmd !== 'server_status') return null;
      if (answer === limited) throw limited;
      return answer;
    });
    await expect(pollServerStatus('survival')).rejects.toMatchObject({ code: 'invite_rate_limited' });
    await expect(pollServerStatus('survival')).rejects.toMatchObject({ code: 'invite_rate_limited' });
    expect(strikesOf('survival')).toBe(2);
    answer = ok;
    await expect(pollServerStatus('survival')).resolves.toMatchObject({ status: 'online' });
    expect(strikesOf('survival')).toBe(0);
  });
});
```

`ui/src/features/home/labels.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { addressText } from './labels';

describe('addressText', () => {
  it('writes host:port, with brackets around IPv6', () => {
    expect(addressText({ host: 'play.example.com', port: 25566 })).toBe('play.example.com:25566');
    expect(addressText({ host: '2001:db8::1', port: 25565 })).toBe('[2001:db8::1]:25565');
  });
});
```

In `ui/src/state/handleEvent.test.ts`:
- In Plan 3b's "opens New instance with a deep link once setup is done", make the mock answer `{ kind: 'pack', url: 'https://x.example/pack.toml' }` instead of the bare string.
- Add:

```ts
it('opens Join server for a join deep link', async () => {
  const invite = `https://play.example.com/${'A'.repeat(43)}`;
  mockIPC((cmd) => (cmd === 'deep_link_take' ? { kind: 'join', invite } : null));
  queryClient.setQueryData(qk.settings, { version: 1, language: null, defaultMemoryMb: 4096, minimizeOnPlay: true, firstRunDone: true });
  handleEvent({ type: 'deep_link' });
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'joinServer', invite }));
  queryClient.clear();
});

it('an ended run refreshes a server instance status', () => {
  const spy = vi.spyOn(queryClient, 'invalidateQueries');
  handleEvent({ type: 'run_state', instance: 'survival', state: 'idle', exit: exitOf({ code: 0 }), update: null });
  expect(spy.mock.calls.map(([f]) => f?.queryKey)).toContainEqual(qk.serverStatus('survival'));
  spy.mockRestore();
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test -- inviteLink serverPoll labels handleEvent`
Expected: FAIL (modules missing, join links not routed).

- [ ] **Step 3: Implement**

`ui/src/ipc/inviteLink.ts`:

```ts
// The TypeScript mirror of launcher-core's `InviteLink::parse` and `canonical()`
// (crates/launcher-core/src/invite/link.rs). Both run the same generated table,
// ui/src/ipc/contract/invite_links.json, so they can't drift apart.
const TOKEN = /^[A-Za-z0-9_-]{43}$/;
const MAX_INPUT = 512;

function isLoopback(hostname: string): boolean {
  const host = hostname.toLowerCase();
  return host === 'localhost' || host === '[::1]' || /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host);
}

/** The canonical invite (lowercase host, no trailing slash), or null when `input` isn't one. */
export function inviteCanonical(input: string): string | null {
  const raw = input.trim();
  // Checked on the raw text, as in Rust: URL parsing hides an empty `?` or `#` and turns `\` into `/`.
  // eslint-disable-next-line no-control-regex
  if (!raw || raw.length > MAX_INPUT || /[\s\u0000-\u001f\u007f-\u009f?#@\\]/.test(raw)) return null;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  const allowed = url.protocol === 'https:' || (url.protocol === 'http:' && isLoopback(url.hostname));
  if (!allowed || url.username || url.password || !url.hostname) return null;
  const token = url.pathname.replace(/^\//, '').replace(/\/$/, '');
  if (!TOKEN.test(token)) return null;
  return `${url.protocol}//${url.host}/${token}`;
}

export const isInviteLink = (input: string): boolean => inviteCanonical(input) !== null;

/** The invite's host, for "From play.example.com". Never show the token itself. */
export function inviteHost(invite: string): string | null {
  const canonical = inviteCanonical(invite);
  return canonical ? new URL(canonical).hostname : null;
}
```

`ui/src/state/serverPoll.ts`:

```ts
import { commands } from '../ipc/commands';
import { isLauncherError } from '../ipc/errors';
import type { ServerStatusView } from '../ipc/schema';

/** Status polls: the selected instance while the window is focused, and the rail. */
export const SELECTED_POLL = 30_000;
export const RAIL_POLL = 60_000;
const MAX_POLL = 120_000;

/** Rate limits in a row, per instance. A success clears them. */
const strikes = new Map<string, number>();

/** The next poll: `base`, doubled for each rate limit in a row, at most 2 minutes. */
export function nextPoll(base: number, limited: number): number {
  return limited > 0 ? Math.min(base * 2 ** limited, MAX_POLL) : base;
}

export function strikesOf(id: string): number {
  return strikes.get(id) ?? 0;
}

export async function pollServerStatus(id: string): Promise<ServerStatusView> {
  try {
    const status = await commands.serverStatus(id);
    strikes.delete(id);
    return status;
  } catch (error) {
    if (isLauncherError(error) && error.code === 'invite_rate_limited') strikes.set(id, strikesOf(id) + 1);
    throw error;
  }
}

export function resetServerPolls() {
  strikes.clear();
}
```

`ui/src/state/route.ts`: add the member `| { name: 'joinServer'; invite: string; relinkFor?: string }`.

`ui/src/state/deepLink.ts` (3b's): replace the navigate line with:

```ts
    if (!link) return;
    if (link.kind === 'pack') navigate({ name: 'newInstance', link: link.url });
    else navigate({ name: 'joinServer', invite: link.invite });
```

`ui/src/state/queries.ts`:
- Import `isInviteLink` from `../ipc/inviteLink`, and `nextPoll`, `pollServerStatus` and `strikesOf` from `./serverPoll`.
- Add the keys `joinPreview: (invite: string) => ['join', 'preview', invite] as const` and `serverStatus: (id: string) => ['server', 'status', id] as const`.
- Add:

```ts
export const useJoinPreview = (invite: string) =>
  useQuery({
    queryKey: qk.joinPreview(invite),
    queryFn: () => commands.joinPreview(invite),
    enabled: isInviteLink(invite),
    staleTime: 15_000,
  });

/** Live status, polled every `base` ms (backing off after rate limits) while the window is in front. */
export const useServerStatus = (id: string, enabled: boolean, base: number) =>
  useQuery({
    queryKey: qk.serverStatus(id),
    queryFn: () => pollServerStatus(id),
    enabled,
    staleTime: 10_000,
    refetchInterval: () => nextPoll(base, strikesOf(id)),
    refetchIntervalInBackground: false,
  });

export function useJoinAdd(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (link: string) => commands.joinAdd(link),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useServerRelink(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (link: string) => commands.serverRelink(id, link),
    onSuccess: () => {
      for (const queryKey of [qk.instances, qk.serverStatus(id), qk.packStatus(id)]) void qc.invalidateQueries({ queryKey });
    },
    onError: onError(opts),
  });
}
```

`ui/src/state/handleEvent.ts`: add `qk.serverStatus(id)` to the list of queries invalidated when a run ends.

`ui/src/features/home/labels.ts`:

```ts
export const isServer = (instance: Instance) => instance.source.kind === 'perland';

/** The saved invite of a server instance. */
export const inviteOf = (instance: Instance): string | null =>
  instance.source.kind === 'perland' ? instance.source.invite : null;

/** "play.example.com:25566", with brackets around an IPv6 literal. */
export function addressText(address: ServerAddress): string {
  return address.host.includes(':') ? `[${address.host}]:${address.port}` : `${address.host}:${address.port}`;
}
```

Import the type `ServerAddress` from `../../ipc/schema` there.

- [ ] **Step 4: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS. `App.tsx`'s `Screen` switch fails to typecheck on the new route member until Task 17; add `case 'joinServer': return null;` for now, and Task 17 replaces it.

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): the invite rule mirrored from Rust, the Join server route, typed deep links, status polling with backoff

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 16: The mock backend learns servers

**Files:**
- Create: `ui/src/ipc/mockServers.ts`
- Modify: `ui/src/ipc/mock.ts` (`MockOptions.server`, seeding, the command hook, the scripted wait), `ui/src/ipc/mock.test.ts`, `ui/src/main.tsx` (`?server=<scenario>`)

**Interfaces:**
- Consumes the fixtures `server_instance.json`, `join_preview.json` and `server_status.json` (Task 14), plus `inviteCanonical` (Task 15).
- Produces in `ipc/mockServers.ts`:
  - `type ServerScenario = 'online' | 'vanilla' | 'pending' | 'whitelisted' | 'gone' | 'network'`;
  - `MOCK_INVITE` (43 × `A`) and `MOCK_SERVER_ID = 'survival'`;
  - `seedServers(scenario, listing)`, which adds the `survival` server instance;
  - `handleServer(servers, listing, cmd, args)`, which answers `join_preview`, `join_add`, `server_status` and `server_relink`, or returns `NOT_MINE`.
- `MockOptions` gains `server?: ServerScenario`, and `main.tsx` reads `?server=online` etc.
- Scenario `pending`: Play on `survival` stays in `checking` for about 2 s, emitting `server_pack_wait` progress, then runs like any other instance.

- [ ] **Step 1: Write the failing tests**

In `ui/src/ipc/mock.test.ts`, import `MOCK_INVITE` from `./mockServers` and the types `JoinPreview`, `Instance` and `ServerStatusView`, then add:

```ts
  it('answers each server scenario', async () => {
    const online = createMockBackend({ server: 'online' });
    expect((await online.handle('join_preview', { link: MOCK_INVITE })) as JoinPreview).toMatchObject({
      name: 'survival',
      existing: 'survival',
      whitelisted: false,
    });
    expect(((await online.handle('server_status', { id: 'survival' })) as ServerStatusView).status).toBe('online');
    const preview = async (server: 'vanilla' | 'pending' | 'whitelisted') =>
      (await createMockBackend({ server }).handle('join_preview', { link: MOCK_INVITE })) as JoinPreview;
    expect((await preview('vanilla')).pack).toBeNull();
    expect((await preview('pending')).pack?.state).toBe('pending');
    expect((await preview('whitelisted')).whitelisted).toBe(true);
    await expect(createMockBackend({ server: 'network' }).handle('join_preview', { link: MOCK_INVITE })).rejects.toMatchObject({
      code: 'invite_unsupported',
      params: { address: 'play.example.com:25577' },
    });
    const gone = createMockBackend({ server: 'gone' });
    await expect(gone.handle('join_preview', { link: MOCK_INVITE })).rejects.toMatchObject({ code: 'invite_gone' });
    expect(((await gone.handle('server_status', { id: 'survival' })) as ServerStatusView).gone).toBe(true);
    await expect(online.handle('join_preview', { link: 'https://play.example.com/short' })).rejects.toMatchObject({
      code: 'invite_link_invalid',
    });
  });

  it('adds a server once and moves it to a new link', async () => {
    const { handle, state } = createMockBackend({ server: 'online' });
    const other = `https://play.example.com/${'B'.repeat(43)}`;
    const added = (await handle('join_add', { link: other })) as Instance;
    expect(((await handle('join_add', { link: ` ${other}/ ` })) as Instance).id).toBe(added.id);
    expect(state.listing.instances.filter((i) => i.source.kind === 'perland')).toHaveLength(2);
    const moved = (await handle('server_relink', { id: 'survival', link: other })) as Instance;
    expect(moved.source).toMatchObject({ kind: 'perland', invite: other });
  });

  it('scripts a pack the server is still building', async () => {
    const backend = installMock({ server: 'pending' });
    await backend.handle('play', { id: 'survival' });
    expect(backend.state.runs.get('survival')?.state).toBe('checking');
    await vi.waitFor(() => expect(backend.state.runs.get('survival')?.state).toBe('running'), { timeout: 8000 });
    stopMockRuns();
  });
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test -- mock`
Expected: FAIL (`server` isn't an option, and the commands are unknown).

- [ ] **Step 3: Implement**

Create `ui/src/ipc/mockServers.ts`:

```ts
// Mock Perland servers for `pnpm --filter ui dev:mock` and the tests. One server instance is
// seeded from the contract fixtures, and the invite answers follow a scenario. Fake tokens only.
import joinPreviewFx from './contract/join_preview.json';
import serverInstanceFx from './contract/server_instance.json';
import serverStatusFx from './contract/server_status.json';
import { inviteCanonical } from './inviteLink';
import type { Instance, JoinPreview, LauncherError, ServerStatusView } from './schema';

export type ServerScenario = 'online' | 'vanilla' | 'pending' | 'whitelisted' | 'gone' | 'network';

/** The invite the seeded server instance has. */
export const MOCK_INVITE = `https://play.example.com/${'A'.repeat(43)}`;
export const MOCK_SERVER_ID = 'survival';
export const NOT_MINE = Symbol('not a server command');

export interface MockServers {
  scenario: ServerScenario;
}

interface Listing {
  instances: Instance[];
}

const clone = <T,>(value: T): T => structuredClone(value);

function fail(category: LauncherError['category'], code: string, message: string, params: Record<string, string> = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

function sorted(instances: Instance[]): Instance[] {
  return [...instances].sort((a, b) => a.name.localeCompare(b.name));
}

/** Add the `survival` server instance for `scenario`. */
export function seedServers(scenario: ServerScenario | undefined, listing: Listing): MockServers | null {
  if (!scenario) return null;
  const instance = clone(serverInstanceFx) as Instance;
  if (instance.server) {
    instance.server.gone = scenario === 'gone';
    instance.server.whitelisted = scenario === 'whitelisted';
  }
  if (scenario === 'vanilla') instance.game = { minecraft: '1.21.1', loader: null };
  listing.instances = sorted([...listing.instances, instance]);
  return { scenario };
}

function preview(servers: MockServers, existing: string | null): JoinPreview {
  const [modded, vanilla] = clone(joinPreviewFx) as JoinPreview[];
  const base = servers.scenario === 'vanilla' ? vanilla! : modded!;
  return {
    ...base,
    existing,
    whitelisted: servers.scenario === 'whitelisted',
    pack: base.pack && servers.scenario === 'pending' ? { ...base.pack, state: 'pending' } : base.pack,
  };
}

function status(servers: MockServers): ServerStatusView {
  const [online, gone] = clone(serverStatusFx) as ServerStatusView[];
  if (servers.scenario === 'gone') return gone!;
  const asleep = servers.scenario === 'vanilla';
  return {
    ...online!,
    status: asleep ? 'sleeping' : online!.status,
    players: asleep ? null : online!.players,
    whitelisted: servers.scenario === 'whitelisted',
  };
}

function perlandOf(instance: Instance): string | null {
  return instance.source.kind === 'perland' ? inviteCanonical(instance.source.invite) : null;
}

/** Answer a server command, or `NOT_MINE`. */
export function handleServer(servers: MockServers | null, listing: Listing, cmd: string, args: Record<string, unknown>): unknown {
  switch (cmd) {
    case 'join_preview':
    case 'join_add': {
      const canonical = inviteCanonical(String(args.link ?? '')) ?? fail('pack', 'invite_link_invalid', "That isn't a Perland invite link");
      if (!servers || servers.scenario === 'gone') fail('pack', 'invite_gone', "This invite link doesn't work anymore.");
      if (servers.scenario === 'network') {
        fail('pack', 'invite_unsupported', "Networks can't be joined from the launcher yet.", { address: 'play.example.com:25577' });
      }
      const existing = listing.instances.find((i) => perlandOf(i) === canonical) ?? null;
      if (cmd === 'join_preview') return preview(servers, existing?.id ?? null);
      if (existing) return existing;
      const taken = new Set(listing.instances.map((i) => i.id));
      let id = MOCK_SERVER_ID;
      for (let n = 2; taken.has(id); n++) id = `${MOCK_SERVER_ID}-${n}`;
      const created: Instance = {
        ...(clone(serverInstanceFx) as Instance),
        id,
        source: { kind: 'perland', invite: canonical, engine: 'https://api.example.com/' },
      };
      listing.instances = sorted([...listing.instances, created]);
      return created;
    }
    case 'server_status':
      if (!servers) fail('pack', 'no_pack', "This instance doesn't come from a server invite");
      return status(servers);
    case 'server_relink': {
      const target = listing.instances.find((i) => i.id === args.id) ?? fail('disk', 'instance_not_found', 'gone');
      const canonical = inviteCanonical(String(args.link ?? '')) ?? fail('pack', 'invite_link_invalid', "That isn't a Perland invite link");
      const moved: Instance = {
        ...target,
        source: { kind: 'perland', invite: canonical, engine: 'https://api.example.com/' },
        updatePolicy: 'auto',
        server: target.server ? { ...target.server, gone: false } : null,
      };
      listing.instances = listing.instances.map((i) => (i.id === moved.id ? moved : i));
      if (servers) servers.scenario = 'online';
      return moved;
    }
    default:
      return NOT_MINE;
  }
}
```

In `ui/src/ipc/mock.ts`:
- `MockOptions` gains `server?: ServerScenario`.
- In `seed`, after the `if (!options.firstRun) { … }` block that adds `sky-islands` and just before `return {`, add `const servers = seedServers(options.server, listing);`, and add `servers` to the returned object.
- The `deep_link_take` case (Task 14 made it return `{ kind: 'pack', url }`) hands a pending link that `inviteCanonical` accepts over as `{ kind: 'join', invite: <canonical> }` instead, so `dev:mock ?link=<invite>` exercises a join deep link.
- In `handle`'s `default:` branch, before the `plugin:` check:

```ts
      {
        const served = handleServer(state.servers, state.listing, cmd, args);
        if (served !== NOT_MINE) return served;
      }
```

- In the `play` case, after its `already_running` check and before 3b's `void scriptRun(state, target);`:

```ts
      if (state.servers?.scenario === 'pending' && target.id === MOCK_SERVER_ID) {
        void scriptServerWait(state, target);
        return null;
      }
```

- Add this script next to 3b's `scriptRun`. `MockRun` and the `run_state` event carry the fields Plan 3b gave them (Task 0 row 17): `MockRun` is `{ state, cancelled, updateOnly, answer }`, and every `run_state` has `update`.

```ts
/** A server whose pack is still building: Play waits in `checking` with "Server is preparing
 *  the modpack…", then runs like any instance. */
async function scriptServerWait(state: MockState, instance: Instance) {
  const id = instance.id;
  const run: MockRun = { state: 'checking', cancelled: false, updateOnly: false, answer: null };
  state.runs.set(id, run);
  event({ type: 'run_state', instance: id, state: 'checking', exit: null, update: null });
  for (let n = 0; n < 5; n++) {
    if (run.cancelled) return;
    event({ type: 'progress', instance: id, task: { key: 'server_pack_wait', params: { name: instance.name } }, done: 0, total: 0, bytes: 0 });
    await wait(400);
  }
  if (run.cancelled || state.runs.get(id) !== run) return;
  state.runs.delete(id);
  // No `MockPack` for a server instance, so 3b's script runs it like a plain one: queued, preparing, running.
  void scriptRun(state, instance);
}
```

In `ui/src/main.tsx`, pass `server: (params.get('server') as ServerScenario | null) ?? undefined` to `installMock`, and import the type.

- [ ] **Step 4: Run them to verify they pass**

Run: `pnpm --filter ui test -- mock && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): mock servers: online, vanilla, pending, whitelisted, gone and network, plus a scripted pack wait

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: The Join server screen

Structure, behaviour, strings and tests only. `/impeccable` styles it in Task 20.

**Files:**
- Create: `ui/src/features/servers/motd.ts`, `ui/src/features/servers/motd.test.ts`, `ui/src/features/servers/Motd.tsx`
- Create: `ui/src/features/servers/StatusPill.tsx`, `ui/src/features/servers/StatusPill.test.tsx`
- Create: `ui/src/features/join/ServerCard.tsx`, `ui/src/features/join/JoinServer.tsx`, `ui/src/features/join/JoinServer.test.tsx`
- Modify: `ui/src/App.tsx` (`joinServer` screen)
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (`join.*`, `server.*` status words)

**Interfaces:**
- Consumes `useJoinPreview`, `useJoinAdd`, `useServerRelink`, `inviteCanonical`/`isInviteLink`, the route and the mock (Tasks 15–16).
- Produces:
  - `motdRuns(text): MotdRun[]` and `motdPlain(text): string`: `§` codes become runs, `§r` resets, and `§k` and unknown codes are dropped.
  - `<Motd text />`.
  - `statusWord(t, status, players, gone): string` and `<StatusPill status players gone />`: a square LED plus a word, `role="status"`.
  - `<ServerCard preview />` and `<JoinServer invite relinkFor? />`.
- Behaviour:
  - **Link input:** an empty or invalid `invite` (or relink mode) shows a link field with **Check link**. An invalid link shows `invite_link_invalid` inline, without asking Rust.
  - **The card shows:**
    - the icon (`<img>` from the `data:` URL, else the generated plot icon), the name, a team mark when there's an accent, and the MOTD;
    - the pill;
    - the version line ("Minecraft 1.20.1", or "Vanilla Minecraft 1.21.1" without a pack) and the pack line ("Survival Pack 1.0.4 · 142 mods");
    - "From play.example.com".
  - **Whitelist:** "Invite only: ask the owner to add Steve" (the active account), or "…add you" without one.
  - **Buttons:** **Add server** (the one primary), or **Open** when `existing` is set, and Cancel. In relink mode: **Use this link**, with "This link is for *new*, not *old*." when the names differ (case-insensitive). The warning still allows the relink.
  - **Errors** show inline by code. `invite_unsupported` adds **Copy address** (from `params.address`).

- [ ] **Step 1: Strings**

`en.json`, new `join` section:

```json
  "join": {
    "title": "Join server",
    "relinkTitle": "Use a new invite link",
    "linkLabel": "Invite link",
    "linkPlaceholder": "https://play.example.com/…",
    "check": "Check link",
    "from": "From {{host}}",
    "add": "Add server",
    "adding": "Adding…",
    "open": "Open",
    "useLink": "Use this link",
    "mismatch": "This link is for {{new}}, not {{old}}.",
    "inviteOnly": "Invite only: ask the owner to add {{name}}",
    "inviteOnlyNoAccount": "Invite only: ask the owner to add you",
    "versionLabel": "Version",
    "versionLine": "Minecraft {{version}}",
    "vanilla": "Vanilla Minecraft {{version}}",
    "packLabel": "Modpack",
    "packMods_one": "{{count}} mod",
    "packMods_other": "{{count}} mods",
    "note": "The server's mods download the first time you press Play.",
    "copyAddress": "Copy address"
  },
```

`en.json`, new `server` section (Tasks 18–19 add more keys to it):

```json
  "server": {
    "online": "Online",
    "onlinePlayers": "Online · {{online}}/{{max}}",
    "onlineCount": "Online · {{online}}",
    "starting": "Starting",
    "sleeping": "Sleeping · join to wake it",
    "offline": "Offline",
    "unknown": "Unknown",
    "statusLabel": "Server status: {{status}}"
  },
```

`es.json`:

```json
  "join": {
    "title": "Unirse a un servidor",
    "relinkTitle": "Usar un nuevo enlace de invitación",
    "linkLabel": "Enlace de invitación",
    "linkPlaceholder": "https://play.ejemplo.com/…",
    "check": "Revisar enlace",
    "from": "De {{host}}",
    "add": "Agregar servidor",
    "adding": "Agregando…",
    "open": "Abrir",
    "useLink": "Usar este enlace",
    "mismatch": "Este enlace es de {{new}}, no de {{old}}.",
    "inviteOnly": "Solo con invitación: pídele al dueño que agregue a {{name}}",
    "inviteOnlyNoAccount": "Solo con invitación: pídele al dueño que te agregue",
    "versionLabel": "Versión",
    "versionLine": "Minecraft {{version}}",
    "vanilla": "Minecraft {{version}} sin mods",
    "packLabel": "Modpack",
    "packMods_one": "{{count}} mod",
    "packMods_other": "{{count}} mods",
    "note": "Los mods del servidor se descargan la primera vez que presionas Jugar.",
    "copyAddress": "Copiar dirección"
  },
  "server": {
    "online": "En línea",
    "onlinePlayers": "En línea · {{online}}/{{max}}",
    "onlineCount": "En línea · {{online}}",
    "starting": "Iniciando",
    "sleeping": "Dormido · únete para despertarlo",
    "offline": "Apagado",
    "unknown": "Desconocido",
    "statusLabel": "Estado del servidor: {{status}}"
  },
```

If i18next resolves Spanish plurals with a `_many` form in this version, add `"packMods_many": "{{count}} mods"` to `es.json` and the same key to `en.json`. The i18n test requires identical key sets, and Plan 3b's `update.kept_*` keys show which form the project uses.

- [ ] **Step 2: Write the failing tests**

`ui/src/features/servers/motd.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { motdPlain, motdRuns } from './motd';

describe('motdRuns', () => {
  it('splits § colour and format codes into runs', () => {
    expect(motdRuns('§6Survival§r with §lfriends')).toEqual([
      { text: 'Survival', color: '6', bold: false, italic: false, underline: false, strike: false },
      { text: ' with ', color: null, bold: false, italic: false, underline: false, strike: false },
      { text: 'friends', color: null, bold: true, italic: false, underline: false, strike: false },
    ]);
  });

  it('drops obfuscation, unknown codes and a trailing §, and a colour resets formats', () => {
    expect(motdRuns('§l§kx§zy§az§').map((r) => [r.text, r.color, r.bold])).toEqual([
      ['x', null, true],
      ['y', null, true],
      ['z', 'a', false],
    ]);
    expect(motdPlain('§6Survival§r\nwith friends')).toBe('Survival\nwith friends');
  });
});
```

`ui/src/features/servers/StatusPill.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { renderWithProviders } from '../../test/render';
import { StatusPill } from './StatusPill';

describe('StatusPill', () => {
  it.each([
    ['online', { online: 3, max: 20 }, false, 'Online · 3/20'],
    ['online', { online: 9, max: null }, false, 'Online · 9'],
    ['online', null, false, 'Online'],
    ['starting', null, false, 'Starting'],
    ['sleeping', null, false, 'Sleeping · join to wake it'],
    ['offline', null, false, 'Offline'],
    [null, null, false, 'Unknown'],
    ['online', { online: 3, max: 20 }, true, 'Unknown'],
  ] as const)('%s %j gone=%s reads %s', (status, players, gone, word) => {
    renderWithProviders(<StatusPill status={status} players={players} gone={gone} />);
    expect(screen.getByRole('status')).toHaveTextContent(word);
    expect(screen.getByRole('status')).toHaveAccessibleName(`Server status: ${word}`);
  });
});
```

`ui/src/features/join/JoinServer.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MOCK_INVITE } from '../../ipc/mockServers';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { JoinServer } from './JoinServer';

const OTHER = `https://play.example.com/${'B'.repeat(43)}`;

describe('JoinServer', () => {
  it('previews a server and adds it', async () => {
    const { user, calls } = renderApp(<JoinServer invite={OTHER} />, { server: 'online' });
    expect(await screen.findByRole('heading', { name: 'survival' })).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Online · 3/20');
    expect(screen.getByText('From play.example.com')).toBeInTheDocument();
    expect(screen.getByText('Survival Pack 1.0.4 · 142 mods')).toBeInTheDocument();
    expect(screen.getByText('Minecraft 1.20.1')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Add server' }));
    expect(sent(calls, 'join_add')).toEqual([{ link: OTHER }]);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival-2' }));
  });

  it('offers Open for a server that is already added', async () => {
    const { user, calls } = renderApp(<JoinServer invite={MOCK_INVITE} />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Open' }));
    expect(sent(calls, 'join_add')).toEqual([]);
    expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' });
  });

  it('says when the server is invite only, naming the active account', async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'whitelisted' });
    expect(await screen.findByText('Invite only: ask the owner to add Steve')).toBeInTheDocument();
  });

  it('shows a vanilla server without a pack line', async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'vanilla' });
    expect(await screen.findByText('Vanilla Minecraft 1.21.1')).toBeInTheDocument();
    expect(screen.queryByText(/mods$/)).not.toBeInTheDocument();
  });

  it('refuses networks but gives their address to copy', async () => {
    const { user } = renderApp(<JoinServer invite={OTHER} />, { server: 'network' });
    expect(
      await screen.findByText("Networks can't be joined from the launcher yet. Join with the address play.example.com:25577."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Copy address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25577');
  });

  it('says when the link is dead', async () => {
    renderApp(<JoinServer invite={OTHER} />, { server: 'gone' });
    expect(await screen.findByText("This invite link doesn't work anymore. Ask the owner for a new one.")).toBeInTheDocument();
  });

  it('checks a pasted link before asking the launcher', async () => {
    const { user, calls } = renderApp(<JoinServer invite="" />, { server: 'online' });
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), 'https://play.example.com/short');
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(screen.getByText("That isn't a Perland invite link.")).toBeInTheDocument();
    expect(sent(calls, 'join_preview')).toEqual([]);
    await user.clear(screen.getByRole('textbox', { name: 'Invite link' }));
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(await screen.findByRole('heading', { name: 'survival' })).toBeInTheDocument();
  });

  it('relinks, warning when the link is for another server', async () => {
    const { user, calls } = renderApp(<JoinServer invite="" relinkFor="survival" />, { server: 'vanilla' });
    expect(await screen.findByRole('heading', { name: 'Use a new invite link' })).toBeInTheDocument();
    await user.type(screen.getByRole('textbox', { name: 'Invite link' }), OTHER);
    await user.click(screen.getByRole('button', { name: 'Check link' }));
    expect(await screen.findByText('This link is for creative, not survival.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Use this link' }));
    expect(sent(calls, 'server_relink')).toEqual([{ id: 'survival', link: OTHER }]);
    await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'survival' }));
  });
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- motd StatusPill JoinServer`
Expected: FAIL (modules missing).

- [ ] **Step 4: Implement**

`ui/src/features/servers/motd.ts`:

```ts
/** One stretch of a MOTD with one style. `color` is the § code (`0`–`9`, `a`–`f`), or null. */
export interface MotdRun {
  text: string;
  color: string | null;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
}

const COLORS = '0123456789abcdef';
const PLAIN = { color: null, bold: false, italic: false, underline: false, strike: false };

/** Split a MOTD on its § codes. A colour resets the formats, as in Minecraft. */
export function motdRuns(motd: string): MotdRun[] {
  const runs: MotdRun[] = [];
  let style: Omit<MotdRun, 'text'> = { ...PLAIN };
  let text = '';
  const chars = [...motd];
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i] ?? '';
    if (ch !== '§') {
      text += ch;
      continue;
    }
    const code = (chars[i + 1] ?? '').toLowerCase();
    i++;
    if (text) runs.push({ text, ...style });
    text = '';
    if (COLORS.includes(code) && code !== '') style = { ...PLAIN, color: code };
    else if (code === 'l') style = { ...style, bold: true };
    else if (code === 'o') style = { ...style, italic: true };
    else if (code === 'n') style = { ...style, underline: true };
    else if (code === 'm') style = { ...style, strike: true };
    else if (code === 'r') style = { ...PLAIN };
    // `k` (obfuscated) and unknown codes are dropped.
  }
  if (text) runs.push({ text, ...style });
  return runs;
}

export const motdPlain = (motd: string): string =>
  motdRuns(motd)
    .map((r) => r.text)
    .join('');
```

`ui/src/features/servers/Motd.tsx`:

```tsx
import { motdRuns } from './motd';

/** A server's MOTD in its § colours. Text only: nothing in it is ever parsed as markup. */
export function Motd({ text }: { text: string }) {
  return (
    <p className="motd whitespace-pre-line">
      {motdRuns(text).map((run, i) => (
        <span
          key={i}
          className={run.color ? `mc-${run.color}` : undefined}
          style={{
            fontWeight: run.bold ? 700 : undefined,
            fontStyle: run.italic ? 'italic' : undefined,
            textDecoration: [run.underline && 'underline', run.strike && 'line-through'].filter(Boolean).join(' ') || undefined,
          }}
        >
          {run.text}
        </span>
      ))}
    </p>
  );
}
```

`ui/src/features/servers/StatusPill.tsx`:

```tsx
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import type { ServerPlayers, ServerStatus } from '../../ipc/schema';

/** The status in words; a dead link or no data is "Unknown". */
export function statusWord(t: TFunction, status: ServerStatus | null, players: ServerPlayers | null, gone: boolean): string {
  if (gone || !status) return t('server.unknown');
  switch (status) {
    case 'online':
      if (!players) return t('server.online');
      return players.max !== null
        ? t('server.onlinePlayers', { online: players.online, max: players.max })
        : t('server.onlineCount', { online: players.online });
    case 'starting':
      return t('server.starting');
    case 'sleeping':
      return t('server.sleeping');
    case 'offline':
      return t('server.offline');
  }
}

/** A square LED and a word: the status never relies on colour alone. */
export function StatusPill({ status, players, gone }: { status: ServerStatus | null; players: ServerPlayers | null; gone: boolean }) {
  const { t } = useTranslation();
  const word = statusWord(t, status, players, gone);
  return (
    <span className="status-pill" data-status={gone || !status ? 'unknown' : status} role="status" aria-label={t('server.statusLabel', { status: word })}>
      <span className="status-led" aria-hidden />
      {word}
    </span>
  );
}
```

`ui/src/features/join/ServerCard.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import type { JoinPreview } from '../../ipc/schema';
import { Motd } from '../servers/Motd';
import { StatusPill } from '../servers/StatusPill';

/** What the invite says about the server. Nothing here is lit: the screen's button is. */
export function ServerCard({ preview }: { preview: JoinPreview }) {
  const { t } = useTranslation();
  const version = preview.game.version;
  const pack = preview.pack;
  return (
    <article className="panel grid gap-4 p-6" aria-labelledby="server-name">
      <div className="flex items-start gap-5">
        {preview.icon ? (
          <img src={preview.icon} alt="" width={64} height={64} className="server-icon" draggable={false} />
        ) : (
          <PlotIcon seed={preview.name} size={64} />
        )}
        <div className="grid min-w-0 gap-2">
          <div className="flex flex-wrap items-center gap-3">
            <h2 id="server-name" className="font-display text-2xl [overflow-wrap:anywhere]">
              {preview.name}
            </h2>
            {preview.team?.accent && (
              <span className="team-mark" data-accent={preview.team.accent} title={preview.team.displayName || undefined} aria-hidden />
            )}
          </div>
          <Motd text={preview.motd} />
          <StatusPill status={preview.status} players={preview.players} gone={false} />
        </div>
      </div>
      <dl className="grid gap-1">
        {version && (
          <div>
            <dt className="sr-only">{t('join.versionLabel')}</dt>
            <dd>{pack ? t('join.versionLine', { version }) : t('join.vanilla', { version })}</dd>
          </div>
        )}
        {pack && (
          <div>
            <dt className="sr-only">{t('join.packLabel')}</dt>
            <dd>{`${[pack.name, pack.version].filter(Boolean).join(' ')} · ${t('join.packMods', { count: pack.modCount })}`}</dd>
          </div>
        )}
      </dl>
      <p className="text-sm text-ink-dim">{t('join.from', { host: preview.host })}</p>
    </article>
  );
}
```

`ui/src/features/join/JoinServer.tsx`:

```tsx
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { isLauncherError } from '../../ipc/errors';
import { isInviteLink } from '../../ipc/inviteLink';
import type { LauncherError } from '../../ipc/schema';
import { useAccounts, useInstances, useJoinAdd, useJoinPreview, useServerRelink } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Copy } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { ServerCard } from './ServerCard';

const notAnInvite: LauncherError = {
  category: 'pack',
  code: 'invite_link_invalid',
  message: "That isn't a Perland invite link",
  fix: null,
  detail: null,
  params: {},
};

const copy = (text: string) => void navigator.clipboard.writeText(text).catch(pushError);

/** Confirm a server before adding it (or before moving an instance to a new link). Nothing is
 *  written until the button. */
export function JoinServer({ invite, relinkFor }: { invite: string; relinkFor?: string }) {
  const { t } = useTranslation();
  const listing = useInstances();
  const accounts = useAccounts();
  const [draft, setDraft] = useState(invite);
  const [link, setLink] = useState(invite.trim());
  const valid = isInviteLink(link);
  const preview = useJoinPreview(valid ? link : '');
  const add = useJoinAdd({ inline: true });
  const relink = useServerRelink(relinkFor ?? '', { inline: true });
  const target = relinkFor ? (listing.data?.instances.find((i) => i.id === relinkFor) ?? null) : null;
  const account = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active)?.username;
  const asking = relinkFor !== undefined || !valid;
  const data = preview.data;
  const oldName = target?.server?.name ?? target?.name ?? '';
  const mismatch = data && target && data.name.trim().toLowerCase() !== oldName.trim().toLowerCase();
  const address = isLauncherError(preview.error) && preview.error.code === 'invite_unsupported' ? preview.error.params.address : undefined;

  const check = (event: FormEvent) => {
    event.preventDefault();
    add.reset();
    relink.reset();
    setLink(draft.trim());
  };
  const go = () => {
    if (!data) return;
    if (relinkFor) relink.mutate(link, { onSuccess: () => navigate({ name: 'home', id: relinkFor }) });
    else if (data.existing) navigate({ name: 'home', id: data.existing });
    else add.mutate(link, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  const busy = add.isPending || relink.isPending;
  const label = relinkFor ? t('join.useLink') : data?.existing ? t('join.open') : busy ? t('join.adding') : t('join.add');

  return (
    <ScreenFrame title={relinkFor ? t('join.relinkTitle') : t('join.title')}>
      <div className="grid max-w-3xl gap-6">
        {asking && (
          <form onSubmit={check} className="flex flex-wrap items-end gap-3">
            <Field label={t('join.linkLabel')}>
              <input
                className="field-input min-w-[28rem]"
                type="url"
                inputMode="url"
                spellCheck={false}
                value={draft}
                placeholder={t('join.linkPlaceholder')}
                onChange={(e) => setDraft(e.target.value)}
              />
            </Field>
            <Button type="submit" disabled={!draft.trim()}>
              {t('join.check')}
            </Button>
          </form>
        )}
        {link && !valid && <ErrorCard error={notAnInvite} />}
        {preview.isError && (
          <ErrorCard
            error={preview.error}
            actions={
              address ? (
                <Button icon={<Copy />} onClick={() => copy(address)}>
                  {t('join.copyAddress')}
                </Button>
              ) : undefined
            }
          />
        )}
        {data && <ServerCard preview={data} />}
        {data?.whitelisted && (
          <p className="invite-only">{account ? t('join.inviteOnly', { name: account }) : t('join.inviteOnlyNoAccount')}</p>
        )}
        {mismatch && <p role="alert">{t('join.mismatch', { new: data.name, old: oldName })}</p>}
        {add.isError && <ErrorCard error={add.error} />}
        {relink.isError && <ErrorCard error={relink.error} />}
        {data && (
          <div className="sticky bottom-0 flex items-center justify-end gap-4 bg-ground py-4">
            {!relinkFor && !data.existing && <p className="text-sm text-ink-dim">{t('join.note')}</p>}
            <Button onClick={() => navigate({ name: 'home', id: relinkFor ?? null })}>{t('common.cancel')}</Button>
            <Button variant="primary" size="lg" disabled={busy} onClick={go}>
              {label}
            </Button>
          </div>
        )}
      </div>
    </ScreenFrame>
  );
}
```

`ui/src/App.tsx`: in `Screen`, replace Task 15's placeholder with:

```tsx
    case 'joinServer':
      return <JoinServer key={`${route.invite}|${route.relinkFor ?? ''}`} invite={route.invite} relinkFor={route.relinkFor} />;
```

Import `JoinServer` from `./features/join/JoinServer`.

Add plain, readable placeholder rules for `.status-pill`, `.status-led`, `.team-mark`, `.server-icon` (`image-rendering: pixelated`) and `.mc-0`…`.mc-f` (Minecraft's 16 text colours) to `ui/src/styles/app.css`, so the screen works before Task 20. `/impeccable` replaces them in Task 20.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): the Join server screen: server card, invite-only line, Add or Open, relink with a name check

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 18: The server instance view: status pill, address, dead-link plate, banner, the wait label

**Files:**
- Create: `ui/src/features/servers/AddressLine.tsx`, `ui/src/features/servers/DeadLinkPlate.tsx`, `ui/src/features/servers/ServerPanel.tsx`
- Modify: `ui/src/features/home/Home.tsx`, `ui/src/features/home/UpdatePlate.tsx` (3b's), `ui/src/ui/PlayButton.tsx` (3b's)
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (`server.*`, `progress.server_pack_wait`)
- Test: `ui/src/features/home/Home.test.tsx`

**Interfaces:**
- Consumes `useServerStatus` with `SELECTED_POLL`, `StatusPill`, `isServer`, `addressText` (Tasks 15 and 17), and 3b's `UpdatePlate` and `PlayButton` (Task 0 row 16).
- Produces `<ServerPanel instance />` in the marquee's plate column for server instances:
  - the pill, from live status, falling back to the saved `ServerLink`;
  - the address line with Copy;
  - the invite-only line;
  - the dead-link plate ("This invite link stopped working · **Use a new link**" → `joinServer({ invite: '', relinkFor: id })`).
- 3b's `UpdatePlate` for a server instance shows only "Server updated · updates when you play" when an update exists. There is no **Update** button, and no "Couldn't check" line (the pill covers reachability).
- `PlayButton` while `checking` shows the progress task label when the task is `server_pack_wait` ("Server is preparing the modpack…"). Otherwise it keeps 3b's "Checking for updates…".

- [ ] **Step 1: Strings**

Add to `en.json`:
- under `server`:

```json
    "address": "Address",
    "noAddress": "No address yet",
    "copyAddress": "Copy server address",
    "inviteOnly": "Invite only: ask the owner to add {{name}}",
    "deadLink": "This invite link stopped working",
    "useNewLink": "Use a new link",
    "updated": "Server updated · updates when you play"
```

- under `progress`: `"server_pack_wait": "Server is preparing the modpack…"`

Add to `es.json`:
- under `server`:

```json
    "address": "Dirección",
    "noAddress": "Todavía no hay dirección",
    "copyAddress": "Copiar dirección del servidor",
    "inviteOnly": "Solo con invitación: pídele al dueño que agregue a {{name}}",
    "deadLink": "Este enlace de invitación dejó de funcionar",
    "useNewLink": "Usar un enlace nuevo",
    "updated": "Servidor actualizado · se actualiza cuando juegues"
```

- under `progress`: `"server_pack_wait": "El servidor está preparando el modpack…"`

- [ ] **Step 2: Write the failing tests**

In `Home.test.tsx`, import `createMockBackend` from `../../ipc/mock` and `renderWithProviders` from `../../test/render`, then add:

```tsx
describe('server instances', () => {
  it('show the live status, the address and the invite-only line', async () => {
    renderApp(<Home id="survival" />, { server: 'whitelisted' });
    expect(await screen.findByRole('status', { name: 'Server status: Online · 3/20' })).toBeInTheDocument();
    expect(screen.getByText('play.example.com:25566')).toBeInTheDocument();
    expect(screen.getByText('Invite only: ask the owner to add Steve')).toBeInTheDocument();
  });

  it.each([
    ['vanilla', 'Sleeping · join to wake it'],
    ['gone', 'Unknown'],
  ] as const)('read %s as %s', async (server, word) => {
    renderApp(<Home id="survival" />, { server });
    expect(await screen.findByRole('status', { name: `Server status: ${word}` })).toBeInTheDocument();
  });

  it('copy the address', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'online' });
    await user.click(await screen.findByRole('button', { name: 'Copy server address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25566');
  });

  it('raise the dead-link plate, which opens relinking', async () => {
    const { user } = renderApp(<Home id="survival" />, { server: 'gone' });
    expect(await screen.findByText(/This invite link stopped working/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Use a new link' }));
    expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', relinkFor: 'survival' });
  });

  it('say a server update happens at Play, with no Update button', async () => {
    const backend = createMockBackend({ server: 'online' });
    renderWithProviders(<Home id="survival" />, {
      ipc: (cmd, args) =>
        cmd === 'pack_status'
          ? {
              installedVersion: '1.0.4',
              update: { fromVersion: '1.0.4', toVersion: '1.0.5', added: [], updated: [], removed: [], kept: [] },
              checkError: null,
            }
          : backend.handle(cmd, args),
    });
    expect(await screen.findByText('Server updated · updates when you play')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Update' })).not.toBeInTheDocument();
  });

  it('label the wait while the server builds its pack', async () => {
    renderApp(<Home id="survival" />, { server: 'pending' });
    await screen.findByRole('heading', { name: 'Survival' });
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'survival', state: 'checking', exit: null, update: null });
      runStore.apply({
        type: 'progress',
        instance: 'survival',
        task: { key: 'server_pack_wait', params: { name: 'survival' } },
        done: 0,
        total: 0,
        bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Server is preparing the modpack…' })).toBeInTheDocument();
  });

  it('never poll a plain instance', async () => {
    const { calls } = renderApp(<Home id="creative-flat" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Creative Flat' });
    expect(sent(calls, 'server_status')).toEqual([]);
  });
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- Home`
Expected: FAIL.

- [ ] **Step 4: Implement**

`ui/src/features/servers/AddressLine.tsx`:

```tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { ServerAddress } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Copy } from '../../ui/Icon';
import { addressText } from '../home/labels';

/** The address players type, with Copy. */
export function AddressLine({ address }: { address: ServerAddress | null }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  if (!address) return <p className="text-sm text-ink-dim">{t('server.noAddress')}</p>;
  const text = addressText(address);
  const copy = () =>
    void navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      })
      .catch(pushError);
  return (
    <p className="flex flex-wrap items-center gap-3">
      <span className="text-ink-dim">{t('server.address')}</span>
      <span className="selectable font-mono">{text}</span>
      <Button variant="ghost" icon={<Copy />} aria-label={t('server.copyAddress')} onClick={copy}>
        {copied ? t('common.copied') : t('common.copy')}
      </Button>
    </p>
  );
}
```

`ui/src/features/servers/DeadLinkPlate.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { navigate } from '../../state/route';

/** A quiet plate: the instance still plays and joins the last address; this offers a new link. */
export function DeadLinkPlate({ id }: { id: string }) {
  const { t } = useTranslation();
  return (
    <p className="marquee-plate dead-link" role="status">
      {t('server.deadLink')}
      {' · '}
      <button type="button" className="underline underline-offset-4" onClick={() => navigate({ name: 'joinServer', invite: '', relinkFor: id })}>
        {t('server.useNewLink')}
      </button>
    </p>
  );
}
```

`ui/src/features/servers/ServerPanel.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useServerStatus } from '../../state/queries';
import { SELECTED_POLL } from '../../state/serverPoll';
import { AddressLine } from './AddressLine';
import { DeadLinkPlate } from './DeadLinkPlate';
import { StatusPill } from './StatusPill';

/** A server instance's live state, in the slot the desktop spec reserved for it. */
export function ServerPanel({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const status = useServerStatus(instance.id, true, SELECTED_POLL);
  const accounts = useAccounts();
  const saved = instance.server;
  const live = status.data;
  const gone = live?.gone ?? saved?.gone ?? false;
  const whitelisted = live?.whitelisted ?? saved?.whitelisted ?? false;
  const account = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active)?.username;
  return (
    <div className="mt-3 grid gap-2">
      <StatusPill status={live ? live.status : (saved?.lastStatus ?? null)} players={live?.players ?? null} gone={gone} />
      <AddressLine address={live?.address ?? saved?.address ?? null} />
      {whitelisted && (
        <p className="invite-only">{account ? t('server.inviteOnly', { name: account }) : t('join.inviteOnlyNoAccount')}</p>
      )}
      {gone && <DeadLinkPlate id={instance.id} />}
    </div>
  );
}
```

`Home.tsx` `InstanceView`: right after `<p className="marquee-plate mt-3">{versionLine(t, instance)}</p>`, add `{isServer(instance) && <ServerPanel instance={instance} />}`.

`UpdatePlate.tsx` (3b's): right after `const update = status.data?.update ?? null;`, insert:

```tsx
  if (isServer(instance)) {
    // Server instances always update when played: the plate only says so, with no Update button.
    return update ? (
      <p className="marquee-plate mb-3" role="status">
        {t('server.updated')}
      </p>
    ) : null;
  }
```

`PlayButton.tsx` (3b's): change the `checking` branch of the label to:

```tsx
      state === 'checking'
        ? progress?.task.key === 'server_pack_wait'
          ? taskLabel(i18n, progress.task)
          : t('play.checking')
```

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): server instances show status, address, invite-only and a dead-link plate; the server banner; the pack wait label

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 19: Menu, settings, Pack link routing and the rail mark

**Files:**
- Modify: `ui/src/features/home/InstanceMenu.tsx`
- Modify: `ui/src/features/instance-settings/InstanceSettings.tsx`
- Modify: `ui/src/features/new-instance/PackSource.tsx` (3b's `PackLinkForm`)
- Modify: `ui/src/features/rail/Rail.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/home/Home.test.tsx`, `ui/src/features/instance-settings/InstanceSettings.test.tsx`, `ui/src/features/new-instance/NewInstance.test.tsx`, `ui/src/features/rail/Rail.test.tsx`

**Interfaces:**
- Consumes `isServer`, `inviteOf`, `addressText`, `inviteHost`, `isInviteLink`, `useServerStatus`, `RAIL_POLL` and `statusWord` (Tasks 15–17), plus 3b's `PackLinkForm`, the Updates fieldset and `RailItem` (Task 0 row 16).
- **Instance menu (server instances only):**
  - **Play without joining** → `commands.play(id, false)`;
  - **Use a new invite link** → `joinServer({ invite: '', relinkFor: id })`;
  - **Copy invite link**;
  - **Copy server address**, only when an address is known.
- **Instance settings:** the Updates fieldset is hidden for server instances. A **Server** section shows "Invite from \<host\>", **Copy link** and **Use a new link**.
- **Pack link tab:** an input that passes `isInviteLink` routes to `joinServer({ invite })`, and `pack_add` is never called for it.
- **Rail:** a server item carries a last-known status mark (`.rail-server`, `data-status`), separate from 3b's lime LED, and its status word is in the aria-label ("Survival, Online · 3/20"). It polls every `RAIL_POLL`.

- [ ] **Step 1: Strings**

Add to `en.json` under `server`:

```json
    "playWithoutJoining": "Play without joining",
    "useNewInviteLink": "Use a new invite link",
    "copyLink": "Copy invite link",
    "settingsTitle": "Server",
    "settingsHost": "Invite from {{host}}",
    "settingsCopyLink": "Copy link"
```

Add to `es.json` under `server`:

```json
    "playWithoutJoining": "Jugar sin unirse",
    "useNewInviteLink": "Usar un nuevo enlace de invitación",
    "copyLink": "Copiar enlace de invitación",
    "settingsTitle": "Servidor",
    "settingsHost": "Invitación de {{host}}",
    "settingsCopyLink": "Copiar enlace"
```

- [ ] **Step 2: Write the failing tests**

`Home.test.tsx`, inside `describe('server instances', …)`:

```tsx
  it('offer the server items in the menu', async () => {
    const { user, calls } = renderApp(<Home id="survival" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Survival' });
    const open = async () => user.click(screen.getByRole('button', { name: 'More options' }));
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Play without joining' }));
    expect(sent(calls, 'play')).toEqual([{ id: 'survival', join: false }]);
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Copy invite link' }));
    expect(await navigator.clipboard.readText()).toBe(`https://play.example.com/${'A'.repeat(43)}`);
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Copy server address' }));
    expect(await navigator.clipboard.readText()).toBe('play.example.com:25566');
    await open();
    await user.click(screen.getByRole('menuitem', { name: 'Use a new invite link' }));
    expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', relinkFor: 'survival' });
  });

  it('keep the server items off plain instances', async () => {
    const { user } = renderApp(<Home id="creative-flat" />, { server: 'online' });
    await screen.findByRole('heading', { name: 'Creative Flat' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    expect(screen.queryByRole('menuitem', { name: 'Play without joining' })).not.toBeInTheDocument();
  });
```

`InstanceSettings.test.tsx`:

```tsx
it('shows a server section and no Updates control for a server instance', async () => {
  const { user } = renderApp(<InstanceSettings id="survival" />, { server: 'online' });
  expect(await screen.findByText('Invite from play.example.com')).toBeInTheDocument();
  expect(screen.queryByRole('radio', { name: 'Automatic' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Copy link' }));
  expect(await navigator.clipboard.readText()).toBe(`https://play.example.com/${'A'.repeat(43)}`);
  await user.click(screen.getByRole('button', { name: 'Use a new link' }));
  expect(routeSnapshot()).toEqual({ name: 'joinServer', invite: '', relinkFor: 'survival' });
});
```

`NewInstance.test.tsx`:

```tsx
it('sends an invite link pasted as a pack link to Join server', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  await user.click(await screen.findByRole('radio', { name: 'Paste a pack link' }));
  const invite = `https://play.example.com/${'B'.repeat(43)}`;
  await user.type(screen.getByRole('textbox', { name: 'Pack link' }), invite);
  await user.click(screen.getByRole('button', { name: 'Add pack' }));
  expect(routeSnapshot()).toEqual({ name: 'joinServer', invite });
  expect(sent(calls, 'pack_add')).toEqual([]);
});
```

`Rail.test.tsx`:

```tsx
it('marks a server with its last status, in words too', async () => {
  const { container } = renderApp(<Rail />, { server: 'online' });
  expect(await screen.findByRole('button', { name: 'Survival, Online · 3/20' })).toBeInTheDocument();
  expect(container.querySelectorAll('.rail-server')).toHaveLength(1);
});
```

Import `routeSnapshot` where a file doesn't yet.

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- Home InstanceSettings NewInstance Rail`
Expected: FAIL.

- [ ] **Step 4: Implement**

`InstanceMenu.tsx`: import `commands`, `isServer`, `inviteOf`, `addressText`, `pushError`, `navigate` and the icons `Play`, `Reload` and `Copy`. Build the server items and put them first:

```tsx
  const invite = inviteOf(instance);
  const address = instance.server?.address ?? null;
  const copy = (text: string) => void navigator.clipboard.writeText(text).catch(pushError);
  const serverItems = isServer(instance)
    ? [
        { label: t('server.playWithoutJoining'), icon: <Play />, onSelect: () => void commands.play(instance.id, false).catch(pushError) },
        { label: t('server.useNewInviteLink'), icon: <Reload />, onSelect: () => navigate({ name: 'joinServer', invite: '', relinkFor: instance.id }) },
        ...(invite ? [{ label: t('server.copyLink'), icon: <Copy />, onSelect: () => copy(invite) }] : []),
        ...(address ? [{ label: t('server.copyAddress'), icon: <Copy />, onSelect: () => copy(addressText(address)) }] : []),
      ]
    : [];
```

Pass `items={[...serverItems, …existing items]}`. Plan 3b's "Check for updates" item stays where 3b put it.

`InstanceSettings.tsx`:
- `const server = isServer(instance);`
- 3b's Updates fieldset condition becomes `pack && !server`.
- Add this section after it:

```tsx
      {server && (
        <section className="grid gap-3" aria-labelledby="server-settings">
          <h2 id="server-settings" className="text-lg font-bold">
            {t('server.settingsTitle')}
          </h2>
          <p>{t('server.settingsHost', { host: inviteHost(inviteOf(instance) ?? '') ?? '' })}</p>
          <div className="flex flex-wrap gap-3">
            <Button icon={<Copy />} onClick={() => void navigator.clipboard.writeText(inviteOf(instance) ?? '').catch(pushError)}>
              {t('server.settingsCopyLink')}
            </Button>
            <Button onClick={() => navigate({ name: 'joinServer', invite: '', relinkFor: instance.id })}>{t('server.useNewLink')}</Button>
          </div>
        </section>
      )}
```

Import `isServer`, `inviteOf`, `inviteHost`, `Copy` and `pushError`.

`PackSource.tsx` `PackLinkForm.submit` (3b's): before `add.mutate(…)`, add:

```tsx
    // A server invite isn't a pack link: confirm the server first (spec §2.3).
    if (isInviteLink(url)) {
      navigate({ name: 'joinServer', invite: url.trim() });
      return;
    }
```

`Rail.tsx`:
- `Item` gains `server: boolean; lastStatus: ServerStatus | null; gone: boolean`. Fill them from `i.source.kind === 'perland'`, `i.server?.lastStatus ?? null` and `i.server?.gone ?? false`; damaged items get `false`, `null` and `false`.
- In `RailItem`:

```tsx
  const live = useServerStatus(item.id, item.server, RAIL_POLL);
  const serverWord = item.server
    ? statusWord(t, live.data ? live.data.status : item.lastStatus, live.data?.players ?? null, live.data?.gone ?? item.gone)
    : null;
  const label = [item.name, status, serverWord].filter(Boolean).join(', ');
```

  Use `aria-label={label}` instead of 3b's `status ? … : item.name`. Inside the button, after 3b's LED, render:

```tsx
      {item.server && <span className="rail-server" data-status={live.data?.gone || item.gone ? 'unknown' : (live.data?.status ?? item.lastStatus ?? 'unknown')} aria-hidden />}
```

- Add a placeholder `.rail-server` rule in `app.css`: a small mark in the opposite corner from the LED, never lime. Task 20 decides how it looks.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint && pnpm --filter ui build`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): server menu items, a Server section in settings, invite links from Pack link, the rail status mark

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 20: `/impeccable` craft and finish review of the server surfaces

CLAUDE.md requires that UI styling goes through `/impeccable`. Tasks 17–19 built these surfaces with placeholder classes. This task makes them belong to the Arcade Cabinet world, and checks that with the finish reviewer. Behaviour, strings and tests stay as they are.

**Files:** whatever `/impeccable` touches under `ui/src` (mostly `ui/src/styles/app.css` and the server components), plus `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md` if it records new components.

**Interfaces:** none change. Every test from Tasks 15–19 must still pass unmodified, except selectors `/impeccable` deliberately renames, which it updates in the same commit.

- [ ] **Step 1: Run `/impeccable` on the surfaces**

Invoke the `/impeccable` skill, pointing it at `.impeccable/surfaces/ui-src-app-tsx.md` and `DESIGN.md`, with this brief:
- **Surfaces:**
  - the Join server screen: link field, server card, invite-only line, mismatch warning, the action row;
  - `StatusPill`: online, starting, sleeping, offline, unknown;
  - the MOTD colours (`.mc-0`…`.mc-f`): legible on `panel` and on the marquee plate;
  - the team mark (cyan, blue, violet, magenta, other): small, never a second lit element;
  - the server icon at integer pixel scale (64 × 64 PNG → 64 px or 128 px, `image-rendering: pixelated`);
  - `ServerPanel` in the marquee plate column: pill, address line with Copy, invite-only line;
  - the dead-link plate (quiet, a plate, not an error card);
  - 3b's banner in its server variant;
  - the instance settings Server section;
  - the rail's server status mark.
- **Rules from the spec:**
  - One lit button per view: Play, or Join server's primary.
  - Lime is the selection lamp and the "Playing" LED, so the rail's server mark must not be confusable with it (shape, position or hue).
  - Blue is progress.
  - Status is always in words.
- **Open question for `/impeccable` to decide:** the spec asks for an "amber" invite-only line, but `DESIGN.md` reserves amber for acting ("The Hue Has One Job Rule"). Pick a treatment that fits `DESIGN.md`, and note the choice in `.impeccable/surfaces/ui-src-app-tsx.md`.

- [ ] **Step 2: Finish review**

Dispatch the `impeccable-finish-reviewer` agent over the result. Fix every material finding it returns, and repeat until it has none.

- [ ] **Step 3: Check it in the mock**

```bash
pnpm --filter ui dev:mock
```

Open each scenario and look at Home, Join server and the rail:
- `/?server=online`, `?server=vanilla`, `?server=pending` (press Play on Survival to see "Server is preparing the modpack…");
- `?server=whitelisted`, `?server=gone`, `?server=network`.

For Join server, use the rail's + → Pack link → paste `https://play.example.com/BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB` → Add pack.

- [ ] **Step 4: Full gate**

```bash
pnpm --filter ui build
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm -r lint
pnpm -r typecheck
pnpm -r test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add ui/src DESIGN.md .impeccable
git commit -m "style(ui): server surfaces in the Arcade Cabinet world, through /impeccable

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 21: Nightly fuzz targets (CI only)

Never build or run these on this PC. They run in the nightly workflow on Linux with nightly Rust.

**Files:**
- Create: `fuzz/fuzz_targets/invite_link.rs`, `fuzz/fuzz_targets/join_deep_link.rs`
- Modify: `fuzz/Cargo.toml`, `.github/workflows/nightly.yml`

**Interfaces:**
- Consumes `InviteLink` (Task 2) and `launcher_app::deep_link::{parse, DeepLink}` (Task 12; the module is `pub` since Task 13).
- Produces two cargo-fuzz targets, `invite_link` and `join_deep_link`. They assert invariants rather than just "no panic":
  - a parsed link's canonical form re-parses to the same link;
  - `Debug` and `Display` never carry the token;
  - a join deep link always carries a canonical https invite.

- [ ] **Step 1: The targets**

`fuzz/fuzz_targets/invite_link.rs`:

```rust
#![no_main]
use launcher_core::invite::InviteLink;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(link) = InviteLink::parse(text) {
            let canonical = link.canonical();
            let again = InviteLink::parse(&canonical).expect("a canonical link parses");
            assert_eq!(again, link, "the canonical form is stable");
            assert_eq!(again.canonical(), canonical);
            assert!(canonical.starts_with("https://") || canonical.starts_with("http://"));
            // The host may happen to contain the token's characters; nothing else may.
            for shown in [format!("{link:?}"), link.to_string()] {
                assert!(
                    !shown.replace(link.host(), "").contains(link.token()),
                    "the token leaked"
                );
            }
        }
    }
});
```

`fuzz/fuzz_targets/join_deep_link.rs`:

```rust
#![no_main]
use launcher_app::deep_link::{parse, DeepLink};
use launcher_core::invite::InviteLink;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        match parse(text) {
            Ok(DeepLink::Join { invite }) => {
                assert!(invite.starts_with("https://"), "only https invites come from the web");
                let link = InviteLink::parse(&invite).expect("a join link carries a valid invite");
                assert_eq!(link.canonical(), invite, "and it is canonical");
            }
            Ok(DeepLink::Pack { url }) => assert!(url.starts_with("https://")),
            Err(_) => {}
        }
    }
});
```

- [ ] **Step 2: Register them**

In `fuzz/Cargo.toml`, under `[dependencies]`, add `launcher-app = { path = "../crates/launcher-app" }`. Then:

```toml
[[bin]]
name = "invite_link"
path = "fuzz_targets/invite_link.rs"
test = false
doc = false
bench = false

[[bin]]
name = "join_deep_link"
path = "fuzz_targets/join_deep_link.rs"
test = false
doc = false
bench = false
```

In `.github/workflows/nightly.yml`, add `invite_link join_deep_link` to the target list:

```yaml
          for target in packwiz_pack packwiz_index packwiz_meta mrpack_index sidecar pack_paths invite_link join_deep_link; do
```

Rename the step to "Fuzz each pack and link parser for 3 minutes".

- [ ] **Step 3: Commit**

```bash
git add fuzz .github/workflows/nightly.yml
git commit -m "ci(nightly): fuzz invite links and join deep links (CI only)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 4: Verify in CI, not here**

Once the branch is pushed (Task 22), run the nightly workflow on it and read its fuzz job:

```bash
gh workflow run nightly.yml --ref "$(git branch --show-current)"
gh run list --workflow nightly.yml --limit 1
gh run watch <id>
```

Expected: the `fuzz` job builds both new targets and finishes green. A crash uploads `fuzz-crashes`. Reproduce it by adding the crashing input as a unit-test case (tiny input, local), never by fuzzing locally.

---

### Task 22: Whole-branch verification, owner smoke test, PR

- [ ] **Step 1: Rebase and run the full gate**

```bash
git fetch origin && git rebase origin/main
pnpm --filter ui build
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm -r lint
pnpm -r typecheck
pnpm -r test
```

Expected: every command passes. Record the output (test counts) for the PR body.

- [ ] **Step 2: Token check over the repo**

```bash
git log -p origin/main..HEAD | grep -E "/[A-Za-z0-9_-]{43}([/\"' ]|$)" | grep -vE "(A{43}|B{43}|Z{43}|b4sE64url_-)" || echo "no real-looking tokens"
```

Expected: `no real-looking tokens`. Anything else is a real token in the diff: remove it from history before pushing.

- [ ] **Step 3: Whole-branch review**

Use superpowers:requesting-code-review on the whole branch (`origin/main..HEAD`), with this plan's Review Focus as the brief. Fix the findings it confirms.

- [ ] **Step 4: Owner smoke test (real app, real Asalith invite)**

The Asalith server runs on Perland with its invite turned on. **The token never goes into a chat, a PR, a commit, a screenshot or a log.** The owner copies the link from the Panel's invite sheet and pastes it only into the launcher.

Ask the owner to run, in PowerShell:

```powershell
$env:PERLAND_LAUNCHER_ROOT = "E:\perland-scratch\join"; pnpm tauri dev
```

Then check, in order:
1. **Paste:** New instance → **Pack link** → paste the Asalith invite link → **Add pack**.
   - **Join server** opens: the icon, the name, the MOTD in its colours, the status pill with players, the version and pack lines, and "From \<invite host\>".
   - Nothing downloads yet.
2. **Add server:** the instance opens. It shows the pill, the address line (Copy works) and Asalith's icon in the rail with a status mark.
3. **Play:**
   - The button shows "Checking for updates…", or "Server is preparing the modpack…" if the Engine is still building, then "Installing \<name\>" with files and MB.
   - Minecraft starts and **joins the server directly**, without the multiplayer menu.
   - Quit the game.
4. **Menu → Play without joining:** the game starts at the title screen. Quit.
5. **Redeploy:** the owner pushes a modpack change to Asalith and redeploys it in the Panel.
   - Within a minute, the plate says "Server updated · updates when you play", with no Update button.
   - **Play** syncs the new commit and joins.
6. **Deep link:** with the app running, open the join link for the same invite, built locally from the clipboard (the link never leaves the PC):

   ```powershell
   $u = [uri]::EscapeDataString((Get-Clipboard)); Start-Process "perland://join?invite=$u"
   ```

   The launcher comes forward on **Join server** and offers **Open**, because this server is already added.
7. **Offline:** turn the network off and press **Play**. A "Couldn't check for updates" toast appears, the installed version plays and tries the last address. Turn the network back on.
8. **Regenerate** the invite in the Panel:
   - Within about 30 s the dead-link plate shows. **Play** still plays and joins the last known address.
   - **Use a new link** → paste the new invite → **Use this link**: the plate goes away.
   - **Play** is up to date (same commit) and joins.
9. **No token in logs:** with the *new* invite link on the clipboard:

   ```powershell
   $t = Split-Path -Leaf (Get-Clipboard)
   Get-ChildItem -Recurse "E:\perland-scratch\join\logs" | Select-String -SimpleMatch $t
   ```

   Expected: no output. `instances\<id>\instance.json` and `.perland\pack.json` do hold it. That's expected, like the rest of the instance state.

The invite page's **Open in Perland Launcher** button and the Panel's **Open in launcher** need the Engine and Panel plans deployed with `LAUNCHER_DOWNLOAD_URL` set, so they aren't part of this smoke test. Step 6 covers the same deep link.

- [ ] **Step 5: PR**

Push the branch and open a PR against `main`. The body has these sections, then the attribution line:
- **Summary**
- **Owner decisions** (spec §1.1)
- **What changed per layer:** core, app, Tauri, UI, CI
- **Verification:** test counts, gate output, the owner smoke results (no tokens)
- **Follow-ups**

```
🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Bind the PR with the ccd_pr tools and read its CI. Then run Task 21 Step 4 (the nightly fuzz on this branch).

## Deferred

- **Networks:** refused with their address until the Engine exposes a version or pack for them (spec §1.1 decision 2).
- **Whitelist requests:** the Engine has no public request-to-join.
- **Refreshing a stale saved `engine`:** `SourceRef::Perland.engine` is only a hint. A moved Engine is found again through `config.json` after a network error, but the stored value isn't rewritten. Revisit if Engines move often.
- **Installer-time `perland://` registration:** Plan 6, with bundling (as in 3b).
