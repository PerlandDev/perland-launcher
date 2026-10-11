# Perland Launcher: Join by Link (Sub-project 2), Design

- **Date:** 2026-10-11
- **Status:** Draft, awaiting owner review
- **Parent spec:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (approved). §1.1 names this sub-project; §2.1 says it adds a `PerlandServer` source that returns the same `DesiredState` as the other sources.
- **Builds on:** `docs/superpowers/specs/2026-10-10-pack-sync-design.md`, especially §4 (Plan 3b decisions). 3b is in flight in another session; this spec only plugs into what §4 defines and never redesigns it.
- **UI spec and design system:** `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md`, `DESIGN.md`, `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Repos touched:** this launcher, `PerlandDev/PerlandEngine` (API and the invite site in `invite/`), and `PerlandDev/PerlandPanel`.

## 1. Goal

A player clicks **Play with Perland Launcher** on a server's invite page, or pastes the invite link into the launcher. The launcher then creates a server instance that runs exactly the modpack commit the server has deployed. Play installs or updates it and drops the player straight into that server.

Success means:
- the player goes from link to in-game with no Modrinth App, no Prism and no address copying;
- the instance stays in step with the server every time the owner redeploys.

### 1.1 Owner decisions (2026-10-11)

1. **Integration goes through Engine invites, not a per-repo `launcher.toml`.** The invite's `.mrpack` is built from the server's pinned commit, which is exactly what it runs. A packwiz URL follows the repo's latest commit, which can be ahead of the deployment. `launcher.toml` stays for third-party packwiz packs.
2. **Scope:**
   - Modded stacks get the invite's `.mrpack`.
   - Stacks without a modpack get a vanilla instance at the server's game version.
   - Networks (proxy plus backends) are refused with a clear message until the Engine exposes a version or pack for them.
3. **Link format:**
   - The invite URL itself is the key: `https://<invite host>/<43-char token>`.
   - The launcher accepts it pasted, or through `perland://join?invite=<url-encoded invite URL>`.
   - It finds the Engine API the same way the invite page does: `<invite origin>/config.json` → `engineUrl`.
4. **Update check:** the Engine adds `pack.commit` to the invite view. One view request then gives status, players and the update stamp.
5. **Policy:** server instances are always automatic. The 3b "Automatic / Ask me / Don't update" control is hidden for them.
6. **Dead link** (the invite was disabled or regenerated):
   - The instance stays playable at its installed version and still joins the last known address.
   - A quiet plate offers **Use a new link**, which rebinds the instance.
7. **Entry points:** the invite page and the Panel's invite sheet. Both are hidden behind an Engine flag (`LAUNCHER_DOWNLOAD_URL`) until a public launcher release can actually add an account (Plan 4).
   - **Phones (decided while planning):** a phone can't open `perland://`. On the invite page, the launcher tab is the default-selected tab only on desktop. On phones it is still listed but not the default, and it says to open the page on a computer instead of showing the button.
8. **Live check:** the Asalith server is hosted on Perland with an invite turned on. Its invite is used for the owner smoke test. The token never goes into docs, PRs or logs.

### 1.2 Facts this design rests on (Engine `origin/main` d9fb177)

**Public routes**
- They are unauthenticated; the token in the path is the credential:
  - `GET /api/v1/invites/{token}`
  - `GET /api/v1/invites/{token}/icon` (404 when the server has no icon)
  - `GET|HEAD /api/v1/invites/{token}/pack`
  - `GET|HEAD /api/v1/invites/{token}/pack/zip`
  - `GET /api/v1/invites/{token}/mods`
  - `GET /api/v1/invites/{token}/preview`
- They answer in the standard envelope `{success, data, error, meta}`.

**Token**
- Format: 43 characters of base64url (32 random bytes).
- Lifecycle:
  - **Disable:** every route returns 404. Re-enabling keeps the same token.
  - **Regenerate:** the old token returns 404 at once (up to 15 s on other Engine processes for the view).
  - **Expiry:** none.
  - **Unknown, disabled and regenerated** tokens all return the same 404.
