# Undo Last Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Before the launcher changes an instance's mods, keep one restore point; one "Go back to how it was" action restores it; pack instances hold updates until "Update again".

**Architecture:** A new `launcher-core::restore` module owns the restore point on disk (`<instance>/.perland/restore/`): snapshot by hard link (archives) or copy (everything else), coalescing per Play, the journalled folder swap and its recovery. Core sync calls it before the swap. `launcher-app` calls it around Browse installs, removals and runs, and adds three commands and the update hold. The UI adds a menu item, a confirm dialog, a plate line and a crash-card action.

**Tech Stack:** Rust (tokio, serde, `same-file`), Tauri 2 wiring, React + zod + Vitest.

**Spec:** `docs/superpowers/specs/2026-10-21-undo-last-update-design.md` (issue #85)

## Global Constraints

- Never snapshot or restore `saves`, `screenshots`, `logs`, `crash-reports`, `mods-before-link`, dot-folders, or any root-level file (`options.txt`).
- Hard-link `.jar`, `.zip`, `.disabled` files; copy all others. If the first hard link fails, copy everything for that point.
- Copy cap: **1 GiB** copied bytes; plus `SPACE_MARGIN` free space. Over that, the change proceeds without a point.
- A point older than **30 days** reads as none and is deleted.
- One point per instance. Two kinds: `beforeChange`, `beforeGoingBack`.
- A restore point never blocks a change. `prepare` failures are logged and swallowed.
- Everything that writes runs under the instance's `InstanceLock` (held by the caller).
- Every UI string in `en.json` and `es.json` (neutral Latin American Spanish, "tú"). New `ErrorCode`s need `errors.<code>.message` in both.
- The contract is regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, never hand-edited.
- UI styling goes through `/impeccable`.
- Tests stay tiny (a few bytes per file).

## Review Focus

1. **A game folder that lacks a folder in scope** (no `shaderpacks/` before a shader install). Going back must remove the new folder, not fail. Test: `go_back_removes_folder_that_did_not_exist`.
2. **A crash mid-swap** (the journal exists, some folders already swapped). Recovery must finish forward, and Play must not launch a mix. Test: `recover_rolls_forward_after_each_step`.
3. **A file system without hard links.** The copy fallback must still make a correct point. Test: `snapshot_falls_back_to_copy` (force via a `LinkMode::Copy` test hook).
4. **A config file edited after the snapshot.** The snapshot must not change, because config is copied, not linked. Test: `config_edit_does_not_leak_into_point`.
5. **Going back twice in a row** (go back, then "Undo going back"). The instance must return to the post-update state exactly. Test: `go_back_twice_returns_to_after_state`.

---

### Task 1: Restore point model, scope and snapshot

**Files:**
- Create: `crates/launcher-core/src/restore/mod.rs`, `crates/launcher-core/src/restore/snapshot.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod restore;`), `crates/launcher-core/Cargo.toml` (add `same-file` with the version already in `Cargo.lock`)

