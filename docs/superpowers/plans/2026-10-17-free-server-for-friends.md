# Create a Server for Friends Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A signed-in player creates their one free server in three steps (pick vanilla or one of their Modrinth modpack instances, create, share the link). The limits are shown up front, a pack that is too heavy is refused before creation, and the server shows as its own Owner card in Your servers, with Play that starts it and waits, and Delete. Friends join modpack free servers through the link.

**Architecture:**
- **launcher-core:** `PlayerClient` gains the `/api/v1/player/free-server` routes. The Perland source learns to install a Modrinth modpack version named by an invite view (`modrinth` with `pack: null`).
- **launcher-app:** `free_server*` commands on the session's Engine. `free_server_play` starts the server, joins through the invite and primes the run's wake. Your servers drops the duplicate owner entry, and a card with an instance stays joinable.
- **UI:** the entry and reason line, a three-step dialog, and the free-server card. All of it reuses `PublicServerCard`.

**Tech Stack:** Rust (tokio, reqwest, serde), wiremock, Tauri 2, React + TypeScript + zod + React Query, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-17-free-server-for-friends-design.md`. Its "Decisions" table is binding. It builds on `docs/superpowers/specs/2026-10-17-perland-sign-in-your-servers-design.md` (#48, merged in #60).

## Global Constraints

- `crates/launcher-core` and `crates/launcher-app` may not depend on Tauri or UI crates. `src-tauri` is wiring only and has no tests.
- TDD: write the failing test first.
  - Pure logic gets `#[cfg(test)]` unit tests.
  - HTTP behaviour gets `tests/*.rs` with wiremock.
  - The UI gets Vitest + Testing Library.
