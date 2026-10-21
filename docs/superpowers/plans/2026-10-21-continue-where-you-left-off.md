# Continue Where You Left Off Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Home shows a secondary "Continue: {name}" key that runs Play straight into the last world or server of the selected instance.

**Architecture:** `launcher-core::last_place` parses the connect line, finds the latest world, stores `.perland/last-place.json` and checks a record against the files on disk. `launch` gains a `world` quick-play argument. `launcher-app` swaps the run's `join: bool` for a `Destination`, watches log lines for the connect line, records the place before `idle`, and exposes `continue_target`/`play_continue`. The UI adds one raised key above Play.

**Tech Stack:** Rust (tokio, serde), Tauri 2 wiring, React + TanStack Query, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-21-continue-where-you-left-off-design.md` (issue #86)

## Global Constraints

- No Tauri or UI crates in `launcher-core` or `launcher-app`; `src-tauri` is wiring only, with no tests.
- Every UI string goes in `ui/src/i18n/en.json` and `es.json` (neutral Latin American Spanish, informal "tú"). The new error needs `errors.world_continue_unsupported.message` (and `.fix`) in both.
- Contract JSON is regenerated only with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.
- Continue is never amber or lit. Play stays the only lit button on Home.
- Nothing to continue means no key, no error and no toast.
- The record file is `<instance>/.perland/last-place.json`. A damaged file reads as `None`.
- The world version gate is: release `1.X[.Y]` with X ≥ 20; `1.X…-pre*`/`-rc*` with X ≥ 20; snapshot `YYwWWx` ≥ `23w14a`; a first number ≥ 25 (year versions such as `26.1`). Anything else is false.

## Review Focus

- A world folder with spaces or non-ASCII ("Mundo de Ana") must reach `--quickPlaySingleplayer` as a single argument: Task 2 test `quick_play_opens_a_world_on_1_20_1` uses "Mundo de Ana".
- XML log4j output (`<log4j:Message><![CDATA[Connecting to h, 25565]]></log4j:Message>`) must parse: Task 1 test `connect_line_reads_xml_events`.
- An IPv6 literal on the connect line ("Connecting to 2001:db8::1, 25565") splits at the last `, `: Task 1 test `connect_line_takes_ipv6`.
- A record naming `../escape` or `a/b` as the folder must never show or launch: Task 1 test `unsafe_world_folders_never_show`.
- A run that never left the menus keeps the previous place: Task 4 test `a_menu_only_run_keeps_the_last_place`.

---

### Task 1: `last_place` core module

**Files:**
- Create: `crates/launcher-core/src/last_place.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod last_place;`)

**Interfaces:**
- Produces:
  - `#[serde(tag = "kind", rename_all = "camelCase")] pub enum LastPlace { Server { host: String, port: u16, #[serde(default)] own: bool, at: u64 }, World { folder: String, at: u64 } }`
  - `pub async fn read(perland_dir: &Path) -> Option<LastPlace>`
  - `pub async fn write(perland_dir: &Path, place: &LastPlace) -> Result<()>` (with `write_atomic`)
  - `pub fn connect_line(line: &str) -> Option<ServerAddress>`
  - `pub async fn latest_world(game_dir: &Path, since: SystemTime) -> Option<(String, SystemTime)>`
  - `pub fn opens_worlds(minecraft: &str) -> bool`
  - `pub fn safe_folder(folder: &str) -> bool`
  - `pub async fn world_exists(game_dir: &Path, folder: &str) -> bool` (`safe_folder` and `saves/<folder>/level.dat` is a file)
  - `pub async fn listed_name(game_dir: &Path, address: &ServerAddress) -> Option<String>`: the `servers.dat` entry's name (falling back to `address.display()` when it's empty), else `options.txt` `lastServer:` matching gives `address.display()`, else `None`. An entry's ip is parsed as `host[:port]`, with port 25565 by default and brackets around IPv6.
  - `pub fn is_local(address: &ServerAddress) -> bool` (loopback, private, link-local or unique-local IP literal)