**Interfaces (produces):**
```rust
// restore/mod.rs
pub const MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;
pub const COPY_CAP: u64 = 1 << 30;
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)] #[serde(rename_all = "camelCase")]
pub enum PointKind { BeforeChange, BeforeGoingBack }
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)] #[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change { PackUpdate { from: String, to: String }, Installed { name: String }, Removed { name: String } }
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)] #[serde(rename_all = "camelCase")]
pub struct SnapFile { pub path: String, pub size: u64, pub mtime: u64, pub linked: bool }
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)] #[serde(rename_all = "camelCase")]
pub struct PointMeta {
    pub version: u32, pub created_at: u64, pub kind: PointKind, pub scope: Vec<String>,
    pub game: GameSpec, pub source: SourceRef, pub optional_choices: BTreeMap<String, bool>,
    pub pack_version: Option<String>, pub changes: Vec<Change>,
    pub sealed: bool, pub last_change_at: u64, pub clean_run_since: bool,
    pub files: Vec<SnapFile>,
}
pub fn in_scope(top: &str) -> bool;
pub fn top_of(rel: &str) -> Option<&str>;      // "mods/x.jar" -> Some("mods"); "options.txt" -> None
pub(crate) fn restore_dir(perland: &Path) -> PathBuf; // perland/restore
// restore/snapshot.rs
#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum LinkMode { Auto, Copy }
pub(crate) fn is_archive(name: &str) -> bool;
/// Snapshot game_dir/<top> into dest/<top>. Archives are hard-linked unless `mode` is Copy or a
/// link fails (then the caller retries the whole point with Copy). Returns the files written.
pub(crate) fn snapshot_folder(game_dir: &Path, top: &str, dest: &Path, mode: LinkMode)
    -> std::io::Result<Result<Vec<SnapFile>, LinkFailed>>;
pub(crate) struct LinkFailed;
/// Bytes this point alone holds: copied files, and linked files game_dir no longer has as the same file.
pub(crate) fn held_bytes(point_files: &Path, game_dir: &Path, files: &[SnapFile]) -> u64;
```

- [ ] **Step 1: Write the failing tests** in `restore/mod.rs` and `restore/snapshot.rs`:
  - `in_scope_refuses_the_denylist`: false for `saves`, `screenshots`, `logs`, `crash-reports`, `mods-before-link`, `.fabric`; true for `mods`, `config`, `kubejs`.
  - `top_of_root_file_is_none`: `top_of("options.txt") == None`, `top_of("config/a/b.toml") == Some("config")`.
  - `is_archive_names`: true for `a.jar`, `a.zip`, `a.jar.disabled`; false for `a.toml`, `a.json`.
  - `snapshot_links_jars_and_copies_config`: after `snapshot_folder` on mods and on config, `same_file::is_same_file(game/mods/a.jar, dest/mods/a.jar)` is true. The config file is not the same file but has the same bytes. `linked` flags match.
  - `config_edit_does_not_leak_into_point` (Review Focus 4): rewrite the game's config in place, and the snapshot bytes stay the same.
  - `snapshot_falls_back_to_copy` (Review Focus 3): with `LinkMode::Copy`, every file has `linked: false` and none is the same file.
  - `held_bytes_counts_copies_and_orphaned_links`: one linked 3-byte jar still in the game folder counts 0. Delete it from the game, and it counts 3. A copied 2-byte config always counts 2.
- [ ] **Step 2: Run** `cargo test -p launcher-core restore`. Expected: compile failure (module missing).
- [ ] **Step 3: Implement.**
  - Use blocking `std::fs` (callers use `spawn_blocking`).
  - Walk with `std::fs::read_dir` recursively and skip symlinks.
  - `mtime` is unix seconds of `modified()`.
  - A missing `game_dir/<top>` gives an empty `Vec`, and `dest/<top>` is not created.