- The view exposes no stable server ID, only `kind` and `name`.

**`/pack`**
- It serves the `.mrpack` built from the pinned commit.
- Headers: `ETag: "<commit>"`, `Cache-Control: no-cache`, Range and HEAD support.
- **While the pack is building, a GET blocks for up to 2 minutes.** A failed build is remembered for 1 minute and returns 502, 413 or 400 with a `data.code`.
- Server-only mods stay in the index with `env.client = "unsupported"`, or are placed under `server-overrides/`. The launcher already skips both.

**View fields**
- `pack`: `{name, version, minecraft, loader, loaderVersion, modCount, state: ready|pending|unavailable, zipState}`, or `null` for networks and for stacks without a modpack.
- `status`: `online | starting | sleeping | offline`.
- `players`: present only when online (`max` is absent for networks).
- `address`: `{host, port, display}`.
  - `host` is `PUBLIC_GAME_HOST` and may be absent; `port` may be 0.
  - There is no SRV record.
  - Connecting to a sleeping server wakes it.

**Caching and limits**
- The view is cached for 15 s (`Cache-Control: max-age=15`).
- Rate limits are fixed one-minute windows per client IP: view 60/min, `/pack` plus `/pack/zip` 30/min, `/mods` 60/min.
- A 429 has **no `Retry-After`**.

**Networks**
- `pack: null`, `game: {type: "velocity"|...}` with no Minecraft version.
- `/pack` returns 404.

**Whitelist:** there is no public request-to-join; the page only says "ask the owner".

**Where the Engine lives**
- Invite URLs are `<INVITE_BASE_URL>/<token>`, in practice `https://play.<domain>/<token>`.
- The Engine is at `api.<domain>` by deployment convention only.
- The invite site learns `engineUrl` from its own `/config.json`, written from `ENGINE_URL` when the container starts.

## 2. Architecture

The core idea is a new `perland` pack source in launcher-core. Sync, the 3b run phases, the update banner and art all keep working unchanged, because they only ever see a `DesiredState`.

Joining a server is generic and not tied to Perland: `LaunchSpec` gains an optional server address.

### 2.1 launcher-core

**`invite` module (new)**
- **`InviteLink::parse(&str) -> Result<InviteLink>`**
  - Accepts `https://<host>[:port]/<token>[/]`, where the token matches `^[A-Za-z0-9_-]{43}$`.
  - Accepts plain `http` only for loopback hosts.
  - Rejects anything else: extra path segments, a query, a fragment or userinfo.
  - `canonical()` returns the lowercased host with no trailing slash. This is the identity used for dedupe.
  - `Debug` and `Display` print `https://<host>/<invite>`, never the token.
- **`InviteClient`**: one per process, owned by `Sources`. `AppCore` reaches it through its `Sources`, so the cache is shared:
  - `engine_for(&InviteLink) -> Url`
    - Fetches `<origin>/config.json` (≤ 4 KiB) and reads `engineUrl`.
    - `engineUrl` must be https; http is accepted only on loopback.
    - Cached in memory per origin; refreshed after a network error.
  - `view(&InviteLink) -> InviteView`
    - Fetches `GET <engine>/api/v1/invites/<token>` (≤ 64 KiB).
    - Each `(engine, token)` is cached in memory for 15 s, so every caller within that window shares one request.
  - `icon(&InviteLink) -> Option<Vec<u8>>`
    - Fetches `/icon` (≤ 1 MiB). The PNG signature is checked.
  - `pack_url(&InviteLink) -> Url`.
  - `InviteView` is a serde mirror of the Engine's `InviteViewDTO`, including the new `pack.commit` and `launcher` fields. Unknown fields are ignored.
- **Error mapping**, which carries the host only:

| Engine answer | Code |
|---|---|
| 404 on the view, `/pack` or a HEAD to `/pack` | `invite_gone` |
| 404 on `/icon` | no icon (`None`), not an error |
| 429 | `invite_rate_limited` |
| `kind: network`, or `pack: null` with no `game.version` | `invite_unsupported` |
| A `config.json` that is missing, unparsable or not https | `invite_site_unreachable` |
| 5xx | the existing `host_trouble` |
| Offline | the existing `offline` |

