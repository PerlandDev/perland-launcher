# Manage your server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A player who owns or is a member of a server's team runs it from the launcher (state, Start/Stop/Restart, address, crash cause, players, console, a short settings list, More in Perland), and can sign in with GitHub through the panel.

**Architecture:** `launcher-core` trades the keyring device session (`plt_…`) for per-team session JWTs (`perland/session_jwt.rs`), talks to the Engine's stack routes with them (`perland/stacks.rs`) and reads the log stream (`perland/log_stream.rs`). `launcher-app` exposes it through a `StackApi` trait (`manage.rs`, `FakeStack` in tests), merges managed servers into Your servers, and streams console lines as `AppEvent::ServerLog`. The UI adds a `manage` route with Overview, Players, Console and Settings. Everything degrades on today's Engine: missing routes fall back, never break sign-in, Your servers or the free server.

**Tech Stack:** Rust (reqwest 0.12 with `stream`, tokio, serde, wiremock), Tauri wiring, React + TanStack Query + zod + i18next, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-21-manage-your-server-design.md`, building on the Engine's `docs/superpowers/specs/2026-10-21-one-perland-account-design.md` (PerlandEngine, branch `claude/youthful-cerf-4na8zc`).

## Global Constraints

- CLAUDE.md rules apply to every task: TDD; wiremock for HTTP (`crates/*/tests/*.rs`), `#[cfg(test)]` for logic, Vitest + Testing Library for UI; never run fuzz/stress/load tests locally.
- Never a token (`plt_…`, a JWT, a handoff code, an authorization code) in a log, an error, a `Debug` or a URL the app logs. Every type holding one has a manual `Debug` that omits it.
- Every UI string in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). Every new `ErrorCode` gets `errors.<code>.message` in both.
- `ui/src/ipc/contract/*.json` only through `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.
- UI styling through the `/impeccable` skill; direction `.impeccable/surfaces/ui-src-app-tsx.md`, system `DESIGN.md`.
- Plain player words, never operator jargon: no "stack", "compose", "RCON", "scope", "op", "pardon" in UI copy. Labels: "Make admin", "Remove admin", "Kick", "Ban", "Unban".
- Polling: the detail every 5 s while the state is `starting` or `stopping`, 30 s otherwise, only while the Manage screen is open.
- JWT reuse: a cached JWT is used while more than 5 minutes from its `exp`; one refresh in flight per tenant; a 401 on a JWT call drops it and retries once; a 401 from `player-session` signs the player out (as `/me` does today).
- Graceful degradation is a hard rule: a 404 (or 405) from a route the Engine may not have yet (`/auth/handoff`, `manage` field, `plan` field) falls back as Task notes say; it is never an error shown to the player and never breaks today's flows.
- Destructive actions (Stop, Restart, Ban, a version change, console `stop`/`restart`/`end`/`shutdown`) need a clear confirmation.
- Before each commit: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, the changed crate's tests; UI tasks also `pnpm --filter ui build`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.

## Review Focus

1. **A settings save must never drop what it did not show.** The Engine's PUT replaces the whole customization; a field left out returns to the default, and on today's Engine a modpack free server's `MODRINTH_*` keys sit in `customization.env`. The app patches the six fields onto the raw GET document and sends everything else back untouched. Test in Task 6 (`settings_put_keeps_every_field_it_did_not_change`).
2. **An Engine without the new routes.** `/auth/handoff` 404 opens the panel page plainly; Your servers entries without `manage` still dedupe the free server by its own view and team servers by address; memberships without `plan` still list. Tests in Tasks 1, 5 and 8.
3. **Two screens or polls racing a JWT refresh.** Concurrent `jwt_for(tenant)` calls make one `player-session` + one `select-tenant`. Test in Task 2 (`concurrent_callers_share_one_refresh`).
4. **The log stream after the screen closes, and a dropped connection.** `manage_console_close` (or a second open) ends the reader and nothing is emitted afterwards; a dropped stream reconnects after 5 s with `since` set to the last line's time, without duplicating lines. Tests in Tasks 4 and 7.
5. **Server-written text.** Console output, log lines, MOTDs and player names are data: `§` codes and ANSI escapes are stripped before display, a MOTD holding `§` is read only, and nothing is rendered as HTML. Tests in Tasks 4 (strip) and 10 (read-only MOTD).

---

## File Structure

- `crates/launcher-core/src/perland/auth.rs` (new): `AuthClient`: `player-session`, `memberships`, `select-tenant`, `handoff`, `auth/config`.
- `crates/launcher-core/src/perland/session_jwt.rs` (new): `jwt_exp`, `JwtCache`, `JwtSource` trait.
- `crates/launcher-core/src/perland/stacks.rs` (new): `StackClient` and its types.
- `crates/launcher-core/src/perland/log_stream.rs` (new): SSE parser, `strip_formatting`, `follow_logs`.
- `crates/launcher-core/src/perland/panel.rs`: `handoff_url`, `upgrade_url`, `launcher_sign_in_url`.
- `crates/launcher-core/src/perland/api.rs`: `YourServer.manage`, `SignInMethod::GitHub`.
- `crates/launcher-core/src/error.rs` + `ui/src/i18n/{en,es}.json`: new codes.
- `crates/launcher-app/src/manage.rs` (new): `StackApi`, `CoreStack`, every `manage_*` method.
- `crates/launcher-app/src/manage_console.rs` (new): the log stream task and batching.
- `crates/launcher-app/src/{dto.rs,events.rs,runner.rs,testing.rs,your_servers.rs,perland.rs,lib.rs}`.
- `crates/launcher-app/tests/contract.rs`, `src-tauri/src/{commands.rs,main.rs}`.
- `ui/src/ipc/{schema.ts,commands.ts,mockManage.ts,mock.ts}`, `ui/src/state/{route.ts,queries.ts,handleEvent.ts,manageLog.ts}`.
- `ui/src/features/manage/` (new): `ManageScreen.tsx`, `ManageOverview.tsx`, `ManagePlayers.tsx`, `ManageConsole.tsx`, `ManageSettings.tsx`, `causeLine.ts`, tests beside each.
- `ui/src/features/servers-list/{YourServers.tsx,ManagedServerCard.tsx}`, `ui/src/features/free-server/FreeServerCard.tsx`, `ui/src/features/accounts/PerlandAccountCard.tsx`.

---

### Task 1: AuthClient (device session → JWT, memberships, select-tenant, handoff)

**Files:**
- Create: `crates/launcher-core/src/perland/auth.rs`
- Modify: `crates/launcher-core/src/perland/mod.rs` (export)
- Test: `crates/launcher-core/tests/perland_auth.rs`

**Interfaces:**
- Produces:
  - `pub struct SessionJwt(String)` with `pub fn as_str(&self) -> &str`, manual `Debug` printing `SessionJwt(<redacted>)`.
  - `pub struct Membership { pub tenant_id: String, pub tenant_name: String, pub display_name: String /* branding.displayName, else tenant_name */, pub role: String, pub plan: Option<String> /* plan.name, absent on today's Engine */ }`
  - `pub struct Handoff { pub code: String, pub expires_in: u64 }` with redacting `Debug`.
  - `pub struct AuthClient` with `new(http: reqwest::Client, engine: Url)`, and:
    - `async fn player_session(&self, device: &str) -> Result<SessionJwt>`: `POST /api/v1/auth/player-session`, `Authorization: Bearer plt_…`, answer `data.token`.
    - `async fn memberships(&self, jwt: &SessionJwt) -> Result<Vec<Membership>>`: `GET /api/v1/auth/memberships`, `X-User-Token`.
    - `async fn select_tenant(&self, jwt: &SessionJwt, tenant_id: &str) -> Result<SessionJwt>`: body `{tenantId}`, answer `data.token`.
    - `async fn handoff(&self, device: &str, tenant_id: &str) -> Result<Option<Handoff>>`: `POST /api/v1/auth/handoff` body `{tenantId}`; `Ok(None)` on 404 or 405 (Engine without it).
    - `async fn launcher_github(&self) -> bool`: `GET /api/v1/auth/config`, `data.launcherGitHub == true`; any failure is `false`.
  - Errors through the same mapping as `PlayerClient::refusal` (reuse: make `refusal`/`send` helpers shared, e.g. move the common `send`/`Reply`/`Failure` into a private `perland/wire.rs` used by both clients; keep `PlayerClient` behaviour identical). 401 → `PerlandSignedOut`; 403 `account_disabled` → `PerlandSignedOut` too (the session no longer works); 503 `player_accounts_disabled` → `PlayerAccountsClosed`; 404 on `player-session` → `PlayerAccountsClosed` (an Engine without it).

- [ ] **Step 1: Write failing wiremock tests** in `tests/perland_auth.rs` (copy the server setup style of `tests/perland.rs`):
  - `player_session_sends_the_device_token_as_bearer`: mock `POST /api/v1/auth/player-session` with header `authorization: Bearer plt_abc` answering `{"success":true,"data":{"token":"eyJ.a.b","user":{},"newUser":false}}`; assert `jwt.as_str() == "eyJ.a.b"`.
  - `player_session_401_is_signed_out` (`ErrorCode::PerlandSignedOut`), `player_session_404_is_accounts_closed`.
  - `memberships_send_the_jwt_in_x_user_token_and_read_plan`: two entries, one with `"plan":{"name":"player-free","title":"Free"}` and `"branding":{"displayName":"Perland"}`, one without `plan`; assert `plan == Some("player-free")` and `None`, `display_name` values.
  - `select_tenant_posts_the_tenant_id`: body matcher `{"tenantId":"t-1"}`.
  - `handoff_404_is_none`, `handoff_answers_the_code` (`{"code":"hc_1","expiresIn":120}`), `handoff_sends_bearer_device_token`.
  - `launcher_github_reads_auth_config` (`true` when `launcherGitHub:true`; `false` when the field is absent and when the route 404s).
  - `debug_never_prints_tokens`: `format!("{:?}", jwt)` and the `Handoff` contain neither the token nor the code.
- [ ] **Step 2: Run** `cargo test -p launcher-core --test perland_auth` — expected FAIL (module missing).
- [ ] **Step 3: Implement** `auth.rs` (and the shared `wire.rs` if extracted). Body cap 64 KiB as `BODY_CAP`.
- [ ] **Step 4: Run** `cargo test -p launcher-core --test perland_auth --test perland` — PASS (the old player tests still pass).
- [ ] **Step 5: Commit** `feat(core): trade the device session for Perland session JWTs`.

### Task 2: JwtCache

**Files:**
- Create: `crates/launcher-core/src/perland/session_jwt.rs` (unit tests inside)
- Modify: `crates/launcher-core/src/perland/mod.rs`

**Interfaces:**
- Consumes: `SessionJwt`, `AuthClient` (Task 1).
- Produces:
  - `pub fn jwt_exp(jwt: &str) -> Option<u64>`: base64url-decode (no padding, `base64::engine::general_purpose::URL_SAFE_NO_PAD`, tolerate padding) the middle segment, read `exp` (seconds). No verification.
  - `pub trait JwtSource: Send + Sync { fn player_session(&self) -> BoxFuture<'_, Result<SessionJwt>>; fn select_tenant<'a>(&'a self, base: &'a SessionJwt, tenant: &'a str) -> BoxFuture<'a, Result<SessionJwt>>; }` (BoxFuture = `Pin<Box<dyn Future<Output = T> + Send + 'a>>`).
  - `pub struct JwtCache` (`Default`): `pub async fn jwt_for(&self, source: &dyn JwtSource, tenant: &str, now: u64) -> Result<SessionJwt>`; `pub async fn base(&self, source: &dyn JwtSource, now: u64) -> Result<SessionJwt>` (the `player-session` JWT, cached the same way; used for `memberships`); `pub fn forget(&self, tenant: &str)`; `pub fn clear(&self)` (sign-out or a new session).
  - `pub const REFRESH_MARGIN_SECS: u64 = 300;`
  - A token is fresh when `exp > now + 300`; a token with no readable `exp` is used once and not cached.
  - One refresh in flight per key: a `tokio::sync::Mutex` per tenant key (`HashMap<String, Arc<tokio::sync::Mutex<Option<SessionJwt>>>>` behind a `std::sync::Mutex`); the base JWT is its own key `""`.

- [ ] **Step 1: Write failing unit tests** with a `FakeSource` counting calls (returns JWTs built by a helper `jwt_with_exp(exp)` = `"h." + b64url({"exp":exp}) + ".s"`):
  - `exp_reads_the_payload` (`jwt_exp(&jwt_with_exp(1_700_000_000)) == Some(1_700_000_000)`; garbage → `None`).
  - `a_fresh_token_is_reused` (two `jwt_for("t1", now)` → 1 player_session, 1 select_tenant).
  - `a_token_within_five_minutes_of_exp_is_refreshed` (exp = now + 299 → second call refreshes; now + 301 → reused).
  - `tenants_are_cached_apart` ("t1" and "t2" → 2 select_tenant, 1 player_session).
  - `concurrent_callers_share_one_refresh`: 8 `jwt_for("t1")` joined with `futures_util::future::join_all`, the fake sleeping 10 ms (`tokio::time::sleep` under `#[tokio::test(start_paused = true)]`) → exactly 1 select_tenant.
  - `forget_drops_one_tenant`, `clear_drops_everything`.
  - `a_failed_refresh_caches_nothing` (fake errors once then succeeds → second call succeeds).
- [ ] **Step 2: Run** `cargo test -p launcher-core session_jwt` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(core): cache Perland session JWTs per team`.

### Task 3: StackClient and the new error codes

**Files:**
- Create: `crates/launcher-core/src/perland/stacks.rs`
- Modify: `crates/launcher-core/src/error.rs` (codes + the code-string test table), `crates/launcher-core/src/perland/mod.rs`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Test: `crates/launcher-core/tests/perland_stacks.rs`

**Interfaces:**
- Consumes: shared wire helpers (Task 1), `SessionJwt`.
- Produces (`pub` in `perland::stacks`, re-exported from `perland`):
  - New `ErrorCode`s (in a `// managing a server` group): `ServerNotRunning`, `ServerConsoleUnavailable` (Engine `rcon_unavailable`), `TooManyCommands`, `ServerConsoleTimeout` (`rcon_timeout`), `CommandNeedsConfirmation` (`lifecycle_command_requires_confirmation`), `TeamQuotaFull` (`quota_exceeded`), `PlanFeatureLocked` (param `field`), `SettingsChanged`, `VersionChangeNeedsConfirmation`, `DataLossNeedsConfirmation`, `MojangUnavailable`, `TooManyStreams`, `TooManyDiagnoses`, `ManagedBySource`, `ServerNotFound` (404 on a stack route), `ManageForbidden` (403 on a stack route: the role or plan does not allow it). Reuse `HostFull` and `FreeServerOverDisk` (params `usedBytes`, `limitBytes`). Code strings are the snake_case of the variant. Plain English messages, e.g. `ServerConsoleUnavailable`: "The server can't take commands right now. Try again in a minute."; `PlanFeatureLocked`: "Your plan doesn't include this."; `CommandNeedsConfirmation`: "This command stops the server.".
  - Types (serde camelCase, `#[serde(default)]`, unknown fields ignored):
    - `StackItem { stack_name, status, health, address: Option<String>, motd, version, kind, r#type }`
    - `StackDetail { stack_name, status, address: Option<String>, motd, version, r#type, pending_restart: bool, idle: Option<StackIdle> }`, `StackIdle { state, paused: bool, stops_at: Option<String> }`
    - `Online { running: bool, paused: bool, online: u32, max: u32, players: Option<Vec<String>> }`
    - `Diagnosis { outcome, oom_killed: bool, primary: Option<Issue> }`, `Issue { code, known: bool, severity, params: serde_json::Map<String, Value>, subject: Option<IssueSubject { kind, name, version }> }`
    - `NameList { running: bool, offline_mode: bool, unreadable: bool, names: Vec<String> }` built from whitelist `players[].name` and ops `ops[].name`; `BanList { running, unreadable, bans: Vec<Ban { name, reason: Option<String> }> }`
    - `NameResult { name, status, message: Option<String> }`
    - `SettingsDoc { revision: String, managed_by_source: Option<String>, pending_restart: bool, settings: serde_json::Value }` — `settings` is kept raw, whole.
    - `SettingsSaved { revision, pending: bool, restarted: bool, settings: Value }`
    - `ConsoleReply { outcome, output: String, code: Option<String> }`
    - `pub enum Lifecycle { Start, Stop, Restart }`, `pub enum ListKind { Whitelist, Ops, Bans }`, `pub enum Apply { Now, NextRestart }`
  - `pub struct StackClient` (`new(http, engine)`), every call takes `jwt: &SessionJwt` first, sends `X-User-Token`, escapes the stack as one path segment:
    - `get_all(jwt) -> Result<Vec<StackItem>>` (`GET /api/v1/stacks/get-all?kind=single`, cap 1 MiB)
    - `details(jwt, stack) -> Result<StackDetail>` (`?stats=false`)
    - `lifecycle(jwt, stack, Lifecycle) -> Result<()>` (`POST /stacks/{start|stop|restart}/{name}`, empty JSON body `{}`)
    - `diagnosis(jwt, stack) -> Result<Diagnosis>`
    - `online(jwt, stack) -> Result<Online>`; `kick(jwt, stack, names: &[String]) -> Result<Vec<NameResult>>`
    - `list(jwt, stack, ListKind) -> Result<NameList>` for whitelist/ops; `bans(jwt, stack) -> Result<BanList>`
    - `change(jwt, stack, ListKind, add: bool, names: &[String]) -> Result<Vec<NameResult>>`: whitelist `POST …/whitelist` / `…/whitelist/remove`, ops `…/ops` / `…/ops/remove`, bans `…/bans` / `…/bans/pardon`, body `{names}`.
    - `settings(jwt, stack) -> Result<SettingsDoc>`; `put_settings(jwt, stack, body: &Value, apply: Apply) -> Result<SettingsSaved>` (`PUT …/settings`, `?apply=next-restart` for `NextRestart`; timeout 150 s as `CREATE_TIMEOUT`, the Engine recreates the server inside the request)
    - `console(jwt, stack, command: &str, confirm: bool) -> Result<ConsoleReply>` (body `{command, confirm}`)
  - Refusals: Engine `data.code` first (table above; `plan_feature_locked` carries `field`, else `scope`, as param `field`), then 401 → `PerlandSignedOut`, 403 → `ManageForbidden`, 404 → `ServerNotFound`, 429 → rate limited, else `host_trouble`.

- [ ] **Step 1: Write failing wiremock tests**, one per call (path, method, `x-user-token` header, body, parsed answer), plus:
  - `stack_names_are_one_escaped_segment` (`"my world"` → `/api/v1/stacks/details/my%20world`).
  - `stop_sends_an_empty_body`.
  - `settings_keep_the_whole_document` (a GET with `customization.env.MODRINTH_MODPACK` comes back in `SettingsDoc.settings` unchanged).
  - `put_settings_next_restart_sets_the_query` (`apply=next-restart`), `put_settings_now_has_no_query`.
  - `refusals_map_to_codes`: a table test over (status, code, data) → `ErrorCode` for every code above, including `plan_feature_locked` with `{"field":"maxPlayers"}` → param `field == "maxPlayers"`, and `free_server_over_disk` → `FreeServerOverDisk`.
  - `a_401_is_signed_out`, `a_403_is_forbidden`, `a_404_is_not_found`.
  - `debug_holds_no_jwt` for `StackClient`.
- [ ] **Step 2: Add the codes to `error.rs`** (enum list and the `(ErrorCode, &str)` test table) and `errors.<code>.message` for each to `en.json` and `es.json`; run `pnpm --filter ui test -- i18n` (the i18n test checks both files and the code list).
- [ ] **Step 3: Run** `cargo test -p launcher-core --test perland_stacks` — FAIL, then implement `stacks.rs`, then PASS.
- [ ] **Step 4: Commit** `feat(core): the Engine's server routes for Manage`.

### Task 4: Log stream (SSE) and formatting strip

**Files:**
- Create: `crates/launcher-core/src/perland/log_stream.rs` (parser and strip unit tests inside)
- Test: `crates/launcher-core/tests/perland_log_stream.rs`

**Interfaces:**
- Consumes: `StackClient`'s engine/http, `SessionJwt`.
- Produces:
  - `pub fn strip_formatting(text: &str) -> String`: removes `§` + one following char, and ANSI CSI sequences (`ESC [ … final byte 0x40–0x7E`), and other C0 control chars except tab.
  - `pub struct LogLine { pub text: String, pub timestamp: Option<String> }`
  - `pub struct SseParser` with `pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent>`; `pub enum SseEvent { Log(LogLine), End(String) }`. Handles `\n` and `\r\n`, events split across chunks, multi-line `data:`, `:` comments ignored, unknown events ignored, invalid JSON data skipped.
  - `pub async fn follow_logs(client: &StackClient, jwt: impl Fn() -> BoxFuture<'static, Result<SessionJwt>>, stack: &str, mut on_lines: impl FnMut(Vec<LogLine>) + Send, stop: tokio::sync::watch::Receiver<bool>) -> Result<()>`: `GET /api/v1/stacks/{name}/logs/stream?tail=200&timestamps=true` with `X-User-Token`; after an `end` event or a dropped connection, waits `RECONNECT_AFTER = 5 s` and reconnects with `since=<last line's timestamp>` (and `tail=0`), dropping lines whose timestamp is `<=` the last seen; returns when `stop` turns true (checked while reading and while waiting). A 401 asks `jwt()` again once per attempt; 429 `too_many_streams` and 404 return the error. Line text passes through `strip_formatting`.

- [ ] **Step 1: Unit tests**: `strip_removes_section_codes_and_ansi` (`"§aHi \u{1b}[31mthere\u{1b}[0m"` → `"Hi there"`), `parser_reads_log_events`, `parser_joins_chunks` (an event split in 3 pushes), `parser_ignores_comments_and_unknown_events`, `parser_reads_end`, `parser_handles_crlf`.
- [ ] **Step 2: Wiremock tests** (`start_paused` tokio time where waiting): `follow_reads_lines_then_stops` (body of two `log` events and `end`; `stop` set from the callback after the lines → returns `Ok`), `follow_reconnects_with_since_after_end` (second mock matched on `since` query; no duplicate lines), `follow_returns_too_many_streams`.
- [ ] **Step 3: Implement** with `reqwest::Response::bytes_stream()` and `tokio::select!` on the stop receiver.
- [ ] **Step 4: Run** `cargo test -p launcher-core log_stream --test perland_log_stream` — PASS.
- [ ] **Step 5: Commit** `feat(core): follow a server's log stream`.

### Task 5: launcher-app StackApi, managed servers, detail, actions, diagnosis, panel

**Files:**
- Create: `crates/launcher-app/src/manage.rs` (unit tests inside, with `FakeStack`)
- Modify: `crates/launcher-app/src/{lib.rs,runner.rs,testing.rs,dto.rs,perland.rs}`, `crates/launcher-core/src/perland/panel.rs`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces:
  - `pub trait StackApi: Send + Sync + 'static` — the same shape as `PlayerApi` (every call takes `engine: Url, jwt: SessionJwt`), one method per `StackClient` call plus `player_session(engine, device)`, `memberships(engine, jwt)`, `select_tenant(engine, jwt, tenant)`, `handoff(engine, device, tenant) -> Option<Handoff>`, `launcher_github(engine) -> bool`. `CorePlayer`-like `CoreStack` over the no-redirect `HttpClient`. `Runner::stack(&self) -> Arc<dyn StackApi>`; `FakeStack` in `testing.rs` with scripted answers and a call log (like `FakePlayer`).
  - `AppCore` gains `jwts: launcher_core::perland::JwtCache` (cleared in `forget_session` and `adopt`).
  - `pub(crate) async fn manage_jwt(&self, tenant: &str) -> Result<(Session, SessionJwt)>` — through the cache; a `PerlandSignedOut` from `player-session` calls `drop_session`. `pub(crate) async fn with_jwt<T>(&self, tenant, call: impl Fn(Url, SessionJwt) -> BoxFuture<Result<T>>) -> Result<T>`: on `PerlandSignedOut` from the call, `jwts.forget(tenant)` and retry once; a second 401 is returned as is (the device session may still be fine).
  - DTOs (`dto.rs`, camelCase, `Serialize`):
    - `ManageRef { tenant_id: String, stack: String }`
    - `ManagedServer { tenant_id, team_name, stack, name /* = stack */, state: String, address: Option<String>, free: bool /* membership plan player-free, or team name starts with "player-" */ }`
    - `ManageDetail { tenant_id, stack, state: String, address: Option<String>, online: Option<u32>, max: Option<u32>, players: Vec<String>, pending_restart: bool, version: String, stops_at: Option<String> }`
    - `ManageCause { code: String, known: bool, subject: Option<String>, params: BTreeMap<String, String> }`, `ManageDiagnosis { failed: bool, oom_killed: bool, cause: Option<ManageCause> }`
  - `pub(crate) fn manage_state(status: &str, health: &str, idle: Option<&StackIdle>) -> &'static str` returning `"running" | "asleep" | "starting" | "stopping" | "stopped" | "unknown"`:
    - status `sleeping` or idle state `sleeping`, or `idle.paused` → `asleep`; idle `waking` → `starting`; idle `stopping` → `stopping`;
    - status `running` with health `starting` → `starting`; `running` → `running`; `restarting` → `starting`; `exited`, `dead`, `created` → `stopped`; `removing` → `stopping`; else `unknown`.
  - `AppCore` methods (all `Result`, all `None`/empty when signed out):
    - `managed_servers(&self) -> Result<Vec<ManagedServer>>`: `base` JWT → `memberships` → for each with role `owner` or `member` (never `viewer`), `get-all` with that team's JWT, in parallel (`join_all`); a team whose call fails is skipped and logged by code; order: free team first, then teams by display name, stacks by name. `PlayerAccountsClosed` (old Engine) → `Ok(vec![])`.
    - `manage_detail(tenant, stack) -> Result<ManageDetail>`: `details` and, when the state is `running`, `online` (an `online` failure leaves `online/max` `None`).
    - `manage_action(tenant, stack, action: &str /* start|stop|restart */) -> Result<()>`; unknown action → `LinkRefused`-style `Internal` error.
    - `manage_diagnosis(tenant, stack) -> Result<ManageDiagnosis>`: `failed = outcome == "failed"`; `params` values stringified.
    - `manage_open_panel(tenant, stack: Option<String>) -> Result<()>`: needs `config.panel` (else `LinkRefused`); `next` = `/stacks/<escaped stack>` or `/settings/upgrade` when `stack` is `None`; `handoff` with the device session → open `panel::handoff_url(panel, code, next)`; `None` (404/405) or any error (logged by code only) → open `panel::panel_page(panel, next)` plainly. The URL with the code is never logged.
  - `panel.rs`: `pub fn handoff_url(panel: &Url, code: &str, next: &str) -> Url` (`<panel>/login/handoff?code=&next=`), `pub fn panel_page(panel: &Url, next: &str) -> Url`, `pub fn upgrade_path() -> &'static str` (`"/settings/upgrade"`), `pub fn server_path(stack: &str) -> String` (escaped one segment, as `manage_url`).

- [ ] **Step 1: Unit tests in `manage.rs`**: `state_words` (table over the mapping above), `managed_servers_lists_owner_and_member_teams_only`, `managed_servers_on_an_old_engine_is_empty` (`player-session` → `PlayerAccountsClosed`), `managed_servers_skips_a_failing_team`, `a_jwt_401_retries_once_with_a_fresh_token`, `player_session_401_signs_out`, `detail_reads_players_only_while_running`, `diagnosis_names_the_cause`, `open_panel_uses_the_handoff`, `open_panel_without_handoff_opens_the_page` (fake answers `None`), `open_panel_upgrade_goes_to_settings_upgrade`, `open_panel_never_logs_the_code` (`tracing_test` or `testing.rs`'s log capture if present; otherwise assert the code is absent from every `TestHost` record except the opened URL).
  - `panel.rs` unit tests: `handoff_url_escapes_code_and_next`, `server_path_escapes_one_segment`.
- [ ] **Step 2: Run** `cargo test -p launcher-app manage` / `-p launcher-core panel` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS. `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat(app): list and run the servers a player manages`.

### Task 6: Players, settings and console commands in the app

**Files:**
- Modify: `crates/launcher-app/src/manage.rs`, `crates/launcher-app/src/dto.rs`, `crates/launcher-app/src/testing.rs`

**Interfaces:**
- Consumes: Task 5 (`with_jwt`, `FakeStack`).
- Produces:
  - DTOs: `ManagePlayers { running: bool, online: Vec<String>, whitelist: Vec<String>, admins: Vec<String>, bans: Vec<ManageBan { name, reason: Option<String> }>, unreadable: bool }`; `ManageListChange { list: String /* whitelist|admins|bans|online */, add: bool, names: Vec<String> }` (deserialize); `ManageChangeResult { name, status, message: Option<String> }`; `ManageSettings { revision, motd: String, motd_read_only: bool, mode: String, difficulty: String, max_players: Option<u32>, pvp: bool, enable_whitelist: bool, version: String, version_locked: bool, pending_restart: bool }`; `ManageSettingsSave { revision, motd: Option<String>, mode, difficulty, max_players: Option<u32>, pvp, enable_whitelist, version: Option<String>, apply: String /* now|next_restart */, confirm_version_change: bool, acknowledge_data_loss: bool }` (deserialize); `ManageSaved { settings: ManageSettings, restarted: bool, pending: bool }`; `ManageConsoleReply { ok: bool, output: String }`.
  - `manage_players(tenant, stack)`: `online` (only when running), whitelist, ops, bans in parallel; a list that fails alone becomes empty with `unreadable: true`.
  - `manage_players_change(tenant, stack, change: ManageListChange) -> Result<Vec<ManageChangeResult>>`: `online` + `add: false` → kick; `admins` → `ListKind::Ops`; `bans` + `add: false` → pardon. 1–50 names, else `Internal` error before any call.
  - `manage_settings(tenant, stack) -> Result<ManageSettings>`: from `settings.customization` (`motd`, `mode` default `survival`, `difficulty` default `easy`, `maxPlayers`, `pvp` default `true`, `enableWhitelist` default `false`) and `settings.version`; `motd_read_only` when the MOTD holds `§`; `version_locked` when `managed_by_source` is set, `settings.type` is `MODRINTH`, or `customization.env` holds `MODRINTH_MODPACK`.
  - `manage_settings_save(tenant, stack, save: ManageSettingsSave) -> Result<ManageSaved>`: GET the doc, then patch onto `doc.settings` (raw `Value`): `customization.{mode, difficulty, maxPlayers, pvp, enableWhitelist}` always, `customization.motd` only when `save.motd` is `Some` and the saved MOTD is not read only, `version` only when `Some`, not locked and different. Body `{revision: save.revision, settings, confirmVersionChange, acknowledgeDataLoss}`. Everything else in the document is sent back untouched.
  - `manage_console(tenant, stack, command, confirm) -> Result<ManageConsoleReply>`: output through `strip_formatting`; `ok = outcome == "ok"`.

- [ ] **Step 1: Unit tests**: `players_read_four_lists`, `a_failing_list_is_unreadable_not_an_error`, `kick_is_online_remove`, `admins_map_to_ops`, `unban_is_bans_remove`, `changes_refuse_more_than_fifty_names`, `settings_read_defaults`, `settings_motd_with_section_codes_is_read_only`, `settings_version_locked_for_a_modpack` (three cases), **`settings_put_keeps_every_field_it_did_not_change`** (GET doc with `customization.env.MODRINTH_MODPACK`, `viewDistance`, `ops`, `type: MODRINTH`; save changes `pvp` only; assert the PUT body equals the GET doc with only `pvp` (and the five always-written fields as they were) changed, `env` intact), `settings_save_sends_the_revision_and_confirmations`, `settings_save_skips_a_locked_version`, `settings_save_next_restart_passes_apply`, `console_strips_formatting`, `console_needs_confirmation_is_a_code` (`CommandNeedsConfirmation` passes through).
- [ ] **Step 2: Run** — FAIL; **Step 3: implement**; **Step 4: run** `cargo test -p launcher-app manage` — PASS.
- [ ] **Step 5: Commit** `feat(app): players, settings and console for Manage`.

### Task 7: Console log stream and `AppEvent::ServerLog`

**Files:**
- Create: `crates/launcher-app/src/manage_console.rs`
- Modify: `crates/launcher-app/src/{events.rs,app.rs,manage.rs,lib.rs}`, `crates/launcher-app/src/runner.rs` (StackApi gains `follow_logs`)

**Interfaces:**
- Consumes: `follow_logs` (Task 4), `with_jwt` (Task 5).
- Produces:
  - `AppEvent::ServerLog { tenant_id: String, stack: String, lines: Vec<String> }` (serde tag `server_log`, camelCase fields).
  - `StackApi::follow_logs(engine, jwt_source, stack, sink: mpsc::UnboundedSender<Vec<String>>, stop: watch::Receiver<bool>) -> BoxFuture<Result<()>>`.
  - `manage_console_open(tenant, stack) -> Result<()>`: one stream at a time per app (a second open, for any server, stops the first); spawns a task that batches lines and emits at most every `LOG_BATCH = 100 ms`, at most `LOG_BATCH_LINES = 200` lines per event (keeping the newest when more arrived). The first connect is awaited, so `manage_console_open` returns its error (e.g. `TooManyStreams`, `ServerNotFound`) to the UI; an error after that ends the stream, is logged by code only, and emits `AppEvent::ServerLogEnded { tenant_id, stack }` so the console can say "Live log paused" with Reconnect.
  - `manage_console_close(tenant, stack)`: stops the stream if it is that server's.
  - `AppCore` gains `console_stream: std::sync::Mutex<Option<(String, String, watch::Sender<bool>)>>`.

- [ ] **Step 1: Unit tests** (paused tokio time, `FakeStack::follow_logs` feeding lines through the sink): `lines_are_batched_every_100ms`, `a_batch_holds_at_most_200_lines` (450 lines → events of ≤200, newest kept), `close_stops_the_stream_and_nothing_is_emitted_after`, `a_second_open_stops_the_first`, `close_for_another_server_keeps_the_stream`, `server_log_event_serializes` (`{"type":"server_log","tenantId":"t","stack":"s","lines":["a"]}`), `a_stream_error_after_connect_emits_ended`.
- [ ] **Step 2: Run** — FAIL; **Step 3: implement**; **Step 4: run** — PASS.
- [ ] **Step 5: Commit** `feat(app): live console lines for Manage`.

### Task 8: Your servers merges managed servers; GitHub sign-in

**Files:**
- Modify: `crates/launcher-core/src/perland/api.rs` (`YourServer.manage`, `SignInMethod::GitHub`), `crates/launcher-core/src/perland/{pkce.rs or panel.rs}` (`launcher_sign_in_url`), `crates/launcher-app/src/{your_servers.rs,perland.rs,dto.rs}`
- Test: `crates/launcher-core/tests/perland.rs` (parse), unit tests in `your_servers.rs` and `perland.rs`

**Interfaces:**
- Consumes: `managed_servers` (Task 5), `ManageRef`, `launcher_github` (Task 1).
- Produces:
  - `YourServer.manage: Option<ManageRefCore { tenant_id, stack }>` (`#[serde(default)]`).
  - `YourServerCard.manage: Option<ManageRef>`, `FreeServerCard.manage: Option<ManageRef>`, `YourServersPage.managed: Vec<ManagedServer>` (managed servers matched to no card).
  - Matching in `your_servers` (managed servers read alongside; any error → no managed servers, the page stands as today):
    1. The free server: the managed server whose `stack == free.name` in a `free` team (else in any team) sets `FreeServerCard.manage` (the free page reads it: `free_server_page` must call the same matcher; expose `pub(crate) async fn free_manage_ref(&self) -> Option<ManageRef>` and use it in both).
    2. An owner entry with `manage` that matches a managed server by `{tenantId, stack}` keeps its card and gets `manage`; the managed server leaves `managed`.
    3. An owner entry without `manage` (today's Engine): matched by equal address (`ServerAddress` display, case-insensitive) to a managed server; then as 2.
    4. Every other managed server goes in `managed`.
  - `SignInMethod::GitHub` (`"github"`), offered in `PerlandStatus.methods` only when `config.panel` is set AND (`/player/config` lists `github` OR `launcher_github(engine)`).
  - `pub fn launcher_sign_in_url(panel: &Url, redirect: &str, pkce: &Pkce) -> Url`: `<panel>/launcher/sign-in?redirect_uri=&code_challenge=&code_challenge_method=S256&state=`.
  - `perland_sign_in_github(language) -> Result<Option<PerlandAccount>>`: the Discord flow (`perland_sign_in_discord`) generalized into a private `sign_in_in_browser(method, language)`; the URL differs, the loopback, PKCE, cancel, reopen and `POST /player/token` redemption are the same. Refused with `SignInUnavailable` (param `method: github`) when not offered.

- [ ] **Step 1: Tests**: core `your_server_reads_manage` and `your_server_without_manage_is_none`, `methods_keep_github`; app `free_server_card_gets_manage`, `owner_card_with_manage_is_deduped`, `owner_card_without_manage_matches_by_address`, `unmatched_managed_servers_are_listed`, `managed_failure_keeps_your_servers` (FakeStack `player-session` errors → page as today, `managed` empty), `github_offered_only_with_a_panel_and_the_engine_saying_so` (4 cases), `github_sign_in_opens_the_panel_page_and_redeems_at_player_token`, `launcher_sign_in_url_has_the_four_params`.
- [ ] **Step 2: Run** — FAIL; **Step 3: implement**; **Step 4: run** `cargo test -p launcher-core -p launcher-app` — PASS.
- [ ] **Step 5: Commit** `feat(app): managed servers in Your servers, and Sign in with GitHub`.

### Task 9: Wiring: Tauri commands, contract, UI schema and IPC

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs` (register), `crates/launcher-app/tests/contract.rs`, `ui/src/ipc/contract/*.json` (generated), `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/contract.test.ts`
- Create: `ui/src/ipc/mockManage.ts` (+ `mockManage.test.ts`), modify `ui/src/ipc/mock.ts` to route the new commands

**Interfaces:**
- Commands (snake_case Tauri names, args camelCase on the TS side): `managed_servers`, `manage_detail {tenantId, stack}`, `manage_action {tenantId, stack, action}`, `manage_diagnosis`, `manage_players`, `manage_players_change {tenantId, stack, change}`, `manage_settings`, `manage_settings_save {tenantId, stack, save}`, `manage_console {tenantId, stack, command, confirm}`, `manage_console_open`, `manage_console_close`, `manage_open_panel {tenantId, stack: string | null}`, `perland_sign_in_github {language}`. Each through `logged("<name>", …)` like its neighbours.
- Contract samples: `managed_servers.json`, `manage_detail.json` (one per state word), `manage_diagnosis.json`, `manage_players.json`, `manage_settings.json`, `manage_saved.json`, `manage_console_reply.json`, and the new `server_log` event in `events.json`; updated `your_servers_page.json` and `free_server_page.json` (with `manage`), `perland_status.json` (a `github` method).
- zod schemas in `schema.ts` named as the DTOs (`ManagedServer`, `ManageDetail`, …), `state` as `z.enum([...])` with a `.catch('unknown')`.
- `mockManage.ts`: `?manage=` scenarios `running` (default: 3 playing), `asleep`, `stopped`, `crashed` (diagnosis `out_of_memory`), `starting` (becomes running after 3 s), `locked` (save answers `plan_feature_locked` field `maxPlayers`), `modpack` (version locked), `motdCodes` (MOTD with `§`), `none` (no managed servers). The mock free server (mockFree) carries `manage` unless `none`; one extra team server "Team survival". The console mock echoes commands, asks confirmation for `stop`, and emits a `server_log` line every 2 s while open.

- [ ] **Step 1:** Register the commands; run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`; then `cargo test -p launcher-app --test contract` — PASS.
- [ ] **Step 2:** Add schemas and commands; run `pnpm --filter ui test -- contract` — PASS (every contract file parses).
- [ ] **Step 3:** Write `mockManage.test.ts` (each scenario's detail state; confirmation for `stop`; `locked` save error code), implement, PASS.
- [ ] **Step 4:** `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm -r typecheck`, `pnpm -r lint`.
- [ ] **Step 5: Commit** `feat: wire Manage through Tauri, the contract and the mock backend`.

### Task 10: Manage screen UI (through `/impeccable`)

**Files:**
- Create: `ui/src/features/manage/{ManageScreen,ManageOverview,ManagePlayers,ManageConsole,ManageSettings}.tsx`, `causeLine.ts`, tests beside each, `ui/src/state/manageLog.ts`
- Modify: `ui/src/state/route.ts` (`{ name: 'manage'; tenantId: string; stack: string }`), `ui/src/App.tsx` (render the route), `ui/src/state/queries.ts` (hooks), `ui/src/state/handleEvent.ts` (`server_log` → `manageLog`), `ui/src/i18n/{en,es}.json` (`manage.*`)

**Interfaces:**
- Consumes: Task 9 commands and schemas.
- Produces: hooks `useManageDetail(tenantId, stack)` (`refetchInterval: (q) => ['starting','stopping'].includes(q.state.data?.state) ? 5000 : 30000`, only mounted while the screen is open), `useManageDiagnosis` (enabled while `state === 'stopped'`, `staleTime: Infinity`, invalidated after an action), `useManagePlayers`, `useManageSettings`, `useManageAction`, `useManagePlayersChange`, `useManageSettingsSave`, `useManageConsole`, `openManagePanel(tenantId, stack|null)`. `manageLog`: an external store of the last 1000 lines per `tenantId/stack`, `useManageLog(tenantId, stack)`; the console opens the stream on mount and closes it on unmount.
- Screen (§2 of the spec; layout decided in `/impeccable`, one column; Overview, Players, Console, Settings as tabs on a narrow window, stacked on a wide one, or as the skill decides):
  - **Overview:** name and `InstanceIcon`/monogram art; state line `manage.state.running` "Running · {{count}} playing", `asleep` "Asleep · wakes when someone joins", `stopped` "Stopped", `starting` "Starting…", `stopping` "Stopping…", `crashed` "Crashed", `unknown` "Can't tell right now"; one big button: Start (stopped, asleep, crashed), Stop and Restart (running; Restart secondary), none while starting/stopping; Stop and Restart open a confirmation ("Stop {{name}}? Everyone playing is sent out."); address with `CopyRow`; crashed: `causeLine(t, cause)` and **Start again**; `pendingRestart`: "Restart to apply your saved settings."; **More in Perland** (`openManagePanel(tenantId, stack)`).
  - `causeLine`: `manage.cause.<code>` for `out_of_memory`, `mod_missing_dependency`, `mod_incompatible`, `client_only_mod`, `mixin_apply_failed`, `plugin_load_failed`, `plugin_missing_dependency`, `java_version_mismatch`, `version_not_available`, `source_download_failed`, `config_load_failed`, `port_bind_failed`, `installer_failed`, interpolating `subject`; else `manage.cause.unknown` ("The server stopped because of an error{{subjectPart}}.").
  - **Players:** Online (each with Kick, confirm-free), Whitelist (add box + Remove), Admins (add box "Make admin", Remove admin), Bans (add box with Ban (confirm), Unban); per-name result words from `status` (`manage.result.<status>`: `added`, `already_listed`, `not_found`, `failed` + message, `skipped`, …); a link "Whitelist is off — turn it on in Settings" / "Whitelist is on" that switches to Settings; stopped server: lists still editable except Online and Bans (bans need the server running: show "Start the server to change bans").
  - **Console:** the live lines (monospace, `aria-live="polite"` off for bulk, a "Jump to latest" when scrolled up), a command box; `CommandNeedsConfirmation` → confirm dialog "This stops the server. Run it?" → resend with `confirm: true`; the reply output below the command; "Start the server to use the console." while not running.
  - **Settings:** Server-list name (plain text; read only with "Edit in Perland" when `motdReadOnly`), Game mode (Survival, Creative, Adventure, Spectator), Difficulty (Peaceful, Easy, Normal, Hard), Max players (number), PvP (Toggle), Whitelist (Toggle), Minecraft version (hidden when `versionLocked`); Save with "Restart now to apply" / "Apply at next restart" while running (just Save while stopped); `VersionChangeNeedsConfirmation` → dialog then resend with `confirmVersionChange`; `DataLossNeedsConfirmation` → a stronger dialog then resend with both; `SettingsChanged` → "Someone changed these settings. We loaded the new ones." and refetch; `PlanFeatureLocked` with param `field` → that field shows locked with **Upgrade** (`openManagePanel(tenantId, null)`).
  - Refusals show through `ErrorCard` (translated by code).
- [ ] **Step 1: Invoke `/impeccable`** for the screen with the direction and `DESIGN.md`; settle layout before code.
- [ ] **Step 2: Failing Vitest tests** (Testing Library, `test/backend.tsx` mock): `ManageOverview.test.tsx`: running shows "Running · 3 playing" and Stop asks to confirm; asleep shows "Asleep · wakes when someone joins" and Start; stopped shows Start; crashed shows the cause line and Start again; pending restart line. `ManagePlayers.test.tsx`: add to whitelist calls `manage_players_change` with `{list:'whitelist', add:true, names:['Alex']}`; Ban confirms; result words. `ManageConsole.test.tsx`: `stop` asks then resends with confirm; `server_log` lines appear; stream closed on unmount. `ManageSettings.test.tsx`: locked field shows Upgrade; read-only MOTD; version hidden for a modpack; version change confirmation; apply choice passes `apply: 'next_restart'`. `causeLine.test.ts`: known and unknown codes. `route.test.ts`: the manage route.
- [ ] **Step 3: Implement**, strings in both language files.
- [ ] **Step 4: Run** `pnpm --filter ui test`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm --filter ui build` — PASS.
- [ ] **Step 5: Commit** `feat(ui): the Manage screen`.

### Task 11: Your servers cards and the GitHub button (through `/impeccable`)

**Files:**
- Create: `ui/src/features/servers-list/ManagedServerCard.tsx` (+ test)
- Modify: `ui/src/features/servers-list/YourServers.tsx`, `ui/src/features/free-server/FreeServerCard.tsx`, `ui/src/features/servers-list/PublicServerCard.tsx` (a primary-action slot if it has none), `ui/src/features/accounts/PerlandAccountCard.tsx`, `ui/src/i18n/{en,es}.json`

**Interfaces:**
- Consumes: `YourServersPage.managed`, `manage` on cards, `perland_sign_in_github`.
- Behaviour:
  - Your servers order: the free server card, owner cards with `manage`, `managed` cards, then the rest as today. A card with `manage` has **Manage** as its primary action (navigates to `{name:'manage', tenantId, stack}`) and Play beside it; its menu keeps Delete (free server) and drops "Manage in Perland" (the Manage screen has More in Perland). An owner card without `manage` keeps today's "Manage in Perland" menu item.
  - `ManagedServerCard`: name, the state word, the address with Copy, Manage. No Play (no invite known).
  - Sign-in card: **Sign in with GitHub** beside Discord and email when `methods` holds `github`; the same waiting/reopen/cancel states as Discord.
- [ ] **Step 1: `/impeccable`** for the card and the button.
- [ ] **Step 2: Failing tests**: `YourServers.test.tsx` (`a managed owner card shows Manage first and opens the screen`, `an owner card without manage keeps Manage in Perland`, `managed-only servers show their own card`), `PerlandAccountCard.test.tsx` (`github shows only when offered`, `github sign-in calls perland_sign_in_github`).
- [ ] **Step 3: Implement**; **Step 4:** full UI checks as Task 10.
- [ ] **Step 5: Commit** `feat(ui): Manage on Your servers, and Sign in with GitHub`.

### Task 12: Whole-branch verification and docs

- [ ] Run the full pre-commit list from CLAUDE.md: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
- [ ] Grep the diff for token leaks: `tracing::` lines near `jwt`, `token`, `code`, `handoff` print codes only.
- [ ] README: a line under the Perland section on Manage and `PERLAND_PANEL_URL` enabling More in Perland and GitHub sign-in.
- [ ] Commit `docs: Manage your server`.
