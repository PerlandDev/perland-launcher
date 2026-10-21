# Perland Launcher: Manage your server, design

Status: agreed with the owner on 2026-10-21. Pairs with the engine's
`docs/superpowers/specs/2026-10-21-one-perland-account-design.md` (one kind of Perland
account), which this builds on. Builds on Sign in with Perland and Your servers
(`2026-10-17-perland-sign-in-your-servers-design.md`) and Create a server for friends
(`2026-10-17-free-server-for-friends-design.md`).

## 1. Why

A player who owns a server (their free one, or a team's) can see it in the launcher but cannot
run it there: "Manage in Perland" opens the panel in the browser, which signs them in again and
shows an operator's tool. The owner wants running your server from the launcher to be extremely
seamless, for players who are not technical: start, stop, console, settings, players, "all of
it, but in a more intuitive, user friendly way". Not a lot of operations: the panel keeps the
rest.

## 2. Outcome

- A server the signed-in person can run (they are an owner or member of its team) has **Manage**
  on its card. It opens a Manage screen for that server.
- The Manage screen:
  - **Top:** the server's name and icon, the state in words ("Running · 3 playing", "Asleep ·
    wakes when someone joins", "Stopped", "Starting…", "Crashed"), one big button (Start, Stop
    or Restart as the state allows; Start wakes a sleeping server) and the address with Copy.
    When the last run failed, the cause from the engine's diagnosis in plain words, and **Start
    again**.
  - **Players:** who is online (Kick), and three short lists with add and remove: Whitelist,
    Admins (operators), Bans. Plain labels ("Make admin", "Remove admin", "Kick", "Ban",
    "Unban"), never command names. Whether the whitelist is on is a setting (below); the Players
    section links to it.
  - **Console:** the live server log and a box to type a command. `stop`, `restart` and the
    like ask first (the engine's `confirm`). No filters, search or tooling.
  - **Settings:** only server-list name (MOTD), game mode, difficulty, max players, PvP,
    whitelist on or off (`enableWhitelist`) and the Minecraft version (hidden for a server made
    from a modpack, whose pack decides it; the engine refuses it there with 409
    `managed_by_source`), saved with the engine's restart choice worded simply ("Restart now to
    apply" / "Apply at next restart"), and the engine's world-safety confirmations when the
    version changes. A field the plan locks shows locked with Upgrade.
  - **More in Perland:** opens the panel already signed in (the engine's handoff), at the
    server's page. Upgrade opens the panel's upgrade page the same way.
- Viewers and invited or following players keep today's join card.
- Signed-in sessions move from the player-only routes to a normal Perland session, without the
  player signing in again.
- **Sign in with GitHub** joins Discord and the email code on the sign-in card: it opens
  `<panel>/launcher/sign-in?redirect_uri=&code_challenge=&code_challenge_method=S256&state=` in
  the system browser with the same loopback listener and PKCE as Discord, and redeems the code
  at the same `POST /api/v1/player/token`. Shown only when the launcher knows the panel's URL.

## 3. Goals, non-goals, anti-goals

Goals: the outcome above; no new sign-in for existing players; works for the free server and
for a team's servers; every refusal worded for a non-technical player.

Non-goals: files, backups, plugins, mods, Git, networks, scheduled restarts, resize, icon
upload, MOTD colours (the MOTD is edited as plain text; a MOTD holding `§` codes is shown read
only with "Edit in Perland"); server create beyond today's free-server create; managing a
network or its backends.

Anti-goals: a token in a log or an error; a destructive action (stop, restart, ban, a version
change) without a clear confirmation; showing an operator's jargon (stack, compose, RCON, scope)
to a player; polling the engine faster than it asks (the stack detail every 5 s while a state
changes, 30 s otherwise, and only while the screen is open).

## 4. Facts this rests on

- Device session `plt_…` in the keyring (`launcher-core/src/perland/session.rs`), used today on
  `/api/v1/player/*` only (`perland/api.rs`).
- Engine, after the one-account work: `POST /api/v1/auth/player-session` (Bearer `plt_`) answers
  a session JWT for the user; `GET /api/v1/auth/memberships` lists teams (`plan`, `role`);
  `POST /api/v1/auth/select-tenant {tenantId}` re-issues a JWT for that team with `role` and
  `scp`. JWTs last 12 h. `POST /api/v1/auth/handoff` gives a one-time code for the panel's
  `/login/handoff?code=&next=`; the handoff takes `{tenantId}` so the panel opens the server's
  team. Your servers' owner entries carry `manage: {tenantId, stack}`.
- Server routes (JWT in `X-User-Token`, existing): `GET /api/v1/stacks/get-all`, `GET
  /stacks/details/{name}?stats=false`, `POST /stacks/{start|stop|restart}/{name}` (stop and
  restart take `{broadcast?}`), `GET /stacks/{name}/diagnosis`, `GET /stacks/{name}/logs?tail=`
  and `/logs/stream` (SSE: `event: log` with `{stream, timestamp, text}`, `: ping` every 15 s,
  `event: end`), `POST /stacks/{name}/console {command, confirm}`, `GET /stacks/{name}/players`,
  `POST …/players/kick`, `GET/POST …/whitelist` (+`/remove`), `…/ops` (+`/remove`), `…/bans`
  (+`/pardon`) with `{names}` and per-name statuses, `GET/PUT /stacks/{name}/settings`
  (`revision`, `customization.{motd, mode, difficulty, maxPlayers, pvp, enableWhitelist}`,
  `version`; 409 `settings_changed`, `version_change_requires_confirmation`,
  `data_loss_requires_acknowledgement`; `?apply=next-restart`). Free servers answer 402
  `plan_feature_locked` with `data.field` for what the plan locks, and 409
  `free_server_over_disk` on start.

## 5. Design

### 5.1 launcher-core

- `perland/session_jwt.rs`: `JwtCache` keyed by tenant ID. `jwt_for(tenant)`: a cached token
  more than 5 minutes from `exp` (decode the payload's `exp` without verifying), else
  `player-session` → `select-tenant`. One refresh in flight per tenant. A 401 on a JWT call
  drops the cached token and retries once; a 401 from `player-session` is signed out (as today).
- `perland/stacks.rs`: `StackClient` over the routes above, with the session's engine, sending
  `X-User-Token`. Body caps as `PlayerClient`'s. Errors map through `refusal()` plus new codes
  (`server_not_running`, `rcon_unavailable`, `too_many_commands`, `rcon_timeout`,
  `lifecycle_command_requires_confirmation`, `quota_exceeded`, `host_full`,
  `plan_feature_locked`, `settings_changed`, the two version codes, `mojang_unavailable`,
  `free_server_over_disk`, `too_many_streams`, `too_many_diagnoses`) to new `ErrorCode`s, each
  with `errors.<code>.message` in `en.json` and `es.json`.
- `perland/log_stream.rs`: SSE reader over `reqwest`'s byte stream: lines, `event:` / `data:`,
  ignore comments, stop on `end`, reconnect after 5 s with `since` = the last line's time, give
  up after the screen closes. Tested with wiremock.

### 5.2 launcher-app

- `manage.rs`: `managed_servers()` (for every membership with role owner or member:
  `get-all`, mapped to `ManagedServer {tenantId, teamName, stack, name, status, address,
  iconUrl?, players?}`), `manage_detail`, `manage_action(start|stop|restart)`,
  `manage_diagnosis`, `manage_players` and the list changes, `manage_settings_get/put`,
  `manage_console(command, confirm)`, `manage_console_open/close` (starts and stops the log
  stream, emitting `AppEvent::ServerLog {tenantId, stack, lines}` batched every 100 ms, at most
  200 lines), `manage_open_panel(tenantId, next)` (handoff for that team, then the system browser). Each through a
  `StackApi` trait with a `FakeStack` for tests, like `PlayerApi`.
- Your servers dedupes an owner card against a managed server by `manage {tenantId, stack}`.
- DTOs in `dto.rs`, contract samples, `UPDATE_CONTRACT=1 cargo test -p launcher-app --test
  contract`, commands registered in `src-tauri`.

### 5.3 UI (through `/impeccable`)

- Your servers: managed servers first (their own small section heading "Your servers" stays),
  each card with Manage as the primary action and Play beside it.
- A `manage` route (`state/route.ts`) with the screen of §2: one column, sections as tabs
  (Overview, Players, Console, Settings) on a narrow window and stacked on a wide one is the
  builder's call through `/impeccable`. Every string in `en.json` and `es.json`.
- Polling: the detail every 5 s while starting or stopping, 30 s otherwise, only while open.

## 6. Testing

TDD: wiremock tests for every `StackClient` call and the SSE reader; app unit tests with
`FakeStack`; Vitest + Testing Library for the screen and its states (running, asleep, stopped,
crashed with a cause, locked field, refusal words); the mock backend gains `?manage=` scenarios.

## 7. Left to the builder

Layout and visual details (through `/impeccable`), the exact mock scenarios, the batching
numbers within the limits above.
