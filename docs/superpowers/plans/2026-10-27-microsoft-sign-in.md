# Microsoft Sign-in Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Beta and stable builds start Minecraft only for a signed-in Microsoft account whose ownership was checked online before every launch. Offline accounts remain in dev builds only.

**Architecture:**
- **`launcher-core`** gets an `msa` module: the Microsoft OAuth, Xbox, XSTS and Minecraft HTTP calls behind injectable base URLs, a keyed credential store for refresh tokens, and the pure account rules.
- **`launcher-app`** wraps it in an `MsaApi` trait, with a real and a fake implementation, the way `PlayerApi` wraps Perland. It adds the sign-in commands and the session check that every Play goes through.
- **The UI** gets Microsoft sign-in in First run and Accounts, one shared sign-in prompt, early checks on the flows that do work before Play, and a notice for players coming from an older beta.

**Tech Stack:** Rust (tokio, reqwest, serde, keyring 3.6, wiremock), Tauri 2 wiring, React 19 + zod + i18next + TanStack Query, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-27-microsoft-sign-in-design.md` (read it first; this plan argues from it).

## Global Constraints

- `launcher-core` and `launcher-app` never depend on Tauri. `src-tauri` is wiring only and has no tests.
- TDD: a failing test before each piece of code. Pure logic gets `#[cfg(test)]` tests. HTTP behaviour gets `crates/launcher-core/tests/*.rs` with wiremock. UI gets Vitest.
- Never put a token in a log, `LauncherError` message or detail, a `Debug` output, an `AppEvent`, the accounts file or a crash report. Every token type has a manual `Debug` that prints `<redacted>`.
- Every new `ErrorCode` gets `errors.<code>.message` (and `fix` where the player can act) in `ui/src/i18n/en.json` and `es.json`. Spanish is neutral Latin American, informal "tú".
- Every UI string goes in both `en.json` and `es.json`.
- `ui/src/ipc/contract/*.json` is regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, never edited by hand.
- UI styling goes through the `/impeccable` skill, `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- Before every commit run: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm -r test`. Never run fuzz, stress or load tests locally.
- Commit messages end with the session's attribution lines (see the conversation's system reminder).
- Production endpoints (the spec's values):

  | Endpoint | Address |
  |---|---|
  | Microsoft authorize | `https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize` |
  | Microsoft token | `https://login.microsoftonline.com/consumers/oauth2/v2.0/token` |
  | Xbox user | `https://user.auth.xboxlive.com/user/authenticate` |
  | XSTS | `https://xsts.auth.xboxlive.com/xsts/authorize` |
  | Minecraft | `https://api.minecraftservices.com` (`/authentication/login_with_xbox`, `/entitlements/mcstore`, `/minecraft/profile`) |

- Scope `XboxLive.signin offline_access`, `prompt=select_account`, PKCE S256.

## Review Focus

1. **A player whose Microsoft refresh token was revoked** (password changed, app access removed) presses Play. Expected: the sign-in prompt, never the game, and never an endless refresh loop. Covered in Task 8 (`refresh_refused_asks_to_sign_in_again`).
2. **Two Plays at once on the same account** (two instances, or a double click) while the Minecraft token is stale. Expected: one refresh chain, both launches use its result, and the rotated refresh token is stored once, not overwritten by a stale one. Covered in Task 8 (`concurrent_launches_share_one_refresh`).
3. **A hand-edited or old accounts file whose `active` names an offline account**, in a beta build. Expected: treated as no account. Accounts, Play and `account_ready` all agree; the entry stays in the file. Covered in Task 1 (`visible_drops_offline_and_clears_active`) and Task 8 (`active_offline_in_beta_is_no_account`).
4. **Sign-in cancelled or a second sign-in started while the code is being redeemed.** Expected: `Ok(None)`, nothing stored, and the first attempt's answer is never adopted. Covered in Task 7 (`second_sign_in_cancels_first`).
5. **No internet at Play time with a still-valid cached Minecraft token.** Expected: the game does not start (`offline`). The cached token is never used without the online profile check. Covered in Task 8 (`network_failure_never_launches`).

---

### Task 1: Offline accounts only in dev builds; account rules

**Files:**
- Modify: `crates/launcher-core/src/accounts.rs`
- Modify: `crates/launcher-app/src/app.rs` (`app_info`, `account_add_offline`, `accounts_list`)
- Modify: `crates/perland-cli/src/main.rs` (its `offline_allowed` call)

**Interfaces:**
- Produces:
  - `pub fn offline_allowed(build: BuildKind) -> bool`, true only for `BuildKind::Dev`. The second parameter is removed.
  - `pub fn visible(file: AccountsFile, build: BuildKind) -> AccountsFile`: outside `Dev`, removes every `Provider::Offline` account and sets `active` to `None` when it named one.
  - `pub fn hides_offline(file: &AccountsFile, build: BuildKind) -> bool`: outside `Dev`, true when the file holds at least one offline account.
  - `pub struct MsaWaiting { pub client_id: String, pub at: u64 }` (Serialize/Deserialize, camelCase, Clone, PartialEq).
  - `AccountsFile` gains `#[serde(default)] pub msa_waiting: Option<MsaWaiting>` and `#[serde(default)] pub offline_notice_dismissed: bool`. `licensed_signin_seen` stays, so old files still parse, but it is no longer read.
  - These two are the launcher's own state. They live in the accounts file, not in `Settings` as the spec puts it, because the UI writes `Settings` whole through `settings_set` and would overwrite them.
  - `AccountStore`:
    - `async fn add_microsoft(&self, account: Account) -> Result<AccountsFile>` (upserts by uuid and makes it active)
    - `async fn set_msa_waiting(&self, waiting: Option<MsaWaiting>) -> Result<()>`
    - `async fn dismiss_offline_notice(&self) -> Result<()>`
  - `add_offline(name, build)` keeps its signature and refuses with `OfflineLocked` unless `Dev`.

- [ ] **Step 1: Write the failing unit tests** in `accounts.rs`'s test module:
  - `offline_gate`: Dev is allowed, Beta and Release are not.
  - `visible_drops_offline_and_clears_active`: a file holding offline `Steve` (active) and a Microsoft `Alex` gives, for Beta, only `Alex` and `active == None`; for Dev it is unchanged.
  - `hides_offline_only_outside_dev`.
  - `old_file_parses`: JSON with only `accounts`, `active` and `licensedSigninSeen` parses with `msa_waiting == None` and `offline_notice_dismissed == false`.
  - `add_microsoft_upserts_and_activates`.
  - `add_offline_refused_in_beta_even_after_licensed_signin`: `licensed_signin_seen = true` and Beta still gives `OfflineLocked`.
- [ ] **Step 2:** Run `cargo test -p launcher-core accounts`. Expected: FAIL (signatures and fields missing).
- [ ] **Step 3: Implement** the interfaces above.
  - In `app.rs`, `app_info` reports `offline_allowed(self.config.build)` and drops the `licensed_signin_seen` read.
  - `accounts_list` and every `active()` lookup in the app go through `visible(…, self.config.build)`.
  - The CLI calls `offline_allowed(BuildKind::current())`.
- [ ] **Step 4:** Run `cargo test -p launcher-core accounts` and `cargo test -p launcher-app`. Expected: PASS. Fix the app tests that asserted the beta gate (`app.rs` around the `beta.account_add_offline` test): beta now refuses.
- [ ] **Step 5: Commit** `feat(accounts): offline accounts only in dev builds`.

### Task 2: New error codes and their words; retire `MicrosoftUnavailable`

**Files:**
- Modify: `crates/launcher-core/src/error.rs` (the `error_codes!` list and its snake_case name table)
- Modify: `ui/src/i18n/en.json` and `es.json` (`errors.*`)
- Modify: `crates/perland-cli/src/main.rs` (`launch_instance`)
- Regenerate: `ui/src/ipc/contract/error_codes.json`

**Interfaces:**
- Produces these `ErrorCode` variants under `// accounts`:

  | Variant | snake_case name |
  |---|---|
  | `MicrosoftNotConfigured` | `microsoft_not_configured` |
  | `MicrosoftAwaitingApproval` | `microsoft_awaiting_approval` |
  | `MicrosoftSignInAgain` | `microsoft_sign_in_again` |
  | `MicrosoftRateLimited` | `microsoft_rate_limited` |
  | `MinecraftNotOwned` | `minecraft_not_owned` |
  | `MinecraftNoProfile` | `minecraft_no_profile` |
  | `XboxNoProfile` | `xbox_no_profile` |
  | `XboxRegionBlocked` | `xbox_region_blocked` |
  | `XboxAgeVerification` | `xbox_age_verification` |
  | `XboxChildAccount` | `xbox_child_account` |

- Removes `MicrosoftUnavailable` and its strings.
- The English messages are fixed copy:

  | Code | message | fix |
  |---|---|---|
  | `microsoft_awaiting_approval` | "Microsoft sign-in is waiting on approval from Microsoft. You'll be able to play as soon as it's ready." | none |
  | `microsoft_not_configured` | "Microsoft sign-in isn't set up in this build." | none |
  | `microsoft_sign_in_again` | "Sign in with Microsoft again to keep playing." | "Sign in" |
  | `minecraft_not_owned` | "This Microsoft account doesn't own Minecraft: Java Edition." | "Sign in with the account that bought it, or get it at minecraft.net." |
  | `minecraft_no_profile` | "This account owns Minecraft but hasn't picked a player name yet." | "Set one up at minecraft.net, then sign in again." |
  | `xbox_no_profile` | "This Microsoft account has no Xbox profile yet." | "Sign in once at xbox.com to create one, then try again." |
  | `xbox_region_blocked` | "Xbox Live isn't available in your country, so this account can't sign in." | none |
  | `xbox_age_verification` | "This account needs adult verification on xbox.com before it can sign in." | none |
  | `xbox_child_account` | "This is a child account. An adult needs to add it to a Microsoft family first." | none |
  | `microsoft_rate_limited` | "Microsoft is getting too many sign-ins right now. Try again in a minute." | none |

  The implementer writes the Spanish to match.

- [ ] **Step 1:** Add the variants and names. The existing tests on `ErrorCode::ALL` and the UI's `contract.test.ts` (every code has `errors.<code>.message` in both languages) are the failing tests.
- [ ] **Step 2:** Run `cargo test -p launcher-core error`, then `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then `pnpm --filter ui test contract`. Expected: the UI contract test FAILS for the missing strings.
- [ ] **Step 3:** Add the strings to `en.json` and `es.json`. Remove `errors.microsoft_unavailable`.
  - The CLI's non-offline branch becomes `OfflineLocked`, with the fix "This is a dev harness; use a debug build".
  - Its refusal of offline accounts in release builds comes from Task 1's `offline_allowed`.
- [ ] **Step 4:** Run `cargo test --workspace` and `pnpm --filter ui test`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(errors): Microsoft sign-in error codes`.

### Task 3: Keyed credential store for refresh tokens

**Files:**
- Create: `crates/launcher-core/src/msa/mod.rs` (module root; `pub mod secrets;` for now)
- Create: `crates/launcher-core/src/msa/secrets.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod msa;`)

