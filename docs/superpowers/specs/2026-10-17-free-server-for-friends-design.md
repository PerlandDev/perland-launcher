# Perland Launcher: Create a Server for Friends, Design

- **Date:** 2026-10-17
- **Issue:** PerlandDev/perland-launcher#49, part of the Perland integration roadmap (#47; snapshot `docs/superpowers/specs/2026-10-16-perland-integration-ideas.md`, L2).
- **Needs (merged):**
  - PerlandDev/PerlandEngine#440 (free servers, #430). The contract is `docs/superpowers/specs/2026-10-17-free-servers-design.md` in PerlandEngine ("Launcher contract"), plus the `/api/v1/player/free-server` rows of its CLAUDE.md.
  - PerlandDev/perland-launcher#60 (#48: Sign in with Perland and Your servers).
  - This spec never changes the Engine contract.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-17-perland-sign-in-your-servers-design.md`: `PlayerClient`, the session, `PlayerApi`, Your servers, `PublicServerCard` with tag and menu.
  - `docs/superpowers/specs/2026-10-11-join-by-link-design.md`: `SourceRef::Perland`, `sources/perland.rs`, server instances, wake on join.
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner brainstormed the idea with Claude on 2026-10-16 and pre-approved this session's recommendations. Every decision the issue does not settle is marked **our call**.

## 1. Why

Players want their own small server for friends, and setting up hosting is out of reach for a casual 12-year-old. Perland now gives each signed-in player one free server. The launcher should make it three steps (pick vanilla or a modpack, create, share the link), with the limits said up front in plain words, so nobody is surprised later.

## 2. Outcome

- **Entry point:** a signed-in player whose Engine offers free servers sees **Create a server for friends** in the Your servers section of the Servers screen, until they have one.
- **Step 1, Pick:**
  - The player chooses **Vanilla** (the newest Minecraft) or **A modpack** from their own instances installed from Modrinth.
  - They name the server, prefilled from their name.
  - The limits show up front: "Up to 10 players. 3 GB of memory for vanilla, 6 GB with mods. Sleeps when nobody plays and wakes when someone joins. Deleted after 30 days without players."
- **Step 2, Create:** "Creating your server…".
  - A modpack that needs more memory than a free server has is refused before anything is created: "This modpack needs 8 GB; free servers have 6 GB."
  - Other refusals are one plain line.
- **Step 3, Share:** the invite link with **Copy link** and **Play now**.
- **Your servers:** the new server appears there tagged *Owner*, as its own card built from the free server's state (so it shows while being created too). Its card menu has **Delete server**, with a confirmation.
  - A server close to deletion reads "Deleted on 14 Nov unless someone plays."
  - A server over its disk limit reads "It's out of space and stays off."
- **Play wakes it:**
  - Play on a sleeping server wakes it and shows "Starting the server…" (the existing wake on join).
  - Play on an `offline` (stopped or crashed) one starts it first and waits until it is up.
  - The player never sees a start button.
- **Friends join with the link**, modpack servers included: the launcher installs the server's Modrinth modpack version, then connects. Afterwards their card in Your servers plays that instance.
- **When creating isn't possible:** the entry point says why in one line:
  - "Free servers need a verified email on your Perland account. Add one, then try again." (for an account without a verified email; since one Perland account, the email is added on the panel's account page, which "Add an email" opens signed in while a panel is configured)
  - "Another account with your email already has a free server."
  - "A server is being created right now."
  - "You can create another server in 2 days."
  - "Free servers are full right now. Try again later."
- **Feature off or older Engine:** nothing of this shows.

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - the three-step create flow with the limits in plain words;
  - refusing a pack that is too heavy before creation;
  - the Owner card with deletion and disk warnings;
  - Delete;
  - Play that starts a stopped server;
  - joining a server that runs a Modrinth modpack (owner and friends);
  - all of it in `pnpm --filter ui dev:mock`.
- **Non-goals:**
  - the world download (**our call**: playing resets the 30-day clock, so the warning tells the player to play; a download needs a save flow and polling, so it waits for a later issue);
  - choosing a Minecraft version for vanilla (**our call**: the newest; everyone joins through the link, which creates a matching instance);
  - uploading a `.mrpack` file or using an instance that isn't from Modrinth (the Engine accepts a Modrinth version id only);
  - picking a modpack from Browse inside the flow;
  - server settings, whitelist or console;
  - a stop button.
- **Anti-goals:**
  - a player surprised by a limit after creating;
  - a start or stop button the player has to learn;
  - a half-created server left behind by the launcher (the Engine owns rollback);
  - an error card for a player whose Engine has free servers off.

## 4. Facts this rests on (PerlandEngine `main`, 9810488)

- All routes need `Authorization: Bearer plt_…`.
- **Feature off:** every free-server route answers 503 `free_servers_disabled`, after `player_accounts_disabled`.
- **`GET /api/v1/player/free-server`** answers `{limits, create, server}`:
  - `limits`: `maxPlayers`, `memoryMb.vanilla`, `memoryMb.modded`, `diskBytes`, `sleepMinutes`, `unusedDays` (30), `warningDays` (7), `createsPerWeek` (3).
  - `create`: `{allowed}`, else `reason`, one of `email_required`, `free_server_exists`, `too_many_free_servers` (with `retryAfter` in seconds), `free_servers_full`.
  - `server`: null, or a view with:
    - `name` and `kind` (`vanilla` or `modrinth`);
    - `game {type, version}` and `modrinth?{projectId, versionId}`;
    - `memoryMb` and `maxPlayers`;
    - `status`: `online`, `starting`, `sleeping`, `offline`, `stopped`, `creating` or `deleting`;
    - `players`, `address`, `inviteUrl?` and `joinUrl?`;
    - `createdAt` and `lastPlayedAt`;
    - `deletion?{deletesAt, warned}`;
    - `disk {usedBytes, limitBytes, checkedAt?, over}`.
  - Poll at most every 15 s.
- **`POST /free-server`** takes `{name, kind, version?, modrinthVersionId?, language?}`:
  - `name` must match `^[A-Za-z0-9_-]{1,32}$`;
  - `vanilla`: `version` is optional (LATEST when empty);
  - `modrinth`: `modrinthVersionId` must match `^[A-Za-z0-9]{1,16}$`;
  - `language`: `es`, else `en`.
- **Its answers:**
  - 201 with the view.
  - 429 `too_many_free_servers` (`retryAfter`; also 10 attempts an hour and one at a time).
  - 400 `invalid_free_server` (the message names the field).
  - 403 `email_required`.
  - 409 `free_server_exists`.
  - 400 `modpack_unsupported` (`reason`: `not_found`, `not_modpack`, `loader`, `too_large`).
  - 400 `modpack_too_heavy` (`requiredMemoryMb`, `limitMemoryMb`, `serverMods`).
  - 502 `modrinth_unavailable`.
  - 503 `free_servers_full`.
  - 402 `quota_exceeded` (the operator's plan is misconfigured).

  The create runs server-side and can take tens of seconds.
- **`POST /free-server/start`** starts or wakes the server and answers the view. 404 means no server; 409 `free_server_over_disk` (`usedBytes`, `limitBytes`).
- **`DELETE /free-server`** answers `{deleted}`.
- **Your servers:** the player's free server shows in `GET /servers` as `relation: owner`, with the same invite.
- **Invite view:** for a modpack free server, the public view carries `modrinth: {projectId, versionId}` and `pack: null`; `game` holds the loader and the pack's Minecraft version.
- **`GET /servers` and the public list do not send `modrinth`** (`publiclist.Server` has no such field, PerlandEngine 9810488), so a Your servers card cannot tell a Modrinth-pack server from a modded server without a pack. Only the invite view and the free-server view carry it.
- **Statuses:** `stopped` means over the disk limit only (`POST /start` then answers 409 `free_server_over_disk`); a crashed or stopped-by-hand server reads `offline`. Right after a 201 the server is usually `starting` (a modpack installs on first start, up to 10 minutes).
- **`create.reason` subtleties:** `free_server_exists` can come with `server: null` (another account of the same normalized inbox owns one); `too_many_free_servers` with `retryAfter` of 5 seconds or less means another create of this account is still running. The Engine finishes a create even when the client leaves, so a retry after a lost answer gets `free_server_exists` or that short `too_many_free_servers`.
- **Email-only players have `displayName: ""`**; the launcher's `Profile.name` falls back to the email. An email sign-in never joins an existing Discord-only player (it creates a new player).
- **The run's wake on join** waits only when the ping gets no answer and the instance's stored `last_status` is `Starting` (`runs.rs::wake_for_join`).
- **launcher-core today:**
  - `modrinth::resolve` stamps with the bare version id.
  - `sources/perland.rs` refuses a modded server with `pack: null` (`joinable_game`, `invite_unsupported`).
  - `InviteView` and `PublicServer` have no `modrinth` field.
  - `sources/modrinth.rs::resolve(src, project_id, version_id)` installs a Modrinth modpack version.
  - Instances installed from Browse carry `SourceRef::Modrinth {projectId, versionId}`; `.mrpack` imports do not.
  - Wake on join works for any `SourceRef::Perland` instance and needs no public listing.

## 5. Decisions

| Question | Decision |
|---|---|
| Where the modpack comes from | **The player's own instances installed from Modrinth** (`source.kind === "modrinth"`), the version they have installed. This is the simplest choice that matches the Engine's contract, which takes a Modrinth version id. When they have none, the step says "Install a modpack from Browse first", with a link to Browse (**our call**) |
| Vanilla version | None sent, so the Engine uses the newest Minecraft and reports it as `LATEST`. The launcher reads `LATEST` (any case) as the Mojang manifest's latest release when it joins and checks for updates, and shows it as "(latest)". Right after a Mojang release a player can be one version ahead of a server that has not restarted yet (**our call**) |
| Server name | Prefilled from the Perland account's Discord-based `displayName` only, never from the email (a kid's email must not land on a public invite page): spaces become `-`, characters outside `[A-Za-z0-9_-]` are removed, and it is cut to 32; when nothing is left (email-only players), `my-server`. Editable, checked against the Engine's rule before sending. The name shows to friends on the invite page (**our call**) |
| Language | The UI language (`es`, else `en`): the invite page opens in it and the warning email uses it |
| Where the flow lives | A dialog opened from Your servers: three steps in one dialog, with Back on step 1 only (**our call**: no new route) |
| Limits text | Built from `limits` (players, GB rounded down to whole GB, `unusedDays`), never hard-coded |
| Too heavy | Shown as the Engine says, before anything exists: "This modpack needs {required} GB; free servers have {limit} GB." The player stays on step 1 to pick another one |
| Create timeout | The create request gets 150 s (the Engine creates the stack while the request is open); other calls keep 20 s |
| Lost create answer | When the create request times out or fails on the network, or answers `free_server_exists` / `too_many_free_servers` with `retryAfter` ≤ 5 right after the player pressed Create, the dialog shows "Still creating your server…" and polls `GET /free-server` every 5 s for up to 5 minutes. When a server appears it goes to step 3; when time runs out it says "Your server is still being set up. It will show in Your servers." and closes (**our call**) |
| After create | Step 3 shows `inviteUrl` (Copy link) and **Play now**. Play now joins through the invite like any Play, and the card shows up in Your servers. Without `inviteUrl` (an Engine without `INVITE_BASE_URL`), step 3 shows the address to copy instead |
| Owner card | A **standalone card built from the free-server view** (the app builds a `PublicServerCard` from it: token from `inviteUrl`, `joinable` from the view, `instance` as cards do), first in Your servers. The `/servers` entry with `relation: owner` and the same invite (canonical), or, when the view has no `inviteUrl`, the same name, is left out so the server shows once. Extras: the deletion line while `deletion.warned`, the out-of-space line while `disk.over` or `stopped`, the status words "Being created…" / "Being deleted…" (Play disabled), and the menu item **Delete server** (**our call**: the view exists while creating and without `INVITE_BASE_URL`, when `/servers` has no matching card) |
| Joinable cards | `/servers` lacks `modrinth`, so a Your servers card is joinable when the existing rule passes, **or** the player already has the instance that joins it (`instance` set: Play then just plays it), and the owner's free-server card is joinable from its own view whenever it has an invite. A friend's first join of a modpack free server goes through the invite link, which reads the invite view with `modrinth` (**our call**; an Engine follow-up adding `modrinth` to `publiclist.Server` would make the public rule exact, and the launcher already reads the field when present) |
| Delete | A confirmation dialog ("Delete {name}? The world is deleted too. You can create a new server afterwards."), then `DELETE`. Afterwards the entry point comes back, subject to the weekly limit |
| Play on the owner's free server | One app command, `free_server_play()`, does it all and answers the instance id, then the UI starts the game with join as every Play does. It refuses `creating`, `deleting` and `stopped` / `disk.over` (the out-of-space line already explains; Play is disabled). For `offline` it calls `POST /start` first. It then joins through the invite (`join_add`) and stores the instance's `last_status` from the view (`starting` after a start or a fresh create), so the run's wake waits with "Starting the server…" until the server answers; `sleeping` keeps the existing knock (**our call**: a UI hook alone cannot make the run wait) |
| Owner's existing Modrinth instance | Playing the free server makes a server instance next to the instance the pack came from, like any join; it stays in step with what the server runs (**our call**) |
| Statuses `creating` / `deleting` | The card reads "Being created…" or "Being deleted…" with Play disabled |
| Polling | `GET /free-server` while the Servers screen is open and the player is signed in: every 30 s, and every 15 s while the status is `creating`, `starting` or `deleting` (**our call**: the contract's minimum is 15 s). Both this poll and Your servers' dedupe read the page, so the Engine gets about one extra read a minute. That is accepted (**our call**) |
| Joining a Modrinth-pack server | Keep `SourceRef::Perland`, so the instance stays a server instance with its wake, Multiplayer entry and news. `InviteView` and `PublicServer` gain `modrinth: Option<{projectId, versionId}>`. `joinable_game` counts it like a pack. On sync, `perland.rs` resolves the Modrinth version through `modrinth::resolve`, and the stamp follows the version id, so a version change on the Engine updates players |
| `create.reason` text | `email_required`: "Free servers need a verified email on your Perland account. Add one, then try again." with a raised "Add an email" that opens the panel's `/account` signed in (`perland_open_account`, a handoff with no team), only while a panel is configured; no button without one. `too_many_free_servers`: with `retryAfter` ≤ 5, "A server is being created right now."; otherwise "You can create another server in {time}." (rounded up to minutes, hours or days). `free_servers_full`: "Free servers are full right now. Try again later." `free_server_exists` with no server: "Another account with your email already has a free server." |

## 6. Design

### 6.1 launcher-core

- **`perland/api.rs`: `PlayerClient` gains:**
  - `free_server(token) -> Option<FreeServerState>`. A 503 `free_servers_disabled` is `None`.
  - `create_free_server(token, &CreateFreeServer) -> FreeServer`, with a 150 s timeout.
  - `start_free_server(token) -> FreeServer`.
  - `delete_free_server(token) -> bool`.
- **DTOs** mirror §4:
  - `FreeServerState {limits: FreeLimits, create: CreateAllowance, server: Option<FreeServer>}`;
  - `FreeServer {name, kind, game, modrinth, memory_mb, max_players, status: String, players, address, invite_url, join_url, created_at, last_played_at, deletion: Option<Deletion>, disk: Disk}`;
  - `CreateFreeServer {name, kind, modrinth_version_id: Option<String>, language}`.
- **Error mapping.** New codes are in §6.4.
  - `email_required`, `free_server_exists`, `too_many_free_servers` (`retryAfter`), `free_servers_full`, `invalid_free_server`, `modpack_unsupported` (`reason`), `modpack_too_heavy` (`requiredMemoryMb`, `limitMemoryMb`) and `free_server_over_disk` map to codes of the same names.
  - `modrinth_unavailable` maps to `host_trouble` with the host `modrinth.com`.
  - `quota_exceeded` maps to `free_servers_full`.
  - 401 is `perland_signed_out`, as before.
  - Anything else maps as before.
- **`invite/view.rs`:** `InviteView.modrinth: Option<InviteModrinth {project_id, version_id}>`. Both ids are checked with `valid_modrinth_id`; an invalid pair reads as `None`.
- **`public_list.rs`:** `PublicServer.modrinth`, the same field (read when present; today's Engine does not send it).
- **`sources/perland.rs`:**
  - `joinable_game(kind, game, has_pack)` is called with `has_pack || modrinth.is_some()`;
  - `resolve_inner` with `pack: None` and `modrinth: Some` resolves through `modrinth::resolve(src, project, version)`, keeping the server's name and about the way the pack path does;
  - `stamp_of` uses `modrinth:<versionId>` in that case, and `resolve_inner` rewrites the resolved stamp to the same value (`modrinth::resolve` stamps the bare id), so the update check doesn't report an update forever;
  - `open_inner`'s game fallback stays as it is.

### 6.2 launcher-app

- **`free_servers.rs`, new commands:**
  - `free_server() -> Option<FreeServerPage>`. `None` when signed out, when the feature is off, or after a 401 (forgets the session as in #48). The page is `{limits, create: {allowed, reason, retryAfter}, server: Option<FreeServerCard>}`; `FreeServerCard` carries the view's fields, plus `invite` (canonical) and `instance` (the instance that already joins it, as cards do).
  - `free_server_create(name, kind, modrinthVersionId, language) -> FreeServerCard`. `name` is checked against the rule first: a bad name is `invalid_free_server`, and no call is made.
  - `free_server_start() -> FreeServerCard`.
  - `free_server_play() -> String` (instance id), per §5 "Play on the owner's free server".
  - `your_servers()` leaves out the owner's free-server entry (§5 "Owner card") and marks a card joinable when `instance` is set (§5 "Joinable cards").
  - `free_server_delete() -> bool`.
- All of them use the session's Engine only.
- **Contract fixtures** cover these new types.

### 6.3 UI (through `/impeccable`)

- **`ui/src/features/free-server/`** holds:
  - `CreateServerDialog` (the three steps);
  - `FreeServerEntry`: the entry button, or the reason line, in Your servers;
  - `freeServer.ts`: name prefill, limits text, retry wording.
- **`YourServers`:**
  - shows the standalone free-server card first when `server` is set (Play goes through `free_server_play`, then `play(id, true)`);
  - shows `FreeServerEntry` when `free_server()` is not null and `server` is null;
  - shows the section heading even when the list is empty, if the entry shows;
  - the free-server card passes its extras (warning lines, Delete in the menu, the status words) to `PublicServerCard`.
- **`PublicServerCard`** gains optional `notes` (extra lines), `menuItems` (items beyond Unfollow), `statusWord` and `onPlay` (replaces the default join), so the free-server card stays the same component.
- **State:** `useFreeServer(enabled)` with its polling rule, and mutations for create, start and delete that invalidate `qk.freeServer` and `qk.yourServers`.
- **Mocks:** `?free=` with these scenarios:
  - `off`;
  - `none`, the default, allowed;
  - `has`: a sleeping server, plus its Owner entry in Your servers;
  - `warned`;
  - `overDisk`;
  - `emailRequired`;
  - `rateLimited`;
  - `full`.

  In the mocks, creating takes 2 s. A modpack instance whose project id ends in `heavy` (e.g. `heavy`) answers `modpack_too_heavy`, needing 8192 of 6144 MB. The mock instances include one Modrinth modpack instance.
- **Strings:** every string in en and es, under `freeServer.*`, plus `errors.<code>.message` for each new code.

### 6.4 Error codes

All new codes go in a `// free servers` group:

