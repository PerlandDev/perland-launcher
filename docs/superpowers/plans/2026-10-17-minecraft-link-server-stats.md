# Link the Minecraft account, and per-server achievements and playtime: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A signed-in Perland player links their Minecraft account with a code typed in the game, and sees their achievements and playtime per server on server cards and on a server instance's Home.

**Architecture:** launcher-core gains three player calls (`link_code`, `unlink`, `stats`) and the `minecraft` field on the player and the stored profile. launcher-app wraps them in four commands (`perland_link_code`, `perland_link_poll`, `perland_unlink`, `perland_stats`) over the existing session, and src-tauri wires them. The UI adds a `MinecraftLink` row to the Perland account card, a `ServerStatsLine` on server cards and a plate on `ServerPanel`. All of these show only while `perland_stats` answers `Some`.

**Tech Stack:** Rust (tokio, reqwest, serde, wiremock), Tauri 2, React + TypeScript, React Query, zod, Vitest + Testing Library, i18next.

**Spec:** `docs/superpowers/specs/2026-10-17-minecraft-link-server-stats-design.md`. Read it before any task. The Engine contract is in `/home/user/perlandengine/internal/playersapi/dtos.go` (read only).

## Global Constraints

- `launcher-core` and `launcher-app` never depend on Tauri or UI crates. `src-tauri` is wiring only and has no tests.
- TDD: write the failing test first. Pure logic gets `#[cfg(test)]` unit tests, HTTP gets `crates/launcher-core/tests/perland.rs` with wiremock, and the UI gets Vitest + Testing Library.
- Never run fuzzing, stress or load tests locally.
- Errors shown to players are `LauncherError`. The new code is `minecraft_link_closed`, in the `// Perland account` group, with category network and message "Linking isn't available right now."
- Never log, `Debug`-print or put in an error a `plt_` token or a link code. A link code may only travel to the window.
- Every UI string goes in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). `errors.minecraft_link_closed.message` goes in both.
- `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- UI styling goes through the `/impeccable` skill (`impeccable:impeccable`), `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`. One lit (amber) button per view: Play on Home and Servers. On Accounts, the link row's buttons are raised or ghost, never amber.
- Calm when off: signed out, accounts off, 503 `server_addon_disabled` or `player_accounts_disabled`, or a 404 on `/stats`. These render no row, line or plate, and no error or toast.
- Before each commit, run the checks the task touches. Before the PR, run all of them: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm -r test`.

## Review Focus

1. **The add-on turned off while the player is linking:** the poll keeps getting `/me` without `minecraft`, and the code runs out normally. Pinned by the UI expiry test (Task 4).
2. **A marker file written by #48 (no `minecraft` key):** it must still load, or every signed-in player is signed out by the upgrade. Pinned in Task 1.
3. **Stats for a server whose invite was regenerated:** the old token matches no card or instance and shows nothing. It must not crash or pick another server. Pinned in Task 2 (an unmatched token gives `instance: None`).
4. **The Engine answers `/stats` with a network failure:** no row, no line and no toast. The query's error stays silent. Pinned in Task 3 (the hook doesn't push errors) and Task 4 (the row is hidden on an error).
5. **A sign-out while the code is showing:** polling stops, and the next player never sees the previous player's code or stats. Pinned in Task 4 (`switched` removes `qk.perlandStats`, and the poll stops on `perland_signed_out`).

---

### Task 1: launcher-core: the Minecraft link and stats calls

**Files:**
- Modify: `crates/launcher-core/src/perland/api.rs`, `crates/launcher-core/src/perland/mod.rs` (re-exports), `crates/launcher-core/src/perland/session.rs`, `crates/launcher-core/src/error.rs`
- Test: `crates/launcher-core/tests/perland.rs`, plus unit tests in `api.rs` and `session.rs`