**Interfaces:**
- Consumes: `perland::session::{SecretError, KEYRING_SERVICE}` and the keyring error mapping in `perland/session.rs`. Reuse `map_error`; make it `pub(crate)` if needed.
- Produces:
  - `pub trait KeyedSecrets: Send + Sync`, with:
    - `fn load(&self, key: &str) -> Result<Option<String>, SecretError>`
    - `fn save(&self, key: &str, value: &str) -> Result<(), SecretError>`
    - `fn delete(&self, key: &str) -> Result<(), SecretError>` (deleting what isn't there is fine)
  - `pub struct KeyringSecrets` (`new()`), on service `KEYRING_SERVICE`, with the key as the keyring user.
  - `pub struct MemorySecrets` (Default).
  - `pub const CHUNK_CHARS: usize = 1000`. Windows Credential Manager caps a credential at 2560 bytes, stored as UTF-16, so chunks always stay under the cap.
  - `pub fn save_refresh(store: &dyn KeyedSecrets, uuid: &str, token: &str) -> Result<(), SecretError>` writes `msa:<uuid>:0`, `:1`, … and then deletes chunks past the new count until a load finds none.
  - `pub fn load_refresh(store: &dyn KeyedSecrets, uuid: &str) -> Result<Option<String>, SecretError>` joins chunks from `:0` until one is missing. `None` when `:0` is missing.
  - `pub fn delete_refresh(store: &dyn KeyedSecrets, uuid: &str) -> Result<(), SecretError>`.

- [ ] **Step 1: Write the failing unit tests** with `MemorySecrets`:
  - `round_trips_a_long_token`: 2,500 characters give three chunks and load back equal.
  - `shorter_token_removes_stale_chunks`: save 2,500, then 10; `:1` and `:2` are gone.
  - `missing_is_none`.
  - `delete_removes_every_chunk`.
  - `keys_are_per_account`.
- [ ] **Step 2:** Run `cargo test -p launcher-core msa::secrets`. Expected: FAIL.
- [ ] **Step 3: Implement.** `KeyringSecrets` follows `KeyringStore` in `perland/session.rs`: the same features per platform, and `NoBackend` where there is none.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat(msa): keyed credential store for refresh tokens`.

### Task 4: Microsoft OAuth (authorize URL, code and refresh grants)

**Files:**
- Create: `crates/launcher-core/src/msa/oauth.rs`
- Modify: `crates/launcher-core/src/msa/mod.rs` (endpoints, client, token types)
- Test: `crates/launcher-core/tests/msa_oauth.rs`

**Interfaces:**
- Consumes: `perland::Pkce` (verifier, challenge, state); `http::HttpClient`.
- Produces, in `msa/mod.rs`:
  - `pub struct MsaEndpoints { pub authorize: Url, pub token: Url, pub xbox_user: Url, pub xsts: Url, pub minecraft: Url }` with `pub fn production() -> Self`. The values are the Global Constraints table; `minecraft` is the base, without a path.
  - `pub struct MsaClient`, built with `pub fn new(http: HttpClient, endpoints: MsaEndpoints, client_id: String) -> Self`.
  - `pub struct MsTokens { pub access_token: String, pub refresh_token: String }` (manual `Debug`, redacted).
- Produces, in `msa/oauth.rs`, methods on `MsaClient`:
  - `pub fn authorize_url(&self, redirect: &str, pkce: &Pkce) -> Url`, with query `client_id`, `response_type=code`, `redirect_uri`, `scope=XboxLive.signin offline_access`, `code_challenge`, `code_challenge_method=S256`, `state` and `prompt=select_account`.
  - `pub async fn redeem_code(&self, code: &str, verifier: &str, redirect: &str) -> Result<MsTokens>`: a form POST with `client_id`, `grant_type=authorization_code`, `code`, `code_verifier`, `redirect_uri` and `scope`.
  - `pub async fn refresh(&self, refresh_token: &str) -> Result<MsTokens>`: a form POST with `client_id`, `grant_type=refresh_token`, `refresh_token` and `scope`.
- Error mapping:
  - A 400 whose body's `error` is `invalid_grant` gives `MicrosoftSignInAgain`.
  - Another 4xx gives `SignInFailed`.
  - A 429 gives `MicrosoftRateLimited`.
  - 5xx and transport failures map the way `perland::wire` does (`HostTrouble`, `HostUnreachable`, `ConnectionDropped`).
  - No token is ever in a message or detail.

- [ ] **Step 1: Write the failing tests** in `tests/msa_oauth.rs` (wiremock; fake tokens only):
  - `authorize_url_carries_pkce_and_scope`: a unit-style check on the parsed query.
  - `redeem_code_posts_the_verifier`: matches the form body and returns both tokens.
  - `refresh_posts_the_refresh_token`.
  - `invalid_grant_is_sign_in_again`.
  - `rate_limited_is_its_own_code`.
  - `errors_never_carry_tokens`: format the error with `{:?}` and check it contains neither token.
- [ ] **Step 2:** Run `cargo test -p launcher-core --test msa_oauth`. Expected: FAIL.
- [ ] **Step 3: Implement** with reqwest `.form()`.
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5: Commit** `feat(msa): Microsoft OAuth code and refresh grants`.

### Task 5: Xbox Live and XSTS

**Files:**
- Create: `crates/launcher-core/src/msa/xbox.rs`
- Test: `crates/launcher-core/tests/msa_xbox.rs`

**Interfaces:**
- Produces:
  - `pub struct XboxUser { pub token: String, pub uhs: String }` (redacted `Debug`)
  - `pub struct Xsts { pub token: String, pub uhs: String, pub xuid: Option<String> }` (redacted `Debug`)
  - `MsaClient::xbox_user(&self, ms_access: &str) -> Result<XboxUser>`
  - `MsaClient::xsts(&self, user: &XboxUser) -> Result<Xsts>`
  - `pub fn xerr_code(xerr: u64) -> ErrorCode` (pure)
- Request bodies are the spec's values, sent as JSON with `Accept: application/json`.
- `xerr_code` maps the spec's table:

  | `XErr` | Code |
  |---|---|
  | 2148916233 | `XboxNoProfile` |
  | 2148916235 | `XboxRegionBlocked` |
  | 2148916236, 2148916237 | `XboxAgeVerification` |
  | 2148916238 | `XboxChildAccount` |
  | anything else | `SignInFailed` |

- `uhs` comes from `DisplayClaims.xui[0].uhs` and `xuid` from the XSTS `DisplayClaims.xui[0].xid`. A missing `uhs` gives `SignInFailed`.

- [ ] **Step 1: Write the failing tests:**
  - `user_token_sends_rps_ticket`: the body has `RpsTicket` `d=<token>`.
  - `xsts_reads_uhs_and_xuid`.
  - `xsts_401_maps_every_xerr`: one wiremock case per row of the table.
  - `xerr_code_unknown_is_sign_in_failed`: a unit test.
- [ ] **Step 2:** Run `cargo test -p launcher-core --test msa_xbox`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5: Commit** `feat(msa): Xbox Live and XSTS tokens`.

### Task 6: Minecraft token, ownership, profile, and the chain

**Files:**
- Create: `crates/launcher-core/src/msa/minecraft.rs`
- Test: `crates/launcher-core/tests/msa_minecraft.rs`

**Interfaces:**
- Produces:
  - `pub struct McToken { pub access_token: String, pub expires_at: u64 }` (Unix seconds; redacted `Debug`)
  - `pub struct McProfile { pub id: String, pub name: String }` (`id` is the API's 32-hex form)
  - `pub fn dashed(id: &str) -> String` (pure: 32 hex characters to the 8-4-4-4-12 form)
  - `MsaClient::minecraft_login(&self, xsts: &Xsts, now: u64) -> Result<McToken>`: body `identityToken: "XBL3.0 x=<uhs>;<token>"`, `expires_at = now + expires_in`. **403 gives `MicrosoftAwaitingApproval`**, 429 gives `MicrosoftRateLimited`, 401 gives `SignInFailed`.
  - `MsaClient::owns_game(&self, mc: &McToken) -> Result<bool>`: `GET /entitlements/mcstore`; true when an item's `name` is `game_minecraft` or `product_minecraft`.
  - `MsaClient::profile(&self, mc: &McToken) -> Result<Option<McProfile>>`: `GET /minecraft/profile`. 404 gives `Ok(None)`; **401 gives `MicrosoftSignInAgain`**.
  - `pub fn ownership(profile: Option<McProfile>, owns: bool) -> Result<McProfile>` (pure). `Some` gives Ok. `None` with `owns` gives `MinecraftNoProfile`. `None` without it gives `MinecraftNotOwned`.
  - `pub struct SignedIn { pub profile: McProfile, pub refresh_token: String, pub minecraft: McToken, pub xuid: Option<String> }` (redacted `Debug`)
  - `MsaClient::finish(&self, ms: MsTokens, now: u64) -> Result<SignedIn>`. It runs `xbox_user`, then `xsts`, then `minecraft_login`, then `profile`. `owns_game` is asked only when the profile is `None`. The result goes through `ownership`.

- [ ] **Step 1: Write the failing tests:**
  - `login_403_is_awaiting_approval`
  - `login_sets_expiry_from_now`
  - `profile_404_is_none`
  - `profile_401_is_sign_in_again`
  - `entitlements_recognise_both_names`
  - `ownership_decides_three_ways` (unit)
  - `dashed_formats_uuid` (unit)
  - `finish_runs_the_whole_chain`: every endpoint mocked, each with `.expect(1)`.
  - `finish_asks_entitlements_only_without_profile`
- [ ] **Step 2:** Run `cargo test -p launcher-core --test msa_minecraft`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** Run it again. Expected: PASS.
- [ ] **Step 5: Commit** `feat(msa): Minecraft token, ownership and profile`.

### Task 7: The app's Microsoft sign-in (commands, approval memo, browser page)

**Files:**
- Modify: `crates/launcher-core/src/perland/loopback.rs` (a page variant)
- Create: `crates/launcher-app/src/msa.rs`
- Modify:
  - `crates/launcher-app/src/runner.rs`: `fn msa(&self) -> Arc<dyn MsaApi>`, and `CoreRunner` holds one
  - `crates/launcher-app/src/app.rs`: fields; `AppConfig.ms_client_id: Option<String>`; the `AppCore::new` parameter `msa_secrets: Arc<dyn KeyedSecrets>`
  - `crates/launcher-app/src/testing.rs`: `FakeMsa`, plus `MemorySecrets` in the test core
  - `crates/launcher-app/src/events.rs`: `AppEvent::MicrosoftSignIn { phase: PerlandSignInPhase }`
  - `crates/launcher-app/src/dto.rs`: `AccountReady`

**Interfaces:**
- Consumes: Tasks 1, 3, 4 and 6; `perland::{Pkce, Loopback, Callback, Language}`; the sign-in slot pattern of `AppCore::start_sign_in` in `perland.rs`.
- Produces:
  - The trait, in `launcher-app/src/msa.rs`:

    ```rust
    pub trait MsaApi: Send + Sync + 'static {
        fn client_id(&self) -> Option<String>;
        fn authorize_url(&self, redirect: &str, pkce: &Pkce) -> Result<String>;
        fn redeem(&self, code: String, verifier: String, redirect: String) -> BoxFuture<Result<SignedIn>>;
        fn refresh(&self, refresh_token: String) -> BoxFuture<Result<SignedIn>>;
        fn profile(&self, token: McToken) -> BoxFuture<Result<Option<McProfile>>>;
    }
    ```

  - `pub struct CoreMsa`, built with `CoreMsa::new(http, endpoints, client_id: Option<String>)`. Every method answers `MicrosoftNotConfigured` while `client_id` is `None`. `redeem` is `redeem_code` then `finish`, and `refresh` is `refresh` then `finish`.
  - The commands, on `AppCore`:
    - `pub async fn account_sign_in_microsoft(&self, language: &str) -> Result<Option<AccountsFile>>`
    - `pub fn account_sign_in_cancel(&self)`
    - `pub async fn account_ready(&self) -> Result<AccountReady>`
    - `pub async fn account_dismiss_offline_notice(&self) -> Result<()>`
  - `account_remove` also calls `delete_refresh` and drops the account's in-memory token.
  - `#[derive(Serialize)] #[serde(rename_all = "camelCase")] pub struct AccountReady { pub ready: bool, pub awaiting_approval: bool, pub offline_notice: bool }`:
    - `ready`: the visible active account is Microsoft and `load_refresh` is `Some`. No network call.
    - `awaiting_approval`: `msa_waiting.client_id == msa().client_id()`.
    - `offline_notice`: `hides_offline` and not dismissed.
  - `AppCore` fields:
    - `msa_secrets: Arc<dyn KeyedSecrets>`
    - `msa_tokens: Mutex<HashMap<String, (McToken, Option<String>)>>`, keyed by dashed uuid, holding the Minecraft token and the xuid
    - `msa_refresh: tokio::sync::Mutex<()>`
    - `msa_sign_in: Mutex<Option<watch::Sender<bool>>>`
  - `Page::MicrosoftContinue` in `loopback.rs`. English: "Go back to Perland Launcher to finish signing in. You can close this tab." Spanish: "Vuelve a Perland Launcher para terminar de iniciar sesión. Puedes cerrar esta pestaña." `Loopback::wait` gains a `success: Page` argument (Perland callers pass `Page::Success`).