- [ ] **Step 4: Run** `cargo test -p launcher-core restore`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(restore): restore point model and link-or-copy snapshot`.

### Task 2: Prepare, coalesce, seal, read, prune

**Files:**
- Create: `crates/launcher-core/src/restore/point.rs`
- Modify: `crates/launcher-core/src/restore/mod.rs` (re-exports)

**Interfaces:**
- Consumes: Task 1 types, `InstanceStore::{load, game_dir, perland_dir}`, `fsutil::{write_atomic, rename_with_retry}`, `sync::apply::{free_space, SPACE_MARGIN}` (`pub(crate)`), `sync::lockfile::{MANAGED_FILE, PACK_FILE}`.
- Produces:
```rust
pub struct ChangeCtx { pub during_run: bool, pub now: u64 }
/// Best effort: never fails the change. `tops` are filtered with `in_scope`.
pub async fn before_change(store: &InstanceStore, id: &str, tops: &[String], change: Change, ctx: ChangeCtx);
/// Append `change` without snapshotting anything (resumed sync).
pub async fn note_change(store: &InstanceStore, id: &str, change: Change, now: u64);
pub async fn read(store: &InstanceStore, id: &str, now: u64) -> Option<PointMeta>;   // prunes > MAX_AGE_SECS and damaged
pub async fn held_size(store: &InstanceStore, id: &str) -> u64;
pub async fn seal(store: &InstanceStore, id: &str);
pub async fn run_ended(store: &InstanceStore, id: &str, started_at: u64, crashed: bool);
pub(crate) async fn build_point(store, id, tops: &[String], kind: PointKind, now: u64, into: &Path) -> Result<PointMeta>;
pub(crate) async fn swap_in(restore_dir: &Path) -> Result<()>;  // new/ -> point/ via old/
```

**Coalescing rules** (spec §5.4):

| Existing point | `during_run` | Action |
|---|---|---|
| none | false | build a new unsealed point |
| none | true | build a new point with `sealed = true` |
| unsealed | any | append the change; snapshot missing `tops` into `point/files/<top>` via `new-<top>/`; add them to `scope` |
| sealed | true | append the change; add missing `tops` as above |
| sealed | false | build a new point, which replaces it |
| `beforeGoingBack` | any | build a new `beforeChange` point, which replaces it |

Every append sets `last_change_at = now` and `clean_run_since = false`. Changes are deduplicated against the last entry.

- [ ] **Step 1: Write the failing tests** (temp `InstanceStore` via `test_support`, tiny files):
  - `first_change_makes_unsealed_point`: `read(..).unwrap()` has `kind == BeforeChange`, `!sealed`, `scope == ["mods"]`, `changes == [Installed{name:"Sodium"}]`, and the old jar under `point/files/mods`.
  - `second_change_before_play_appends`: two `before_change` calls give one point with both changes. The second call's newly installed jar is **not** in the snapshot.
  - `new_top_is_added_to_unsealed_point`: `mods` then `config` gives `scope == ["config","mods"]` (sorted).
  - `sealed_point_is_replaced`: `seal`, then `before_change`, gives a fresh point with only the new change.
  - `change_during_run_appends_to_sealed_point`: `seal`, then `before_change(during_run: true)`, appends. `last_change_at` moves, and `run_ended(started_at < last_change_at, false)` leaves `clean_run_since == false`.
  - `clean_run_marks_point`: `run_ended(started_at >= last_change_at, crashed=false)` sets `clean_run_since`. With `crashed=true`, it does not.
  - `denylist_never_snapshotted`: `tops = ["saves","mods"]` gives `scope == ["mods"]`, and no `saves` under `point/files`.
  - `old_point_is_pruned`: `read(now + MAX_AGE_SECS + 1)` returns None and removes `restore/point`.
  - `damaged_point_reads_none`: garbage `point.json` reads None.
  - `copy_over_cap_makes_no_point`: run this with a test-only cap override (`pub(crate) static COPY_CAP_OVERRIDE` or a `cap` arg on an inner fn) of 4 bytes and a 10-byte config. No point exists, and the call returns normally.
  - `leftover_new_dir_is_cleared`: a stale `restore/new/` is removed before building.
  - `point_keeps_managed_and_pack_json`: both files are copied when present. `pack_version` comes from `managed.json`.
- [ ] **Step 2: Run** `cargo test -p launcher-core restore`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `build_point` runs the snapshot in `spawn_blocking` with `LinkMode::Auto`. On `LinkFailed`, it wipes `into` and retries with `Copy`.
  - It counts copied bytes and fails with an internal error over `COPY_CAP`, or when `free_space(perland) < copied + SPACE_MARGIN`. That check runs before the copy, from a size walk.
  - It writes `point.json` with `write_atomic`.
  - `swap_in` does: remove `old/` if present, rename `point/` to `old/`, rename `new/` to `point/` (`rename_with_retry`), then remove `old/`.
  - Log failures through `tracing::warn!` with the folder and count only.
- [ ] **Step 4: Run** `cargo test -p launcher-core restore`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(restore): take, coalesce, seal and prune restore points`.