**Interfaces:**
- Produces:
  - DTOs, all `Deserialize` with camelCase and serde defaults:
    - `pub struct MinecraftAccount { pub uuid: String, pub name: String, pub linked_at: String }`
    - `Player.minecraft: Option<MinecraftAccount>`
    - `pub struct LinkCode { pub code: String, pub expires_in: u64 }`, with a manual `Debug` that hides `code`
    - `pub struct StatsInvite { pub token: String }`, with a manual `Debug` that hides `token`
    - `pub struct ServerStats { pub name: String, pub invite: Option<StatsInvite>, pub playtime_seconds: u64, pub advancements: u32 }`
    - `pub struct PlayerStats { pub minecraft: Option<MinecraftAccount>, pub servers: Vec<ServerStats> }`
  - `PlayerClient` methods:
    - `link_code(&self, token: &str) -> Result<LinkCode>`
    - `unlink(&self, token: &str) -> Result<()>`
    - `stats(&self, token: &str) -> Result<Option<PlayerStats>>`
  - `Profile.minecraft: Option<String>` (the linked name)
  - `ErrorCode::MinecraftLinkClosed` (`"minecraft_link_closed"`)
  - Everything new is re-exported from `launcher_core::perland`.

- [ ] **Step 1: Write the failing wiremock tests** in `tests/perland.rs`, following the file's existing helpers and the bearer header checks:
  - `link_code_success`: `POST /api/v1/player/minecraft/link-code` with `Authorization: Bearer plt_…` answers `{"success":true,"data":{"code":"ABCD-EFGH","expiresAt":"…","expiresIn":600}}`. Expect `LinkCode { code: "ABCD-EFGH", expires_in: 600 }`.
  - `link_code_addon_off_is_minecraft_link_closed`: a 503 with data `{"code":"server_addon_disabled"}` gives error code `MinecraftLinkClosed`, category network.
  - `unlink_success`: `DELETE /api/v1/player/minecraft` with the bearer answers `{"data":{"unlinked":true}}`. Expect `Ok(())`.
  - `stats_success`: data `{"minecraft":{"uuid":"069a…","name":"Moi","linkedAt":"…"},"servers":[{"name":"asalith","invite":{"token":"A…(43)","url":"https://play.example.com/A…"},"playtimeSeconds":7200,"advancements":12,"advancementIds":["minecraft:story/root"],"firstSeenAt":"…","lastSeenAt":"…"},{"name":"private","playtimeSeconds":60,"advancements":0,"advancementIds":[]}]}`. Expect `Some`, `minecraft.name == "Moi"`, two servers, the first with `invite.token` and `advancements == 12`, the second with `invite: None`.
  - `stats_off_is_none`: one case each for a 503 with `server_addon_disabled`, a 503 with `player_accounts_disabled`, and a plain 404. All give `Ok(None)`.
  - `stats_401_is_signed_out`: a plain 401 gives `PerlandSignedOut`.
- [ ] **Step 2: Write the failing unit tests:**
  - `api.rs`: `player_minecraft_parses_and_is_optional`, where `{"id":"p"}` gives `None` and `{"id":"p","minecraft":{"name":"Moi","uuid":"u","linkedAt":"t"}}` gives `Some` with the name. Also `link_code_debug_hides_code`.
  - `session.rs`:
    - `profile_carries_minecraft_name`;
    - `marker_without_minecraft_still_loads`: write the #48 marker JSON `{"engine":"https://api.example.com/","name":"Steve","email":null,"discord":true}` plus a secret in `MemoryStore`, then `load()` gives `Some` with `profile.minecraft == None`;
    - `marker_round_trips_minecraft`.
- [ ] **Step 3: Run them and see them fail:** `cargo test -p launcher-core perland`, which fails to compile on the missing items.
- [ ] **Step 4: Implement.**
  - The calls:
    - `link_code` is `POST`, using `BODY_CAP` and `Route::Plain`.
    - `unlink` is `DELETE`. Add `Method::DELETE` use; the body is ignored.
    - `stats` is `GET` with a new `STATS_CAP` of 4 MiB (200 servers × 2000 IDs can pass 1 MiB). Before `ok()`, check 404, and 503 whose failure code is `server_addon_disabled` or `player_accounts_disabled`: both give `Ok(None)`.
  - In `refusal`, add `Some("server_addon_disabled") => minecraft_link_closed(&host)`.
  - `Marker.minecraft: Option<String>` with `#[serde(default)]`, written from `Profile.minecraft` and read back into it.
  - `Profile::from_player` takes `player.minecraft.as_ref().map(|m| m.name.clone())`.
