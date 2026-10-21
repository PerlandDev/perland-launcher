# Perland Launcher: Undo Last Update, Design

- **Date:** 2026-10-21
- **Builds on:**
  - `docs/superpowers/specs/2026-10-10-pack-sync-design.md` (sync, the 7-day trash, `managed.json` / `pack.json`);
  - `docs/superpowers/specs/2026-10-16-link-instance-design.md` (`mods-before-link/`, server instances);
  - `docs/superpowers/specs/2026-10-11-browse-design.md` (Browse installs);
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the brief told us to decide the open questions ourselves. Every decision the brief does not settle is marked **our call**.
- **Out of scope:** issues #80 and #81 (hand-uploaded mods on servers).

## 1. Why

Pack sync, Browse installs and mod removals now change an instance's mods for the player, often without them asking. When a change breaks something (the game crashes on start, or a mod they relied on is gone), a casual player has no way back. Asking on Discord is the only fix today.

The bar: a 12-year-old whose game stopped starting after an update presses one button and plays again, and is never left unable to join their server without being told why.

## 2. Outcome

- **Before a change:** before the launcher changes an instance's mods, it keeps a **restore point**: how the touched folders, the pack lockfile and the game version were before.
- **One action, "Go back to how it was":** this action restores the point. It appears in three places, and only when there is something to go back to:
  1. the instance menu;
  2. the after-update plate on Home ("Updated to 1.4.2 · Go back");
  3. the crash card, when the game crashes on the first run after a change.
- **Never touched:** `saves/`, `screenshots/` and `options.txt` are never snapshotted and never restored. Neither are `logs/` or `crash-reports/`.
- **Pack instances:** going back on an instance that updates from a pack (a Perland server, a Modrinth link or a `.mrpack`) **holds updates** for that instance. The instance stops updating until the player presses **Update again**. Home says this plainly and says that joining the server may not work meanwhile.
- **Going back can be undone:** going back keeps the state it replaced as the new restore point. On a pack instance, "Update again" brings the update back. On other instances, "Undo going back" does.

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - a restore point before every sync that changes files, every Browse install into an existing instance, and every mod or resource/shader pack removal;
  - one "Go back" action in the menu, on the after-update plate and on the crash card;
  - an honest size, and near-zero disk cost where hard links work;
  - a restore that leaves the instance playable when interrupted;
  - a plain hold-and-update-again flow for pack and server instances;
  - every string in en and es;
  - working in `dev:mock`.
- **Non-goals:**
  - a history of many restore points or a picker (one point per instance, §5.4);
  - undoing a single mod out of a larger change;
  - backing up worlds (a different feature);
  - restore points for new instances (the first install, a pack add, a folder import), which have nothing to go back to;
  - enabling or disabling a mod, and adding the player's own jar (`mods_add`): the player undoes these themselves in one click, so neither takes a restore point (**our call**).
- **Anti-goals:**
  - a disabled or greyed "Go back" button with no explanation;
  - touching a world, a screenshot or `options.txt`;
  - a half-restored instance that crashes or won't start;
  - a server instance that silently stops matching its server;
  - a restore point that silently doubles the instance's disk use;
  - an update blocked because a restore point could not be made.

## 4. Facts this rests on (`main`, 6e16088)

- **Folders:** `InstanceStore::perland_dir(id)` is `<instance>/.perland/` and the game dir is `<instance>/minecraft/`. Both sit in the same instance folder, so they are always on one volume. A rename moves them together and a delete removes them, the same way `playtime.rs` works.
- **What sync writes** (`sync/mod.rs` `sync_inner`):
  - It stages everything, writes `pack.json`, then swaps.
  - Replaced and removed files it placed move to `.perland/trash/<unix-secs>/`, which is purged after 7 days.
  - The player's files at pack paths move to `minecraft/mods-before-link/`.
  - It can change `Instance.game` (Minecraft and loader version).
  - `managed.json` is written last.
- **Sync never rewrites a file in place:** it renames staged files into place. Browse installs hard-link from a temp file (`link_no_replace`). So a jar's inode never changes content after placement, which is what makes hard-link snapshots of jars safe.
- **Not every removal is reversible today:** `mods::remove` and `content::remove` delete files outright.
- **When sync runs:** only inside a run, on Play or the banner's Update (`runs.rs`, phase `Syncing`). The instance lock is held for the whole run. `UpdatePolicy` is Auto / Ask / Pinned.
- **Crashes:** `ExitInfo::is_crash` (launcher-app `events.rs`) decides the crash card. The only crash hint today is out of memory (`launch/crash.rs`).
- **`fsutil`** has `write_atomic`, `rename_with_retry` and `is_in_use`. The `same-file` crate is already in `Cargo.lock` (transitively). Depending on it directly is fine.

