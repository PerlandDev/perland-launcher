# Perland Launcher: link the Minecraft account, and per-server achievements and playtime, Design

- **Date:** 2026-10-17
- **Issue:** PerlandDev/perland-launcher#52, part of the Perland integration roadmap (#47; snapshot `docs/superpowers/specs/2026-10-16-perland-integration-ideas.md`, L5). It also finishes the part of #32 left for later (achievements per server).
- **Needs (merged):**
  - PerlandDev/PerlandEngine#439 (the server add-on, the first part of PerlandEngine#433). Contract: `docs/superpowers/specs/2026-10-17-server-addon-stats-and-link-design.md` in PerlandEngine, its "Player routes" and "Launcher contract" sections, and the `/api/v1/player/minecraft*` and `/stats` rows of its CLAUDE.md. This spec never changes that contract.
  - PerlandDev/perland-launcher#60 (Sign in with Perland, #48): the session, `PlayerClient`, `PerlandAccountCard`, Your servers.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-17-perland-sign-in-your-servers-design.md`: the session, its Engine, the redaction rules, the calm-when-off rule.
  - `docs/superpowers/specs/2026-10-17-public-server-list-design.md` and `2026-10-17-server-news-design.md`: `PublicServerCard`, `ServerPanel`, and the one-quiet-line pattern.
  - The local playtime counter of #32 (`launcher-core/src/playtime.rs`, `ui/src/ui/playtime.ts`), which stays as it is.
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner brainstormed the idea with Claude on 2026-10-16 and pre-approved this session's recommendations. Decisions the issue does not settle are marked **our call**.

## 1. Why

When a player plays on a Perland server, the server keeps their achievements, not their instance. Since #32, the launcher counts time played per instance on this computer, but it cannot show what the player did on a server. The Perland server add-on now reports each player's advancements and playtime to the Engine. To read their own totals, a player first links their Minecraft account to their Perland account, which also tells Perland who they are in the game (later work uses it for invites, whitelists and friends online).

## 2. Outcome

- **Link, on Accounts:** inside the signed-in Perland account card, a *Minecraft account* row.
  - Not linked: "Link your Minecraft account to see your achievements on each server." and **Link Minecraft account**.
  - Pressing it shows a short code (`ABCD-EFGH`) and "Type `/perland link ABCD-EFGH` on a Perland server", with **Copy** for the whole command, the time left and **Cancel**.
  - The launcher checks every 3 seconds. Once the player types the command in the game, the row says "Linked to **Moi**" (the Minecraft name) with **Unlink**.
  - A code that runs out says so and offers **Get a new code**.
- **Achievements and playtime on the server's card:** the cards of Your servers and the public list show one quiet line, "12 achievements · 30 h played", for a server where the player has stats.
- **On the server's page (Home of a server instance):** one small plate, "12 achievements". Home keeps the local playtime counter of #32 in the version line, so no second, different playtime shows on the same screen.
- **Calm when off:** signed out, accounts off, the add-on off (503 `server_addon_disabled`), or an older Engine (404): no Minecraft row, no stats line, no plate, no error.

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - the link through a code typed in the game, as the Engine's launcher contract describes it;
  - showing the linked Minecraft name, and Unlink;
  - per-server achievements (count) and playtime on server cards, and the achievements count on a server instance's Home;
  - everything working in `pnpm --filter ui dev:mock`.
- **Non-goals:**
  - linking through Microsoft sign-in (later, once the launcher has it);
  - medals or badges (out of scope for this session);
  - advancement titles, icons or a list of advancements (the Engine gives IDs only, and modded titles are translation keys);
  - a stats screen, a total across servers, or stats of servers without an invite;
  - changing the local playtime counter of #32.
- **Anti-goals** (a failure even if it works):
  - an error, a toast or an empty box for a player whose Engine has the add-on off or who never signed in;
  - two different playtimes side by side on one screen;
  - a `plt_` token or a link code in a log, an error or the diagnostics export;
  - a token sent to any host other than the session's Engine.

## 4. Facts this rests on (PerlandEngine `main`, 9810488)

All under `{engine}/api/v1/player`, with the player token as a bearer, envelope `{success, data, error, meta}`:

- **`POST /minecraft/link-code`** answers `{code: "ABCD-EFGH", expiresAt, expiresIn: 600}`. A new code replaces the player's previous one. It answers 503 `server_addon_disabled` while the add-on is off, and a plain 429 past 20 a minute per player.
- **`DELETE /minecraft`** answers `{unlinked}` (false when nothing was linked). It works whether the add-on is on or off.
- **`GET /me`** gains `minecraft: {uuid, name, linkedAt}`, absent while none is linked. The contract says to poll `/me` every 3 s while the code is shown, at most until `expiresAt`.
- **`GET /stats`** answers `{minecraft, servers}`. `minecraft` is `null` without a link and `servers` is then `[]`. Each server has:
  - `name` (the stack);
  - `invite` (`{token, url?}`), absent while its invite is off;
  - `playtimeSeconds`, `advancements` (the count), `advancementIds`, `firstSeenAt` and `lastSeenAt`.

  It answers 503 `server_addon_disabled` while the add-on is off, and 503 `player_accounts_disabled` while accounts are off. An Engine older than the add-on answers 404. The contract: read it once per launcher session and after a game exits, match a server to an instance by `invite.token`, and leave out servers without an invite.
- A plain 401 on any of them means signed out (the existing `perland_signed_out` handling).
- Link refusals in the game (wrong code, account in use, offline server) are shown by the add-on in the game. The launcher only sees that `minecraft` appeared on `/me`, or that the code ran out.

## 5. Decisions

