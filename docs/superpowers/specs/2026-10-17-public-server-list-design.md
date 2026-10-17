# Perland Launcher: Public Server List, Design

- **Date:** 2026-10-17
- **Issue:** PerlandDev/perland-launcher#50, part of the Perland integration roadmap (#47; snapshot `docs/superpowers/specs/2026-10-16-perland-integration-ideas.md`).
- **Needs (merged):** PerlandDev/PerlandEngine#431 through PerlandDev/PerlandEngine#435: `GET /api/v1/public-servers`. Its contract is `docs/superpowers/specs/2026-10-16-public-server-listing-design.md` in PerlandEngine. This spec builds on that contract and never changes it.
- **Builds on:** `docs/superpowers/specs/2026-10-11-join-by-link-design.md` (invites, server instances, Play and join, `servers.dat`) and the desktop UI spec, `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner brainstormed the idea with Claude on 2026-10-16 and pre-approved this session's recommendations. Every decision the issue does not settle is marked **our call**.

## 1. Why

Today a player reaches a Perland server only through a link someone gave them. Owners can now mark a server *listed* (#431), and the Engine serves those servers in a public list. The launcher should let anyone find those servers and get in with one press of Play, with or without a Perland account. The bar is that a 12-year-old gets it without reading anything.

## 2. Outcome

A **Servers** screen, reached from the rail, lists Perland's public servers as cards. Each card shows the server's icon, name, modpack (or "Vanilla 1.21.1"), players online and a **Play** button. Servers with players come first and sleeping ones after them, marked "Sleeping, Play wakes it". Stopped servers never show. The player can sort (most players, newest, name) and filter (with or without mods, Minecraft version, has room, category, language, region).

**Play** does what a join link does today, and then connects:
- it creates the server instance, or reuses the one that already joins this server;
- it installs and syncs the pack, starts the game and joins the server.

A sleeping server is woken first, and the meter reads "Starting the server…" until it is up.

The screen keeps a slot above the list for **Your servers** (#48, a later session).

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - the list with its order, sorts and filters;
  - Play through the existing join flow, with a real wake for sleeping servers;
  - no Perland account needed;
  - works in `pnpm --filter ui dev:mock`;
  - every string in en and es.
- **Non-goals:**
  - Your servers, following, signing in (#48);
  - news on the card (#51, next);
  - text search;
  - ping measurement (see §6.5);
  - a server detail page;
  - networks (the Engine lists stacks only);
  - more than one Engine;
  - a mod-loader filter (the issue's own non-goal).
- **Anti-goals:**
  - a screen that looks like a settings form;
  - launcher words (instance, loader, sync) on the cards;
  - more than one lit button competing for attention;
  - an invite token in logs, errors or toasts;
  - a Play that silently does nothing on a sleeping server;
  - a list that breaks or blanks the app when the Engine has the feature switched off.

## 4. Facts this rests on (PerlandEngine `main`, d233a31)

**The list endpoint**
- `GET <engine>/api/v1/public-servers` is public: no auth, `Access-Control-Allow-Origin: *`, standard envelope. It answers `404` (plain `not found`) while the operator's `PUBLIC_SERVER_LIST_ENABLED` is off, `429` past 60 requests a minute per IP (no `Retry-After`), and `503` or `500` with fixed messages.
- **Query:**
  - `sort` = `players` (default), `newest` or `name`;
  - filters `modded`, `minecraft`, `hasRoom`, `category`, `language`, `region`;
  - `limit` (1-100, default 50) and `offset`.
  - A bad value is a plain 400.
- **`data`:** `{servers[], total, facets}`.
  - **A server:** `token`, `inviteUrl?`, `joinUrl?`, `name`, `motd`, `iconVersion?`, `address`, `game`, `modded`, `status` (`online | starting | sleeping`; an unknown value counts as sleeping), `players? {online, max}`, `whitelisted`, `pack?` (the invite view's shape), `categories[]`, `language`, `region?`, `listedAt`.
  - **`facets`** describe the whole listed set, not the filtered page: `categories` (the fixed 10 ids), `languages` (`en`, `es`), `minecraftVersions` (newest first) and `regions`.
- **`sort=players`:** most players first; at equal players `online` before `starting` before `sleeping`, then by name. Sleeping servers have 0 players, so they land after every server with players. This is exactly the issue's order, so the launcher sorts nothing itself.
- **Freshness:**
  - one snapshot per Engine, rebuilt about every 30 s;
  - `Cache-Control: public, max-age=30`;
  - a server that is unlisted or regenerated drops out within about a minute.
- **Icons:** `GET <engine>/api/v1/invites/{token}/icon` when `iconVersion` is present. Not rate limited; 404 without an icon.

**Joining and waking**
- **Join details:** `inviteUrl` is `<INVITE_BASE_URL>/<token>`, absent when the Engine lacks `INVITE_BASE_URL`. The launcher's existing join flow (`join_add` → `PackInput::Perland`, then `play(id, join)`) takes exactly that URL.
- **The waker:** a sleeping stack's port is held by the Engine's waker (PerlandEngine `internal/waker`).
  - It wakes the server when it receives a Minecraft handshake with next state 2 (login) followed by a Login Start packet with a player name, and then kicks that connection with the sleep message.
  - On a whitelisted stack, only an allowed name wakes it; any other name is kicked with "not whitelisted" and nothing wakes.
  - A woken stack nobody joins goes back to sleep after 10 minutes.
  - A status ping (next state 1) never wakes it.
- **Engine discovery:** the launcher already finds an Engine from an invite site's `<origin>/config.json` → `engineUrl` (`InviteClient::engine_for`). Perland's invite site is `https://play.perland.online` (the Engine's docs).

