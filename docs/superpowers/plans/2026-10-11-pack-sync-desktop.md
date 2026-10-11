# Pack Sync in the Desktop App (Plan 3b of 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Players use packs from the desktop app.
- **Adding a pack:** a link, a `.mrpack`, `modrinth:` or a `perland://pack?url=` link.
- **Play:** checks for an update, asks for `ask` packs, syncs with progress on the Play button, then launches.
- **Between plays:** a banner offers the update.
- **Mods:** the pack's mods are separate from the player's own additions.
- **Blocked CurseForge files:** a guided manual-download dialog.
- **Art:** the pack's own art.

**Architecture:**
- **One run:** `launcher-app`'s run state machine gains three phases: `checking`, `asking` and `syncing`. The order is `checking` → `asking` → `queued` → `syncing` → `preparing` → `running`.
- **The lock:** the run holds the instance's `InstanceLock` from the update check until the game exits.
- **Sync:** runs under the existing `install_slot`.
- **Seams:** `launcher-core` gains small additive APIs (`find_update`, `ensure_locked` with a cooperative `Cancel`, a caching `manual::Scanner`). The app reaches them through its existing `Runner` seam, so every app test uses `FakeRunner`.
- **Wiring and UI:** `src-tauri` only forwards new commands and wires the deep-link plugin. The React UI adds the update dialog, banner plate, pack tabs, Mods sections, the manual-download dialog and pack art, styled through `/impeccable`.

**Tech Stack:**
- Rust stable (1.99, edition 2021): tokio, serde.
- New dependencies:
  - `launcher-app`: `url` 2 (deep-link parsing) and `base64` 0.22 (art data URLs).
  - `src-tauri`: `tauri-plugin-deep-link` 2.6.1, plus the `deep-link` feature of `tauri-plugin-single-instance`.
- UI: React 19, TanStack Query, zod 4, i18next, Vitest + Testing Library, `@tauri-apps/plugin-opener` (already a dependency).

**Spec:**
- `docs/superpowers/specs/2026-10-10-pack-sync-design.md`. **§4 is the 3b design (owner-approved 2026-10-11)**; §1–§3 are context.
- Parent specs: `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (§3, §5) and `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md`.
- Design system: `DESIGN.md` and the contract `.impeccable/surfaces/ui-src-app-tsx.md`.
- The Plan 3a plan (`docs/superpowers/plans/2026-10-10-pack-sync-core.md`) shows the core code this plan builds on.

## Global Constraints

- **License:** GPL-3.0-only. No Modrinth or Mojang branding or assets. Nothing is ported, so `NOTICE` doesn't change.
- **Crate boundaries:**
  - `launcher-core` and `launcher-app` never depend on Tauri or a UI crate.
  - `src-tauri` is wiring only (`test = false`). Every behavior and its test lives in `launcher-app` or `launcher-core`.
- **Contract:**
  - `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
  - The zod schemas in `ui/src/ipc/schema.ts` are strict objects and must match the Rust serde shapes exactly.
- **Strings:**
  - Every UI string goes in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json`. Spanish is neutral Latin American with informal "tú"; the existing files say "pack".
  - Every new `ErrorCode` gets `errors.<code>.message` in both files.
  - Copy is short and casual.
  - The owner's exact strings:
    - "Update ready: 1.0.3 → 1.0.4" / "¡Actualización lista!: 1.0.3 → 1.0.4"
    - **Update and play**, **Play 1.0.3**, **Update** / **Actualizar**
    - "+3 added · 2 updated · 1 removed"
    - "Kept your version of N files"
    - "Checking for updates…" / "Buscando actualizaciones…"
- **Design system:**
  - One lit button per view: Play, or the dialog's primary. The banner's Update is never lit.
  - Lime lamp for selection, blue for progress.
  - Corners are pixel-stepped and shadows are hard offsets.
  - New surfaces are styled through the `/impeccable` skill (Task 21).
- **Security:**
  - The `perland://` link is remote input. Only an `https://` packwiz link gets through, and nothing downloads before the player clicks **Add pack**.
  - Paths from the webview are checked by kind: `.jar`, `.mrpack`, `java`/`javaw`.
  - Pack art is shown only through `<img src="data:…">`, never inlined, so an SVG can't run script.
  - Messages name hosts, never full URLs. Tokens never reach logs.
- **Stable APIs:** another session (Plan 4, Microsoft sign-in) may be changing `accounts`/`auth` and the Accounts screen. Don't touch those.
  - Changes are additive. Existing public signatures keep working, except two deliberate breaks:
    - `CoreRunner::new` takes a `Syncer`. Its only caller is `src-tauri`.
    - `AppConfig` gains `downloads_dir`. Its callers are `src-tauri` and the tests.
  - Rebase onto `main` before the PR.
- **Heavy tests:** fuzzing, stress and load tests run only in CI, **never on this PC**. Local test inputs stay tiny.
- **Commits:** every commit message ends with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Before each commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - When `ui/` changed, also: `pnpm --filter ui build`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
- **Machine notes:**
  - No Python on this PC; use Node 24 for scripts.
  - Windows Smart App Control sometimes blocks a fresh unsigned test exe (os error 4551). That's environmental; rerun.
  - The owner's terminal is PowerShell, so give them PowerShell commands.

## Review Focus

1. **Play again right after cancelling a pack run.**
   - The cancelled task drops its `InstanceLock` a moment after the `idle` event.
   - **Expected:** the new run just works. No `instance_locked`.
   - **Pinned by:** `play_again_right_after_a_cancel_gets_the_lock` (Task 8).
2. **Stop during a long first install or update.**
   - **Expected:** the sync stops at the next file and never starts the swap. The game folder is untouched. The lock is released only after nothing is writing any more.
   - **Pinned by:** `a_cancelled_sync_stops_before_touching_the_game_folder` (Task 4) and `stop_while_syncing_waits_for_the_sync_to_stop` (Task 8).
3. **The CLI or a second window holding the same instance.**
   - **Expected:** the run ends with `instance_locked` (an error card with a fix), and nothing is changed. While the app's run holds the lock (checking through game exit), nobody else gets it.
   - **Pinned by:** `the_lock_is_held_from_check_to_game_exit` and `a_locked_instance_ends_the_run_with_instance_locked` (Task 8).
4. **A hostile `perland://` link.**
   - **Inputs:** `http://`, loopback http, a `.mrpack` path, `modrinth:`, two `url` params, another host such as `join`, junk.
   - **Expected:** a `pack_link_invalid` warning toast. The UI never navigates.
   - **Pinned by:** `deep_links_accept_only_https_pack_links` and `a_bad_deep_link_warns_and_keeps_nothing` (Task 11).
5. **A pack update that changes the Minecraft or loader version.**
   - **Expected:** the run prepares the new game version, and the UI's version line updates after the run.
   - **Pinned by:** `a_pack_update_that_changes_the_game_prepares_the_new_version` (Task 8) and "an ended run refreshes the instance and its pack queries" (Task 13).

## File Map

| File | Responsibility |
|---|---|
| `crates/launcher-core/src/http.rs` | 10-hop limit; whole-chain https→http refusal |
| `crates/launcher-core/src/sources/modrinth.rs` | update check survives a deleted installed version |
| `crates/launcher-core/src/error.rs` | `SyncCancelled`, `UpdateDownloadFailed`, `ModManaged` |
| `crates/launcher-core/src/sync/mod.rs` | `Cancel`, `PendingUpdate`, `find_update`, `ensure_locked`, `import_found`; `check` honors Pinned; `install_pack` task |
| `crates/launcher-core/src/sync/manual.rs` | `Scanner` (remembers digests across polls) |
| `crates/launcher-core/tests/{sync,modrinth}.rs` | integration tests for the above |
| `crates/launcher-app/src/dto.rs` | `PackUpdate`, `SyncSummary`, `PackStatus`, `PackContents`, `PackMod`, `OptionalChoice`, `ManualProgress`, `PackArtData`, `PackAddRequest`, `RunSnapshot.update`, `OpenTarget::Downloads` |
| `crates/launcher-app/src/events.rs` | new run phases, `ExitInfo` additions, `RunState.update`, `DeepLink` event |
| `crates/launcher-app/src/runner.rs` | `Runner` pack methods; `CoreRunner` holds a `Syncer` |
| `crates/launcher-app/src/runs.rs` | the run state machine with lock, check, ask, sync, update-only runs |
| `crates/launcher-app/src/packs.rs` (new) | `pack_add`, `pack_status`, `pack_contents`, `pack_set_optional`, `manual_import`, `pack_art`, mod guard |
| `crates/launcher-app/src/deep_link.rs` (new) | `perland://pack?url=` parsing |
| `crates/launcher-app/src/app.rs` | new state fields (`manual`, `pending_link`), `AppConfig.downloads_dir`, `java_test` path check, `folder(Downloads)` |
| `crates/launcher-app/src/testing.rs` | `FakeRunner` pack scripting; pack fixtures |
| `crates/launcher-app/tests/contract.rs` | fixtures for every new type and event |
| `src-tauri/src/{main,commands}.rs`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json` | wiring, deep link, opener allow-list |
| `ui/src/ipc/{schema,commands,mock}.ts` | schemas, commands, mock pack backend |
| `ui/src/state/{runStore,handleEvent,hydrateRuns,queries,route,deepLink}.ts` | run view `update`, deep link, query invalidation |
| `ui/src/ui/PlayButton.tsx` | `checking` / `asking` / `syncing` |
| `ui/src/features/home/{UpdateDialog,ChangeSummary,UpdatePlate,ManualDownloads}.tsx` | new surfaces |
| `ui/src/features/home/{Home,InstanceMenu}.tsx` | mounting them; "Check for updates" |
| `ui/src/features/new-instance/{NewInstance,PackSource}.tsx` | pack link and `.mrpack` modes |
| `ui/src/features/mods/Mods.tsx` | "From the pack" and "Your additions" |
| `ui/src/features/instance-settings/InstanceSettings.tsx` | Updates policy; game version set by the pack |
| `ui/src/art/{Marquee.tsx,usePackArt.ts}`, `ui/src/features/rail/Rail.tsx` | pack art |
| `ui/src/i18n/{en,es}.json` | every new string |

---

### Task 1: Redirects follow ten hops and never drop from https anywhere in the chain

**Files:**
- Modify: `crates/launcher-core/src/http.rs`

**Interfaces:**
- Produces `pub fn allow_redirect_chain(previous: &[reqwest::Url], to: &reqwest::Url) -> bool`. `allow_redirect` stays as is.

- [ ] **Step 1: Write the failing tests**

In `crates/launcher-core/src/http.rs` `mod tests`, after `redirects_never_leave_https_for_the_network`:

```rust
    #[test]
    fn a_chain_that_was_on_https_never_drops_to_network_http() {
        // https → loopback http is fine, but the next hop to the network must not be plain http.
        let chain = [url("https://a.example/p"), url("http://localhost:1/q")];
        assert!(!allow_redirect_chain(&chain, &url("http://b.example/r")));
        assert!(allow_redirect_chain(&chain, &url("http://127.0.0.1:2/r")));
        assert!(allow_redirect_chain(&chain, &url("https://b.example/r")));
        let plain = [url("http://a.example/p")];
        assert!(allow_redirect_chain(&plain, &url("http://b.example/q")));
        assert!(!allow_redirect_chain(
            &[url("https://a.example/p")],
            &url("ftp://a.example/q")
        ));
    }

    #[tokio::test]
    async fn ten_redirects_are_followed_and_the_eleventh_is_refused() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        for n in 0..11 {
            Mock::given(method("GET"))
                .and(path(format!("/hop/{n}")))
                .respond_with(
                    ResponseTemplate::new(302).insert_header("location", format!("/hop/{}", n + 1)),
                )
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/hop/11"))
            .respond_with(ResponseTemplate::new(200).set_body_string("end"))
            .mount(&server)
            .await;
        let client = client("test").unwrap();
        // /hop/1 → … → /hop/11 is ten redirects.
        let body = client
            .get(format!("{}/hop/1", server.uri()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert_eq!(body, "end");
        // /hop/0 → … → /hop/11 is eleven.
        let err = client
            .get(format!("{}/hop/0", server.uri()))
            .send()
            .await
            .unwrap_err();
        assert!(err.is_redirect());
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --lib http::`
Expected: a compile error (`allow_redirect_chain` not found). After a temporary stub, `ten_redirects…` fails because the 10th redirect is refused.

- [ ] **Step 3: Implement**

Replace the policy closure in `client`:

```rust
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // `previous` holds the original URL and every redirect followed so far.
            if attempt.previous().len() > MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if !allow_redirect_chain(attempt.previous(), attempt.url()) {
                attempt.error("refused a redirect from https to plain http")
            } else {
                attempt.follow()
            }
        }))
```

Add below `allow_redirect`:

```rust
/// May a request that has visited `previous` (oldest first) follow a redirect to `to`? Each hop
/// must pass `allow_redirect`, and once any URL in the chain was https, plain http is allowed
/// only to this computer: a detour through a loopback http hop doesn't reopen the network.
pub fn allow_redirect_chain(previous: &[reqwest::Url], to: &reqwest::Url) -> bool {
    if previous.last().is_some_and(|last| !allow_redirect(last, to)) {
        return false;
    }
    let was_secure = previous.iter().any(|u| u.scheme() == "https");
    !(was_secure && to.scheme() == "http" && !is_loopback(to))
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-core --lib http::`
Expected: PASS. The existing `loopback_redirects_are_still_followed` (a `/loop` that never ends) still errors with `is_redirect()`.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/http.rs
git commit -m "fix(http): follow ten redirects and refuse plain http anywhere after https

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Update checks survive a deleted Modrinth version and respect Pinned

**Files:**
- Modify: `crates/launcher-core/src/sources/modrinth.rs` (`check`)
- Modify: `crates/launcher-core/src/sync/mod.rs` (`Syncer::check`)
- Test: `crates/launcher-core/tests/modrinth.rs`, `crates/launcher-core/tests/sync.rs`

**Interfaces:**
- `modrinth::check(src, project_id, installed_version_id, minecraft) -> UpdateCheck` keeps its signature.
- `Syncer::check(&self, store, instance) -> UpdateCheck` now returns `UpToDate` for `UpdatePolicy::Pinned` without touching the network.

- [ ] **Step 1: Write the failing tests**

Append to `crates/launcher-core/tests/modrinth.rs`:

```rust
#[tokio::test]
async fn a_deleted_installed_version_still_finds_updates() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    sky(&server, tmp.path()).await;
    let src = sources(&server, tmp.path());
    // "GONE" was deleted upstream: GET /v2/version/GONE answers 404. Releases are still offered.
    assert_eq!(
        modrinth::check(&src, "P1", "GONE", "1.20.1").await,
        UpdateCheck::Available {
            source: SourceRef::Modrinth {
                project_id: "P1".into(),
                version_id: "R1".into()
            },
            stamp: "R1".into(),
            version: "R1-num".into(),
        }
    );
}
```

In `crates/launcher-core/tests/sync.rs`, in `pinned_instances_stay_on_their_version`, right after the `.mount(&env.server).await;` that publishes `9.9.9`, add:

```rust
    assert_eq!(
        env.syncer.check(&env.store, &instance).await,
        UpdateCheck::UpToDate,
        "a pinned instance has no updates to offer"
    );
```

The test's final `hits(&env.server, "/pack/pack.toml") == 0` assertion now also covers `check`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-core --test modrinth a_deleted && cargo test -p launcher-core --test sync pinned`
Expected: both FAIL. The Modrinth check returns `Unavailable`, and the pinned check returns `Available` and fetches `pack.toml`.

- [ ] **Step 3: Implement**

In `sources/modrinth.rs` `check`, replace the `installed` lookup and the final `match`:

```rust
    // A version deleted upstream must not hide updates forever: compare by id against releases.
    let installed = match version(src, installed_version_id).await {
        Ok(v) => Some(v),
        Err(e) if e.code == ErrorCode::PackNotFound => None,
        Err(e) => return failed(e),
    };
    let mut list = match versions(src, project_id, Some(minecraft)).await {
        Ok(list) => list,
        Err(e) => return failed(e),
    };
    // The query filters too, but updates must never move to another Minecraft version.
    list.retain(|v| v.game_versions.iter().any(|g| g == minecraft));
    let installed_type = installed
        .as_ref()
        .map_or("release", |v| v.version_type.as_str());
    match pick_update(&list, installed_type) {
        Some(v)
            if v.id != installed_version_id
                && installed
                    .as_ref()
                    .is_none_or(|i| v.date_published > i.date_published) =>
        {
            UpdateCheck::Available {
                source: SourceRef::Modrinth {
                    project_id: project_id.to_string(),
                    version_id: v.id.clone(),
                },
                stamp: v.id.clone(),
                version: v.version_number.clone(),
            }
        }
        _ => UpdateCheck::UpToDate,
    }
```

In `sync/mod.rs`, replace `Syncer::check`:

```rust
    /// Is there an update? `UpToDate` for a pinned instance (it never updates) and for one that
    /// was never synced (its first sync installs the newest version anyway).
    pub async fn check(&self, store: &InstanceStore, instance: &Instance) -> UpdateCheck {
        if instance.update_policy == UpdatePolicy::Pinned {
            return UpdateCheck::UpToDate;
        }
        match self.installed(store, &instance.id).await {
            Some(installed) => self.sources.check(&instance.source, &installed).await,
            None => UpdateCheck::UpToDate,
        }
    }
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-core --test modrinth && cargo test -p launcher-core --test sync`
Expected: PASS, including the existing checks.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sources/modrinth.rs crates/launcher-core/src/sync/mod.rs crates/launcher-core/tests/modrinth.rs crates/launcher-core/tests/sync.rs
git commit -m "fix(sync): update checks survive a deleted Modrinth version and skip pinned packs

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Error codes and strings for 3b

Every later task uses these, so they land first, together with their translations and the regenerated code list.

**Files:**
- Modify: `crates/launcher-core/src/error.rs` (the `error_codes!` list)
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Produces `ErrorCode::SyncCancelled`, `ErrorCode::UpdateDownloadFailed` and `ErrorCode::ModManaged` (snake_case on the wire).
- Produces the i18n key `progress.install_pack` ("Installing {{name}}").

- [ ] **Step 1: Add the codes**

In `error.rs`'s `error_codes!`:
- Change the mods line to `ModNotJar, ModExists, ModNotFound, ModNameInvalid, ModManaged,`.
- Extend the packs line to end with `PackPathIsFolder, SyncCancelled, UpdateDownloadFailed,`.

- [ ] **Step 2: Run the guard tests to see them fail**

Run: `cargo test -p launcher-app --test contract`
Expected: FAIL, "error_codes.json is stale".

- [ ] **Step 3: Translate and regenerate**

In `ui/src/i18n/en.json`:
- Under `"progress"` add `"install_pack": "Installing {{name}}"`.
- Under `"errors"` add:

```json
    "mod_managed": { "message": "{{file}} comes with the pack, so the pack decides whether it's on.", "fix": "Mods you add yourself can be turned off or removed." },
    "sync_cancelled": { "message": "The update was cancelled." },
    "update_download_failed": { "message": "Couldn't download the update, so you're playing {{version}}.", "fix": "Try updating again later." },
```

In `ui/src/i18n/es.json`:
- Under `"progress"` add `"install_pack": "Instalando {{name}}"`.
- Under `"errors"` add:

```json
    "mod_managed": { "message": "{{file}} viene con el pack, así que el pack decide si está activado.", "fix": "Los mods que agregues tú sí puedes desactivarlos o quitarlos." },
    "sync_cancelled": { "message": "Se canceló la actualización." },
    "update_download_failed": { "message": "No se pudo descargar la actualización, así que juegas la {{version}}.", "fix": "Intenta actualizar más tarde." },
```

Then regenerate: `$env:UPDATE_CONTRACT = "1"; cargo test -p launcher-app --test contract; Remove-Item Env:UPDATE_CONTRACT` (or `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract` in bash).

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-app --test contract && pnpm --filter ui test -- i18n contract`
Expected: PASS. The i18n test checks every code is translated in both languages with the same placeholders.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/error.rs ui/src/i18n ui/src/ipc/contract/error_codes.json
git commit -m "feat(core): error codes for cancelled syncs, failed update downloads and pack mods

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: `find_update`, `ensure_locked`, cancelling, and falling back to the installed version

The app must hold one `InstanceLock` across the update check, the player's answer, the sync and the game. So `ensure` is split into steps the app can call while it holds the lock. `ensure` itself keeps its behavior for the CLI.

**Files:**
- Modify: `crates/launcher-core/src/sync/mod.rs`
- Test: `crates/launcher-core/src/sync/mod.rs` (`#[cfg(test)] mod tests`, new), `crates/launcher-core/tests/sync.rs`

**Interfaces:**
- Consumes `ErrorCode::{SyncCancelled, UpdateDownloadFailed}` (Task 3).
- Produces in `launcher_core::sync`:

```rust
#[derive(Debug, Clone, Default)]
pub struct Cancel(/* Arc<AtomicBool> */);
impl Cancel { pub fn cancel(&self); pub fn is_cancelled(&self) -> bool; }

#[derive(Debug, Clone, PartialEq)]
pub struct PendingUpdate {
    pub source: SourceRef,      // what the instance's source becomes once installed
    pub desired: DesiredState,
    pub changes: Changes,       // what the update adds, updates, removes (kept edits excluded)
    pub kept: Vec<String>,      // paths of override files the player edited that it keeps
}

impl Syncer {
    pub async fn find_update(&self, store: &InstanceStore, instance: &Instance) -> Result<Option<PendingUpdate>>;
    pub async fn ensure_locked(&self, lock: &InstanceLock, store: &InstanceStore, id: &str,
        accept: Option<PendingUpdate>, sink: &EventSink, cancel: &Cancel) -> Result<Ensured>;
}
```

- `find_update` returns `Err` for a failed check: an `update_check_failed` warning, after which the installed version plays. It returns `Ok(None)` when the pack is pinned, never synced or up to date.
- `ensure_locked` behaves like this:
  - Never synced: resolves the source and syncs.
  - `accept` set: syncs the update. A `network`-category failure falls back to the installed version with an `update_download_failed` warning (param `version`).
  - Otherwise: syncs the installed version.
  - A cancel before the swap fails with `sync_cancelled`.
- The first sync's progress task is `install_pack`; later syncs use `sync_pack`.

- [ ] **Step 1: Write the failing integration tests**

Append to `crates/launcher-core/tests/sync.rs` (`v1`, `synced`, `Env`, `done`, `hits` already exist there). Add `sync::{Cancel, PendingUpdate}` to the `launcher_core::sync` import, and `events::Event` is already imported.

```rust
/// v1 plus "jei", minus "zoom", published as 1.1.0.
async fn publish_v1_1(env: &Env) {
    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "1.1.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .file("config/a.toml", b"a = 1", true)
        .mount(&env.server)
        .await;
}

async fn update_for(env: &Env, instance: &Instance) -> PendingUpdate {
    let fresh = env.store.load(&instance.id).await.unwrap();
    env.syncer
        .find_update(&env.store, &fresh)
        .await
        .unwrap()
        .expect("an update")
}

#[tokio::test]
async fn find_update_previews_without_downloading() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;
    assert_eq!(
        env.syncer.find_update(&env.store, &instance).await.unwrap(),
        None,
        "up to date"
    );
    publish_v1_1(&env).await;
    let update = update_for(&env, &instance).await;
    assert_eq!(update.changes.from_version.as_deref(), Some("1.0.0"));
    assert_eq!(update.changes.to_version, "1.1.0");
    assert_eq!(update.changes.added, vec!["jei".to_string()]);
    assert_eq!(update.changes.removed, vec!["zoom".to_string()]);
    assert!(update.kept.is_empty());
    assert_eq!(update.desired.pack_version, "1.1.0");
    assert_eq!(hits(&env.server, "/files/jei.jar").await, 0, "a preview downloads nothing");

    let mut pinned = env.store.load(&instance.id).await.unwrap();
    pinned.update_policy = UpdatePolicy::Pinned;
    assert_eq!(env.syncer.find_update(&env.store, &pinned).await.unwrap(), None);
}

#[tokio::test]
async fn ensure_locked_installs_an_accepted_update_and_names_it_in_the_source() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;
    publish_v1_1(&env).await;
    let update = update_for(&env, &instance).await;
    let lock = InstanceLock::try_acquire(&env.store.perland_dir(&instance.id)).unwrap();
    let ensured = env
        .syncer
        .ensure_locked(&lock, &env.store, &instance.id, Some(update), &env.sink(), &Cancel::default())
        .await
        .unwrap();
    assert!(ensured.warning.is_none());
    assert_eq!(done(ensured.outcome).changes.to_version, "1.1.0");
    assert!(env.game_file(&instance.id, "mods/jei.jar").exists());
    assert_eq!(
        env.syncer.installed(&env.store, &instance.id).await.unwrap().pack_version,
        "1.1.0"
    );
    // Without an accepted update, the installed version is kept (fast path).
    let again = env
        .syncer
        .ensure_locked(&lock, &env.store, &instance.id, None, &env.sink(), &Cancel::default())
        .await
        .unwrap();
    assert!(done(again.outcome).up_to_date);
}

#[tokio::test]
async fn a_failed_update_download_plays_the_installed_version() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;
    publish_v1_1(&env).await;
    // The new mod's host refuses it: a network failure while staging, before anything moves.
    Mock::given(method("GET"))
        .and(path("/files/jei.jar"))
        .respond_with(ResponseTemplate::new(404))
        .with_priority(1)
        .mount(&env.server)
        .await;
    let update = update_for(&env, &instance).await;
    let lock = InstanceLock::try_acquire(&env.store.perland_dir(&instance.id)).unwrap();
    let ensured = env
        .syncer
        .ensure_locked(&lock, &env.store, &instance.id, Some(update), &env.sink(), &Cancel::default())
        .await
        .unwrap();
    let warning = ensured.warning.unwrap();
    assert_eq!(warning.code, ErrorCode::UpdateDownloadFailed);
    assert_eq!(warning.params["version"], "1.0.0");
    assert!(done(ensured.outcome).up_to_date);
    assert!(!env.game_file(&instance.id, "mods/jei.jar").exists());
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());
    assert_eq!(
        env.syncer.installed(&env.store, &instance.id).await.unwrap().pack_version,
        "1.0.0"
    );
    assert_eq!(env.store.load(&instance.id).await.unwrap().source, instance.source);
}

#[tokio::test]
async fn a_cancelled_sync_stops_before_touching_the_game_folder() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;
    publish_v1_1(&env).await;
    let update = update_for(&env, &instance).await;
    let lock = InstanceLock::try_acquire(&env.store.perland_dir(&instance.id)).unwrap();
    let cancel = Cancel::default();
    cancel.cancel();
    let err = env
        .syncer
        .ensure_locked(&lock, &env.store, &instance.id, Some(update), &env.sink(), &cancel)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::SyncCancelled);
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());
    assert!(!env.game_file(&instance.id, "mods/jei.jar").exists());
    assert_eq!(
        env.syncer.installed(&env.store, &instance.id).await.unwrap().pack_version,
        "1.0.0"
    );
}

#[tokio::test]
async fn the_first_sync_says_installing_and_later_ones_say_updating() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;
    let keys = |env: &Env| -> Vec<String> {
        env.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::Progress { task, .. } => Some(task.key.clone()),
                _ => None,
            })
            .collect()
    };
    assert!(keys(&env).iter().all(|k| k == "install_pack"));
    env.events.lock().unwrap().clear();
    publish_v1_1(&env).await;
    ensure(&env, &instance.id).await.unwrap();
    assert!(!keys(&env).is_empty());
    assert!(keys(&env).iter().all(|k| k == "sync_pack"));
}
```