- [ ] **Step 1: Write failing unit tests** in `#[cfg(test)] mod tests`:
  - `connect_line_reads_plain_lines`: `"[12:00:01] [Render thread/INFO]: Connecting to play.example.com, 25565"` gives `ServerAddress::new("play.example.com", 25565)`.
  - `connect_line_reads_xml_events`.
  - `connect_line_takes_ipv6`.
  - `connect_line_ignores_others`: `"Connecting to"`, `"Connecting to bad host, 1"` and `"Connecting to a.b, 0"` give `None`.
  - `opens_worlds_by_version`: true for `1.20`, `1.20.1`, `1.21.4`, `1.20-pre1`, `23w14a`, `24w01a`, `26.1`; false for `1.19.4`, `23w13a`, `1.12.2`, `b1.7.3`, `rd-132211` and `""`.
  - `unsafe_world_folders_never_show`: `safe_folder` is false for `""`, `"."`, `".."`, `"../escape"`, `"a/b"` and `"a\\b"`, and true for `"Mundo de Ana"`.
  - `latest_world_picks_the_newest_since_the_start`: two worlds with `level.dat`, mtimes set with `std::fs::File::set_modified`. The newest one at or after `since` wins, and a world older than `since` gives `None`.
  - `read_write_round_trip` and `damaged_file_reads_none`.
  - `listed_name_finds_the_server_list_entry`: build `servers.dat` with `server_list::with_server` (named "Asalith Fields", ip `play.example.com:25566`). It matches on host and port, and a different port gives `None`.
  - `listed_name_falls_back_to_direct_connect`: `options.txt` with `lastServer:mc.example.org` matches port 25565.
  - `is_local_spots_lan_addresses`: `192.168.1.5` and `127.0.0.1` are true; `play.example.com` and `8.8.8.8` are false.
- [ ] **Step 2:** `cargo test -p launcher-core last_place` fails to compile (the module is missing).
- [ ] **Step 3:** Implement. `connect_line` takes the text after the last `"Connecting to "`, cuts at `"]]>"`, splits at the last `", "`, trims, and parses the port as `i64`. `latest_world` reads `saves/*`, takes the max mtime of `level.dat` and `session.lock` for each world with a `level.dat`, and keeps those `>= since`.
- [ ] **Step 4:** `cargo test -p launcher-core last_place` passes.
- [ ] **Step 5:** Commit "Remember the last world or server: core".

### Task 2: World quick play in launch

**Files:**
- Modify: `crates/launcher-core/src/launch/mod.rs`, `crates/launcher-core/src/play.rs`, `crates/launcher-core/src/error.rs`

**Interfaces:**
- Produces: `pub const QUICK_PLAY_SINGLEPLAYER: &str = "is_quick_play_singleplayer";`, `LaunchSpec.world: Option<&'a str>`, `StartSpec.world: Option<&'a str>`, and `ErrorCode::WorldContinueUnsupported` (wire `world_continue_unsupported`, category game).

- [ ] **Step 1: Tests in `launch/mod.rs`:**
  - `quick_play_opens_a_world_on_1_20_1`: `arg_after(&cmd.args, "--quickPlaySingleplayer") == "Mundo de Ana"`, with no `--quickPlayMultiplayer` and no `--server`.
  - `a_world_on_a_version_without_quick_play_is_refused`: 1.12.2 with a world gives `Err` with `code == WorldContinueUnsupported`.
- [ ] **Step 2:** The tests fail.
- [ ] **Step 3:** Implement. Add the feature to the `RuleContext` when the world is set and declared, and insert the var `quickPlaySingleplayer`. Every existing `LaunchSpec`/`StartSpec` literal gets `world: None`; `play::command` passes `spec.world`. Add the code to `error.rs` beside `ServerWakeFailed` and to `invite_codes_exist`'s neighbour list (or a new small test).
- [ ] **Step 4:** `cargo test -p launcher-core` passes.
- [ ] **Step 5:** Commit.

### Task 3: Run destination in launcher-app

**Files:**
- Modify: `crates/launcher-app/src/runs.rs`, `crates/launcher-app/src/runner.rs`, `crates/launcher-app/src/testing.rs`

**Interfaces:**
- Produces: `pub(crate) enum Destination { Title, JoinOwn, Join(ServerAddress), World(String) }`, with `PlayArgs.destination` replacing `PlayArgs.join`. `play_with(id, join)` maps to `JoinOwn`/`Title`. `Runner::start(&self, instance, prepared, session, join: Option<ServerAddress>, world: Option<String>, sink)`. The fake records `started_worlds: Mutex<Vec<Option<String>>>`.
- `wake_for_join` is used for `Join(a)` with `last = None`.

- [ ] **Step 1: Tests in `runs.rs`:**
  - `a_join_destination_joins_that_address`: starts with `Some(other)` and pings the other address.
  - `a_world_destination_opens_the_world`: `started_worlds == [Some("Mundo")]` and `started_joins == [None]`.
- [ ] **Step 2:** The tests fail.
- [ ] **Step 3:** Implement. Add `pub(crate) async fn play_to(self: &Arc<Self>, id, destination)`, shared by `play_with`.
- [ ] **Step 4:** `cargo test -p launcher-app runs` passes.
- [ ] **Step 5:** Commit.

### Task 4: Record the place and expose Continue

