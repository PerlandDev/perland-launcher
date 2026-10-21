# Perland Launcher: Continue Where You Left Off, Design

- **Date:** 2026-10-21
- **Issue:** filed with this spec (label `enhancement`), one of four launcher enhancements built in parallel (A undo last update, B crash doctor, C this, D share my instance).
- **Builds on:**
  - `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (launch, redacted output);
  - `docs/superpowers/specs/2026-10-11-join-by-link-design.md` (server instances, "Play and join", waking a sleeping server);
  - `crates/launcher-core/src/playtime.rs` (per-instance records in `.perland/`);
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner is away and asked for no questions. Every decision below is **our call**, made from `CLAUDE.md`, `PRODUCT.md` and `DESIGN.md`, with the reason next to it.

## 1. Why

After pressing Play, a player still clicks through the title screen and then the world list or the server list to get back to where they were. PRODUCT.md's success bar is "from opening the launcher to playing in as few clicks as possible". The launcher already knows how to drop the game straight into a server (`--quickPlayMultiplayer` on 1.20+, `--server`/`--port` before that), and Minecraft 1.20+ also takes `--quickPlaySingleplayer <world folder>`. The missing piece is remembering where the player was.

## 2. Outcome

- Home shows a **Continue** key for the selected instance when the launcher knows where the player last was and can take them back there: "Continue: Asalith Fields" or "Continue: My survival world".
- One click starts a normal run (pack check and sync, install, Java) that ends in that world or on that server, instead of the title screen.
- On a Perland server it behaves exactly like "Play and join": the pack syncs and a sleeping server is woken and waited for.
- When there's nowhere to continue (never played, played only in the menus, the world was deleted or renamed, the server left the player's lists, a Perland server that's gone, a version that can't open a world straight away), there's no Continue key and no message.

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - learn the last world or server from the game run itself, cheaply and without new permissions;
  - one Continue key on Home, secondary to Play;
  - Continue only where it really works;
  - the full Play path (sync, wake, progress, errors) for every Continue;
  - every string in en and es; works in `dev:mock`.
- **Non-goals:**
  - a history or picker of several recent worlds and servers (one place only);
  - Continue on the rail, in the instance menu, or on server cards (D owns the instance menu; Home is where Play is);
  - Realms (needs Microsoft sign-in, not built);
  - reading `level.dat` (gzip NBT) for the world's display name;
  - changing the launcher's log handling or crash handling (B's area).
- **Anti-goals:**
  - a second hero next to Play, or a second lit (amber) button;
  - a Continue that silently opens the title screen, or one that errors because the place is gone;
  - any new error, toast or empty box when there is nothing to continue;
  - storing anything about the player outside the instance folder.

## 4. How the launcher learns where the player was (our call)

We considered three sources and use the two that are reliable and cheap together:

| Source | Verdict | Why |
|---|---|---|
| The game's output line `Connecting to <host>, <port>` | **Used for servers** | Logged by every client version when a connection starts (title-screen join, Multiplayer list, Direct Connect, launcher join). The launcher already reads every output line (redacted) during the run, so watching for it costs one substring check per line and no file reads. From 1.17 it's the address the player typed; before 1.17 it's the address after the SRV lookup, which is what the game itself connected to. |
| `saves/*/level.dat` (and `session.lock`) modification times | **Used for worlds** | The world folder is exactly what `--quickPlaySingleplayer` takes. A world's `level.dat` is written when it's saved and when the player leaves it; `session.lock` when it opens. One `stat` per world at the end of a run. |
| `Preparing level "<name>"` in the log | Rejected | It names the level, not the folder, and its wording changed across versions. |

**At the end of each run** (the game exited, crashed or was stopped), the launcher works out the last place:

1. **Server candidate:** the last `Connecting to` line of this run, with the wall-clock time the launcher read it.
2. **World candidate:** the world folder in `saves/` whose newest `level.dat`/`session.lock` time is at or after the run's start.
3. The later of the two wins. A world's time is when it was last saved (usually when the player left it), a server's when the connection began, so "played a world, then joined a server" picks the server and "joined a server, then played a world" picks the world.
4. **Neither:** the record is left as it was. A run where the player only looked at the menus doesn't forget the last place.

The parser accepts the plain log line and Mojang's XML log4j event (`<![CDATA[Connecting to h, p]]>`). The host and port must pass `ServerAddress::new` (the same check that guards every address reaching the game's arguments); anything else is ignored.

**LAN worlds (our call):** a connection to a loopback, private or link-local IP literal is ignored unless that address is in the player's Multiplayer list. A LAN world gets a new port every time it opens, so a Continue there would nearly always fail.

**The record** lives in `<instance>/.perland/last-place.json`, like `playtime.json`, so it moves with a rename and goes with a delete:

```json
{ "kind": "server", "host": "play.example.com", "port": 25565, "own": true, "at": 1792400000 }
{ "kind": "world", "folder": "My survival world", "at": 1792400000 }
```

`own` is true when the address matched the instance's own Perland server when it was recorded (our call): that server can move, and `list_server` then repoints its Multiplayer entry, so an address match alone would lose it exactly when Continue should follow it.

A missing or damaged file reads as "nowhere", never as an error. A failed write is logged, nothing more (as with playtime). Writing happens before the run's final `idle` event, so the UI's refetch on `idle` sees it.

## 5. When Continue shows (our call)

Each time Home asks, the record is checked against the files as they are now. Continue shows only if all of these hold:

- **World:**
  - `saves/<folder>/level.dat` exists (deleted or renamed hides Continue);
  - the folder name is one plain path component (no separator, not `.` or `..`);
  - the instance's Minecraft version opens a world straight away: release 1.20 or later, snapshot 23w14a or later, a 1.20+ pre-release or release candidate, or the year-numbered versions (26.1 and later). Anything older or unrecognised gets no world Continue, since a button that opens the title screen would silently do nothing.
- **Server:** the address is still somewhere the player can see it in the game:
  - an entry in the instance's Multiplayer list (`servers.dat`), whose name becomes the label;
  - or the instance's own Perland server: the record says `own` (or the address equals its last known address), labelled with the server's name, and hidden when that server is marked gone or the instance is no longer a server instance;
  - or the Direct Connect field (`lastServer` in `options.txt`), labelled with the address.

  A server the player removed from their list, or a Perland server that's gone, hides Continue. We never ping a server to decide (it would cost a network round trip on every Home view and wake nothing).
- **Before 1.17, servers behind an SRV record (our call):** the logged address is the SRV target, which doesn't match what the player typed in their list, so those servers get no Continue. We accept that rather than run DNS lookups on every Home view; it only hides the key, never breaks it. Perland invite addresses are host and port, not SRV, and a Perland server instance is matched by `own` anyway.
- **Quitting the launcher while the game runs (our call):** that run never ends in this launcher (playtime is counted at quit for the same reason), so it records no place; the previous place stays and the next finished run records again. Recording from a half-finished run would guess.
- Multiplayer Continue works on every version that logs the connect line: 1.20+ uses quick play, older versions `--server`/`--port`, which the launch code already chooses from the version's declared features.

**Label:** a server's name from the Multiplayer list (falling back to the address when the name is empty), a Perland server's name, or a world's folder name. The folder name is almost always the world's name as typed (Minecraft only swaps illegal characters and adds " (1)" for duplicates), and showing it avoids parsing `level.dat`.

## 6. Launching a Continue

- New command `play_continue(id)`. It reads and checks the place again (the files may have changed since Home rendered). If it no longer holds, it plays normally (title screen): the player pressed a button to play, so they still get to play, and Home's refetch hides the key.
- The run goes through the existing `start_run`/`drive` path with a **destination** instead of the `join` flag:
  - `Title` — plain Play (unchanged);
  - `JoinOwn` — "Play and join" (unchanged), also used when the Continue server is the instance's own Perland server, so a moved address is followed and the pack syncs first;
  - `Join(address)` — any other server. The same wake check as "Play and join" runs for this address with no last known status (our call: the instance's own status says nothing about another server): a Perland waker that answers is knocked and waited for, and anything else is joined at once. A sleeping Perland server in the player's list wakes the same way;
  - `World(folder)` — `--quickPlaySingleplayer <folder>`.
- **Defence in depth:** the launch code adds `--quickPlaySingleplayer` only when the resolved version (with its loader merged) declares `is_quick_play_singleplayer`. If a world Continue reaches a version without it (the version heuristic was wrong), the run fails before the game starts with a new error `world_continue_unsupported` ("This Minecraft version can't open a world straight away" / fix "Press Play and pick the world in Singleplayer"), never a silent title screen.
- Everything else (account check, Microsoft gate, "already running", the update question, progress, crash card) is Play's.

## 7. Home (our call, through `/impeccable`)

- **Where:** in Play's column, directly above the Play button, right-aligned to it. Play stays the one big lit button; Continue is a standard **raised** key at the default size (not `lg`), never amber, never lit.
- **What it says:** "Continue: {name}" / "Continuar: {name}", with a small pixel icon for what it is (a world or a server). Long names truncate with an ellipsis inside a key no wider than Play; the full label is the key's accessible name and tooltip.
- **When:** only while the run is idle, and only when the instance has a place (section 5). Disabled with Play's reason when there's no account, like Play.
- **After a run:** Home refetches the place on `idle`, so the key appears or changes as soon as the game closes.
- **Mock:** `dev:mock` seeds Asalith Fields with a server place and the vanilla instance with a world place, and the mock's runs set a place when they end, so the key can be seen and pressed.

## 8. Parts

- `crates/launcher-core/src/last_place.rs` (new):
  - `LastPlace` (serde, the file above), `read`, `write`;
  - `connect_line(&str) -> Option<ServerAddress>`;
  - `latest_world(game_dir, since) -> Option<(String, SystemTime)>`;
  - `decide(server, world) -> Option<LastPlace>`;
  - `opens_worlds(minecraft) -> bool` (the version check);
  - `find_server(game_dir, &ServerAddress) -> Option<String>` (Multiplayer list name or Direct Connect), using `server_list::entries` and `options.txt`;
  - unit tests for each.
- `crates/launcher-core/src/launch/mod.rs`: `QUICK_PLAY_SINGLEPLAYER`, a `world: Option<&str>` on `LaunchSpec`, the feature check and the new error; `play::StartSpec` passes it through.
- `crates/launcher-core/src/error.rs`: `ErrorCode::WorldContinueUnsupported`.
- `crates/launcher-app`:
  - the run's destination, the connect-line watch in the run's sink, recording the place before `idle`;
  - `continue_target(id) -> Option<ContinueTarget>` (`{kind: "world" | "server", name}`) and `play_continue(id)`;
  - `Runner::start` takes the world;
  - contract fixtures regenerated with `UPDATE_CONTRACT=1`.
- `src-tauri`: two commands, wiring only.
- `ui/`: `commands.continueTarget`/`playContinue`, a `useContinueTarget(id)` query refetched on `idle`, the Continue key in Home, the mock, en/es strings, Vitest tests.

## 9. Left to the builder

- The exact pixel icons for world and server (from the existing icon set where one fits).
- Test names, module-internal helpers, and how the run's sink shares the last connect line with `drive`.
