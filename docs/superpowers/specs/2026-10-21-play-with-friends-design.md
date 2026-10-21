# Perland Launcher + Engine: Play This With Friends, Design

- **Date:** 2026-10-21
- **Issue:** PerlandDev/perland-launcher#83 (part D2). Engine: PerlandDev/PerlandEngine#497.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-21-share-instance-mrpack-design.md` (D1, the `.mrpack` writer, `launcher_core::share`).
  - `docs/superpowers/specs/2026-10-17-free-server-for-friends-design.md` (the free-server flow, its contract and UI).
  - PerlandEngine `main` at 66f358f: `internal/freeservers` (`create.go`, `modpack.go`), `internal/playersapi/freeservers.go`, `cmd/server/freeservers.go` (`freeStackRequest`), `internal/minecraft/compose/modrinth.go` (`writeTypeEnv`), `internal/minecraft/invites` (`Packs`, `Pack`, `withModpack`).
- **Status:** design only, **not built**. The owner is away and asked for no questions; every decision is **our call** with its reason. The build was deferred on purpose (section 9).

## 1. Why

A player who built a modded instance wants to play it with friends on their free Perland server. Today a free server takes only vanilla or a Modrinth modpack *version id*, so an instance the player put together (Browse installs and tuned settings) can't become a server.

## 2. Outcome

- **Entry point.** **Play this with friends** sits in the instance's ⋯ menu, next to **Share…**. It shows only when all of these hold:
  - the player is signed in to Perland;
  - the Engine says it takes a player pack (`limits.pack`, section 5.6);
  - the player has no free server yet (one with a server sees its card in Your servers instead);
  - the instance has a loader (vanilla instances use the existing Create a server flow).
- **Step 1, check (before anything is created).** The dialog reads the instance like Share… does, without worlds, resource packs or shader packs. It says the plan's limits up front, built from the Engine's numbers like the existing create dialog: "Up to 10 players. 6 GB of memory with mods. Sleeps when nobody plays…". It refuses here, in one plain line, when:
  - **a mod isn't on Modrinth:** "Free servers only run mods from Modrinth. 2 of your mods aren't there: Foo, Bar." (no Create button);
  - **the pack is too heavy:** "Your mods need 8 GB; free servers have 6 GB." This uses the Engine's memory rule (`RequiredMemoryMB`: 2048 MiB + 24 MiB per server-side mod, rounded up to 512), recomputed in the launcher from the same index, so nothing is uploaded or created;
  - **the file is too big:** "Your settings are too big for a free server." (the upload cap, section 5.2).
- **Step 2, create.** "Creating your server…". The launcher writes the server pack (section 4) and uploads it with the name and language. The Engine checks it again and creates the server exactly like a Modrinth free server. Every existing refusal reads as today (`email_required`, `free_server_exists`, `too_many_free_servers`, `free_servers_full`, `host_full`, `modpack_too_heavy`…).
- **Step 3, share.** The invite link with **Copy link** and **Play now**, the same step 3 as today.
- **Friends** open the invite link in Perland Launcher. Their launcher reads the invite view's `pack` and downloads `/pack`, a path every released launcher already follows. They get the same mods, from Modrinth, and the same settings.
- **An older launcher** on an updated Engine sees nothing new. **A new launcher** on an older Engine shows no button (`limits.pack` is absent).

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - a free server made from the player's own instance, under every existing free-server rule and limit;
  - the too-heavy and not-on-Modrinth refusals happen before anything is uploaded or created;
  - friends join through the existing invite pack path;
  - the button appears only on Engines that support it.
- **Non-goals:**
  - worlds (the server starts a new world; a world upload is a separate feature with its own abuse questions);
  - resource packs and shader packs on the server (client-only; friends get the Modrinth-hosted ones through the pack index);
  - mods not on Modrinth;
  - updating the server's pack after creation (delete and create again, within the weekly limit);
  - CurseForge.
- **Anti-goals:**
  - a free server running any code Perland can't trace to Modrinth;
  - an upload that writes files the free-server rules forbid (`server.properties`, `ops.json`, `whitelist.json`, eula, scripts);
  - Perland redistributing player-supplied binaries to invitees;
  - a half-created server after a refusal;
  - a button on an Engine that can't take the pack.

## 4. The server pack (launcher side)

This reuses `launcher_core::share` (D1):