- [ ] **Step 5: Run and see them pass:** `cargo test -p launcher-core`, then `cargo clippy -p launcher-core --all-targets -- -D warnings`.
- [ ] **Step 6: Commit:** `feat(core): Minecraft link code, unlink and per-server stats calls (#52)`

### Task 2: launcher-app and src-tauri: the four commands

**Files:**
- Modify:
  - `crates/launcher-app/src/perland.rs`: `PlayerApi`, `CorePlayer`, and `account()` for `minecraft`;
  - `crates/launcher-app/src/dto.rs`;
  - `crates/launcher-app/src/testing.rs`: `FakePlayer`, and `sample_player()` with `minecraft: None`;
  - `crates/launcher-app/src/lib.rs`, if modules are declared there;
  - `crates/launcher-app/tests/contract.rs`.
- Create: `crates/launcher-app/src/minecraft_link.rs` (the commands and their tests).
- Modify: `src-tauri/src/commands.rs` and `src-tauri/src/main.rs` (wrappers and `invoke_handler` registration, like `perland_status`).
- Generated: `ui/src/ipc/contract/*.json` (by the contract test).

**Interfaces:**
- Consumes: Task 1's `PlayerClient::{link_code, unlink, stats}`, `LinkCode`, `PlayerStats`, `ServerStats`, `Profile.minecraft` and `ErrorCode::MinecraftLinkClosed`.
- Produces:
  - `PlayerApi` gains:
    - `fn link_code(&self, engine: Url, token: String) -> BoxFuture<Result<LinkCode>>`
    - `fn unlink(&self, engine: Url, token: String) -> BoxFuture<Result<()>>`
    - `fn stats(&self, engine: Url, token: String) -> BoxFuture<Result<Option<PlayerStats>>>`
  - `FakePlayer` gains `link_code: Mutex<Result<LinkCode>>` (default `ABCD-EFGH`, 600), `unlink: Mutex<Result<()>>` and `stats: Mutex<Result<Option<PlayerStats>>>` (default `Ok(Some(empty))`), recorded under the routes `"link_code"`, `"unlink"` and `"stats"`.
  - DTOs, `Serialize` and camelCase:
    - `PerlandAccount.minecraft: Option<String>`
    - `pub struct LinkCodeView { pub code: String, pub expires_in: u64 }`, with a manual `Debug` hiding `code`
    - `pub struct PerlandStats { pub servers: Vec<ServerStatsView> }`
    - `pub struct ServerStatsView { pub token: String, pub instance: Option<String>, pub name: String, pub playtime_seconds: u64, pub advancements: u32 }`. `token` is an invite credential like `PublicServerCard.token`, so give it a manual `Debug` too.
  - `AppCore` methods:
    - `pub async fn perland_link_code(&self) -> Result<LinkCodeView>`
    - `pub async fn perland_link_poll(&self) -> Result<Option<String>>`
    - `pub async fn perland_unlink(&self) -> Result<()>`
    - `pub async fn perland_stats(&self) -> Result<Option<PerlandStats>>`
  - Tauri commands `perland_link_code`, `perland_link_poll`, `perland_unlink` and `perland_stats`, with no arguments, each through `logged(...)`.