- [ ] **Step 2: Write the failing unit test for kept edits**

At the end of `crates/launcher-core/src/sync/mod.rs` add a test module. It proves `kept_edits` hashes only `ReplaceUnlessModified` files: a folder at a mod's path would make a survey of that mod fail, so a non-empty answer shows the mod was never surveyed.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::hash::sha1_hex,
        sources::{FileMode, FileSource, Side},
    };

    fn file(path: &str, mode: FileMode, body: &[u8]) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.into(),
            source: FileSource::Remote { urls: vec![] },
            hashes: Hashes::sha1(sha1_hex(body)),
            size: None,
            side: Side::Both,
            optional: None,
            mode,
            manual: None,
        }
    }

    #[tokio::test]
    async fn kept_edits_hashes_only_override_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::write(game.join("config/c.toml"), b"mine").unwrap();
        std::fs::create_dir_all(game.join("mods/a.jar")).unwrap(); // a folder: surveying it fails
        let old = file("config/c.toml", FileMode::ReplaceUnlessModified, b"old");
        let managed = Managed {
            version: Managed::VERSION,
            stamp: "s1".into(),
            pack_version: "1.0.0".into(),
            choices: BTreeMap::new(),
            files: BTreeMap::from([(
                "config/c.toml".to_string(),
                lockfile::ManagedFile {
                    name: "c".into(),
                    sha1: sha1_hex(b"old"),
                    size: 3,
                    mode: FileMode::ReplaceUnlessModified,
                    declared: old.hashes.clone(),
                },
            )]),
        };
        let new = file("config/c.toml", FileMode::ReplaceUnlessModified, b"new");
        let jar = file("mods/a.jar", FileMode::Replace, b"jar");
        let kept = kept_edits(game, &[&new, &jar], Some(&managed)).await;
        assert_eq!(kept, vec!["config/c.toml".to_string()]);
    }
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p launcher-core --test sync && cargo test -p launcher-core --lib sync::tests`
Expected: compile errors (`find_update`, `ensure_locked`, `Cancel`, `kept_edits` don't exist).

- [ ] **Step 4: Implement**

In `sync/mod.rs`:

1. Imports: add `error::ErrorCategory` to the `crate::error` import, and `sources::FileMode` to the `sources` import.

2. Below `update_check_failed`, add:

```rust
/// Stops a sync before its next file and before the swap. Once the swap starts the sync
/// finishes, so a cancel never leaves an instance half-updated.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

fn cancelled() -> LauncherError {
    LauncherError::pack(ErrorCode::SyncCancelled, "The update was cancelled")
}

/// Downloading the update failed before anything moved: the installed version plays instead.
fn update_download_failed(installed: &str, e: LauncherError) -> LauncherError {
    LauncherError::network(
        ErrorCode::UpdateDownloadFailed,
        format!("Couldn't download the update, so you're playing {installed}"),
    )
    .with_param("version", installed)
    .with_fix("Try updating again later")
    .with_detail(e.message)
}

/// An update the player can take: where it comes from, the pack it is, and what it changes.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingUpdate {
    /// What the instance's source becomes once the update is installed.
    pub source: SourceRef,
    pub desired: DesiredState,
    /// What the update adds, updates and removes; edits the player keeps are not "updated".
    pub changes: Changes,
    /// Override files the player edited that the update keeps as they are.
    pub kept: Vec<String>,
}

/// Override files the player edited that a sync to `wanted` would keep. Only
/// `ReplaceUnlessModified` files are hashed, never the mods, so a preview stays quick.
async fn kept_edits(game_dir: &Path, wanted: &[&PackFile], managed: Option<&Managed>) -> Vec<String> {
    let overrides: Vec<&PackFile> = wanted
        .iter()
        .copied()
        .filter(|f| f.mode == FileMode::ReplaceUnlessModified)
        .collect();
    match apply::survey(game_dir, &overrides, None).await {
        Ok(disk) => plan::plan(&overrides, managed, &disk).kept_modified,
        Err(_) => Vec::new(),
    }
}
```

3. Add `find_update` to `impl Syncer`, after `check`:

```rust
    /// The update the player could take now, with what it would change, or `None` (pinned,
    /// never synced, up to date). `Err` is the failed check: a warning, after which the
    /// installed version plays.
    pub async fn find_update(
        &self,
        store: &InstanceStore,
        instance: &Instance,
    ) -> Result<Option<PendingUpdate>> {
        if instance.update_policy == UpdatePolicy::Pinned {
            return Ok(None);
        }
        let Some(installed) = self.installed(store, &instance.id).await else {
            return Ok(None);
        };
        let source = match self.sources.check(&instance.source, &installed).await {
            UpdateCheck::UpToDate => return Ok(None),
            UpdateCheck::Unavailable(e) => return Err(e),
            UpdateCheck::Available { source, .. } => source,
        };
        let desired = self
            .sources
            .resolve(&source)
            .await
            .map_err(update_check_failed)?;
        let managed = Managed::load(&store.perland_dir(&instance.id)).await;
        let wanted = plan::wanted(&desired, &instance.optional_choices);
        let kept = kept_edits(&store.game_dir(&instance.id), &wanted, managed.as_ref()).await;
        let changes =
            plan::changes_except(&wanted, managed.as_ref(), &desired.pack_version, &kept);
        Ok(Some(PendingUpdate {
            source,
            desired,
            changes,
            kept,
        }))
    }