| Question | Decision |
|---|---|
| Is the feature available? | `GET /stats` is the probe. A 200 means the add-on is on, so the Minecraft row and the stats show. 503 `server_addon_disabled` or `player_accounts_disabled`, a 404, or being signed out mean the feature is off: the app answers `None` and nothing shows. A 401 forgets the session, as elsewhere, and answers `None`. Any other failure (network) is an error the UI keeps quiet: no row and no line, and the next read tries again (**our call**: no extra probe call; the contract allows reading `/stats` once per session anyway) |
| When stats are read | React Query `staleTime: Infinity` (once per launcher session), invalidated when a game exits (the same place the local playtime is invalidated), after a link, after an unlink, and after a sign-in or sign-out (the Your servers removal already does this for its own key). The query key also carries the linked Minecraft name, so a link or unlink made elsewhere, which `perland_status` picks up within a minute, reads the stats again (**our call**) |
| Where the link UI sits | In `PerlandAccountCard`'s signed-in body, under the profile row, as one more row (`MinecraftLink.tsx`). No new screen (**our call**) |
| The code view | The code in large tabular type, split as the Engine sends it (`ABCD-EFGH`). One line: "Type `/perland link ABCD-EFGH` on a Perland server", with **Copy** (copies `/perland link ABCD-EFGH`). "Expires in 9:41", counting down each second. **Cancel** goes back to the not-linked row (the Engine forgets the code after 10 minutes anyway) |
| Polling | While the code shows and has not run out: `perland_link_poll` every 3 s, and it stops on link, on expiry, on Cancel, or when the card unmounts. A 401 ends it and the card shows signed out. Any other failure is ignored and the next tick tries again (**our call**) |
| `perland_link_poll` | Calls `/me` directly (not through `perland_status`'s once-a-minute throttle), saves the fresher profile and answers the linked Minecraft name or `None` |
| Expiry | At `expiresIn` seconds after the code arrived (measured locally, so a skewed clock doesn't matter): "The code expired." with **Get a new code**, which asks the Engine for a fresh one (**our call**) |
| Linked | "Linked to **Moi**" (the name the account had when linked). **Unlink** is a ghost button, with no confirmation: a new code links again, as Sign out has none (**our call**) |
| The linked name between launches | `Profile` gains `minecraft: Option<String>`, kept in the marker file (non-secret, like the email). `PerlandAccount` gains `minecraft: string \| null`. `perland_status`'s once-a-minute `/me` refreshes it, so a link made elsewhere shows within a minute |
| A link-code refusal | 503 `server_addon_disabled` is a new error code `minecraft_link_closed` ("Linking isn't available right now."), shown inline under the row. A 429 is the existing `rate_limited` |
| Stats matching | `ServerStats.token` is the stats' `invite.token`. A card matches by `card.token == stats.token`. An instance matches by the token of its saved invite (`server_source`), so the app gives each stats entry its `instance` id, as cards get theirs. Entries without an invite are dropped in the app (**our call**) |
| The card line | `"{n} achievements · {time} played"` from the server's numbers: the achievements part only when `advancements > 0`, the playtime part only when `formatPlaytime` gives text (one minute or more). Nothing when both are empty. Placed with the card's facts, quiet ink, no colour (**our call**) |
| Home | A plate under the status plates in `ServerPanel`: "12 achievements", only when `advancements > 0`. The server's playtime is not shown on Home, where the local counter already sits (**our call**: no two different playtimes on one screen) |
| Accounts total | Unchanged: the local total of #32. Server playtime is not added to it (**our call**) |
| Owner tag, private servers | A server without an invite (its invite is off) is the player's private history and is left out, as the contract allows |
| Medals | Out of scope (session notes) |
| Closing issues | The PR says "Part of" #52 and #32 and closes neither: #52 still has linking through Microsoft sign-in, and #32 still has medals. Each gets a comment with what shipped (**our call**) |

## 6. Design

### 6.1 launcher-core (`perland/api.rs`)

- **DTOs:**
  - `MinecraftAccount {uuid, name, linked_at}` (serde default).
  - `Player` gains `minecraft: Option<MinecraftAccount>`.
  - `LinkCode {code, expires_in}`; `expires_at` is not read.
  - `PlayerStats {minecraft: Option<MinecraftAccount>, servers: Vec<ServerStats>}`.
  - `ServerStats {name, invite: Option<StatsInvite {token}>, playtime_seconds: u64, advancements: u32}`. `advancementIds` and the dates are not read, since nothing shows them.
- **Calls:**
  - `link_code(token) -> LinkCode`: `POST /minecraft/link-code`.
  - `unlink(token) -> ()`: `DELETE /minecraft`.
  - `stats(token) -> Option<PlayerStats>`: `GET /stats`, body cap 4 MiB, since up to 200 servers with up to 2000 advancement IDs each can pass 1 MiB (**our call**). `Ok(None)` for 503 `server_addon_disabled` or `player_accounts_disabled`, and for a 404.
- **Error mapping:** `server_addon_disabled` on link-code is `minecraft_link_closed` (network). 401 stays `perland_signed_out`. Everything else maps as today.
- **`perland/session.rs`:** `Profile.minecraft: Option<String>` from `Player.minecraft.name`. `Marker` gains `minecraft` with `#[serde(default)]`, so a marker from #48 still reads.
- **`error.rs`:** `MinecraftLinkClosed` in the `// Perland account` group, code `minecraft_link_closed`.

### 6.2 launcher-app

- **`PlayerApi`** gains `link_code`, `unlink` and `stats` (and `CorePlayer`, `FakePlayer`).
- **`PerlandAccount`** gains `minecraft: Option<String>` (from the profile).
- **New DTOs:**
  - `LinkCodeView {code, expires_in}`;
  - `PerlandStats {servers: Vec<ServerStatsView>}`;
  - `ServerStatsView {token, instance: Option<String>, name, playtime_seconds, advancements}`.
- **Commands** (`perland.rs` or a new `minecraft_link.rs`), each with thin wrappers in `src-tauri/src/commands.rs` and registered in `main.rs`:
  - `perland_link_code() -> LinkCodeView`. Signed out, it is `perland_signed_out`; a 401 forgets the session.
  - `perland_link_poll() -> Option<String>`. It calls `/me` on the session's Engine and refreshes the stored profile. A 401 forgets the session and answers `perland_signed_out`.
  - `perland_unlink()`. It calls `DELETE /minecraft`, then clears `minecraft` in the stored profile; a 401 forgets the session.
  - `perland_stats() -> Option<PerlandStats>`. It is `None` when signed out, when the client answers `None`, and after a 401 (which forgets the session). Entries without an invite are dropped; `instance` comes from the instances' saved invite tokens.
- **Contract:** `LinkCodeView`, `PerlandStats` and the changed `PerlandAccount` get fixtures in `tests/contract.rs`. Regenerate with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

### 6.3 UI (through `/impeccable`)

- **State:**
  - zod schemas: `linkCode`, `perlandStats`, `serverStats`, and `perlandAccount.minecraft`;
  - `commands.perlandLinkCode`, `perlandLinkPoll`, `perlandUnlink` and `perlandStats`;
  - `qk.perlandStats`;
  - `usePerlandStats()`, enabled only while signed in, with `staleTime: Infinity`;
  - `useServerStats(token)` and `useInstanceStats(id)` selectors over it;
  - invalidated on game exit (beside `qk.playtime`), on link and on unlink, and removed on sign-in and sign-out (`switched`).
- **`features/accounts/MinecraftLink.tsx`:** the row inside the signed-in card. It shows only when `usePerlandStats()` has data (the feature is on). Its states:
  1. **Not linked:** the line and **Link Minecraft account**.
  2. **Code:** as in §5.
  3. **Expired:** "The code expired." and **Get a new code**.
  4. **Linked:** the name, and **Unlink**.

  Errors show inline in the card's error style. Moving between states moves focus to the state's first control, as the card already does. A screen-reader announcement says when the account is linked.
- **`features/servers-list/PublicServerCard.tsx`:** a `ServerStatsLine` (`features/servers-list/ServerStatsLine.tsx`) with the card's facts, from `useServerStats(card.token)`.
- **`features/servers/ServerPanel.tsx`:** a plate "12 achievements" from `useInstanceStats(instance.id)`, styled like the other plates on art.
- **Formatting:** `achievements` uses i18n plurals (`servers.stats.achievements_one`/`_other`). Playtime uses `formatPlaytime` plus "played" (`servers.stats.played`).
- **Mocks** (`ui/src/ipc/mockPerland.ts`). The `signedIn` scenario is not linked, with the link available. The mock code is `ABCD-EFGH`, and the mock poll links as `Steve` on its third call. New scenarios:
  - `linked`: already linked as `Steve`, with stats for the owner and one followed server, and for the mock server instance;
  - `noAddon`: signed in, with the add-on off (stats `null`).
- **Strings** in `en.json` and `es.json` (neutral Latin American Spanish, informal "tú"), under `perland.minecraft.*` and `servers.stats.*`, plus `errors.minecraft_link_closed.message`.

## 7. Testing

- **Core unit tests:**
  - `Player.minecraft` parses, and is absent when not sent;
  - an old marker without `minecraft` reads;
  - `Profile::from_player` carries the name.
- **Core HTTP tests** (`tests/perland.rs`, wiremock):
  - link-code success;
  - link-code 503 `server_addon_disabled` is `minecraft_link_closed`;
  - unlink success;
  - stats success, with an entry without an invite kept in core;
  - stats 503 (both codes) and 404 are `None`;
  - stats 401 is `perland_signed_out`;
  - the bearer header is set on all three.
- **App tests** with `FakePlayer`:
  - signed out: stats is `None` and link-code is `perland_signed_out`;
  - stats maps `instance` by invite token and drops entries without an invite;
  - a 401 on stats forgets the session;
  - the poll returns the name and the status's account then carries it;
  - unlink clears it.
- **UI (Vitest):**
  - `MinecraftLink`: hidden when stats is `null`; link, then the code with Copy, then the poll linking, then Linked; expiry offers a new code; Unlink; the `minecraft_link_closed` error inline;
  - `ServerStatsLine`: both parts, only one, none;
  - `ServerPanel`: the plate only with achievements;
  - stats invalidated on game exit.
- **Contract test:** the regenerated fixtures parse with the zod schemas.
- No fuzz, stress or load tests locally (CLAUDE.md).

## 8. Owner to do

None of these blocks the merge. Until they are done, the launcher shows none of this.

- [ ] Turn on the server add-on on the production Engine, as PerlandDev/PerlandEngine#439 lists:
  - `SERVER_ADDON_ENABLED=true` and `SERVER_ADDON_ENGINE_URL`, with `PLAYER_ACCOUNTS_ENABLED=true`;
  - add the add-on jar to Asalith's modpack;
  - confirm Asalith runs in online mode;
  - restart Asalith once.
- [ ] Once it is on, link once from a released launcher, to see the code, the in-game command and the linked name end to end. That path was tested here with fakes only.

## 9. Left to the builder

- Exact component structure and styling, through `/impeccable` and the surface's direction.
- Wording polish of the strings, keeping each one short and plain.
