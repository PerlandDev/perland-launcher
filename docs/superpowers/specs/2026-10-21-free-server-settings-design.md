# Free Server Settings and Servers From Any Instance, Design

- **Date:** 2026-10-21
- **Repos:** PerlandEngine (the contract and its checks), perland-launcher (the create form and the instance shortcut), PerlandPanel (the `/free-server` create form).
- **Builds on:**
  - `2026-10-17-free-server-for-friends-design.md` (the free-server flow and its contract).
  - `2026-10-21-play-with-friends-design.md` (the instance → server pack flow, launcher#95).
  - PerlandEngine `main` at 34de86f: `internal/freeservers/create.go`, `internal/playersapi/freeservers*.go`, `cmd/server/freeservers.go` (`freeStackRequest`), `internal/minecraft/controllers/free-server-rules.go` (`checkFreeSettings`), `internal/minecraft/versions` (the Mojang list `GET /versions?type=vanilla` serves), `internal/minecraft/presets` (hardcore as survival + `HARDCORE`, difficulty locked to hard).
- **Status:** approved by the owner's brief; phase 2 (section 6) added when the owner widened the scope ("fully implemented, work on your own"). The owner asked for no questions, so every decision below marked **our call** is ours, with its reason.

## 1. Why

The owner: "Let people choose the Minecraft server version when they create free servers, as well as other basic settings, and one great shortcut would be to create servers from an instance."

Today a vanilla free server is always the newest Minecraft, every other setting is a default, and only modded instances can become a server. The panel kept "latest only" on purpose: a mistyped version makes a server that never starts and still costs one of the week's 3 creations. So the version has to be checked against the real list before anything is reserved; then a picker is safe.

## 2. Goals, non-goals, anti-goals

- **Goals:**
  - the vanilla create takes a version, checked against the Engine's real vanilla list, refused before the creation is reserved;
  - a small set of basic settings at create (section 3.2), checked with the free-server rules, written as the stack's customization so Manage → Settings shows them;
  - the same settings on the pack create;
  - the launcher's create form gets a version picker and the settings behind an Options disclosure; one click still makes today's server;
  - any instance not linked to a server becomes a free server from its ⋯ menu;
  - the panel's create form gets the same picker and settings.
- **Non-goals:** spectator mode, world type / flat worlds, view distance, starting whitelist names, ops, RCON, a world upload, offline mode, resizing. Everything a free server may change later stays in Manage → Settings.
- **Anti-goals:** a picker that can cost a creation on a server that won't start; an option that lets a free server escape its limits (online mode, max players, env); breaking the one-click path; a new launcher or panel showing the picker against an Engine that doesn't check the version.

## 3. Engine

### 3.1 The version check

- `POST /free-server` with `kind` `vanilla` and a `version` other than `LATEST` (empty stays `LATEST`, which needs no list) is checked against the vanilla list the version pickers use (`versions.Catalog.List(ctx, "vanilla", "")`, Mojang's manifest without the releases that have no server jar).
- **Snapshots are accepted** (**our call**). The list, the stack create and itzg all run them, and a launcher instance on a snapshot should be able to become a server. The clients show releases first and snapshots behind the existing "Show snapshots" toggle. Old alphas and betas are not in the list, so they are refused.
- The match is exact (case-sensitive), as Mojang's ids are.
- An unknown version is 400 `invalid_free_server` naming `version`.
- **When the list can't be read, the create refuses** (**our call**) with 502 `versions_unavailable`, the code `GET /versions` already answers. Reason: passing would bring back the very risk this feature removes; waiting longer than the catalog's own 10 s client timeout would hold the request for nothing. The catalog serves a stale copy when a refresh fails, so this only happens when the Engine has not read Mojang's list since it booted and Mojang is down now. The refusal is before the reservation, so it costs no creation, and `LATEST` still works.
- **Order:** the check sits where the modpack check is (after the body, email, existing server, block and pause checks), so a cheap refusal never waits on Mojang. A refused create counts as an attempt (the hourly 10) like every other refusal, and never as a creation.
- The service gets the list through a `Versions` interface declared in `freeservers` (the package stays a leaf); `cmd/server` passes the controllers' catalog, so both share one cache.

### 3.2 Basic settings at create

A new optional `settings` object on the JSON body:

| Field | Values | Default when left out | Written as |
|---|---|---|---|
| `gameMode` | `survival`, `creative`, `adventure`, `hardcore` | itzg's (survival) | `mode`; `hardcore` is `mode` survival + `hardcore` true, as presets do |
| `difficulty` | `peaceful`, `easy`, `normal`, `hard` | itzg's (easy); `hard` with hardcore | `difficulty` |
| `maxPlayers` | 1 to `limits.maxPlayers` | `limits.maxPlayers` (today's value) | `maxPlayers` |
| `pvp` | bool | itzg's (on) | `pvp` |
| `whitelist` | bool | off | `enableWhitelist` and `enforceWhitelist` |
| `seed` | at most 64 bytes, no control characters | random | `seed` |
| `motd` | at most 64 bytes, no control characters, no `§` | itzg's | `motd` |

- **The list (our call):** these are the settings a friend group asks about first; each is already a typed customization field the stack settings route takes on a free server, and none can pass a limit. Spectator is left out (nobody starts a world in it), and so are the world type, distances and the rest of Manage → Settings.
- **Hardcore** with a `difficulty` other than `hard` is 400 `invalid_free_server` naming `settings.difficulty` (Minecraft locks a hardcore world to hard; presets refuse the same).
- **Whitelist** on writes enforce too, so turning it on means only listed players join. The list starts empty; the owner adds names in Manage → Players. Starting names are a non-goal: the free server's owner may not have a Minecraft name linked, and a wrong name locks friends out.
- **MOTD** 64 bytes and no `§` (**our call**): one line of the server list, with no formatting codes a player can't see while typing. Manage → Settings still takes the full 256.
- **Seed** is free text; Minecraft hashes text seeds. 64 bytes is the customization's own limit.
- A value outside these is 400 `invalid_free_server` with the message naming the field (`settings.gameMode`, ...) and, new for every `invalid_free_server`, `data.field` naming it, so clients can point at the field. Unknown keys inside `settings` are ignored, like the body's.
- **Online mode stays forced on.** The create writes `onlineMode: true` as today, after the settings, and no setting names it.
- The settings are part of the create's body check (the first refusal after the attempts), so a bad one costs nothing.
- The free server view keeps `maxPlayers` as the plan's limit (unchanged meaning). The server's own number is in Manage → Settings.

### 3.3 The pack create

- `POST /free-server/pack` takes the same object as a `settings` form field holding JSON (optional; at most 1 KiB; JSON that doesn't parse is 400 `invalid_free_server` naming `settings`). Same checks, same customization.
- A pack's own server-side files cannot undo online mode: the pack checker keeps only text config files under `config/` and `defaultconfigs/` (no `server.properties`), and the stack create writes `ONLINE_MODE` as an env value, which itzg applies over `server.properties` on every start. A test pins that the pack create's stack request still has online mode on with settings given.
- The Modrinth create (`kind` `modrinth`) takes `settings` too: a Modrinth pack may ship `server-overrides/server.properties`, and itzg's env values win over it the same way.

### 3.4 What `GET /free-server` advertises

`limits.settings` (new; its presence means the Engine checks the version and takes `settings`):

```json
"settings": {
  "gameModes": ["survival", "creative", "adventure", "hardcore"],
  "difficulties": ["peaceful", "easy", "normal", "hard"],
  "seedMaxBytes": 64,
  "motdMaxBytes": 64
}
```

`limits.maxPlayers` already bounds `maxPlayers`. Clients draw the version picker and Options only when `limits.settings` is present (**our call**: an older Engine ignores unknown body fields and does not check the version, so a client must not offer the picker there).

The version list itself is the existing `GET /api/v1/versions?type=vanilla` (the panel) or the launcher's own Mojang list (`catalog_minecraft`, the same manifest). No new list route.

### 3.5 Records

Swagger regenerated; overlay examples for both creates and the GET; CLAUDE.md route rows; changelog items (the version check, the settings, `limits.settings`, `data.field`); the roadmap item for free-server settings, with the paired panel item; `go run ./cmd/api-catalog` if a code changes (none new: `versions_unavailable` and `invalid_free_server` exist).

## 4. Launcher

### 4.1 The create dialog (Servers → Create a server for friends)

- **Vanilla** gets a **Minecraft version** select, the New instance one (`VersionFields` without the loader, `useVersionChoice`): the newest release preselected, "Show snapshots" to see snapshots. Leaving it as it is sends that release's id (the Engine checks it).
  - **Our call:** send the id the list resolved rather than `LATEST`: the player sees the exact version they get, and the server stays on it when Mojang ships a new one (the instance they join with is on that version too).
  - While the list hasn't loaded (offline), the select is empty and Create sends no version, so the Engine uses `LATEST`, today's behaviour.
- **Options** is a disclosure under the name, closed by default, so Create stays one click away. Inside: Game mode (segments: Survival, Creative, Adventure, Hardcore), Difficulty (select; shows Hard and is locked with Hardcore), Max players (number, 1 to the limit, the limit by default), PvP (toggle, on), Whitelist (toggle, off; hint: only players you add in Manage → Players can join), World seed (text, optional), Server list message (text, optional).
  - Only what the player changed is sent; an untouched form sends no `settings`, so the server is exactly today's.
- Both the picker and Options show only when `limits.settings` is present.
- A refusal naming a field (`data.field`) puts the line under that field; others keep today's line.

### 4.2 Any instance → a free server

The ⋯ item **Play this with friends…** (en) / **Jugar esto con amigos…** (es, the current key) keeps its words (**our call**: it says what the player gets, in PRODUCT.md's plain voice; "Create a server from this…" names the mechanism). It now shows for **every instance kind**, when the player is signed in, has no free server, the instance isn't linked to a server, and:

| Instance | Server | Needs |
|---|---|---|
| vanilla (no loader) | `kind` vanilla on the instance's exact Minecraft version; nothing uploaded | `limits.settings` (the version is checked) |
| installed from a Modrinth modpack (`source.kind` modrinth) | `kind` modrinth with the instance's Modrinth version id | nothing more |
| any other modded instance | today's pack upload | `limits.pack` |
| linked to a server | not offered | |

- **Modrinth packs by version id (our call):** exact for the pack as published, no upload, and it works on an Engine with uploads off. Mods the player added on top aren't on the server; the dialog says so in one line ("Runs {{pack}} as published on Modrinth. Mods you added yourself stay on your computer.").
- A vanilla instance on a version the Engine wouldn't take (an old alpha or beta, which the launcher's list marks `old_*`) gets the dialog with one line saying free servers can't run that version, and no Create.
- The dialog prefills: the name (as today), the version (always, shown as text, not a picker: the server must match the instance), and nothing else (an instance doesn't know game mode or difficulty; those live in the world). The same **Options** disclosure as 4.1 when `limits.settings` is present.
- The modded flow keeps its check step (mods, memory, not on Modrinth); vanilla and Modrinth instances skip it.

### 4.3 Core and app

- `perland::CreateFreeServer` gains `version: Option<String>` and `settings: Option<FreeServerSettings>` (all fields optional, camelCase, skipped when `None`). The pack upload sends `settings` as a form field with the JSON when given.
- `FreeLimits` gains `settings: Option<FreeSettingsLimits>`.
- `free_server_create(name, kind, version, modrinth_version_id, settings, language)` checks the version against the Engine's compose-safe rule before sending and the settings against `limits.settings`' shape (game mode and difficulty in the lists, max players ≥ 1, seed and motd byte limits); the Engine checks again.
- `friends_create(id, name, settings, language)` chooses the path by the instance (4.2).
- The contract is regenerated with `UPDATE_CONTRACT=1`. Strings in en and es.

## 5. Panel

- The `/free-server` create form gets a **Minecraft version** `VersionCombobox` (`mode="minecraft"`, `serverType="vanilla"`, from `GET /versions`), empty meaning the newest, and an **Options** disclosure with the same settings as 4.1. Both only when `limits.settings` is present; otherwise the form is today's.
- The "latest only" rule in the form's comment goes, replaced by the reason it's now safe (the Engine checks the version).
- i18n en/es/ca; the mock engine (`scripts/mock-players.mjs`) gains the version check (against its mock versions list), the settings checks and `limits.settings`; unit tests; a roadmap record in the Engine per the panel's "Finishing a change".

## 6. Phase 2: create on any team

The owner widened the scope while phase 1 was being built: creating servers from the launcher is for every Perland user, not only free servers. Players on paid teams (owner or member, `servers:write`) have far more room, and Manage already works for any team (launcher#94, #97).

### 6.1 Order (our call)

Ordered PRs, each merged when green so value ships early:

1. **Engine PR 1:** sections 3.1–3.5 (version check, settings, `limits.settings`).
2. **Launcher PR 1:** sections 4.1–4.3 (picker, Options, any instance → free server). The master releases a beta after it.
3. **Panel PR:** section 5.
4. **Engine PR 2:** 6.2 (team create from a Modrinth pack version and from an uploaded pack, one shared checker).
5. **Launcher PR 2:** 6.3 (one create flow with a Where choice; any instance → a server on any team).

### 6.2 Engine: team stacks from packs

- **Today** `POST /api/v1/stacks/create-server` can't run a Modrinth pack: `ServerRequestDTO.ModrinthPack` is `json:"-"`, set only by the free servers.
- **Modrinth version id.** The create-server body gains `modrinthVersionId` (`^[A-Za-z0-9]{1,16}$`, the free create's rule). With it, `type`, `version`, `loaderVersion`, `modpack`, `preset` and `repos.modpack` must be left out (400 naming the field, like `preset_conflict`): the pack decides them. The engine runs the shared checker with the request's `ram` as the memory limit (`modpack_unsupported` with `data.reason`, `modpack_too_heavy` with `requiredMemoryMb`/`limitMemoryMb`, 502 `modrinth_unavailable`) and sets `ModrinthPack`, so the stack is made exactly like a Modrinth free server (`compose` TYPE MODRINTH). An older Engine refuses such a body (no `type`), never silently makes a vanilla server.
- **Uploaded pack.** `POST /api/v1/stacks/create-server/pack`, multipart: `request` (the create-server JSON, at most 64 KiB, with the same left-out rule as `modrinthVersionId`), `sha256`, and the `.mrpack` as `pack` (at most `ModpackMaxBytes`). Same `servers:write` scope, the same quota check (a middleware variant that reads the `request` part), the same audit action (`stack.create`), the same 10 uploads a minute per client IP. The pack goes through the shared checker exactly as a free server's (only files Modrinth serves with matching hashes; overrides only text configs under `config/` and `defaultconfigs/`), and the stack mounts the clean pack the engine wrote, read-only.
- **No switch (our call):** a team stack already takes any file through the files API, so a checked pack adds no risk; `FREE_SERVER_PACKS_ENABLED` stays the free servers' own switch.
- **One checker.** `ModpackChecker` and `PackChecker` (with their errors, `RequiredMemoryMB`, `ReadPackSummary`) move from `internal/freeservers` to a new leaf package `internal/minecraft/packcheck`; freeservers and the stack create both call it. Nothing is copied.
- `servers:write`-scoped creates stay refused on a free team (`freeCreateAllowed`), so the free team keeps its own create.
- Records as in 3.5; CLAUDE.md rows for both routes.

### 6.3 Launcher: one create flow

- **Servers → Create a server** (en) / **Crear un servidor** (es) replaces "Create a server for friends" when the player belongs to a team that can create; otherwise the button is today's.
- **Where** is the first choice: **Your free server** (the free team; free-server rules and copy, phase 1's form) or each team where the player is owner or member (from `/auth/memberships`, the JWT from `select-tenant`, as Manage does), by name. The team path never says "free".
- **On a team:** name, server type (Vanilla, Paper, Fabric, Forge, NeoForge, Quilt), Minecraft version per type and the loader version for modded (`GET /api/v1/versions` with the team's JWT), memory (choices in whole GB up to the team's `available` memory from `GET /api/v1/tenants/current/usage`, default 4 GB or the most available below it), CPU (our call: not asked; 2 cores, or the most the team has left below that), and the same Options as 4.1 (max players default 20, no plan cap but the Engine's).
- **Answers, worded plainly:** `started: false` → "Your server is ready, but your team has no room to run it now. Stop another server to start it."; 402 `quota_exceeded` → "Your team's plan has no room for a server this size."; 503 `host_full` → today's line; 402 `plan_feature_locked` on a field → the field shown locked with **Upgrade** (opens the panel's billing page), as the panel does.
- **Any instance → a server on any team:** the same ⋯ item opens the same dialog with Where. On a team:
  - vanilla → type vanilla, the instance's version;
  - a loader instance with no mods → that type, version and loader version;
  - an instance from a Modrinth pack → `modrinthVersionId`;
  - any other modded instance → the uploaded pack (`/create-server/pack`), after the same mods check as the free path, against the memory chosen.
- After a team create the dialog's last step is Manage for that server (not the invite step, since a team server's invite is the team's choice).

## 7. Left to the builder

Exact copy within PRODUCT.md (en/es, plus ca on the panel), the layout of Options within each design system, test names, and how the mock engines fake the version list.