- Never run fuzz, stress or load tests locally, and keep test inputs tiny. Build with `CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0` (the disk allowance is limited).
- Errors shown to players are `LauncherError` with a stable `code`. Each new `ErrorCode` gets `errors.<code>.message` in both `ui/src/i18n/en.json` and `es.json`.
- Every UI string goes in both `en.json` and `es.json`, in neutral Latin American Spanish with informal "tú".
- `ui/src/ipc/contract/*.json` is generated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- UI styling goes through the `/impeccable` skill, `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- Tokens and secrets:
  - Never log a `plt_` token or an invite token. Errors name the Engine's host only.
  - Player calls go to the session's Engine origin only, with the token sent as `Authorization: Bearer`, never in a URL.
  - Test tokens are obviously fake, such as `plt_` followed by 43 `x`.
- Engine contract values (PerlandEngine 9810488):
  - name `^[A-Za-z0-9_-]{1,32}$`; `kind` is `vanilla` or `modrinth`; `modrinthVersionId` is `^[A-Za-z0-9]{1,16}$`; `language` is `es`, else `en`;
  - statuses: `online`, `starting`, `sleeping`, `offline`, `stopped` (over disk only), `creating`, `deleting`;
  - error codes: `free_servers_disabled` (503, which reads as off), `email_required`, `free_server_exists`, `too_many_free_servers` (`retryAfter`), `free_servers_full`, `invalid_free_server`, `modpack_unsupported` (`reason`), `modpack_too_heavy` (`requiredMemoryMb`, `limitMemoryMb`, `serverMods`), `free_server_over_disk` (`usedBytes`, `limitBytes`), `modrinth_unavailable`, `quota_exceeded`.
- Timeouts and polling:
  - the create request gets 150 s; every other call keeps 20 s;
  - `GET /free-server` is polled every 30 s, or every 15 s while the status is `creating`, `starting` or `deleting`;
  - after a lost create answer, poll every 5 s for up to 5 minutes.
- The server name prefill comes from the Discord-based `displayName` only, never the email. Without one it is `my-server`.
- Before each commit that touches the UI, run `pnpm --filter ui build`. Before the PR, run the full "Before committing" list from CLAUDE.md.

## Review Focus

1. **The create answer is lost** (a timeout, a dropped connection, or a retry answering `free_server_exists` or `too_many_free_servers` with `retryAfter` ≤ 5): the dialog must poll and reach step 3, not show a failure. → Task 5, `lost_answer_polls_until_the_server_appears`.
2. **Play on an `offline` free server:** start it, then wait for it to be up, not launch into a server that is still booting. → Task 3, `play_offline_starts_and_primes_starting`.
3. **The email never becomes the server name:** an email-only player gets `my-server`. → Task 5, `prefill_never_uses_the_email`.
4. **Free servers off, or an older Engine** (503 `free_servers_disabled` or 404): nothing shows and nothing errors. → Task 1 `free_server_disabled_is_none`, Task 6 `nothing_when_free_servers_off`.
5. **A modpack free server joined from a link** syncs the Modrinth version, and the update check does not report an update forever. → Task 2, `modrinth_stamp_matches_after_resolve`.

---

### Task 1: Free-server API and error codes (launcher-core)

**Files:**
- Modify: `crates/launcher-core/src/perland/api.rs` (`PlayerClient` methods and DTOs), `crates/launcher-core/src/error.rs` (codes and the code-name table), `ui/src/i18n/en.json`, `ui/src/i18n/es.json` (`errors.<code>.message`), and the regenerated `ui/src/ipc/contract/error_codes.json`.
- Test: `crates/launcher-core/tests/perland.rs` (wiremock).

**Interfaces:**
- Produces, in `launcher_core::perland`:
  - `PlayerClient::free_server(token) -> Result<Option<FreeServerState>>`: `None` on 503 `free_servers_disabled` and on 404.
  - `PlayerClient::create_free_server(token, &CreateFreeServer) -> Result<FreeServer>` with a 150 s timeout.
  - `PlayerClient::start_free_server(token) -> Result<FreeServer>`.
  - `PlayerClient::delete_free_server(token) -> Result<bool>`.
  - DTOs, serde camelCase, mirroring spec §4:
    - `FreeServerState { limits: FreeLimits, create: CreateAllowance, server: Option<FreeServer> }`;
    - `FreeLimits { max_players, memory_mb: MemoryLimits { vanilla, modded }, disk_bytes, sleep_minutes, unused_days, warning_days, creates_per_week }`;
    - `CreateAllowance { allowed: bool, reason: Option<String>, retry_after: Option<u64> }`;
    - `FreeServer { name, kind, game: InviteGame, modrinth: Option<InviteModrinth>, memory_mb, max_players, status: String, players: Option<…the invite players shape…>, address: Option<…>, invite_url: Option<String>, join_url: Option<String>, created_at, last_played_at: Option<String>, deletion: Option<Deletion { deletes_at, warned }>, disk: Disk { used_bytes, limit_bytes, checked_at: Option<String>, over: bool } }`. Reuse the existing invite types for `game`, `players` and `address` where the shapes match. `InviteModrinth` comes from Task 2; if Task 1 lands first, define it here in `invite/view.rs` and have Task 2 reuse it.
    - `CreateFreeServer { name, kind: String, modrinth_version_id: Option<String>, language: String }`, which serializes `modrinthVersionId` and omits `None`.
  - New `ErrorCode`s under `// free servers`, with category and params as in spec §6.4: `EmailRequired`, `FreeServerExists`, `TooManyFreeServers`, `FreeServersFull`, `InvalidFreeServer`, `ModpackUnsupported`, `ModpackTooHeavy`, `FreeServerOverDisk`. Mapping:
    - `modrinth_unavailable` → `host_trouble`, with the host `modrinth.com`;
    - `quota_exceeded` → `FreeServersFull`;
    - 401 → `PerlandSignedOut`.
  - English messages (Spanish in the same spirit):

    | Code | Message |
    |---|---|
    | `email_required` | "Free servers need a verified email on your account." |
    | `free_server_exists` | "You already have a free server." |
    | `too_many_free_servers` | "You can't create another server yet." |
    | `free_servers_full` | "Free servers are full right now. Try again later." |
    | `invalid_free_server` | "Check the server name: letters, numbers, - and _ only, up to 32." |
    | `modpack_unsupported` | "This modpack can't run on a free server." |
    | `modpack_too_heavy` | "This modpack needs {{required}} GB; free servers have {{limit}} GB." |
    | `free_server_over_disk` | "Your server is out of space, so it stays off." |

    `required` and `limit` are GB, which the UI fills in from the MB params.

