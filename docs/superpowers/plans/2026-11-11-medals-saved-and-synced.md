# Medals saved and synced Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The "Medallas y logros" screen with 24 medals (14 need no account), every earned tier kept with its date on the computer and, with a Perland session, in the Perland account.

**Architecture:** PerlandEngine gains a `player_medals` table and `GET`/`PUT /api/v1/player/medals` that only store and merge (highest tier per row kept by the primary key, earliest `earned_at`). The launcher moves the medal rules from the UI into `launcher-core`, which counts the measures no file holds yet, dates every newly reached tier in `<data root>/medals.json`, and syncs it; the UI renders what the core returns.

**Tech Stack:** Go + Gin + GORM (Engine); Rust + Tokio (launcher-core, launcher-app); React + TanStack Query + Vitest (ui).

**Spec:** `docs/superpowers/specs/2026-11-11-medals-saved-and-synced-design.md` (read it; this plan does not repeat it).

## Global Constraints

- Two repositories, two branches: PerlandEngine `feat/player-medals` (Tasks E1–E3, ships first), perland-launcher `feat/medals-saved-synced` (Tasks L1–L6).
- Everything in the repositories in English; UI strings in `ui/src/i18n/en.json` and `es.json` (neutral Latin American Spanish, "tú"; medals feminine: "Conseguida el …").
- Medal ids, tiers and thresholds exactly as spec §5. Tier values 1–4; a single stores its rim tier (`firstGame` 1, `doctor` 2, `completionist` 4, `diamonds` 2, `nether` 2, `end` 3, `elytra` 3, `hero` 3, `dragon` 4, `wither` 4).
- Engine rules: id `^[a-z][A-Za-z0-9]{0,31}$`; tier 1–4; `earnedAt` ≥ 2026-01-01T00:00:00Z and ≤ now + 24 h; ≤ 200 entries per call; ≤ 200 rows per player (409, nothing written); body cap 32 KiB (413 `file_too_large`); 30 requests a minute per player; third-party session 403.
- Tiers are only ever added; a stored date only ever gets earlier.
- No toast, no Engine catalogue, no telemetry of launcher actions (spec §2).
- Before committing (launcher): `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`. Engine: `go build ./...`, `go vet ./...`, `go test ./...`, `golangci-lint run`.
- No pull request, release or GitHub comment without asking the owner.

## Review Focus

- A `medals.json` that is damaged, empty or from a newer version: the launcher starts, shows zero local medals, and the next write replaces it with a valid file. (Test in L2.)
- Two engines or two launchers sending the same medal with different dates at once: the earliest date wins and no row is duplicated. (Test in E1, the concurrent upsert case.)
- A player who signs in to account A, then B on the same computer: B never shows A's account-only medals, both keep the computer's own. (Test in L4.)
- An Engine without the routes (404) or with player accounts off (503): no error reaches the screen, local medals still show, sync retries next trigger. (Test in L4.)
- A run killed or crashed counts for `firstGame`, `days`, `versions` and `marathon` like a clean exit, but a crashed run never completes `doctor`. (Test in L3.)

---

## PerlandEngine (branch `feat/player-medals`)

### Task E1: `internal/playermedals` store

**Files:**
- Create: `internal/playermedals/medal.go` (entity, `Validate`), `internal/playermedals/store.go` (`Store` interface, `GormStore`), `internal/playermedals/playermedalstest/store.go` (`MemStore`), `internal/playermedals/store_suite_test.go`
- Modify: `internal/infrastructure/db/database.go` (AutoMigrate `&playermedals.Medal{}` next to `&playerservers.Link{}`), `.github/workflows/ci.yml` (`PLAYERMEDALS_TEST_DSN` beside the other SQL suites)

**Interfaces:**
- Produces:
  - `type Medal struct { PlayerID uuid.UUID; MedalID string; Tier int16; EarnedAt time.Time }` (table `player_medals`, primary key `(player_id, medal_id, tier)`).
  - `type Entry struct { ID string \`json:"id"\`; Tier int \`json:"tier"\`; EarnedAt time.Time \`json:"earnedAt"\` }`
  - `func Validate(entries []Entry, now time.Time) error` returning `*InvalidError{Field string}` (`medals[3].earnedAt: ...`).
  - `type Store interface { List(ctx, player uuid.UUID) ([]Entry, error); Merge(ctx, player uuid.UUID, entries []Entry) ([]Entry, error) }`; `Merge` returns `ErrTooMany` when the result would pass `MaxPerPlayer = 200`.
  - `const MaxPerCall = 200`, `MaxPerPlayer = 200`, `var Floor = time.Date(2026,1,1,0,0,0,0,time.UTC)`.

