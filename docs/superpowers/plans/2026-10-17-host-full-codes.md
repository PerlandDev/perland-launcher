# Host full, paused and blocked free servers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Word the Engine's three new free-server refusals (`host_full`, `free_servers_paused`, `free_server_blocked`) as errors and as create-gate reasons.

**Architecture:** The Engine's `data.code` already maps to an `ErrorCode` in `PerlandClient::refusal` (`crates/launcher-core/src/perland/api.rs`), which serves create (`POST /free-server`) and start/wake (`POST /free-server/start`); `free_servers.rs` passes those errors through unchanged. The create gate's `reason` is shown by `reasonLine` (`ui/src/features/free-server/freeServer.ts`). Each gets three new cases.

**Tech Stack:** Rust (wiremock tests), React + Vitest, i18next.

**Spec:** PerlandEngine `docs/superpowers/specs/2026-10-17-platform-capacity-and-monitoring-design.md`, Parts 1, 2 and 6.

## Global Constraints

- `host_full` reads "Perland is full right now. Try again in a few minutes." `data.dimension` and `data.scope` are not shown to players and not kept as params.
- `free_servers_paused`: "New free servers are paused. Try again later." `free_server_blocked`: "You can't create a free server."
- An unknown code keeps the existing fallbacks (status-based error; `freeServer.reason.other`).
- Every string in `en.json` and `es.json` (neutral Latin American Spanish, "tú"). Contract JSON regenerated, never hand-edited.

## Review Focus

- `host_full` on start (wake) as well as create: covered by a start wiremock test.
- `host_full` with `dimension`/`scope` in data: the test sends them and asserts no such params.
- A create-gate reason the launcher doesn't know: existing `something_new` assertion stays.

---

### Task 1: Error codes and Engine mapping

**Files:** `crates/launcher-core/src/error.rs`, `crates/launcher-core/src/perland/api.rs`, `crates/launcher-core/tests/perland.rs`, `ui/src/ipc/contract/error_codes.json` (generated), `ui/src/i18n/{en,es}.json`

- [ ] Tests: `free_server_codes_exist` gains `HostFull`/`host_full`, `FreeServersPaused`/`free_servers_paused`, `FreeServerBlocked`/`free_server_blocked`. Wiremock: `create_host_full` (503, data `{dimension: "memory", scope: "free"}` → `HostFull`, category Network, no `dimension`/`scope` params), `start_host_full` (503 on `/free-server/start` → `HostFull`), `create_paused` (503 → `FreeServersPaused`), `create_blocked` (403 → `FreeServerBlocked`).
- [ ] Add the variants to the free-servers group; add constructors `host_full`, `free_servers_paused`, `free_server_blocked` (all `network`, `host` param) and their match arms in `refusal`.
- [ ] `errors.<code>.message` in both locales; regenerate the contract; run `cargo test -p launcher-core -p launcher-app`.

### Task 2: Create gate reasons and mock

**Files:** `ui/src/features/free-server/freeServer.ts`, `freeServer.test.ts`, `ui/src/ipc/mockFree.ts`, `mockFree.test.ts`, `ui/src/i18n/{en,es}.json`, doc comments in `api.rs` / `dto.rs` / `schema.ts`

- [ ] Test: `reasonLine` maps `host_full` → `freeServer.reason.hostFull`, `free_servers_paused` → `.paused`, `free_server_blocked` → `.blocked` (same English as the errors).
- [ ] Mock scenarios `hostFull`, `paused`, `blocked`: the gate says the reason and create refuses with the code.
- [ ] `pnpm -r test`, lint, typecheck.
