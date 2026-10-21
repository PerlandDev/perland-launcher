# Free Server Settings and Servers From Any Instance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The free-server create takes a checked Minecraft version and basic settings (Engine), the launcher and panel draw them, and any launcher instance becomes a free server from its ⋯ menu.

**Architecture:** The Engine checks the version against its vanilla catalog and the settings against fixed lists inside `freeservers`, and writes them as the stack's customization in `freeStackRequest`. `GET /free-server` advertises `limits.settings`; clients show the picker and Options only when it's there.

**Tech Stack:** Go + Gin (Engine), Rust + wiremock + React/Vitest (launcher), Next 16 + Vitest (panel).

**Spec:** `docs/superpowers/specs/2026-10-21-free-server-settings-design.md` (launcher repo).

## Global Constraints

- Game modes: `survival`, `creative`, `adventure`, `hardcore`. Difficulties: `peaceful`, `easy`, `normal`, `hard`. Hardcore → mode survival + hardcore true; difficulty only `hard` (default `hard`).
- `maxPlayers` 1..`limits.maxPlayers`; left out → `limits.maxPlayers`.
- `seed` ≤ 64 bytes, `motd` ≤ 64 bytes, neither with control characters; `motd` without `§`.
- `whitelist: true` → `enableWhitelist` and `enforceWhitelist` true.
- Online mode always written `true`, after the settings.
- Version: empty → `LATEST` (no list read); else exact id in the vanilla catalog (releases + snapshots), else 400 `invalid_free_server` field `version`; catalog unavailable → 502 `versions_unavailable`. Before the reservation.
- Invalid settings → 400 `invalid_free_server`, field `settings.<name>`; every `invalid_free_server` gains `data.field`.
- `limits.settings = {gameModes, difficulties, seedMaxBytes: 64, motdMaxBytes: 64}`.
- Launcher: every UI string in en.json and es.json; contract via `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`; UI via `/impeccable`.
- Panel: i18n en/es/ca.

## Review Focus

- An untouched launcher/panel form sends no `settings` and (offline list) no version → byte-for-byte today's request. Test in L3 and P2.
- Hardcore picked then switched back to Survival: difficulty is unlocked and keeps the player's earlier choice, not "hard". Test in L3.
- A pack `settings` form field that isn't JSON → 400 `settings`, not 500. Test in E3.
- `maxPlayers` 0 / negative / above the limit → refused, not defaulted. Test in E2.
- A vanilla instance on `old_beta` → dialog explains and has no Create. Test in L4.

---

## Engine (PerlandEngine)

### Task E1: Version check

**Files:** Modify `internal/freeservers/service.go` (Config gets `Versions`), `internal/freeservers/create.go`; `cmd/server/freeservers.go` (wire `controllers.Versions`). Test `internal/freeservers/service_test.go` (or a new `version_test.go`).

**Interfaces:** Produces `type Versions interface { List(ctx context.Context, serverType, minecraft string) (versions.Result, error) }`-compatible interface declared in `freeservers` returning plain ids (to keep the leaf rule: `VanillaIDs(ctx) ([]string, error)` adapter in `cmd/server`), and `ErrVersionsUnavailable`.

