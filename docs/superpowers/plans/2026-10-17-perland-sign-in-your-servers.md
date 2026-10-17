# Sign in with Perland and Your Servers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A player signs in to a Perland player account (Discord through the browser, or an email code), and the Servers screen shows *Your servers* (Owner, Invited, Followed) above the public list. Play follows a public server, Unfollow is in its menu, and joining through an invite link while signed in records *Invited*.

**Architecture:**
- **launcher-core** gains a `perland` module:
  - a `PlayerClient` for the Engine's `/api/v1/player` routes;
  - PKCE and a one-shot loopback HTTP listener;
  - a `SessionStore` that keeps the token in the OS credential store behind a non-secret marker file.
- **launcher-app** exposes commands over a `PlayerApi` trait the `Runner` hands out, so tests use a fake.
- **The UI** gets the Accounts card and the Your servers slot, built from the existing public card.

**Tech Stack:** Rust (tokio, reqwest, serde, the `keyring` crate, `getrandom`, `base64`, `sha2`, `percent-encoding`), wiremock, Tauri 2, React + TypeScript + zod + React Query, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-17-perland-sign-in-your-servers-design.md`. Read it with this plan; the "Decisions" table is binding.

## Global Constraints

- `crates/launcher-core` and `crates/launcher-app` may not depend on Tauri or UI crates; `src-tauri` is wiring only and has no tests.
- TDD: write the failing test first. Pure logic gets `#[cfg(test)]` unit tests, HTTP behaviour gets `tests/*.rs` with wiremock, and the UI gets Vitest + Testing Library.
- Never run fuzz, stress or load tests locally. Keep test inputs tiny.
- Errors shown to players are `LauncherError` with a stable `code`. A new `ErrorCode` needs `errors.<code>.message` in both `ui/src/i18n/en.json` and `es.json`.
- Every UI string goes in both `en.json` and `es.json` (neutral Latin American Spanish, informal "tú").
- `ui/src/ipc/contract/*.json` is generated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- UI styling goes through the `/impeccable` skill, `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- Never log a `plt_` token, an email code, the PKCE verifier or the authorization code.
- Every type holding one of these gets a manual `Debug` printing `<redacted>`. Errors name the Engine's host only.
- No real tokens in code or tests. Test tokens are obviously fake, e.g. `plt_` followed by 43 `x` characters.
- Player calls go only to the session's Engine origin. The token is sent only as `Authorization: Bearer`, never in a URL.
- An invite token goes only in JSON bodies.
- Engine contract values:
  - token answer: `{token, player, newPlayer}`;
  - `PlayerDTO`: `{id, displayName, email?, discord?{id, username, globalName, avatarUrl}, minecraft?, createdAt}`;
  - `/config`: `{enabled, methods}`;
  - email start: 202 `{expiresIn, resendIn}`;
  - Your servers entry: `ServerDTO` plus `relation`, `following` and `news?`;
  - codes: `player_accounts_disabled`, `sign_in_method_unavailable` (`method`), `invalid_email`, `too_many_codes` (`retryAfter`), `invalid_code` (`attemptsLeft`), `email_unavailable`, `player_servers_unavailable` and `too_many_player_servers`.
- Before each commit that touches the UI, run `pnpm --filter ui build`. Before the PR, run the full CLAUDE.md "Before committing" list.

## Review Focus

1. **Loopback hit by something else first** (a browser prefetch, a favicon request, a stale tab with an old `state`): the wait must continue and the real callback must still sign the player in. → Task 2, test `loopback_ignores_strays_then_accepts`.
2. **Credential store missing or locked on Linux:** sign-in must still work for the session, and a player who never signs in must never touch the store. → Task 3, tests `no_marker_never_reads_the_secret` and `no_backend_keeps_memory_only`.
3. **Token dies (90 days unused, or signed out elsewhere):** the first 401 must quietly show signed out, with no error toast. → Task 4 test `status_after_401_forgets_the_session`, Task 5 test `your_servers_after_401_is_none_and_forgets`.
4. **Invite from another Engine (a friend's self-hosted install):** follow and joined must never send our token there. → Task 5, test `follow_skips_an_invite_of_another_engine`.
5. **Older or switched-off Engine** (`/config` 404 or `enabled: false`, servers 503): nothing shows, and there are no error cards. → Task 1 test `config_404_is_off`, Task 6 mock scenario `off`, Task 8 test `nothing_when_accounts_off`.

---

### Task 1: Player API client and error codes (launcher-core)

**Files:**
- Create: `crates/launcher-core/src/perland/mod.rs`, `crates/launcher-core/src/perland/api.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod perland;`), `crates/launcher-core/src/error.rs` (new codes)
- Test: `crates/launcher-core/tests/perland.rs` (wiremock), unit tests in `api.rs`

**Interfaces:**
- Produces, in `launcher_core::perland`:
  - `PlayerClient::new(http: reqwest::Client, engine: Url) -> Self` and `fn engine(&self) -> &Url`.
  - Calls on `PlayerClient`, all `async`, all returning `Result<_>`:
    - `config() -> PlayerConfig`
    - `token(code: &str, verifier: &str, redirect: &str) -> TokenAnswer`
    - `email_start(email: &str, language: &str) -> EmailStarted`
    - `email_verify(email: &str, code: &str) -> TokenAnswer`
    - `me(token: &str) -> Player`
    - `sign_out(token: &str) -> ()`
    - `servers(token: &str) -> Vec<YourServer>`
    - `follow(token: &str, invite_token: &str) -> ()`
    - `unfollow(token: &str, invite_token: &str) -> ()`
    - `joined(token: &str, invite_token: &str) -> ()`
  - DTOs:
    - `PlayerConfig { enabled: bool, methods: Vec<SignInMethod> }` with `PlayerConfig::off()`;
    - `enum SignInMethod { Discord, Email }`; unknown strings are dropped while deserializing the list;
    - `Player { id, display_name: String, email: Option<String>, discord: Option<DiscordAccount>, created_at: String }`;
    - `DiscordAccount { id, username, global_name: Option<String> }`;
    - `TokenAnswer { token: String, player: Player, new_player: bool }`, with a manual Debug that hides `token`;
    - `EmailStarted { expires_in: u64, resend_in: u64 }`;
    - `YourServer { #[serde(flatten)] server: PublicServer, relation: Relation, following: bool }`;
    - `enum Relation { Owner, Invited, Followed }`; unknown values read as `Followed`.
  - New `ErrorCode` variants under `// Perland account`: `PlayerAccountsClosed`, `SignInUnavailable`, `SignInFailed`, `SignInTimedOut`, `InvalidEmail`, `InvalidCode`, `TooManyCodes`, `EmailUnavailable`, `PerlandSignedOut`, `PlayerServersClosed`, `TooManyPlayerServers`. Add each to the code-name test table in `error.rs`.

- [ ] **Step 1: Write failing unit tests in `api.rs`**
  - `relation_unknown_is_followed`: deserializing `"friend"` gives `Followed`.
  - `methods_drop_unknown`: `["discord","passkey","email"]` gives `[Discord, Email]`.
  - `token_answer_debug_hides_token`: `format!("{:?}")` does not contain `plt_`.
- [ ] **Step 2: Write failing wiremock tests in `tests/perland.rs`.** One `MockServer` as the Engine; envelope helper `{"success":true,"data":…}`; errors are `{"success":false,"error":{…},"data":{"code":…,…}}`. Copy the failure shape from `tests/public_list.rs` and from the Engine's `internal/api/response/envelope.go`.
  - `config_reads_methods`.
  - `config_404_is_off` and `config_503_is_off`: both give `PlayerConfig::off()`.
  - `token_posts_code_verifier_redirect`: body matcher on all three fields; answers `TokenAnswer`.
  - `invalid_code_carries_attempts_left`: 400 `invalid_code`, `attemptsLeft: 2` gives code `InvalidCode` and param `attemptsLeft` = `"2"`.
  - `too_many_codes_carries_retry_after`.
  - `email_unavailable_maps`, `invalid_email_maps`.
  - `sign_in_method_unavailable_maps_with_method`.
  - `player_accounts_disabled_is_closed`.
  - `me_sends_bearer_and_parses`: header matcher `authorization: Bearer plt_xxx…`.
  - `me_401_is_signed_out`: code `PerlandSignedOut`.
  - `servers_parses_relations`: two entries, `owner` and `followed`.
  - `servers_503_unavailable_is_closed`: code `PlayerServersClosed`.
  - `follow_posts_token_in_body`: path exactly `/api/v1/player/servers/follow`.
  - `follow_404_is_ok`, `joined_404_is_ok`.
  - `follow_409_too_many`.
  - `sign_out_posts`.
  - `errors_name_host_only`: the error message and detail contain the mock host but not `/api/v1`.
- [ ] **Step 3: Run `cargo test -p launcher-core --test perland` and the unit tests.** Expected: FAIL (module missing).
- [ ] **Step 4: Implement the client and the codes.**
  - Reuse `http::read_capped`; caps are 64 KiB, except `servers()` at 1 MiB, and 4 KiB for error bodies.
  - Reuse the request timeout of `InviteTiming`'s default, 20 s.
  - Reuse the public list's error constructors (`request_failed`, `host_trouble`, `rate_limited`, `unexpected`), making them `pub(crate)` where needed.
  - Fallback messages are plain English.
- [ ] **Step 5: Run the tests again.** Expected: PASS. Also run `cargo test -p launcher-core error`.
- [ ] **Step 6: Commit** `feat(core): Perland player API client (#48)`.

### Task 2: PKCE and the loopback listener (launcher-core)

**Files:**
- Create: `crates/launcher-core/src/perland/pkce.rs`, `crates/launcher-core/src/perland/loopback.rs`
- Modify: `crates/launcher-core/Cargo.toml` and workspace `Cargo.toml` (add `getrandom` 0.3 and `base64` 0.22, versions already in `Cargo.lock`)
- Test: unit tests in both files; `crates/launcher-core/tests/perland_loopback.rs`

**Interfaces:**
- Produces:
  - `Pkce { verifier, challenge, state }`:
    - `Pkce::new() -> Pkce`;
    - `Pkce::from_verifier(verifier: &str, state: &str) -> Pkce` (for tests);
    - a manual Debug.
  - `fn authorize_url(engine: &Url, redirect: &str, pkce: &Pkce) -> Url`, which gives `{engine}/api/v1/player/discord/authorize?redirect_uri=…&code_challenge=…&code_challenge_method=S256&state=…`.
  - Loopback types:
    - `Loopback::bind() -> std::io::Result<Loopback>`, `fn redirect_uri(&self) -> String` giving `http://127.0.0.1:{port}/callback`;
    - `async fn wait(self, state: &str, language: Language, deadline: Duration, cancel: CancellationToken-like) -> Result<Callback>`.
    - Cancel: use `tokio::sync::watch::Receiver<bool>`, or whatever cancel type `launcher_core` already has. Check for `Cancel` in launcher-app and pick a core-side equivalent.
    - `enum Callback { Code(String), Denied, Failed(String), Canceled }`, with a manual Debug hiding the code;
    - `enum Language { En, Es }` with `Language::parse(&str)`.

- [ ] **Step 1: Write failing unit tests.**
  - `challenge_matches_rfc7636`: verifier `dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk` gives challenge `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`.
  - `new_is_43_url_safe_chars`.
  - `authorize_url_encodes`: the redirect is percent-encoded; the URL has the 4 params.
  - `parse_request_line`: `GET /callback?code=a&state=b HTTP/1.1` gives the code and state.
  - Also cover a non-GET request, another path, and a request over 8 KiB.
- [ ] **Step 2: Write failing integration tests** with a real bind and a reqwest client as the browser:
  - `loopback_ignores_strays_then_accepts`: first `/favicon.ico` (404), then `/callback?state=wrong&code=x` (page "old link", wait continues), then the right state, which gives `Code`;
  - `loopback_denied`: `error=access_denied` gives `Denied`;
  - `loopback_failed`: `error=server_error` gives `Failed`;
  - `loopback_deadline`: 50 ms gives code `SignInTimedOut`;
  - `loopback_cancel`;
  - `success_page_is_spanish`: with `Language::Es` the page contains `Ya iniciaste sesión`.
- [ ] **Step 3: Run the tests.** Expected: FAIL.
- [ ] **Step 4: Implement.**
  - The port comes from binding `127.0.0.1:0`; rebind if it is below 1024.
  - Each connection gets a 5 s read timeout and an 8 KiB cap.
  - Answer with `Content-Type: text/html; charset=utf-8`, `Cache-Control: no-store` and `Connection: close`.
  - Pages are inline HTML with no scripts and no external assets. Use this copy:

    | Page | en | es |
    |---|---|---|
    | success | "You're signed in to Perland. You can close this tab and go back to the launcher." | "Ya iniciaste sesión en Perland. Puedes cerrar esta pestaña y volver al launcher." |
    | denied | "Sign-in was canceled. You can close this tab." | "Cancelaste el inicio de sesión. Puedes cerrar esta pestaña." |
    | failed | "Something went wrong. Go back to the launcher and try again." | "Algo salió mal. Vuelve al launcher e inténtalo de nuevo." |
    | old link | "This sign-in link is old. Go back to the launcher and try again." | "Este enlace para iniciar sesión ya no sirve. Vuelve al launcher e inténtalo de nuevo." |
- [ ] **Step 5: Run the tests again.** Expected: PASS.
- [ ] **Step 6: Commit** `feat(core): PKCE and the loopback sign-in listener (#48)`.

### Task 3: Session storage (launcher-core)

**Files:**
- Create: `crates/launcher-core/src/perland/session.rs`
- Modify: `crates/launcher-core/Cargo.toml`. Add `keyring` (3.x) with `windows-native` and `sync-secret-service` + `crypto-rust` on Linux, through target-specific dependency tables. Leave it off other targets, where `KeyringStore` is replaced by the memory store at compile time.
- Test: unit tests in `session.rs` with `tempfile`

**Interfaces:**
- Produces:
  - `Session { engine: Url, token: String, profile: Profile }` with a manual Debug.
  - `Profile { name: String, email: Option<String>, discord: bool }`, built by `Profile::from_player(&Player)`. `name` is `display_name`, else the email, else `""`.
  - `trait SecretStore: Send + Sync`:
    - `fn load(&self) -> std::result::Result<Option<String>, SecretError>`;
    - `fn save(&self, secret: &str) -> Result<(), SecretError>`;
    - `fn delete(&self) -> Result<(), SecretError>`;
    - `enum SecretError { NoBackend, Other(String) }`.
  - `MemoryStore::default()` and `KeyringStore::new()` (service `online.perland.launcher`, user `perland-account`).
  - `SessionStore::new(marker: PathBuf, secret: Arc<dyn SecretStore>)`, with `load() -> Option<Session>`, `save(&Session) -> Result<()>` and `forget()`.
  - Marker file: `<root>/perland-account.json`, holding `{engine, name, email, discord}` and never the token. It is written with `fsutil::write_atomic`.

- [ ] **Step 1: Write failing tests.**
  - `no_marker_never_reads_the_secret`: a spy store counts calls; `load()` without a marker gives `None` and 0 calls.
  - `save_then_load_round_trips`.
  - `marker_without_secret_is_cleared`: a marker, an empty store, then `load()` gives `None` and the marker is gone.
  - `no_backend_keeps_memory_only`: `save` with a store answering `NoBackend` writes no marker, but `load()` on the same `SessionStore` instance gives the session from memory.
  - `forget_deletes_both`.
  - `marker_never_holds_the_token`: read the file and assert no `plt_`.
  - `session_debug_hides_token`.
- [ ] **Step 2: Run** `cargo test -p launcher-core perland::session`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Keyring calls run in `tokio::task::spawn_blocking` when called from async: give `SessionStore` async wrappers `load_async`, `save_async` and `forget_async`, or make the app call it inside `spawn_blocking`. The builder picks one and states it in the Interfaces of Task 4.
  - Map `keyring::Error::NoStorageAccess` and `PlatformFailure` to `NoBackend`, and `NoEntry` to `Ok(None)`.
- [ ] **Step 4: Run** the tests; also `cargo build -p launcher-core` and `cargo clippy -p launcher-core --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): Perland session storage in the credential store (#48)`.

### Task 4: Sign-in commands (launcher-app + src-tauri)

**Files:**
- Create: `crates/launcher-app/src/perland.rs`
- Modify:
  - `crates/launcher-app/src/lib.rs`;
  - `runner.rs`: add `fn player(&self) -> Arc<dyn PlayerApi>`, `CoreRunner` builds it from `syncer.sources.invites` and the site;
  - `testing.rs`: add `FakePlayer`, and `RecordingHost` records `open_url`;
  - `events.rs`: add `Host::open_url(&self, url: &str)`, and `AppEvent::PerlandSignIn { phase }`;
  - `app.rs`: AppCore fields `perland: tokio::sync::Mutex<PerlandState>` and the `SessionStore` built from `config` root plus `KeyringStore`; tests inject `MemoryStore`;
  - `dto.rs`;
  - `tests/contract.rs`;
  - `src-tauri/src/commands.rs` and `src-tauri/src/main.rs`: register the commands; the Tauri host implements `open_url` with `tauri-plugin-opener` (`open_url(url, None)`).
- Test: `#[cfg(test)]` in `perland.rs`; contract test.

**Interfaces:**
- Consumes: Task 1 `PlayerClient` and the DTOs, Task 2 `Pkce`, `authorize_url`, `Loopback` and `Callback`, Task 3 `SessionStore`.
- Produces:
  - `trait PlayerApi: Send + Sync`. Each method returns a `BoxFuture` over the matching `PlayerClient` call, taking `engine: Url` first:
    - `config`
    - `token`
    - `email_start`
    - `email_verify`
    - `me`
    - `sign_out`
    - `servers`
    - `follow`
    - `unfollow`
    - `joined`
  - Also on `PlayerApi`: `fn site_engine(&self) -> BoxFuture<Result<Url>>` (`engine_for_site` on the public list's site).
  - DTOs, camelCase:
    - `PerlandStatus { enabled: bool, methods: Vec<String>, account: Option<PerlandAccount> }`;
    - `PerlandAccount { name: String, email: Option<String>, discord: bool, new_player: bool }`;
    - `EmailCodeSent { resend_in: u64 }`;
    - `PerlandSignInPhase { Waiting, Exchanging }` (serde lowercase).
  - AppCore methods:
    - `perland_status() -> Result<PerlandStatus>`
    - `perland_sign_in_discord(language: &str) -> Result<Option<PerlandAccount>>`
    - `perland_sign_in_reopen() -> Result<()>`
    - `perland_sign_in_cancel()`
    - `perland_email_start(email: &str, language: &str) -> Result<EmailCodeSent>`
    - `perland_email_verify(email: &str, code: &str) -> Result<PerlandAccount>`
    - `perland_sign_out() -> Result<()>`
    - `pub(crate) async fn perland_session(&self) -> Option<Session>`, for Task 5: it loads lazily once and returns a clone.
  - Commands, same names in snake_case: `perland_status`, `perland_sign_in_discord {language}`, `perland_sign_in_reopen`, `perland_sign_in_cancel`, `perland_email_start {email, language}`, `perland_email_verify {email, code}`, `perland_sign_out`.

- [ ] **Step 1: Write failing tests** with `FakePlayer`, `MemoryStore` and `RecordingHost`.
  - `status_off_when_config_disabled`.
  - `status_signed_out_lists_methods`.
  - `email_verify_saves_session_and_returns_account`.
  - `status_after_401_forgets_the_session`: `me` answers `PerlandSignedOut`, so the status account is `None` and the store is empty.
  - `status_keeps_session_when_engine_unreachable`: `me` answers `request_failed`, so the account is still shown from the profile.
  - `status_reads_store_lazily_once`.
  - `discord_sign_in_opens_the_authorize_url_and_exchanges`:
    - the fake host records the URL; the test parses `redirect_uri` and `state` out of it;
    - a reqwest GET to `redirect_uri?code=c&state=s` completes the flow;
    - `FakePlayer::token` saw `code` `c` and the verifier whose S256 is the URL's challenge;
    - the session is saved.
  - `discord_sign_in_cancel_is_none`.
  - `starting_a_second_sign_in_cancels_the_first`.
  - `discord_denied_is_none`: `error=access_denied` gives `Ok(None)`.
  - `discord_failed_is_sign_in_failed`.
  - `sign_out_forgets_even_when_engine_fails`.
  - `perland_calls_use_the_session_engine`: the session holds engine A, `site_engine` now answers B, and `me` is called with A.
- [ ] **Step 2: Add contract fixtures** `perland_status` (signed in and signed out samples) and `email_code_sent`, plus the event in the events fixture.
- [ ] **Step 3: Run** `cargo test -p launcher-app perland`. Expected: FAIL.
- [ ] **Step 4: Implement** the AppCore methods and the `CoreRunner` `PlayerApi`, then wire src-tauri.
  - Each command wrapper uses `logged("name", …)`.
  - The deadline is 10 minutes.
  - Emit `PerlandSignIn{Waiting}` after opening the browser and `{Exchanging}` before `/token`.
  - The authorize URL is kept in state for `reopen`.
- [ ] **Step 5: Run** `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then `cargo test -p launcher-app`, then `cargo build -p perland-launcher` (src-tauri; check its package name in `src-tauri/Cargo.toml`). Expected: PASS and builds.
- [ ] **Step 6: Commit** `feat(app): Sign in with Perland commands (#48)`.

### Task 5: Your servers commands (launcher-app + src-tauri)

**Files:**
- Create: `crates/launcher-app/src/your_servers.rs`
- Modify: `crates/launcher-app/src/public_servers.rs` (make `card(…)` reusable with a `keep_offline: bool`), `dto.rs`, `lib.rs`, `tests/contract.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`
- Test: `#[cfg(test)]` in `your_servers.rs`

**Interfaces:**
- Consumes: Task 4 `perland_session()`, `PlayerApi`; Task 1 `YourServer`, `Relation`; `servers::server_source`, `InviteLink::token()`, `InviteLink::parse`.
- Produces:
  - `YourServersPage { servers: Vec<YourServerCard> }`;
  - `YourServerCard { #[serde(flatten)] card: PublicServerCard, relation: String /* "owner"|"invited"|"followed" */, following: bool }`;
  - AppCore methods:
    - `your_servers() -> Result<Option<YourServersPage>>`;
    - `your_servers_follow(invite: &str)` and `your_servers_joined(invite: &str)`, which never fail: they return `()` and log quietly;
    - `your_servers_unfollow(invite: &str) -> Result<()>`.
  - Commands of the same names: `your_servers`, `your_servers_follow {invite}`, `your_servers_unfollow {invite}`, `your_servers_joined {invite}`.
  - The UI calls follow, unfollow and joined with the canonical invite URL it holds (`card.invite`, the Join screen's invite).

- [ ] **Step 1: Write failing tests.**
  - `none_when_signed_out`.
  - `none_when_servers_closed`.
  - `your_servers_after_401_is_none_and_forgets`.
  - `cards_keep_relation_following_and_instance`: an instance already joined names its id, as in `public_servers.rs` tests.
  - `offline_stays_offline`.
  - `unknown_status_is_sleeping`.
  - `follow_sends_invite_token`.
  - `follow_does_nothing_signed_out`.
  - `follow_skips_an_invite_of_another_engine`: an invite whose Engine (`engine_for_site` on the invite's origin, or `server_source` for an instance) differs from the session's; the fake records no call. **Rule:** use the token only when the invite's site resolves to the session's Engine origin.
  - `follow_error_is_swallowed`.
  - `unfollow_reports_errors`.
  - `joined_sends_invite_token`.
- [ ] **Step 2: Add contract fixtures** `your_servers_page`: one owner, one invited, one followed; one `offline`.
- [ ] **Step 3: Run** `cargo test -p launcher-app your_servers`. Expected: FAIL.
- [ ] **Step 4: Implement**, and register the commands in src-tauri.
- [ ] **Step 5: Run** the contract update, `cargo test -p launcher-app`, and the src-tauri build. Expected: PASS.
- [ ] **Step 6: Commit** `feat(app): Your servers commands (#48)`.

### Task 6: UI data layer and mocks

**Files:**
- Modify: `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/state/queries.ts` (qk keys and hooks), `ui/src/ipc/mock.ts`, `ui/src/main.tsx` (the `?perland=` param), `ui/src/ipc/contract.test.ts`
- Create: `ui/src/ipc/mockPerland.ts`
- Test: `ui/src/ipc/contract.test.ts`, `ui/src/ipc/mockPerland.test.ts`

**Interfaces:**
- Consumes: the Task 4 and 5 commands and contract JSON.
- Produces:
  - zod: `perlandStatus`, `perlandAccount`, `emailCodeSent`, `yourServerCard` (`publicServerCard.extend({relation, following})`) and `yourServersPage`;
  - `commands.perlandStatus()`, `.perlandSignInDiscord(language)`, `.perlandSignInReopen()`, `.perlandSignInCancel()`, `.perlandEmailStart(email, language)`, `.perlandEmailVerify(email, code)`, `.perlandSignOut()`, `.yourServers()`, `.yourServersFollow(invite)`, `.yourServersUnfollow(invite)` and `.yourServersJoined(invite)`;
  - hooks: `usePerlandStatus()` (`qk.perland`) and `useYourServers(enabled: boolean)` (`qk.yourServers`, `refetchInterval: 60_000`);
  - `MockOptions.perland?: 'off'|'signedOut'|'signedIn'|'empty'|'emailOnly'`, defaulting to `signedOut`.
- Mock behaviour:
  - `perland_sign_in_discord` resolves after 1500 ms;
  - email code `123456` works, and any other code is `invalid_code` with `attemptsLeft` counting 4, 3, … 0;
  - `signedIn` serves one owner, one invited (whitelisted) and two followed servers, one of them also in `MOCK_PUBLIC_SERVERS` with the same token;
  - follow, unfollow and joined mutate the mock's Your servers set.

- [ ] **Step 1: Write failing tests.**
  - The contract test parses `perland_status.json`, `email_code_sent.json` and `your_servers_page.json` with the strict schemas.
  - `mockPerland.test.ts` covers: the `off` scenario gives `enabled: false`; the wrong-code countdown; follow then `your_servers` includes the server; unfollow removes a followed entry.
- [ ] **Step 2: Run** `pnpm --filter ui test -- contract mockPerland`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm --filter ui test`, `pnpm -r typecheck` and `pnpm -r lint`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(ui): Perland account data layer and mocks (#48)`.

### Task 7: Perland account card on Accounts (UI, through /impeccable)

**Files:**
- Create: `ui/src/features/accounts/PerlandAccountCard.tsx`, `ui/src/features/accounts/PerlandAccountCard.test.tsx`
- Modify: `ui/src/features/accounts/Accounts.tsx`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`

**Interfaces:**
- Consumes: Task 6 hooks and commands; the `perland_sign_in` event (listen the way other `AppEvent`s are listened to in the UI).
- Produces: the `<PerlandAccountCard/>` component, placed on Accounts above the Minecraft list.
- Strings: `perland.*` keys, plus `errors.<code>.message` in both languages for every Task 1 code. Suggested English:
  - `player_accounts_closed`: "Perland accounts aren't open yet.";
  - `sign_in_failed`: "Couldn't sign you in. Try again.";
  - `sign_in_timed_out`: "Sign-in took too long. Try again.";
  - `invalid_email`: "Check the email address.";
  - `invalid_code`: "That code isn't right.";
  - `too_many_codes`: "Wait {{retryAfter}} seconds before asking for another code.";
  - `email_unavailable`: "The email couldn't be sent. Try again in a moment.";
  - `perland_signed_out`: "You were signed out of Perland.";
  - `player_servers_closed`: "Your servers aren't available right now.";
  - `too_many_player_servers`: "You have too many servers saved. Unfollow some first.";
  - `sign_in_unavailable`: "That way of signing in isn't available."
- Behaviour (spec §6.3): hidden while `enabled` is false; a signed-out button per method; the waiting state with "Open the page again" and Cancel; email then code with "Send a new code" after `resendIn`; inline errors; signed in shows the initial tile, name, the email or "Discord", and Sign out. A wrong code with `attemptsLeft === 0` reads "Ask for a new code".

- [ ] **Step 1: Run the `/impeccable` skill** for this card against `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`. Decide the layout and states before writing code.
- [ ] **Step 2: Write failing tests** with `renderApp(…, {perland: …})`:
  - `hidden_when_off`;
  - `signed_out_shows_both_methods`;
  - `email_only_shows_email`;
  - `discord_shows_waiting_then_signed_in`;
  - `cancel_returns_to_signed_out`;
  - `email_wrong_then_right_code`;
  - `attempts_exhausted_asks_for_new_code`;
  - `sign_out_returns_to_signed_out`;
  - `spanish_strings_exist`: every `perland.*` key is in es.
- [ ] **Step 3: Run** `pnpm --filter ui test -- PerlandAccountCard`. Expected: FAIL.
- [ ] **Step 4: Implement** the component, add the strings, and pass `i18n.language` (`es` when it starts with `es`, else `en`) to the commands.
- [ ] **Step 5: Run** `pnpm --filter ui test`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm --filter ui build`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(ui): Perland account on Accounts (#48)`.

### Task 8: Your servers on the Servers screen (UI, through /impeccable)

**Files:**
- Modify:
  - `ui/src/features/servers-list/YourServers.tsx`;
  - `ui/src/features/servers-list/PublicServerCard.tsx`: props `tag?: 'owner'|'invited'`, `menu?: {onUnfollow(): void}`, `follow?: boolean` (default true for public cards); `usePlay` calls `yourServersFollow` after `joinAdd` when `follow`;
  - `ui/src/features/servers-list/ServersScreen.tsx` (pass the followed tokens to public cards);
  - `ui/src/features/join/JoinServer.tsx`: call `yourServersJoined(invite)` after each successful outcome (add, existing, relink, link);
  - `ui/src/features/servers/StatusPill.tsx` or `labels.ts`, if `offline` needs a word;
  - `en.json`, `es.json`.
- Test: `ui/src/features/servers-list/YourServers.test.tsx`, additions to `ServersScreen.test.tsx` and `JoinServer` tests

**Interfaces:**
- Consumes: Task 6 `usePerlandStatus`, `useYourServers` and commands.
- Strings, `servers.yours.*`, in both languages:
  - heading "Your servers";
  - tags "Owner" and "Invited";
  - "Unfollow";
  - the menu button's label "More for {{name}}";
  - the nudge "Sign in with Perland to keep your servers here." with the link "Sign in".
- Behaviour:
  - The slot renders `useYourServers(status.account != null)`.
  - Nothing renders while loading, when `null`, or when empty.
  - Signed out with `enabled` true shows the nudge, whose link navigates to `{name:'accounts'}`.
  - `enabled` false renders nothing.
  - Cards reuse `PublicServerCard`, with `tag` from `relation` and `menu` when `relation === 'followed'`.
  - `follow` is true only for followed entries.
  - Public cards get `menu` when their token is a followed Your servers entry.
  - After unfollow, invalidate `qk.yourServers`.

- [ ] **Step 1: Run the `/impeccable` skill** for the slot, the tag and the card menu (reuse the `InstanceMenu` pattern).
- [ ] **Step 2: Write failing tests:**
  - `nothing_when_accounts_off`;
  - `nudge_when_signed_out`, whose link goes to accounts;
  - `nothing_when_signed_in_and_empty`;
  - `shows_tags_owner_and_invited`;
  - `menu_only_on_followed`;
  - `unfollow_removes_the_card`;
  - `public_play_follows`: `sent(calls,'your_servers_follow')` after Play;
  - `owner_play_does_not_follow`;
  - `public_card_has_menu_when_followed`;
  - `join_screen_records_joined_on_add` and `join_screen_records_joined_on_existing`;
  - `offline_card_reads_offline`.
- [ ] **Step 3: Run** the targeted Vitest files. Expected: FAIL.
- [ ] **Step 4: Implement**, and add the strings.
- [ ] **Step 5: Run** `pnpm --filter ui test`, lint, typecheck and `pnpm --filter ui build`. Expected: PASS.
- [ ] **Step 6: Check by hand in `pnpm --filter ui dev:mock`:** `?perland=signedIn`, `?perland=signedOut`, `?perland=off` and `?perland=empty`. Use the Playwright Chromium at `/opt/pw-browsers/chromium` for screenshots of Accounts and Servers, desktop width, for the PR.
- [ ] **Step 7: Commit** `feat(ui): Your servers on the Servers screen (#48)`.

### Task 9: Whole-branch verification

- [ ] Run every CLAUDE.md "Before committing" check, in order:
  - `pnpm --filter ui build`
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm -r lint`
  - `pnpm -r typecheck`
  - `pnpm -r test`

  All must pass. Fix and commit anything they find (`chore: fmt` etc.).
- [ ] `git grep -n "plt_[A-Za-z0-9_-]\{43\}"` must find only obviously fake test tokens.
