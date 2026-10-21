# Perland Launcher: medals and an achievements view, Design

- **Date:** 2026-10-21
- **Issue:** PerlandDev/perland-launcher#32 (reported on Discord by elpollooscuro). Shipped already: the local playtime counter (#32) and, with the Minecraft link (#52, PR #61), achievements and playtime per server from the Engine's `GET /api/v1/player/stats`. This spec finishes #32: **medals**, and a view of the achievements behind the count.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-17-minecraft-link-server-stats-design.md` (the stats query, `ServerStatsLine`, the Home plate, the calm-when-off rule).
  - The server add-on contract in PerlandEngine (`docs/superpowers/specs/2026-10-17-server-addon-stats-and-link-design.md`, "Player routes"). This spec does not change it.
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner authorised this workstream to design and ship without a design review. Every decision the issue does not settle is **our call**.

## 1. Why

The reporter asked for "medallitas según tu grado de enfermedad": badges that grow with how much you play. The launcher already knows the numbers (time on this computer, time and achievements per server, and which achievements), but only shows them as quiet counts. Medals turn them into something to chase, and an achievements list answers the obvious next question after "12 achievements": which ones?

## 2. Outcome

- **Medals, on Accounts:** a *Medals* section under the account list. Each medal is a small pixel badge with its name and one line: earned medals show their tier, locked ones show what earns them and, for ladders, a progress meter ("34 of 50 h").
- **Achievements per server:** a dialog listing the player's achievements on one server, grouped like the game's tabs (Minecraft, Nether, The End, Adventure, Husbandry, then everything from mods), earned first, with the vanilla ones not yet earned shown locked so it reads as a checklist. Opened from:
  - the server card's stats line ("12 achievements · 30 h played" becomes a button);
  - Home's "7 achievements" plate (becomes a button);
  - Accounts, a list of the player's servers under the medals ("Asalith · 12 achievements").
- **Calm when off:** without Perland stats (signed out, add-on off, older Engine) only the local playtime medal shows, and no server list. Signed in with the feature on but not linked: the server medals show locked with one line, "Link your Minecraft account to earn these." No error ever.

## 3. Goals, non-goals, anti-goals

