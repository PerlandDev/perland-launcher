# Crash Doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a crash, name the cause in plain words (mods named) and offer the fixes the launcher can do itself, as a list with one primary.

**Architecture:** `launcher-core::launch::crash` reads the redacted evidence (tail, crash report, latest.log, hs_err) after a crash exit, runs an ordered regex rule table, resolves mod ids to jars, and returns a typed `Crash` that builds a `LauncherError`. `launcher-app` turns it into `ExitInfo { cause, fixes }`. The UI's `CrashCard` renders the cause by code and the fixes in order.

**Tech Stack:** Rust (`regex`, `zip`, `toml`, `serde_json`, tokio), React + zod + i18next, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-21-crash-doctor-design.md`

## Global Constraints

- `launcher-core` and `launcher-app` never depend on Tauri; `src-tauri` is wiring only (one new command `play_repair`).
- Every evidence text passes through the run's `Redactor` before any rule reads it; `detail` holds only redacted lines.
- Evidence caps: crash report first 256 KiB, latest.log last 512 KiB, hs_err first 128 KiB; files modified before the launch are ignored.
- New `ErrorCode`s: `CrashOutOfMemory`, `CrashGraphicsDriver`, `CrashJavaTooOld`, `CrashJavaTooNew`, `CrashModMissingDependency`, `CrashModsClash`, `CrashModWrongVersion`, `CrashModFailed`, `CrashModFileDamaged`, `CrashGameFilesDamaged`; each has `errors.<code>.message` and `.fix` in en and es.
- Rule order (first match over any source wins): out_of_memory, graphics_driver, java_too_old, java_too_new, mod_missing_dependency, mods_clash, mod_wrong_version, mod_failed, mod_file_damaged, game_files_damaged.
- Card fixes are never amber: first raised (`secondary`), the rest `ghost`; Copy for support `ghost`.
- Contract JSON regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.
- Before each commit: the CLAUDE.md check list.

## Review Focus

- A crash whose `crash-reports/` holds only an older report: must stay generic (Task 2 test `stale_crash_report_is_ignored`).
- A mod named in a log that isn't in `mods/` (already removed, or a jar-in-jar): the card names it but offers no Turn off (Task 3 test `unknown_mod_has_no_file`).
- A pack-owned mod as the culprit: no Turn off button (Task 5 test `pack_mod_gets_no_disable`).
- A token or home path inside a log line that becomes `detail`: redacted (Task 2 test `evidence_is_redacted`).
- A huge or binary `latest.log`: read capped, never panics on invalid UTF-8 (Task 2 test `invalid_utf8_is_lossy`).

---

### Task 1: Rule table and fixtures

**Files:**
- Delete: `crates/launcher-core/src/launch/crash.rs`
- Create: `crates/launcher-core/src/launch/crash/mod.rs`, `rules.rs`, `testdata/<code>.hit.txt` + `<code>.miss.txt` for the 10 codes
- Modify: `crates/launcher-core/Cargo.toml` (+`regex`), root `Cargo.toml` workspace deps, `crates/launcher-core/src/error.rs` (codes), `NOTICE`

**Interfaces (produces):**
```rust
pub enum Source { Tail, CrashReport, LatestLog, HsErr }
pub struct Evidence { pub source: Source, pub text: String }
pub struct ModRef { pub id: String, pub name: Option<String>, pub file: Option<String> }
pub enum Vendor { Amd, Nvidia, Intel }            // serde snake_case
pub enum CrashCause {
    OutOfMemory, GraphicsDriver { vendor: Option<Vendor> },
    JavaTooOld { required: u32, running: Option<u32> }, JavaTooNew,
    ModMissingDependency { module: ModRef, missing: String },
    ModsClash { module: ModRef, other: ModRef },
    ModWrongVersion { module: ModRef }, ModFailed { module: ModRef },
    ModFileDamaged { file: String }, GameFilesDamaged,
}
/// A matched mixin config, still to resolve to its jar (Task 3).
pub struct Crash { pub cause: CrashCause, pub detail: String, pub mixin_config: Option<String> }
pub fn classify(evidence: &[Evidence]) -> Option<Crash>   // rules.rs, pure
impl Crash { pub fn code(&self) -> ErrorCode; pub fn error(&self) -> LauncherError }
```
`error()` params: `mod`, `other`, `missing`, `required`, `running`, `vendor` (names: `ModRef.name` else `id`). Category: java causes `Java`, mod causes `Loader`, damaged `Integrity`, others `Game`.

- [ ] Step 1: fixture test `every_rule_has_a_hit_and_a_near_miss` + `fixtures_classify_as_named`: for each code in `RULE_CODES`, load `testdata/<code>.{hit,miss}.txt` (`include_str!` via a `fixture(code, kind)` match), split on `\n---\n`, read `# source: …` and `# param k=v` headers; each hit case → `classify` yields that code and `error().params` contains every `param`; each miss case → not that code.
- [ ] Step 2: write the 20 fixture files (tiny, from real formats: Fabric 0.15 `- Mod 'Sodium' (sodium) 0.5.8 requires version 1.20.1 of 'Minecraft' (minecraft), but only the wrong version is present: 1.20.4!`, FML `Mod ID: 'flywheel', Requested by: 'create', … Actual version: '[MISSING]'`, HotSpot `# C  [atio6axx.dll+0x1a2d4a]`, etc.). Near-misses: `unable to create native thread`; `# C  [jvm.dll+0x…]`; `NoClassDefFoundError`; dep on `minecraft` (must be wrong_version, not missing); `Reference map … could not be read`; zip error naming no path.
- [ ] Step 3: run `cargo test -p launcher-core crash` → FAIL.
- [ ] Step 4: implement rules with `std::sync::LazyLock<Regex>`; required Java from class file version = N − 44; mod_failed takes `from mod <id>` when printed, else keeps `mixin_config` for Task 3 and an id-less `ModRef` is not emitted (no config and no id → no match).
- [ ] Step 5: tests pass; clippy; commit "Recognise common crash causes from the game's logs".