```

4. Replace `ensure` with `ensure` + `ensure_locked` + `after_sync`:

```rust
    /// Bring a pack instance up to date before playing, following its update policy (core spec
    /// §3.1). Pinned stays on the installed version. Auto and ask take updates (the app asks
    /// first, with `find_update`). A failed check plays what's installed, with a warning.
    pub async fn ensure(
        &self,
        store: &InstanceStore,
        id: &str,
        sink: &EventSink,
    ) -> Result<Ensured> {
        // Loading validates `id` before the lock creates anything under it.
        let instance = store.load(id).await?;
        let lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        let (update, check_warning) = match self.find_update(store, &instance).await {
            Ok(update) => (update, None),
            Err(e) => (None, Some(e)),
        };
        let ensured = self
            .ensure_locked(&lock, store, id, update, sink, &Cancel::default())
            .await?;
        Ok(Ensured {
            warning: check_warning.or(ensured.warning),
            ..ensured
        })
    }

    /// `ensure` for a caller that holds the instance's lock and already chose: `accept` is the
    /// update to install, `None` keeps the installed version. A never-synced instance installs
    /// what its source names now. If downloading an accepted update fails, nothing has moved
    /// yet, so the installed version is synced instead, with an `update_download_failed` warning.
    pub async fn ensure_locked(
        &self,
        _lock: &InstanceLock,
        store: &InstanceStore,
        id: &str,
        accept: Option<PendingUpdate>,
        sink: &EventSink,
        cancel: &Cancel,
    ) -> Result<Ensured> {
        let instance = store.load(id).await?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        let Some(installed) = self.installed(store, id).await else {
            let desired = self.sources.resolve(&instance.source).await?;
            let outcome = self.sync_inner(store, id, &desired, sink, cancel).await?;
            self.after_sync(store, id, &desired, &outcome, None).await?;
            return Ok(Ensured {
                outcome,
                warning: None,
            });
        };
        let Some(update) = accept else {
            let outcome = self.sync_inner(store, id, &installed, sink, cancel).await?;
            return Ok(Ensured {
                outcome,
                warning: None,
            });
        };
        match self.sync_inner(store, id, &update.desired, sink, cancel).await {
            Ok(outcome) => {
                let new_source = (update.source != instance.source).then_some(update.source);
                self.after_sync(store, id, &update.desired, &outcome, new_source)
                    .await?;
                Ok(Ensured {
                    outcome,
                    warning: None,
                })
            }
            // Network failures happen only while staging or installing the loader, before
            // anything in the game folder moves.
            Err(e) if e.category == ErrorCategory::Network => {
                let warning = update_download_failed(&installed.pack_version, e);
                let outcome = self.sync_inner(store, id, &installed, sink, cancel).await?;
                Ok(Ensured {
                    outcome,
                    warning: Some(warning),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// After a newly installed version: name it in the instance's source, and fetch its art.
    async fn after_sync(
        &self,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        outcome: &SyncOutcome,
        new_source: Option<SourceRef>,
    ) -> Result<()> {
        let SyncOutcome::Done(report) = outcome else {
            return Ok(());
        };
        if let Some(source) = new_source {
            // Only now: a source that names a version whose sync never finished would hide
            // the update from the next check.
            let mut fresh = store.load(id).await?;
            fresh.source = source;
            store.save(&fresh).await?;
        }
        if !report.up_to_date {
            art::fetch(
                &self.sources.http,
                &desired.about.art,
                &store.perland_dir(id).join("art"),
            )
            .await;
        }
        Ok(())
    }
```

5. Rename the body of `sync_locked` to a private `sync_inner`, with the extra `cancel: &Cancel` parameter and no lock parameter. `sync_locked` becomes:

```rust
    /// `sync` for a caller that already holds the instance's lock.
    pub async fn sync_locked(
        &self,
        _lock: &InstanceLock,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        sink: &EventSink,
    ) -> Result<SyncOutcome> {
        self.sync_inner(store, id, desired, sink, &Cancel::default())
            .await
    }
```

   Inside `sync_inner`:
   - Pass `if managed.is_none() { "install_pack" } else { "sync_pack" }` and `cancel` to `stage_all`.
   - Directly before `save_pack(&perland, desired).await?;` (the point of no return) add:

```rust
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
```

6. Change `stage_all`'s signature to take `task_key: &str` and `cancel: &Cancel`:
   - Build the task with `TaskId::new(task_key).with("name", pack_name)`.
   - In the per-file closure, start with `if failed.load(Ordering::SeqCst) || cancel.is_cancelled() { return None; }`.
   - After the `first_error` check, add `if cancel.is_cancelled() { return Err(cancelled()); }`.

7. `import_manual` stays. Task 5 adds `import_found` next to it.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p launcher-core`
Expected: PASS: the new tests plus every existing sync, ensure and CLI-facing test. `an_update_that_fails_to_resolve_plays_the_installed_version` still sees `update_check_failed`, because `find_update` maps resolve failures through `update_check_failed`.

- [ ] **Step 6: Check the CLI still builds**

Run: `cargo build -p perland-cli`
Expected: OK. The CLI calls `ensure`, `sync`, `check` and `import_manual`, whose signatures are unchanged.

- [ ] **Step 7: Commit**

```bash
git add crates/launcher-core/src/sync/mod.rs crates/launcher-core/tests/sync.rs
git commit -m "feat(sync): find_update and ensure_locked for callers that hold the lock; cancel; fall back to the installed version

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: A Downloads scanner that remembers what it hashed

The "Download these manually" dialog polls the Downloads folder every 2 s, and a one-shot `scan` hashes every `.jar`/`.zip` on every poll. `Scanner` remembers each file's digests by (path, size, modified time).

**Files:**
- Modify: `crates/launcher-core/src/sync/manual.rs`
- Modify: `crates/launcher-core/src/sync/mod.rs` (`import_found`)

**Interfaces:**
- Produces `pub struct Scanner` (`Default`, `Debug`) with `pub async fn scan(&mut self, dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>>`.
- `pub async fn scan(dir, wanted)` stays, as `Scanner::default().scan(...)`.
- Produces `Syncer::import_found(&self, found: &[(ManualFile, PathBuf)]) -> Result<()>`. `import_manual` uses it.

- [ ] **Step 1: Write the failing test**

In `sync/manual.rs` `mod tests` (the existing `wanted(file_name, hashes)` helper builds a `ManualFile`):

```rust
    #[tokio::test]
    async fn a_scanner_hashes_each_file_once_until_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other.jar"), b"other").unwrap();
        let by_sha1 = wanted("mod.jar", Hashes::sha1(sha1_hex(b"mod")));
        let by_murmur = wanted(
            "cf.jar",
            Hashes {
                murmur2: Some(curseforge_murmur2(b"cf")),
                ..Hashes::default()
            },
        );
        let want = [by_sha1.clone(), by_murmur.clone()];
        let mut scanner = Scanner::default();
        assert!(scanner.scan(dir.path(), &want).await.unwrap().is_empty());
        let first = scanner.hashed;
        assert_eq!(first, 2, "other.jar: one sha1, one murmur2 check");
        assert!(scanner.scan(dir.path(), &want).await.unwrap().is_empty());
        assert_eq!(scanner.hashed, first, "nothing new: nothing hashed");

        std::fs::write(dir.path().join("mod (1).jar"), b"mod").unwrap();
        std::fs::write(dir.path().join("cf.jar"), b"cf").unwrap();
        let found = scanner.scan(dir.path(), &want).await.unwrap();
        let mut names: Vec<_> = found.iter().map(|(f, _)| f.file_name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["cf.jar".to_string(), "mod.jar".to_string()]);

        // A changed file is hashed again; a vanished one is forgotten.
        std::fs::remove_file(dir.path().join("mod (1).jar")).unwrap();
        std::fs::remove_file(dir.path().join("cf.jar")).unwrap();
        let before = scanner.hashed;
        std::fs::write(dir.path().join("other.jar"), b"other, but longer now").unwrap();
        assert!(scanner.scan(dir.path(), &want).await.unwrap().is_empty());
        assert_eq!(scanner.hashed, before + 2);
        assert_eq!(scanner.seen.len(), 1);
    }
```

(`curseforge_murmur2` and `sha1_hex` are already imported by that test module.)

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p launcher-core --lib sync::manual`
Expected: compile error (`Scanner` not found).

- [ ] **Step 3: Implement**

Replace `scan` and `scan_blocking` in `sync/manual.rs`:

```rust
use std::{collections::HashSet, time::SystemTime};

/// Looks for the wanted files in a folder, again and again. It remembers each file's digests
/// by size and modified time, so a poll hashes only files that are new or changed.
#[derive(Debug, Default)]
pub struct Scanner {
    seen: HashMap<PathBuf, Seen>,
    /// How many hash computations ran (for tests).
    hashed: usize,
}

#[derive(Debug)]
struct Seen {
    len: u64,
    modified: Option<SystemTime>,
    /// Computed on first use; `Some("")` if the file couldn't be read.
    sha1: Option<String>,
    /// For wanted files without a sha1: did this file match, by the wanted file's path.
    verdicts: HashMap<String, bool>,
}

/// One scan with a fresh `Scanner`.
pub async fn scan(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
    Scanner::default().scan(dir, wanted).await
}

impl Scanner {
    /// Files directly in `dir` that are one of `wanted`, matched by hash. A file with the
    /// expected name is tried first, then any other `.jar` or `.zip`, because browsers rename
    /// duplicates (`mod (1).jar`).
    pub async fn scan(
        &mut self,
        dir: &Path,
        wanted: &[ManualFile],
    ) -> Result<Vec<(ManualFile, PathBuf)>> {
        let (dir, wanted) = (dir.to_path_buf(), wanted.to_vec());
        let mut state = std::mem::take(self);
        let (state, found) = tokio::task::spawn_blocking(move || {
            let found = state.scan_blocking(&dir, &wanted);
            (state, found)
        })
        .await
        .map_err(std::io::Error::other)?;
        *self = state;
        found
    }

    fn scan_blocking(&mut self, dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut candidates: Vec<(String, PathBuf)> = Vec::new();
        let mut present = HashSet::new();
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() || meta.len() > MAX_CANDIDATE {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
                continue;
            }
            let path = entry.path();
            let modified = meta.modified().ok();
            let unchanged = self
                .seen
                .get(&path)
                .is_some_and(|s| s.len == meta.len() && s.modified == modified);
            if !unchanged {
                self.seen.insert(
                    path.clone(),
                    Seen {
                        len: meta.len(),
                        modified,
                        sha1: None,
                        verdicts: HashMap::new(),
                    },
                );
            }
            present.insert(path.clone());
            candidates.push((name, path));
        }
        self.seen.retain(|path, _| present.contains(path));
        candidates.sort();
        let mut found = Vec::new();
        for file in wanted {
            if file.hashes.is_empty() {
                continue; // nothing to prove a match with
            }
            let mut ordered: Vec<&(String, PathBuf)> = candidates.iter().collect();
            ordered.sort_by_key(|(name, _)| *name != file.file_name);
            for (_, path) in ordered {
                if self.matches(path, file) {
                    found.push((file.clone(), path.clone()));
                    break;
                }
            }
        }
        Ok(found)
    }

    fn matches(&mut self, path: &Path, file: &ManualFile) -> bool {
        let Some(seen) = self.seen.get_mut(path) else {
            return false;
        };
        match &file.hashes.sha1 {
            Some(expected) => {
                if seen.sha1.is_none() {
                    seen.sha1 = Some(sha1_file(path).unwrap_or_default());
                    self.hashed += 1;
                }
                seen.sha1
                    .as_deref()
                    .is_some_and(|s| !s.is_empty() && s.eq_ignore_ascii_case(expected))
            }
            None => *seen.verdicts.entry(file.path.clone()).or_insert_with(|| {
                self.hashed += 1;
                file.hashes.verify_file(path).unwrap_or(false)
            }),
        }
    }
}
```

In `sync/mod.rs`, replace `import_manual` with:

```rust
    /// Copy files a scan found into the object cache (the player's copies stay where they
    /// are); the next sync uses them.
    pub async fn import_found(&self, found: &[(ManualFile, PathBuf)]) -> Result<()> {
        for (file, path) in found {
            self.sources.cache.insert_file(path, &file.hashes).await?;
        }
        Ok(())
    }

    /// Scan `dir` once and import every blocked file found there. Returns how many were found.
    pub async fn import_manual(&self, dir: &Path, files: &[ManualFile]) -> Result<usize> {
        let found = manual::scan(dir, files).await?;
        self.import_found(&found).await?;
        Ok(found.len())
    }
```

If the borrow checker rejects `self.hashed += 1` inside the closure while `seen` borrows `self.seen`, hoist the counter: bind `let hashed = &mut self.hashed;` before `self.seen.get_mut(path)` and use `*hashed += 1`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p launcher-core`
Expected: PASS, including the existing `scan` tests and `blocked_curseforge_files_wait_for_a_manual_download`.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-core/src/sync/manual.rs crates/launcher-core/src/sync/mod.rs
git commit -m "feat(sync): a Downloads scanner that hashes each file once until it changes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 6: DTOs, run phases and events for packs, plus the contract

Everything the UI will receive is defined first, so the contract fixtures and the zod schemas (Task 13) are written against one shape.

**Files:**
- Modify: `crates/launcher-app/src/dto.rs`, `crates/launcher-app/src/events.rs`
- Modify: `crates/launcher-app/src/runs.rs`: `emit_state` and every event literal get `update: None`.
- Modify: `crates/launcher-app/src/testing.rs`: patterns, only if they stop compiling.
- Modify: `crates/launcher-app/src/app.rs`: the `folder()` match gets a `Downloads` arm.
- Modify: `crates/launcher-app/tests/contract.rs`
- Regenerate: `ui/src/ipc/contract/*.json`
- Modify: `ui/src/ipc/schema.ts`, `ui/src/ipc/contract.test.ts`. This is Task 13's schema step, pulled forward so `pnpm -r test` stays green in this commit.

**Interfaces:**
- Produces in `launcher_app::dto`, all `Serialize` and camelCase:
  - `PackUpdate { from_version: Option<String>, to_version: String, added: Vec<String>, updated: Vec<String>, removed: Vec<String>, kept: Vec<String> }`, with `impl From<&PendingUpdate>`.
  - `SyncSummary { from_version: Option<String>, to_version: String, added: usize, updated: usize, removed: usize, kept_modified: Vec<String> }`, with `impl From<&SyncReport>`.
  - `PackStatus { installed_version: Option<String>, update: Option<PackUpdate>, check_error: Option<LauncherError> }`
  - `OptionalChoice { enabled: bool, description: Option<String> }`
  - `PackMod { path: String, name: String, optional: Option<OptionalChoice> }`
  - `PackContents { managed: Vec<PackMod>, additions: Vec<LocalMod> }`
  - `ManualProgress { found: usize, total: usize, remaining: Vec<ManualFile> }`
  - `PackArtData { icon: Option<String>, banner: Option<String> }`
- Also in `launcher_app::dto`:
  - `PackAddRequest` (`Serialize + Deserialize`, tagged `kind`, snake_case): `Link { url: String }` | `File { path: PathBuf }`.
  - `RunSnapshot` gains `update: Option<PackUpdate>`.
  - `OpenTarget` gains `Downloads`, serialized `{"kind":"downloads"}`.
- Produces in `launcher_app::events`:
  - `RunStateKind` gains `Checking`, `Asking` and `Syncing`.
  - `ExitInfo` becomes camelCase and gains `synced: Option<SyncSummary>`, `manual: Vec<ManualFile>` and `update_only: bool`. `is_crash()` is false when `update_only`.
  - `AppEvent::RunState` gains `update: Option<PackUpdate>`.
  - New unit variant `AppEvent::DeepLink`, serialized `{"type":"deep_link"}`. It means "a pack link is waiting; call `deep_link_take`".

- [ ] **Step 1: Write the failing tests**

In `dto.rs` `mod tests`:

```rust
    use launcher_core::{
        instances::SourceRef,
        sources::{DesiredState, PackAbout},
        sync::{plan::Changes, PendingUpdate, SyncReport},
    };

    fn changes() -> Changes {
        Changes {
            from_version: Some("1.0.3".into()),
            to_version: "1.0.4".into(),
            added: vec!["JEI".into(), "Create".into()],
            updated: vec!["Sodium".into()],
            removed: vec!["Zoomify".into()],
        }
    }

    #[test]
    fn an_update_and_a_summary_keep_names_and_counts() {
        let pending = PendingUpdate {
            source: SourceRef::Packwiz {
                url: "https://example.com/pack.toml".into(),
            },
            desired: DesiredState {
                name: "Sky".into(),
                pack_version: "1.0.4".into(),
                minecraft: "1.20.1".into(),
                loader: None,
                files: vec![],
                stamp: "s2".into(),
                about: PackAbout::default(),
                archive: None,
            },
            changes: changes(),
            kept: vec!["config/a.toml".into()],
        };
        let update = PackUpdate::from(&pending);
        assert_eq!(update.added, vec!["JEI".to_string(), "Create".to_string()]);
        assert_eq!(update.kept, vec!["config/a.toml".to_string()]);
        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["fromVersion"], "1.0.3");
        assert_eq!(json["toVersion"], "1.0.4");

        let summary = SyncSummary::from(&SyncReport {
            up_to_date: false,
            changes: changes(),
            kept_modified: vec!["options.txt".into()],
            game_changed: false,
        });
        assert_eq!((summary.added, summary.updated, summary.removed), (2, 1, 1));
        assert_eq!(
            serde_json::to_value(&summary).unwrap()["keptModified"][0],
            "options.txt"
        );
    }

    #[test]
    fn pack_add_requests_are_tagged_by_kind() {
        let link: PackAddRequest =
            serde_json::from_str(r#"{"kind":"link","url":"https://x.example/pack.toml"}"#)
                .unwrap();
        assert_eq!(
            link,
            PackAddRequest::Link {
                url: "https://x.example/pack.toml".into()
            }
        );
        let file: PackAddRequest =
            serde_json::from_str(r#"{"kind":"file","path":"C:/Downloads/Sky.mrpack"}"#).unwrap();
        assert_eq!(
            file,
            PackAddRequest::File {
                path: "C:/Downloads/Sky.mrpack".into()
            }
        );
        let downloads: OpenTarget = serde_json::from_str(r#"{"kind":"downloads"}"#).unwrap();
        assert_eq!(downloads, OpenTarget::Downloads);
    }
```

In `events.rs` `mod tests`:

```rust
    #[test]
    fn an_update_only_run_is_never_a_crash() {
        assert!(!ExitInfo {
            update_only: true,
            ..ExitInfo::default()
        }
        .is_crash());
        let json = serde_json::to_value(ExitInfo::default()).unwrap();
        assert_eq!(json["updateOnly"], false);
        assert!(json["manual"].as_array().unwrap().is_empty());
        assert!(json["synced"].is_null());
    }

    #[test]
    fn new_phases_and_the_deep_link_event_serialize() {
        let e = AppEvent::RunState {
            instance: "sky".into(),
            state: RunStateKind::Asking,
            exit: None,
            update: None,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["state"], "asking");
        assert!(json["update"].is_null());
        assert_eq!(
            serde_json::to_value(AppEvent::DeepLink).unwrap(),
            serde_json::json!({ "type": "deep_link" })
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib`
Expected: compile errors (the types and fields don't exist).

- [ ] **Step 3: Implement**

In `dto.rs`, add the types below the existing ones. Imports: `std::path::PathBuf`, `launcher_core::{mods::LocalMod, sync::{ManualFile, PendingUpdate, SyncReport}, LauncherError}`.

```rust
/// An update the player can take, as the "Update ready" dialog and banner show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdate {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    /// Files the player edited that the update keeps as they are ("Kept your version of N files").
    pub kept: Vec<String>,
}

impl From<&PendingUpdate> for PackUpdate {
    fn from(u: &PendingUpdate) -> Self {
        Self {
            from_version: u.changes.from_version.clone(),
            to_version: u.changes.to_version.clone(),
            added: u.changes.added.clone(),
            updated: u.changes.updated.clone(),
            removed: u.changes.removed.clone(),
            kept: u.kept.clone(),
        }
    }
}

/// What a finished sync changed: "Updated to 1.0.4", "Kept your version of N files".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub kept_modified: Vec<String>,
}

impl From<&SyncReport> for SyncSummary {
    fn from(r: &SyncReport) -> Self {
        Self {
            from_version: r.changes.from_version.clone(),
            to_version: r.changes.to_version.clone(),
            added: r.changes.added.len(),
            updated: r.changes.updated.len(),
            removed: r.changes.removed.len(),
            kept_modified: r.kept_modified.clone(),
        }
    }
}

/// For the banner: the installed version, and the update (or why there's no answer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    pub installed_version: Option<String>,
    pub update: Option<PackUpdate>,
    pub check_error: Option<LauncherError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalChoice {
    pub enabled: bool,
    pub description: Option<String>,
}

/// A mod that comes with the pack. Read-only, except an optional one's toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMod {
    pub path: String,
    pub name: String,
    pub optional: Option<OptionalChoice>,
}

/// A pack instance's Mods screen: the pack's mods, and the player's own jars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackContents {
    pub managed: Vec<PackMod>,
    pub additions: Vec<LocalMod>,
}

/// "Download these manually": how many of the blocked files were found so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualProgress {
    pub found: usize,
    pub total: usize,
    pub remaining: Vec<ManualFile>,
}

/// Pack art as `data:` URLs (the CSP allows no remote images). `None` falls back to the
/// generated art.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackArtData {
    pub icon: Option<String>,
    pub banner: Option<String>,
}

/// What the New-instance screen hands over. A link is never a file path, and a file is only
/// ever a `.mrpack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackAddRequest {
    Link { url: String },
    File { path: PathBuf },
}
```

Also in `dto.rs`:
- Add `Downloads` to `OpenTarget`, after `Logs`.
- Add `pub update: Option<PackUpdate>` to `RunSnapshot`.

In `app.rs` `folder()`, add an arm that Task 11 makes real:

```rust
            OpenTarget::Downloads => {
                return Err(LauncherError::disk(
                    ErrorCode::Internal,
                    "Couldn't find your Downloads folder",
                ))
            }
```

In `events.rs`:
- `RunStateKind` becomes `Idle, Checking, Asking, Queued, Syncing, Preparing, Running, Stopping`, with a doc comment per new variant:
  - `Checking`: looking for a pack update.
  - `Asking`: waiting for the player's answer to "Update ready".
  - `Syncing`: bringing the pack's files in line.
- `ExitInfo`: add `#[serde(rename_all = "camelCase")]` and these fields:

  ```rust
      /// What the run's sync changed (None when nothing changed or nothing synced).
      pub synced: Option<SyncSummary>,
      /// Files the sync needs the player to download by hand (`manual_downloads_needed`).
      pub manual: Vec<ManualFile>,
      /// The banner's Update: the run synced and stopped there, with no game.
      pub update_only: bool,
  ```

  In `is_crash`, add `&& !self.update_only`.
- `AppEvent::RunState` gets `update: Option<PackUpdate>` ("the update on offer while `asking`").
- Add the variant:

  ```rust
      /// A `perland://pack` link arrived; the UI asks for it with `deep_link_take`.
      DeepLink,
  ```

In `runs.rs`:
- `emit_state` builds `AppEvent::RunState { instance, state, exit, update: None }`.
- `run_states` fills `update: None` for now; Task 8 fills it.

Add `update: None` to any other `AppEvent::RunState { … }` literal the compiler points at, including the `events.rs` tests.

- [ ] **Step 4: Extend the contract fixtures**

In `crates/launcher-app/tests/contract.rs`, import the new DTOs and `launcher_core::{download::Hashes, sync::ManualFile}`, then add:

```rust
fn sample_update() -> PackUpdate {
    PackUpdate {
        from_version: Some("1.0.3".into()),
        to_version: "1.0.4".into(),
        added: vec![
            "Just Enough Items".into(),
            "Create".into(),
            "Farmer's Delight".into(),
        ],
        updated: vec!["Sodium".into(), "Iris".into()],
        removed: vec!["Zoomify".into()],
        kept: vec!["config/sodium-options.json".into()],
    }
}

fn sample_manual() -> ManualFile {
    ManualFile {
        path: "mods/optifine.jar".into(),
        name: "OptiFine".into(),
        file_name: "OptiFine_1.20.1_HD_U_I6.jar".into(),
        page_url: "https://www.curseforge.com/projects/235216".into(),
        hashes: Hashes::sha1("3f786850e387550fdab836ed7e6dc881de23001b"),
    }
}
```

In the `"events"` vector:
- Add `update: None` to every existing `RunState`.
- Append:

```rust
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Checking,
                exit: None,
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Asking,
                exit: None,
                update: Some(sample_update()),
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Syncing,
                exit: None,
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    code: Some(0),
                    synced: Some(SyncSummary {
                        from_version: Some("1.0.3".into()),
                        to_version: "1.0.4".into(),
                        added: 3,
                        updated: 2,
                        removed: 1,
                        kept_modified: vec!["options.txt".into()],
                    }),
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    error: Some(
                        LauncherError::pack(
                            ErrorCode::ManualDownloadsNeeded,
                            "1 files have to be downloaded by hand",
                        )
                        .with_param("count", 1)
                        .with_fix("Download them from CurseForge, then try again"),
                    ),
                    manual: vec![sample_manual()],
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    update_only: true,
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::DeepLink,
```

Elsewhere in `contract.rs`:
- `"run_snapshots"` becomes two snapshots: the existing `Running` one with `update: None`, plus `RunSnapshot { instance: "sky-islands".into(), state: RunStateKind::Asking, update: Some(sample_update()) }`.
- Add `OpenTarget::Downloads` to `"open_targets"`.
- Add these checks:

```rust
    check(
        "pack_status",
        vec![
            PackStatus {
                installed_version: Some("1.0.3".into()),
                update: Some(sample_update()),
                check_error: None,
            },
            PackStatus {
                installed_version: Some("1.0.3".into()),
                update: None,
                check_error: Some(
                    LauncherError::network(
                        ErrorCode::UpdateCheckFailed,
                        "Couldn't check for updates",
                    )
                    .with_fix("You can still play the version you have"),
                ),
            },
        ],
    );
    check(
        "pack_contents",
        PackContents {
            managed: vec![
                PackMod {
                    path: "mods/create.jar".into(),
                    name: "Create".into(),
                    optional: None,
                },
                PackMod {
                    path: "mods/iris.jar".into(),
                    name: "Iris Shaders".into(),
                    optional: Some(OptionalChoice {
                        enabled: false,
                        description: Some("Shaders, for strong computers".into()),
                    }),
                },
            ],
            additions: vec![LocalMod {
                file_name: "zoomify-2.11.2.jar".into(),
                enabled: true,
                size: 524_288,
            }],
        },
    );
    check(
        "manual_progress",
        ManualProgress {
            found: 1,
            total: 2,
            remaining: vec![sample_manual()],
        },
    );
    check(
        "pack_art",
        PackArtData {
            icon: Some("data:image/png;base64,iVBORw0KGgo=".into()),
            banner: None,
        },
    );
    check(
        "pack_add_requests",
        vec![
            PackAddRequest::Link {
                url: "https://perlanddev.github.io/asalith-modpack/pack.toml".into(),
            },
            PackAddRequest::File {
                path: PathBuf::from("C:/Users/player/Downloads/Sky Islands 1.0.4.mrpack"),
            },
        ],
    );
```

- [ ] **Step 5: Regenerate the fixtures and update the zod schemas**

Run: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

Then update `ui/src/ipc/schema.ts` (below; Task 13 builds on it), and add the five new fixtures to `contract.test.ts`'s `cases`:

```ts
import manualProgress from './contract/manual_progress.json';
import packAddRequests from './contract/pack_add_requests.json';
import packArt from './contract/pack_art.json';
import packContents from './contract/pack_contents.json';
import packStatus from './contract/pack_status.json';
// …
  ['manual_progress', s.manualProgress, manualProgress],
  ['pack_add_requests', s.packAddRequest.array(), packAddRequests],
  ['pack_art', s.packArtData, packArt],
  ['pack_contents', s.packContents, packContents],
  ['pack_status', s.packStatus.array(), packStatus],
```

The schema changes in `ui/src/ipc/schema.ts`:

```ts
export const runStateKind = z.enum(['idle', 'checking', 'asking', 'queued', 'syncing', 'preparing', 'running', 'stopping']);
export const hashes = z.strictObject({
  sha1: z.string().nullable(),
  sha256: z.string().nullable(),
  sha512: z.string().nullable(),
  md5: z.string().nullable(),
  murmur2: z.number().int().nullable(),
});
export const manualFile = z.strictObject({
  path: z.string(),
  name: z.string(),
  fileName: z.string(),
  pageUrl: z.string(),
  hashes,
});
export const packUpdate = z.strictObject({
  fromVersion: z.string().nullable(),
  toVersion: z.string(),
  added: z.array(z.string()),
  updated: z.array(z.string()),
  removed: z.array(z.string()),
  kept: z.array(z.string()),
});
export const syncSummary = z.strictObject({
  fromVersion: z.string().nullable(),
  toVersion: z.string(),
  added: z.number().int(),
  updated: z.number().int(),
  removed: z.number().int(),
  keptModified: z.array(z.string()),
});
export const exitInfo = z.strictObject({
  code: z.number().int().nullable(),
  killed: z.boolean(),
  cancelled: z.boolean(),
  crash: crashHint.nullable(),
  error: launcherError.nullable(),
  synced: syncSummary.nullable(),
  manual: z.array(manualFile),
  updateOnly: z.boolean(),
});
// appEvent: the run_state variant gains `update: packUpdate.nullable()`, and a new variant:
//   z.strictObject({ type: z.literal('deep_link') }),
export const runSnapshot = z.strictObject({ instance: z.string(), state: runStateKind, update: packUpdate.nullable() });
export const packStatus = z.strictObject({
  installedVersion: z.string().nullable(),
  update: packUpdate.nullable(),
  checkError: launcherError.nullable(),
});
export const optionalChoice = z.strictObject({ enabled: z.boolean(), description: z.string().nullable() });
export const packMod = z.strictObject({ path: z.string(), name: z.string(), optional: optionalChoice.nullable() });
export const packContents = z.strictObject({ managed: z.array(packMod), additions: z.array(localMod) });
export const manualProgress = z.strictObject({ found: z.number().int(), total: z.number().int(), remaining: z.array(manualFile) });
export const packArtData = z.strictObject({ icon: z.string().nullable(), banner: z.string().nullable() });
export const packAddRequest = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('link'), url: z.string() }),
  z.strictObject({ kind: z.literal('file'), path: z.string() }),
]);
// openTarget gains: z.strictObject({ kind: z.literal('downloads') }),
```

Export a type for each new schema: `Hashes`, `ManualFile`, `PackUpdate`, `SyncSummary`, `PackStatus`, `OptionalChoice`, `PackMod`, `PackContents`, `ManualProgress`, `PackArtData`, `PackAddRequest`. Define `runStateKind` and `exitInfo` in place of the old ones. `exitInfo` must come after `syncSummary` and `manualFile`.

The UI code that builds `run_state` events or `ExitInfo` objects now fails to typecheck: `ui/src/ipc/mock.ts`, and the tests that call `runStore.apply({ type: 'run_state', … })`. Fix them mechanically in this step:
- Add `update: null` to every `run_state` literal and every `RunSnapshot` literal (for example `runStore.test.ts` `hydrate` calls).
- Build every `exit` object in the mock through one helper, `exitOf(partial)`, which fills `synced: null, manual: [], updateOnly: false`:

```ts
export const exitOf = (partial: Partial<ExitInfo>): ExitInfo => ({
  code: null, killed: false, cancelled: false, crash: null, error: null,
  synced: null, manual: [], updateOnly: false, ...partial,
});
```

Put `exitOf` in `ui/src/ipc/mock.ts` and import it in tests that build exits. The `PlayButton`/`Rail` `switch` statements over `RunStateKind` still compile; Task 14 gives the new states their labels.

- [ ] **Step 6: Run everything**

Run: `cargo test -p launcher-app && pnpm -r typecheck && pnpm -r test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/launcher-app ui/src
git commit -m "feat(app): DTOs, run phases and events for packs

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: The `Runner` seam learns packs; `pack_add`

**Files:**
- Modify: `crates/launcher-app/src/runner.rs`, `crates/launcher-app/src/app.rs` (`AppConfig.downloads_dir`, test `core()`), `crates/launcher-app/src/lib.rs`
- Create: `crates/launcher-app/src/packs.rs`
- Modify: `crates/launcher-app/src/testing.rs`
- Modify: `src-tauri/src/main.rs` (`build_core` only, so the workspace keeps building)

**Interfaces:**
- Produces on `trait Runner`:

```rust
    /// A new instance for a pack: named after it, with its game version, policy and art.
    /// Its files arrive with the first sync.
    fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>>;
    /// The update this pack instance could take now. `Err` is a warning: play what's installed.
    fn find_update(&self, instance: Instance) -> BoxFuture<Result<Option<PendingUpdate>>>;
    /// Bring a pack instance to `accept` (or what's installed). The caller holds `lock`.
    fn sync(
        &self,
        lock: Arc<InstanceLock>,
        id: String,
        accept: Option<PendingUpdate>,
        cancel: Cancel,
        sink: EventSink,
    ) -> BoxFuture<Result<Ensured>>;
    /// Put files the player downloaded by hand into the cache.
    fn import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>>;
```

- `CoreRunner::new(installer: Arc<Installer>, syncer: Syncer, launcher_version: String)`.
- `AppConfig` gains `pub downloads_dir: Option<PathBuf>`.
- `AppCore::pack_add(&self, request: PackAddRequest) -> Result<Instance>`, in the new `packs.rs`.
- Test helpers in `testing.rs`:
  - `up_to_date() -> Ensured`
  - `desired(version, files) -> DesiredState`
  - `pack_file(path, name, optional: Option<bool>) -> PackFile`
  - `pending(from, to) -> PendingUpdate`
  - `manual_file(file_name, body) -> ManualFile`
  - `managed_for(paths: &[&str]) -> Managed`
  - `async pack_instance(core, name, installed: Option<&DesiredState>) -> Instance`
- `FakeRunner` pub fields, used by later tasks:
  - `instances_dir: Mutex<Option<PathBuf>>`
  - `added: Mutex<Vec<PackInput>>`
  - `updates: Mutex<HashMap<String, Result<Option<PendingUpdate>>>>`
  - `checks: AtomicUsize`
  - `sync_results: Mutex<VecDeque<Result<Ensured>>>`
  - `syncs: Mutex<Vec<(String, Option<String>)>>`: (id, accepted version)
  - `hold_sync: AtomicBool`, with `fn open_sync_gate(&self, n: usize)`
  - `sync_game: Mutex<Option<GameSpec>>`
  - `imported: Mutex<Vec<ManualFile>>`
  - `prepared: Mutex<Vec<GameSpec>>`

- [ ] **Step 1: Write the failing test**

Create `crates/launcher-app/src/packs.rs`:

```rust
//! Pack commands other than playing: adding a pack, its status, its mods, manual downloads
//! and art (spec addendum §4.4). Runs and syncs live in `runs.rs`.
use launcher_core::{instances::Instance, sources::PackInput, ErrorCode, LauncherError, Result};

use crate::{app::AppCore, dto::PackAddRequest, events::Host, runner::Runner};

fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// A new instance from a pack link (https `pack.toml` or `modrinth:`) or a `.mrpack` file.
    /// Nothing but the art downloads until its first Play.
    pub async fn pack_add(&self, request: PackAddRequest) -> Result<Instance> {
        let _ = (request, link_invalid);
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{accounts::BuildKind, instances::SourceRef, sources::PackInput, ErrorCode};

    use crate::{app::tests::core, dto::PackAddRequest};

    #[tokio::test]
    async fn pack_add_takes_links_and_mrpack_files_only() {
        let (core, runner, _host, tmp) = core(BuildKind::Dev);
        let link = |url: &str| PackAddRequest::Link { url: url.into() };
        let url = "https://perlanddev.github.io/asalith-modpack/pack.toml";
        let added = core.pack_add(link(url)).await.unwrap();
        assert_eq!(added.source, SourceRef::Packwiz { url: url.into() });
        core.pack_add(link("modrinth:sky")).await.unwrap();

        let mrpack = tmp.path().join("Sky Islands.MRPACK");
        std::fs::write(&mrpack, b"zip").unwrap();
        core.pack_add(PackAddRequest::File {
            path: mrpack.clone(),
        })
        .await
        .unwrap();
        assert_eq!(
            *runner.added.lock().unwrap(),
            vec![
                PackInput::Packwiz { url: url.into() },
                PackInput::Modrinth {
                    project: "sky".into(),
                    version: None
                },
                PackInput::MrpackFile { path: mrpack },
            ]
        );

        // A link is never a file, and never plain http.
        for bad in [
            "C:/Downloads/pack.mrpack",
            "http://evil.example/pack.toml",
            "pack.toml",
        ] {
            assert_eq!(
                core.pack_add(link(bad)).await.unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad}"
            );
        }
        // A file is only ever a regular .mrpack file.
        let exe = tmp.path().join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let folder = tmp.path().join("folder.mrpack");
        std::fs::create_dir_all(&folder).unwrap();
        for bad in [exe, folder, tmp.path().join("missing.mrpack")] {
            assert_eq!(
                core.pack_add(PackAddRequest::File { path: bad.clone() })
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::PackInvalid,
                "{}",
                bad.display()
            );
        }
        assert_eq!(
            runner.added.lock().unwrap().len(),
            3,
            "refused input never reaches the core"
        );
    }
}
```

Add `mod packs;` to `lib.rs`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p launcher-app --lib packs::`
Expected: a compile error (`FakeRunner.added` doesn't exist), then a `todo!` panic once Step 4 is in.

- [ ] **Step 3: Implement the seam**

In `runner.rs`:
- Add the four trait methods from Interfaces.
- Imports: `std::path::PathBuf`, `launcher_core::{locks::InstanceLock, sources::PackInput, sync::{Cancel, Ensured, ManualFile, PendingUpdate, Syncer}}`.
- `CoreRunner` gets a `Syncer`:

```rust
pub struct CoreRunner {
    installer: Arc<Installer>,
    syncer: Arc<Syncer>,
    launcher_version: String,
}

impl CoreRunner {
    pub fn new(installer: Arc<Installer>, syncer: Syncer, launcher_version: String) -> Self {
        Self {
            installer,
            syncer: Arc::new(syncer),
            launcher_version,
        }
    }
    pub fn installer(&self) -> &Arc<Installer> {
        &self.installer
    }
    fn store(&self) -> InstanceStore {
        InstanceStore::new(self.installer.dirs.instances())
    }
}
```

In `impl Runner for CoreRunner`:

```rust
    fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { Ok(syncer.add_pack(&store, &input).await?.0) })
    }

    fn find_update(&self, instance: Instance) -> BoxFuture<Result<Option<PendingUpdate>>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move { syncer.find_update(&store, &instance).await })
    }

    fn sync(
        &self,
        lock: Arc<InstanceLock>,
        id: String,
        accept: Option<PendingUpdate>,
        cancel: Cancel,
        sink: EventSink,
    ) -> BoxFuture<Result<Ensured>> {
        let (syncer, store) = (self.syncer.clone(), self.store());
        Box::pin(async move {
            syncer
                .ensure_locked(&lock, &store, &id, accept, &sink, &cancel)
                .await
        })
    }

    fn import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>> {
        let syncer = self.syncer.clone();
        Box::pin(async move { syncer.import_found(&found).await })
    }
```

In `app.rs`:
- Add `pub downloads_dir: Option<PathBuf>` to `AppConfig`, with the doc comment "Where the player's browser saves files (manual CurseForge downloads)."
- In `tests::core`, set `downloads_dir: Some(tmp.path().join("Downloads"))`.
- After creating the runner, set `*runner.instances_dir.lock().unwrap() = Some(dirs.instances());`. Do this before `dirs` moves into the config.

In `src-tauri/src/main.rs` `build_core`:

```rust
    let installer = Arc::new(Installer {
        dirs: dirs.clone(),
        meta: meta.clone(),
        downloader: Downloader::new(http.clone()),
        platform: Platform::current(),
    });
    let syncer = launcher_core::sync::Syncer::for_installer(installer.clone(), http);
    let runner = Arc::new(CoreRunner::new(installer, syncer, version.clone()));
    let host = Arc::new(host::TauriHost::new(app));
    let config = AppConfig {
        dirs,
        build: BuildKind::current(),
        launcher_version: version,
        system_memory_mb: host::system_memory_mb(),
        downloads_dir: launcher_core::sync::manual::downloads_dir(),
    };
```

- [ ] **Step 4: Implement the fake and the helpers**

In `testing.rs`, extend `FakeRunner` with the fields from Interfaces. They start empty, plus a private `sync_gate: Arc<Semaphore>` that starts with `Semaphore::new(0)`. Add:

```rust
    /// Let `n` held syncs finish (see `hold_sync`).
    pub fn open_sync_gate(&self, n: usize) {
        self.sync_gate.add_permits(n);
    }
```

In `prepare`, record the game first: `self.prepared.lock().unwrap().push(instance.game.clone());`.

The fake's pack methods:

```rust
    fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>> {
        self.added.lock().unwrap().push(input.clone());
        let dir = self
            .instances_dir
            .lock()
            .unwrap()
            .clone()
            .expect("tests::core sets instances_dir");
        Box::pin(async move {
            let store = InstanceStore::new(dir);
            let mut instance = store
                .create(
                    "Sky Islands",
                    GameSpec {
                        minecraft: "1.20.1".into(),
                        loader: None,
                    },
                )
                .await?;
            instance.source = match input {
                PackInput::Packwiz { url } => SourceRef::Packwiz { url },
                PackInput::MrpackFile { path } => SourceRef::Mrpack {
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    sha1: "0".repeat(40),
                },
                PackInput::Modrinth { project, version } => SourceRef::Modrinth {
                    project_id: project,
                    version_id: version.unwrap_or_else(|| "v1".into()),
                },
            };
            store.save(&instance).await?;
            Ok(instance)
        })
    }

    fn find_update(&self, instance: Instance) -> BoxFuture<Result<Option<PendingUpdate>>> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        let answer = self
            .updates
            .lock()
            .unwrap()
            .get(&instance.id)
            .cloned()
            .unwrap_or(Ok(None));
        Box::pin(async move { answer })
    }

    fn sync(
        &self,
        _lock: Arc<InstanceLock>,
        id: String,
        accept: Option<PendingUpdate>,
        cancel: Cancel,
        sink: EventSink,
    ) -> BoxFuture<Result<Ensured>> {
        self.syncs
            .lock()
            .unwrap()
            .push((id.clone(), accept.map(|u| u.desired.pack_version)));
        let result = self
            .sync_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(up_to_date()));
        let hold = self.hold_sync.load(Ordering::SeqCst);
        let gate = self.sync_gate.clone();
        let game = self.sync_game.lock().unwrap().take();
        let dir = self.instances_dir.lock().unwrap().clone();
        Box::pin(async move {
            sink(Event::Progress {
                task: TaskId::new("sync_pack").with("name", "Sky Islands"),
                done: 0,
                total: 1,
                bytes: 0,
            });
            while hold {
                if cancel.is_cancelled() {
                    return Err(LauncherError::pack(ErrorCode::SyncCancelled, "cancelled"));
                }
                if let Ok(permit) = gate.try_acquire() {
                    permit.forget();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            if let (Some(game), Some(dir)) = (game, dir) {
                let store = InstanceStore::new(dir);
                let mut instance = store.load(&id).await?;
                instance.game = game;
                store.save(&instance).await?;
            }
            result
        })
    }

    fn import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>> {
        self.imported
            .lock()
            .unwrap()
            .extend(found.into_iter().map(|(file, _)| file));
        Box::pin(async { Ok(()) })
    }
```

The helpers (top of `testing.rs`):

```rust
pub(crate) fn up_to_date() -> Ensured {
    Ensured {
        outcome: SyncOutcome::Done(SyncReport {
            up_to_date: true,
            ..SyncReport::default()
        }),
        warning: None,
    }
}

pub(crate) fn pack_file(path: &str, name: &str, optional: Option<bool>) -> PackFile {
    PackFile {
        path: path.into(),
        name: name.into(),
        source: FileSource::Remote { urls: vec![] },
        hashes: Hashes::sha1("a".repeat(40)),
        size: Some(1),
        side: Side::Both,
        optional: optional.map(|default| OptionalFile {
            default,
            description: Some(format!("Optional {name}")),
        }),
        mode: FileMode::Replace,
        manual: None,
    }
}

pub(crate) fn desired(version: &str, files: Vec<PackFile>) -> DesiredState {
    DesiredState {
        name: "Sky Islands".into(),
        pack_version: version.into(),
        minecraft: "1.20.1".into(),
        loader: None,
        files,
        stamp: format!("stamp-{version}"),
        about: PackAbout::default(),
        archive: None,
    }
}

pub(crate) fn pending(from: &str, to: &str) -> PendingUpdate {
    PendingUpdate {
        source: SourceRef::Packwiz {
            url: "https://example.com/pack.toml".into(),
        },
        desired: desired(to, vec![]),
        changes: Changes {
            from_version: Some(from.into()),
            to_version: to.into(),
            added: vec!["JEI".into()],
            updated: vec![],
            removed: vec!["Zoomify".into()],
        },
        kept: vec![],
    }
}

pub(crate) fn manual_file(file_name: &str, body: &[u8]) -> ManualFile {
    ManualFile {
        path: format!("mods/{file_name}"),
        name: file_name.trim_end_matches(".jar").into(),
        file_name: file_name.into(),
        page_url: "https://www.curseforge.com/projects/1".into(),
        hashes: Hashes::sha1(sha1_hex(body)),
    }
}

/// A lockfile that says sync wrote each of `paths`.
pub(crate) fn managed_for(paths: &[&str]) -> Managed {
    Managed {
        version: Managed::VERSION,
        stamp: "stamp-1.0.3".into(),
        pack_version: "1.0.3".into(),
        choices: BTreeMap::new(),
        files: paths
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    ManagedFile {
                        name: p.to_string(),
                        sha1: "a".repeat(40),
                        size: 1,
                        mode: FileMode::Replace,
                        declared: Hashes::sha1("a".repeat(40)),
                    },
                )
            })
            .collect(),
    }
}

/// A packwiz instance named `name`; `installed` becomes its `pack.json` (as if it had synced).
pub(crate) async fn pack_instance(
    core: &AppCore<FakeRunner, RecordingHost>,
    name: &str,
    installed: Option<&DesiredState>,
) -> Instance {
    let mut instance = core
        .instance_create(NewInstance {
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: None,
        })
        .await
        .unwrap();
    instance.source = SourceRef::Packwiz {
        url: "https://example.com/pack.toml".into(),
    };
    core.instances.save(&instance).await.unwrap();
    if let Some(d) = installed {
        lockfile::save_pack(&core.instances.perland_dir(&instance.id), d)
            .await
            .unwrap();
    }
    instance
}
```

Imports:
- `std::{collections::{BTreeMap, VecDeque}, path::PathBuf, sync::atomic::AtomicBool}`
- `launcher_core::{download::{hash::sha1_hex, Hashes}, instances::{GameSpec, InstanceStore, SourceRef}, locks::InstanceLock, sources::{DesiredState, FileMode, FileSource, OptionalFile, PackAbout, PackFile, PackInput, Side}, sync::{lockfile::{self, Managed, ManagedFile}, plan::Changes, Cancel, Ensured, ManualFile, PendingUpdate, SyncOutcome, SyncReport}}`
- `crate::{app::AppCore, dto::NewInstance}`

- [ ] **Step 5: Implement `pack_add`**

Replace the stub in `packs.rs`:

```rust
    pub async fn pack_add(&self, request: PackAddRequest) -> Result<Instance> {
        let input = match request {
            PackAddRequest::Link { url } => match PackInput::parse(&url)? {
                PackInput::MrpackFile { .. } => return Err(link_invalid()),
                input => input,
            },
            PackAddRequest::File { path } => {
                let is_mrpack = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"));
                let is_file = tokio::fs::metadata(&path)
                    .await
                    .is_ok_and(|m| m.is_file());
                if !is_mrpack || !is_file {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    return Err(LauncherError::pack(
                        ErrorCode::PackInvalid,
                        format!("{name} isn't a .mrpack file"),
                    )
                    .with_param("file", name));
                }
                PackInput::MrpackFile { path }
            }
        };
        self.runner.add_pack(input).await
    }
```

`PackInput::parse` already refuses `http://` to a non-loopback host, and a bare `pack.toml`, with `pack_link_invalid`.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p launcher-app && cargo build -p perland-launcher`
Expected: PASS and OK.

- [ ] **Step 7: Commit**

```bash
git add crates/launcher-app src-tauri/src/main.rs
git commit -m "feat(app): the runner seam learns packs; pack_add takes links and .mrpack files only

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 8: Play checks, asks, syncs, and holds the lock until the game exits

**Files:**
- Modify: `crates/launcher-app/src/runs.rs`
- Modify: `crates/launcher-app/src/app.rs` (`Run`, `ManualState`, `AppCore.manual`)
- Modify: `crates/launcher-app/src/testing.rs` (`RecordingHost` helpers)

**Interfaces:**
- Consumes the seam from Task 7 and the DTOs from Task 6.
- Produces:
  - `AppCore::play_answer(&self, id: &str, update: bool) -> Result<()>`.
  - `pub(crate) fn start_run(self: &Arc<Self>, id: &str, instance: Instance, play: Option<(Session, bool)>, checks: bool) -> Result<()>`. Task 9 calls it with `play: None`.
  - `pub(crate) async fn checks_for_updates(&self, instance: &Instance) -> bool`.
  - `pub(crate) struct ManualState { total: usize, remaining: Vec<ManualFile>, scanner: Scanner }` and `AppCore.manual: tokio::sync::Mutex<HashMap<String, ManualState>>`. Task 11 reads them.
  - While `asking`, `run_states()` snapshots carry the offer.
- Run phases:
  - **Pack instance:** `checking → [asking] → queued → syncing → preparing → running → [stopping] → idle`. A never-synced or pinned pack starts at `queued`.
  - **Vanilla:** unchanged (`queued → preparing → running`).
- Stop:
  - In `checking`, `asking`, `queued` or `preparing`, Stop cancels at once, as today.
  - In `syncing`, Stop asks the sync to stop and shows `stopping`. The run ends `cancelled` once the sync has returned and nothing is writing.

- [ ] **Step 1: Write the failing tests**

Add to `RecordingHost` in `testing.rs`:

```rust
    pub fn last_update(&self, id: &str) -> Option<PackUpdate> {
        self.events.lock().unwrap().iter().rev().find_map(|e| match e {
            AppEvent::RunState {
                instance,
                update: Some(update),
                ..
            } if instance == id => Some(update.clone()),
            _ => None,
        })
    }

    /// Wait until `id` has reported `state` at least `n` times (panics after 5 s).
    pub async fn wait_for_nth(&self, id: &str, state: RunStateKind, n: usize) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.states(id).iter().filter(|s| **s == state).count() < n {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{id} never reached {state:?} {n} times; saw {:?}",
                self.states(id)
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
```

In `runs.rs` `mod tests`:
- Add imports: `launcher_core::{instances::{GameSpec, UpdatePolicy}, locks::InstanceLock, sync::{plan::Changes, Ensured, SyncOutcome, SyncReport}}`, `crate::testing::{desired, manual_file, pack_instance, pending}`, `std::sync::atomic::Ordering` (already there).
- Then add:

```rust
    /// A pack instance "Sky" (id `sky`) that synced 1.0.3 before, with `policy`.
    async fn pack(core: &TestCore, policy: UpdatePolicy) -> String {
        let installed = desired("1.0.3", vec![]);
        let mut instance = pack_instance(core, "Sky", Some(&installed)).await;
        instance.update_policy = policy;
        core.instances.save(&instance).await.unwrap();
        instance.id
    }

    fn offer(runner: &FakeRunner, id: &str) {
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.to_string(), Ok(Some(pending("1.0.3", "1.0.4"))));
    }

    fn lock_of(core: &TestCore, id: &str) -> launcher_core::Result<InstanceLock> {
        InstanceLock::try_acquire(&core.instances.perland_dir(id))
    }

    #[tokio::test]
    async fn a_pack_checks_then_syncs_then_plays() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Preparing, Running]
        );
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        assert!(
            host.last_exit(&id).unwrap().synced.is_none(),
            "nothing changed"
        );
    }

    #[tokio::test]
    async fn ask_waits_for_the_player_and_takes_the_update() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        assert_eq!(host.last_update(&id).unwrap().to_version, "1.0.4");
        let snapshot = core.run_states();
        assert_eq!(
            snapshot[0].update.as_ref().unwrap().from_version.as_deref(),
            Some("1.0.3"),
            "a reloaded UI can show the dialog again"
        );
        assert!(
            runner.syncs.lock().unwrap().is_empty(),
            "nothing syncs before the answer"
        );
        core.play_answer(&id, true).unwrap();
        core.play_answer(&id, true).unwrap(); // a second click is harmless
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Asking, Queued, Syncing, Preparing, Running]
        );
        assert_eq!(
            *runner.syncs.lock().unwrap(),
            vec![(id.clone(), Some("1.0.4".to_string()))]
        );
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn playing_the_installed_version_skips_the_update() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        core.play_answer(&id, false).unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn auto_takes_updates_without_asking_and_pinned_never_checks() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Auto).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Preparing, Running]
        );
        assert_eq!(runner.syncs.lock().unwrap()[0].1.as_deref(), Some("1.0.4"));
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;

        let mut pinned = core.instances.load(&id).await.unwrap();
        pinned.update_policy = UpdatePolicy::Pinned;
        core.instances.save(&pinned).await.unwrap();
        let checks = runner.checks.load(Ordering::SeqCst);
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for_nth(&id, Running, 2).await;
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "pinned never checks"
        );
        let states = host.states(&id);
        assert_eq!(
            &states[states.len() - 4..],
            &[Queued, Syncing, Preparing, Running]
        );
        core.stop(&id).unwrap();
        host.wait_for_nth(&id, Idle, 2).await;
    }

    #[tokio::test]
    async fn stop_while_asking_cancels_without_syncing() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        core.stop(&id).unwrap();
        assert_eq!(host.states(&id), vec![Checking, Asking, Idle]);
        assert!(host.last_exit(&id).unwrap().cancelled);
        assert!(runner.syncs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_check_warns_and_plays_the_installed_version() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::network(
                ErrorCode::UpdateCheckFailed,
                "Couldn't check for updates",
            )),
        );
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert!(host.warnings().contains(&ErrorCode::UpdateCheckFailed));
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn stop_while_syncing_waits_for_the_sync_to_stop() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.hold_sync.store(true, Ordering::SeqCst);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Syncing).await;
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Stopping, Idle]
        );
        assert!(host.last_exit(&id).unwrap().cancelled);
        assert!(runner.prepared.lock().unwrap().is_empty());
        lock_of(&core, &id).expect("released once the sync stopped");
    }

    #[tokio::test]
    async fn blocked_files_end_the_run_with_the_manual_list() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.sync_results.lock().unwrap().push_back(Ok(Ensured {
            outcome: SyncOutcome::NeedsManual(vec![manual_file("optifine.jar", b"optifine")]),
            warning: None,
        }));
        core.play(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        let exit = host.last_exit(&id).unwrap();
        let error = exit.error.clone().unwrap();
        assert_eq!(error.code, ErrorCode::ManualDownloadsNeeded);
        assert_eq!(error.params["count"], "1");
        assert_eq!(exit.manual.len(), 1);
        assert!(!exit.is_crash());
        assert!(core.manual.lock().await.contains_key(&id));
    }

    #[tokio::test]
    async fn the_lock_is_held_from_check_to_game_exit() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        assert_eq!(
            lock_of(&core, &id).unwrap_err().code,
            ErrorCode::InstanceLocked
        );
        core.play_answer(&id, false).unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            lock_of(&core, &id).unwrap_err().code,
            ErrorCode::InstanceLocked
        );
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        lock_of(&core, &id).expect("free once the game exited");
    }

    #[tokio::test]
    async fn a_locked_instance_ends_the_run_with_instance_locked() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        let _cli = lock_of(&core, &id).unwrap(); // the CLI is syncing it
        core.play(&id).await.unwrap();
        host.wait_for(&id, Idle).await; // after the short retry
        assert_eq!(
            host.last_exit(&id).unwrap().error.unwrap().code,
            ErrorCode::InstanceLocked
        );
        assert!(runner.syncs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn play_again_right_after_a_cancel_gets_the_lock() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        core.stop(&id).unwrap();
        core.play(&id).await.unwrap(); // the cancelled task may still hold the lock
        host.wait_for_nth(&id, Asking, 2).await;
        core.stop(&id).unwrap();
    }

    #[tokio::test]
    async fn a_pack_update_that_changes_the_game_prepares_the_new_version() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        *runner.sync_game.lock().unwrap() = Some(GameSpec {
            minecraft: "1.21.1".into(),
            loader: None,
        });
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(runner.prepared.lock().unwrap()[0].minecraft, "1.21.1");
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn sync_results_reach_the_exit_and_warnings_reach_the_ui() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.sync_results.lock().unwrap().push_back(Ok(Ensured {
            outcome: SyncOutcome::Done(SyncReport {
                up_to_date: false,
                changes: Changes {
                    from_version: Some("1.0.3".into()),
                    to_version: "1.0.4".into(),
                    added: vec!["JEI".into()],
                    updated: vec![],
                    removed: vec![],
                },
                kept_modified: vec!["config/a.toml".into()],
                game_changed: false,
            }),
            warning: Some(
                LauncherError::network(ErrorCode::UpdateDownloadFailed, "x")
                    .with_param("version", "1.0.3"),
            ),
        }));
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        let synced = host.last_exit(&id).unwrap().synced.unwrap();
        assert_eq!((synced.to_version.as_str(), synced.added), ("1.0.4", 1));
        assert_eq!(synced.kept_modified, vec!["config/a.toml".to_string()]);
        assert!(host.warnings().contains(&ErrorCode::UpdateDownloadFailed));
    }
```

`RunStateKind::*` is already glob-imported in that test module, so `Checking`, `Asking` and `Syncing` resolve once they exist. `FakeRunner` is imported from `crate::testing`.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib runs::`
Expected: compile errors (`play_answer` and `manual` don't exist), then assertion failures (the states lack `Checking`).

- [ ] **Step 3: Extend `Run` and `AppCore`**

In `app.rs`:

```rust
/// One instance's current run. Absent from `runs` means idle.
pub(crate) struct Run {
    pub(crate) phase: RunStateKind,
    /// Set until the game is running: aborting the task cancels the run.
    pub(crate) abort: Option<AbortHandle>,
    /// Set once the game is running: stopping kills it.
    pub(crate) killer: Option<Arc<dyn Kill>>,
    /// Asks a sync to stop at its next file (Stop while syncing).
    pub(crate) cancel: Cancel,
    /// Set while asking: delivers the player's answer to "Update ready".
    pub(crate) answer: Option<oneshot::Sender<bool>>,
    /// The update on offer while asking, for `run_states` after a reload.
    pub(crate) offer: Option<PackUpdate>,
}

/// Files the last sync couldn't download, while the player fetches them by hand.
pub(crate) struct ManualState {
    pub(crate) total: usize,
    pub(crate) remaining: Vec<ManualFile>,
    pub(crate) scanner: Scanner,
}
```

Add `pub(crate) manual: tokio::sync::Mutex<HashMap<String, ManualState>>` to `AppCore` and initialize it empty in `new`. Imports: `launcher_core::sync::{manual::Scanner, Cancel, ManualFile}`, `tokio::sync::oneshot`, `crate::dto::PackUpdate`.

- [ ] **Step 4: Rewrite the run state machine**

In `runs.rs`:
- Update the module doc to the new phase list.
- Add imports: `launcher_core::{instances::{SourceRef, UpdatePolicy}, locks::InstanceLock, sync::{lockfile::PACK_FILE, manual::Scanner, Cancel, Ensured, ManualFile, PendingUpdate, SyncOutcome}}`, `tokio::sync::oneshot`, `crate::{app::ManualState, dto::{PackUpdate, SyncSummary}}`.
- Replace `play`, `stop`, `run_states`, `set_phase`, `drive` and `finish`, and add the helpers.

```rust
/// How long a run waits for an instance lock that a just-ended run is still letting go of.
const LOCK_WAIT: Duration = Duration::from_secs(2);

impl<R: Runner, H: Host> AppCore<R, H> {
    /// Queue a run. Everything after this arrives as events.
    pub async fn play(self: &Arc<Self>, id: &str) -> Result<()> {
        // … unchanged: load + id check, account, session, `minimize` …
        let checks = self.checks_for_updates(&instance).await;
        self.start_run(id, instance, Some((session, minimize)), checks)
    }

    /// Does a run of `instance` start with an update check? Only a pack that synced before
    /// and isn't pinned: a first sync installs the newest version anyway.
    pub(crate) async fn checks_for_updates(&self, instance: &Instance) -> bool {
        instance.source != SourceRef::None
            && instance.update_policy != UpdatePolicy::Pinned
            && tokio::fs::try_exists(self.instances.perland_dir(&instance.id).join(PACK_FILE))
                .await
                .unwrap_or(false)
    }

    /// Start a run. `play` is the session and minimize setting, or `None` to only take the
    /// pack's update (the banner's Update).
    pub(crate) fn start_run(
        self: &Arc<Self>,
        id: &str,
        instance: Instance,
        play: Option<(Session, bool)>,
        checks: bool,
    ) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if runs.contains_key(id) {
            return Err(LauncherError::game(
                ErrorCode::AlreadyRunning,
                "This instance is already running",
            )
            .with_param("id", id));
        }
        self.logs.reset(id);
        let first = if checks {
            RunStateKind::Checking
        } else {
            RunStateKind::Queued
        };
        // Emitted under the lock, before the task exists, so the first state always comes first.
        self.emit_state(id, first, None);
        let cancel = Cancel::default();
        let core = Arc::clone(self);
        let (run_id, run_cancel) = (id.to_string(), cancel.clone());
        let task = tokio::spawn(async move {
            core.drive(run_id, instance, play, checks, run_cancel)
                .await
        });
        runs.insert(
            id.to_string(),
            Run {
                phase: first,
                abort: Some(task.abort_handle()),
                killer: None,
                cancel,
                answer: None,
                offer: None,
            },
        );
        Ok(())
    }

    /// The player's answer to "Update ready": take the update, or play what's installed.
    /// Ignored unless the run is asking, so a double click is harmless.
    pub fn play_answer(&self, id: &str, update: bool) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = runs.get_mut(id) {
            if run.phase == RunStateKind::Asking {
                if let Some(answer) = run.answer.take() {
                    let _ = answer.send(update);
                }
                run.offer = None;
            }
        }
        Ok(())
    }

    /// Cancel a run that hasn't started its game, or kill a running game. A sync is asked to
    /// stop, and the run ends once it has. Unknown or idle ids are fine.
    pub fn stop(&self, id: &str) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        let Some(run) = runs.get_mut(id) else {
            return Ok(());
        };
        match run.phase {
            RunStateKind::Checking
            | RunStateKind::Asking
            | RunStateKind::Queued
            | RunStateKind::Preparing => {
                if let Some(abort) = run.abort.take() {
                    abort.abort();
                }
                runs.remove(id);
                self.emit_state(
                    id,
                    RunStateKind::Idle,
                    Some(ExitInfo {
                        cancelled: true,
                        ..ExitInfo::default()
                    }),
                );
            }
            // Aborting mid-sync could leave a file write running after the lock is gone.
            RunStateKind::Syncing => {
                run.phase = RunStateKind::Stopping;
                run.cancel.cancel();
                self.emit_state(id, RunStateKind::Stopping, None);
            }
            RunStateKind::Running => {
                run.phase = RunStateKind::Stopping;
                if let Some(killer) = &run.killer {
                    killer.kill();
                }
                self.emit_state(id, RunStateKind::Stopping, None);
            }
            RunStateKind::Stopping | RunStateKind::Idle => {}
        }
        Ok(())
    }
```

`stop_all`, `has_active_runs` and `log_tail` stay. `run_states` maps `update: run.offer.clone()`. `set_phase` never leaves `stopping`:

```rust
    fn set_phase(&self, id: &str, phase: RunStateKind) {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = runs.get_mut(id) {
            if run.phase != RunStateKind::Stopping {
                run.phase = phase;
                self.emit_state(id, phase, None);
            }
        }
    }

    /// The instance's lock. A run that just ended may still be letting go of it (a cancelled
    /// task is dropped a moment after its `idle`), so a busy lock is retried briefly.
    async fn lock_instance(&self, id: &str) -> Result<InstanceLock> {
        let dir = self.instances.perland_dir(id);
        let deadline = tokio::time::Instant::now() + LOCK_WAIT;
        loop {
            match InstanceLock::try_acquire(&dir) {
                Err(e)
                    if e.code == ErrorCode::InstanceLocked
                        && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                other => return other,
            }
        }
    }

    /// Check for an update and decide whether this run takes it. `auto` packs and update-only
    /// runs take it, and `ask` asks the player. A failed check is a warning: the installed
    /// version plays.
    async fn decide_update(
        &self,
        id: &str,
        instance: &Instance,
        update_only: bool,
    ) -> Option<PendingUpdate> {
        let update = match self.runner.find_update(instance.clone()).await {
            Ok(update) => update?,
            Err(error) => {
                self.host.emit(AppEvent::Warning {
                    instance: Some(id.to_string()),
                    error,
                });
                return None;
            }
        };
        if update_only || instance.update_policy == UpdatePolicy::Auto {
            return Some(update);
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
            let run = runs.get_mut(id)?;
            let offer = PackUpdate::from(&update);
            run.phase = RunStateKind::Asking;
            run.answer = Some(tx);
            run.offer = Some(offer.clone());
            self.host.emit(AppEvent::RunState {
                instance: id.to_string(),
                state: RunStateKind::Asking,
                exit: None,
                update: Some(offer),
            });
        }
        match rx.await {
            Ok(true) => Some(update),
            _ => None,
        }
    }

    /// The sync stopped for files the player must download by hand: remember them for
    /// `manual_import`, and end the run with the list.
    async fn needs_manual(&self, id: &str, files: Vec<ManualFile>) -> ExitInfo {
        self.manual.lock().await.insert(
            id.to_string(),
            ManualState {
                total: files.len(),
                remaining: files.clone(),
                scanner: Scanner::default(),
            },
        );
        ExitInfo {
            error: Some(
                LauncherError::pack(
                    ErrorCode::ManualDownloadsNeeded,
                    format!("{} files have to be downloaded by hand", files.len()),
                )
                .with_param("count", files.len())
                .with_fix("Download them from CurseForge, then try again"),
            ),
            manual: files,
            ..ExitInfo::default()
        }
    }
```

`drive` and `finish` become:

```rust
    async fn drive(
        self: Arc<Self>,
        id: String,
        instance: Instance,
        play: Option<(Session, bool)>,
        checks: bool,
        cancel: Cancel,
    ) {
        let update_only = play.is_none();
        let ended = |info: ExitInfo| ExitInfo {
            update_only,
            ..info
        };
        let (log_tx, batcher) = spawn_batcher(id.clone(), self.host.clone(), self.logs.clone());
        let sink = self.run_sink(&id, log_tx);
        // Held from the update check until the run ends: the CLI or another window can't
        // change this instance's files meanwhile (spec addendum §4.2).
        let lock = match self.lock_instance(&id).await {
            Ok(lock) => Arc::new(lock),
            Err(error) => {
                return self
                    .finish(&id, ended(failed(error)), sink, batcher, None)
                    .await
            }
        };
        let accept = if checks {
            let accept = self.decide_update(&id, &instance, update_only).await;
            self.set_phase(&id, RunStateKind::Queued);
            accept
        } else {
            None
        };
        let Ok(permit) = self.install_slot.clone().acquire_owned().await else {
            return;
        };
        let mut instance = instance;
        let mut synced = None;
        if instance.source != SourceRef::None {
            self.set_phase(&id, RunStateKind::Syncing);
            let result = self
                .runner
                .sync(lock.clone(), id.clone(), accept, cancel.clone(), sink.clone())
                .await;
            let report = match result {
                Ok(Ensured {
                    outcome: SyncOutcome::Done(report),
                    warning,
                }) => {
                    if let Some(error) = warning {
                        self.host.emit(AppEvent::Warning {
                            instance: Some(id.clone()),
                            error,
                        });
                    }
                    self.manual.lock().await.remove(&id);
                    report
                }
                Ok(Ensured {
                    outcome: SyncOutcome::NeedsManual(files),
                    ..
                }) => {
                    let info = self.needs_manual(&id, files).await;
                    return self
                        .finish(&id, ended(info), sink, batcher, Some(lock))
                        .await;
                }
                Err(error) if error.code == ErrorCode::SyncCancelled => {
                    let info = ExitInfo {
                        cancelled: true,
                        ..ExitInfo::default()
                    };
                    return self
                        .finish(&id, ended(info), sink, batcher, Some(lock))
                        .await;
                }
                Err(error) => {
                    return self
                        .finish(&id, ended(failed(error)), sink, batcher, Some(lock))
                        .await
                }
            };
            synced = (!report.up_to_date).then(|| SyncSummary::from(&report));
            // Stop arrived once the swap had begun: the sync finished, but the player wants out.
            if cancel.is_cancelled() {
                let info = ExitInfo {
                    cancelled: true,
                    synced,
                    ..ExitInfo::default()
                };
                return self
                    .finish(&id, ended(info), sink, batcher, Some(lock))
                    .await;
            }
            // A pack update can change the game version.
            instance = match self.instances.load(&id).await {
                Ok(instance) => instance,
                Err(error) => {
                    return self
                        .finish(&id, ended(failed(error)), sink, batcher, Some(lock))
                        .await
                }
            };
        }
        let Some((session, minimize)) = play else {
            drop(permit);
            let info = ExitInfo {
                synced,
                update_only: true,
                ..ExitInfo::default()
            };
            return self.finish(&id, info, sink, batcher, Some(lock)).await;
        };
        self.set_phase(&id, RunStateKind::Preparing);
        let prepared = match self.runner.prepare(instance.clone(), sink.clone()).await {
            Ok(prepared) => prepared,
            Err(error) => {
                let info = ExitInfo {
                    synced,
                    ..failed(error)
                };
                return self.finish(&id, info, sink, batcher, Some(lock)).await;
            }
        };
        drop(permit);
        let game = match self
            .runner
            .start(instance, prepared, session, sink.clone())
            .await
        {
            Ok(game) => game,
            Err(error) => {
                let info = ExitInfo {
                    synced,
                    ..failed(error)
                };
                return self.finish(&id, info, sink, batcher, Some(lock)).await;
            }
        };
        // … the killer-recording block stays exactly as it is …
        if minimize {
            self.host.minimize();
        }
        let info = match game.wait().await {
            Ok(exit) => ExitInfo {
                code: exit.code,
                killed: exit.killed,
                crash: exit.crash,
                synced,
                ..ExitInfo::default()
            },
            Err(error) => ExitInfo {
                synced,
                ..failed(error)
            },
        };
        let crashed = info.is_crash();
        self.finish(&id, info, sink, batcher, Some(lock)).await;
        if minimize || crashed {
            self.host.restore();
        }
    }

    async fn finish(
        &self,
        id: &str,
        info: ExitInfo,
        sink: EventSink,
        batcher: JoinHandle<()>,
        lock: Option<Arc<InstanceLock>>,
    ) {
        drop(sink);
        // Let the last log lines reach the UI before the idle state does.
        let _ = tokio::time::timeout(Duration::from_secs(2), batcher).await;
        // Let go of the instance before saying so: a Play right after `idle` must get the lock.
        drop(lock);
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        // A cancelled run was already removed and reported by `stop`. The final state goes out
        // under the lock, like `stop` does, so a Play arriving now can't report `queued` first.
        if runs.remove(id).is_some() {
            self.emit_state(id, RunStateKind::Idle, Some(info));
        }
    }
```

- [ ] **Step 5: Run every app test**

Run: `cargo test -p launcher-app`
Expected: PASS, the new tests and all existing run tests alike.
- Vanilla still reports `[Queued, Preparing, Running, Idle]`.
- `a_finished_runs_idle_is_reported_before_the_next_runs_queued` still holds, because `finish` drops the instance lock and then emits `idle` under the runs lock.

- [ ] **Step 6: Commit**

```bash
git add crates/launcher-app
git commit -m "feat(app): Play checks for pack updates, asks, syncs, and holds the instance lock until the game exits

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: The banner's Update (update-only runs) and `pack_status`

**Files:**
- Modify: `crates/launcher-app/src/packs.rs`

**Interfaces:**
- Produces:
  - `AppCore::pack_update(self: &Arc<Self>, id: &str) -> Result<()>`: an update-only run, `checking → queued → syncing → idle`, ending with `exit.updateOnly` set.
  - `AppCore::pack_status(&self, id: &str) -> Result<PackStatus>`.
  - `pub(crate) async fn pack_instance(&self, id: &str) -> Result<Instance>`: refuses a non-pack instance with `no_pack`.

- [ ] **Step 1: Write the failing tests**

In `packs.rs` `mod tests`, add imports: `std::sync::atomic::Ordering`, `launcher_core::LauncherError`, `crate::{dto::NewInstance, events::RunStateKind::*, testing::{desired, exit, pack_instance, pending}}`.

```rust
    #[tokio::test]
    async fn the_banner_update_syncs_without_playing() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        core.pack_update(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(host.states(&id), vec![Checking, Queued, Syncing, Idle]);
        assert_eq!(
            *runner.syncs.lock().unwrap(),
            vec![(id.clone(), Some("1.0.4".to_string()))]
        );
        let exit = host.last_exit(&id).unwrap();
        assert!(exit.update_only && !exit.is_crash() && exit.error.is_none());
        assert!(runner.prepared.lock().unwrap().is_empty(), "no game");
    }

    #[tokio::test]
    async fn pack_update_refuses_vanilla_and_busy_instances() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        core.account_add_offline("Steve").await.unwrap();
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(
            core.pack_update(&plain.id).await.unwrap_err().code,
            ErrorCode::NoPack
        );
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            core.pack_update(&id).await.unwrap_err().code,
            ErrorCode::AlreadyRunning
        );
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn pack_status_reports_the_update_or_why_not() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        let status = core.pack_status(&id).await.unwrap();
        assert_eq!(status.installed_version.as_deref(), Some("1.0.3"));
        assert!(status.update.is_none() && status.check_error.is_none());

        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        assert_eq!(
            core.pack_status(&id).await.unwrap().update.unwrap().to_version,
            "1.0.4"
        );

        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::network(
                ErrorCode::UpdateCheckFailed,
                "Couldn't check for updates",
            )),
        );
        assert_eq!(
            core.pack_status(&id)
                .await
                .unwrap()
                .check_error
                .unwrap()
                .code,
            ErrorCode::UpdateCheckFailed
        );

        let never = pack_instance(&core, "Fresh", None).await.id;
        let checks = runner.checks.load(Ordering::SeqCst);
        let status = core.pack_status(&never).await.unwrap();
        assert!(status.installed_version.is_none() && status.update.is_none());
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "nothing installed to compare against"
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib packs::`
Expected: compile errors (`pack_update` and `pack_status` don't exist).

- [ ] **Step 3: Implement**

In `packs.rs`, add imports `std::sync::Arc`, `launcher_core::{instances::SourceRef, sync::lockfile}` and `crate::dto::{PackStatus, PackUpdate}`, plus:

```rust
fn no_pack() -> LauncherError {
    LauncherError::pack(ErrorCode::NoPack, "This instance doesn't come from a pack")
}
```

Then, inside the `impl` block next to `pack_add`:

```rust
    /// The instance `id`, if it comes from a pack.
    pub(crate) async fn pack_instance(&self, id: &str) -> Result<Instance> {
        let instance = self.instances.load(id).await?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        Ok(instance)
    }

    /// The banner's Update: take the pack's update without playing.
    pub async fn pack_update(self: &Arc<Self>, id: &str) -> Result<()> {
        let instance = self.pack_instance(id).await?;
        let checks = self.checks_for_updates(&instance).await;
        self.start_run(id, instance, None, checks)
    }

    /// For the banner: the installed version and the update on offer. A failed check is
    /// reported in `check_error`, not as an error: the banner just says it couldn't check.
    pub async fn pack_status(&self, id: &str) -> Result<PackStatus> {
        let instance = self.pack_instance(id).await?;
        let installed = lockfile::load_pack(&self.instances.perland_dir(id)).await;
        let (update, check_error) = match &installed {
            None => (None, None),
            Some(_) => match self.runner.find_update(instance).await {
                Ok(update) => (update.as_ref().map(PackUpdate::from), None),
                Err(error) => (None, Some(error)),
            },
        };
        Ok(PackStatus {
            installed_version: installed.map(|d| d.pack_version),
            update,
            check_error,
        })
    }
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-app`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-app/src/packs.rs
git commit -m "feat(app): update-only runs for the banner, and pack_status

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: A pack's mods and optional choices; guarding pack mods and webview paths

**Files:**
- Modify: `crates/launcher-app/src/packs.rs`
- Modify: `crates/launcher-app/src/app.rs` (`mod_set_enabled`, `mod_remove`, `java_test`)

**Interfaces:**
- Produces:
  - `AppCore::pack_contents(&self, id) -> Result<PackContents>`: managed mods, sorted by name case-insensitively.
  - `AppCore::pack_set_optional(&self, id, path: &str, enabled: bool) -> Result<Instance>`.
- Changes:
  - `mod_set_enabled` and `mod_remove` refuse a file the lockfile manages, with `mod_managed` (param `file`).
  - `java_test` refuses anything that isn't a regular file named `java`, `javaw`, `java.exe` or `javaw.exe`, with `java_test_failed`, and never runs it.

- [ ] **Step 1: Write the failing tests**

In `packs.rs` `mod tests` (add `crate::testing::{managed_for, pack_file}` to the imports):

```rust
    /// "Sky" with JEI (managed, on disk), Shaders (optional, off by default), a config file,
    /// and the player's own sodium.jar.
    async fn sky(core: &crate::app::tests::TestCore) -> String {
        let installed = desired(
            "1.0.3",
            vec![
                pack_file("mods/jei.jar", "JEI", None),
                pack_file("mods/shaders.jar", "Shaders", Some(false)),
                pack_file("config/a.toml", "a.toml", None),
            ],
        );
        let id = pack_instance(core, "Sky", Some(&installed)).await.id;
        managed_for(&["mods/jei.jar", "config/a.toml"])
            .save(&core.instances.perland_dir(&id))
            .await
            .unwrap();
        let mods = core.instances.game_dir(&id).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("jei.jar"), b"jei").unwrap();
        std::fs::write(mods.join("sodium.jar"), b"sodium").unwrap();
        id
    }

    #[tokio::test]
    async fn pack_contents_splits_the_pack_from_the_players_additions() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = sky(&core).await;
        let contents = core.pack_contents(&id).await.unwrap();
        let names: Vec<_> = contents.managed.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["JEI", "Shaders"], "mods only, sorted");
        assert!(contents.managed[0].optional.is_none());
        let shaders = contents.managed[1].optional.as_ref().unwrap();
        assert!(!shaders.enabled);
        assert_eq!(shaders.description.as_deref(), Some("Optional Shaders"));
        let additions: Vec<_> = contents
            .additions
            .iter()
            .map(|m| m.file_name.as_str())
            .collect();
        assert_eq!(additions, vec!["sodium.jar"]);
    }

    #[tokio::test]
    async fn optional_choices_save_for_the_next_sync() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = sky(&core).await;
        let saved = core
            .pack_set_optional(&id, "mods/shaders.jar", true)
            .await
            .unwrap();
        assert_eq!(saved.optional_choices.get("mods/shaders.jar"), Some(&true));
        let contents = core.pack_contents(&id).await.unwrap();
        assert!(contents.managed[1].optional.as_ref().unwrap().enabled);
        for not_optional in ["mods/jei.jar", "mods/nope.jar", "config/a.toml"] {
            assert_eq!(
                core.pack_set_optional(&id, not_optional, false)
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::ModNotFound,
                "{not_optional}"
            );
        }
    }

    #[tokio::test]
    async fn pack_mods_cannot_be_toggled_or_removed_but_additions_can() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = sky(&core).await;
        let refused = core
            .mod_set_enabled(&id, "jei.jar", false)
            .await
            .unwrap_err();
        assert_eq!(refused.code, ErrorCode::ModManaged);
        assert_eq!(refused.params["file"], "jei.jar");
        assert_eq!(
            core.mod_remove(&id, "jei.jar").await.unwrap_err().code,
            ErrorCode::ModManaged
        );
        core.mod_set_enabled(&id, "sodium.jar", false).await.unwrap();
        core.mod_remove(&id, "sodium.jar.disabled").await.unwrap();
    }

    #[tokio::test]
    async fn webview_paths_must_be_the_right_kind_of_file() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let tool = tmp.path().join("tool.exe");
        std::fs::write(&tool, b"MZ").unwrap();
        let folder = tmp.path().join("java");
        std::fs::create_dir_all(&folder).unwrap();
        for bad in [tool.clone(), folder] {
            assert_eq!(
                core.java_test(bad).await.unwrap_err().code,
                ErrorCode::JavaTestFailed
            );
        }
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(
            core.mods_add(&plain.id, vec![tool]).await.unwrap_err().code,
            ErrorCode::ModNotJar
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib packs::`
Expected: compile errors, then `ModManaged` failures. The `java_test` case fails only by luck today: `tool.exe` is executed.

- [ ] **Step 3: Implement**

In `packs.rs`, add imports `launcher_core::{mods, sync::{lockfile::Managed, plan}}` and `crate::dto::{OptionalChoice, PackContents, PackMod}`, then:

```rust
    /// A pack instance's mods: the pack's own (from `pack.json`, client side, under `mods/`)
    /// and the jars the player added.
    pub async fn pack_contents(&self, id: &str) -> Result<PackContents> {
        let instance = self.pack_instance(id).await?;
        let perland = self.instances.perland_dir(id);
        let game_dir = self.instances.game_dir(id);
        let installed = lockfile::load_pack(&perland).await;
        let mut managed: Vec<PackMod> = match &installed {
            None => Vec::new(),
            Some(pack) => {
                let choices = plan::effective_choices(pack, &instance.optional_choices);
                pack.files
                    .iter()
                    .filter(|f| f.side.on_client() && f.path.starts_with("mods/"))
                    .map(|f| PackMod {
                        path: f.path.clone(),
                        name: f.name.clone(),
                        optional: f.optional.as_ref().map(|o| OptionalChoice {
                            enabled: choices.get(&f.path).copied().unwrap_or(o.default),
                            description: o.description.clone(),
                        }),
                    })
                    .collect()
            }
        };
        managed.sort_by_key(|m| m.name.to_lowercase());
        let lock = Managed::load(&perland).await;
        let names = lockfile::additions(&game_dir, lock.as_ref()).await?;
        let additions = mods::list(&game_dir)
            .await?
            .into_iter()
            .filter(|m| names.contains(&m.file_name))
            .collect();
        Ok(PackContents { managed, additions })
    }

    /// Turn an optional pack mod on or off. It applies on the next sync (Play or Update),
    /// offline too, from `pack.json` and the cache.
    pub async fn pack_set_optional(
        &self,
        id: &str,
        path: &str,
        enabled: bool,
    ) -> Result<Instance> {
        let mut instance = self.pack_instance(id).await?;
        let installed = lockfile::load_pack(&self.instances.perland_dir(id)).await;
        let optional = installed.as_ref().is_some_and(|pack| {
            pack.files
                .iter()
                .any(|f| f.path == path && f.optional.is_some() && f.side.on_client())
        });
        if !optional {
            return Err(LauncherError::disk(
                ErrorCode::ModNotFound,
                format!("{path} isn't an optional mod of this pack"),
            )
            .with_param("file", path));
        }
        instance.optional_choices.insert(path.to_string(), enabled);
        self.instances.update(&instance).await
    }

    /// Mods that came with the pack are the pack's: the player can't turn them off or remove
    /// them (sync would only put them back).
    pub(crate) async fn refuse_managed(&self, id: &str, file: &str) -> Result<()> {
        let base = file.strip_suffix(".disabled").unwrap_or(file);
        let managed = Managed::load(&self.instances.perland_dir(id)).await;
        if managed.is_some_and(|m| m.files.contains_key(&format!("mods/{base}"))) {
            return Err(LauncherError::disk(
                ErrorCode::ModManaged,
                format!("{base} comes with the pack"),
            )
            .with_param("file", base)
            .with_fix("Mods you add yourself can be turned off or removed"));
        }
        Ok(())
    }
```

In `app.rs`:
- In `mod_set_enabled` and `mod_remove`, add `self.refuse_managed(id, file).await?;` right after the busy check.
- Add at module level:

```rust
/// The names a Java executable has. Anything else is never run.
const JAVA_NAMES: [&str; 4] = ["java", "javaw", "java.exe", "javaw.exe"];
```

- Replace `java_test`:

```rust
    /// Run `<path> -version`, but only for a regular file named like Java: the path comes
    /// from the webview.
    pub async fn java_test(&self, path: PathBuf) -> Result<JavaTest> {
        let named_java = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| JAVA_NAMES.iter().any(|j| n.eq_ignore_ascii_case(j)));
        let is_file = tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file());
        if !named_java || !is_file {
            return Err(LauncherError::java(
                ErrorCode::JavaTestFailed,
                "That doesn't look like a working Java",
            )
            .with_fix("Pick the java executable inside a Java installation's bin folder")
            .with_detail(path.display()));
        }
        Ok(JavaTest {
            major: java::probe(&path).await?,
        })
    }
```

`mods_add` already refuses anything but a regular `.jar` (in `mods::add`). The test pins that; no code change is needed.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p launcher-app`
Expected: PASS. The existing `java_test_reports_failure` (a missing path) still gets `JavaTestFailed`.

- [ ] **Step 5: Commit**

```bash
git add crates/launcher-app
git commit -m "feat(app): pack contents and optional choices; guard pack mods and webview paths

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Manual downloads, pack art and `perland://` links

**Files:**
- Modify: `crates/launcher-app/src/packs.rs`
- Modify: `crates/launcher-app/src/app.rs` (`pending_link`, `folder(Downloads)`)
- Create: `crates/launcher-app/src/deep_link.rs`
- Modify: `crates/launcher-app/Cargo.toml`: add `url = "2"` and `base64 = "0.22"` under `[dependencies]`.
- Modify: `crates/launcher-app/src/lib.rs`: add `pub mod deep_link;`.

**Interfaces:**
- Produces:
  - `AppCore::manual_import(&self, id) -> Result<ManualProgress>`.
  - `AppCore::pack_art(&self, id) -> Result<PackArtData>`.
  - `AppCore::deep_link(&self, raw: &str)`: never fails. A good link is stored and emits `AppEvent::DeepLink`; a bad one emits a `pack_link_invalid` warning.
  - `AppCore::deep_link_take(&self) -> Option<String>`.
  - `launcher_app::deep_link::parse(raw: &str) -> Result<String>`: the https `pack.toml` URL.
- Changes `folder(OpenTarget::Downloads)` to return the player's Downloads folder.

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-app/src/deep_link.rs` with the module doc, the imports and the tests:

```rust
//! `perland://pack?url=<https pack.toml>` (core spec §5). The link is remote input: only an
//! https packwiz link gets through, and the player still clicks Add pack before anything
//! downloads.
use launcher_core::{sources::PackInput, ErrorCode, LauncherError, Result};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_links_accept_only_https_pack_links() {
        let asalith = "https://perlanddev.github.io/asalith-modpack/pack.toml";
        for good in [
            "perland://pack?url=https%3A%2F%2Fperlanddev.github.io%2Fasalith-modpack%2Fpack.toml",
            "perland://pack/?url=https://perlanddev.github.io/asalith-modpack/pack.toml",
            " perland://pack?url=https://perlanddev.github.io/asalith-modpack/pack.toml ",
        ] {
            assert_eq!(parse(good).unwrap(), asalith, "{good}");
        }
        for bad in [
            "perland://pack?url=http://x.example/pack.toml",
            "perland://pack?url=http://localhost:8080/pack.toml",
            "perland://pack?url=C:%5CUsers%5Cme%5Cevil.mrpack",
            "perland://pack?url=modrinth:sky",
            "perland://pack?url=https://a.example/pack.toml&url=https://b.example/pack.toml",
            "perland://pack",
            "perland://join?url=https://a.example/pack.toml",
            "perland://pack/extra?url=https://a.example/pack.toml",
            "https://perlanddev.github.io/asalith-modpack/pack.toml",
            "not a link",
        ] {
            assert_eq!(
                parse(bad).unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad}"
            );
        }
    }
}
```

In `packs.rs` `mod tests`, add imports `crate::{app::ManualState, dto::{OpenTarget, PackArtData}, events::AppEvent, testing::manual_file}` and `launcher_core::sync::manual::Scanner`:

```rust
    #[tokio::test]
    async fn manual_import_finds_downloads_and_imports_each_once() {
        let (core, runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        assert_eq!(core.manual_import(&id).await.unwrap().total, 0, "nothing pending");
        let (a, b) = (manual_file("a.jar", b"aaa"), manual_file("b.jar", b"bbb"));
        core.manual.lock().await.insert(
            id.clone(),
            ManualState {
                total: 2,
                remaining: vec![a, b],
                scanner: Scanner::default(),
            },
        );
        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let progress = core.manual_import(&id).await.unwrap();
        assert_eq!((progress.found, progress.total), (0, 2));
        std::fs::write(downloads.join("a (1).jar"), b"aaa").unwrap();
        let progress = core.manual_import(&id).await.unwrap();
        assert_eq!((progress.found, progress.remaining.len()), (1, 1));
        core.manual_import(&id).await.unwrap();
        assert_eq!(
            runner.imported.lock().unwrap().len(),
            1,
            "a found file is imported once"
        );
        std::fs::write(downloads.join("b.jar"), b"bbb").unwrap();
        let progress = core.manual_import(&id).await.unwrap();
        assert_eq!((progress.found, progress.total), (2, 2));
        assert!(progress.remaining.is_empty());
        assert_eq!(core.folder(OpenTarget::Downloads).await.unwrap(), downloads);
    }

    #[tokio::test]
    async fn pack_art_comes_back_as_data_urls_of_real_images_only() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        assert_eq!(
            core.pack_art(&id).await.unwrap(),
            PackArtData {
                icon: None,
                banner: None
            }
        );
        let art = core.instances.perland_dir(&id).join("art");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("icon.png"), b"\x89PNG\r\n\x1a\nrest").unwrap();
        std::fs::write(art.join("banner.png"), b"<html>not a png</html>").unwrap();
        std::fs::write(
            art.join("banner.svg"),
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .unwrap();
        let data = core.pack_art(&id).await.unwrap();
        assert!(data
            .icon
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(data
            .banner
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
    }

    #[tokio::test]
    async fn a_good_deep_link_waits_for_the_ui() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        core.deep_link("perland://pack?url=https%3A%2F%2Fx.example%2Fpack.toml");
        assert!(host
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, AppEvent::DeepLink)));
        assert_eq!(
            core.deep_link_take().as_deref(),
            Some("https://x.example/pack.toml")
        );
        assert_eq!(core.deep_link_take(), None, "taken once");
    }

    #[tokio::test]
    async fn a_bad_deep_link_warns_and_keeps_nothing() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        core.deep_link("perland://pack?url=http%3A%2F%2Fx.example%2Fpack.toml");
        assert_eq!(host.warnings(), vec![ErrorCode::PackLinkInvalid]);
        assert_eq!(core.deep_link_take(), None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p launcher-app --lib`
Expected: compile errors (`parse`, `manual_import`, `pack_art` and `deep_link` don't exist).

- [ ] **Step 3: Implement the link parser**

In `deep_link.rs`, above the tests:

```rust
fn invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

/// The https `pack.toml` URL inside a `perland://pack?url=…` link.
pub fn parse(raw: &str) -> Result<String> {
    let link = url::Url::parse(raw.trim()).map_err(|_| invalid())?;
    if link.scheme() != "perland"
        || link.host_str() != Some("pack")
        || !matches!(link.path(), "" | "/")
    {
        return Err(invalid());
    }
    let mut urls = link
        .query_pairs()
        .filter(|(key, _)| key == "url")
        .map(|(_, value)| value.into_owned());
    let (Some(pack), None) = (urls.next(), urls.next()) else {
        return Err(invalid());
    };
    // Stricter than a pasted link: a web page never gets loopback http.
    if !pack.starts_with("https://") {
        return Err(invalid());
    }
    match PackInput::parse(&pack).map_err(|_| invalid())? {
        PackInput::Packwiz { url } => Ok(url),
        _ => Err(invalid()),
    }
}
```

- [ ] **Step 4: Implement manual import, art and the pending link**

In `app.rs`, add `pub(crate) pending_link: Mutex<Option<String>>` (the std `Mutex` already imported) to `AppCore`, initialized `Mutex::new(None)`. Replace the `Downloads` arm in `folder`:

```rust
            OpenTarget::Downloads => self.config.downloads_dir.clone().ok_or_else(|| {
                LauncherError::disk(ErrorCode::Internal, "Couldn't find your Downloads folder")
            })?,
```

In `packs.rs`, add imports `std::path::Path`, `base64::{engine::general_purpose::STANDARD, Engine}`, `launcher_core::sources::art::{sniff, MAX_ART_BYTES}` and `crate::{deep_link, dto::{ManualProgress, PackArtData}, events::AppEvent}`, then:

```rust
/// Each art file type and the media type the webview needs.
const ART: [(&str, &str); 4] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];

/// `<dir>/<stem>.<ext>` as a `data:` URL, if it is a real image of that type within the size
/// limit. The core wrote it, but the disk is the player's, so it is checked again here.
async fn art_url(dir: &Path, stem: &str) -> Option<String> {
    for (ext, mime) in ART {
        let path = dir.join(format!("{stem}.{ext}"));
        let Ok(meta) = tokio::fs::metadata(&path).await else {
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_ART_BYTES as u64 {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(&path).await else {
            continue;
        };
        if sniff(&bytes) == Some(ext) {
            return Some(format!("data:{mime};base64,{}", STANDARD.encode(&bytes)));
        }
    }
    None
}
```

And in the `impl` block:

```rust
    /// Look in the Downloads folder for the files the last sync couldn't download, and put
    /// what's there into the cache. The "Download these manually" dialog polls this.
    pub async fn manual_import(&self, id: &str) -> Result<ManualProgress> {
        self.instances.load(id).await?;
        let mut manual = self.manual.lock().await;
        let Some(state) = manual.get_mut(id) else {
            return Ok(ManualProgress {
                found: 0,
                total: 0,
                remaining: Vec::new(),
            });
        };
        if let Some(dir) = &self.config.downloads_dir {
            let found = state.scanner.scan(dir, &state.remaining).await?;
            if !found.is_empty() {
                self.runner.import_manual(found.clone()).await?;
                state
                    .remaining
                    .retain(|f| !found.iter().any(|(got, _)| got.path == f.path));
            }
        }
        Ok(ManualProgress {
            found: state.total - state.remaining.len(),
            total: state.total,
            remaining: state.remaining.clone(),
        })
    }

    /// The pack's icon and banner from `.perland/art/`.
    pub async fn pack_art(&self, id: &str) -> Result<PackArtData> {
        self.instances.load(id).await?;
        let dir = self.instances.perland_dir(id).join("art");
        Ok(PackArtData {
            icon: art_url(&dir, "icon").await,
            banner: art_url(&dir, "banner").await,
        })
    }

    /// A `perland://` link arrived (at startup or from a second launch). A good one waits for
    /// the UI (`deep_link_take`); a bad one is only a warning.
    pub fn deep_link(&self, raw: &str) {
        match deep_link::parse(raw) {
            Ok(url) => {
                *self.pending_link.lock().unwrap_or_else(|e| e.into_inner()) = Some(url);
                self.host.emit(AppEvent::DeepLink);
            }
            Err(error) => self.host.emit(AppEvent::Warning {
                instance: None,
                error,
            }),
        }
    }

    /// The waiting pack link, once.
    pub fn deep_link_take(&self) -> Option<String> {
        self.pending_link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
```

- [ ] **Step 5: Run every app test**

Run: `cargo test -p launcher-app`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/launcher-app Cargo.lock
git commit -m "feat(app): manual-download import, pack art as data URLs, and perland://pack links

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
### Task 12: Tauri wiring: pack commands, `perland://` and the opener allow-list

`src-tauri` has no tests (`test = false`). This task is verified by building, clippy, and the owner's smoke test (Task 22).

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`, `src-tauri/src/commands.rs`
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes the `AppCore` methods from Tasks 7–11.
- Produces these Tauri commands (JS argument names in parentheses):
  - `pack_add(request)`
  - `play_answer(id, update)`
  - `pack_update(id)`
  - `pack_status(id)`
  - `pack_contents(id)`
  - `pack_set_optional(id, path, enabled)`
  - `manual_import(id)`
  - `pack_art(id)`
  - `deep_link_take()`
- The scheme `perland` is registered. A link that starts the app, or arrives in a second launch, reaches `core.deep_link(url)`.

- [ ] **Step 1: Dependencies and config**

In `src-tauri/Cargo.toml`:

```toml
tauri-plugin-deep-link = "2.6.1"
tauri-plugin-single-instance = { version = "2.5.2", features = ["deep-link"] }
```

In `src-tauri/tauri.conf.json`, add at top level, next to `"bundle"`:

```json
  "plugins": {
    "deep-link": { "desktop": { "schemes": ["perland"] } }
  },
```

In `src-tauri/capabilities/default.json`, extend the opener allow-list (CurseForge pages for manual downloads):

```json
      "allow": [
        { "url": "https://github.com/PerlandDev/*" },
        { "url": "https://www.gnu.org/licenses/*" },
        { "url": "https://www.curseforge.com/projects/*" }
      ]
```

- [ ] **Step 2: Commands**

Append to `src-tauri/src/commands.rs`. Import `launcher_app::dto::{ManualProgress, PackAddRequest, PackArtData, PackContents, PackStatus}`.

```rust
#[tauri::command]
pub async fn pack_add(core: CoreState<'_>, request: PackAddRequest) -> Res<Instance> {
    core.pack_add(request).await
}

#[tauri::command]
pub fn play_answer(core: CoreState<'_>, id: String, update: bool) -> Res<()> {
    core.play_answer(&id, update)
}

#[tauri::command]
pub async fn pack_update(core: CoreState<'_>, id: String) -> Res<()> {
    core.inner().pack_update(&id).await
}

#[tauri::command]
pub async fn pack_status(core: CoreState<'_>, id: String) -> Res<PackStatus> {
    core.pack_status(&id).await
}

#[tauri::command]
pub async fn pack_contents(core: CoreState<'_>, id: String) -> Res<PackContents> {
    core.pack_contents(&id).await
}

#[tauri::command]
pub async fn pack_set_optional(
    core: CoreState<'_>,
    id: String,
    path: String,
    enabled: bool,
) -> Res<Instance> {
    core.pack_set_optional(&id, &path, enabled).await
}

#[tauri::command]
pub async fn manual_import(core: CoreState<'_>, id: String) -> Res<ManualProgress> {
    core.manual_import(&id).await
}

#[tauri::command]
pub async fn pack_art(core: CoreState<'_>, id: String) -> Res<PackArtData> {
    core.pack_art(&id).await
}

#[tauri::command]
pub fn deep_link_take(core: CoreState<'_>) -> Option<String> {
    core.deep_link_take()
}
```

- [ ] **Step 3: Plugins and the link handler**

In `src-tauri/src/main.rs`:
- Add `use tauri_plugin_deep_link::DeepLinkExt;`.
- After `.plugin(tauri_plugin_single_instance::init(…))`, which must stay first, add `.plugin(tauri_plugin_deep_link::init())`.
- Replace `.setup(…)` with:

```rust
        .setup(|app| {
            let core = build_core(app.handle().clone())?;
            app.manage(core.clone());
            // Installers register the scheme; a dev run registers itself (per user, no admin).
            #[cfg(all(debug_assertions, any(windows, target_os = "linux")))]
            if let Err(e) = app.deep_link().register_all() {
                eprintln!("couldn't register perland:// links: {e}");
            }
            // Started by a link…
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    core.deep_link(url.as_str());
                }
            }
            // …or handed one while running (a second launch, through single-instance).
            let linked = core.clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    linked.deep_link(url.as_str());
                }
            });
            Ok(())
        })
```

Add the nine commands to `generate_handler!`, after `commands::java_test`.

The single-instance callback already brings the window forward. With the `deep-link` feature, a second launch's URL is delivered to `on_open_url` in the running app.

- [ ] **Step 4: Build and lint**

Run: `cargo build -p perland-launcher && cargo clippy --workspace --all-targets -- -D warnings`
Expected: OK, no warnings.

- [ ] **Step 5: Commit**

```bash
git add src-tauri Cargo.lock
git commit -m "feat(tauri): pack commands, perland:// deep links, CurseForge pages in the opener allow-list

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: UI state: commands, run view, deep links, query refresh

Task 6 already updated `schema.ts` and the contract tests. This task makes the rest of the UI's state understand packs.

**Files:**
- Modify: `ui/src/ipc/commands.ts`, `ui/src/state/runStore.ts`, `ui/src/state/handleEvent.ts`, `ui/src/state/exit.ts`, `ui/src/state/route.ts`, `ui/src/state/queries.ts`, `ui/src/App.tsx`
- Create: `ui/src/state/deepLink.ts`
- Modify: `ui/src/features/home/labels.ts` (`isPack`)
- Test: `ui/src/state/runStore.test.ts`, `ui/src/state/handleEvent.test.ts`, `ui/src/state/exit.test.ts`

**Interfaces:**
- Consumes the Task 6 schemas and the Task 12 command names.
- Produces:
  - **`commands`:** `packAdd`, `playAnswer`, `packUpdate`, `packStatus`, `packContents`, `packSetOptional`, `manualImport`, `packArt`, `deepLinkTake`.
  - **`RunView.update`:** `PackUpdate | null`. Set only while `asking`.
  - **Query keys:** `qk.packStatus(id)`, `qk.packContents(id)`, `qk.packArt(id)`, `qk.manual(id)`.
  - **Hooks:** `usePackStatus(id, enabled)`, `usePackContents(id, enabled)`, `usePackArt(id, enabled)`, `useAddPack(opts)`, `usePackOptional(id, opts)`.
  - **Route:** `{ name: 'newInstance'; link?: string }`.
  - **`takeDeepLink(): Promise<void>`** in `state/deepLink.ts`.
  - **`isPack(instance)`** in `features/home/labels.ts`.

- [ ] **Step 1: Write the failing tests**

Add to `ui/src/state/runStore.test.ts`:

```ts
  it('keeps the offer only while asking and starts a fresh run from checking', () => {
    const store = createRunStore();
    const offer = {
      fromVersion: '1.0.3', toVersion: '1.0.4', added: ['JEI'], updated: [], removed: [], kept: [],
    };
    store.apply({ type: 'log', instance: 'pack', lines: ['old'] });
    store.apply({ type: 'run_state', instance: 'pack', state: 'checking', exit: null, update: null });
    expect(store.log('pack')).toEqual([]);
    store.apply({ type: 'run_state', instance: 'pack', state: 'asking', exit: null, update: offer });
    expect(store.run('pack').update).toEqual(offer);
    store.apply({ type: 'run_state', instance: 'pack', state: 'queued', exit: null, update: null });
    expect(store.run('pack').update).toBeNull();
    store.apply({ type: 'run_state', instance: 'pack', state: 'syncing', exit: null, update: null });
    store.apply(progress(1));
    expect(store.run('pack').progress?.done).toBe(1);
    store.apply({ type: 'run_state', instance: 'pack', state: 'preparing', exit: null, update: null });
    expect(store.run('pack').progress).toBeNull();
    expect(store.log('pack')).toEqual([], 'still the same run');
    store.hydrate([{ instance: 'other', state: 'asking', update: offer }], {});
    expect(store.run('other').update).toEqual(offer);
  });
```

Add to `ui/src/state/handleEvent.test.ts`, with imports `mockIPC` from `@tauri-apps/api/mocks`, `vi`, `exitOf` from `../ipc/mock`, `qk, queryClient` from `./queries`, `routeSnapshot` from `./route`:

```ts
it('an ended run refreshes the instance and its pack queries', () => {
  const spy = vi.spyOn(queryClient, 'invalidateQueries');
  handleEvent({ type: 'run_state', instance: 'sky', state: 'idle', exit: exitOf({ code: 0 }), update: null });
  const keys = spy.mock.calls.map(([filters]) => filters?.queryKey);
  expect(keys).toContainEqual(qk.instances);
  expect(keys).toContainEqual(qk.packStatus('sky'));
  expect(keys).toContainEqual(qk.packContents('sky'));
  expect(keys).toContainEqual(qk.packArt('sky'));
  spy.mockRestore();
});

it('leaves the drawer closed when files need a manual download', () => {
  resetActivity();
  const error = { category: 'pack', code: 'manual_downloads_needed', message: 'x', fix: null, detail: null, params: { count: '1' } } as const;
  const manual = [{ path: 'mods/a.jar', name: 'A', fileName: 'a.jar', pageUrl: 'https://www.curseforge.com/projects/1',
    hashes: { sha1: 'a', sha256: null, sha512: null, md5: null, murmur2: null } }];
  handleEvent({ type: 'run_state', instance: 'sky', state: 'idle', exit: exitOf({ error, manual }), update: null });
  const { result } = renderHook(() => useActivity());
  expect(result.current.open).toBe(false);
});

it('opens New instance with a deep link once setup is done', async () => {
  mockIPC((cmd) => (cmd === 'deep_link_take' ? 'https://x.example/pack.toml' : null));
  queryClient.setQueryData(qk.settings, { version: 1, language: null, defaultMemoryMb: 4096, minimizeOnPlay: true, firstRunDone: false });
  handleEvent({ type: 'deep_link' });
  await new Promise((r) => setTimeout(r, 10));
  expect(routeSnapshot()).toEqual({ name: 'home', id: null }, 'first run: the link waits');
  queryClient.setQueryData(qk.settings, { version: 1, language: null, defaultMemoryMb: 4096, minimizeOnPlay: true, firstRunDone: true });
  handleEvent({ type: 'deep_link' });
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'newInstance', link: 'https://x.example/pack.toml' }));
  queryClient.clear();
});
```

The existing `handleEvent` tests build `exit` objects inline and need `update: null` plus the new exit fields. Wrap their exits in `exitOf({…})`; Task 6 already did this if the typecheck required it.

In `ui/src/state/exit.test.ts`, add:

```ts
it('never calls an update-only run a crash', () => {
  expect(isCrash(exitOf({ updateOnly: true }))).toBe(false);
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test -- state`
Expected: FAIL. `qk.packStatus` is missing, `update` isn't stored, the log isn't reset on `checking`, and deep links do nothing.

- [ ] **Step 3: Implement**

`ui/src/ipc/commands.ts`: import the new types and add:

```ts
  packAdd: (request: PackAddRequest) => invoke<Instance>('pack_add', { request }),
  playAnswer: (id: string, update: boolean) => invoke<null>('play_answer', { id, update }),
  packUpdate: (id: string) => invoke<null>('pack_update', { id }),
  packStatus: (id: string) => invoke<PackStatus>('pack_status', { id }),
  packContents: (id: string) => invoke<PackContents>('pack_contents', { id }),
  packSetOptional: (id: string, path: string, enabled: boolean) =>
    invoke<Instance>('pack_set_optional', { id, path, enabled }),
  manualImport: (id: string) => invoke<ManualProgress>('manual_import', { id }),
  packArt: (id: string) => invoke<PackArtData>('pack_art', { id }),
  deepLinkTake: () => invoke<string | null>('deep_link_take'),
```

`ui/src/state/runStore.ts`:
- Add `update: PackUpdate | null` to `RunView` and `update: null` to `IDLE`.
- Replace the `run_state` case:

```ts
        case 'run_state': {
          const { instance: id, state, exit, update } = event;
          heardLive.add(id);
          // A run starts when an idle instance reports anything else: checking or queued.
          const starting = (runs.get(id)?.state ?? 'idle') === 'idle' && state !== 'idle';
          if (starting) setLog(id, NO_LINES);
          setRun(id, (run) => ({
            state,
            progress: state === run.state ? run.progress : null,
            lastExit: starting ? null : (exit ?? run.lastExit),
            update: state === 'asking' ? (update ?? run.update) : null,
          }));
          break;
        }
```

- In `hydrate`, set `update: snapshot.update` along with the state.

`ui/src/state/exit.ts`: `isCrash` also requires `!exit.updateOnly`.

`ui/src/state/route.ts`: change the newInstance member to `| { name: 'newInstance'; link?: string }`.

`ui/src/state/deepLink.ts`:

```ts
import { commands } from '../ipc/commands';
import { navigate } from './route';
import { pushError } from './toasts';

/** Open New instance with the waiting `perland://` pack link, if there is one. Nothing is
 *  added until the player clicks Add pack. */
export async function takeDeepLink(): Promise<void> {
  try {
    const link = await commands.deepLinkTake();
    if (link) navigate({ name: 'newInstance', link });
  } catch (error) {
    pushError(error);
  }
}
```

`ui/src/state/handleEvent.ts`:

```ts
import type { AppEvent, Settings } from '../ipc/schema';
import { openActivity } from './activity';
import { takeDeepLink } from './deepLink';
import { isCrash } from './exit';
import { qk, queryClient } from './queries';
import { runStore } from './runStore';
import { pushError } from './toasts';

/** Everything the UI does with an event from Rust. */
export function handleEvent(event: AppEvent) {
  runStore.apply(event);
  if (event.type === 'warning') pushError(event.error);
  // During first-run setup the link waits in Rust; the shell takes it once setup is done.
  if (event.type === 'deep_link' && queryClient.getQueryData<Settings>(qk.settings)?.firstRunDone) {
    void takeDeepLink();
  }
  if (event.type === 'run_state' && event.state === 'idle') {
    // A sync can change the game version, the pack's mods and its art.
    const id = event.instance;
    for (const queryKey of [qk.instances, qk.mods(id), qk.packStatus(id), qk.packContents(id), qk.packArt(id)]) {
      void queryClient.invalidateQueries({ queryKey });
    }
    const exit = event.exit;
    // Blocked files get their own dialog, not the activity drawer.
    if (exit && exit.manual.length === 0 && (exit.error || isCrash(exit))) openActivity(id);
  }
}
```

`ui/src/state/queries.ts`:
- Add the keys `packStatus: (id: string) => ['pack', 'status', id] as const`, and the same shape for `packContents`, `packArt` and `manual`.
- Add the hooks:

```ts
export const usePackStatus = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packStatus(id), queryFn: () => commands.packStatus(id), enabled, staleTime: TEN_MINUTES });
export const usePackContents = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packContents(id), queryFn: () => commands.packContents(id), enabled });
export const usePackArt = (id: string, enabled: boolean) =>
  useQuery({ queryKey: qk.packArt(id), queryFn: () => commands.packArt(id), enabled, staleTime: Infinity });

export function useAddPack(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: PackAddRequest) => commands.packAdd(request),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function usePackOptional(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ path, enabled }: { path: string; enabled: boolean }) => commands.packSetOptional(id, path, enabled),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.packContents(id) });
      void qc.invalidateQueries({ queryKey: qk.instances });
    },
    onError: onError(opts),
  });
}
```

- In `useModMutations`, `onSuccess` also does `void qc.invalidateQueries({ queryKey: qk.packContents(id) })`, so a pack instance's additions list follows.

`ui/src/features/home/labels.ts`: `export const isPack = (instance: Instance) => instance.source.kind !== 'none';`

`ui/src/App.tsx`:
- In `Shell`, add `useEffect(() => void takeDeepLink(), []);`. It runs once the shell exists, after first run, and picks up a link that started the app.
- In `Screen`, render `<NewInstance key={route.link ?? 'new'} link={route.link} />`. `NewInstance` accepts the prop in Task 17; until then add `link?: string` to its props and ignore it.

- [ ] **Step 4: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): pack commands, run offers, deep links and refreshing pack queries after a run

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 14: The mock backend learns packs (and Forge versions)

`pnpm --filter ui dev:mock` and every UI test run against `ui/src/ipc/mock.ts`, so it needs a pack instance that behaves like the real one.

**Files:**
- Modify: `ui/src/ipc/mock.ts`, `ui/src/ipc/mock.test.ts`, `ui/src/main.tsx` (URL flags)
- Modify: `ui/src/features/new-instance/NewInstance.test.tsx` (Forge now gets Forge versions)

**Interfaces:**
- `MockOptions` gains `manual?: boolean` (the pack's first sync ends with two blocked files), `noUpdate?: boolean` (no update on offer) and `link?: string` (a pending deep link).
- The seed gains a pack instance, listed after the fixtures:
  - id `sky-islands`, named "Sky Islands"
  - packwiz source `https://packs.example/sky-islands/pack.toml`
  - Forge 1.20.1 / 47.4.22
  - policy `ask`, installed 1.0.3
  - the update from `pack_status.json[0]`, the contents from `pack_contents.json`, and generated SVG art