- The flow:
  1. Take the slot, cancelling the previous attempt.
  2. Build the PKCE and bind the loopback.
  3. Open `authorize_url` with `host.open_url`.
  4. Emit `MicrosoftSignIn{Waiting}`, then wait.
  5. On `Callback::Code`, emit `Exchanging` and `redeem`.
  6. If the attempt was cancelled meanwhile, keep nothing and return `Ok(None)`.
  7. On `MicrosoftAwaitingApproval`, store `MsaWaiting { client_id, at: now }`, then return the error.
  8. On success:
     - clear `msa_waiting`
     - `save_refresh`
     - put the Minecraft token and xuid in `msa_tokens`
     - `add_microsoft(Account { provider: Microsoft, username: profile.name, uuid: dashed(profile.id) })`
     - return the visible file.
  - Denied or cancelled gives `Ok(None)`.

- [ ] **Step 1: Write the failing tests** in `msa.rs`'s test module, using `FakeMsa` (scripted answers) and the test core:
  - `sign_in_stores_account_and_secret`
  - `awaiting_approval_is_remembered_then_cleared`: the first redeem gives 403 and `account_ready().awaiting_approval` is true. A later success sets it false.
  - `awaiting_approval_ignores_another_client_id`
  - `second_sign_in_cancels_first`
  - `cancel_while_redeeming_keeps_nothing`
  - `not_configured_refuses_before_opening_the_browser`: no `open_url` was recorded.
  - `ready_needs_a_stored_refresh_token`
  - `offline_notice_shows_until_dismissed` (Beta core, file with an offline account)
  - `remove_deletes_the_secret`
  - In `loopback.rs`, `microsoft_page_says_continue` (en and es).