### Task 3: Going back, the journal, recovery, the hold field

**Files:**
- Create: `crates/launcher-core/src/restore/go_back.rs`
- Modify:
  - `crates/launcher-core/src/instances.rs`: add the field
    ```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_hold: Option<UpdateHold>
    ```
    and
    ```rust
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct UpdateHold { pub since: u64, pub version: String }
    ```
    Fix every `Instance { .. }` literal.
  - `crates/launcher-core/src/error.rs`: `ErrorCode::RestorePointGone` maps to `"restore_point_gone"`, and `ErrorCode::RestoreFailed` to `"restore_failed"`. Add the constructors `restore::point_gone()` (category Disk, message "There is nothing to go back to") and `restore::failed(detail)` (message "Couldn't go back to how it was", fix "Close any window showing the instance's folder, then try again").

**Interfaces (produces):**
```rust
pub struct WentBack { pub kept_current: u32 }
/// Caller holds the instance lock and has checked the game isn't running.
/// `hold`: set on the instance as part of the restore (the app decides it).
pub async fn go_back(store: &InstanceStore, id: &str, hold: Option<UpdateHold>, now: u64) -> Result<WentBack>;
/// Finish or clean an interrupted go_back. Ok(()) when there was nothing to do.
pub async fn recover(store: &InstanceStore, id: &str) -> Result<()>;
#[cfg(test)] pub(crate) enum StopAfter { Staged, Journal, Swapped(usize), Bookkeeping, Hold }
```

**Journal** `restore/restoring.json`:
```json
{ "version": 1, "tops": ["config", "mods"], "hold": null, "had": { "mods": true, "config": true } }
```
`had[top]` is false when the point had no such folder, so recovery moves the current one aside without replacing it.

**Steps of `go_back`** follow spec §5.5, 1–9.
- Step 6 restores, onto a freshly loaded `Instance`: `managed.json` and `pack.json` (deleting them when the point had none), `game`, `source` and `optional_choices`. When `hold` is `Some`, it sets `update_hold`; when `None`, it clears it.
- Step 8 removes a `mods-before-link/<name>` only when `same_file::is_same_file` matches `mods/<name>`.
- Recovery, with a journal: for each top whose `minecraft/.<top>.perland-restore` exists, it moves `minecraft/<top>` aside (if present) and renames the staged folder in. It then redoes steps 6–9. Step 6 is idempotent, and it reads the bookkeeping from `point/`.
- The `beforeGoingBack` point waits in `restore/new/` until step 9.
- Recovery without a journal removes `minecraft/.*.perland-restore`, `restore/aside` and `restore/new`.

- [ ] **Step 1: Write the failing tests:**
  - `go_back_restores_folders_and_bookkeeping`:
    - **Setup:** `mods/{a.jar}`, `config/c.toml = "1"`, `managed.json` v1 and game 1.20.1. Then `before_change`, then simulate the update: `mods/{b.jar}`, `config/c.toml = "2"`, `managed.json` v2 and game 1.20.4.
    - **Expect after `go_back`:** `mods == {a.jar}`, `c.toml == "1"`, `managed.json` v1, game 1.20.1, `saves/w/level.dat` untouched, and `options.txt` untouched.
  - `go_back_keeps_after_state_as_point`: afterwards `read()` is `kind == BeforeGoingBack`, and its snapshot has `b.jar`.
  - `go_back_twice_returns_to_after_state` (Review Focus 5).
  - `go_back_removes_folder_that_did_not_exist` (Review Focus 1): the point had no `shaderpacks`. It was added after. Going back leaves no `shaderpacks` folder.
  - `go_back_sets_and_clears_hold`: `Some(UpdateHold{since:5, version:"1.0"})` is on the reloaded instance. A second go-back with `None` clears it.
  - `go_back_keeps_a_rename_made_meanwhile`: the instance `name` is changed after the snapshot and stays changed.
  - `go_back_without_point_is_point_gone`: the error code is `RestorePointGone`.
  - `in_place_edited_jar_is_kept_and_counted`: change a linked jar's bytes in place in the game folder (same inode; the snapshot shares it, so the size or mtime no longer matches). The current file is kept, and `kept_current == 1`.
  - `mods_before_link_duplicate_is_cleared`: hard-link `mods/p.jar` into `mods-before-link/p.jar` before the update. After going back, `mods-before-link/p.jar` is gone and `mods/p.jar` exists.
  - `recover_rolls_forward_after_each_step` (Review Focus 2): for each `StopAfter` variant, run `go_back` with that stop, then `recover`. Assert the final state equals either the full before state or (only for `Staged`) the untouched after state, never a mix. Assert no `.perland-restore`, `aside`, `new` or `restoring.json` is left.