- `main.tsx` reads `?manual`, `?noUpdate` and `?link=<url>`.

- [ ] **Step 1: Write the failing tests**

In `ui/src/ipc/mock.test.ts`:
- Change the first test's ids to `['asalith-fields', 'creative-flat', 'sky-islands']`.
- Add:

```ts
  it('scripts a pack run that asks, updates and plays', async () => {
    const backend = installMock();
    await backend.handle('play', { id: 'sky-islands' });
    await vi.waitFor(() => expect(backend.state.runs.get('sky-islands')?.state).toBe('asking'));
    await backend.handle('play_answer', { id: 'sky-islands', update: true });
    await vi.waitFor(() => expect(backend.state.runs.get('sky-islands')?.state).toBe('running'), { timeout: 5000 });
    const status = (await backend.handle('pack_status', { id: 'sky-islands' })) as PackStatus;
    expect(status).toMatchObject({ installedVersion: '1.0.4', update: null });
    stopMockRuns();
  });

  it('finds manual downloads one poll at a time', async () => {
    const { handle } = createMockBackend({ manual: true });
    const first = (await handle('manual_import', { id: 'sky-islands' })) as ManualProgress;
    const second = (await handle('manual_import', { id: 'sky-islands' })) as ManualProgress;
    expect([first.found, second.found, second.total]).toEqual([1, 2, 2]);
    expect(second.remaining).toEqual([]);
  });

  it('serves Forge versions for Forge', async () => {
    const { handle } = createMockBackend();
    const forge = (await handle('catalog_loaders', { kind: 'forge', mc: '1.20.1' })) as LoaderVersion[];
    expect(forge[0]?.version).toBe('47.4.22');
  });

  it('refuses links that are not https or modrinth', async () => {
    const { handle } = createMockBackend();
    await expect(handle('pack_add', { request: { kind: 'link', url: 'http://evil.example/pack.toml' } })).rejects.toMatchObject({
      code: 'pack_link_invalid',
    });
  });
```