- [ ] **Step 2:** Run `cargo test -p launcher-app msa` and `cargo test -p launcher-core loopback`. Expected: FAIL.
- [ ] **Step 3: Implement.** Mirror `sign_in_in_browser` in `perland.rs` for the slot, cancel and phases.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5: Commit** `feat(app): Microsoft sign-in`.

### Task 8: The check before every launch

**Files:**
- Modify: `crates/launcher-app/src/msa.rs` (`session_for`)
- Modify: `crates/launcher-app/src/runs.rs` (lines 161-171: the account branch)
- Test: in `msa.rs` and `runs.rs`

**Interfaces:**
- Consumes: Task 7.
- Produces: `pub(crate) async fn session_for(&self, account: Account) -> Result<Session>` on `AppCore`.
- **Offline:** allowed only when `offline_allowed(build)`, else `NoAccount`. Gives `offline_session()`.
- **Microsoft:**
  1. Hold `msa_refresh` for the whole check, so concurrent launches share one refresh.
  2. Load the refresh token. Missing gives `MicrosoftSignInAgain`.
  3. If `msa_tokens` has no entry, or `expires_at - now < 3600`, call `refresh`. Then `save_refresh` with the **new** refresh token and store the token in memory.
  4. Call `profile`. `None` gives `MinecraftNotOwned`. `MicrosoftSignInAgain` (a 401) refreshes once and asks again; a second 401 is `MicrosoftSignInAgain`.
  5. A refresh refused with `MicrosoftSignInAgain` deletes nothing and returns it.
  6. Any network error is returned as is, so the game never starts on it.
  7. Update the stored username when the profile name changed.
  8. Return `Session { username: profile.name, uuid: profile.id (undashed), access_token, user_type: "msa", xuid }`.