## 5. Design

### 5.1 What a restore point holds

A restore point is a folder at `<instance>/.perland/restore/point/`:

```
point/
  point.json      what, when, why, and the bookkeeping below
  files/<top>/…   a snapshot of each top-level folder in scope, as it was
  managed.json    copy of .perland/managed.json, if there was one
  pack.json       copy of .perland/pack.json, if there was one
```

`point.json` (camelCase, versioned):
- `version: 1`;
- `createdAt` (unix secs);
- `kind`: `beforeChange` or `beforeGoingBack`;
- `scope`: top-level folder names in `files/`;
- `game`: the instance's `GameSpec` at snapshot time;
- `source`: the instance's `SourceRef` at snapshot time (an update can rewrite it to name the new version);
- `optionalChoices`: the instance's optional choices at snapshot time;
- `packVersion`: the version from `managed.json`, if any, for display;
- `changes`: the list of what happened after the point was taken, for display. Each entry is one of:
  - `{kind: "packUpdate", from, to}`
  - `{kind: "installed", name}`
  - `{kind: "removed", name}`
  
  Entries are appended as changes happen, and the UI shows at most three plus "and N more".
- `sealed: bool`: set when the game is launched after the point was taken (§5.4);
- `lastChangeAt` (unix secs): when the last change was appended;
- `cleanRunSince: bool`: set when a run that **started after `lastChangeAt`** ends without a crash (§5.7); appending a change resets it to false;
- `files`: per file, `{path, size, mtime, linked: bool}` (used for the size and for the restore check).

**Scope (our call).**
- **For a sync:** the set of top-level folders named by any path in `plan.write`, `plan.trash` or `plan.case_aliases`.
- **For a Browse install or a removal:** the kind folder: `mods`, `resourcepacks` or `shaderpacks`.
- **Always out of scope:** the denylist `saves`, `screenshots`, `logs`, `crash-reports`, `mods-before-link`, and any folder whose name starts with `.`. A root-level file (`options.txt`, `servers.dat`, …) is never in scope.
- **Root-level files a pack writes:** if a pack writes a root-level file, sync changes it but going back does not restore it. That is the price of never touching `options.txt`. It is listed in §8.

### 5.2 Disk cost: hard links, not copies

- **Mod, pack and shader archives are hard-linked** into the snapshot. These are files ending `.jar`, `.zip` or `.disabled` (after any `.disabled` suffix the base is still an archive).
  - This costs nothing while the instance still has the same file.
  - It costs the file's size only once the change removes or replaces it. The 7-day trash already held that file anyway, through another link to the same inode.
- **Everything else is copied.** That is mostly `config/`, small text files that the game and players rewrite in place, where a hard link would let the edit leak into the snapshot.
- **When hard linking fails** (FAT32/exFAT, some network drives, or an `EXDEV`/unsupported error on the first link), the whole point falls back to copying. Before copying, the launcher checks the free space with the existing `apply::free_space` and `SPACE_MARGIN`.
- **No space for a point:** if there is not enough space for the copy, or the copied part would pass **1 GiB**, the change goes ahead **without** a restore point (**our call**: blocking an update can keep a player off their server). The event log notes it, and the after-update plate shows nothing to undo, since nothing can be undone.
- **The size we show is honest:** it counts every copied file, plus every linked file that the game folder no longer holds as the very same file. The `same-file` crate compares by inode on Unix and by file index on Windows. It is computed when asked, not stored.
- **Pruning:** a point is deleted:
  - when a newer point replaces it (§5.4);
  - when it is 30 days old (checked when a point is read or made);
  - with the instance.

### 5.3 Making a point safely

`restore::prepare(...)` runs under the instance lock, before the first byte of the change moves:

1. Remove any leftover `restore/new/`.
2. Build the snapshot in `restore/new/`, then write `point.json` there with `write_atomic`.
3. Swap it in:
   - rename `point/` to `restore/old/` if it exists;
   - rename `new/` to `point/` (with `rename_with_retry`);
   - delete `old/`.

   An interruption leaves either the old point or the new one, plus leftovers that the next prepare removes.