**Files:**
- Create: `crates/launcher-app/src/continue_play.rs` (in `lib.rs`)
- Modify: `crates/launcher-app/src/runs.rs` (sink watch and record before `finish`), `crates/launcher-app/src/dto.rs`, `crates/launcher-app/tests/contract.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Produces:
  - `#[serde(rename_all = "camelCase")] pub struct ContinueTarget { pub kind: ContinueKind, pub name: String }` with `pub enum ContinueKind { World, Server }` (snake_case);
  - `AppCore::continue_target(&self, id: &str) -> Result<Option<ContinueTarget>>`;
  - `AppCore::play_continue(self: &Arc<Self>, id: &str) -> Result<()>`;
  - Tauri commands `continue_target(id)` and `play_continue(id)`.
- Record rule: the run's sink keeps `(ServerAddress, SystemTime)` of the last connect line (skipping `is_local` addresses unless `listed_name` finds them, checked at record time). After `game.wait()`, `latest_world(game_dir, run_wall_start)` runs, and the later of the two is written with `own = (address == instance's server address)`. With neither, nothing is written.
- Resolve rule (spec §5): a world needs `opens_worlds(&instance.game.minecraft)` and `world_exists`, and is named by its folder. A server is `own` (the instance is still a server instance, not `gone`), named by the server link's name or else the instance's name; or `listed_name`. `play_continue` maps them to `World` / `JoinOwn` / `Join`, and to `Title` when the record doesn't resolve.

- [ ] **Step 1: Tests (`continue_play.rs`, with the fake runner):**
  - `a_server_joined_in_game_becomes_continue`: `start_lines` holds a connect line to an address listed in `servers.dat` as "Hub". After the game ends, `continue_target` is `Server "Hub"`.
  - `a_world_played_becomes_continue`: the fake finishes after a test writes `saves/Mundo/level.dat`. The instance is 1.20.1 and the target is `World "Mundo"`.
  - `a_menu_only_run_keeps_the_last_place`.
  - `a_deleted_world_hides_continue`, `an_old_version_never_offers_a_world`, `a_server_removed_from_the_list_hides_continue`, `a_gone_perland_server_hides_continue`.
  - `continue_on_the_own_server_plays_and_joins`: `started_joins == [Some(address())]`, and the record's `own` is true.
  - `continue_on_a_world_starts_it`, `continue_with_nothing_plays_normally`.
- [ ] **Step 2:** The tests fail.
- [ ] **Step 3:** Implement. Add a contract `check("continue_target", vec![Some(world), Some(server), None])` and run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Wire the two Tauri commands.
- [ ] **Step 4:** `cargo test --workspace` and clippy pass.
- [ ] **Step 5:** Commit.

### Task 5: UI Continue key (through `/impeccable`)

**Files:**
- Modify: `ui/src/ipc/commands.ts`, `ui/src/ipc/schema.ts` (if types are listed there), `ui/src/ipc/mock.ts`, `ui/src/state/queries.ts`, `ui/src/state/handleEvent.ts`, `ui/src/features/home/Home.tsx`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Create: `ui/src/features/home/ContinueKey.tsx`, `ui/src/features/home/ContinueKey.test.tsx`

**Interfaces:**
- `commands.continueTarget(id): Promise<ContinueTarget | null>`, `commands.playContinue(id): Promise<null>`, `qk.continueTarget(id)`, `useContinueTarget(id)`. `handleEvent` invalidates `['continue']` on `idle`.
- Strings: `home.continue` = "Continue: {{name}}" / "Continuar: {{name}}"; `errors.world_continue_unsupported.message` = "This Minecraft version can't open a world straight away" / "Esta versión de Minecraft no puede abrir un mundo directamente"; `.fix` = "Press Play and pick the world in Singleplayer" / "Presiona Jugar y elige el mundo en Un jugador".

- [ ] **Step 1: Tests (`ContinueKey.test.tsx`):**
  - the key renders "Continue: Asalith Fields" and calls `playContinue`;
  - it is absent when the target is `null` and when the run isn't idle;
  - it is disabled without an account.
- [ ] **Step 2:** The tests fail.
- [ ] **Step 3:** Implement. Place the key above Play in Home's Play column. In the mock, seed `asalith-fields` with a server target and `vanilla-1-20-1` with a world target.
- [ ] **Step 4:** `pnpm -r test`, `pnpm -r lint` and `pnpm -r typecheck` pass. Run the impeccable critique/polish on Home.
- [ ] **Step 5:** Commit.

### Task 6: Ship

- [ ] Run the full CLAUDE.md check list. Take Playwright screenshots in `dev:mock`. Open the PR (draft), get CI green, request review, fix what it finds, merge `main` again, then merge the PR.