| Code | Category | Params |
|---|---|---|
| `email_required` | auth | |
| `free_server_exists` | network | |
| `too_many_free_servers` | network | `retryAfter` |
| `free_servers_full` | network | |
| `invalid_free_server` | network | |
| `modpack_unsupported` | pack | `reason` |
| `modpack_too_heavy` | pack | `requiredMemoryMb`, `limitMemoryMb` |
| `free_server_over_disk` | network | `usedBytes`, `limitBytes` |

Each code gets `errors.<code>.message` in en and es. The UI turns MB into GB in the message (`{{required}}`, `{{limit}}`).

## 7. Testing

- **Core wiremock tests:**
  - each free-server route and each error mapping;
  - `free_servers_disabled` reads as `None`;
  - the create body;
  - the bearer header, with no token in the URL.
- **Core unit tests:**
  - `InviteView` and `PublicServer` parse `modrinth` and drop an invalid pair;
  - `joinable_game` accepts a modded server with `modrinth`;
  - `perland` resolve and stamp with `modrinth`, through a wiremock Modrinth (copy the existing Modrinth source tests).
- **App tests** (fake player):
  - `None` when signed out or off;
  - a bad name is refused before any call;
  - a 401 forgets the session;
  - the card's `invite` and `instance`.
- **UI (Vitest):**
  - the entry and each reason line;
  - the dialog's three steps;
  - vanilla and modpack (with no modpack instances, the Browse hint);
  - too heavy keeps the player on step 1 with the GB message;
  - Copy link;
  - Play now;
  - Delete with confirmation;
  - the warning lines;
  - start before Play when offline;
  - nothing when off.
- **Contract test:** the new fixtures parse with the strict schemas.

## 8. Owner to do

- [ ] Turn on free servers on the production Engine (`FREE_SERVERS_ENABLED=true`, plus player accounts, SMTP, the waker image, `PUBLIC_GAME_HOST` and `INVITE_BASE_URL`), as PerlandDev/PerlandEngine#440 lists. Until then the launcher shows none of this.
- [ ] Try one vanilla and one modpack free server end to end on a real Engine. itzg installing a Modrinth pack was not verified on Docker in the Engine PR either.

## 9. Left to the builder

- The exact layout through `/impeccable`, and the wording polish of the strings.
- How the dialog's steps animate, if at all.