- [ ] Tests: `TestCreateVanillaVersionChecked` — known `1.21.1` creates with spec Version `1.21.1`; snapshot `24w14a` passes; unknown `1.99` → `*InvalidError{Field:"version"}` with no reservation and no stack call; `LATEST`/empty never call Versions; Versions erroring → `ErrVersionsUnavailable`, no reservation. Run, see fail.
- [ ] Implement: in `create`, for `KindVanilla` with version ≠ `LATEST`, after the pause check, call `s.cfg.Versions.VanillaIDs(ctx)`; nil `Versions` in Config means unchecked (tests that don't care). Map in `writeFreeError`: `ErrVersionsUnavailable` → 502 `versions_unavailable` (`response.CodeVersionsUnavailable` exists — reuse).
- [ ] Run `go test ./internal/freeservers/ ./internal/playersapi/ ./cmd/server/`, pass. Commit.

### Task E2: Settings on the JSON create

**Files:** Create `internal/freeservers/settings.go` + `settings_test.go`; modify `create.go` (`CreateInput.Settings *Settings`, checked in `check`), `service.go` (`StackSpec.Settings Settings`), `cmd/server/freeservers.go` (`freeStackRequest`), `internal/playersapi/freeservers.go` (request DTO `Settings *freeservers.Settings \`json:"settings"\``; `invalid_free_server` gains `data.field`), tests `cmd/server/freeservers_test.go`, `internal/playersapi/freeservers_test.go`.

**Interfaces:** `type Settings struct { GameMode, Difficulty string; MaxPlayers int; PVP, Whitelist *bool; Seed, MOTD string }` with json tags `gameMode difficulty maxPlayers pvp whitelist seed motd`; `func (s Settings) check(maxPlayers int) (Settings, error)` returns normalized settings (hardcore difficulty defaulted to hard, MaxPlayers defaulted to the limit) or `*InvalidError{Field:"settings.x"}`. Exported `GameModes`, `Difficulties`, `SeedMaxBytes = 64`, `MOTDMaxBytes = 64`.

- [ ] Tests (table): each bad value per field (`gameMode: "spectator"`, `difficulty: "insane"`, hardcore+easy → `settings.difficulty`, `maxPlayers` 0/-1/limit+1, seed 65 bytes, seed with `\n`, motd 65 bytes, motd with `§`); good ones normalize. `freeStackRequest` test: hardcore → Mode survival, Hardcore true, Difficulty hard; whitelist → both flags; OnlineMode true always; no settings → only MaxPlayers + OnlineMode (today). Playersapi test: body with bad settings → 400 with `data.field`. Run, fail.
- [ ] Implement; run `go test ./...` for the three packages; commit.

### Task E3: Settings on the pack and Modrinth creates

**Files:** `internal/playersapi/freeservers_pack.go` (+ test), `create.go`.

- [ ] Tests: multipart `settings` `{"gameMode":"creative"}` reaches `CreateFromPack` input; `settings` `not json` → 400 `invalid_free_server` field `settings`; over 1 KiB → same. Modrinth-kind create with settings → stack spec has them and OnlineMode true. Run, fail.
- [ ] Implement; tests pass; commit.

### Task E4: `limits.settings` and records

**Files:** `internal/freeservers/service.go` (`LimitsView.Settings *SettingsLimitsView` — always set), swagger annotations on both creates and the GET, `docs/` overlay examples, `CLAUDE.md` route rows (free-server GET/POST/pack), `site/src/data/changelog.yaml`, `site/src/data/roadmap.yaml`, `make swagger` (or the repo's swagger command), `go run ./cmd/api-catalog` if it changes anything.

- [ ] Test: GET view has `limits.settings` with the exact lists. Implement. Regenerate swagger; `go build ./... && go vet ./... && go test ./... && golangci-lint run`; site `pnpm test && pnpm build`. Commit, PR, CI + PR Records green, merge.

## Launcher (perland-launcher)

### Task L1: Core wire

**Files:** `crates/launcher-core/src/perland/api.rs` (`CreateFreeServer.version`, `.settings`; `create_free_server_from_pack(..., settings: Option<FreeServerSettings>)`; `FreeLimits.settings: Option<FreeSettingsLimits>`), `crates/launcher-core/tests/perland.rs` (wiremock).

**Interfaces:** `pub struct FreeServerSettings { game_mode: Option<String>, difficulty: Option<String>, max_players: Option<u32>, pvp: Option<bool>, whitelist: Option<bool>, seed: Option<String>, motd: Option<String> }` (serde camelCase, skip None); `pub struct FreeSettingsLimits { game_modes: Vec<String>, difficulties: Vec<String>, seed_max_bytes: u32, motd_max_bytes: u32 }`.

- [ ] wiremock tests: create body with version + settings matches exact JSON; without them the body equals today's (no `version`, no `settings` keys); pack upload sends a `settings` part only when given; GET parses `limits.settings` and its absence. Fail → implement → pass → commit.

### Task L2: App commands and contract

**Files:** `crates/launcher-app/src/free_servers.rs`, `friends.rs`, `dto.rs`, `testing.rs`, `src-tauri` command wiring, `crates/launcher-app/tests/contract.rs`, `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/mock.ts`.

**Interfaces:** `free_server_create(name, kind, version: Option<String>, modrinth_version_id, settings: Option<FreeServerSettings>, language)`; `friends_create(id, name, settings: Option<FreeServerSettings>, language)` choosing: no loader → vanilla on `instance.game.minecraft`; `source` modrinth → modrinth with its version id; else pack. `friends_check` only for the pack path (vanilla/modrinth never call it).

- [ ] Unit tests: bad version (`1.21 1`, 40 chars) → `InvalidFreeServer` with no call; vanilla instance → `create_free_server` call with its version; Modrinth instance → `create_free_server` kind modrinth with its version id, no pack call; modded → pack call with settings; linked-to-server instance → refused. Fail → implement → pass. Regenerate contract. Commit.

### Task L3: Create dialog: version picker and Options (`/impeccable`)

**Files:** `ui/src/features/free-server/CreateServerDialog.tsx` (+ test), new `ui/src/features/free-server/ServerOptions.tsx` (+ test) holding the Options disclosure and a `settingsOf(state): FreeServerSettings | null` that returns only changed fields, `ui/src/i18n/en.json`, `es.json`.

- [ ] Tests: with `limits.settings`, Vanilla shows the version select with the newest release; Create untouched sends `version` = newest release and `settings: null`; with no list loaded sends `version: null`; without `limits.settings` no select, no Options (today); Options closed by default; Hardcore locks difficulty to Hard and back to Survival restores the earlier pick; max players input bounded 1..limit; a refusal with `params.field` shows under that field. Fail → implement → pass. Commit.

### Task L4: Any instance → server (`/impeccable`)

**Files:** `ui/src/features/home/InstanceMenu.tsx`, `ui/src/features/free-server/FriendsServerDialog.tsx` (+ tests), i18n.

- [ ] Tests: menu item shows for vanilla (needs `limits.settings`), Modrinth (always), modded (needs `limits.pack`), never for a server instance or with a free server; vanilla dialog shows the version as text, no mods check, Create sends `friendsCreate(id, name, settings, lang)`; Modrinth dialog shows the "as published" line; `old_beta` vanilla shows the can't-run line and no Create. Fail → implement → pass. All CLAUDE.md checks; screenshots; PR; merge main; CI green; merge.

## Panel (PerlandPanel), after E4 merges

### Task P1: Mock engine and lib

**Files:** `scripts/mock-players.mjs` (version check against its versions list, settings checks, `limits.settings`, `data.field`), `app/lib/free-server.ts` (types, `settingsProblem`), `app/lib/actions/player.ts` (`createFreeServerAction(name, language, version?, settings?)`), tests.

- [ ] Tests for `settingsProblem` and the action body (untouched → today's body). Implement. Commit.

### Task P2: Create form

**Files:** `app/components/play/create-form.tsx` (+ test), `app/components/play/free-plan.tsx` (pass `limits.settings`), i18n en/es/ca.

- [ ] Tests: no `limits.settings` → today's form; with it, a `VersionCombobox` (vanilla) and a closed Options; untouched submit sends no version and no settings; hardcore locks difficulty. Implement; panel checks; screenshots; Engine roadmap record for the panel item; PR; merge.

## Phase 2 (spec §6), after launcher PR 1 and the panel

### Task E5: Shared packcheck package

**Files:** Create `internal/minecraft/packcheck/` (moved `modpack.go`, `pack.go` and their tests from `internal/freeservers`); modify `internal/freeservers` (use `packcheck.Modpack`, errors), `cmd/server`, `internal/playersapi` (error mapping via `packcheck` types).

- [ ] Move with `git mv`; no behaviour change; every existing test passes in its new home. `go build ./... && go test ./...`. Commit (`Changelog: none (refactor)` for this commit's part).

### Task E6: Team create from a Modrinth version and from an uploaded pack

**Files:** `internal/minecraft/model/DTOs/servers/servers-request-body-dto.go` (`ModrinthVersionID string \`json:"modrinthVersionId,omitempty"\``, left-out rule in `ValidateFields`), `internal/minecraft/controllers/servers-controller.go` (`CreateNewServer` runs `packcheck.ModpackChecker.Check(ctx, id, ramMB)`; new `CreateNewServerFromPack`), `internal/minecraft/routes/servers-routes.go` + `audit.go` (`POST /create-server/pack`), `internal/api/middleware/quota-middleware.go` (variant reading the `request` part), swagger, CLAUDE.md, changelog, roadmap, overlay examples.

- [ ] Tests: `modrinthVersionId` with `type` → 400 naming `type`; good id → stack request has `ModrinthPack` with the pack's loader/version; too heavy against `ram` → 400 `modpack_too_heavy`; upload route: bad sha → 400; unsafe pack → 400 `modpack_unsupported` `unsafe`; quota middleware refuses an oversize `request` part with 402; audit records `stack.create`; free team still 402 `plan_feature_locked`. Implement; all Engine checks; PR; merge.

### Task L5: Launcher: one create flow with Where

**Files:** `crates/launcher-core/src/perland/stacks.rs` (`create_server`, `create_server_from_pack`, `usage`, `versions(type, minecraft?)`), `crates/launcher-app/src/manage.rs` or new `create.rs` (`creatable_teams`, `team_create`, `team_create_from_instance`), DTOs, contract, `ui/src/features/free-server/CreateServerDialog.tsx` (Where), new `ui/src/features/free-server/TeamServerForm.tsx`, `InstanceMenu.tsx`, `FriendsServerDialog.tsx`, i18n.

- [ ] wiremock: create-server body per instance kind (vanilla; loader no mods → type/version/loaderVersion; Modrinth → `modrinthVersionId` with no type); pack upload multipart parts `request`, `sha256`, `pack`; `started: false` mapped to a card flag; 402 `plan_feature_locked` keeps `field`. Vitest: Where lists free + owner/member teams (no viewer); team path has no "free" copy; memory choices bounded by `available`; started:false line; locked field shows Upgrade. Implement; all CLAUDE.md checks; screenshots; PR; merge.