**`sources/perland.rs` (new)**
- **`PackInput::Perland { link: InviteLink }`**
  - `PackInput::parse` tries `InviteLink::parse` before its "any URL is packwiz" rule.
  - The two can't collide, because packwiz URLs end in `pack.toml`.
- **`SourceRef::Perland { invite: String, engine: String }`**
  - Persisted in `instance.json` as `{"kind":"perland","invite":…,"engine":…}`.
  - Its `Debug` hides the token.
- **`resolve(link)`**
  1. Read the view.
  2. **Modded stack** (`pack` present):
     - If `pack.state` is `pending`, poll the view every 5 s for up to 3 minutes. The progress label is the new task `server_pack_wait`. If it is still pending after that, return `server_pack_pending`.
     - If `pack.state` is `unavailable`, return `server_pack_unavailable`.
     - Download `/pack` with the existing `Downloader` (Range resume) into a temp file, hash it, `cache.insert_file`, then run `mrpack::read_archive` with `stamp = pack.commit`.
     - If the Engine sends no `commit`, the stamp is the response ETag with the quotes removed.
     - A 502, 413 or 400 from `/pack` becomes `server_pack_unavailable`, with the Engine's `data.code` in the detail.
  3. **Vanilla stack** (`pack: null`, kind `stack`):
     - Return a `DesiredState` with no files, `loader: None`, `minecraft = game.version` and `stamp = "vanilla:<version>"`.
     - When the server moves to a new Minecraft version, sync installs it.
  4. **`about` for both kinds:**
     - `name` = the view name;
     - `description` = the note;
     - `art.icon` = the invite `/icon` URL. It is fetched by the existing `art::fetch`, which never fails and surfaces no error, so no token can leave it;
     - `art.banner` = none;
     - `policy` = `Auto`.
  - `pack_version` = `pack.version`, or the game version for vanilla.
- **`check(source, installed)`**
  - Reads the view, using the cache.
  - If there is a commit, compare it, or `vanilla:<version>`, with the installed stamp.
  - **Fallback for an Engine without `commit`:** `HEAD /pack` with `If-None-Match: "<installed stamp>"`. A 304 means up to date; a 200 with a different ETag means an update is available.
  - Never fails: errors come back as `UpdateCheck::Unavailable(err)`, as with the other sources.
- **`open`:** reads **only the view** and returns the source to persist, plus a `DesiredState` with no files that carries the name, game version, loader, `about` and stamp. It does not download the `.mrpack`.
  - **Why:** resolving at Add would download the pack, and while a pack is building it would block `join_add` for up to 3 minutes with no progress shown.
  - The first Play resolves, because the instance has never synced.
  - The CLI's `pack add` calls `ensure` for Perland sources instead of syncing the state `open` returns.
- **Error wrapping:** every `Downloader` / `MetaClient` error for an invite URL is re-wrapped so URL params become the host. `redact_url` keeps the path, and the token is in the path.

**`instances.rs`**
- `Instance` gains `#[serde(default)] server: Option<ServerLink>`:

```rust
pub struct ServerLink {
    pub address: Option<ServerAddress>, // last known, validated
    pub name: String,                   // the view name at the last read
    pub accent: Option<String>,         // team accent: cyan|blue|violet|magenta|other
    pub whitelisted: bool,
    pub last_status: Option<ServerStatus>, // online|starting|sleeping|offline
    pub last_seen: Option<i64>,            // unix seconds of the last successful view read
    pub gone: bool,                         // the last read returned invite_gone
}
pub struct ServerAddress { pub host: String, pub port: u16 }
```

- **When `ServerLink` is written:** by `join_add`, `server_status` and `server_relink`, and by Play only when no address is known yet.
  - The Play-time update check doesn't write instance state, because core `check` has no store access. The 30 s status poll keeps the link fresh.
  - When a read returns `invite_gone`, only `gone` changes; the last known address stays.