- **Goals:** medal rules computed in the launcher from data it already has; earned/locked, tiers and progress; an achievements view per server with translated names for vanilla advancements; en and es.
- **Non-goals:**
  - an Engine change. The Engine already sends `advancementIds` per server. It does **not** send when each advancement was earned, so the view shows no dates (only "last played" per server would be possible, and is left out as noise) (**our call**);
  - medals synced to the Perland account, shown to other players, or announced in the game;
  - titles of modded advancements (they are translation keys the server doesn't hold; the view shows a readable form of the ID);
  - a toast or celebration when a medal is earned (later; it needs "seen" state);
  - achievements per instance outside Perland servers (the game keeps them on the server, see the issue).
- **Anti-goals:** Mojang art or textures (item icons, the advancement frames) or Modrinth branding; amber medals that compete with the one lit button; an error, empty box or spinner for a player without stats; two different playtimes presented as one number.

## 4. Facts this rests on

- `GET /stats` (Engine `main`, dac62ce) gives each server `playtimeSeconds`, `advancements` (count) and `advancementIds` (completed advancements with a display, so no recipes; sorted; at most 2000), `firstSeenAt`, `lastSeenAt`. Launcher-core reads only the first two today.
- Local playtime per instance (`usePlaytime()`) is already on Accounts, summed into "Time played on this computer".
- The stats query (`usePerlandStats`) is once per launcher session, invalidated on game exit, link, unlink, sign-in and sign-out.
- Vanilla advancement IDs in 1.20.1 are `minecraft:<tab>/<name>`, tabs `story`, `nether`, `end`, `adventure`, `husbandry`.

## 5. Decisions

| Question | Decision |
|---|---|
| Where medal rules live | In the UI as a pure module, `ui/src/features/medals/rules.ts`, with Vitest unit tests. Every input is already UI state (local playtime, stats), and every output is presentation (which badge, which line). Same split as `motd.ts` and `statsText`. A Rust copy would need a second stats read or an IPC round trip for pure arithmetic, and `dev:mock` would need a duplicate (**our call**) |
| What the backend changes | `ServerStats` (core) reads `advancementIds` (`Vec<String>`, serde default, at most 2000 kept), and `ServerStatsView` carries `advancement_ids`. The contract fixtures are regenerated. Nothing else |
| Medal set | Four ladders and seven single medals (§6.1). Ladders have four tiers. Small, recognisable set: each one reads in a line (**our call**) |
| Achievements across servers | "Achievements" ladder counts the **distinct** IDs over all servers, so the same achievement on two servers counts once; notable medals are earned on any server (**our call**) |
| Server time | The server ladder uses the **best single server's** time, the medal is about being a regular somewhere (**our call**). The local ladder uses the local total. They are separate medals with separate names, so no two playtimes are presented as one |
| Tier names | Copper, Iron, Gold, Diamond (Cobre, Hierro, Oro, Diamante): words players know, no textures (**our call**) |
| Medal art | Drawn in the launcher: a pixel-notched badge whose rim is the tier's metal and whose face carries a pixelarticons emblem. Metals are an art palette (like plot icons), not chrome hues; locked medals are `raised` faces with `ink-dim` emblems and a pixel lock. Progress uses the blue segmented-meter vocabulary (blue means progress). Final look decided through `/impeccable` |
| Achievement names | A table of the vanilla 1.20.1 advancement IDs (110) with names in en and es under `advancements.<tab>.<name>`, written for this launcher in plain words (the English ones match the game's titles so players recognise them; the Spanish ones are ours, neutral Latin American). Unknown IDs (mods, newer versions) show a readable form of the ID ("create:andesite_alloy" → "Andesite alloy", with the mod id "create" as its group) (**our call**) |
| Locked achievements in the view | Vanilla tabs list the table's advancements not yet earned, dimmed with a lock, after the earned ones; each tab says "5 of 16". The "From mods" group lists earned only, since the launcher can't know the rest. The server's own count stays the Engine's `advancements` |
| Where the view opens | `AchievementsDialog` (one component), from the card stats line, Home's plate and the Accounts server list. Only when the server has at least one achievement ID |
| Order of servers on Accounts | The Engine's order (last seen first) (**our call**) |
| Strings | All in `en.json` and `es.json`: `medals.*`, `achievements.*`, `advancements.*` |

## 6. Design

### 6.1 Medal rules (`ui/src/features/medals/rules.ts`)

Input: `{ localSeconds: number, stats: { servers: { playtimeSeconds, advancementIds }[] } | null, linked: boolean }`.
Output: `Medal[]`, each `{ id, kind: 'ladder' | 'single', tier: 0..4 (0 = locked; singles are 0 or their fixed tier), value, next: number | null, needsStats: boolean }`, in a fixed display order.

Ladders (thresholds per tier 1..4):

| id | value | tiers |
|---|---|---|
| `time` | local total seconds | 1 h, 10 h, 100 h, 500 h |
| `regular` | best single server's playtime seconds | 1 h, 10 h, 50 h, 200 h |
| `achiever` | distinct achievement IDs over all servers | 5, 25, 50, 100 |
| `traveler` | servers with at least one achievement | 1, 2, 3, 5 |

Singles (earned when any server's IDs contain one of the listed IDs), each with a fixed tier for its rim:

| id | IDs | tier |
|---|---|---|
| `diamonds` | `minecraft:story/mine_diamond` | 2 |
| `nether` | `minecraft:story/enter_the_nether` | 2 |
| `end` | `minecraft:story/enter_the_end` | 3 |
| `dragon` | `minecraft:end/kill_dragon` | 4 |
| `elytra` | `minecraft:end/elytra` | 3 |
| `wither` | `minecraft:nether/summon_wither` | 4 |
| `hero` | `minecraft:adventure/hero_of_the_village` | 3 |

`needsStats` is true for everything but `time`. The UI hides `needsStats` medals when stats are `null` (feature off) and shows them locked with the link line when not linked.

### 6.2 Backend

- `launcher-core/src/perland/api.rs`: `ServerStats.advancement_ids: Vec<String>` (`advancementIds`), truncated to 2000 after parsing. Wiremock test in `tests/perland.rs` extends the stats success case.
- `launcher-app`: `ServerStatsView.advancement_ids`, filled in `perland_stats`; the manual `Debug` adds the count only. Contract fixture updated, regenerated with `UPDATE_CONTRACT=1`.
- UI: `serverStats` zod schema gains `advancementIds: z.array(z.string())`; mocks give the `linked` scenario realistic IDs (vanilla plus a couple of `create:` ones).

### 6.3 UI

- `features/medals/rules.ts` and `rules.test.ts`.
- `features/medals/advancements.ts`: the vanilla table (tab → names in order), `group(id)` (tab or mod id), `readableName(id, t)`.
- `features/medals/MedalBadge.tsx`: the pixel badge (SVG frame + emblem), sizes 48 and 32.
- `features/medals/Medals.tsx`: the Accounts section (heading, grid of medals with name, tier or hint, progress meter), and `ServerAchievementsList` (one row per server with achievements, a button opening the dialog).
- `features/medals/AchievementsDialog.tsx`: title "Achievements on {server}", total, groups as sections with "n of m" (vanilla) or "n" (mods), each item with a small check or lock.
- `ServerStatsLine` and `AchievementsPlate`: become buttons when the server has IDs, keeping their quiet look.
- `Accounts.tsx`: renders `<Medals />` after the account list block.

## 7. Testing

- **Vitest unit:** every ladder at 0, at each threshold and between; `next` null at the top tier; distinct counting across servers; singles earned on any server; `needsStats` behaviour; readable names for vanilla, modded and malformed IDs; the advancement table has an `en` and `es` string for every entry (i18n test).
- **Vitest UI:** Medals hides server medals when stats are `null`, shows the link line when not linked, earned/locked and progress text; the dialog groups, counts and lists locked vanilla; the card line and Home plate open the dialog.
- **Rust:** `advancementIds` parses, is defaulted when absent, and is capped; contract fixture.
- **End to end:** `pnpm --filter ui dev:mock` with the `linked` scenario, driven with Playwright; screenshots in the PR.

## 8. Left to the builder

- The badge's exact pixel drawing and layout, through `/impeccable`.
- String wording, kept short.