4. If anything fails, log it, remove `new/`, and let the change go ahead without a point (as in §5.2).

Extending an unsealed point with a new folder (§5.4) builds that folder in `restore/new-<top>/`, renames it into `point/files/<top>`, then rewrites `point.json`.

### 5.4 What "the last update" means

There is **one restore point per instance** (**our call**). It means "how it was when you last played".

- A change takes a restore point **only if there is no unsealed point**. The point is sealed when the game launches. So every change between two Plays (five Browse installs in a row, or a removal followed by a sync on Play) lands in the same point, and going back undoes all of them at once.
- **When an unsealed point exists:**
  - a new change appends to its `changes`;
  - if the change touches a top-level folder that is not yet in the point's `scope`, that folder is snapshotted now and added. It has not changed since the point was taken, unless the player changed it by hand.
- **When the only point is sealed:** a change replaces it.
- **Display:** "the last update" is shown as the point's `changes`, for example "Updated to 1.4.2, installed Sodium and 2 more".
- **Order on Play:** Play → sync (takes the point if files change) → launch (seals it) → the run ends.
- **Changes while the game runs** (**our call**). Browse installs are allowed while running (`content_guard`, browse spec §4.2):
  - if a point exists, sealed or not, the install appends to it and does not replace it;
  - if no point exists, it takes a new one, sealed at once.
  - Either way `lastChangeAt` moves past the run's start, so that run's clean exit does not mark the point as tested.
  - Going back then undoes the mid-game install together with the rest of the point.
- **A resumed sync:** a sync that resumes an interrupted swap takes **no** snapshot. It is recognised by `pack.json`'s stamp differing from `managed.json`'s stamp, the state `save_pack`-before-swap leaves behind.
  - The interrupted attempt already took the point before `save_pack`. The resume only appends its `changes` entry, deduplicated, and never extends the scope.
  - Without a point (for example the first attempt had no space), it goes ahead without one.

### 5.5 Going back

`restore::go_back(store, id)` runs under the instance lock (refused with the existing in-use error while running or busy).

1. **Recover** any interrupted restore first (§5.6).
2. **Take a `beforeGoingBack` point of now**, over the same scope, with the current `managed.json`/`pack.json`/game/choices. It is built in `restore/new/` as in §5.3, but **not swapped in yet**.
3. **Stage the restored folders:** for each folder in scope, build `minecraft/.<top>.perland-restore/` by hard-linking (or copying, as in §5.2) from `point/files/<top>`.
   - Copied files are copied again, so the point stays intact until the end.
   - A linked file whose size or mtime no longer matches `point.json` was changed in place by someone. It is skipped, the current file is kept, and the result counts it (**our call**: a partial match is better than a dead end).
4. **Write the journal** `restore/restoring.json` (`write_atomic`) listing the folders and step `swapping`.
5. **Swap each folder** (`rename_with_retry`):
   - `minecraft/<top>` → `restore/aside/<top>`;
   - `minecraft/.<top>.perland-restore` → `minecraft/<top>`.

   A folder that did not exist at snapshot time is moved aside and not replaced.
6. **Put back the bookkeeping:** `managed.json` and `pack.json` (or delete them if the point had none), and `game`, `source` and `optionalChoices` into `instance.json`. Reload, modify and save, so a rename meanwhile is kept.
7. **Hold updates** (§5.8) if the instance has a pack source.
8. **Undo the setting aside:** a jar in `minecraft/mods-before-link/` that is the very same file (`same-file`) as one now back in `mods/` is removed from `mods-before-link/`. Otherwise "Update again" would set it aside a second time.
9. **Finish:**
   - swap the `beforeGoingBack` point in as the new `point/` (§5.3 step 3);
   - delete `restore/aside/` and the journal.

Every step before 5 only writes into `restore/` and dot-folders, so failing there changes nothing the game sees. Steps 2 and 3 count against free space as in §5.2.

### 5.6 Interrupted restores

The journal makes a restore all or nothing:

- **Recovery** runs whenever the instance lock is next taken by `go_back`, by Play/`start_run` before sync, or by `pack_update`.
  - With no journal, recovery deletes any `minecraft/.*.perland-restore/` and `restore/aside/` leftovers.
  - With a journal, every staged folder was complete before the journal was written, so recovery **rolls forward**: it finishes steps 5–9 for the folders not yet swapped. A folder is not yet swapped when `.<top>.perland-restore` still exists.