- [ ] **Step 2: Run** `cargo test -p launcher-core restore`. Expected: FAIL.
- [ ] **Step 3: Implement** `go_back.rs`. It uses `rename_with_retry` for every folder rename. Map an in-use refusal to `restore::failed`.
- [ ] **Step 4: Run** `cargo test -p launcher-core`. Expected: PASS (instance literals fixed).
- [ ] **Step 5: Commit** `feat(restore): go back with a journalled swap, recovery and update hold`.

### Task 4: Hook sync into restore points

**Files:**
- Modify: `crates/launcher-core/src/sync/mod.rs` (`sync_inner`, just before `save_pack(&perland, desired)`)

**Interfaces:**
- Consumes: `restore::{before_change, note_change, top_of, Change, ChangeCtx}`.
- **Rule** (spec §5.4, resumed sync): `managed.is_some()` and the plan writes or trashes something.
  - Resumed means `lockfile::load_pack(&perland).map(|p| p.stamp) != managed.stamp`. A resumed sync calls `note_change`.
  - Otherwise it calls `before_change` with `tops` = the sorted, deduplicated `top_of` over `plan.write`, `plan.trash` and `plan.case_aliases`, and `Change::PackUpdate { from: managed.pack_version, to: desired.pack_version }`.
  - `during_run: false`: sync runs before launch.
  - When `from == to` (a repair), call nothing: a repair restores the pack's own files.

- [ ] **Step 1: Write the failing tests** in `sync/mod.rs` tests, using the existing sync test helpers:
  - `update_sync_takes_restore_point`: after a v1 → v2 sync, `restore::read` has `PackUpdate{from:"1",to:"2"}`, and the snapshot holds v1's jar.
  - `first_install_takes_no_point`: there is no point after the first sync.
  - `up_to_date_sync_takes_no_point`: the same.
  - `resumed_sync_only_notes`: write a v2 `pack.json` while `managed.json` says v1, with an unsealed point present. The sync appends the change without re-snapshotting: the snapshot's `mods` still equals the original.