- **The address is validated before it is stored:**
  - The host must be a DNS name or an IP literal, with no leading `-`, no whitespace and at most 253 characters.
  - The port must be 1–65535.
  - If `address.host` is absent, the Engine URL's host is used, as the invite page does.
  - If the port is 0 or the value is invalid, `address` is `None`.

**`launch`**
- `LaunchSpec` (and `play::StartSpec`) gain `join: Option<ServerAddress>`.
- `build_command` picks the argument style from the version JSON:
  - **It declares an `is_quick_play_multiplayer` feature rule (1.20+):** enable that feature in the `RuleContext` and set `${quickPlayMultiplayer}` to `host:port`.
  - **It doesn't:** append `--server <host> --port <port>`. This covers the modern JSONs from 1.13–1.19 and legacy `minecraftArguments`.
  - Loader children (Fabric, Quilt, Forge, NeoForge) inherit vanilla's rules through the existing merge.

**Redaction**
- `play::start` registers the instance's invite token with `Redactor::with_secret`, next to the access token, whenever the source is `Perland`.

### 2.2 launcher-app

**Commands.** Tauri only forwards them.

| Command | Returns | Notes |
|---|---|---|
| `join_preview(link)` | `JoinPreview` | `{ host, name, motd, kind, status, players, game, pack, whitelisted, team, address, icon: Option<data URL>, existing: Option<instance id> }`. Writes nothing to disk. |
| `join_add(link)` | `Instance` | Returns the existing instance if one already has this canonical invite. Otherwise runs `Syncer::add_pack(PackInput::Perland)` and writes `ServerLink`. Nothing installs; the first Play does that (3b §4.1 decision 4). |
| `server_status(id)` | `ServerStatusView` | `{ status, players, whitelisted, packState, gone, address }`. Reads the view through the shared cache, refreshes `ServerLink`, and takes no lock. |
| `server_relink(id, link)` | `Instance` | Rewrites `source` and `ServerLink`, keeps the files, and the next Play syncs. |

**`pack_add`:** given an invite link, it behaves exactly like `join_add`. The UI normally routes invite links to Join server first.

**Play**
- Play uses 3b's run unchanged:

```
idle → checking → queued → syncing → preparing → running → idle
```

  `asking` never happens, because the policy is always `Auto`.
- **When the check fails:**
  - **Offline, 429 or 5xx:** play the installed version with the existing `update_check_failed` warning.
  - **`invite_gone`:** play the installed version, set `ServerLink.gone`, and the dead-link plate shows.
  - **Never synced and the check fails:** the run ends with that error.
- **Joining:**
  - `play` gains an optional `join: bool` argument, default `true`.
  - The runner passes `instance.server.address` as `LaunchSpec.join` when it is `true` and an address is known.
  - **No known address:** the run warns `server_address_unknown` and starts at the title screen.
- **Policy:** `instance_update` keeps `update_policy` at `Auto` for a `Perland` source, silently, with no error. The UI never offers the control.

**Deep link**
- 3b's `deep_link::parse` currently accepts only `perland://pack?url=`. This plan extends it to return a typed link:

```rust
pub enum DeepLink { Pack { url: String }, Join { invite: String } }
```

- `perland://join?invite=<url-encoded>` is accepted only when the decoded value passes `InviteLink::parse` **and is https**. Loopback http is accepted only when pasted, for local development, mirroring 3b's stricter rule for pack links.
- `deep_link_take()` returns the typed `DeepLink`. `AppEvent::DeepLink` stays whatever 3b ships (its plan makes it a unit signal that tells the UI to call `deep_link_take`). This is a small contract change to 3b's command, made in this plan after 3b merges.
- The `pack` behaviour itself is untouched.

**Contract fixtures:** new fixtures for `join_preview`, `server_status`, the `perland` `SourceRef`, `ServerLink` and `DeepLink`, plus an `invite_links.json` table shared by the Rust and TypeScript link checks.

### 2.3 Data flow

