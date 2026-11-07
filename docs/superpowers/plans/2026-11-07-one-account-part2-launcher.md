# One Perland account, Part 2 (launcher) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every launcher sign-in and sign-up goes through the panel's sign-in page (OAuth); the launcher stops calling the deprecated player sign-in routes; the in-launcher free-server create asks for a verified email with an in-launcher code step.

**Architecture:** `launcher-core` loses the older sign-in clients (`/player/token`, `/player/email/*`, Discord's `/player/discord/authorize`, the panel's `/launcher/sign-in` page) and gains the account-email client (`/api/v1/auth/me/email/start|verify`, `Authorization: Bearer plt_…`) and the `launcher_update_required` code. `launcher-app` keeps one browser sign-in (OAuth) and gains `perland_account_email_start|verify`. The UI's account card keeps one button (Continue with Perland); the free server slot and the create dialog show an email-code step on `email_required`.

**Tech Stack:** Rust (reqwest, wiremock tests), Tauri wiring, React + Vitest + Testing Library.

**Spec:** PerlandEngine `docs/superpowers/specs/2026-11-07-one-account-registration-design.md`, section "One way in" and decision 4 (this session can't read PerlandEngine; the coordinator relayed the section verbatim, quoted below).

> - "A launcher player who must verify an email for a free server does it inside the launcher: the account email routes /auth/me/email/start|verify also accept a plt_ device session, and the launcher's free-server create shows the code step on email_required."
> - "Every sign-in and registration goes through the panel's sign-in page: the launcher already uses OAuth (/oauth/authorize, PKCE, loopback) for GitHub and Discord."
> - "The launcher removes in-app email sign-in (its email method opens the browser like the others)."
> - "Deprecated, then removed after a window of one launcher release that no longer calls them: /player/email/start, /player/email/verify, /player/discord/authorize, /player/token (the OAuth token endpoint stays), /auth/launcher-code, /auth/player-session. While deprecated they sign existing accounts in but create none (403 launcher_update_required)."
> - Decision 4: "The launcher skips the plan page. It keeps creating the free server inside the launcher; its upgrade links open the panel's plan page, signed in (the existing handoff)."

## Engine contract (from the coordinator, engine `claude/one-account-part2-engine`)

- `POST /api/v1/auth/me/email/start`, `Authorization: Bearer plt_…`, body `{email, language}` (`en`|`es`) → 202 `{expiresIn, resendIn}`. Errors (`data.code`): 400 `invalid_email`, 429 `too_many_codes` (`data.retryAfter`), 502 `email_unavailable`, 503 `sign_in_method_unavailable` (`data.method`), plain 429.
- `POST /api/v1/auth/me/email/verify`, same header, body `{email, code}` → 200 `{email}`. Errors: 400 `invalid_code` (`data.attemptsLeft`), 409 `email_in_use`, 403 `account_disabled`, plain 429.
- 403 `launcher_update_required`: `{"success":false,"data":{"code":"launcher_update_required"},"error":"…"}` from the deprecated routes when a sign-in would create an account.
- `/player/*` routes take `Bearer plt_` directly; `POST /api/v1/auth/handoff` (Bearer plt_) stays: "More in Perland" is unchanged.

## Rulings

1. **OAuth is the only way in.** An Engine without OAuth discovery shows no Perland card (`PerlandStatus::off()`), as an Engine with no known method does today. Every production Engine has had discovery since the launcher's OAuth release.
2. **Older sessions still sign out** at `/player/sign-out` (`Session.client == None`): not a deprecated route, and a saved session from 0.2.x must end cleanly.
3. **Manage keeps `/auth/player-session` until the engine confirms a replacement** (asked the engine session: `/auth/memberships` + `/auth/select-tenant` with `Bearer plt_`). It creates no account, so it never meets `launcher_update_required`. Task 5 switches it when confirmed; if not confirmed before merge, it stays and the coordinator is told it must not be removed yet.
4. **`launcher_update_required` is mapped wherever the launcher could meet it** (player, auth and OAuth clients) to `ErrorCode::LauncherUpdateRequired`, category `auth`, fix "Update Perland Launcher from Settings." The UI shows it like any error.
5. **The free server's "Add an email" opens the in-launcher email step**, panel or not; the panel's account page is no longer the way to add an email there. The create dialog shows the same step when a create answers `email_required`; after the email is verified the player is back on the form with it filled and presses Create again.
6. **`email_in_use`** gets its own `ErrorCode::EmailInUse` ("That email is already on another Perland account."), fix "Use another email, or sign in with that account."

## Global Constraints

- launcher-core / launcher-app never depend on Tauri; `src-tauri` is wiring only, no tests.
- Every UI string in `ui/src/i18n/en.json` and `es.json` (neutral Latin American Spanish, "tú"); each new `ErrorCode` gets `errors.<code>.message` in both.
- Regenerate the IPC contract: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`; never hand-edit `ui/src/ipc/contract/*.json`.
- Tokens and codes never in logs or `Debug`.
- Pre-commit (last task, and before each push): `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.

## Review Focus

1. A player whose saved session came from `/player/email/verify` or Discord (`client: None`) upgrades: still signed in, Sign out still ends it (`/player/sign-out`). Test in Task 2.
2. The code typed with spaces (`123 456`) verifies. Test in Task 3.
3. Verifying the email updates the shown account email without waiting a minute for `/me`. Test in Task 3.
4. A 401 from the email routes signs the player out like any other `plt_` call. Test in Task 1 (`signed_out`) and Task 3.
5. Closing the email step mid-way and reopening it starts at the email field, not a stale code step. Test in Task 4.

---

### Task 1: Core: account email client, new codes, drop the older sign-in clients

**Files:**
- Modify: `crates/launcher-core/src/perland/api.rs` (remove `token`, `email_start`, `email_verify`, `Route::Token`/`Route::SignIn` if unused; add account email; map new codes)
- Modify: `crates/launcher-core/src/perland/pkce.rs` (remove `authorize_url` + its tests), `panel.rs` (remove `launcher_sign_in_url` + test), `mod.rs` re-exports
- Modify: `crates/launcher-core/src/perland/auth.rs`, `oauth.rs` (map `launcher_update_required`)
- Modify: `crates/launcher-core/src/error.rs` (`LauncherUpdateRequired` = `launcher_update_required`, `EmailInUse` = `email_in_use`)
- Test: `crates/launcher-core/tests/perland.rs` (or a new `tests/perland_account_email.rs`), remove tests of the removed routes

**Interfaces:**
- Produces: `PlayerClient::account_email_start(&self, token: &str, email: &str, language: &str) -> Result<EmailStarted>` (reuses `EmailStarted { resend_in, .. }`); `PlayerClient::account_email_verify(&self, token: &str, email: &str, code: &str) -> Result<String>` (the verified email). Both POST to `/api/v1/auth/me/email/{start,verify}` with `Credential::Bearer(token)`.
- Refusal mapping for these two: `invalid_email`, `too_many_codes`, `email_unavailable`, `sign_in_method_unavailable`, `invalid_code` (with `attemptsLeft`), `email_in_use` → `EmailInUse`, `account_disabled` and 401 → `PerlandSignedOut`, `launcher_update_required` → `LauncherUpdateRequired`.

- [ ] **Step 1: Failing wiremock tests:** `account_email_start_sends_bearer_and_body` (asserts path `/api/v1/auth/me/email/start`, header `authorization: Bearer plt_x`, body `{"email":"a@b.co","language":"es"}`, answer `resend_in == 60`); `account_email_verify_returns_the_email`; `account_email_verify_wrong_code` (400 `invalid_code`, `attemptsLeft: 2` → code `InvalidCode`, param `attemptsLeft = "2"`); `account_email_in_use` (409 → `EmailInUse`); `account_email_401_signs_out` (→ `PerlandSignedOut`); `launcher_update_required_is_its_own_code` (403 on `/api/v1/player/me` and on `oauth_token` → `LauncherUpdateRequired`).
- [ ] **Step 2: Run** `cargo test -p launcher-core --test perland` → fails (methods missing).
- [ ] **Step 3: Implement**; add a route-agnostic `send_auth(route)` beside `send` (prefix `/api/v1/auth`). Remove the older clients and their tests; `cargo build --workspace` will point at the app callers (Task 2 removes them; for this task, delete the callers' trait methods only if needed to compile, or do Tasks 1 and 2 together in one commit).
- [ ] **Step 4: Run** `cargo test -p launcher-core` → PASS.
- [ ] **Step 5: Commit** `feat(core): account email routes and launcher_update_required; drop older sign-in clients`.

### Task 2: App: OAuth is the only sign-in

**Files:**
- Modify: `crates/launcher-app/src/perland.rs` (remove `PlayerApi::{token,email_start,email_verify}`, `Flow`, `perland_sign_in_discord`, `perland_sign_in_github`, `perland_email_start`, `perland_email_verify`, `PerlandState::email_engine`, `offered`; `PerlandStatus::signed_out` → `perland` with discovery, else `off()`)
- Modify: `crates/launcher-app/src/testing.rs` (FakePlayer), `dto.rs` (drop `EmailCodeSent` only if unused after Task 3; it is reused there, so keep)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (unregister the four commands)

**Interfaces:**
- Produces: `PerlandStatus.methods` is `["perland"]` or the status is off. `perland_sign_in(language)` unchanged.

- [ ] **Step 1: Failing tests** in `perland.rs` tests: `no_discovery_means_no_card` (config enabled, methods `[discord, email]`, discovery `None` → `enabled == false`, `methods` empty); `legacy_session_still_signs_out_at_player_sign_out` (saved session with `client: None`; sign out calls FakePlayer `sign_out`, not `revoke`). Delete tests of removed methods.
- [ ] **Step 2: Run** `cargo test -p launcher-app perland` → fails.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p launcher-app` → PASS.
- [ ] **Step 5: Commit** `feat(app): sign in only on the Perland page`.

### Task 3: App: account email commands

**Files:**
- Modify: `crates/launcher-app/src/perland.rs` (or `free_servers.rs`), `testing.rs`, `dto.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, contract test registration

**Interfaces:**
- Consumes: Task 1 client methods (via new `PlayerApi::account_email_start(engine, token, email, language) -> BoxFuture<Result<EmailStarted>>` and `account_email_verify(engine, token, email, code) -> BoxFuture<Result<String>>`).
- Produces: `AppCore::perland_account_email_start(&self, email: &str, language: &str) -> Result<EmailCodeSent>`; `AppCore::perland_account_email_verify(&self, email: &str, code: &str) -> Result<PerlandAccount>` (the account with the new email). Tauri commands `perland_account_email_start { email, language }`, `perland_account_email_verify { email, code }`.

- [ ] **Step 1: Failing tests:** `account_email_start_needs_a_session` (signed out → `PerlandSignedOut`); `account_email_verify_strips_spaces_and_updates_the_profile` (code `"123 456"` reaches the fake as `"123456"`; the returned account and the next `perland_status` show the new email, saved to the marker); `account_email_verify_401_forgets_the_session`.
- [ ] **Step 2: Run** → fail. **Step 3: Implement**, using the session's engine and token; on `PerlandSignedOut` forget the session holding that token (as other calls do). Language via `language_tag`.
- [ ] **Step 4:** `cargo test -p launcher-app` PASS; `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.
- [ ] **Step 5: Commit** `feat(app): verify an account email in the launcher`.

### Task 4: UI: one sign-in button, the email-code step for free servers

**Files:**
- Modify: `ui/src/features/accounts/PerlandAccountCard.tsx` (+ test): only the signed-in, browser and `oauth` views remain; delete the email/code steps and the older-method view.
- Create: `ui/src/features/free-server/EmailCodeStep.tsx` (+ `EmailCodeStep.test.tsx`): email field → Send code → code field with Verify, Send a new code (countdown), Use another email; errors under the field (`invalid_code` with tries left, as the card did). Props: `{ onVerified: (email: string) => void; onCancel: () => void }`.
- Modify: `FreeServerEntry.tsx` (+ test): `email_required` shows "Add an email", always, opening a dialog with `EmailCodeStep`; on verified, invalidate the free-server page and Perland status queries and close.
- Modify: `CreateServerDialog.tsx` (+ test): a create refused with `email_required` swaps the form for `EmailCodeStep`; verified returns to the form with the entered values and a status line `freeServer.email.verified`.
- Modify: `ui/src/ipc/commands.ts`, `schema.ts`, `mock.ts`, `mockPerland.ts` (+ tests): drop `perlandSignInDiscord`, `perlandSignInGithub`, `perlandEmailStart`, `perlandEmailVerify`; add `perlandAccountEmailStart(email, language)` → `EmailCodeSent`, `perlandAccountEmailVerify(email, code)` → `PerlandAccount`. Mock: code `123456` verifies; mock free-server `emailRequired` scenario turns allowed after verify.
- Modify: `ui/src/i18n/en.json`, `es.json`: move the email/code strings the card used into `freeServer.email.*` (title "Add an email", lead "Free servers need a verified email. We'll send you a code.", plus the existing label/button strings), add `freeServer.email.verified`, `errors.launcher_update_required.{message,fix}`, `errors.email_in_use.{message,fix}`; remove keys nothing uses.

- [ ] **Step 1: Failing tests:** card: signed out with `methods: ['perland']` shows one button and no email field; `EmailCodeStep`: sends trimmed email, then verifies `'123 456'` → `onVerified('a@b.co')`; wrong code shows tries left; reopening starts at the email field; `FreeServerEntry`: `email_required` without a panel still offers "Add an email" and opens the step; `CreateServerDialog`: create refused `email_required` shows the step, verified goes back to the form with the name kept.
- [ ] **Step 2: Run** `pnpm --filter ui test` → fail. **Step 3: Implement** (styling via existing `panel`, `Field`, `Button`; no new visual language). **Step 4:** `pnpm --filter ui test`, `pnpm -r lint`, `pnpm -r typecheck` PASS.
- [ ] **Step 5: Commit** `feat(ui): sign in on the Perland page only; verify an email for a free server in the launcher`.

### Task 5 (when the engine confirms): Manage without `/auth/player-session`

**Files:** `crates/launcher-core/src/perland/auth.rs`, `session_jwt.rs`, `crates/launcher-app/src/manage.rs`, tests `crates/launcher-core/tests/perland_auth.rs`.

- [ ] Replace the base-JWT step: `memberships(device)` and `select_tenant(device, tenant)` with `Credential::Bearer(plt_)`; remove `player_session`; the JWT cache keeps per-tenant JWTs only. Tests: wiremock asserts `Authorization: Bearer plt_…` on both routes and that `/auth/player-session` is never called; a 401 on memberships signs the player out (as the refused player-session did).
- [ ] Commit `feat: manage servers without player-session`.

### Task 6: Docs and full check

- [ ] README/CLAUDE.md mentions of email sign-in or Discord routes updated; `NOTICE` untouched.
- [ ] Run the full pre-commit list; all green. Commit, push, PR ready when the engine PR is merged.