- [ ] **Step 1: Write the failing tests** — `TestValidate` table (bad id `"1x"`, `"a-b"`, 33 chars; tier 0 and 5; `earnedAt` 2025-12-31 and now+25h; 201 entries) each naming its field; `runStoreSuite(t, newStore)` with: list empty; merge inserts sorted by id then tier; merging an earlier date replaces, a later one does not; a second merge returns the union; 200 rows then one new → `ErrTooMany` and nothing written; two goroutines merging the same row with different dates → one row, earliest date. `TestMemStore` runs it; `TestGormStore` runs it on `PLAYERMEDALS_TEST_DSN` (skip without).
- [ ] **Step 2: Run** `go test ./internal/playermedals/...` — FAIL (package missing).
- [ ] **Step 3: Implement.** `GormStore.Merge`: one transaction, count existing + new keys under `SELECT … FOR UPDATE` of the player's rows, then `INSERT … ON CONFLICT (player_id, medal_id, tier) DO UPDATE SET earned_at = LEAST(player_medals.earned_at, EXCLUDED.earned_at)`, then `List`. Times stored in UTC, truncated to the second.
- [ ] **Step 4: Run** `go test ./internal/playermedals/...` (and with the DSN locally if Postgres is available) — PASS.
- [ ] **Step 5: Commit** `Add the player medals store`.

### Task E2: the player routes

**Files:**
- Create: `internal/playersapi/medals.go`, `internal/playersapi/medals_test.go`
- Modify: `internal/playersapi/routes.go` (`Config.Medals playermedals.Store`, routes, a `limitPerPlayer` limiter of 30/min), `cmd/server/players.go` (wire `playermedals.NewGormStore(db)`), `CLAUDE.md` (Players table rows, the store in the repository tree), `site/src/data/changelog.yaml` (newest date, areas `players`), `site/src/data/roadmap.yaml` (item "Medals saved to the Perland account", in-progress, paired launcher item), `site/spec/overlay.yaml` (examples), `docs/swagger.json` (regenerate)

**Interfaces:**
- Consumes: `playermedals.Store`, `Validate`, `ErrTooMany`, `InvalidError` (E1); `thirdParty(c)` (existing).
- Produces: `GET /api/v1/player/medals` → `{medals: [Entry]}`; `PUT /api/v1/player/medals` body `{medals: [Entry]}` → same shape, merged. `Config.Medals == nil` → 503 `player_accounts_disabled`.

- [ ] **Step 1: Write the failing tests** — with `MemStore` and a player session: GET empty → `{"medals":[]}`; PUT two entries → 200 with both; PUT same id/tier later date → date unchanged; PUT bad tier → 400 whose message starts `medals[0].tier`; PUT body over 32 KiB → 413 `file_too_large`; third-party session on both routes → 403; store at 200 rows plus one new → 409; 31st request in a minute → 429; no session → 401.
- [ ] **Step 2: Run** `go test ./internal/playersapi/ -run Medals` — FAIL.
- [ ] **Step 3: Implement** handlers `Medals` and `PutMedals` in `medals.go` (`http.MaxBytesReader` 32 KiB, `DisallowUnknownFields`), register `g.GET("/medals", on, player, budget, h.medalsOn, limitPerPlayer(medals), h.Medals)` and the PUT likewise. Swag annotations, tag `Players`.
- [ ] **Step 4: Run** `go test ./internal/playersapi/...`, `make swagger` (or the repo's swag command), `go run ./cmd/api-catalog`, `cd site && pnpm test && pnpm build` — PASS.
- [ ] **Step 5: Commit** `Add the player medals routes`.

### Task E3: account merge moves medals

**Files:**
- Modify: `internal/accountmerge/gorm_store.go` (steps beside "move Your servers rows"), `internal/accountmerge/service.go` (`Counts.Medals int \`json:"medals"\``), the merge SQL suite test, `CLAUDE.md` (merge bullet: `player_medals` moved, earliest date kept)

**Interfaces:**
- Consumes: table `player_medals` (E1).

- [ ] **Step 1: Write the failing test** in the merge suite: survivor has `time` tier 1 at day 5, absorbed has `time` tier 1 at day 2 and `tailor` tier 1 → after merge the survivor holds `time` 1 at day 2 and `tailor` 1; absorbed holds none; `Counts.Medals == 2`.
- [ ] **Step 2: Run** `ACCOUNTMERGE_TEST_DSN=… go test ./internal/accountmerge/` — FAIL.
- [ ] **Step 3: Implement** three statements in the transaction: update survivor's rows to the earlier date from the absorbed's matching rows; delete the absorbed's matching rows; move the rest. No cap applied (spec: merges move everything).
- [ ] **Step 4: Run** the suite — PASS.
- [ ] **Step 5: Commit** `Move medals when two accounts merge`. Ask the owner before opening the Engine pull request.

---

## Perland Launcher (branch `feat/medals-saved-synced`)

### Task L1: medal rules in the core

**Files:**
- Create: `crates/launcher-core/src/medals/mod.rs`, `crates/launcher-core/src/medals/rules.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod medals;`)

