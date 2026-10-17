# Public Server List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Servers screen that lists Perland's public servers and joins one with a single Play, waking it first when it sleeps (PerlandDev/perland-launcher#50).

**Architecture:**
- **Core:** two new modules in `launcher-core`. `public_list` reads `GET /api/v1/public-servers` from the Engine that the Perland site's `config.json` names. `server_wake` speaks just enough of the Minecraft protocol to ping a server and knock on its waker.
- **App:** `launcher-app` adds two commands and a `waking` run phase after `preparing`.
- **UI:** a new `servers` route and screen reuse the existing join flow (`joinAdd` → `play(id, true)`).

**Tech Stack:** Rust (tokio, reqwest, serde, wiremock for tests), Tauri 2, React 19 + TanStack Query + zod, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-17-public-server-list-design.md`. Read it first; this plan argues from it. The Engine contract is `docs/superpowers/specs/2026-10-16-public-server-listing-design.md` in PerlandEngine (a read-only clone at `/home/user/perlandengine`), and the waker is `internal/waker/` there.

## Global Constraints

- `launcher-core` and `launcher-app` never depend on Tauri or UI crates. `src-tauri` is wiring only and has no tests.
- TDD: the failing test first. Pure logic gets `#[cfg(test)]` unit tests, HTTP behaviour gets `crates/launcher-core/tests/*.rs` with wiremock, UI gets Vitest + Testing Library.
- Keep local test inputs tiny. No fuzz, stress or load tests locally.
- Player-facing errors are `LauncherError`. A new `ErrorCode` needs `errors.<code>.message` in both `ui/src/i18n/en.json` and `es.json`.
- Every UI string goes in both `en.json` and `es.json`: neutral Latin American Spanish, informal *tú*.
- `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- Invite tokens are credentials. A `Debug` impl, log line, error message, param or detail never holds one; errors carry the Engine's host only.
- **Default Perland site:** `https://play.perland.online`; override with the env var `PERLAND_SITE_URL` (https, or http on loopback).
- **Page size:** 50. **List body cap:** 512 KiB. **Icon cap:** 1 MiB (PNG signature checked). **Icon cache:** 256 entries.
- **Wake:** connect deadline 5 s; exchange deadline 5 s; protocol number 767; ping every 2 s; give up after 5 minutes.
- **Waker texts** (exact, from PerlandEngine `internal/waker/message.go`): not whitelisted is `§cYou are not whitelisted on this server.`; no room contains `cannot start right now`; a status answer from the waker has `version.name == "Sleeping"`.
- **New error codes:**
  - `server_list_closed` (network): en "The server list isn't open yet. Check back soon." / es "La lista de servidores aún no está abierta. Vuelve pronto."
  - `server_wake_failed` (network): en "Couldn't wake the server. The game will try to join anyway." / es "No se pudo despertar el servidor. El juego intentará entrar igual."
- **Before every commit**, run the CLAUDE.md "Before committing" checks that apply to what the task touched:
  - Rust: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, then the crate's tests;
  - UI: `pnpm --filter ui lint`, `pnpm --filter ui typecheck`, `pnpm --filter ui test`.
  - The full list runs in Task 7.
- **Commit messages** end with:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jhqr9QaewJmCwnRRyNsTJ9
  ```
- All UI styling goes through the `/impeccable` skill against `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.

## Review Focus