- **When recovery can't finish** (a folder still in use): it returns the in-use error and Play is refused. This is the one case we block, because launching a half-swapped instance is the anti-goal. The error's fix says "Close any window showing the instance's folder, then try again".
- **Required test:** an interrupted restore, simulated by stopping after each step, followed by recovery, always ends in either the before state or the after state, never a mix.

### 5.7 Where "Go back" appears

The UI asks `restore_point(id) → RestorePointDto | null`. It returns null when there is no point, the point is older than 30 days, or the point is damaged.

- **Instance menu:**
  - "Go back to how it was…" when the point is `beforeChange`;
  - "Undo going back…" when it is `beforeGoingBack` on an instance without a pack source;
  - nothing otherwise.

  On a pack instance after going back, "Update again" shows instead, on the plate and in the menu (§5.8). The item is **hidden** (not disabled, unlike Change image, which is disabled) while the instance is running or busy. It reads the same `useRun` state.
- **After-update plate** (`UpdatePlate.tsx`): when `lastExit.synced` reports changes and an unsealed `beforeChange` point exists, the plate adds a quiet "Go back" text action next to "Updated to X". It is not a second lit button.
- **Crash card:** when `ExitInfo::is_crash` and the point is `beforeChange` with `cleanRunSince: false`, the card offers "Go back to how it was".
  - **Building on Crash doctor:** the parallel Crash doctor work (branch `claude/crash-doctor`) owns the crash card's layout and hint model. In that model, `ExitInfo.fixes: Vec<CrashFix>` is rendered in order, the first one raised.
    - This feature adds `CrashFix::Restore`, pushed in `runs.rs` after the exit, also when no cause is recognised.
    - It goes **first** when no cause is recognised, and **after** the cause's own fixes otherwise, so a recognised cause (out of memory, a missing mod) keeps its specific fix as the primary one.
  - **If this merges before Crash doctor:** `ExitInfo` gets a minimal `restore: bool`, and the card renders one more action next to "Raise memory" (memory first for out of memory). Crash doctor folds it into `fixes` when it merges.
  - When a run ends without a crash, the launcher sets `cleanRunSince: true` on the point.
- **Confirm dialog** (all three places open it):
  - **Title:** "Go back to how {name} was?"
  - **Body:** the `changes` list ("This undoes: Updated to 1.4.2, installed Sodium and 2 more").
  - **Reassurance:** "Your worlds, screenshots and game settings stay as they are."
  - **Size:** "The restore point uses {size}." It is omitted when under 1 MB.
  - **Server warning, for a server instance:** "{server} uses the newer mods. Until you update again, you may not be able to join."
  - **Buttons:** "Go back" (primary) and Cancel.
- **After going back,** a toast says "{name} is back to how it was."

### 5.8 Pack and server instances: the hold

Going back cannot win against a server: the next sync re-applies the server's pack. So going back on any instance that checks for updates (`checks_for_updates`: a source, a policy other than Pinned, and a `pack.json`) sets `Instance.update_hold = Some({since, version})` in `instance.json`, where `version` is the restored pack version (**our call**: an explicit hold, separate from the player's `UpdatePolicy`, so their setting is untouched).

**While held:**

- **Sync is skipped:** Play launches with the restored files. `runs.rs` treats a held instance like Pinned for `checks_for_updates`.
- **Server instance plate:** the update plate on Home shows a persistent line that can't be dismissed: "You went back to the earlier mods. {server} uses newer ones, so joining may not work. **Update again**".
  - It stays visible while held, including after a Play.
  - On a non-server pack instance it reads "Updates are paused because you went back. **Update again**".
- **Instance menu:** "Update again" replaces "Check for updates".
- **Joining anyway:** when the run joins a server (play-and-join) while held, the game launches as usual (the player was warned). If the run ends with a crash or within 60 seconds, the crash/exit card repeats the hold line with "Update again".
- **Update again:**
  - clears the hold and starts the update-only run `pack_update`;
  - that sync takes a fresh `beforeChange` point, so the player can go back once more.