**Interfaces:**
- Produces:
  - `pub struct Measures { pub local_seconds: u64, pub best_instance_seconds: u64, pub instances: u32, pub modpacks: u32, pub mods: u32, pub versions: u32, pub days: u32, pub longest_run_seconds: u64, pub first_game: bool, pub theme: bool, pub decorator: bool, pub tailor: bool, pub doctor: bool }`
  - `pub enum Group { Local, Perland }`; `pub enum Kind { Ladder, Single }`
  - `pub struct Rule { pub id: &'static str, pub group: Group, pub kind: Kind, pub unit: Unit, pub steps: &'static [u64], pub rim: u8 }`; `pub static CATALOGUE: [Rule; 24]` in display order (local first, as spec §5, then `regular`, `achiever`, `traveler`, the seven singles).
  - `pub fn local_tiers(m: &Measures, earned: &Earned) -> BTreeMap<&'static str, u8>` (completionist reads the other 13 local results and `earned`).
  - `pub fn perland_tiers(stats: &PlayerStats) -> BTreeMap<&'static str, u8>` (ported from `ui/src/features/medals/rules.ts`, same advancement ids).
  - `pub fn value(id, m, stats) -> u64` for the UI's progress.

- [ ] **Step 1: Write the failing tests** — one per ladder at each threshold boundary (e.g. `instanceTime` 5 h − 1 s → 0, 5 h → 1, 300 h → 4); each single false/true → 0/rim; completionist false with one ladder at 3, true with all ladders at 4 and all singles earned; perland: `minecraft:story/mine_diamond` on any server → `diamonds` 2; `achiever` counts distinct ids across servers; catalogue has 24 unique ids, 14 local.
- [ ] **Step 2: Run** `cargo test -p launcher-core medals::rules` — FAIL.
- [ ] **Step 3: Implement** `rules.rs`.
- [ ] **Step 4: Run** the tests — PASS.
- [ ] **Step 5: Commit** `Move the medal rules into the core and add the launcher medals`.

### Task L2: the medals file

**Files:**
- Create: `crates/launcher-core/src/medals/store.rs`

**Interfaces:**
- Consumes: L1 types.
- Produces:
  - `pub const MEDALS_FILE: &str = "medals.json";`
  - `pub type Earned = BTreeMap<String, BTreeMap<u8, DateTime<Utc>>>` (`chrono` is already a dependency; if not, RFC3339 strings via `time`).
  - `pub struct Counters { modpacks: u32, mods: u32, versions: BTreeSet<String>, days: BTreeSet<NaiveDate>, longest_run_seconds: u64, first_game: bool, theme: bool, decorator: bool, tailor: bool, pending_doctor: Option<String>, doctor: bool }` (serde camelCase, defaults).
  - `pub struct AccountCache { player_id: String, earned: Earned, synced_at: Option<DateTime<Utc>> }`
  - `pub struct MedalsStore` with `new(path)`, `async fn read() -> MedalsFile`, `async fn update<F>(f: F) -> Result<MedalsFile>` (lock, read, apply, write_atomic), `fn record(earned: &mut Earned, id, tier, now)` (adds missing tiers 1..=tier dated `now`, never removes, never moves a date later), `fn merge(a: &Earned, b: &Earned) -> Earned` (union, earliest date per tier).

- [ ] **Step 1: Write the failing tests** — missing file, empty file, `{"version":99}` and garbage all read as default and the next `update` writes a valid file; `record` tier 3 adds 1, 2, 3; recording tier 2 again keeps dates; `merge` takes the earliest per tier and the union of ids; round trip of the spec §6 JSON example.
- [ ] **Step 2: Run** `cargo test -p launcher-core medals::store` — FAIL.
- [ ] **Step 3: Implement** after `whats_new_seen.rs`'s pattern.
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `Keep earned medals with their dates in medals.json`.

### Task L3: counting in the app