- [ ] **Step 1: Write the failing app tests** in `minecraft_link.rs`, in the style of `perland.rs`'s tests (`TestCore` with a saved session):
  - `stats_signed_out_is_none_and_calls_nothing`;
  - `stats_drops_servers_without_invite_and_names_the_instance`: the fake answers two servers, one with an invite whose token equals a server instance's saved invite token (on the session's Engine), and one without an invite. Expect one view, with `instance == Some(<that id>)` and `advancements`/`playtime_seconds` copied.
  - `stats_unmatched_token_has_no_instance`;
  - `stats_none_from_engine_is_none`;
  - `stats_401_forgets_the_session`, then `perland_status().account` is `None`;
  - `link_code_signed_out_is_perland_signed_out`;
  - `link_code_goes_to_the_session_engine`: check `calls` for the route `"link_code"` and the session's Engine;
  - `link_poll_returns_the_name_and_status_carries_it`: the fake `me` has `minecraft` set to `Moi`. Expect `Some("Moi")`, then `perland_status().account.minecraft == Some("Moi")` with no further `/me` call.
  - `link_poll_401_forgets_and_errors`;
  - `unlink_clears_the_name`.
- [ ] **Step 2: Run and see them fail:** `cargo test -p launcher-app minecraft_link`.
- [ ] **Step 3: Implement.**
  - `perland_link_poll` calls `me` on the session's Engine and keeps the fresher profile through the existing `refresh_profile` (make it `pub(crate)`). On a 401 it calls `drop_session` and returns the error. It also resets `state.checked` to now.
  - `perland_unlink` saves the profile with `minecraft = None` after the Engine answers.
  - `perland_stats` builds a token-to-instance map from `self.instances.list()` with `servers::server_source`. It keeps an instance only when its saved Engine parses to the session's Engine origin.
  - Every command that gets `PerlandSignedOut` calls `drop_session(&session.token)`.
  - A signed-out `perland_link_code` and `perland_unlink` answer the same error the API's `signed_out` builds: code `PerlandSignedOut`, category auth, "You were signed out of Perland.".
- [ ] **Step 4: Add the contract fixtures** in `tests/contract.rs`:
  - `check("link_code", LinkCodeView { code: "ABCD-EFGH", expires_in: 600 })`;
  - `check("perland_stats", vec![Some(PerlandStats{…one server with an instance, one without…}), None])`;
  - add `minecraft: Some("Steve")` to one `PerlandStatus` fixture account and `None` to the others.

  Then run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, and run `cargo test -p launcher-app --test contract` again to confirm it passes.