1. **Getting the link in**
   - The invite page's button opens `perland://join?invite=…`. The OS hands it to the launcher, or to the running instance through 3b's `single-instance` forwarding. The UI routes to **Join server**.
   - An invite link pasted into 3b's **Pack link** tab is detected by `isInviteLink`, a TypeScript mirror of the Rust rule tested against `invite_links.json`, and routed to the same screen.
   - A link that arrives before first-run setup is done waits, as in 3b.
2. **Confirm.** `join_preview` resolves the Engine, then reads the view and the icon. It downloads nothing else.
   - If `existing` is set, the screen offers **Open** instead of **Add server**.
3. **Add.** `join_add` creates the instance and opens it.
4. **Play.** See §2.2. The progress label shows "Server is preparing the modpack…" while the run polls a pending pack.
5. **Live status.**
   - The UI polls `server_status`:
     - every 30 s for the selected instance while the window is focused;
     - every 60 s for the rail's server instances.
   - The 15 s shared cache means status, 3b's `pack_status` and the Play-time check reuse view reads.
   - After a 429, the poll backs off from 30 s to 60 s, then 120 s, and resets after a success.
6. **Relink.** **Use a new link** opens Join server in relink mode. If the new invite's name differs, it warns ("This link is for *X*, not *Y*") but still allows it.

### 2.4 Errors

There are no new categories. Each code gets `errors.<code>.message` in `en.json` and `es.json`, plus `.fix` where noted.

| Code | Category | Message (en) |
|---|---|---|
| `invite_link_invalid` | Pack | That isn't a Perland invite link. |
| `invite_gone` | Pack | This invite link doesn't work anymore. Ask the owner for a new one. Fix: **Use a new invite link**. |
| `invite_unsupported` | Pack | Networks can't be joined from the launcher yet. Join with the address `{address}`. |
| `invite_site_unreachable` | Network | Couldn't reach the server's Perland site at `{host}`. |
| `invite_rate_limited` | Network | Too many requests. Try again in a minute. |
| `server_pack_unavailable` | Pack | The server's modpack isn't available right now. |
| `server_pack_pending` | Pack | The server is still preparing the modpack. Try again in a minute. |
| `server_address_unknown` | Game | Couldn't get the server address, so the game starts at the title screen. A warning, not a failure. |

### 2.5 Token safety and untrusted input

**Invite tokens are credentials**
- **Storage:** they're stored in `instance.json`, which is local, like the rest of the instance state. The player can see and copy their own invite link (the owner shares it publicly). The risk is logs and bug reports.
- **Never in errors:** no `LauncherError` (message, detail, params) carries a token. Errors name the host only.
- **`Debug`:** `Debug` for `InviteLink`, `SourceRef::Perland`, `JoinPreview` and the commands' requests prints `<invite>`.
- **Redactor:** the Redactor masks the token in game output and the log file (§2.1).

**The deep link can come from any website**
- **Before the click:** only `config.json`, the view and the icon are fetched, all size-capped. Nothing is written to disk before **Add server**.
- **URLs:**
  - The invite URL and `engineUrl` must be https; http is allowed only on loopback.
  - Redirects follow 3b's policy: https→http is refused across the whole chain.
  - The confirm screen names the invite host.
- **The `.mrpack`:** it goes through the existing mrpack limits, safe paths and hash checks, like any other pack.
- **The address:** it is validated before it reaches the game's arguments (§2.1).

### 2.6 UI

All styling goes through `/impeccable` in the Arcade Cabinet world: one lit button, lime for the selection lamp, blue for progress, and status always in words.

- **Join server** (new route `joinServer({ invite, relinkFor? })`)
  - **Server card:**
    - the icon at integer pixel scale, the name, the MOTD in § colours;
    - the status pill with players;
    - the version line, and the pack line (name, version, mod count);
    - "From `<host>`".
  - **Whitelist:** an "Invite only: ask the owner to add *\<active account name\>*" line. `DESIGN.md` reserves amber for "act", so `/impeccable` picks the colour and records the choice.
  - **Team accent:** a small mark only, never a second lit element.
  - **Buttons:** **Add server** (lit) and Cancel. In relink mode: **Use this link**, plus the name-mismatch warning.
  - **Errors:** shown inline by code. `invite_unsupported` shows the address with Copy.