- [ ] **Step 2: Run** `cargo test -p launcher-core sync::`. Expected: FAIL.
- [ ] **Step 3: Implement** the hook.
- [ ] **Step 4: Run** `cargo test -p launcher-core`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(sync): keep a restore point before an update changes files`.

### Task 5: App layer: commands, hold, runs, Browse and removals

**Files:**
- Create: `crates/launcher-app/src/restore.rs` (the commands and their tests)
- Modify:
  - `crates/launcher-app/src/lib.rs`
  - `runs.rs`:
    - `checks_for_updates` gains `&& instance.update_hold.is_none()`;
    - seal at `Running`;
    - `run_ended` after the exit;
    - `ExitInfo.restore`;
    - call `restore::recover` right after `lock_instance` succeeds, finishing with a `failed(error)` exit when it errs.
  - `events.rs`: `ExitInfo.restore: bool`, which the UI reads as "offer Go back on the crash card". If Crash doctor merged first, use `CrashFix::Restore` instead (see below).
  - `browse.rs` `browse_install`: when `stored.instance.is_some()`, after `content_guard`, call `before_change`. Its arguments:
    - `tops`: from each item's `relative_path`;
    - `change`: `Change::Installed{name: item.title}` for each item;
    - `during_run`: `_guard._lock.is_none()`.
  - `app.rs` `mod_remove` / `content_remove`:
    - take `InstanceLock::try_acquire(perland_dir)`; map `InstanceLocked` to `busy(id)`;
    - `mod_remove` also refuses while `is_installing`;
    - call `before_change(tops: ["mods"] / [folder], Change::Removed{name: file})` before deleting.
  - `packs.rs`: `pack_status` clears `update_hold` when the check finds no update and the installed `pack_version == hold.version`.
  - `dto.rs`:
    ```rust
    pub struct RestorePointDto {
        kind: PointKind,
        created_at: u64,
        changes: Vec<Change>,
        size_bytes: u64,
        server: Option<String>, // server display name for the warning
    }
    ```
    The instance DTO carries `update_hold`.
  - `src-tauri/src/commands.rs` and `src-tauri/src/main.rs`: wiring only.
  - `crates/launcher-app/tests/contract.rs`: samples for `restore_point` and the exit info.

**Interfaces (produces):**
```rust
impl Core {
    pub async fn restore_point(&self, id: &str) -> Result<Option<RestorePointDto>>;
    /// Refused while busy. Takes the lock. hold = Some{since: now, version: point.pack_version}
    /// iff checks_for_updates(current instance) is true.
    pub async fn restore_go_back(&self, id: &str) -> Result<RestorePointDto>; // the new (beforeGoingBack) point
    /// Clears update_hold, then pack_update(id).
    pub async fn pack_update_again(self: &Arc<Self>, id: &str) -> Result<()>;
}
```
Tauri commands: `restore_point`, `restore_go_back`, `pack_update_again`.

- **Crash card offer:** `ExitInfo.restore` is true iff `info.is_crash()` and `read()` is `BeforeChange` with `!clean_run_since`. Compute it after `run_ended(.., crashed: true)`.
- **If Crash doctor merged first:** `git log origin/main --oneline | grep -i crash` shows it, and `ExitInfo` has `fixes`. Then add `CrashFix::Restore` (unit variant, `kind: "restore"`), and push it first when `cause.is_none()`, last otherwise. Do not add `restore: bool`.

- [ ] **Step 1: Write the failing tests** with the existing app test harness (`testing.rs`, the fake runner):
  - `restore_point_is_none_without_point`.
  - `go_back_on_pack_instance_sets_hold_and_play_skips_check`: after `restore_go_back`, the instance has `update_hold`, `checks_for_updates` is false, and the next Play's fake runner sees `accept == None` with no `find_update` call.
  - `go_back_on_plain_instance_sets_no_hold`.
  - `go_back_on_pinned_instance_sets_no_hold`.
  - `go_back_refused_while_running`: the error is the same as `busy(id)`.
  - `pack_update_again_clears_hold_and_runs_update`.
  - `pack_status_clears_hold_when_source_matches`.
  - `mod_remove_takes_point_and_lock`: after the removal, `restore_point` lists `Removed{name:"x.jar"}`. Holding the lock elsewhere makes `mod_remove` fail with busy.
  - `crash_after_change_offers_restore`: a fake game exits with code 1 after a sync that took a point, giving `exit.restore == true`. A clean exit gives `restore == false`, and the following crash gives `restore == false`.
  - `browse_install_takes_point` (existing browse test setup): the point has `Installed{..}`.
- [ ] **Step 2: Run** `cargo test -p launcher-app`. Expected: FAIL.
- [ ] **Step 3: Implement**, then run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(app): go back, update hold and restore points around changes`.

### Task 6: UI