### Task 2: Evidence

**Files:** Create `crates/launcher-core/src/launch/crash/evidence.rs`.

**Interfaces (produces):** `pub async fn gather(game_dir: &Path, pid: Option<u32>, started: SystemTime, tail: &[String], redactor: &Redactor) -> Vec<Evidence>` (tail first, then crash report, latest.log, hs_err).

- [ ] Tests (tempdir): `newest_fresh_crash_report_is_read`, `stale_crash_report_is_ignored` (mtime set before `started` with `File::set_modified`), `latest_log_reads_the_last_part` (cap via a `pub(crate) const`), `hs_err_matches_the_pid`, `evidence_is_redacted` (secret + home in latest.log → `<redacted>`/`~`), `invalid_utf8_is_lossy`.
- [ ] Implement with `spawn_blocking` + `std::fs`; missing/unreadable files are skipped. Commit.

### Task 3: Mods on disk, and the wiring into `spawn`

**Files:** Create `crates/launcher-core/src/launch/crash/mods.rs`; modify `crash/mod.rs`, `launch/mod.rs` (`GameExit.crash: Option<Crash>`, `spawn_with_log` records `SystemTime::now()` before spawn, keeps a redactor clone, calls `crash::diagnose`).

**Interfaces (produces):**
```rust
pub struct JarMod { pub file: String, pub id: String, pub name: Option<String> }
pub fn scan(game_dir: &Path) -> Vec<JarMod>                          // enabled jars only
pub fn owner_of_entry(game_dir: &Path, entry: &str) -> Option<String> // file containing a root entry
pub async fn diagnose(game_dir: &Path, pid: Option<u32>, started: SystemTime, tail: &[String], redactor: &Redactor) -> Option<Crash>
```
`diagnose` = gather → classify → resolve every `ModRef` (fill `file`, `name` from `scan`; `mixin_config` → `owner_of_entry` → that jar's id/name) → for `ModFileDamaged` keep the file name.

- [ ] Tests: `scan_reads_fabric_quilt_and_forge_ids` (zip built in test, `fabric.mod.json`, `quilt.mod.json`, `META-INF/mods.toml` with `[[mods]] modId/displayName`), `disabled_jars_are_skipped`, `mixin_config_finds_its_jar`, `unknown_mod_has_no_file`, `diagnose_names_the_mod_from_its_jar`.
- [ ] Update the existing launch tests and `launcher-app` users of `GameExit.crash`. Commit.

### Task 4: Repair re-verifies by hash

**Files:** `crates/launcher-core/src/play.rs` (`prepare(installer, instance, repair: bool, sink)`), `install/forge.rs` (`pub async fn forget_processors(dirs: &LauncherDirs, resolved: &ResolvedVersion) -> Result<()>` removes `MARKER`, absent is fine), `crates/launcher-app/src/runner.rs` (`Runner::prepare(&self, instance, repair: bool, sink)`; `CoreRunner` with repair builds an `Installer { downloader: downloader.clone().with_existing_check(ExistingCheck::Hash), .. }`), `runs.rs` (`PlayArgs.repair`, `pub async fn play_repair(self: &Arc<Self>, id: &str) -> Result<()>`), `testing.rs` (FakeRunner records `repairs: Mutex<Vec<bool>>`), `src-tauri` (`play_repair` command + handler), perland-cli caller.

- [ ] Tests: `forget_processors_removes_the_marker` (core), `play_repair_prepares_with_repair` and `play_does_not_repair` (app). Commit.

### Task 5: Cause and fixes on `ExitInfo`

**Files:** `crates/launcher-app/src/events.rs` (`cause`, `fixes`, `CrashFix`), new `crates/launcher-app/src/crash_fixes.rs`, `runs.rs`, `tests/contract.rs`, `ui/src/ipc/contract/*` (regenerated).

**Interfaces (produces):**
```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CrashFix { MoreMemory, RecommendedJava, JavaSettings, FindMod { project: String },
    DisableMod { file: String, name: String }, Repair, UpdateDrivers { vendor: Vendor } }
pub(crate) fn fixes_for(crash: &Crash, instance: &Instance, managed: &HashSet<String>, recommended_java: Option<u32>) -> Vec<CrashFix>
pub(crate) fn modrinth_project(id: &str) -> String   // alias list from spec §10
```
Rules as spec §7: clash → disable `other` then `module`; java too old with custom path and `recommended >= required` → [RecommendedJava, JavaSettings]; else [JavaSettings]; too new with custom path → [RecommendedJava] else [JavaSettings]. `managed` = lower-cased `mods/<file>` keys of the pack's `Managed`.

- [ ] Tests in `crash_fixes.rs` (one per cause incl. `pack_mod_gets_no_disable`, `java_too_old_on_automatic_opens_settings`, `fabric_module_ids_map_to_fabric_api`); `runs.rs` test `crash_carries_cause_and_fixes` (FakeRunner exit with an OOM `Crash` → `ExitInfo.cause.code == CrashOutOfMemory`, `fixes == [MoreMemory]`).
- [ ] Contract sample: one crash exit with a cause and two fixes; regenerate; commit.

### Task 6: UI schema, mock, strings

**Files:** `ui/src/ipc/schema.ts` (`crashFix` discriminated union, `exitInfo.cause`, `exitInfo.fixes`, drop `crashHint`), `ui/src/ipc/commands.ts` (`playRepair`), `ui/src/ipc/mock.ts` (`?crash=<code>` picks a sample cause; `play_repair` scripts a run), `ui/src/main.tsx`, `en.json`/`es.json` (`errors.crash_*`, `activity.fix.*`, toasts).

- [ ] `pnpm --filter ui test contract` passes against regenerated JSON; mock test `crash_param_picks_the_cause`. Commit.

### Task 7: The crash card

**Files:** `ui/src/features/activity/CrashCard.tsx`, new `ui/src/features/activity/CrashFixes.tsx`, `CrashCard.test.tsx`; drivers URLs in `CrashFixes.tsx`:
`amd → https://www.amd.com/en/support/download/drivers.html`, `nvidia → https://www.nvidia.com/en-us/drivers/`, `intel → https://www.intel.com/content/www/us/en/support/detect.html`.

- [ ] Tests: `known_cause_shows_message_and_fix_line`, `unknown_cause_keeps_the_generic_card`, `first_fix_is_raised_others_ghost`, `disable_mod_turns_it_off_and_dismisses`, `find_mod_shows_only_when_on_modrinth`, `more_memory_without_room_opens_settings`, `recommended_java_sets_automatic`, `repair_starts_a_repair_run`, `details_show_redacted_lines`.
- [ ] `/impeccable` critique + polish against `.impeccable/surfaces/ui-src-app-tsx.md`; record the decision in the surface file. Commit.

### Task 8: Ship

- [ ] Full check list; screenshots via Playwright on `dev:mock?crash=…`; code review; draft PR; CI green; merge `main`, re-check, merge; report to the master session.