**Files:**
- Create: `crates/launcher-app/src/medals.rs`
- Modify: `crates/launcher-app/src/app.rs` (field `medals: MedalsStore` from `root.join(MEDALS_FILE)`; `settings_set`), `crates/launcher-app/src/runs.rs` (`record_playtime` call sites at lines ~410 and ~986; the crash branch at ~990), `crates/launcher-app/src/packs.rs` (`pack_add` from Explore; `instance_art_set` own picture), `crates/launcher-app/src/skins.rs` (`skin_save`), `crates/launcher-app/src/browse.rs` (`browse_install`), `crates/launcher-app/src/minecraft_link.rs` (`perland_stats`)

**Interfaces:**
- Consumes: L1, L2.
- Produces on `AppCore`:
  - `async fn medals_note(&self, event: MedalEvent)` where `enum MedalEvent { RunEnded { instance: String, version: String, seconds: u64, crashed: bool, diagnosed: bool }, ThemeSaved, OwnPicture, SkinSaved, PackInstalled, ModsInstalled(u32) }`. It updates counters, then `medals_evaluate()`. Never returns an error to its caller (logs a warning).
  - `async fn medals_evaluate(&self) -> Result<()>`: measures from counters + `playtime_list()` + the instance list; records new local tiers; with cached stats, records new Perland tiers in `earned` too (spec §6 as amended); triggers sync (L4) when anything was added.
  - `async fn medals_view(&self) -> Result<MedalsView>` (DTO in L5).
- Values pinned: `doctor` completes when a run of the instance in `pending_doctor` ends with `seconds >= 600` and `crashed == false`; a diagnosed crash sets `pending_doctor`. `days` uses the local calendar date of the run's end. `versions` uses the instance's Minecraft version string. Only an Explore modpack install counts for `PackInstalled`; `ModsInstalled(n)` counts the mods the plan installed, dependencies included. `ThemeSaved` only when the saved theme differs from `Theme::default()`.
- First run: `medals_evaluate` at `AppCore` start credits measures already on disk (spec §6).

- [ ] **Step 1: Write the failing tests** (in `app.rs` test module with the existing test core): a 2 h 30 min run → `marathon` 1, `firstGame`, `days` counter 1; three runs on two dates → `days` = 2; a crashed diagnosed run then a 9-minute run → no `doctor`, then a 10-minute run → `doctor` 2; a crashed run then 10 minutes → no `doctor` without a diagnosis; theme saved default → nothing, non-default → `theme`; deleting instances after `collector` 2 → still 2; existing playtime of 12 h on start → `time` 2 dated at start.
- [ ] **Step 2: Run** `cargo test -p launcher-app medals` — FAIL.
- [ ] **Step 3: Implement** `medals.rs` and the calls at each site (after the action succeeded).
- [ ] **Step 4: Run** `cargo test --workspace` — PASS.
- [ ] **Step 5: Commit** `Count the launcher medals where things happen`.

### Task L4: sync with the account

**Files:**
- Modify: `crates/launcher-core/src/perland/api.rs` (`medals_get`, `medals_put`), `crates/launcher-app/src/medals.rs` (`medals_sync`), `crates/launcher-app/src/app.rs` (start trigger), the sign-in and sign-out paths of the Perland account in `crates/launcher-app/src/perland*.rs`
- Test: `crates/launcher-core/tests/perland_medals.rs` (wiremock)