- **The hold clears by itself** when `pack_status` finds the source's version equal to the held `version`, for example because the server went back too. There is nothing left to differ.
- **No dead end:** the player can always see why they might not join, and the fix sits one button away.
- **Pinned instances get no hold:** Play only repairs them toward the stored `pack.json`, and going back restored that file too, so the restored state is stable.
  - Their menu shows "Undo going back" like an instance without a pack source.
  - If the player later switches such an instance to Auto or Ask, the next Play updates as usual. They chose that.
- **The hold blocks the remote check only:** Play's repair toward the restored `pack.json` and `managed.json` still runs, and it matches the restored files.

### 5.9 Hooks into existing changes

| Change | Where | Scope | `changes` entry |
|---|---|---|---|
| Sync that writes or trashes anything, when `managed.json` existed | `sync_inner`, after staging and before `save_pack` | top folders of the plan | `packUpdate {from, to}` |
| Browse install into an existing instance (it only adds files; there is no Browse update flow today, and one added later hooks in the same way) | `browse::install`, before linking | kind folder | `installed {name}` per project |
| `mods::remove`, `content::remove` | before the delete | `mods` / kind folder | `removed {name}` |

- **Removals take the lock:** `mod_remove` and `content_remove` today only check `is_busy`. They now take `InstanceLock::try_acquire` for the snapshot and the delete, refusing with the existing in-use error while installing or running.
  - Browse installs during a run keep relying on the run's lock (the run holds it), as today.
- **Removals stay hard deletes:** the snapshot already holds a link to the file, so a removal costs no extra space. It now costs the file's size only until the point is replaced.
- **Sealing:** the run state machine seals the point (`restore::seal`) when it reaches `Running`, and sets `cleanRunSince` when a run ends without a crash.

### 5.10 Code layout

- **Core** (`crates/launcher-core/src/restore.rs`, or `restore/` if it grows), with no app or UI types:
  - `prepare`, `note_change`, `seal`, `mark_clean_run`, `read`, `size`, `go_back`, `recover`;
  - the scope and denylist rules;
  - unit tests on temp dirs, kept tiny.
  - New `ErrorCode`s: `restore_point_gone` (the point vanished or is damaged) and `restore_failed` (with detail). Each gets `errors.<code>.message` in en and es.
- **App** (`crates/launcher-app`):
  - commands `restore_point(id)`, `restore_go_back(id)` and `pack_update_again(id)` (clears the hold, then the existing `pack_update`);
  - `RestorePointDto { kind, createdAt, changes, sizeBytes, hold }`;
  - `update_hold` on the instance DTO;
  - contract samples regenerated with `UPDATE_CONTRACT=1`.
- **Tauri:** wiring only.
- **UI:**
  - the menu items in `useInstanceActions`;
  - a `GoBackDialog`;
  - the plate and crash card additions;
  - the zod schemas, `commands.ts` bindings and `mock.ts` cases. The mock seeds one instance with a point and one held server instance.
- **Styling:** goes through `/impeccable` per `CLAUDE.md`.

## 6. Constraints

- **TDD as `CLAUDE.md` says:** core unit tests for scope, linking, prepare, coalescing, go-back, recovery after each step, and hold; app tests for the commands and the hold in `runs.rs`; Vitest for the dialog, the menu item's presence and absence, the plate, and the crash card.
- **No large test inputs:** a "big mods folder" test uses a handful of tiny files.
- **Logs:** no tokens in logs. Restore logs name folders and counts, not file contents.
- **Locks:** everything runs under the existing `InstanceLock`, with no new lock. Removals start taking it (§5.9). `restore` lives in launcher-core so it can use `apply::free_space` and `SPACE_MARGIN`.

## 7. Left to the builder

- **Exact copy:** the exact Spanish wording (neutral Latin American, "tú"). Suggested: "Volver a como estaba", "Actualizar de nuevo", "Deshacer la vuelta atrás".
- **Module split:** whether `restore` is one file or a folder.
- **Size format:** how the size is formatted (reuse whatever the app uses for MB).

## 8. Known limits (said plainly)

- **Root-level files:** these, including `options.txt`, are never restored, even if the update changed them.
- **Files edited in place:** a jar a player overwrote in place after the update (rare) is kept as it is now, and the result says how many.
- **No hard links:** on file systems without hard links, a point costs a full copy of the folders in scope. Past 1 GiB, there is no point.
- **One step back:** "Go back" reaches only how it was when you last played, not any earlier state.