- [ ] **Step 5: Wire src-tauri:**
  - add the four wrappers to `commands.rs`, next to the `perland_*` ones;
  - register them in `main.rs`'s `generate_handler!`.

  Check with `cargo build -p perland-launcher` (or the src-tauri crate's name in `src-tauri/Cargo.toml`) and `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 6: Run** `cargo test --workspace` and `cargo fmt --all`.
- [ ] **Step 7: Commit:** `feat(app): link-code, link poll, unlink and stats commands (#52)`

### Task 3: UI state: schemas, commands, queries, mocks and strings

**Files:**
- Modify:
  - `ui/src/ipc/schema.ts` and `ui/src/ipc/commands.ts`;
  - `ui/src/state/queries.ts` and `ui/src/state/handleEvent.ts`;
  - `ui/src/ipc/mockPerland.ts` and `ui/src/ipc/mock.ts`, if the scenario parser or the command routing need it;
  - `ui/src/i18n/en.json` and `ui/src/i18n/es.json`: only `errors.minecraft_link_closed.message` here. Component strings come with their tasks.
- Test: `ui/src/ipc/contract.test.ts` (already runs every fixture; make sure the new ones get schemas), `ui/src/ipc/mockPerland.test.ts`, `ui/src/state/handleEvent.test.ts` and `ui/src/state/perlandQueries.test.tsx`.

**Interfaces:**
- Consumes: Task 2's command names and DTO shapes (camelCase JSON).
- Produces:
  - zod schemas:
    - `perlandAccount` gains `minecraft: z.string().nullable()`;
    - `linkCode = z.strictObject({ code: z.string(), expiresIn: z.number() })`;
    - `serverStats = z.strictObject({ token, instance: nullable, name, playtimeSeconds, advancements })`;
    - `perlandStats = z.strictObject({ servers: z.array(serverStats) })`;
    - with the types `LinkCode`, `ServerStats` and `PerlandStats`.
  - `commands`:
    - `perlandLinkCode(): Promise<LinkCode>`
    - `perlandLinkPoll(): Promise<string | null>`
    - `perlandUnlink(): Promise<null>`
    - `perlandStats(): Promise<PerlandStats | null>`
  - Queries:
    - `qk.perlandStats = ['perlandStats']`;
    - `usePerlandStats()`: `useQuery({ queryKey: [...qk.perlandStats, status.data?.account?.minecraft ?? null], queryFn: commands.perlandStats, enabled: Boolean(status.data?.account), staleTime: Infinity })`, so a link made elsewhere (seen through `perland_status`) reads the stats again; invalidating or removing `qk.perlandStats` matches it as a prefix, reading `usePerlandStatus()` itself. Its error is never toasted; follow how `useYourServers` keeps quiet.
    - `useServerStats(token: string): ServerStats | undefined`;
    - `useInstanceStats(id: string): ServerStats | undefined`.
  - `handleEvent`: `qk.perlandStats` joins the run-idle invalidation list beside `qk.playtime`.
  - Mock scenarios: `PerlandScenario` gains `'linked' | 'noAddon'`.
    - `signedIn`: not linked, with stats `{ servers: [] }`.
    - `linked`: account `minecraft: 'Steve'`, with stats for the owner token (`advancements: 12`, `playtimeSeconds: 108000`), for the shared followed token (`advancements: 3`, `playtimeSeconds: 2400`), and for the mock server instance's token with its `instance` id (see `joinedInstance`/`inviteOf` in `mockServers.ts`), `advancements: 7`.
    - `noAddon`: stats `null`.
    - `perland_link_code` answers `{ code: 'ABCD-EFGH', expiresIn: 600 }`.
    - `perland_link_poll` answers `null` twice, then `'Steve'` and sets the account's `minecraft`.
    - `perland_unlink` clears it.
    - Exported constants: `MOCK_LINK_CODE = 'ABCD-EFGH'` and `MOCK_LINK_POLLS = 3`.

- [ ] **Step 1: Write the failing tests:**
  - `mockPerland.test.ts`:
    - the link flow (code, then `null`, `null`, `'Steve'`, then status `minecraft === 'Steve'`);
    - `noAddon` stats `null`;
    - `linked` stats include the instance id;
    - `perlandScenario('linked')` is accepted.
  - `handleEvent.test.ts`: run idle invalidates `qk.perlandStats`.
  - `perlandQueries.test.tsx`: `usePerlandStats` doesn't fetch while signed out, and fetches again when `account.minecraft` changes from `null` to a name.
- [ ] **Step 2: Run and see them fail:** `pnpm --filter ui test -- --run mockPerland handleEvent perlandQueries`.
- [ ] **Step 3: Implement.** In `PerlandAccountCard.switched`, also `qc.removeQueries({ queryKey: qk.perlandStats })`. Add `errors.minecraft_link_closed.message` to en ("Linking isn't available right now.") and es ("La vinculación no está disponible ahora.").
- [ ] **Step 4: Run** `pnpm --filter ui test -- --run`, `pnpm -r typecheck` and `pnpm -r lint`.
- [ ] **Step 5: Commit:** `feat(ui): Minecraft link and stats state, mocks (#52)`

### Task 4: UI: the Minecraft account row on Accounts

**Files:**
- Create: `ui/src/features/accounts/MinecraftLink.tsx` and `ui/src/features/accounts/MinecraftLink.test.tsx`
- Modify: `ui/src/features/accounts/PerlandAccountCard.tsx` (render `<MinecraftLink />` in the signed-in body, under the profile panel), and `en.json`/`es.json` (`perland.minecraft.*`)
- Possibly modify: the stylesheet the card uses, if a class is needed (see `ui/src/styles/`), and `.impeccable/surfaces/ui-src-app-tsx.md` (a short "Minecraft link (#52), decided 2026-10-17" entry).

**Interfaces:**
- Consumes: Task 3's `usePerlandStats`, `usePerlandStatus`, `commands.perlandLinkCode`/`perlandLinkPoll`/`perlandUnlink`, `qk.perland`, `qk.perlandStats`, `MOCK_LINK_CODE` and `MOCK_LINK_POLLS`.
- Produces: `export function MinecraftLink()`, which renders `null` unless signed in and `usePerlandStats().data` is non-null.

Copy (en / es):

| Key | en | es |
|---|---|---|
| `perland.minecraft.title` | Minecraft account | Cuenta de Minecraft |
| `perland.minecraft.pitch` | Link your Minecraft account to see your achievements on each server. | Vincula tu cuenta de Minecraft para ver tus logros en cada servidor. |
| `perland.minecraft.link` | Link Minecraft account | Vincular cuenta de Minecraft |
| `perland.minecraft.instructions` | Type {{command}} on a Perland server. | Escribe {{command}} en un servidor de Perland. |
| `perland.minecraft.copy` | Copy command | Copiar comando |
| `perland.minecraft.copied` | Command copied | Comando copiado |
| `perland.minecraft.expiresIn` | Expires in {{time}} | Vence en {{time}} |
| `perland.minecraft.expired` | The code expired. | El código venció. |
| `perland.minecraft.newCode` | Get a new code | Obtener un código nuevo |
| `perland.minecraft.linked` | Linked to {{name}} | Vinculada a {{name}} |
| `perland.minecraft.unlink` | Unlink | Desvincular |
| `perland.minecraft.linkedAnnounce` | Your Minecraft account {{name}} is linked. | Tu cuenta de Minecraft {{name}} quedó vinculada. |

- [ ] **Step 1: Invoke the `impeccable:impeccable` skill** for this row (an Accounts surface, Operate mode), and read `DESIGN.md` and the surface file. The visual decisions are yours within them: the code in large tabular type, quiet ink, no amber, the panel style of the card above.
- [ ] **Step 2: Write the failing tests** in `MinecraftLink.test.tsx`, using fake timers and the mock IPC as `PerlandAccountCard.test.tsx` does:
  - `hidden when stats is null` (`noAddon`), `hidden when signed out`;
  - `links with a code`:
    1. press Link Minecraft account; `ABCD-EFGH` shows with the instructions holding `/perland link ABCD-EFGH`;
    2. Copy writes `/perland link ABCD-EFGH` (mock the clipboard as other tests do);
    3. advance 3 s three times; "Linked to Steve" shows, and the announcement fires;
  - `counts down and expires`: `Expires in 10:00`, then after 1 s `9:59`; after 600 s "The code expired." with Get a new code, and polling stopped (count the poll calls);
  - `cancel goes back and stops polling`;
  - `unlink shows the link button again` (`linked`);
  - `link code refusal shows inline`: make the mock answer `minecraft_link_closed` and expect the translated message with `role="alert"`;
  - `a sign-out during the code stops polling`: the poll answers `perland_signed_out`, then no further poll and `qk.perland` is invalidated.
- [ ] **Step 3: Run and see them fail:** `pnpm --filter ui test -- --run MinecraftLink`.
- [ ] **Step 4: Implement `MinecraftLink`.**
  - Its state is `'idle' | 'code' | 'expired'`.
  - The code and its arrival time live in state. The countdown uses the shared formatting `m:ss`. The poll runs every 3000 ms in a `setInterval` cleared on state change and unmount, and ignores a result that arrives after it was stopped.
  - On a linked result: `invalidateQueries(qk.perland)`, `invalidateQueries(qk.perlandStats)`, the announcement, and focus to Unlink.
  - Unlink calls `perlandUnlink`, then invalidates both, and focus goes to Link.
  - Errors use the card's alert style.
- [ ] **Step 5: Run** `pnpm --filter ui test -- --run`, `pnpm -r lint` and `pnpm -r typecheck`. Then look at it in `pnpm --filter ui dev:mock` with `?perland=signedIn`, `?perland=linked` and `?perland=noAddon` (the `/impeccable` live check, or Playwright screenshots with the pre-installed Chromium). Keep the screenshots for the PR.
- [ ] **Step 6: Commit:** `feat(ui): link the Minecraft account on Accounts (#52)`

### Task 5: UI: achievements and playtime on server cards and Home

**Files:**
- Create: `ui/src/features/servers-list/ServerStatsLine.tsx` and `ui/src/features/servers-list/ServerStatsLine.test.tsx`
- Modify:
  - `ui/src/features/servers-list/PublicServerCard.tsx`: the line with the card's facts, under `NewsLine`;
  - `ui/src/features/servers/ServerPanel.tsx`: the plate;
  - `ui/src/features/home/Home.test.tsx`, or a new `ServerPanel.test.tsx`;
  - `en.json`/`es.json`: `servers.stats.*`;
  - the styles, if a plate class is needed.

**Interfaces:**
- Consumes: Task 3's `useServerStats(token)`, `useInstanceStats(id)` and `formatPlaytime` (`ui/src/ui/playtime.ts`).
- Produces:
  - `export function ServerStatsLine({ token }: { token: string })`, which renders `null` when there is nothing to say;
  - `export function statsText(t, stats: ServerStats): string`: `''`, or the parts joined by ` · `.

Copy (en / es):

| Key | en | es |
|---|---|---|
| `servers.stats.achievements_one` | {{count}} achievement | {{count}} logro |
| `servers.stats.achievements_other` | {{count}} achievements | {{count}} logros |
| `servers.stats.played` | {{time}} played | {{time}} jugado |
| `servers.stats.label` | Your stats on this server | Tus estadísticas en este servidor |

- [ ] **Step 1: Invoke `impeccable:impeccable`** for the card line and the Home plate (the Servers screen and Home surfaces). The line is quiet ink like the card's game line. The plate follows `InviteOnly onArt` and `NewsLine onArt`, never amber or lime.
- [ ] **Step 2: Write the failing tests:**
  - `statsText`:
    - `{advancements: 12, playtimeSeconds: 108000}` gives `"12 achievements · 1 day 6 h played"`. Use whatever `formatPlaytime` returns for 108000 in en, asserted through it rather than hard-coding the unit words.
    - `{advancements: 1, playtimeSeconds: 0}` gives `"1 achievement"`.
    - `{0, 59}` gives `''`.
    - `{0, 3600}` gives `"1 h played"` (via `formatPlaytime`).
  - `ServerStatsLine`:
    - renders nothing without stats;
    - renders the text with `linked` mock stats for the owner token;
    - renders nothing while signed out or with `noAddon`.
  - `ServerPanel`:
    - with `linked` stats for the mock server instance, "7 achievements" shows;
    - with `advancements: 0`, no plate;
    - with the local playtime present, the version line keeps its local time (no second playtime on Home).
- [ ] **Step 3: Run and see them fail:** `pnpm --filter ui test -- --run ServerStatsLine ServerPanel Home`.
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run** `pnpm --filter ui test -- --run`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm --filter ui build`. Then look at the Servers screen and a server instance's Home in `dev:mock` with `?perland=linked`, and keep the screenshots.
- [ ] **Step 6: Commit:** `feat(ui): achievements and playtime per server on cards and Home (#52)`

### Task 6: Records and the design system

**Files:**
- Modify: `DESIGN.md`, through the `impeccable:impeccable-documenter` agent or by hand following its format, only if Tasks 4 and 5 added a component or token worth recording. Also `.impeccable/surfaces/ui-src-app-tsx.md` (the decisions of Tasks 4 and 5, in the "decided 2026-10-17" style), and `README.md` if it lists features.

- [ ] **Step 1:** Record the new UI decisions:
  - the link row is raised or ghost, never amber;
  - the stats line is quiet ink;
  - the Home plate shows achievements only, beside the local playtime.
- [ ] **Step 2:** Run every check from Global Constraints. All must pass.
- [ ] **Step 3: Commit:** `docs: record the Minecraft link and stats surfaces (#52)`