**Interfaces:**
- Produces:
  - `pub async fn medals_get(&self, token: &str) -> Result<Option<Vec<MedalEntry>>>` and `medals_put(&self, token, &[MedalEntry]) -> Result<Option<Vec<MedalEntry>>>`; `None` on 404 and on 503 `player_accounts_disabled`. `MedalEntry { id: String, tier: u8, earned_at: DateTime<Utc> }` serde camelCase.
  - `async fn medals_sync(&self)`: with a session, PUT `merge(earned, account.earned)` (only the cache whose `player_id` is the session's); store the answer as `AccountCache`. Any failure: log at info, change nothing. Triggers: core start, Perland sign-in done, `medals_view` called, `medals_evaluate` added a tier. At most one sync in flight (a `tokio::sync::Mutex` try-lock; a skipped trigger is fine).
  - Sign-out drops `account`; keeps `earned` and `counters`.

- [ ] **Step 1: Write the failing tests** — wiremock: GET and PUT parse; 404 → `None`; 503 `player_accounts_disabled` → `None`; 500 → `Err`. App: signed in as A with cache of A → PUT body is the union; session switches to B → A's cache dropped before the PUT, B's answer stored; 404 leaves the file unchanged; sign-out removes `account` and keeps `earned`.
- [ ] **Step 2: Run** `cargo test -p launcher-core --test perland_medals` and `cargo test -p launcher-app medals_sync` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace` — PASS.
- [ ] **Step 5: Commit** `Sync medals with the Perland account`.

### Task L5: IPC

**Files:**
- Modify: `crates/launcher-app/src/dto.rs`, the command list in `src-tauri` (wiring only), `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts` (and `mockPerland.ts` scenarios `signedOut`, `signedIn`, `linked` with sample medals and dates from the spike), `ui/src/state/queries.ts` (`useMedals`, invalidated on `medals-changed` event)
- Regenerate: `ui/src/ipc/contract/*.json` with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`

**Interfaces:**
- Produces: command `medals_view` → `MedalsView { medals: Vec<MedalView>, saved_to_account: bool }`, `MedalView { id, group: "local"|"perland", kind: "ladder"|"single", tier: u8, rim: u8, value: u64, goal: Option<u64>, unit: "time"|"count", steps: Vec<u64>, earned_at: Option<String> }` (`earned_at`: the current tier's date, RFC3339). Event `medals-changed` emitted after any write.

- [ ] **Step 1: Write the failing test** — contract test fails until the new command's JSON exists; a `dto.rs` unit test for the `MedalView` of `time` at tier 2 with its date.
- [ ] **Step 2: Run** `cargo test -p launcher-app --test contract` — FAIL.
- [ ] **Step 3: Implement** and regenerate the contract.
- [ ] **Step 4: Run** `cargo test --workspace`, `pnpm -r typecheck` — PASS.
- [ ] **Step 5: Commit** `Serve the medals to the UI`.

### Task L6: the screen

**Files:**
- Modify: `ui/src/features/medals/Medals.tsx`, `MedalBadge.tsx`, `AchievementsDialog.tsx`, `AchievementsScreen.tsx`, `ui/src/styles/app.css`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`, `ui/src/features/medals/*.test.tsx`
- Delete: the rule code of `ui/src/features/medals/rules.ts` (keep only display helpers it still needs, or delete the file)
- Create: `release-notes/medals-saved.json` (with the `release-notes` skill; kind `feature`)

**Interfaces:**
- Consumes: `useMedals()` (L5), `usePerlandStatus()`, `useAccounts()`, `usePerlandStats()`.

Port the spike (`git stash` "achievements spike10 (mockup)", patch `achievements-steam-spike10.patch`) onto `useMedals()`, without its `SAMPLE`/`DATES` and without reformatting the i18n files (edit keys in place, keep each file's formatting). Fix the spike's plural forms (`modpacks.goal_one` "Instala 1 modpack desde Explorar", `modder.goal_one` "Instala 1 mod desde Explorar", `versions.goal_one` "Juega en 1 versión de Minecraft", `days.goal_one` "Juega 1 día"). Text and link states exactly as spec §4.

- [ ] **Step 1: Write the failing tests** — signed out: "10 de 24 medallas" from the mock, saved line local, to-earn hint with the link "Iniciar sesión"; Perland + Microsoft not linked: link "Vincular cuentas" in both places, and clicking it navigates to `accounts`; linked: no link, server list shown, saved line account; an earned medal shows "Conseguida el …"; a to-earn medal has `data-unearned` on its list and no meter; the Minecraft section renders while signed out.
- [ ] **Step 2: Run** `pnpm --filter ui test -- medals` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the full pre-commit list from Global Constraints — PASS. Capture before/after screenshots (desktop, the three states) with `pnpm --filter ui dev:mock`.
- [ ] **Step 5: Commit** `Show medals and Minecraft achievements apart, saved and synced`. Ask the owner before opening the launcher pull request and before any release.

### Task L7: how to get each vanilla achievement

**Files:**
- Modify: `ui/src/features/medals/advancements.ts` (or the i18n files, whichever holds the names today), `ui/src/features/medals/AchievementsDialog.tsx`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`, `ui/src/features/medals/AchievementsDialog.test.tsx`

**Interfaces:**
- Consumes: the advancement ids the dialog already lists.

Every vanilla advancement the dialog names gets one line of how to get it, written in our own words in en and es (never Mojang's text, per CLAUDE.md), under the key `achievements.how.<id with / and : as .>`. The dialog shows it under the name, and its header carries the hint line from the spike (`medals.mcDialogHint`).

- [ ] **Step 1: Write the failing test** — every id in the vanilla list has a non-empty `how` in both locales (a loop over the list); the dialog shows the line for `minecraft:story/mine_diamond`.
- [ ] **Step 2: Run** `pnpm --filter ui test -- AchievementsDialog` — FAIL.
- [ ] **Step 3: Write the lines** (short, one sentence, informal "tú").
- [ ] **Step 4: Run** the UI tests and `pnpm -r lint` — PASS.
- [ ] **Step 5: Commit** `Say how to get each Minecraft achievement`.