## 5. Decisions

| Question | Decision |
|---|---|
| Where the list comes from | The configured **Perland site** (default `https://play.perland.online`), through its `config.json` → `engineUrl`, the way invite links already find their Engine. The launcher never hard-codes an API host, so it follows wherever the site points. **Our call** |
| Overriding the site | The environment variable `PERLAND_SITE_URL` at launch (an https origin, or http on loopback), read once by `src-tauri` and handed to the core. There is no Settings control: players never need it. Developers and a staging build use the variable. **Our call** |
| Where the screen lives | Its own route `servers`, from a **Servers** button in the rail foot, above Browse, which stays for Modrinth content. Not inside Browse (the issue's layout) |
| What a card shows | Icon, name, one line for the game, the players or sleep word, and **Play**. The game line is the modpack's name and version when there is one, else "Vanilla 1.21.1". The category chips, language, MOTD and address are left off the card: the filters cover the first two, and the rest is noise for a 12-year-old. **Our call** (the issue's card fields) |
| Whitelisted servers | The card also shows the existing **Invite only** line (blue info, pixel lock) and keeps Play: an invited player still gets in, and anyone else sees the game's own "not whitelisted" message. **Our call** |
| Order | The Engine's: `sort=players` by default, so the launcher never reorders. Sleeping servers are already after the ones with players |
| Sorts | Most players (default), Newest, Name A–Z, in one Select. The choice and filters last for the session only (like Browse's), not in settings. **Our call** |
| Filters | One row above the list: **With mods** (Any / With mods / Without mods), **Minecraft version** (Any + the facet's versions), **Has room** (a toggle), **Category** (Any + the 10 categories), **Language** (Any + English / Español). **Region** appears only when the facets name two or more regions, which today they never do. Every value is sent to the Engine as its query; the launcher filters nothing itself. **Our call** on the controls |
| Paging | 50 a page, with the next page loaded when the end of the list scrolls into view (Browse's sentinel), up to the Engine's `total`. **Our call** |
| Refreshing | The first page refetches every 30 s while the screen is open (the Engine's own cache time), and the next pages reload with it. Leaving the screen stops it. **Our call** |
| Play | `join_add(invite)` (it returns the instance that already joins this server, compared by canonical invite), then `play(id, join = true)`, then the UI opens that instance's Home, where the meter shows the run. One code path with join links; no new join command. **Our call** |
| Which link Play uses | `inviteUrl` when the Engine sends one, else `<Perland site>/<token>`: the site is the invite host by definition. Rust computes it into the DTO's `invite`, so the UI never builds URLs. **Our call** |
| The sleeping state on a card | Status word "Sleeping, Play wakes it", with the hollow LED. `starting` reads "Starting". Play is the same button in every state |
| Waking | A new run phase **`waking`** ("Starting the server…") **after `preparing`**, just before the game starts. A first install of Minecraft, the loader and Java happens in `preparing` and can outlast the waker's 10 minutes, so waking any earlier could let the server fall asleep again before the game connects. The run releases the install slot before it waits, so other instances' installs and the launcher's own update are never held up (builder check). It runs only when Play joins (`join = true`), in five steps:<br>1. **Ping:** the launcher sends a Minecraft status ping to the server's address. The ping wakes nothing.<br>2. **The waker answers** (its version name is `Sleeping`): the launcher sends one handshake and Login Start with the active account's name (§6.1) and reads the kick that comes back.<br>3. **Woken:** the phase turns `waking`, and the launcher pings every 2 s until the real server answers (a version name that isn't `Sleeping`).<br>4. **Nothing answers but the view says `starting`:** the same wait, without the knock.<br>5. **Anything else:** straight to the game.<br>The Engine's view is never polled for this: it is cached for 15 s and reads `sleeping` while the server is still waking (builder check). **Our call** |
| Reading the waker's kick | The waker's reply says what happened: the fixed `§cYou are not whitelisted on this server.` means not whitelisted, a reply containing `cannot start right now` means the team has no room, and any other reply means woken. With not whitelisted or no room the launcher doesn't wait: the game connects and the player reads the server's own message. **Our call** |
| Waking without joining | A plain Play of a server instance whose last status was `sleeping` (title screen, the server in the Multiplayer list) sends the same ping and knock in the background without waiting, so the server is starting by the time the player picks it. The game opens at once. **Our call** |
| Giving up on a wake | The real server hasn't answered 5 minutes after the knock: the run goes on and the game connects. A failed ping, knock or wait is a `server_wake_failed` warning in the drawer, never an error that ends the run. A Stop during `waking` ends the run as cancelled. **Our call** |
| Modded servers without a modpack | The Engine lists them (`modded: true`, `game.loader` set, `pack: null`), but the launcher can't join them yet (`perland::supported`). Hiding them would make the Engine's `total` and paging wrong, so their card shows with Play disabled. In its place sits the line "Join from Minecraft: play.perland.online:25566" and a Copy button, the address being what `supported`'s own message names. Rust computes `joinable` with the same rule `supported` uses (one shared function), and the game line is the loader and version ("Fabric 1.21.1"). **Our call** (builder check) |
| No account | The list, the cards and the filters need no Perland account. Play needs a launcher account (offline today), as every Play does: without one, the existing `no_account` error with its "Open Accounts" fix |
| Feature off, or an older Engine | Launcher betas ship soon after merge, while the Engine's list ships switched off (owner, relayed 2026-10-17). So a `404` (feature off, or an Engine without the route) and a `503` (the Engine can't serve it) both map to `server_list_closed`. The screen then shows a calm empty state, "The server list isn't open yet. Check back soon.", with no error styling, no Retry and no toast. So does a `config.json` without an `engineUrl`. Only a player-side problem (offline) or a passing one (429, other 5xx) shows the error card with Retry. A test covers each case at the core and in the UI. **Owner's requirement; the mapping is our call** |
| Room for Your servers | The screen's body is a stack of sections: an empty `YourServers` slot (renders nothing) above **Public servers**. #48 fills the slot without moving the list. **Our call** on the structure |

## 6. Design

### 6.1 launcher-core

**`public_list` module (new, `crates/launcher-core/src/public_list.rs`)**

- `PublicListClient`: one per process, owned by `Sources` next to the `InviteClient`, sharing its `reqwest::Client`.
  - `new(http, site: Url)`. `site` is checked like an engine URL (`check_engine_url`: https, or http on loopback; no user info, query or fragment) and reduced to its origin.
  - `engine() -> Result<Url>` reads `<site>/config.json` → `engineUrl`. It reuses the `InviteClient`'s site cache and its rules (a 4 KiB cap; `engineUrl` https, or http on loopback). To share that, `InviteClient` gains `engine_for_origin(origin: &Url)`, and `engine_for(&InviteLink)` calls it.
  - `page(&ListQuery) -> Result<ListPage>` fetches `GET <engine>/api/v1/public-servers?<query>`:
    - capped at 512 KiB;
    - `ListQuery` = `{ sort, modded: Option<bool>, minecraft, has_room: bool, category, language, region, offset }`, with `limit` fixed at 50;
    - only set filters go into the query string, and `has_room` only when true.
  - `icon(token, icon_version) -> Option<Vec<u8>>` fetches `GET <engine>/api/v1/invites/<token>/icon`:
    - PNG signature checked, at most 1 MiB;
    - in-memory cache keyed by (token, icon version), capped at 256 entries (oldest out);
    - never an error.
  - `invite_for(&PublicServer) -> Option<String>`: `inviteUrl` when it parses as an `InviteLink`, else `<site origin>/<token>` when that parses, else `None`. A server without a usable invite shows its card with Play disabled.
- `ListPage` = `{ servers: Vec<PublicServer>, total: u32, facets: Facets }`, a serde mirror of `publiclistdtos.ListDTO` in which every optional field has a default and unknown fields are ignored.
  - `PublicServer` reuses the invite view's types (`InviteAddress`, `InviteGame`, `InvitePlayers`, `InvitePack`).
  - `status` is parsed with `ServerStatus::parse`; anything else counts as `Sleeping` (the Engine's contract).
  - A `PublicServer` holds a token, so its `Debug` prints only the name.
- **Errors** carry the Engine's host only:

| Answer | Code |
|---|---|
| 404 or 503 on the list | `server_list_closed` (new) |
| 429 | `invite_rate_limited` (existing: "Too many requests. Try again in a minute.") |
| 5xx | `host_trouble` |
| no answer | `offline` |
| a `config.json` that is missing or unusable | `invite_site_unreachable` |
| a body that is too large or invalid | `meta_unexpected` |
| 400 | `meta_unexpected` with detail "the server list refused the filters" (a launcher bug, never a player error) |

**`server_wake` module (new, `crates/launcher-core/src/server_wake.rs`)**

Every connection is TCP to `host:port` with a 5 s connect deadline and a 5 s deadline for the whole exchange.

- **`ping(address) -> Ping`**: `Waker`, `Server` or `NoAnswer`.
  - It sends a Handshake (packet 0x00: protocol 767, the host, the port, next state 1) and a Status Request (0x00), then reads the Status Response (0x00, one string of at most 32 KiB).
  - **`Waker`:** the response's JSON says `version.name == "Sleeping"`.
  - **`Server`:** any other valid response.
  - **`NoAnswer`:** a refused or timed-out connection, or anything unreadable.
  - It never errors, and a ping never wakes a server.
- **`knock(address, name) -> Result<Knock>`**:
  - It sends the Handshake with next state 2 and a Login Start: the name, at most 16 characters (offline names already are), then the 16 zero bytes of the UUID newer protocols carry. The waker reads only the name.
  - It reads the Login Disconnect (0x00, a JSON chat string, at most 32 KiB) and closes.
  - `Knock` is `Woken`, `NotWhitelisted` (the text is exactly `§cYou are not whitelisted on this server.`) or `NoRoom` (the text contains `cannot start right now`).
  - A connection that fails, or no answer, is `server_wake_failed` (new, category network, `host` param).
- **`wait_until_up(address, cancel, every, for)`**: pings until `Server`, and returns `Up`, `TimedOut` or `Cancelled`.
- **Encoding:** VarInt and string encoding live in this module.
- **Unit tests:**
  - the bytes against a hand-built handshake;
  - a fake waker on a local `TcpListener` that parses the packets the way the Engine's `internal/waker` does and answers status and kick;
  - a fake real server;
  - a closed port.

### 6.2 launcher-app

**Commands.** Tauri only forwards them.

| Command | Returns | Notes |
|---|---|---|
| `servers_list(query)` | `PublicServerPage` | `{ servers: PublicServerCard[], total, facets }`. Each card: `{ token, invite: string \| null, joinable: bool, address: string \| null (display form), name, iconVersion, game: {version, loader}, modded, status, players, whitelisted, pack: {name, version, modCount} \| null, categories, language, region, instance: string \| null }`. `instance` is the id of the instance that already joins this server, matched by canonical invite. Nothing is written |
| `servers_icon(token, iconVersion)` | `string \| null` | A `data:image/png;base64,…` URL, or null. The UI fetches it per card through React Query (stale forever, keyed by token and version) |

Both go in `crates/launcher-app/src/public_servers.rs`. The `Runner` trait gains `public_page` and `public_icon` so the app tests use fakes, as `invite_view` does.

**The waking phase (`runs.rs`)**

- `RunStateKind` gains `Waking` (serialized `waking`).
- **Order:** in `drive`, after `prepare` succeeds and the install slot's permit is dropped, and before `start`, a server instance with a known address takes the steps in §5 (Waking):
  - **`join = true`:**
    1. `ping`.
    2. **`Waker`:** `knock` with the session's name.
       - **`Woken`:** set the phase to `Waking` and `wait_until_up` (every 2 s, for 5 minutes).
       - **`NotWhitelisted` or `NoRoom`:** go straight on.
    3. **`NoAnswer`** when the instance's last saved status is `starting`: set `Waking` and wait.
    4. **Otherwise:** go straight on.
  - **`join = false`:** when the last saved status is `sleeping`, spawn `ping` and `knock` on their own task, and start the game without waiting.
- **Stop during `waking`** cancels the wait and ends the run as cancelled (the same `ExitInfo` as a Stop during sync). `set_phase` refuses `Waking` once the run is `stopping`, as it does for `Preparing`.
- **Tests:** `ping`, `knock` and `wait_until_up` go through the `Runner` trait (`server_ping`, `server_knock`, `server_wait`), so the app tests use fakes.
- **Failures:** a failed knock or wait emits `AppEvent::Warning` with `server_wake_failed`, and the run continues.
- **The UI:** the meter's phase label for `waking` is "Starting the server…". The meter shows no blocks (an indeterminate state, like `checking`).

**Contract fixtures:** new fixtures for `servers_list` (`public_server_page.json`), `servers_icon`, and the `waking` run state. They are written by `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract` and never edited by hand.

**Tauri (`src-tauri`):** reads `PERLAND_SITE_URL` once at start-up (default `https://play.perland.online`) and passes it into `Sources`. An unusable value logs a warning and falls back to the default. It registers the two commands.

### 6.3 UI (`ui/`)

All styling goes through the `/impeccable` skill against `DESIGN.md` and the surface file.

**Route and rail**
- `Route` gains `{ name: 'servers' }`.
- The rail foot gets a **Servers** button above Browse, with its icon and, when the rail is wide, the label. It shows `aria-current` on the screen.

**Screen (`ui/src/features/servers-list/`)**
- `ScreenFrame` wide, titled "Servers", with the subtitle "Find a Perland server and press Play."
- **The body stacks:**
  - `<YourServers />` renders `null` now (the #48 slot);
  - a "Public servers" heading;
  - the controls row: the Sort Select and the filters;
  - the card grid.
- **Cards** sit in a grid of 2 columns at the minimum window (960 × 600 with the wide rail) and 3 when there is room. Card content, top to bottom:
  - the icon (64 px server icon, pixel-scaled as on Join server, or the generated `PlotIcon` when there is none or it fails);
  - the name (display face, wraps, never cut);
  - the game line;
  - the status pill:
    - online: "7 / 20 playing", or "7 playing" without a max;
    - sleeping: "Sleeping, Play wakes it";
    - starting: "Starting";
  - **Invite only**, when whitelisted;
  - **Play**.
- **Play on the cards:** one lit button per view is the system's rule, and a grid would otherwise be a wall of amber. Each card's Play is a raised control at rest and becomes the lit amber button when its card is hovered or holds focus. The impeccable build owns the exact treatment. **Our call**
- **Play flow:**
  1. The button shows busy ("Joining…") while it works.
  2. It calls `joinAdd(invite)`, then `play(id, true)`, then `navigate({ name: 'home', id })`.
  3. **A failure:** a toast with the described error, and the card stays.
  4. **An instance of this server already running:** the existing `already_running` error opens that instance's Home instead of a toast.
- **States:**
  - **loading:** a few skeleton cards;
  - **error:** the existing `ErrorCard` with Retry;
  - **`server_list_closed`:** the calm "isn't open yet" state;
  - **an empty filtered result:** "No servers match. Try fewer filters.", with a **Clear filters** ghost button;
  - **an empty list:** "No public servers yet.".
- **Data:** `useInfiniteQuery(['servers', query])` with `refetchInterval: 30_000` on the screen, and `useQuery(['serverIcon', token, version])` per card with `staleTime: Infinity`.

**Mock (`dev:mock`):** `ui/src/ipc/mock.ts` and `mockServers.ts` serve about eight public servers that cover every state:
- online with players;
- full;
- sleeping;
- starting;
- whitelisted;
- vanilla;
- two Minecraft versions;
- both languages.

The mock applies the query the way the Engine does and returns one page. `joinAdd` and `play` from a card create a mock server instance and run it through the mock's run phases, `waking` included for a sleeping server.

**i18n:** every new string goes in `en.json` and `es.json` (neutral Latin American, *tú*), including:
- the 10 category names (`servers.category.<id>`: Survival, SMP, Creative, Modded/Tech → "Mods y tecnología", Magic, Adventure/RPG, Skyblock, Minigames, PvP, Roleplay);
- the language names (English, Español);
- `run.phase.waking`;
- `errors.server_list_closed.message` and `errors.server_wake_failed.message`.

### 6.4 Error codes

| Code | Category | en | es |
|---|---|---|---|
| `server_list_closed` | network | The server list isn't open yet. Check back soon. | La lista de servidores aún no está abierta. Vuelve pronto. |
| `server_wake_failed` | network | Couldn't wake the server. The game will try to join anyway. | No se pudo despertar el servidor. El juego intentará entrar igual. |

### 6.5 What waits

- **Ping:** every listed server runs in one place today, and the Engine leaves ping to the launcher. A ping filter would sort identical numbers, so it waits for the second region. When it comes, the launcher pings each card's address with a status request (never a login: that would wake it) and shows a ping badge plus a "Close to you" sort. The region filter already appears on its own once there are two regions. **Our call**
- Following a server on Play, Your servers and the tags in it (#48); news and the new-post dot (#51); several Engines.

## 7. Testing

- **launcher-core:**
  - wiremock tests in `crates/launcher-core/tests/public_list.rs`:
    - config discovery;
    - the query string per filter;
    - parsing a full page and a minimal one;
    - unknown status → sleeping;
    - 404 → `server_list_closed`, 429, 5xx, oversize and invalid bodies;
    - the icon (PNG check, 404 → None, cache);
    - `invite_for` with and without `inviteUrl`.
  - unit tests in `server_wake.rs`:
    - the packet bytes;
    - a fake waker: ping `Waker`, the knock outcomes;
    - a fake real server: ping `Server`;
    - a refused connection: ping `NoAnswer`, knock `server_wake_failed`;
    - `wait_until_up` up, timeout and cancel.
  - the closed-list mapping: 404, 503 and a `config.json` without `engineUrl` all give `server_list_closed`.
- **launcher-app:**
  - `servers_list` maps cards and finds the existing instance;
  - the run enters `waking` after `preparing`, with the install slot released, for a waker that answers with join;
  - it knocks with the account's name and waits until up;
  - it skips the wait on not whitelisted or no room;
  - it waits without a knock on a `starting` server that doesn't answer;
  - it times out and starts the game;
  - it ends cancelled on Stop;
  - it warns and continues on a failed knock;
  - it only knocks in the background without join;
  - the contract fixtures.
- **UI** (Vitest + Testing Library):
  - the rail button routes;
  - cards render each state;
  - the filters send their query;
  - Region is hidden with one region;
  - Play calls `joinAdd`, then `play(id, true)`, then navigates;
  - an existing instance is reused;
  - the closed state for `server_list_closed` (no error card, no toast) and the empty states;
  - a card that isn't joinable shows the address and a disabled Play;
  - the `waking` phase label.
- **Local test inputs stay tiny.** No fuzzing, stress or load tests run locally (CLAUDE.md).

## 8. Owner to do

- Set `PUBLIC_SERVER_LIST_ENABLED=true` on the production Engine (PerlandEngine#435's own to-do). Until then the screen shows "isn't open yet".
- Check that `https://play.perland.online/config.json` names the production Engine. The launcher finds the list through it.
- List a server (Asalith) from the panel once PerlandDev/PerlandPanel#205 ships, and try Play on a sleeping server.

## 9. Left to the builder

- File and component names inside the stated modules.
- The exact grid breakpoints and skeleton look (through impeccable).
- The exact poll interval inside "every 3 s".
- The test structure.