Imports: `installMock`, `vi`, and the types `ManualProgress`, `PackStatus`, `LoaderVersion`.

In `NewInstance.test.tsx`, "offers Forge and NeoForge and creates a Forge instance": the loader version is now `'47.4.22'`. Update both the `toHaveValue` and the expected `instance_create` request.

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test -- mock NewInstance`
Expected: FAIL (no `sky-islands`, and no pack commands).

- [ ] **Step 3: Implement**

In `ui/src/ipc/mock.ts`:

1. Imports: the contract fixtures `packStatusFx` (`./contract/pack_status.json`), `packContentsFx` (`./contract/pack_contents.json`) and `manualProgressFx` (`./contract/manual_progress.json`), plus the new types.

2. Mock run and pack state:

```ts
interface MockRun {
  state: RunStateKind;
  cancelled: boolean;
  /** Set while asking: resolves with the player's answer. */
  answer: ((update: boolean) => void) | null;
}

interface MockPack {
  installed: string | null;
  update: PackUpdate | null;
  contents: PackContents;
  art: PackArtData;
  manual: ManualFile[];
  found: number;
}

const svg = (body: string) => `data:image/svg+xml;base64,${btoa(body)}`;
const SKY_ART: PackArtData = {
  icon: svg(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" shape-rendering="crispEdges"><rect width="16" height="16" fill="#7cc4ff"/><rect y="9" width="16" height="7" fill="#3f8f4f"/><rect y="12" width="16" height="4" fill="#6b4a2b"/></svg>',
  ),
  banner: svg(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 270" shape-rendering="crispEdges"><defs><linearGradient id="s" x2="0" y2="1"><stop offset="0" stop-color="#9fd8ff"/><stop offset="1" stop-color="#e9c46a"/></linearGradient></defs><rect width="480" height="270" fill="url(#s)"/><rect x="60" y="120" width="140" height="40" fill="#3f8f4f"/><rect x="80" y="160" width="100" height="24" fill="#6b4a2b"/><rect x="290" y="90" width="120" height="36" fill="#3f8f4f"/><rect x="306" y="126" width="88" height="20" fill="#6b4a2b"/></svg>',
  ),
};