1. **An Engine with the feature off, or older:** a 404 or 503 from the list, or a `config.json` with no `engineUrl`, gives the calm "isn't open yet" state, never an ErrorCard or toast (the owner's release requirement). Tests: Task 1 maps 404, 503 and a missing `engineUrl` to `server_list_closed`; Task 6 renders the calm state for that code with no `role="alert"`.
2. **A whitelisted sleeping server:** the knock is answered "not whitelisted", the run must not wait 5 minutes, and the game starts at once. Test: Task 4, `waking_skips_the_wait_when_not_whitelisted`.
3. **A Stop pressed while waiting for the server:** the run ends as cancelled and no game starts. Test: Task 4, `stop_during_waking_cancels_the_run`.
4. **A server with no usable invite** (no `inviteUrl` and a token that doesn't parse), or a modded stack without a modpack: its card shows with Play disabled, never a crash or an empty grid. Tests: Task 1 `invite_for_*` and `joinable_*`; Task 6 `a card that can't be joined shows the address and a disabled Play`.
5. **Spanish strings ~30% longer, at the minimum 960 × 600 window with the wide rail:** card names wrap, Play and the filter labels never truncate. Check: Task 6's impeccable finish review at 960 × 600 in `es`.

---

### Task 1: Core `public_list` client

**Files:**
- Create: `crates/launcher-core/src/public_list.rs`
- Create: `crates/launcher-core/tests/public_list.rs`
- Modify:
  - `crates/launcher-core/src/lib.rs` (pub mod);
  - `crates/launcher-core/src/error.rs` (`ServerListClosed` → `"server_list_closed"`);
  - `crates/launcher-core/src/invite/client.rs` (site-origin engine lookup);
  - `crates/launcher-core/src/sources/perland.rs` (share the joinable rule);
  - `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (`errors.server_list_closed.message`);
  - `ui/src/ipc/contract/error_codes.json` (regenerated).

**Interfaces:**
- Produces:
  ```rust
  pub const DEFAULT_SITE: &str = "https://play.perland.online";
  pub const PAGE_SIZE: u32 = 50;
  /// https, or http on loopback; no user info, query or fragment. Reduced to its origin.
  pub fn site_url(raw: &str) -> Option<Url>;

  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum ListSort { #[default] Players, Newest, Name }

  #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase", default)]
  pub struct ListQuery { pub sort: ListSort, pub modded: Option<bool>, pub minecraft: Option<String>,
      pub has_room: bool, pub category: Option<String>, pub language: Option<String>,
      pub region: Option<String>, pub offset: u32 }
  impl ListQuery { pub fn pairs(&self) -> Vec<(&'static str, String)>; }

  #[derive(Clone, Default, Deserialize)] #[serde(rename_all = "camelCase")]
  pub struct PublicServer { pub token: String, pub invite_url: Option<String>, pub name: String,
      pub motd: String, pub icon_version: Option<String>, pub address: InviteAddress,
      pub game: InviteGame, pub modded: bool, pub status: String, pub players: Option<InvitePlayers>,
      pub whitelisted: bool, pub pack: Option<InvitePack>, pub categories: Vec<String>,
      pub language: String, pub region: Option<String>, pub listed_at: Option<String> }
  impl PublicServer { pub fn status(&self) -> ServerStatus; pub fn joinable(&self) -> bool; }
  impl Debug for PublicServer // prints only `name`

  #[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase", default)]
  pub struct Facets { pub categories: Vec<String>, pub languages: Vec<String>,
      pub minecraft_versions: Vec<String>, pub regions: Vec<String> }

  #[derive(Clone, Debug, Default)] pub struct ListPage { pub servers: Vec<PublicServer>, pub total: u32, pub facets: Facets }
  pub struct FetchedPage { pub engine: Url, pub page: ListPage }

  #[derive(Clone)] pub struct PublicListClient;
  impl PublicListClient {
      pub fn new(invites: InviteClient, site: Url) -> Self;
      pub fn site(&self) -> &Url;
      pub async fn page(&self, query: &ListQuery) -> Result<FetchedPage>;
      pub async fn icon(&self, token: &str, icon_version: &str) -> Option<Vec<u8>>;
      pub fn invite_for(&self, server: &PublicServer) -> Option<InviteLink>;
  }
  ```
- In `invite::client`: `pub async fn engine_for_site(&self, site: &Url) -> Result<Url>`. It shares the per-origin cache and `ask_site` logic with `engine_for(&InviteLink)`. Refactor `ask_site` to take an origin string and a host for errors.
- In `sources::perland`: `pub fn joinable_game(kind: &str, game: &InviteGame, has_pack: bool) -> bool`. `refused` and `PublicServer::joinable` both call it (with kind `"stack"`).

- [ ] **Step 1: Write the failing tests** in `crates/launcher-core/tests/public_list.rs`, using `wiremock` the way `tests/invite.rs` does (a `config.json` route naming `server.uri()` as `engineUrl`). Each test is a few lines with a minimal JSON body:
  - `reads_the_engine_from_the_site_config`: two `page` calls make one `config.json` request.
  - `sends_only_set_filters`: `ListQuery { sort: Newest, modded: Some(false), minecraft: Some("1.21.1"), has_room: true, category: Some("smp"), language: Some("es"), offset: 50, .. }` hits `/api/v1/public-servers` with exactly `sort=newest&modded=false&minecraft=1.21.1&hasRoom=true&category=smp&language=es&limit=50&offset=50`. A default query sends only `limit=50` (and `offset=0`).
  - `parses_a_full_and_a_minimal_server`: the Engine spec's example server, and `{"token":"…43 chars…","name":"x","status":"online"}`.
  - `unknown_status_reads_as_sleeping`: `"status":"waking"` → `ServerStatus::Sleeping`.
  - `closed_list_is_server_list_closed`: 404 → `ErrorCode::ServerListClosed`; 503 → `ServerListClosed`; a `config.json` of `{}` → `ServerListClosed`.
  - `rate_limit_and_trouble`: 429 → `InviteRateLimited`, 500 → `HostTrouble`, 400 → `MetaUnexpected`, a body over the cap → `MetaUnexpected`.
  - `no_error_names_a_token`: for each failure, `format!("{e:?}")` does not contain the token.
  - `icon_checks_png_and_caches`: a PNG body → `Some`, and a second call makes no request; a 404 → `None`; a non-PNG body → `None`.
  - `invite_for_prefers_invite_url`: `inviteUrl = https://play.example.com/<token>` → that link. Without it → `<site>/<token>`. With a bad token and no `inviteUrl` → `None`.
  - `joinable_follows_the_join_rule`: a vanilla stack with a version → true; fabric with a pack → true; fabric without a pack → false.

  `404 → ServerListClosed` overrides `config.json` errors only for a site config that answers but has no `engineUrl`. An unreachable site stays `Offline`.

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-core --test public_list`. Expected: they don't compile (unresolved module `public_list`).

- [ ] **Step 3: Implement** `public_list.rs`, `engine_for_site`, `joinable_game`, and the new `ErrorCode` variant with its string. Reuse `read_capped`, `request_failed`, `host_of` and `check_engine_url`. The icon cache is a `Mutex<VecDeque<((String, String), Vec<u8>)>>` capped at 256, oldest out. Map a `config.json` with no `engineUrl`, as reached from `page`, to `server_list_closed`. Through `engine_for(&InviteLink)` it stays `invite_site_unreachable`.

- [ ] **Step 4: Add the messages** `errors.server_list_closed.message` to `en.json` and `es.json` with the Global Constraints text, and regenerate the contract: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

- [ ] **Step 5: Run.** Run `cargo test -p launcher-core` and `pnpm --filter ui test -- i18n contract`. Expected: PASS.

- [ ] **Step 6: Commit.** Commit `feat(core): read Perland's public server list (#50)`.

### Task 2: Core `server_wake`

**Files:**
- Create: `crates/launcher-core/src/server_wake.rs` (with its `#[cfg(test)]` tests)
- Modify:
  - `crates/launcher-core/src/lib.rs`;
  - `crates/launcher-core/src/error.rs` (`ServerWakeFailed` → `"server_wake_failed"`);
  - `en.json` and `es.json` (`errors.server_wake_failed.message`);
  - the regenerated `error_codes.json`.

**Interfaces:**
- Consumes: `launcher_core::instances::ServerAddress { host, port }`, `launcher_core::sync::Cancel` (`is_cancelled`, `cancelled().await`).
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Ping { Waker, Server, NoAnswer }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Knock { Woken, NotWhitelisted, NoRoom }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum WaitEnd { Up, TimedOut, Cancelled }
  pub async fn ping(address: &ServerAddress) -> Ping;
  pub async fn knock(address: &ServerAddress, name: &str) -> Result<Knock>;
  pub async fn wait_until_up(address: &ServerAddress, cancel: &Cancel, every: Duration, limit: Duration) -> WaitEnd;
  ```

- [ ] **Step 1: Write the failing unit tests.**
  - Packet bytes:
    - `handshake_bytes`: `handshake("a.b", 25565, 2)` equals the VarInt-framed `[len, 0x00, VarInt(767), 3, b'a', b'.', b'b', 0x63, 0xDD, 0x02]`.
    - `varint_round_trips`: 0, 127, 128, 25565 and `i32::MAX`.
  - A fake waker: a `TcpListener` on `127.0.0.1:0` in a spawned task. It parses one handshake like the Engine's `Handle`, answers status with `{"version":{"name":"Sleeping","protocol":767},…}`, and answers login with a Disconnect whose JSON `text` is picked per test.
    - `ping_sees_the_waker`: `Ping::Waker`.
    - `knock_sends_the_name`: the fake records the name, which equals `"Steve"`.
    - `knock_reads_not_whitelisted`: the text `§cYou are not whitelisted on this server.` → `NotWhitelisted`.
    - `knock_reads_no_room`: `§fasalith §ccannot start right now.\n\n…` → `NoRoom`.
    - `knock_reads_woken`: `§aWaking up §fasalith§a...` → `Woken`.
  - A fake real server whose status has `"name":"Paper 1.21.1"`: `ping_sees_a_server`.
  - A closed port (bind, take the port, drop the listener): `ping_without_answer` → `NoAnswer`; `knock_on_a_closed_port_fails` → `ErrorCode::ServerWakeFailed`, with param `host = "127.0.0.1"`.
  - `wait_until_up`:
    - `wait_until_up_returns_up`: the port closed, then a fake server bound to it after 2 polls; `every` = 20 ms.
    - `wait_until_up_times_out`: `limit` = 60 ms.
    - `wait_until_up_cancels`: cancel fires at 30 ms; returns `Cancelled` within one `every`.

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-core server_wake`. Expected: compile failure.

- [ ] **Step 3: Implement.** Write packets with `tokio::net::TcpStream` under `tokio::time::timeout` (5 s connect, 5 s exchange). The Login Start is VarInt id 0, the name as a string, then 16 zero bytes. A response string is read up to 32 KiB. The kick's JSON is parsed for `text` (fall back to the raw string). `wait_until_up` loops `ping`, sleeping `every` with `tokio::select!` on `cancel.cancelled()`, until `Server`, the limit or the cancel.

- [ ] **Step 4: Add the messages.** Add `errors.server_wake_failed.message` in en and es, then regenerate the contract as in Task 1.

- [ ] **Step 5: Run.** Run `cargo test -p launcher-core server_wake` and `pnpm --filter ui test -- i18n`. Expected: PASS.

- [ ] **Step 6: Commit.** Commit `feat(core): ping and wake a sleeping Perland server (#50)`.

### Task 3: App commands `servers_list` and `servers_icon`, Tauri wiring

**Files:**
- Create: `crates/launcher-app/src/public_servers.rs` (with `#[cfg(test)]` tests)
- Modify:
  - `crates/launcher-app/src/lib.rs`;
  - `runner.rs` (trait plus `CoreRunner`);
  - `testing.rs` (`FakeRunner`);
  - `dto.rs`;
  - `tests/contract.rs` (new fixture `public_server_page`);
  - `src-tauri/src/main.rs` and `src-tauri/src/commands.rs`;
  - `README.md` (one line on `PERLAND_SITE_URL` in the dev section).

**Interfaces:**
- Consumes: Task 1's `PublicListClient`, `ListQuery`, `FetchedPage`, `Facets` and `PublicServer`, plus `AppCore::instance_for_invite(&InviteLink)` (existing, `servers.rs`).
- Produces:
  - **`Runner` trait:**
    ```rust
    fn public_page(&self, query: ListQuery) -> BoxFuture<Result<FetchedPage>>;
    fn public_icon(&self, token: String, icon_version: String) -> BoxFuture<Option<Vec<u8>>>;
    fn public_invite(&self, server: &PublicServer) -> Option<InviteLink>;
    ```
  - **`CoreRunner`:** `CoreRunner::new` builds a `PublicListClient` from `syncer.sources.invites.clone()` and `public_list::site_url(DEFAULT_SITE)`; `pub fn with_site(self, site: Url) -> Self` replaces it.
  - **DTOs** (`dto.rs`, `Serialize`, camelCase):
    ```rust
    pub struct PublicServerPage { pub servers: Vec<PublicServerCard>, pub total: u32, pub facets: Facets }
    pub struct PublicServerCard { pub token: String, pub invite: Option<String>, pub joinable: bool,
        pub address: Option<String>, pub name: String, pub icon_version: Option<String>,
        pub game: ServerGame, pub modded: bool, pub status: ServerStatus,
        pub players: Option<ServerPlayers>, pub whitelisted: bool, pub pack: Option<ServerPack>,
        pub categories: Vec<String>, pub language: String, pub region: Option<String>,
        pub instance: Option<String> }
    ```
  - **`AppCore`:**
    - `pub async fn servers_list(&self, query: ListQuery) -> Result<PublicServerPage>`;
    - `pub async fn servers_icon(&self, token: &str, icon_version: &str) -> Option<String>` (a `data:image/png;base64,` URL).
  - **Tauri commands:** `servers_list(query)` and `servers_icon(token, iconVersion)`.
- **Card rules:**
  - `invite` is `public_invite(..).map(|l| l.canonical())`.
  - `joinable` is `server.joinable() && invite.is_some()`.
  - `address` is `ServerAddress::from_view(&server.address, &engine).map(|a| a.display())`.
  - `instance` is `instance_for_invite(&link)` when there is a link.

- [ ] **Step 1: Write the failing tests** in `public_servers.rs`, with `FakeRunner` gaining a settable `public_page` answer:
  - `cards_carry_the_join_details`;
  - `a_server_already_joined_names_its_instance` (seed one via `join_add(&invite('A'))` as the run tests do; the page server has the same token);
  - `a_modded_server_without_a_pack_is_not_joinable`;
  - `an_address_without_a_host_uses_the_engine_host`;
  - `errors_pass_through` (`ServerListClosed`).

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-app public_servers`. Expected: compile failure.

- [ ] **Step 3: Implement** the trait methods, `CoreRunner`, `FakeRunner`, DTOs and commands. In `src-tauri/src/main.rs`, read `std::env::var("PERLAND_SITE_URL")`. Pass a usable value to `with_site`, else log `tracing::warn!` and keep the default. Never log the value when it is unusable; log only that it was ignored. Register both commands in `commands.rs` like `join_preview`.

- [ ] **Step 4: Add the contract fixture.** In `tests/contract.rs`, add `check("public_server_page", …)` with three cards: an online modded server with a pack and an existing instance; a sleeping vanilla whitelisted one; and a non-joinable fabric one without a pack, with an address. Use fake tokens of `'A'`/`'B'`/`'C'` × 43, and facets with two versions and one region. Regenerate with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

- [ ] **Step 5: Run.** Run `cargo test -p launcher-app` and `cargo build -p perland-launcher` (or the `src-tauri` package name in `src-tauri/Cargo.toml`). Expected: PASS and builds.

- [ ] **Step 6: Commit.** Commit `feat(app): servers_list and servers_icon commands (#50)`.

### Task 4: The `waking` run phase

**Files:**
- Modify:
  - `crates/launcher-app/src/events.rs` (`RunStateKind::Waking`, serialized `waking`, placed after `Preparing`);
  - `runner.rs`;
  - `testing.rs`;
  - `runs.rs` (`drive` and its tests);
  - `tests/contract.rs` (if a run-state fixture lists the kinds);
  - the regenerated fixtures.

**Interfaces:**
- Consumes: Task 2's `ping`, `knock`, `wait_until_up`, `Ping`, `Knock`, `WaitEnd`.
- Produces: `RunStateKind::Waking`, and on `Runner`:
  ```rust
  fn server_ping(&self, address: ServerAddress) -> BoxFuture<Ping>;
  fn server_knock(&self, address: ServerAddress, name: String) -> BoxFuture<Result<Knock>>;
  fn server_wait(&self, address: ServerAddress, cancel: Cancel) -> BoxFuture<WaitEnd>; // every 2 s, 5 min
  ```
  `FakeRunner` records every call (`knocks: Mutex<Vec<(ServerAddress, String)>>`) and answers from settable fields.

- [ ] **Step 1: Write the failing tests** in `runs.rs`'s test module, next to `play_and_join_connects_to_the_server` and using its helpers. Each asserts the emitted state sequence (via the existing host recorder) and `runner.started_joins`:
  - `waking_follows_preparing_and_releases_the_install_slot`:
    - ping `Waker`, knock `Woken`, wait `Up`;
    - the states run `… Preparing, Waking, Running`;
    - the knock name is the offline account's name;
    - while the fake's `server_wait` is parked, another instance's `play` reaches `Syncing`/`Preparing` (the slot is free).
  - `waking_skips_the_wait_when_not_whitelisted`: knock `NotWhitelisted`; no `Waking` state; `server_wait` never called; the game starts.
  - `waking_skips_the_wait_on_no_room`: the same with `NoRoom`.
  - `a_starting_server_that_doesn_t_answer_is_waited_for`: the saved `last_status` is `Starting`, ping `NoAnswer`; `Waking` happens and no knock is sent.
  - `waking_times_out_and_starts_the_game`: wait `TimedOut` → the game starts.
  - `stop_during_waking_cancels_the_run`: `stop(id)` while `server_wait` is parked; the run ends `cancelled`; `started_joins` is empty.
  - `a_failed_knock_warns_and_starts_the_game`: knock `Err(server_wake_failed)` → an `AppEvent::Warning` with that code, and the game starts.
  - `plain_play_of_a_sleeping_server_knocks_without_waiting`: `join = false`, `last_status` `Sleeping`; the knock is recorded (await a short notify from the fake); no `Waking` state.
  - `plain_instances_never_wake`: no ping or knock.

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-app runs::`. Expected: compile failure on `Waking` / `server_ping`.

- [ ] **Step 3: Implement** in `drive`:
  - After `prepare` returns `Ok` and `drop(permit)` (move the existing drop up if needed), and before `start`, call one helper: `async fn wake_for_join(&self, id: &str, address: &ServerAddress, last: Option<ServerStatus>, name: &str, cancel: &Cancel) -> bool`. It returns `false` when the run was cancelled. It follows spec §6.2 "Order".
  - `set_phase(&id, RunStateKind::Waking)` returning false means the run is stopping: end it as cancelled with the same `ExitInfo` the existing Preparing check uses.
  - For `join == false` and `last == Some(Sleeping)`: `tokio::spawn` the ping and knock, ignoring the result.
  - The session's player name: `args.session` already holds it. Read it there; don't load the account again.

- [ ] **Step 4: Regenerate the contract** (`UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`), then run `cargo test -p launcher-app`. Expected: PASS.

- [ ] **Step 5: Commit.** Commit `feat(app): wake a sleeping server before the game joins it (#50)`.

### Task 5: UI data layer, mock and the waking label

**Files:**
- Modify:
  - `ui/src/ipc/schema.ts` (zod `publicServerPage`, `publicServerCard`, `facets`, `listQuery`; `runStateKind` gains `'waking'`);
  - `ui/src/ipc/commands.ts` (`serversList`, `serversIcon`);
  - `ui/src/ipc/contract.test.ts` (parse the new fixture);
  - `ui/src/state/queries.ts` (`useServersList`, `useServerIcon`);
  - `ui/src/ipc/mock.ts` and `ui/src/ipc/mockServers.ts`;
  - every place that labels run phases (find with `grep -rn "'preparing'" ui/src`), for the `waking` label;
  - `en.json` and `es.json`.

**Interfaces:**
- Produces:
  - `commands.serversList(query: ListQuery) => Promise<PublicServerPage>` and `commands.serversIcon(token: string, iconVersion: string) => Promise<string | null>`;
  - `useServersList(query: ListQuery)`: a `useInfiniteQuery` keyed `['servers', query]`, with `getNextPageParam` giving `offset + 50` while that is below `total`, and `refetchInterval: 30_000`;
  - `useServerIcon(token, iconVersion)`: `staleTime: Infinity`, enabled only with an `iconVersion`;
  - mock exports `MOCK_PUBLIC_SERVERS: PublicServerCard[]` (eight cards, the spec's list).

- [ ] **Step 1: Write the failing tests:**
  - `contract.test.ts` parses `public_server_page.json` with the zod schema;
  - `mock.test.ts`:
    - `servers_list` applies `sort`, `modded`, `minecraft`, `hasRoom`, `category`, `language`;
    - its `facets` cover the whole set;
    - `joinAdd` + `play` of the sleeping mock server emits a `waking` state before `running`;
    - `servers_list` answers `server_list_closed` under a `closed` mock scenario.
  - A label test where run phases are labelled: `waking` reads "Starting the server…".

- [ ] **Step 2: Run them to see them fail.** Run `pnpm --filter ui test`. Expected: FAIL.

- [ ] **Step 3: Implement** the schema, commands, queries, mock and labels. Add the strings `run.phase.waking` (en "Starting the server…", es "Iniciando el servidor…"), keyed the way the existing phase labels are. Add the mock scenario to the existing scenario switch (`dev:mock` reads it the way `ServerScenario` is read today).

- [ ] **Step 4: Run.** Run `pnpm --filter ui test`, `pnpm --filter ui typecheck` and `pnpm --filter ui lint`. Expected: PASS.

- [ ] **Step 5: Commit.** Commit `feat(ui): public server list data, mock and the waking label (#50)`.

### Task 6: Servers screen and rail button (through `/impeccable`)

**Files:**
- Create:
  - `ui/src/features/servers-list/ServersScreen.tsx`;
  - `PublicServerCard.tsx`;
  - `ServerFilters.tsx`;
  - `YourServers.tsx` (returns `null`; the #48 slot);
  - `ServersScreen.test.tsx`.
- Modify:
  - `ui/src/state/route.ts` (`{ name: 'servers' }`);
  - `ui/src/App.tsx` (the `Screen` case);
  - `ui/src/features/rail/Rail.tsx` (a Servers button above Browse, the same pattern as `rail-browse`, with `aria-current` on the route);
  - `ui/src/features/rail/Rail.test.tsx`;
  - styles where impeccable puts them;
  - `en.json` and `es.json`.

**Interfaces:**
- Consumes: Task 5's `useServersList`, `useServerIcon` and `commands`; the existing `StatusPill`, `InviteOnly`, `PlotIcon`, `useServerIconSize`, `ScreenFrame`, `Select`, `Toggle`, `ErrorCard`, `describeError`, the toast store, `navigate` and `copyText`.

- [ ] **Step 1: Write the failing tests** in `ServersScreen.test.tsx` with `ui/src/test/backend.tsx`'s mock backend:
  - `the rail's Servers button opens the screen`;
  - `cards show name, game line, players and Play`:
    - "Asalith Fields 1.0.4";
    - "Vanilla 1.21.1";
    - "7 / 20 playing";
  - `a sleeping server reads "Sleeping, Play wakes it"`;
  - `a whitelisted server shows Invite only`;
  - `filters send their query` (pick Without mods, then the last `servers_list` call has `modded: false`);
  - `Region is hidden with fewer than two regions`;
  - `Play joins the server and opens its Home` (calls `join_add` with the card's `invite`, then `play` with `join: true`; the route is `home` with that id);
  - `Play reuses the instance that already joins the server` (no `join_add` when `instance` is set);
  - `a card that can't be joined shows the address and a disabled Play`;
  - `a closed list shows a calm message`: scenario `closed` → the text "The server list isn't open yet. Check back soon.", and `queryByRole('alert')` is null;
  - `no matches offers Clear filters`;
  - `an offline error shows Retry`.

- [ ] **Step 2: Run them to see them fail.** Run `pnpm --filter ui test -- ServersScreen Rail`. Expected: FAIL.

- [ ] **Step 3: Build with `/impeccable`.** Invoke the `impeccable` skill with a craft or build brief for this screen. Give it the spec's §6.3, the surface file's rules (one lit button per view, so a card's Play is raised at rest and lit on hover or focus; status in words; no Mojang or Modrinth branding) and the 960 × 600 minimum. Strings:
  - "Servers" / "Servidores";
  - subtitle "Find a Perland server and press Play." / "Encuentra un servidor de Perland y presiona Jugar.";
  - "Public servers" / "Servidores públicos";
  - "Sleeping, Play wakes it" / "Dormido, Jugar lo despierta";
  - "Starting" / "Iniciando";
  - "{{online}} / {{max}} playing" / "{{online}} / {{max}} jugando";
  - "Joining…" / "Entrando…";
  - "Join from Minecraft: {{address}}" / "Entra desde Minecraft: {{address}}";
  - "No servers match. Try fewer filters." / "Ningún servidor coincide. Prueba con menos filtros.";
  - "Clear filters" / "Quitar filtros";
  - "No public servers yet." / "Aún no hay servidores públicos.";
  - the sorts: "Most players" / "Más jugadores", "Newest" / "Más nuevos", "Name A–Z" / "Nombre A–Z";
  - the filters: "With mods" / "Con mods" (Any / With mods / Without mods: "Cualquiera" / "Con mods" / "Sin mods"), "Minecraft version" / "Versión de Minecraft", "Has room" / "Hay lugar", "Category" / "Categoría", "Language" / "Idioma", "Region" / "Región";
  - the 10 `servers.category.*` names and `servers.language.en` / `servers.language.es` ("English" / "Español").

  Play's flow and states as in spec §6.3. The page's `already_running` error navigates to that instance's Home instead of toasting.

- [ ] **Step 4: Run.** Run `pnpm --filter ui test`, `typecheck` and `lint`. Expected: PASS.

- [ ] **Step 5: Look at it.** In `pnpm --filter ui dev:mock` (Playwright with Chromium at `/opt/pw-browsers/chromium`), take screenshots at 960 × 600 with the wide and the narrow rail, in en and es: the list, a hovered card, the sleeping card, the closed scenario, and Play on a sleeping server reaching "Starting the server…". Fix what impeccable's finish review flags.

- [ ] **Step 6: Commit.** Commit `feat(ui): Servers screen with Perland's public servers (#50)`.

### Task 7: Whole-branch verification

- [ ] **Step 1: Run every CLAUDE.md "Before committing" check:**
  - `pnpm --filter ui build`;
  - `cargo fmt --all`;
  - `cargo clippy --workspace --all-targets -- -D warnings`;
  - `cargo test --workspace`;
  - `pnpm -r lint`;
  - `pnpm -r typecheck`;
  - `pnpm -r test`.

  Expected: all pass.
- [ ] **Step 2: Scan for tokens.** Run `git diff origin/main --stat`, then `git diff origin/main | grep -nE "[A-Za-z0-9_-]{43}"`. The only 43-character strings allowed are the fake `A`/`B`/`C` tokens.
- [ ] **Step 3: Commit** any fixes: `chore: verification fixes (#50)`.