- In `runs.rs`, replace the `Provider::Offline` refusal with `let session = self.session_for(account).await?;`. The active account comes from `visible(…)`. Pass the client id as `clientid`: add `client_id: Option<String>` to `Session`, filled for Microsoft, and use it in `launch/mod.rs` for `("clientid", …)`.

- [ ] **Step 1: Write the failing tests:**
  - `fresh_token_skips_refresh_but_checks_profile`
  - `stale_token_refreshes_and_stores_rotated_token`
  - `concurrent_launches_share_one_refresh`: two `session_for` at once and `FakeMsa` counts one refresh.
  - `refresh_refused_asks_to_sign_in_again`
  - `profile_401_refreshes_once`
  - `network_failure_never_launches`: a cached valid token and `profile` erroring `HostUnreachable` give that error.
  - `active_offline_in_beta_is_no_account`, in `runs.rs`, through `play`.
  - `msa_session_is_msa_with_xuid_and_clientid`
  - `access_token_is_redacted_in_game_log`, in `runs.rs`: the game's output line containing the token is stored with `<redacted>`.
- [ ] **Step 2:** Run `cargo test -p launcher-app`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(app): check the Microsoft session before every launch`.

### Task 9: Wiring, contract and release build

**Files:**
- Modify: `src-tauri/src/main.rs` (client id; `CoreMsa`; `KeyringSecrets`, or `MemorySecrets` where the keyring has no backend, as `perland_store` decides)
- Modify: `src-tauri/src/commands.rs` (four commands: `account_sign_in_microsoft`, `account_sign_in_cancel`, `account_ready`, `account_dismiss_offline_notice`; register them in `main.rs`)
- Modify: `.github/workflows/release.yml` (build env `PERLAND_MS_CLIENT_ID: ${{ vars.PERLAND_MS_CLIENT_ID }}`, next to `PERLAND_BUILD`)
- Modify: `crates/launcher-app/tests/contract.rs` (samples: `AccountReady` with each flag, `AccountsFile` with `msaWaiting`, the `MicrosoftSignIn` event)
- Regenerate: `ui/src/ipc/contract/*.json`

**Interfaces:**
- Produces, in `crates/launcher-core/src/msa/mod.rs`, so it is tested in core rather than in `src-tauri`:
  - `pub fn client_id(run: Option<&str>, built: Option<&str>, build: BuildKind) -> Option<String>`
  - The runtime value is used **only when `build == Dev`**.
  - Accepted form: 36 characters, a GUID `8-4-4-4-12` of hex. Anything else is ignored with a warning.
  - `main.rs` calls it with `std::env::var("PERLAND_MS_CLIENT_ID").ok()` and `option_env!("PERLAND_MS_CLIENT_ID")`.

- [ ] **Step 1: Write the failing unit tests** in `msa/mod.rs`:
  - `client_id_runtime_only_in_dev`
  - `client_id_rejects_non_guid`
  - `client_id_built_value_used_in_release`
- [ ] **Step 2:** Run `cargo test -p launcher-core msa::tests`. Expected: FAIL.
- [ ] **Step 3: Implement**, wire `main.rs` and `commands.rs`, add the workflow line, and add the contract samples.
- [ ] **Step 4:** Run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat: wire Microsoft sign-in into the app and release build`.

### Task 10: UI: schema, commands, mock; First run and Accounts

**Files:**
- Modify: `ui/src/ipc/schema.ts`:
  - `accountsFile` gains `msaWaiting: z.strictObject({ clientId: z.string(), at: z.number().int() }).nullable()` and `offlineNoticeDismissed: z.boolean()`
  - new `accountReady`
  - the `microsoftSignIn` event
- Modify: `ui/src/ipc/commands.ts` (`accountSignInMicrosoft(language)`, `accountSignInCancel()`, `accountReady()`, `accountDismissOfflineNotice()`)
- Modify: `ui/src/ipc/mock.ts`: `MockOptions.microsoft?: 'signedOut' | 'signedIn' | 'awaiting' | 'notConfigured' | 'notOwned'`
  - the four commands; sign-in resolves after 1.5 s with a Microsoft account `Alex`, or fails with the matching code
  - `beta: true` hides offline accounts and sets `offlineNotice` when the fixture holds one
- Modify: `ui/src/features/first-run/FirstRun.tsx`, `ui/src/features/accounts/Accounts.tsx`
- Create: `ui/src/features/accounts/MicrosoftSignIn.tsx`: the button with its waiting and exchanging states, Cancel, and the awaiting-approval line. First run, Accounts and the prompt share it.
- Modify: `ui/src/i18n/en.json` and `es.json`: `accounts.microsoft*` and `firstRun.*`. Remove `accounts.offlineLocked`, `accounts.removeBodyLocked`, `errors.offline_locked.fix`'s "coming soon" and `firstRun.accountLockedBody`.
- Test: `FirstRun.test.tsx`, `Accounts.test.tsx`, `MicrosoftSignIn.test.tsx`, `mock.test.ts`

**Interfaces:**
- Consumes: Task 9's contract.
- Produces: `MicrosoftSignIn` props `{ onSignedIn?: () => void; variant: 'primary' | 'inline' }`. It reads `accountReady` through a `useAccountReady()` query (key `['accountReady']`), which every sign-in, remove and dismiss invalidates.
- The English copy:

  | Place | Text |
  |---|---|
  | Button | "Sign in with Microsoft" |
  | Why line | "You need a Microsoft account that owns Minecraft: Java Edition." |
  | While waiting | "Finish signing in in your browser." |
  | Awaiting-approval line | the `errors.microsoft_awaiting_approval.message` text; the button reads "Try again" |

- [ ] **Step 1: Write the failing Vitest tests:**
  - `FirstRun`:
    - a beta `appInfo` shows "Sign in with Microsoft" and no name field
    - a dev one shows the name field
    - Skip goes to step 3
  - `Accounts`:
    - beta shows "Add Microsoft account" and never "Add offline account"
    - a Microsoft account lists with Remove
  - `MicrosoftSignIn`:
    - shows the waiting state, and Cancel calls `accountSignInCancel`
    - awaiting approval shows the line and "Try again"
    - a `minecraft_not_owned` failure shows its translated message
  - `mock.test.ts`: the `awaiting` option rejects with `microsoft_awaiting_approval` and then `accountReady` says `awaitingApproval: true`.
- [ ] **Step 2:** Run `pnpm --filter ui test`. Expected: FAIL.
- [ ] **Step 3: Implement**, through `/impeccable` for the button and states.
- [ ] **Step 4:** Run `pnpm --filter ui test`, `pnpm -r typecheck` and `pnpm -r lint`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(ui): Microsoft sign-in in First run and Accounts`.

### Task 11: UI: one sign-in prompt, early checks, the notice for older betas

**Files:**
- Create: `ui/src/state/signInPrompt.ts`: a store like `toasts.ts`, with `openSignInPrompt()`, `closeSignInPrompt()` and `useSignInPrompt()`
- Create: `ui/src/features/accounts/SignInPrompt.tsx`: a dialog holding `MicrosoftSignIn variant="primary"`, mounted once in `App.tsx`
- Modify: `ui/src/state/toasts.ts`: `pushError` opens the prompt instead of a toast when the error is a `LauncherError` with code `no_account` or `microsoft_sign_in_again`
- Create: `ui/src/state/requireAccount.ts`: `async function requireAccount(): Promise<boolean>`. It calls `commands.accountReady()`, opens the prompt and returns false when not `ready`. A failed call returns true and leaves the launch check to decide.
- Modify:
  - `ui/src/state/queries.ts` (`useJoinAdd`): `await requireAccount()` before the instance is created
  - `ui/src/features/free-server/usePlayFreeServer.ts`: before the server is started
  - `ui/src/features/servers-list/PublicServerCard.tsx`: before the join
- Create: `ui/src/features/home/OfflineAccountsNotice.tsx`, shown on `Home.tsx` while `accountReady.offlineNotice`. It holds the sign-in button and a Dismiss that calls `accountDismissOfflineNotice`.
- Modify: `ui/src/i18n/en.json` and `es.json` (`accounts.prompt*`, `home.offlineGone*`)
- Test: `toasts.test.ts`, `requireAccount.test.ts`, `SignInPrompt.test.tsx`, `OfflineAccountsNotice.test.tsx`, and the existing `PublicServerCard`, `usePlayFreeServer` and `useJoinAdd` tests

**Interfaces:**
- Consumes: Task 10's `MicrosoftSignIn` and `useAccountReady`.
- The English copy:

  | Place | Text |
  |---|---|
  | Prompt title | "Sign in to play" |
  | Prompt body | "Perland Launcher needs a Microsoft account that owns Minecraft: Java Edition." |
  | After sign-in | the prompt closes and a notice says "Signed in. Press Play again." |
  | Notice | "Offline accounts are gone. Perland Launcher now needs a Microsoft account that owns Minecraft: Java Edition." |

- [ ] **Step 1: Write the failing tests:**
  - `pushError` with `no_account` adds no toast and opens the prompt; any other error is a toast as before
  - `requireAccount` returns false and opens the prompt when not ready, and true when ready or when `accountReady` fails
  - `PublicServerCard`: Join while not ready never calls the join command
  - `usePlayFreeServer`: never starts the server while not ready
  - `useJoinAdd`: never creates an instance while not ready
  - `OfflineAccountsNotice`: shows only while `offlineNotice`, and Dismiss calls the command and hides it
- [ ] **Step 2:** Run `pnpm --filter ui test`. Expected: FAIL.
- [ ] **Step 3: Implement**, through `/impeccable` for the dialog and the notice.
- [ ] **Step 4:** Run `pnpm --filter ui test`, `pnpm -r typecheck`, `pnpm -r lint` and `pnpm --filter ui build`. Expected: PASS. Then check by hand in `pnpm --filter ui dev:mock` with `beta` and each `microsoft` option.
- [ ] **Step 5: Commit** `feat(ui): sign-in prompt, early checks and the offline-accounts notice`.

### Task 12: Wording outside the app

**Files:**
- Modify: `README.md` (Develop: `PERLAND_MS_CLIENT_ID`, read at runtime only in dev builds; Releasing: the `vars.PERLAND_MS_CLIENT_ID` repository variable)
- Modify: `PRODUCT.md` (lines 18, 41 and 57: needs a Microsoft account that owns Minecraft: Java Edition; offline accounts only in local dev builds)
- Modify: `docs/superpowers/specs/2026-10-09-launcher-core-design.md` and `2026-10-10-launcher-desktop-ui-design.md`: one line where each describes the offline gate, "Superseded by `2026-10-27-microsoft-sign-in-design.md`"
- Modify: `CLAUDE.md`: one line, "`PERLAND_MS_CLIENT_ID` is the Microsoft client ID, built in; offline accounts exist only in dev builds"

- [ ] **Step 1:** Make the edits.
- [ ] **Step 2:** Run `grep -rniE "offline/dev accounts|coming soon|later plan" README.md PRODUCT.md ui/src/i18n`. Expected: no match about Microsoft or offline accounts.
- [ ] **Step 3: Commit** `docs: Microsoft sign-in replaces offline accounts`.