export const exitOf = (partial: Partial<ExitInfo>): ExitInfo => ({
  code: null, killed: false, cancelled: false, crash: null, error: null,
  synced: null, manual: [], updateOnly: false, ...partial,
});
```

If Task 6 already added `exitOf`, keep it.

3. `MockOptions` gains `manual?: boolean; noUpdate?: boolean; link?: string;`. In `seed`, when not `firstRun`, append the pack instance and its state:

```ts
  const sky: Instance = {
    id: 'sky-islands',
    name: 'Sky Islands',
    source: { kind: 'packwiz', url: 'https://packs.example/sky-islands/pack.toml' },
    game: { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.4.22' } },
    updatePolicy: 'ask',
    optionalChoices: {},
    java: { path: null, memoryMb: 6144 },
  };
  const packs = new Map<string, MockPack>();
  if (!options.firstRun) {
    listing.instances = [...listing.instances, sky];
    const offer = (clone(packStatusFx) as PackStatus[])[0]?.update ?? null;
    const blocked = (clone(manualProgressFx) as ManualProgress).remaining;
    packs.set(sky.id, {
      installed: '1.0.3',
      update: options.noUpdate ? null : offer,
      contents: clone(packContentsFx) as PackContents,
      art: SKY_ART,
      manual: options.manual ? [...blocked, { ...blocked[0]!, path: 'mods/sodium-extra.jar', name: 'Sodium Extra', fileName: 'sodium-extra-0.5.4.jar', pageUrl: 'https://www.curseforge.com/projects/447673' }] : [],
      found: 0,
    });
  }
```

   Return `packs` and `pendingLink: options.link ?? null` in the state object as well. With the `manual` option the pack's first sync after any Play ends with those two files, until `manual_import` has found both.

4. `event()` callers: every `run_state` literal gets `update: null`, and `exit` objects go through `exitOf`. `setState` sends `update: null`.

5. The pack prelude, which runs before the existing preparing/running script:

```ts
/** Check, ask, sync, like the real run. Returns the sync summary, or null when the run was
 *  stopped or ended early (update only, or blocked files). */
async function packPrelude(
  state: MockState,
  instance: Instance,
  run: MockRun,
  updateOnly: boolean,
): Promise<{ synced: SyncSummary | null } | null> {
  const id = instance.id;
  const pack = state.packs.get(id);
  if (!pack) return { synced: null };
  const checks = pack.installed !== null && instance.updatePolicy !== 'pinned';
  let take = false;
  if (checks) {
    await wait(400);
    if (run.cancelled) return null;
    if (pack.update) {
      if (updateOnly || instance.updatePolicy === 'auto') take = true;
      else {
        run.state = 'asking';
        event({ type: 'run_state', instance: id, state: 'asking', exit: null, update: pack.update });
        take = await new Promise<boolean>((resolve) => (run.answer = resolve));
        if (run.cancelled) return null;
      }
    }
    setState(state, id, 'queued');
  }
  await wait(200);
  if (run.cancelled) return null;
  setState(state, id, 'syncing');
  const total = 24;
  for (let done = 0; done <= total; done++) {
    if (run.cancelled) return null;
    event({
      type: 'progress',
      instance: id,
      task: { key: pack.installed ? 'sync_pack' : 'install_pack', params: { name: instance.name } },
      done,
      total,
      bytes: done * 2_400_000,
    });
    await wait(50);
  }
  if (pack.found < pack.manual.length) {
    state.runs.delete(id);
    const error: LauncherError = {
      category: 'pack', code: 'manual_downloads_needed', message: `${pack.manual.length} files have to be downloaded by hand`,
      fix: null, detail: null, params: { count: String(pack.manual.length) },
    };
    event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ error, manual: pack.manual, updateOnly }), update: null });
    return null;
  }
  let synced: SyncSummary | null = null;
  if (take && pack.update) {
    const u = pack.update;
    synced = { fromVersion: u.fromVersion, toVersion: u.toVersion, added: u.added.length, updated: u.updated.length, removed: u.removed.length, keptModified: u.kept };
    pack.installed = u.toVersion;
    pack.update = null;
  } else if (!pack.installed) {
    pack.installed = '1.0.3';
  }
  if (updateOnly) {
    state.runs.delete(id);
    event({ type: 'run_state', instance: id, state: 'idle', exit: exitOf({ synced, updateOnly: true }), update: null });
    return null;
  }
  return { synced };
}
```

6. `scriptPlay(state, instance)` becomes `scriptRun(state, instance, updateOnly = false)`:
   - It creates `run = { state: first, cancelled: false, answer: null }`, where `first` is `'checking'` for a pack that checks and `'queued'` otherwise. It emits `first`, then for a pack awaits `packPrelude` (returning on `null`).
   - For vanilla it keeps the old 300 ms queued wait.
   - It then continues with the existing preparing and running script.
   - The crash exit carries the prelude's `synced`.

7. New command handlers in `handle`:

```ts
    case 'play_answer': {
      const run = state.runs.get(String(args.id));
      if (run?.state === 'asking' && run.answer) {
        run.answer(Boolean(args.update));
        run.answer = null;
      }
      return null;
    }
    case 'pack_update': {
      const target = find(args.id) ?? fail('disk', 'instance_not_found', 'gone');
      if (!state.packs.has(target.id)) fail('pack', 'no_pack', "This instance doesn't come from a pack");
      if (state.runs.has(target.id)) fail('game', 'already_running', 'already running');
      void scriptRun(state, target, true);
      return null;
    }
    case 'pack_status': {
      const pack = state.packs.get(String(args.id)) ?? fail('pack', 'no_pack', "This instance doesn't come from a pack");
      return { installedVersion: pack.installed, update: pack.installed ? pack.update : null, checkError: null } satisfies PackStatus;
    }
    case 'pack_contents':
      return (state.packs.get(String(args.id)) ?? fail('pack', 'no_pack', 'no pack')).contents;
    case 'pack_set_optional': {
      const id = String(args.id);
      const pack = state.packs.get(id) ?? fail('pack', 'no_pack', 'no pack');
      const target = find(id) ?? fail('disk', 'instance_not_found', 'gone');
      const mod = pack.contents.managed.find((m) => m.path === args.path && m.optional);
      if (!mod?.optional) fail('disk', 'mod_not_found', 'not optional', { file: String(args.path) });
      mod.optional.enabled = Boolean(args.enabled);
      const updated: Instance = { ...target, optionalChoices: { ...target.optionalChoices, [String(args.path)]: Boolean(args.enabled) } };
      state.listing.instances = state.listing.instances.map((i) => (i.id === id ? updated : i));
      return updated;
    }
    case 'manual_import': {
      const pack = state.packs.get(String(args.id));
      if (!pack || pack.manual.length === 0) return { found: 0, total: 0, remaining: [] } satisfies ManualProgress;
      pack.found = Math.min(pack.found + 1, pack.manual.length);
      return { found: pack.found, total: pack.manual.length, remaining: pack.manual.slice(pack.found) } satisfies ManualProgress;
    }
    case 'pack_art':
      return state.packs.get(String(args.id))?.art ?? ({ icon: null, banner: null } satisfies PackArtData);
    case 'pack_add': {
      const request = args.request as PackAddRequest;
      if (request.kind === 'link' && !/^(https:\/\/|modrinth:)/.test(request.url.trim())) {
        fail('pack', 'pack_link_invalid', "That doesn't look like a pack link");
      }
      const name = request.kind === 'file' ? (request.path.split(/[\\/]/).pop() ?? 'Pack').replace(/\.mrpack$/i, '') : 'Asalith';
      let id = slug(name);
      for (let n = 2; find(id); n++) id = `${slug(name)}-${n}`;
      const created: Instance = {
        id,
        name,
        source: request.kind === 'file' ? { kind: 'mrpack', name: `${name}.mrpack`, sha1: '0'.repeat(40) } : { kind: 'packwiz', url: request.url.trim() },
        game: { minecraft: '1.20.1', loader: { kind: 'forge', version: '47.4.22' } },
        updatePolicy: 'ask',
        optionalChoices: {},
        java: { path: null, memoryMb: state.settings.defaultMemoryMb },
      };
      state.listing.instances = [...state.listing.instances, created].sort((a, b) => a.name.localeCompare(b.name));
      state.packs.set(id, { installed: null, update: null, contents: { managed: [], additions: [] }, art: SKY_ART, manual: [], found: 0 });
      return created;
    }
    case 'deep_link_take': {
      const link = state.pendingLink;
      state.pendingLink = null;
      return link;
    }