- **Server instance view**
  - **Status pill** in the slot the desktop spec reserved for this, a square LED plus a word:
    - **Online · 3/20**
    - **Starting**
    - **Sleeping · join to wake it**
    - **Offline**
    - **Unknown** (no data yet, or a dead link)
  - **Address line** with Copy.
  - **Whitelist line**, when the server is invite-only.
  - **Dead-link plate:** "This invite link stopped working · **Use a new link**".
  - **3b's banner:** for server instances it reads "Server updated · updates when you play", with no **Update** button.
  - **Play** keeps its label.
- **Instance menu:** Play without joining, Use a new invite link, Copy invite link, Copy server address.
- **Instance settings:** the Updates control is hidden. A **Server** section shows the invite host, Copy link and Use a new link.
- **Rail:** the server icon, through 3b's art. A last-known status mark that can't be confused with the lime "Playing" lamp; the status is also in the aria-label.
- **i18n:** every string in en and es.
- **Mock backend:** online modded, vanilla, pack pending, whitelisted, gone and network scenarios, plus a scripted `server_pack_wait`.

## 3. Engine and Panel changes

### 3.1 Engine (one PR, including the invite site)

1. **`pack.commit`**
   - Add `PackDTO.Commit string `json:"commit,omitempty"``: the pinned modpack commit, the same value as the `/pack` ETag without the quotes.
   - It is read from the pinned commit that `Summary` already resolves.
2. **The launcher flag**
   - A new env var, `LAUNCHER_DOWNLOAD_URL`: an absolute https URL, validated at boot like `INVITE_BASE_URL`. An invalid value stops boot.
   - When it is set, and only for `kind: stack`:
     - the public `InviteViewDTO` gains `launcher: { downloadUrl }`;
     - the owner `InviteDTO` (stacks only) gains `launcher: { joinUrl, downloadUrl }`, with `joinUrl = perland://join?invite=<url-encoded invite url>`, present only when `INVITE_BASE_URL` is set.
   - The Engine owns the `perland://` format, so the Panel never builds it.
3. **Invite site**
   - **New tab:** a **Perland Launcher** tab, first in `invite/src/lib/launchers.ts`, shown only when `view.launcher` is present.
     - **Desktop** (no coarse pointer and no mobile user agent): the tab is selected by default and contains:
       - the primary button **Open in Perland Launcher**, which opens `perland://join?invite=<encodeURIComponent(location.origin + "/" + token)>`;
       - below it, "Don't have it? Download Perland Launcher" (`downloadUrl`).
     - **Phones:** the previous default tab stays selected. The launcher tab shows "Open this page on your computer to play with Perland Launcher." (es: "Abre esta página en tu computadora para jugar con Perland Launcher.") in place of the button, with the download link still visible.
     - The desktop/phone decision is a pure, unit-tested function in `launchers.ts`.
     - The invite surface brief names Copy address as the page's one primary button. `/impeccable` decides how the two primaries coexist and records it in `invite/DESIGN.md`.
   - **No-pack servers:** they get the tab too.
   - **Not installed:** there is no reliable detection, so the download link is always visible.
   - The other tabs (Modrinth App, Prism, the official launcher zip, CurseForge) stay.
   - Copy in en and es; styled through `/impeccable` against the invite surface brief.
4. **Conventions** (Engine CLAUDE.md):
   - swaggo annotations, then `make docs` and `make sdk`;
   - a docs overlay example;
   - a `site/src/data/changelog.yaml` item and a `roadmap.yaml` item;
   - CLAUDE.md notes for the env var and the fields.
   - Tests use `httptest`:
     - commit present / absent;
     - launcher block for stack vs network vs unset env;
     - an invalid env value stops boot.
   - Invite site: `pnpm test`, `pnpm check`, `pnpm build`.

### 3.2 Panel (one PR, after the Engine SDK is regenerated)