- [ ] **Step 1: Write the failing wiremock tests:**
  - `free_server_reads_state`: limits, `create.allowed`, and a `server` with `deletion` and `disk`.
  - `free_server_disabled_is_none`: 503 `free_servers_disabled` gives `Ok(None)`.
  - `free_server_404_is_none`.
  - `create_posts_body`: a body matcher on `name`, `kind`, `modrinthVersionId` and `language`, plus the bearer header.
  - `create_vanilla_omits_version_id`.
  - `create_too_heavy_carries_memory`: params `requiredMemoryMb` = 8192 and `limitMemoryMb` = 6144.
  - `create_unsupported_carries_reason`.
  - `create_rate_limited_carries_retry_after`.
  - `create_exists`, `create_full`, `create_invalid`, `create_email_required`.
  - `create_quota_is_full`.
  - `create_modrinth_unavailable_is_host_trouble`.
  - `start_over_disk`.
  - `start_answers_view`.
  - `delete_answers_flag`.
  - `free_server_401_is_signed_out`.
- [ ] **Step 2: Run** `cargo test -p launcher-core --test perland free`. Expected: FAIL.
- [ ] **Step 3: Implement**, reusing the existing envelope parsing, error mapping and caps (64 KiB). Add the codes, then regenerate the contract and add the en/es messages.
- [ ] **Step 4: Run** `cargo test -p launcher-core`, `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract` and `pnpm --filter ui test -- i18n`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): free-server API client (#49)`.

### Task 2: Join servers that run a Modrinth modpack (launcher-core)

**Files:**
- Modify:
  - `crates/launcher-core/src/invite/view.rs` (`InviteView.modrinth`, `InviteModrinth`);
  - `crates/launcher-core/src/public_list.rs` (`PublicServer.modrinth`; `joinable` counts it);
  - `crates/launcher-core/src/sources/perland.rs`: `joinable_game` callers, `resolve_inner`, `stamp_of`, `check_inner` if it compares stamps.
- Test: unit tests in those files; `crates/launcher-core/tests/` (extend the Perland source wiremock tests, or add one that serves an invite view with `modrinth` and a Modrinth API version and `.mrpack`; copy the fixtures style of the existing Modrinth source tests).

**Interfaces:**
- Produces:
  - `InviteModrinth { project_id: String, version_id: String }` (serde camelCase).
  - `InviteView.modrinth: Option<InviteModrinth>`. An id that fails `sources::valid_modrinth_id` reads as `None`: deserialize, then sanitize.
  - `PublicServer.modrinth`, the same field.
  - `joinable_game` keeps its signature. Callers pass `has_pack || modrinth.is_some()`.
  - In `perland.rs`:
    - `resolve_inner`, with `pack: None` and `modrinth: Some`, calls `modrinth::resolve(src, &project_id, &version_id)` and keeps the server-name and about behaviour of the pack path;
    - `stamp_of` returns `Some(format!("modrinth:{version_id}"))` in that case;
    - the resolved state's stamp is rewritten to the same string.

- [ ] **Step 1: Write the failing tests.**
  - `view_reads_modrinth` and `view_drops_invalid_modrinth`.
  - `public_server_reads_modrinth`.
  - `modded_with_modrinth_is_joinable`; a modded server without either stays not joinable.
  - `stamp_of_modrinth`.
  - `modrinth_stamp_matches_after_resolve`: resolve gives a stamp equal to `stamp_of`, and check reports no update for the same version.
  - `modrinth_version_change_is_an_update`.
  - The existing pack and vanilla tests are unchanged.
- [ ] **Step 2: Run the targeted tests.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p launcher-core`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): join servers that run a Modrinth modpack (#49)`.

### Task 3: Free-server commands and Your servers changes (launcher-app + src-tauri)

**Files:**
- Create: `crates/launcher-app/src/free_servers.rs`.
- Modify:
  - `crates/launcher-app/src/perland.rs`: `PlayerApi` gains `free_server`, `create_free_server`, `start_free_server` and `delete_free_server`, taking `engine, token`; `CorePlayer` implements them;
  - `crates/launcher-app/src/testing.rs`: `FakePlayer` with recorded calls and a `hold()` route;
  - `crates/launcher-app/src/your_servers.rs`;
  - `crates/launcher-app/src/dto.rs`, `lib.rs`, `tests/contract.rs`;
  - `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`.
- Test: `#[cfg(test)]` in `free_servers.rs` and `your_servers.rs`.

**Interfaces:**
- Consumes:
  - Task 1's DTOs and the client methods.
  - From #48: `AppCore::perland_session()`, `drop_session(token)`, `PlayerApi`, `public_servers::card(.., keep_offline)` and `instance_of`.
  - Task 2's `InviteModrinth`.
- Produces:
  - DTOs:
    - `FreeServerPage { limits: FreeLimits, create: CreateAllowance, server: Option<FreeServerCard> }`;
    - `FreeServerCard { card: PublicServerCard (flatten), kind, memory_mb, max_players, free_status: String /* the raw view status */, deletion: Option<Deletion>, disk: Disk, invite_url: Option<String>, join_url: Option<String> }`.
  - Building the card from the view:
    - the token comes from `invite_url` through `InviteLink::parse(..).token()`, or `""` without one;
    - `status` comes from the view through `ServerStatus::parse`, with `offline` kept;
    - `joinable` is true when there is an invite and the status is not `creating`, `deleting` or `stopped`, and `disk.over` is false;
    - `instance` is filled as other cards fill it.
  - `AppCore` methods:
    - `free_server() -> Result<Option<FreeServerPage>>`: `None` when signed out or off. On a 401 it calls `drop_session` and returns `None`.
    - `free_server_create(name, kind, modrinth_version_id: Option<String>, language) -> Result<FreeServerCard>`: it checks `name` against `^[A-Za-z0-9_-]{1,32}$` before any call (`InvalidFreeServer`), and `kind` must be `vanilla` or `modrinth`, with `modrinth` requiring a valid version id.
    - `free_server_start() -> Result<FreeServerCard>`.
    - `free_server_delete() -> Result<bool>`.
    - `free_server_play() -> Result<String>`, which returns the instance id. It refuses with `FreeServerOverDisk` or `InviteUnsupported` (existing code) for `creating`, `deleting`, `stopped`, `disk.over` or a missing invite. For `offline` it calls start first. Then it runs `join_add(invite)`, then sets that instance's `server.last_status` to `ServerStatus::Starting` when the view's status is `starting` or the server was just started. Use the existing instance-store write path; find how `server_status` or `servers.rs` updates `last_status`.
  - `your_servers()` changes:
    - it leaves out the entry with `relation: owner` whose canonical invite equals the free server's `invite_url`, or, without one, whose name equals the free server's name. This costs one extra `free_server` call, made in parallel, and errors are ignored;
    - a card with `instance` set is `joinable: true`.
  - Commands: `free_server`, `free_server_create {name, kind, modrinthVersionId, language}`, `free_server_start`, `free_server_delete` and `free_server_play`.

- [ ] **Step 1: Write the failing tests:**
  - `free_server_none_when_signed_out`, `free_server_none_when_off`;
  - `free_server_401_forgets`;
  - `create_refuses_a_bad_name_without_a_call`, `create_modrinth_needs_a_version`;
  - `create_sends_language`;
  - `card_built_from_view` (token, status, joinable rules);
  - `creating_is_not_joinable`, `stopped_is_not_joinable`;
  - `play_offline_starts_and_primes_starting`: the start was called, `join_add` happened, and the instance's `last_status` is `Starting`;
  - `play_sleeping_does_not_start`;
  - `play_stopped_refuses`;
  - `your_servers_hides_the_free_owner_entry`, `your_servers_hides_by_name_without_invite`;
  - `card_with_instance_is_joinable`;
  - `calls_use_the_session_engine`.
- [ ] **Step 2: Add contract fixtures** `free_server_page`: signed-in with a server and allowed without a server.
- [ ] **Step 3: Run** `cargo test -p launcher-app free_server your_servers`. Expected: FAIL.
- [ ] **Step 4: Implement**, register the commands in src-tauri, and regenerate the contract.
- [ ] **Step 5: Run** `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and the src-tauri build. Expected: PASS.
- [ ] **Step 6: Commit** `feat(app): free-server commands (#49)`.

### Task 4: UI data layer and mocks

**Files:**
- Modify: `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/state/queries.ts`, `ui/src/ipc/mock.ts`, `ui/src/main.tsx` (`?free=`), `ui/src/ipc/contract.test.ts`.
- Create: `ui/src/ipc/mockFree.ts`, `ui/src/ipc/mockFree.test.ts`.

**Interfaces:**
- Produces:
  - zod: `freeServerPage`, `freeServerCard` (`publicServerCard.extend({...})`), `freeLimits` and `createAllowance`.
  - Commands: `freeServer()`, `freeServerCreate(name, kind, modrinthVersionId, language)`, `freeServerStart()`, `freeServerDelete()` and `freeServerPlay()`.
  - Hooks:
    - `useFreeServer(enabled)` (`qk.freeServer`), whose `refetchInterval` follows the rule in Global Constraints;
    - `useFreeServerCreate`, `useFreeServerDelete` and `useFreeServerPlay` mutations, which invalidate `qk.freeServer` and `qk.yourServers`.
  - `MockOptions.free?: 'off'|'none'|'has'|'warned'|'overDisk'|'emailRequired'|'rateLimited'|'creating'|'full'|'exists'`, defaulting to `none`. It only applies while `perland` is signed in.
  - Mock behaviour:
    - create waits 2 s, then answers `starting`;
    - a modpack instance whose `projectId` ends in `heavy` answers `modpack_too_heavy` (8192 of 6144);
    - `has` adds the server, sleeping, with an invite;
    - the mock instances include one `source.kind === 'modrinth'` instance and one `…heavy` instance.

- [ ] **Step 1: Write the failing tests:**
  - the contract test parses `free_server_page.json`;
  - mockFree covers each scenario's page, the create delay, too heavy, and delete clearing the server.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `pnpm --filter ui test`, `pnpm -r typecheck` and `pnpm -r lint`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(ui): free-server data layer and mocks (#49)`.

### Task 5: The entry and the three-step dialog (UI, through /impeccable)

**Files:**
- Create: `ui/src/features/free-server/FreeServerEntry.tsx`, `CreateServerDialog.tsx`, `freeServer.ts` and tests next to them.
- Modify: `ui/src/i18n/en.json`, `ui/src/i18n/es.json`.

**Interfaces:**
- Consumes: Task 4's hooks and commands, `usePerlandStatus` (#48) for `displayName` (check what `PerlandAccount` exposes; if it only has `name`, which falls back to the email, add a `displayName` field in Task 3 or use `discord ? name : ''`; decide and state it), and `useInstances` (the existing instances hook, for modrinth instances).
- Produces:
  - In `freeServer.ts`:
    - `prefillName(displayName: string): string` (spec §5);
    - `limitsText(t, limits)`;
    - `retryText(t, retryAfterSeconds)`, which gives "a create is running" at ≤ 5 s, otherwise minutes, hours or days, rounded up;
    - `reasonLine(t, create, hasServer)`.
  - `<FreeServerEntry/>`: the "Create a server for friends" button, or the reason line.
  - `<CreateServerDialog/>`:
    - Step 1: vanilla or a modpack, with the player's `source.kind === 'modrinth'` instances, or the "Install a modpack from Browse first" link to Browse. The name field is prefilled and validated, and the limits text sits above.
    - Step 2: "Creating your server…"; the lost-answer polling rule (spec §5).
    - Step 3: the invite link with Copy link, and Play now, which uses `free_server_play`, then `commands.play(id, true)`, then navigates home, like `usePlay`. Without `inviteUrl`, the address to copy is shown instead.
    - Errors stay on step 1 inline: too heavy with GB, unsupported by `reason`, invalid name, full, and the others by `errors.<code>.message`.
- Strings: `freeServer.*` in both languages. Every limit number comes from `limits`.

- [ ] **Step 1: Run `/impeccable`** for the entry, the dialog and its three steps, against `DESIGN.md` and the surface. Reuse the app's Dialog, Field and Button.
- [ ] **Step 2: Write the failing tests:**
  - `prefill_never_uses_the_email`, `prefill_sanitizes`;
  - `retry_text_running_and_days`;
  - `entry_reasons` (each reason line);
  - `vanilla_create_reaches_share`;
  - `modpack_lists_modrinth_instances`, `no_modpack_instances_points_to_browse`;
  - `too_heavy_stays_on_step_one_with_gb`;
  - `lost_answer_polls_until_the_server_appears` (fake timers);
  - `copy_link`;
  - `play_now_calls_free_server_play_then_play`;
  - `spanish_strings_exist`.
- [ ] **Step 3: Run them.** Expected: FAIL.
- [ ] **Step 4: Implement**, and add the strings.
- [ ] **Step 5: Run** `pnpm --filter ui test`, lint, typecheck and `pnpm --filter ui build`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(ui): create a server for friends (#49)`.

### Task 6: The free-server card in Your servers (UI, through /impeccable)

**Files:**
- Modify:
  - `ui/src/features/servers-list/PublicServerCard.tsx`, adding the optional props `notes?: string[]`, `menuItems?: {label, onSelect}[]`, `statusWord?: string` and `onPlay?: () => Promise<void>`;
  - `ui/src/features/servers-list/YourServers.tsx`: the free card first, `FreeServerEntry`, and the heading shown when the entry shows;
  - `en.json`, `es.json`.
- Create: `ui/src/features/free-server/FreeServerCard.tsx` and `DeleteServerDialog.tsx`, with tests.

**Interfaces:**
- Consumes: Task 4 and Task 5 (`FreeServerEntry`).
- Behaviour:
  - Notes:
    - "Deleted on {date} unless someone plays." while `deletion.warned`, with the date in the UI's locale;
    - "It's out of space and stays off." while `disk.over` or `stopped`.
  - Status words: "Being created…" and "Being deleted…" for those statuses, with Play disabled.
  - Menu: **Delete server**, which opens the confirmation (spec §5 wording), then `free_server_delete`.
  - Play: `free_server_play`, then `commands.play(id, true)`, then home. A `free_server_over_disk` is a toast.
  - The tag is *Owner*.
- Strings: `freeServer.card.*` and `freeServer.delete.*`.

- [ ] **Step 1: Run `/impeccable`** for the notes, the status words and the delete confirmation. Reuse the menu and Dialog patterns.
- [ ] **Step 2: Write the failing tests:**
  - `free_card_first_with_owner_tag`;
  - `warned_note`, `over_disk_note_and_play_disabled`;
  - `creating_status_word_play_disabled`;
  - `delete_confirms_then_deletes`;
  - `play_uses_free_server_play`;
  - `entry_shows_when_no_server`;
  - `nothing_when_free_servers_off`;
  - `public_card_unchanged_without_new_props` (a snapshot or a role check, so the public list is unchanged).
- [ ] **Step 3: Run them.** Expected: FAIL.
- [ ] **Step 4: Implement.**
- [ ] **Step 5: Run** `pnpm --filter ui test`, lint, typecheck and build. Expected: PASS.
- [ ] **Step 6: Commit** `feat(ui): the free server in Your servers (#49)`.

### Task 7: Whole-branch verification

- [ ] Run every CLAUDE.md "Before committing" check, in order: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck` and `pnpm -r test`. All must pass.
- [ ] Check by hand in `pnpm --filter ui dev:mock` with `?perland=signedIn&free=none`, `has`, `warned`, `overDisk` and `emailRequired`, and in the create flow. Take screenshots for the PR.