```

`stop` cancels `checking`, `asking`, `queued`, `syncing` and `preparing` alike. It sets `run.cancelled`, calls `run.answer?.(false)`, deletes the run, and emits `idle` with `exitOf({ cancelled: true })`.

`catalog_loaders` answers by kind:

```ts
    case 'catalog_loaders': {
      const kind = String(args.kind);
      if (kind === 'forge') return [{ version: '47.4.22', stable: true }, { version: '47.4.20', stable: true }];
      if (kind === 'neoforge') return [{ version: '21.1.209', stable: true }];
      return loaderVersionsFx;
    }
```

`plugin:dialog|open` returns `'C:/Users/player/Downloads/Sky Islands 1.0.4.mrpack'` when `options.filters?.[0]?.extensions` includes `'mrpack'`.

8. `ui/src/main.tsx`:

```ts
    installMock({
      firstRun: params.has('firstRun'),
      crash: params.has('crash'),
      release: params.has('release'),
      manual: params.has('manual'),
      noUpdate: params.has('noUpdate'),
      link: params.get('link') ?? undefined,
    });
```

- [ ] **Step 4: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS. The Rail tests still pass: "Sky Islands" sorts before the damaged `old-pack`, and the arrow-key test moves from Asalith to Creative Flat.

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): mock backend with a pack instance, scripted ask/sync, manual downloads; Forge mock versions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Play shows checking and syncing; the "Update ready" dialog

**Files:**
- Modify: `ui/src/ui/PlayButton.tsx`, `ui/src/features/rail/Rail.tsx` (status)
- Create: `ui/src/features/home/ChangeSummary.tsx`, `ui/src/features/home/UpdateDialog.tsx`
- Modify: `ui/src/features/home/Home.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/home/Home.test.tsx`

**Interfaces:**
- Produces:
  - `changeLine(t: TFunction, counts: { added: number; updated: number; removed: number }): string`, used again by the banner in Task 16.
  - `<ChangeSummary update={PackUpdate} />` and `<UpdateDialog instance={Instance} update={PackUpdate} />`.

- [ ] **Step 1: Strings**

`en.json`:
- `play`: add `"checking": "Checking for updates…"`, `"asking": "Update ready"`, `"syncing": "Updating…"`.
- Add a new `"update"` section:

```json
  "update": {
    "title": "Update ready: {{from}} → {{to}}",
    "titleNew": "Update ready: {{to}}",
    "updateAndPlay": "Update and play",
    "playOld": "Play {{version}}",
    "playInstalled": "Play what you have",
    "update": "Update",
    "added_one": "+{{count}} added",
    "added_other": "+{{count}} added",
    "updated_one": "{{count}} updated",
    "updated_other": "{{count}} updated",
    "removed_one": "{{count}} removed",
    "removed_other": "{{count}} removed",
    "sameFiles": "Same mods, new version",
    "showNames": "Show what changed",
    "hideNames": "Hide what changed",
    "addedHeading": "Added",
    "updatedHeading": "Updated",
    "removedHeading": "Removed",
    "kept_one": "Kept your version of {{count}} file",
    "kept_other": "Kept your version of {{count}} files",
    "updatedTo": "Updated to {{version}}",
    "checkFailed": "Couldn't check for updates"
  },
```

`es.json`:
- `play`: add `"checking": "Buscando actualizaciones…"`, `"asking": "¡Actualización lista!"`, `"syncing": "Actualizando…"`.
- Add:

```json
  "update": {
    "title": "¡Actualización lista!: {{from}} → {{to}}",
    "titleNew": "¡Actualización lista!: {{to}}",
    "updateAndPlay": "Actualizar y jugar",
    "playOld": "Jugar {{version}}",
    "playInstalled": "Jugar lo que tienes",
    "update": "Actualizar",
    "added_one": "+{{count}} nuevo",
    "added_other": "+{{count}} nuevos",
    "updated_one": "{{count}} actualizado",
    "updated_other": "{{count}} actualizados",
    "removed_one": "{{count}} quitado",
    "removed_other": "{{count}} quitados",
    "sameFiles": "Mismos mods, nueva versión",
    "showNames": "Ver qué cambió",
    "hideNames": "Ocultar qué cambió",
    "addedHeading": "Nuevos",
    "updatedHeading": "Actualizados",
    "removedHeading": "Quitados",
    "kept_one": "Mantuvimos tu versión de {{count}} archivo",
    "kept_other": "Mantuvimos tu versión de {{count}} archivos",
    "updatedTo": "Actualizado a la {{version}}",
    "checkFailed": "No se pudo buscar actualizaciones"
  },
```

- [ ] **Step 2: Write the failing tests**

Add to `Home.test.tsx` (imports: `packStatusFx` from `../../ipc/contract/pack_status.json`, `type PackStatus`):

```ts
  const offer = (packStatusFx as PackStatus[])[0]!.update!;

  it('asks before updating and sends the answer', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'asking', exit: null, update: offer }));
    const dialog = screen.getByRole('dialog', { name: 'Update ready: 1.0.3 → 1.0.4' });
    expect(within(dialog).getByText('+3 added · 2 updated · 1 removed')).toBeInTheDocument();
    expect(within(dialog).getByText('Kept your version of 1 file')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Show what changed' }));
    expect(within(dialog).getByText("Just Enough Items, Create, Farmer's Delight")).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Update and play' }));
    expect(sent(calls, 'play_answer')).toEqual([{ id: 'sky-islands', update: true }]);
  });

  it('plays the installed version, or cancels with Escape', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'asking', exit: null, update: offer }));
    await user.click(screen.getByRole('button', { name: 'Play 1.0.3' }));
    expect(sent(calls, 'play_answer')).toEqual([{ id: 'sky-islands', update: false }]);
    await user.keyboard('{Escape}');
    expect(sent(calls, 'stop')).toEqual([{ id: 'sky-islands' }]);
  });

  it('shows checking and installing on the Play button', async () => {
    renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() => runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'checking', exit: null, update: null }));
    expect(screen.getByRole('button', { name: 'Cancel: Checking for updates…' })).toBeInTheDocument();
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'sky-islands', state: 'syncing', exit: null, update: null });
      runStore.apply({
        type: 'progress', instance: 'sky-islands',
        task: { key: 'install_pack', params: { name: 'Sky Islands' } }, done: 2, total: 24, bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Installing Sky Islands' })).toBeInTheDocument();
  });
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- Home`
Expected: FAIL (no dialog, and Play shows Play during `checking`).

- [ ] **Step 4: Implement**

`ui/src/ui/PlayButton.tsx`: replace the `queued || preparing` branch head:

```tsx
  const waiting = state === 'checking' || state === 'asking' || state === 'queued' || state === 'syncing' || state === 'preparing';
  if (waiting) {
    const label =
      state === 'checking'
        ? t('play.checking')
        : state === 'asking'
          ? t('play.asking')
          : state === 'queued'
            ? t('play.queued')
            : progress
              ? taskLabel(i18n, progress.task)
              : state === 'syncing'
                ? t('play.syncing')
                : t('play.preparing');
    const fraction = progress && progress.total > 0 ? progress.done / progress.total : 0;
    // Syncing and preparing fill the meter; the other waits show it empty.
    const look = state === 'syncing' || state === 'preparing' ? 'preparing' : 'queued';
    return (
      <button type="button" className="play" data-state={look} onClick={onStop} aria-label={t('play.cancelLabel', { label })}>
        {/* body unchanged */}
      </button>
    );
  }
```

`ui/src/features/rail/Rail.tsx`: the "preparing" status covers `checking`, `asking`, `queued`, `syncing` and `preparing`.

`ui/src/features/home/ChangeSummary.tsx`:

```tsx
import type { TFunction } from 'i18next';
import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { PackUpdate } from '../../ipc/schema';

/** "+3 added · 2 updated · 1 removed", or a line for a new version with the same mods. */
export function changeLine(t: TFunction, counts: { added: number; updated: number; removed: number }): string {
  const parts = [
    counts.added > 0 ? t('update.added', { count: counts.added }) : null,
    counts.updated > 0 ? t('update.updated', { count: counts.updated }) : null,
    counts.removed > 0 ? t('update.removed', { count: counts.removed }) : null,
  ].filter((part): part is string => part !== null);
  return parts.length > 0 ? parts.join(' · ') : t('update.sameFiles');
}

const KINDS = ['added', 'updated', 'removed'] as const;

/** The change line, the names behind it on request, and the edits the update keeps. */
export function ChangeSummary({ update }: { update: PackUpdate }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const listId = useId();
  const counts = { added: update.added.length, updated: update.updated.length, removed: update.removed.length };
  const hasNames = counts.added + counts.updated + counts.removed > 0;
  return (
    <div className="grid gap-2">
      <p className="tabular text-ink">{changeLine(t, counts)}</p>
      {hasNames && (
        <button
          type="button"
          className="justify-self-start text-sm text-blue-ink underline underline-offset-4"
          aria-expanded={open}
          aria-controls={listId}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? t('update.hideNames') : t('update.showNames')}
        </button>
      )}
      {open && (
        <dl id={listId} className="grid gap-2 text-sm">
          {KINDS.filter((kind) => update[kind].length > 0).map((kind) => (
            <div key={kind}>
              <dt className="font-semibold text-ink">{t(`update.${kind}Heading`)}</dt>
              <dd>{update[kind].join(', ')}</dd>
            </div>
          ))}
        </dl>
      )}
      {update.kept.length > 0 && <p>{t('update.kept', { count: update.kept.length })}</p>}
    </div>
  );
}
```

`ui/src/features/home/UpdateDialog.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance, PackUpdate } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { ChangeSummary } from './ChangeSummary';

/** Play paused on an `ask` pack's update. Update and play is the one lit button; closing cancels the run. */
export function UpdateDialog({ instance, update }: { instance: Instance; update: PackUpdate }) {
  const { t } = useTranslation();
  const answerOnce = useSingleFlight();
  const answer = (take: boolean) =>
    answerOnce((done) => void commands.playAnswer(instance.id, take).catch(pushError).finally(done));
  const title = update.fromVersion
    ? t('update.title', { from: update.fromVersion, to: update.toVersion })
    : t('update.titleNew', { to: update.toVersion });
  return (
    <Dialog
      open
      onClose={() => void commands.stop(instance.id).catch(pushError)}
      title={title}
      actions={
        <>
          <Button onClick={() => answer(false)}>
            {update.fromVersion ? t('update.playOld', { version: update.fromVersion }) : t('update.playInstalled')}
          </Button>
          <Button variant="primary" onClick={() => answer(true)}>
            {t('update.updateAndPlay')}
          </Button>
        </>
      }
    >
      <ChangeSummary update={update} />
    </Dialog>
  );
}
```

In `Home.tsx` `InstanceView`, after the `<section>`'s content add `{run.state === 'asking' && run.update && <UpdateDialog instance={instance} update={run.update} />}`.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS, including `i18n.test.ts` (same keys and placeholders in both languages).

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): Play shows checking and syncing; the Update ready dialog

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 16: The update banner plate and "Check for updates"

**Files:**
- Create: `ui/src/features/home/UpdatePlate.tsx`
- Modify: `ui/src/features/home/Home.tsx`, `ui/src/features/home/InstanceMenu.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (`home.checkUpdates`)
- Test: `ui/src/features/home/Home.test.tsx`

**Interfaces:**
- Produces `<UpdatePlate instance={Instance} />`. It renders nothing for an instance with nothing to say.

- [ ] **Step 1: Write the failing tests**

Add to `Home.test.tsx`:

```ts
  it('offers the update between plays and runs it on Update', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    expect(await screen.findByText('Update ready: 1.0.3 → 1.0.4')).toBeInTheDocument();
    expect(screen.getByText('+3 added · 2 updated · 1 removed')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Update' }));
    expect(sent(calls, 'pack_update')).toEqual([{ id: 'sky-islands' }]);
    expect(screen.getByRole('button', { name: 'Play' })).toHaveClass('play'); // Play stays the lit one
  });

  it('says so quietly when it could not check', async () => {
    const { backend } = renderApp(<Home id="sky-islands" />);
    const handle = backend.handle;
    vi.spyOn(backend, 'handle').mockImplementation(async (cmd, args) =>
      cmd === 'pack_status'
        ? { installedVersion: '1.0.3', update: null, checkError: { category: 'network', code: 'update_check_failed', message: 'x', fix: null, detail: null, params: {} } }
        : handle(cmd, args),
    );
    expect(await screen.findByText("Couldn't check for updates")).toBeInTheDocument();
  });

  it('tells what an update changed after the run', async () => {
    renderApp(<Home id="sky-islands" />, { noUpdate: true });
    await screen.findByRole('heading', { name: 'Sky Islands' });
    act(() =>
      runStore.apply({
        type: 'run_state', instance: 'sky-islands', state: 'idle', update: null,
        exit: exitOf({ updateOnly: true, synced: { fromVersion: '1.0.3', toVersion: '1.0.4', added: 3, updated: 2, removed: 1, keptModified: ['options.txt', 'config/a.toml'] } }),
      }),
    );
    expect(screen.getByText('Updated to 1.0.4 · Kept your version of 2 files')).toBeInTheDocument();
  });

  it('never asks a vanilla instance about updates', async () => {
    const { calls } = renderApp(<Home id="creative-flat" />);
    await screen.findByRole('heading', { name: 'Creative Flat' });
    expect(sent(calls, 'pack_status')).toEqual([]);
  });

  it('checks for updates from the menu', async () => {
    const { user, calls } = renderApp(<Home id="sky-islands" />);
    await screen.findByText('Update ready: 1.0.3 → 1.0.4');
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(screen.getByRole('menuitem', { name: 'Check for updates' }));
    await vi.waitFor(() => expect(sent(calls, 'pack_status').length).toBe(2));
  });
```

Import `exitOf` from `../../ipc/mock`. The "says so quietly" test patches `backend.handle` before the first `pack_status` resolves. If that races in practice, render with a custom `ipc` through `renderWithProviders`, as `NewInstance.test.tsx`'s `renderWithCatalog` does.

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test -- Home`
Expected: FAIL.

- [ ] **Step 3: Strings**

- `en.json` `home`: `"checkUpdates": "Check for updates"`.
- `es.json` `home`: `"checkUpdates": "Buscar actualizaciones"`.

- [ ] **Step 4: Implement**

`ui/src/features/home/UpdatePlate.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { usePackStatus } from '../../state/queries';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { changeLine } from './ChangeSummary';

/** The marquee plate above the name. After a sync it says what changed; between plays it
 *  offers the update (never lit: Play is the one lit button). */
export function UpdatePlate({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const status = usePackStatus(instance.id, true);
  const updateOnce = useSingleFlight();
  if (run.state !== 'idle') return null;
  const synced = run.lastExit?.synced ?? null;
  if (synced) {
    const kept = synced.keptModified.length;
    return (
      <p className="marquee-plate mb-3" role="status">
        {t('update.updatedTo', { version: synced.toVersion })}
        {kept > 0 && ` · ${t('update.kept', { count: kept })}`}
      </p>
    );
  }
  const update = status.data?.update ?? null;
  if (update) {
    const counts = { added: update.added.length, updated: update.updated.length, removed: update.removed.length };
    return (
      <div className="marquee-plate mb-3 flex flex-wrap items-center gap-x-4 gap-y-2" role="status">
        <span className="font-semibold">
          {update.fromVersion
            ? t('update.title', { from: update.fromVersion, to: update.toVersion })
            : t('update.titleNew', { to: update.toVersion })}
        </span>
        <span className="tabular">{changeLine(t, counts)}</span>
        <Button onClick={() => updateOnce((done) => void commands.packUpdate(instance.id).catch(pushError).finally(done))}>
          {t('update.update')}
        </Button>
      </div>
    );
  }
  if (status.data?.checkError) {
    return <p className="marquee-plate mb-3 opacity-80">{t('update.checkFailed')}</p>;
  }
  return null;
}
```

In `Home.tsx` `InstanceView`, in the left column above the `<h1>`, add `{isPack(instance) && <UpdatePlate instance={instance} />}`.

In `InstanceMenu.tsx`:
- Use `useQueryClient`.
- For pack instances, add a first item:

```ts
  ...(isPack(instance)
    ? [{ label: t('home.checkUpdates'), icon: <Reload />, onSelect: () => void qc.invalidateQueries({ queryKey: qk.packStatus(instance.id) }) }]
    : []),
```

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): the update plate above the instance name, and Check for updates

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: New instance: pack link and `.mrpack` import

**Files:**
- Create: `ui/src/features/new-instance/PackSource.tsx`
- Modify: `ui/src/features/new-instance/NewInstance.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/new-instance/NewInstance.test.tsx`

**Interfaces:**
- `NewInstance({ link }: { link?: string })`. With a link it opens on the link tab, prefilled, with a "From <host>" line.
- Produces `<PackLinkForm initial?: string />` and `<PackFileForm />`.

- [ ] **Step 1: Strings**

`en.json` `newInstance`:
- Add:

```json
    "modeVersion": "Pick a version",
    "linkLabel": "Pack link",
    "linkPlaceholder": "https://…/pack.toml",
    "linkHint": "Paste a pack.toml link, or modrinth: and a project name.",
    "linkFrom": "From {{host}}",
    "addPack": "Add pack",
    "adding": "Adding…",
    "filePick": "Choose a .mrpack file",
    "fileNone": "No file chosen yet",
    "fileFilter": "Modpacks",
    "packNote": "The pack's mods download the first time you press Play."
```

- Keep `packLink` and `mrpack`; they become the tab labels.

`es.json` `newInstance`:

```json
    "modeVersion": "Elegir una versión",
    "linkLabel": "Link del pack",
    "linkPlaceholder": "https://…/pack.toml",
    "linkHint": "Pega un link a pack.toml, o modrinth: y el nombre de un proyecto.",
    "linkFrom": "De {{host}}",
    "addPack": "Agregar pack",
    "adding": "Agregando…",
    "filePick": "Elegir un archivo .mrpack",
    "fileNone": "Todavía no elegiste un archivo",
    "fileFilter": "Modpacks",
    "packNote": "Los mods del pack se descargan la primera vez que presionas Jugar."
```

`linkPlaceholder` and `fileFilter` are short, so the i18n "identical long string" check doesn't apply to them.

- [ ] **Step 2: Write the failing tests**

Replace "shows snapshots on request and keeps pack imports as coming soon" in `NewInstance.test.tsx` with a snapshots-only version (drop its two `aria-disabled` lines), and add:

```ts
it('adds a pack from a link and opens it', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  await user.click(await screen.findByRole('radio', { name: 'Paste a pack link' }));
  await user.type(screen.getByRole('textbox', { name: 'Pack link' }), 'https://perlanddev.github.io/asalith-modpack/pack.toml');
  await user.click(screen.getByRole('button', { name: 'Add pack' }));
  expect(sent(calls, 'pack_add')).toEqual([
    { request: { kind: 'link', url: 'https://perlanddev.github.io/asalith-modpack/pack.toml' } },
  ]);
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'asalith' }));
});

it('prefills a perland:// link, names its host, and waits for the click', async () => {
  const { calls } = renderApp(<NewInstance link="https://perlanddev.github.io/asalith-modpack/pack.toml" />);
  expect(await screen.findByRole('textbox', { name: 'Pack link' })).toHaveValue(
    'https://perlanddev.github.io/asalith-modpack/pack.toml',
  );
  expect(screen.getByText('From perlanddev.github.io')).toBeInTheDocument();
  expect(sent(calls, 'pack_add')).toEqual([]);
});

it('imports a .mrpack file', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  await user.click(await screen.findByRole('radio', { name: 'Import a .mrpack file' }));
  await user.click(screen.getByRole('button', { name: 'Choose a .mrpack file' }));
  expect(await screen.findByText('Sky Islands 1.0.4.mrpack')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Add pack' }));
  expect(sent(calls, 'pack_add')).toEqual([
    { request: { kind: 'file', path: 'C:/Users/player/Downloads/Sky Islands 1.0.4.mrpack' } },
  ]);
});