- **Choice:** `ShareChoice { mods: true, settings: true, resource_packs: false, shader_packs: false, worlds: false }`.
- **Refusal:** any mod in the scan that is not `Linked` is a refusal (section 2), named by file name.
- **Index:** only Modrinth entries, with `downloads` on `cdn.modrinth.com` only (stricter than D1's four hosts), sha1, sha512 and `fileSize`.
- **Overrides:** only files under `config/` and `defaultconfigs/` with a text extension (`.toml .json .json5 .properties .cfg .txt .yml .yaml .snbt .conf .ini`), each at most 1 MiB, all of them at most 8 MiB.
  - `kubejs/` is left out: it is scripts.
  - `options*.txt` are left out: they are client files, and friends keep their own.
- **Memory pre-check:** the launcher computes `RequiredMemoryMB` over index `mods/` entries whose `env.server` is not `unsupported`, against `limits.memoryMb.modded`.
- **New core function:** `share::server_pack(game_dir, plan, meta, out) -> Result<ServerPackReport>`. It reuses the writer with a stricter filter. The report counts `{mods, server_mods, required_mb, refused: Vec<String>}`.

## 5. Engine change (the smallest safe one)

### 5.1 Route

`POST /api/v1/player/free-server/pack` (player token, `multipart/form-data`):
- `name`, the free-server name rule;
- `language`, `en` or `es`;
- `pack`, the file.

It is a separate route, so the JSON create keeps its 4 KiB body cap and its swagger shape. It answers `201` with the same `freeservers.View`, and the same refusals in the same order as `Create` (attempts, body, email, existing server, block, pause, **pack check**, tenant, plan, host, reservation, stack, invite). It runs on a context the client leaving does not cancel, like the create.

### 5.2 Upload limits

- **Size:** the request body is capped with `http.MaxBytesReader` at `ModpackMaxBytes` (64 MiB) + 64 KiB for the form; past it, 413 `modpack_unsupported` `reason: too_large`.
- **Spooling:** the file goes to a temporary file under the Engine's data dir, never into memory, and is removed on every path.
- **Hash:** a `sha256` form field must match the file. This catches a truncated upload; it is not a trust signal.
- **Rate:** the existing create attempts limiter (10 an hour, one at a time per account) covers this route too. A per-IP `publiclink.Limiter` of 10 a minute guards the multipart parse before the player lookup.

### 5.3 The pack check (`freeservers.CheckUploadedPack`)

It is untrusted input, and the archive is read with `archive/zip` under these rules. Any breach is 400 `modpack_unsupported`, with a reason in `data.reason`: `not_modpack`, `loader`, `too_large`, or a new `unsafe`.

- **Zip bombs:**
  - at most 2,000 entries;
  - the index at most `IndexMaxBytes` (8 MiB);
  - every entry's declared uncompressed size counted before reading, and every read through `io.LimitReader(size+1)`, with a mismatch refused;
  - overrides in all at most 8 MiB uncompressed;
  - a compression ratio over 100:1 on any entry refused.
- **Index.**
  - It must have `formatVersion` 1 and `game` `minecraft`.
  - `dependencies`: `minecraft` must match `versionRule`, and exactly one of `fabric-loader`, `forge`, `neoforge`, `quilt-loader`, its version matching `versionRule` (the same `loaders` map as `ModpackChecker`).
  - Every file:
    - `path` is relative, uses `/`, has no `..`, `.`, empty segments, backslash, colon or NUL, is at most 255 bytes, and sits under `mods/`, `resourcepacks/` or `shaderpacks/`;
    - `hashes.sha1` is 40 hex and `hashes.sha512` is 128 hex;
    - `fileSize` is at most 512 MiB;
    - `downloads` is exactly one `https://cdn.modrinth.com/...` URL with no user info or port;
    - `env.client` and `env.server`, when present, are `required`, `optional` or `unsupported`.

  Anything else in a file is `unsafe`.
- **Overrides:**
  - only `overrides/config/**` and `overrides/defaultconfigs/**` entries with the text extensions of section 4, at most 1 MiB each;
  - `client-overrides/` with the same rule;
  - `server-overrides/` and any other entry: `unsafe`.

  Refused rather than dropped, so what runs is exactly what the player's launcher showed.
- **Memory:** the same `RequiredMemoryMB` over `mods/` files whose `env.server` isn't `unsupported`, giving `*TooHeavyError` (400 `modpack_too_heavy`) against `moddedMB`.
- **Rewrite:** the Engine never keeps the upload. It writes a **new** `.mrpack` from the parsed index (re-marshalled, unknown fields dropped) and the checked override bytes. The stored file is therefore canonical, and its `sha256` names it.

### 5.4 Storage and the stack

- **Storage:** `<WORKSPACE_ROOT>/.free-packs/<sha256>.mrpack`, written through a temporary file and renamed. It is outside every stack's `./data`, so a free player who can browse their server's files in the panel can't swap it.
- **Stack spec:** `StackSpec.Pack` gains `File string` (the stored path). `freeStackRequest` passes it as `ModrinthPackDTO.File`. `checkModrinthPack` takes either ids or a file path (absolute, under the packs dir, `.mrpack`, compose-safe).
- **Compose (`writeTypeEnv`):**
  - `TYPE=MODRINTH`, `MODRINTH_MODPACK=/perland/pack.mrpack`, `MODRINTH_LOADER=<loader>`, and no `MODRINTH_VERSION`;
  - a read-only bind mount `<stored path>:/perland/pack.mrpack:ro`;
  - labels `com.perland.pack.sha256` instead of the Modrinth ones, so the invite face has no `modrinth`;
  - online mode stays forced, as today.
- **Row:** `Kind` `pack` (new `KindPack`), `PackSHA256` (new column, default `''`), plus `Loader` and `MinecraftVersion` as for `modrinth`. `View.kind` gains `pack` (clients treat an unknown kind like `modrinth`; the launcher shows it as "Your own mods").
- **Cleanup:** the hourly sweep deletes stored packs that no row names and are older than an hour. A failed create's `undo` leaves the file to the sweep.

### 5.5 Friends: the invite pack

`cmd/server` wraps the invites' `Packs` with `freePacks`. For a stack whose free-server row has `kind: pack`:
- `Summary` answers `{name: <server name>, version: <sha256[:12]>, minecraft, loader, loaderVersion, modCount, state: ready, commit: <sha256>}`;
- `Open` answers `playerpack.Pack{Path: stored file, Name: <name>.mrpack, Commit: sha256}`;
- `OpenZip` and `Mods` answer `ErrNotFound`. A classic zip would need the jars, which the Engine never stores.

Every other stack goes to the existing `playerpack.Service`. The public view then carries `pack`, and every released launcher joins through `/pack` with its `mrpack::read_archive` (paths re-checked, hashes verified on download). The `/pack` limiter (30 a minute per IP) applies.

### 5.6 Capability

`GET /free-server` `limits` gains `pack: {maxBytes: 67108864}`. It is absent on older Engines and when the operator turns it off (`FREE_SERVER_PACKS_ENABLED`, default on with free servers). The launcher shows the button only when it is present.

### 5.7 Records

- New code: `modpack_unsupported` reason `unsafe` (catalogued with `go run ./cmd/api-catalog`).
- Changelog items: the route, `limits.pack`, `kind: pack`, the reason.
- Roadmap: the free-servers item.
- CLAUDE.md: the route row.
- Swagger regenerated, with overlay examples.
- Admin list: the `kind` filter gains `pack`.

## 6. Launcher change (after the Engine is deployed)

- **Core:**
  - `perland/api.rs` gains `create_free_server_from_pack(token, name, language, path)`, a multipart upload with the 150 s create timeout and a `sha256` field;
  - `FreeLimits.pack: Option<{max_bytes}>`;
  - `share::server_pack` (section 4).
- **App:**
  - `free_server_pack_check(id) -> PackCheck {limits, mods, required_mb, refused}`, which never uploads;
  - `free_server_create_from_instance(id, name, language) -> FreeServerCard`, which writes the server pack to a temp file, uploads it and removes it, under the same lost-answer polling as today's create.
- **UI:** `PlayWithFriendsDialog` reuses `CreateServerDialog`'s steps 2 and 3. The menu item comes from `useInstanceActions`, shown per section 2. The strings go in en and es, and the mock gains `?free=pack`.
- **New error code:** `free_server_mods_not_on_modrinth`, only for the launcher's own pre-check.

## 7. Tests

- **Engine (Go, table tests):**
  - every refusal of section 5.3 has a crafted archive: traversal (`../x`, `/abs`, `C:x`, backslash), off-host or http downloads, a missing sha512, `server-overrides/server.properties`, a jar in overrides, a script in `kubejs/`, 2,001 entries, an entry whose declared size lies, a 1000:1 entry, two loaders, no loader, a bad Minecraft version;
  - the rewrite drops unknown fields;
  - the memory rule matches `ModpackChecker`'s;
  - the create order (each earlier refusal wins);
  - a failed create leaves no row and the sweep removes the file;
  - `freePacks` serves the stored file for a `pack` row and delegates for others;
  - `writeTypeEnv` writes the mount and no `MODRINTH_VERSION`;
  - the storesuite runs for the new column.
- **Engine, by hand on a host with Docker:** itzg installs a local `.mrpack` from the read-only mount (section 9).
- **Launcher:**
  - the server-pack filter;
  - the memory pre-check against the Engine's numbers;
  - the multipart request (wiremock);
  - the capability gating;
  - the dialog states (Vitest);
  - a round trip: `share::server_pack`, then the Engine's check rules mirrored in a test fixture.

## 8. Release order

The Engine PR merges and deploys first. The launcher PR can merge before or after: without `limits.pack` it shows nothing. A launcher released before the deploy also shows nothing.

## 9. Why this was not built in this session

- **itzg could not be checked.** The safety of section 5.4 rests on itzg's behaviour with `MODRINTH_MODPACK` set to a local file on a read-only mount:
  - that it installs from it;
  - that it verifies index hashes;
  - that it honours `env.server: unsupported`;
  - that it writes nothing back to `/perland`.

  This session's container has no Docker daemon, so that path could not be exercised. Shipping an untrusted-upload path on the strength of reading the docs is the "half-checked" outcome the task asked to avoid.
- **The Engine change is large.** It is a new multipart route, a hardened archive checker, compose and storage changes, an invite-pack decorator, a sweep, a migration, swagger, catalog and the site records, plus the launcher half. That is a feature of its own, best built and reviewed in a session that can run itzg.