- **Invite sheet:** `LinkSlot` and `ViewerLink` in `app/components/invite/invite-dialog.tsx` get an **Open in launcher** secondary `<a href={launcher.joinUrl}>` beside Copy and Open, plus the hint "Players need Perland Launcher · Download". They are shown only when `invite.launcher` is present and the invite is on.
- **Supporting changes:**
  - `toInviteView` and its tests;
  - strings in `app/i18n/dictionaries/{en,es,ca}/invite.ts`;
  - `DESIGN.md` "Invite sheet" updated.
- **Mock engine:** `scripts/mock-engine.mjs` gains `pack.commit`, plus the launcher block behind `MOCK_LAUNCHER_DOWNLOAD_URL`.
- **PR body:** the line `Roadmap: PerlandDev/PerlandEngine#<n>`.

### 3.3 Not changing

- `/pack/zip` and the classic-launcher steps. The launcher never uses the zip.
- The separate player-pack link `/api/v1/packs/{token}`. It isn't an invite and has no status or address.
- Whitelist requests, and networks.

## 4. Testing

TDD throughout, with tiny local inputs. No fuzz, stress or load runs locally.

- **launcher-core unit tests**
  - The `InviteLink::parse` table, which is also exported as `invite_links.json`.
  - Address validation.
  - Join arguments:
    - quick play on the 1.20.1 fixture;
    - `--server/--port` on the 1.12.2 fixture;
    - `--server/--port` on a modern JSON without quick-play rules.
  - Stamps: `commit`, the ETag fallback, `vanilla:<v>`.
  - `Debug` redaction.
- **launcher-core wiremock tests** (`tests/invite.rs`)
  - `config.json` discovery, including a non-https `engineUrl`.
  - View mapping.
  - 404, 429 and 5xx.
  - pending → poll → ready, with the poll interval and limit injectable so the test runs in milliseconds.
  - `unavailable`, and a 502 with `data.code`.
  - The `HEAD /pack` fallback (304 and 200).
  - A vanilla stack, and a vanilla version change.
  - Network kind → unsupported.
  - Size caps.
  - **A token sweep:** every failure path serialized (JSON and `Debug`) must not contain the token.
- **launcher-app tests**
  - `join_add` dedupe, and relink.
  - The shared `server_status` cache and 429 backoff.
  - The runner passes `join`; `join: false` starts at the title screen.
  - A failed check plays the installed version; `invite_gone` sets `gone`.
  - `instance_update` keeps the policy at `Auto`.
  - Deep link `join` parse, and contract fixtures.
- **UI (Vitest + Testing Library)**
  - Join screen states.
  - Every pill state.
  - The dead-link plate.
  - The menu items.
  - Deep link → route.
  - The `isInviteLink` table.
- **Nightly CI fuzz:** targets for `InviteLink::parse` and the `join` deep-link parse.
- **Owner smoke test:** the real Asalith invite: Add, first Play installs and joins, a redeploy triggers an update on the next Play, and a regenerated link shows the dead-link plate and relinks.

## 5. Plans and order

There are three plans, each executed in its own session:

| Plan | Repo | Starts | Contents |
|---|---|---|---|
| **Engine** | PerlandEngine | Now | `pack.commit`, `LAUNCHER_DOWNLOAD_URL` and the `launcher` blocks, the invite-site tab |
| **Panel** | PerlandPanel | After the Engine PR merges and the SDK is regenerated | The **Open in launcher** button, the mock engine, strings, `DESIGN.md` |
| **Launcher** | perland-launcher | **After 3b merges** | Everything in §2 and §4 |

**Why the launcher plan waits for 3b:** its core additions sit next to 3b's (`find_update`, `ensure_locked`, the `sources/mod.rs` dispatch). It also extends 3b's deep link, run phases, Pack link tab, art and banner. Starting earlier would mean conflicts in exactly those files.

The launcher works against an Engine without `pack.commit` (the HEAD fallback) and with or without the `launcher` block, so the plans don't block each other at runtime.

**Going public:** the owner sets `LAUNCHER_DOWNLOAD_URL` only once a public release build can add an account, which needs Plan 4 (Microsoft sign-in).