it('says why a link was refused', async () => {
  const { user } = renderApp(<NewInstance />);
  await user.click(await screen.findByRole('radio', { name: 'Paste a pack link' }));
  await user.type(screen.getByRole('textbox', { name: 'Pack link' }), 'http://evil.example/pack.toml');
  await user.click(screen.getByRole('button', { name: 'Add pack' }));
  expect(await screen.findByText("That doesn't look like a pack link.")).toBeInTheDocument();
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- NewInstance`
Expected: FAIL.

- [ ] **Step 4: Implement**

`ui/src/features/new-instance/PackSource.tsx`:

```tsx
import { open } from '@tauri-apps/plugin-dialog';
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useAddPack } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';

/** The host of an https link, for "From perlanddev.github.io"; null for anything else. */
function hostOf(link: string): string | null {
  try {
    const url = new URL(link.trim());
    return url.protocol === 'https:' ? url.host : null;
  } catch {
    return null;
  }
}

function AddBar({ disabled, pending }: { disabled: boolean; pending: boolean }) {
  const { t } = useTranslation();
  return (
    <div className="sticky bottom-0 flex items-center justify-end gap-4 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
      <p className="text-sm text-ink-dim">{t('newInstance.packNote')}</p>
      <Button onClick={() => navigate({ name: 'home', id: null })}>{t('common.cancel')}</Button>
      <Button type="submit" variant="primary" size="lg" disabled={disabled || pending}>
        {pending ? t('newInstance.adding') : t('newInstance.addPack')}
      </Button>
    </div>
  );
}

/** Paste a pack link (or arrive from perland://). Nothing is added before Add pack. */
export function PackLinkForm({ initial }: { initial?: string }) {
  const { t } = useTranslation();
  const [url, setUrl] = useState(initial ?? '');
  const add = useAddPack({ inline: true });
  const host = initial ? hostOf(initial) : null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!url.trim()) return;
    add.mutate({ kind: 'link', url: url.trim() }, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  return (
    <form onSubmit={submit} className="grid gap-6">
      <Field label={t('newInstance.linkLabel')}>
        <input
          className="field-input"
          type="url"
          inputMode="url"
          spellCheck={false}
          value={url}
          placeholder={t('newInstance.linkPlaceholder')}
          onChange={(e) => {
            add.reset();
            setUrl(e.target.value);
          }}
        />
      </Field>
      <p className="text-sm text-ink-dim">{host ? t('newInstance.linkFrom', { host }) : t('newInstance.linkHint')}</p>
      {add.isError && <ErrorCard error={add.error} />}
      <AddBar disabled={!url.trim()} pending={add.isPending} />
    </form>
  );
}

/** Pick a .mrpack file from disk. */
export function PackFileForm() {
  const { t } = useTranslation();
  const [path, setPath] = useState<string | null>(null);
  const [pickError, setPickError] = useState<unknown>(null);
  const add = useAddPack({ inline: true });
  const pick = async () => {
    setPickError(null);
    try {
      const chosen = await open({ multiple: false, directory: false, title: t('newInstance.mrpack'), filters: [{ name: t('newInstance.fileFilter'), extensions: ['mrpack'] }] });
      if (typeof chosen === 'string') {
        add.reset();
        setPath(chosen);
      }
    } catch (error: unknown) {
      setPickError(error);
    }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!path) return;
    add.mutate({ kind: 'file', path }, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  return (
    <form onSubmit={submit} className="grid gap-6">
      <div className="flex items-center gap-4">
        <Button onClick={() => void pick()}>{t('newInstance.filePick')}</Button>
        <p className="selectable truncate font-mono text-sm text-ink-dim">
          {path ? (path.split(/[\\/]/).pop() ?? path) : t('newInstance.fileNone')}
        </p>
      </div>
      {pickError !== null && <ErrorCard error={pickError} />}
      {add.isError && <ErrorCard error={add.error} />}
      <AddBar disabled={!path} pending={add.isPending} />
    </form>
  );
}
```

`NewInstance.tsx`:
- Take `{ link }: { link?: string }`.
- Add `const [mode, setMode] = useState<'version' | 'link' | 'file'>(link ? 'link' : 'version');`.
- Render a radio segment row at the top of the `ScreenFrame`, using the `seg` label pattern from `InstanceSettings`' Java mode. The labels are `t('newInstance.modeVersion')`, `t('newInstance.packLink')` and `t('newInstance.mrpack')`.
- Then:
  - `mode === 'version'`: the existing form, minus the two `ComingSoon` tiles and the `ComingSoon` import.
  - `mode === 'link'`: `<PackLinkForm initial={link} />`.
  - `mode === 'file'`: `<PackFileForm />`.

`ui/src/ui/ComingSoon.tsx` is still used by Accounts (the Microsoft tile), so keep it.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): new instance from a pack link or a .mrpack file; perland:// links arrive prefilled

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 18: Mods on a pack instance: "From the pack" and "Your additions"

**Files:**
- Modify: `ui/src/features/mods/Mods.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/mods/Mods.test.tsx`

- [ ] **Step 1: Strings**

`en.json` `mods`:

```json
    "fromPackTitle": "From the pack",
    "additionsTitle": "Your additions",
    "fromPack": "Pack",
    "optionalNote": "Optional mods change the next time you play.",
    "noAdditions": "Mods you add show up here. Updates never touch them.",
    "packEmpty": "This pack's mods show up after its first Play."
```

`es.json` `mods`:

```json
    "fromPackTitle": "Del pack",
    "additionsTitle": "Lo que agregaste tú",
    "fromPack": "Pack",
    "optionalNote": "Los mods opcionales cambian la próxima vez que juegues.",
    "noAdditions": "Los mods que agregues aparecen aquí. Las actualizaciones nunca los tocan.",
    "packEmpty": "Los mods de este pack aparecen después de jugarlo por primera vez."
```

- [ ] **Step 2: Write the failing tests**

Add to `Mods.test.tsx`:

```ts
describe('a pack instance', () => {
  it('lists the pack's mods read-only and the player's additions with their actions', async () => {
    const { user, calls } = renderApp(<Mods id="sky-islands" />);
    expect(await screen.findByRole('heading', { name: 'From the pack' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Your additions' })).toBeInTheDocument();
    expect(screen.getByText('Create')).toBeInTheDocument();
    expect(screen.queryByRole('switch', { name: 'Use Create' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Remove Create' })).not.toBeInTheDocument();
    expect(screen.getByText('Optional mods change the next time you play.')).toBeInTheDocument();
    await user.click(screen.getByRole('switch', { name: 'Use Iris Shaders' }));
    expect(sent(calls, 'pack_set_optional')).toEqual([{ id: 'sky-islands', path: 'mods/iris.jar', enabled: true }]);
    expect(screen.getByRole('button', { name: 'Remove zoomify-2.11.2' })).toBeInTheDocument();
  });
});
```

The existing tests for `asalith-fields` (vanilla source) stay as they are.

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- Mods`
Expected: FAIL.

- [ ] **Step 4: Implement**

In `Mods.tsx`:
- `const pack = instance ? isPack(instance) : false;`
- `const contents = usePackContents(id, pack);`
- `const optional = usePackOptional(id, { inline: true });`
- `const source = pack ? contents : mods;`: use `source` for the loading, error and retry blocks.
- `const list = (pack ? contents.data?.additions : mods.data) ?? [];`: this feeds the existing rows, the `nameCount` keys and the remove dialog unchanged.
- The subtitle count is `list.length + (contents.data?.managed.length ?? 0)`.

Before the existing `<ul>` of rows, render for pack instances:

```tsx
        {pack && contents.isSuccess && (
          <section className="grid gap-3" aria-labelledby="pack-mods">
            <h2 id="pack-mods" className="text-lg font-bold">{t('mods.fromPackTitle')}</h2>
            {contents.data.managed.length === 0 ? (
              <p className="text-ink-dim">{t('mods.packEmpty')}</p>
            ) : (
              <ul className="grid gap-2">
                {contents.data.managed.map((mod) => (
                  <li key={mod.path} className="panel flex items-center gap-4 px-4 py-3">
                    <Package className="h-6 w-6 flex-none text-ink-dim" aria-hidden />
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-semibold">{mod.name}</p>
                      {mod.optional?.description && <p className="text-sm text-ink-dim">{mod.optional.description}</p>}
                    </div>
                    {mod.optional ? (
                      <Toggle
                        checked={mod.optional.enabled}
                        disabled={optional.isPending}
                        label={t('mods.toggle', { name: mod.name })}
                        onChange={(on) => optional.mutate({ path: mod.path, enabled: on }, { onError: fail })}
                      />
                    ) : (
                      <span className="badge">{t('mods.fromPack')}</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {contents.data.managed.some((m) => m.optional) && <p className="text-sm text-ink-dim">{t('mods.optionalNote')}</p>}
            <h2 className="mt-4 text-lg font-bold">{t('mods.additionsTitle')}</h2>
          </section>
        )}
```

- The empty message for the additions list is `pack ? t('mods.noAdditions') : t('mods.empty')`.
- The vanilla note (`!instance.game.loader`) stays for non-pack instances only.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): Mods splits a pack's own mods (optional toggles) from the player's additions

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 19: "Download these manually"

**Files:**
- Create: `ui/src/features/home/ManualDownloads.tsx`
- Modify: `ui/src/features/home/Home.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/home/ManualDownloads.test.tsx` (new)

**Interfaces:**
- `<ManualDownloads instance={Instance} files={ManualFile[]} pollMs?: number />`, where `pollMs` defaults to 2000. The dialog closes through `runStore.dismissExit(id)`.

- [ ] **Step 1: Strings**

`en.json`:

```json
  "manual": {
    "title": "Download these manually",
    "body": "Their authors only allow downloads from CurseForge. Open each page and download the file. We'll find it in your Downloads folder.",
    "found": "Found {{found}} of {{total}}",
    "openPage": "Open page",
    "got": "Found",
    "openDownloads": "Open Downloads folder"
  },
```

`es.json`:

```json
  "manual": {
    "title": "Descarga estos a mano",
    "body": "Sus autores solo permiten descargarlos desde CurseForge. Abre cada página y descarga el archivo. Lo vamos a encontrar en tu carpeta de Descargas.",
    "found": "Encontrados: {{found}} de {{total}}",
    "openPage": "Abrir página",
    "got": "Encontrado",
    "openDownloads": "Abrir Descargas"
  },
```

- [ ] **Step 2: Write the failing tests**

`ui/src/features/home/ManualDownloads.test.tsx`:

```tsx
import { screen, within } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import manualFx from '../../ipc/contract/manual_progress.json';
import { createMockBackend } from '../../ipc/mock';
import type { Instance, ManualFile, ManualProgress } from '../../ipc/schema';
import { renderApp, sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { ManualDownloads } from './ManualDownloads';

const files = (manualFx as ManualProgress).remaining as ManualFile[];
const sky = { id: 'sky-islands', name: 'Sky Islands' } as Instance;

it('opens each CurseForge page through the opener', async () => {
  const backend = createMockBackend();
  const calls: [string, unknown][] = [];
  const { user } = renderWithProviders(<ManualDownloads instance={sky} files={files} pollMs={20} />, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === 'manual_import') return { found: 0, total: files.length, remaining: files };
      return backend.handle(cmd, args);
    },
  });
  const dialog = await screen.findByRole('dialog', { name: 'Download these manually' });
  expect(within(dialog).getByText('Found 0 of 1')).toBeInTheDocument();
  await user.click(within(dialog).getByRole('button', { name: 'Open page' }));
  expect(calls.some(([cmd, args]) => cmd === 'plugin:opener|open_url' && (args as { url: string }).url === files[0]!.pageUrl)).toBe(true);
  expect(within(dialog).getByRole('button', { name: 'Continue' })).toBeDisabled();
});

it('enables Continue once every file is found, and plays again', async () => {
  const blocked = [...files, { ...files[0]!, path: 'mods/b.jar', name: 'B', fileName: 'b.jar' }];
  const { user, calls } = renderApp(<ManualDownloads instance={sky} files={blocked} pollMs={20} />, { manual: true });
  const dialog = await screen.findByRole('dialog', { name: 'Download these manually' });
  await vi.waitFor(() => expect(within(dialog).getByText('Found 2 of 2')).toBeInTheDocument());
  await user.click(within(dialog).getByRole('button', { name: 'Continue' }));
  expect(sent(calls, 'play')).toEqual([{ id: 'sky-islands' }]);
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- ManualDownloads`
Expected: FAIL (the module doesn't exist).

- [ ] **Step 4: Implement**

`ui/src/features/home/ManualDownloads.tsx`:

```tsx
import { useQuery } from '@tanstack/react-query';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance, ManualFile } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Check, ExternalLink } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** Files whose authors block other launchers: open each page, and the launcher picks the
 *  downloads up from the Downloads folder (polled). Continue plays again once all are found. */
export function ManualDownloads({ instance, files, pollMs = 2000 }: { instance: Instance; files: ManualFile[]; pollMs?: number }) {
  const { t } = useTranslation();
  const progress = useQuery({
    queryKey: qk.manual(instance.id),
    queryFn: () => commands.manualImport(instance.id),
    refetchInterval: (query) => (query.state.data && query.state.data.total > 0 && query.state.data.remaining.length === 0 ? false : pollMs),
    staleTime: 0,
  });
  // Total 0 means the launcher no longer knows the list (it restarted): trust the run's list.
  const known = progress.data !== undefined && progress.data.total > 0;
  const remaining = known ? progress.data!.remaining : files;
  const total = known ? progress.data!.total : files.length;
  const found = total - remaining.length;
  const close = () => runStore.dismissExit(instance.id);
  const playOnce = useSingleFlight();
  const play = () => playOnce((done) => void commands.play(instance.id).then(close, pushError).finally(done));
  return (
    <Dialog
      open
      onClose={close}
      title={t('manual.title')}
      actions={
        <>
          <Button onClick={() => void commands.openFolder({ kind: 'downloads' }).catch(pushError)}>{t('manual.openDownloads')}</Button>
          <Button variant="primary" disabled={remaining.length > 0} onClick={play}>
            {t('common.continue')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <p>{t('manual.body')}</p>
        <p role="status" className="tabular font-semibold text-ink">
          {t('manual.found', { found, total })}
        </p>
        <ul className="grid gap-2">
          {files.map((file) => {
            const got = !remaining.some((r) => r.path === file.path);
            return (
              <li key={file.path} className="panel flex items-center gap-4 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate font-semibold text-ink">{file.name}</p>
                  <p className="selectable truncate font-mono text-sm">{file.fileName}</p>
                </div>
                {got ? (
                  <span className="flex items-center gap-2 text-lime">
                    <Check className="h-5 w-5" aria-hidden />
                    {t('manual.got')}
                  </span>
                ) : (
                  <Button icon={<ExternalLink />} onClick={() => void openUrl(file.pageUrl).catch(pushError)}>
                    {t('manual.openPage')}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      </div>
    </Dialog>
  );
}
```

In `Home.tsx` `InstanceView`:

```tsx
      {run.state === 'idle' && run.lastExit && run.lastExit.manual.length > 0 && (
        <ManualDownloads instance={instance} files={run.lastExit.manual} />
      )}
```

The dialog's Escape and close go to `dismissExit`. Cancel isn't a separate button, because Escape and the dialog's own close do the same thing.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): Download these manually: open CurseForge pages, find the files, play again

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 20: Pack art in the marquee and rail; pack instances' settings

**Files:**
- Modify: `ui/src/art/Marquee.tsx`, `ui/src/features/home/Home.tsx`, `ui/src/features/rail/Rail.tsx`, `ui/src/styles/app.css`
- Modify: `ui/src/features/instance-settings/InstanceSettings.tsx`
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `ui/src/features/home/Home.test.tsx`, `ui/src/features/rail/Rail.test.tsx`, `ui/src/features/instance-settings/InstanceSettings.test.tsx`

**Interfaces:**
- `Marquee` gains `banner?: string | null`. With a banner, the image replaces the generated sky and hills, and the grille and vignette stay over it.
- `RailItem` gains `pack: boolean`.

- [ ] **Step 1: Strings**

`en.json` `instanceSettings`:

```json
    "updates": "Updates",
    "updatesAuto": "Automatic",
    "updatesAsk": "Ask me",
    "updatesPinned": "Don't update",
    "gameFromPack": "The pack picks the Minecraft version and loader."
```

`es.json` `instanceSettings`:

```json
    "updates": "Actualizaciones",
    "updatesAuto": "Automáticas",
    "updatesAsk": "Preguntarme",
    "updatesPinned": "No actualizar",
    "gameFromPack": "El pack elige la versión de Minecraft y el loader."
```

- [ ] **Step 2: Write the failing tests**

`Home.test.tsx`:

```ts
  it('shows the pack's own banner, and generated art otherwise', async () => {
    const { container } = renderApp(<Home id="sky-islands" />);
    await screen.findByRole('heading', { name: 'Sky Islands' });
    await vi.waitFor(() => expect(container.querySelector('img.marquee-banner')?.getAttribute('src')).toMatch(/^data:image\/svg\+xml;base64,/));
  });
```

`Rail.test.tsx`:

```ts
it('uses a pack's icon', async () => {
  renderApp(<Rail />);
  const sky = await screen.findByRole('button', { name: 'Sky Islands' });
  await vi.waitFor(() => expect(sky.querySelector('img')?.getAttribute('src')).toMatch(/^data:image\//));
  expect(screen.getByRole('button', { name: 'Creative Flat' }).querySelector('img')).toBeNull();
});
```

`InstanceSettings.test.tsx`:

```ts
it('sets a pack's update policy and leaves its game to the pack', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="sky-islands" />);
  expect(await screen.findByText('The pack picks the Minecraft version and loader.')).toBeInTheDocument();
  expect(screen.queryByRole('combobox', { name: 'Minecraft version' })).not.toBeInTheDocument();
  expect(screen.getByRole('radio', { name: 'Ask me' })).toBeChecked();
  await user.click(screen.getByRole('radio', { name: 'Automatic' }));
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const [sentArgs] = sent(calls, 'instance_update') as { instance: Instance }[];
  expect(sentArgs!.instance.updatePolicy).toBe('auto');
  expect(sentArgs!.instance.game).toEqual({ minecraft: '1.20.1', loader: { kind: 'forge', version: '47.4.22' } });
});
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm --filter ui test -- Home Rail InstanceSettings`
Expected: FAIL.

- [ ] **Step 4: Implement**

`Marquee.tsx`:
- Add the `banner` prop.
- When `banner` is set, render `<img src={banner} alt="" className="marquee-banner" draggable={false} />` as the first child of the frame.
- Inside the `<svg>`, render the sky rect, bloom, sun and layers only when `!banner`. Always render the grille and vignette rects, plus their `<defs>`.
- Keep `aria-hidden`.
- In `app.css`, next to `.marquee`:

```css
  .marquee-banner {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center bottom;
  }
  .marquee svg {
    position: relative;
  }
```

  `/impeccable` tunes this in Task 21: the dimming when the game runs, a scrim under the name, and pixelated scaling for small pixel-art banners.

`Home.tsx`:
- `const art = usePackArt(instance.id, isPack(instance));`
- Render `<Marquee … banner={art.data?.banner ?? null} />`.

`Rail.tsx`:
- Items carry `pack: i.source.kind !== 'none'` (damaged items get `pack: false`).
- `RailItem` does `const art = usePackArt(item.id, item.pack);` and renders the icon:

```tsx
        {art.data?.icon ? (
          <img src={art.data.icon} alt="" width={40} height={40} className="rail-art" draggable={false} />
        ) : (
          <PlotIcon seed={item.id} size={40} />
        )}
```

- In `app.css`, add `.rail-art { width: 40px; height: 40px; object-fit: cover; image-rendering: pixelated; }`.

`InstanceSettings.tsx`, inside `Form`:
- `const pack = isPack(instance);` and `const [policy, setPolicy] = useState(instance.updatePolicy);`
- Replace the Game section's body for packs:

```tsx
        {pack ? (
          <>
            <p>{versionLine(t, instance)}</p>
            <p className="text-sm text-ink-dim">{t('instanceSettings.gameFromPack')}</p>
          </>
        ) : (
          <VersionFields choice={versions} />
        )}
```

- Add an Updates fieldset after the Game section, for packs only:

```tsx
      {pack && (
        <fieldset className="grid gap-3">
          <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.updates')}</legend>
          <div className="flex flex-wrap gap-2">
            {(['auto', 'ask', 'pinned'] as const).map((value) => (
              <label key={value} className="seg" data-checked={policy === value ? '' : undefined}>
                <input type="radio" name="updates" className="sr-only" checked={policy === value} onChange={() => edit(setPolicy)(value)} />
                {t(value === 'auto' ? 'instanceSettings.updatesAuto' : value === 'ask' ? 'instanceSettings.updatesAsk' : 'instanceSettings.updatesPinned')}
              </label>
            ))}
          </div>
        </fieldset>
      )}
```

- `ready` doesn't need `choice.resolved.ready` for packs. `save` sends `game: pack ? instance.game : …` (as today) and `updatePolicy: policy`.

- [ ] **Step 5: Run them to verify they pass**

Run: `pnpm --filter ui test && pnpm -r typecheck && pnpm -r lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ui/src
git commit -m "feat(ui): pack art in the marquee and rail; update policy and pack-picked game in instance settings

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 21: `/impeccable` craft and finish review of the new surfaces

CLAUDE.md requires that UI styling goes through `/impeccable`. Tasks 15–20 built the surfaces with existing classes. This task makes them belong to the Arcade Cabinet world and verifies that with the finish reviewer.

**Files:** whatever `/impeccable` touches under `ui/src`, plus `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.

- [ ] **Step 1: Run `/impeccable`**

Invoke `/impeccable` on the contract surface `.impeccable/surfaces/ui-src-app-tsx.md`. Scope it to these surfaces (polish/craft, no redesign):
- **Play button states:** `checking`, `asking`, `syncing`.
- **The "Update ready" dialog:** one lit button, the change line, names on request, the kept line.
- **The update plate** above the instance name: the plate grammar, never lit, readable over pack art.
- **The marquee with a pack banner:** a scrim or vignette so the name stays legible, dimming when the game runs, pixelated scaling for small art.
- **Rail pack icons.**
- **New instance tabs:** version, pack link, `.mrpack`.
- **Mods** "From the pack" and "Your additions" sections.
- **"Download these manually"** dialog.
- **Instance settings** Updates segment.

Constraints to pass along: DESIGN.md's rules (one lit button per view; lime lamp for selection; blue for progress; hard offset under-layers; pixel-stepped corners; snaps, not eases), es strings ~30% longer, 960 × 600 minimum.

- [ ] **Step 2: Screenshots in the mock**

Run `pnpm --filter ui dev:mock` and capture these views with the preview browser:
- `/?` (Sky Islands with the update plate)
- the asking dialog (press Play on Sky Islands)
- `/?manual` (Play, then the manual dialog)
- `/?link=https://perlanddev.github.io/asalith-modpack/pack.toml`
- Mods for Sky Islands
- New instance's three tabs
- each of the above in Spanish

- [ ] **Step 3: Finish review**

Dispatch the `impeccable-finish-reviewer` agent on the build against the contract. Fix every material item it returns, then re-review until it passes. Then dispatch `impeccable-documenter` to record the new components in `DESIGN.md` (update dialog, plate, manual dialog, pack art) and its sidecar.

- [ ] **Step 4: Gate and commit**

Run the full gate:
- `pnpm --filter ui build`
- `pnpm -r lint`
- `pnpm -r typecheck`
- `pnpm -r test`
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

```bash
git add ui DESIGN.md .impeccable
git commit -m "style(ui): impeccable pass on pack surfaces; DESIGN.md records them

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 22: Whole-branch verification, owner smoke test, PR

- [ ] **Step 1: Rebase and run the full gate**

`git fetch origin && git rebase origin/main` (Plan 4 may have landed; resolve conflicts additively). Then the full gate from Task 21 Step 4. Every command must pass, and the output is recorded in the PR body.

- [ ] **Step 2: Whole-branch review**

Use superpowers:requesting-code-review on the whole branch (`origin/main..HEAD`). Fix the findings it confirms.

- [ ] **Step 3: Owner smoke test (real app)**

Ask the owner to run, in PowerShell:

```powershell
$env:PERLAND_LAUNCHER_ROOT = "E:\perland-scratch\3b"; pnpm tauri dev
```

and check, in order:
1. New instance → **Pack link** → paste `https://perlanddev.github.io/asalith-modpack/pack.toml` → **Add pack**. The instance opens with Asalith's art in the marquee and the rail.
2. **Play.** The button shows "Installing Asalith" with files and MB, then Minecraft starts. Quit the game.
3. **Mods:**
   - "From the pack" lists the pack's mods, read-only.
   - Drop a `.jar` in. It shows under "Your additions" and can be turned off or removed.
   - Pack mods can't be.
4. **Update flow.** No new Asalith release is needed: in the scratch root, edit `instances\asalith*\.perland\managed.json` and `pack.json` to say `"packVersion": "0.0.1"` and a different `"stamp"`. Then:
   - Reopen the app. The plate says "Update ready: 0.0.1 → <current>".
   - Press **Play**. The dialog appears. **Play 0.0.1** plays without updating.
   - Press **Play** again → **Update and play** syncs and plays.
   - Report anything odd: if the stamp edit shows as "repairing", that's expected.
5. **Offline:** turn off the network and press Play. A "Couldn't check for updates" toast appears and the game plays.
6. **Deep link:** with the app running, open `perland://pack?url=https%3A%2F%2Fperlanddev.github.io%2Fasalith-modpack%2Fpack.toml` with Win+R. The launcher comes forward on New instance, prefilled, "From perlanddev.github.io", and nothing is added until **Add pack**.
7. **Instance settings → Updates:** Automatic / Ask me / Don't update save and apply on the next Play.

The manual-download dialog can't be smoke-tested with Asalith: none of its CurseForge files are blocked. It is covered by the mock (`/?manual`) and the tests.

- [ ] **Step 4: Knowledge and memory**

- Update the memory note `plan-3-pack-sync` with 3b's status, owner decisions and carry-forwards.
- `write_knowledge` the durable non-obvious items:
  - the run holds `InstanceLock` from `checking` to exit;
  - Stop during `syncing` is cooperative (`Cancel`) because aborting could outlive the lock;
  - the lock-retry window after cancel;
  - deep links only via `deep_link_take`.

- [ ] **Step 5: PR**

Open a PR against `main`. The body has these sections, then the attribution line:
- **Summary**
- **Owner decisions** (addendum §4.1)
- **What changed per layer:** core, app, Tauri, UI
- **Verification:** test counts, gate output, the owner smoke results
- **Carry-forwards**

```
🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

Then bind the PR with the ccd_pr tools and read its CI.

## Carry-forwards from Plan 3a, and where they land

| Item | Task |
|---|---|
| Redirect hop limit off by one; https→http check per hop only | 1 |
| `ensure` fails when an update download fails although the old version is intact | 4 (falls back, with `update_download_failed`) |
| Modrinth: a deleted installed version hides updates forever | 2 |
| `Syncer::check` ignores Pinned | 2 |
| Manual scan re-hashes everything when polled | 5 |
| `java_test` / `mods_add` take webview paths | 10 |
| `ui/src/ipc/mock.ts` returns Fabric versions for Forge | 14 |

## Deferred beyond 3b

- **Browse (Modrinth search):** Plan 3c.
- **Installer-time `perland://` registration:** Plan 6, with bundling. Dev runs register themselves.
- **A "Play 1.0.3 instead" button in the manual-download dialog,** for an update whose new files are blocked. For now the player cancels and keeps playing the installed version by choosing **Play 1.0.3** on the next Play.
- **Object-cache GC, metadata body caps, and the other Plan 6 items** listed in Plan 3a.