**Files:**
- Modify:
  - `ui/src/ipc/schema.ts`: `restorePoint`, `exitInfo.restore` (or the `restore` fix kind), `instance.update_hold`;
  - `ui/src/ipc/commands.ts` and `commands.test.ts`: `restorePoint(id)`, `restoreGoBack(id)`, `packUpdateAgain(id)`;
  - `ui/src/ipc/mock.ts`: the cases for the three commands. Seed one instance with a `beforeChange` point and one held server instance;
  - `ui/src/features/home/InstanceMenu.tsx`;
  - `ui/src/features/home/UpdatePlate.tsx`;
  - `ui/src/features/activity/CrashCard.tsx`;
  - `ui/src/i18n/en.json` and `ui/src/i18n/es.json`.
- Create:
  - `ui/src/features/home/GoBackDialog.tsx` and `GoBackDialog.test.tsx`;
  - `ui/src/features/home/useRestorePoint.ts`: fetches `restorePoint(id)`, refetches on `run_state` idle and after go-back.

**Copy (en; es is left to the builder per spec §7, suggested in the spec):**
- `restore.menu`: "Go back to how it was…"
- `restore.undoMenu`: "Undo going back…"
- `restore.updateAgain`: "Update again"
- `restore.title`: "Go back to how {{name}} was?"
- `restore.undoes`: "This undoes: {{list}}"
- `restore.safe`: "Your worlds, screenshots and game settings stay as they are."
- `restore.size`: "The restore point uses {{size}}."
- `restore.serverWarning`: "{{server}} uses the newer mods. Until you update again, you may not be able to join."
- `restore.confirm`: "Go back"
- `restore.done`: "{{name}} is back to how it was."
- `restore.heldServer`: "You went back to the earlier mods. {{server}} uses newer ones, so joining may not work."
- `restore.held`: "Updates are paused because you went back."
- `restore.change.packUpdate`: "updated to {{to}}"
- `restore.change.installed`: "installed {{name}}"
- `restore.change.removed`: "removed {{name}}"
- `restore.more`: "and {{count}} more"
- `restore.plateAction`: "Go back"
- `errors.restore_point_gone.message`: "There is nothing to go back to."
- `errors.restore_failed.message`: "Couldn't go back to how it was."

- [ ] **Step 1: Write the failing Vitest tests:**
  - `GoBackDialog`:
    - lists at most 3 changes plus "and N more";
    - hides the size under 1 MB;
    - shows the server warning only with `server`;
    - "Go back" calls `restoreGoBack`, then toasts.
  - `InstanceMenu` (in `Home.test.tsx`):
    - there is no "Go back" item when `restorePoint` is null;
    - the item is present for `beforeChange`;
    - it is **absent** (not disabled) while running;
    - "Undo going back…" appears for `beforeGoingBack` on a plain instance;
    - "Update again" appears for a held instance.
  - `UpdatePlate`:
    - a held server instance shows `heldServer` and an "Update again" button that calls `packUpdateAgain`;
    - after a sync with a point, a "Go back" text action appears.
  - `CrashCard`: "Go back to how it was" appears when `exit.restore` (or the `restore` fix) is set, and opens the dialog.
  - `contract.test.ts` parses the regenerated fixtures.
- [ ] **Step 2: Run** `pnpm --filter ui test`. Expected: FAIL.
- [ ] **Step 3: Implement**, with styling through `/impeccable`:
  - the plate action is a quiet text button, not a second lit button;
  - the held line can't be dismissed.
- [ ] **Step 4: Run** `pnpm --filter ui test && pnpm -r lint && pnpm -r typecheck`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(ui): go back to how it was, update again`.

### Task 7: Verify and ship

- [ ] Run every check in `CLAUDE.md` (`pnpm --filter ui build`, `cargo fmt --all`, clippy, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`). All must be green.
- [ ] Take screenshots in `dev:mock` with Playwright (`/opt/pw-browsers/chromium`):
  - the menu item;
  - the dialog;
  - the held plate;
  - the crash card.
- [ ] Get a whole-branch code review, and fix what it finds.
- [ ] Merge the latest `main`, re-run the checks, push, and get CI green.
