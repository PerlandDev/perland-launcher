# Perland Launcher: medals and achievements, saved and synced, Design

- **Date:** 2026-11-11
- **Builds on:** `docs/superpowers/specs/2026-10-21-medals-design.md` (medal rules, the achievements dialog). This spec replaces its non-goal "medals synced to the Perland account" and redesigns the screen.
- **Repositories:** perland-launcher (screen, local medals, local store, sync) and PerlandEngine (the account store and two player routes).
- **Status:** the owner approved the screen from mockups (spike `spike/achievements-steam`, captured 2026-11-11) and chose approach A below.

## 1. Why

The owner wants the screen to work like Steam's: every medal visible, earned ones in colour, the rest in grey, so a player sees what there is to chase. Most medals today need a Perland account, a linked Minecraft account and a Perland server, which the screen never said, and a player without an account had almost nothing to earn. So the launcher gains medals that need no account, the screen says plainly which ones need what, and an earned medal is never lost: it is kept on the computer with the date it was earned and, with a Perland account, in the account, so it survives a reinstall or a new computer.

The screen also mixed two different things under "Achievements": **medals** (the launcher's own) and **achievements** (Minecraft's own advancements, per Perland server). They are now named apart.

## 2. Goals, non-goals, anti-goals

- **Goals**
  - The screen "Medallas y logros" / "Medals and achievements" as approved (section 4).
  - 14 medals that need no account (section 5), 10 that need a Perland account and server (the existing ones), 24 in all.
  - Every earned medal tier kept with its date: on the computer always, in the Perland account when signed in.
  - Earned stays earned: a count that drops later (an instance deleted, a server gone) never takes a medal away.
- **Non-goals**
  - Showing medals to other players, on invites or in the panel.
  - A notification or toast when a medal is earned (later work).
  - Dates for Minecraft achievements (the Engine does not know when one was earned; unchanged from the medals spec).
  - Translated titles for modded advancements (unchanged).
- **Anti-goals**
  - A medal that disappears after it was earned.
  - The screen nagging: no big buttons, no dialogs; one inline link when something is missing.
  - Other launchers (partner apps) writing medals into a player's account.
  - Sending what the player does in the launcher to the Engine beyond the medal list itself.

## 3. Approaches

- **A. The launcher sends its medals and the account only stores them (chosen).** The launcher computes every medal, keeps them locally, and with a Perland session exchanges the list with the Engine, which merges it (highest tier, earliest date) and stores it. Simple, works for every medal. A tampered launcher could grant itself medals; medals are shown only in the player's own launcher, so nothing is lost by it.
- **B. The account checks each medal.** Works for the Perland medals, whose data the Engine has, but not for the 14 launcher medals: the Engine never sees themes, instances, mods or play days, and sending all of that is an anti-goal.

The owner picked A.

## 4. The screen

Route `achievements` (titlebar trophy, tooltip "Medallas y logros"). Screen title "MEDALLAS Y LOGROS" / "MEDALS AND ACHIEVEMENTS". Two top-level sections:

```
MEDALLAS Y LOGROS
Medallas                                              17 de 24 medallas
Las da el Perland Launcher. Unas se ganan usando el launcher; las que
llevan la etiqueta «Cuenta y servidor de Perland», jugando en servidores de Perland.
Tus medallas se guardan en tu cuenta de Perland: ...          (blue line)

  Medallas conseguidas                                              17
  [ two-column panel, colour medals, tiers, progress, "Conseguida el 7 nov 2026" ]

  Medallas por conseguir                                             7
  Las que llevan la etiqueta ... (en Cuentas pulsa «Vincular cuenta de
  Minecraft», copia el código ... /perland link CÓDIGO ...). Vincular cuentas
  [ two-column panel, grey medals, name + goal, tag where needed ]

Logros de Minecraft  [Cuenta y servidor de Perland]
Los logros del propio juego (menú Progresos) ... (en Cuentas pulsa ...). Iniciar sesión
  My world                                             12 logros  >
```

Details:

- **Count** "N de 24 medallas" counts medals with any tier earned.
- **Saved line** (blue): with a Perland session, "Tus medallas se guardan en tu cuenta de Perland: no las pierdes aunque reinstales el launcher o cambies de computadora."; without, "Tus medallas se guardan solo en esta computadora. Inicia sesión en Perland para guardarlas en tu cuenta y no perderlas si reinstalas el launcher."
- **Earned list:** medals with a tier, in catalogue order (no-account first, then Perland). Each shows its badge in colour, name, tag "Cuenta y servidor de Perland" for Perland medals, tier, ladder steps and the meter to the next tier, and "Conseguida el {date}": the date its current tier was earned.
- **To-earn list:** medals with no tier, flat grey (CSS grayscale and lower opacity, `data-unearned`), name, tag where it applies, and the goal of the first tier. No meter.
- **Inline link** (the `AccountsLink` of the spike), after the to-earn hint (only when a Perland medal is still to earn) and after the Minecraft hint, opening Accounts:
  - not signed in to Perland, or no Microsoft account on the launcher: "Iniciar sesión";
  - Perland session and at least one Microsoft account, Minecraft not linked: "Vincular cuentas";
  - linked: no link.
- **Minecraft achievements** section: always shown (signed out too), tagged, the hint text, and, when linked with stats, the list of servers with their count, each opening the existing dialog. The dialog gains a short line of how to get each vanilla advancement, in our own words (en and es), and the hint about where they come from.
- Wording, en and es, is the spike's final text (`spike10` stash). Spanish uses feminine agreement for medals ("Conseguida el …", "Las que llevan …"). The spike's plural keys with `_one` forms like "Instala 1 modpacks" are fixed.

## 5. The medals

Ladders have four tiers (copper, iron, gold, diamond). Singles have one tier whose rim is given.

**No account (14):**

| id | name (es) | kind | measure | tiers / rim |
|---|---|---|---|---|
| `time` | Tiempo de juego | ladder | seconds played over all instances (exists) | 1, 10, 100, 500 h |
| `firstGame` | Primera partida | single | a game run started by the launcher ended | copper |
| `theme` | A mi estilo | single | the launcher theme saved as anything but the default | copper |
| `decorator` | Decorador | single | an own picture set as an instance's image | copper |
| `tailor` | Sastre | single | a skin saved from the skin editor | copper |
| `doctor` | Médico | single | after a crash the launcher diagnosed, the same instance ran 10 minutes without crashing | iron |
| `instanceTime` | Fiel a una instancia | ladder | the most seconds on one instance | 5, 25, 100, 300 h |
| `collector` | Coleccionista | ladder | instances the launcher has now | 2, 5, 15, 30 |
| `modpacks` | Modpackero | ladder | modpacks installed from Explore, ever | 1, 3, 10, 20 |
| `modder` | Modder | ladder | mods installed from Explore, ever (dependencies count) | 10, 50, 200, 500 |
| `versions` | Viajero del tiempo | ladder | distinct Minecraft versions played | 2, 5, 10, 20 |
| `days` | Constante | ladder | distinct local calendar days with a run | 7, 30, 100, 365 |
| `marathon` | Maratón | ladder | the longest single run | 2, 4, 8, 12 h |
| `completionist` | Completista | single | every other no-account medal earned, ladders at diamond | diamond |

**Perland account and server (10, unchanged rules):** `regular`, `achiever`, `traveler` (ladders) and `diamonds`, `nether`, `end`, `elytra`, `hero`, `dragon`, `wither` (singles), computed from `GET /api/v1/player/stats` as today.

## 6. Kept on the computer

`launcher-core` gains `medals.rs`, a store at `<data root>/medals.json` (pattern of `whats_new_seen.rs`: one mutex, atomic writes, a damaged file reads as empty and is never an error to the player):

```json
{
  "version": 1,
  "earned": { "time": { "1": "2026-10-02T18:12:00Z", "2": "2026-11-07T21:40:00Z" } },
  "counters": {
    "modpacks": 1, "mods": 14, "versions": ["1.20.1", "1.21.4"], "days": ["2026-11-10"],
    "longestRunSeconds": 9000, "firstGame": true, "theme": true, "decorator": true, "tailor": false,
    "pendingDoctor": null, "doctor": false
  },
  "account": { "playerId": "…", "earned": { … }, "syncedAt": "…" }
}
```

- `earned` is this computer's own: medal id → tier → the time it was first reached. Tiers only ever get added.
- `counters` holds what no other file records. Measures that other files already hold (playtime per instance, the instance list) are read from there, not copied.
- The events that move counters, each in the Rust core where the action happens, never in the UI: a run ends (`firstGame`, `days`, `versions`, `longestRunSeconds`, and the `doctor` check), the theme setting saved, an instance image set, a skin saved from the editor, a modpack or mod install from Explore succeeds, a crash diagnosed (`pendingDoctor` = instance id).
- After every counter change and on start, the core evaluates the rules and records any newly reached tier with the current time. The rules move from the UI to the core so the core can date them; the UI renders what the core returns.
- **First run after the update:** what can be measured now is credited at once and dated now (time, instance time, collector). Counters that start from zero (modpacks, mods, versions, days, marathon) count from the update on.
- **Perland medals** are evaluated from the stats whenever they are fetched, and a newly reached tier is recorded with that time in `earned` like any other: a Perland medal earned while this computer saw it stays on this computer after signing out.
- `account` caches the signed-in account's set, so the screen shows it offline. Its `playerId` keeps one account's medals from showing under another: when the session's player differs, the cache is dropped and fetched again.

What the screen shows: `earned` merged with `account.earned` when the session's player matches, per medal the highest tier, per tier the earliest date.

## 7. Kept in the account (PerlandEngine)

- **Table** `player_medals`: `player_id` (the profile ID), `medal_id` text, `tier` smallint 1–4, `earned_at` timestamptz; primary key `(player_id, medal_id, tier)`. Deleted with the profile.
- **`GET /api/v1/player/medals`**: `{medals: [{id, tier, earnedAt}]}`, ordered by id then tier.
- **`PUT /api/v1/player/medals`**: body `{medals: [{id, tier, earnedAt}]}`; inserts each row and, on conflict, keeps the earlier `earned_at`; answers the whole merged list as GET does. Never deletes.
- **Rules** (a plain 400 naming the field and index): id `^[a-z][A-Za-z0-9]{0,31}$`; tier 1–4; `earnedAt` RFC3339, not before 2026-01-01, not more than a day ahead of the Engine's clock; at most 200 entries a call; a player holds at most 200 rows (past it the call is refused with 409, nothing written). The Engine does not know the catalogue, so a new medal needs no Engine release.
- **Access:** behind `RequirePlayer` like the other player routes. A third-party app's session (`thirdParty(c)`) is refused with 403 on both routes. Body cap 32 KiB; 30 requests a minute per player. Not audited (no team owns it), like the rest of the player routes.
- **Account merge** moves the absorbed profile's rows to the survivor with the same merge rule.
- Docs: CLAUDE.md player routes table, the changelog and roadmap, swagger and overlay examples, as CLAUDE.md requires.

## 8. Sync

With a Perland session and an Engine that serves the routes:

1. On start, after sign-in, when the screen opens and after a new tier is recorded: `PUT` the union of `earned` and the cached account set, then store the answer as the account cache.
2. A failure (offline, 5xx, 404 from an older Engine) changes nothing locally and is retried at the next trigger. No error is shown; the saved line keeps saying "en tu cuenta" because the next sync will catch up.
3. Signing out keeps `earned`, so every medal earned on this computer, Perland medals included, stays in the earned list. It drops the account cache, so medals the account brought from other computers show again only when that account signs in.

The local `earned` set is the computer's: a second Perland account signing in on the same computer receives the computer's medals too. The owner accepts this: launcher medals are earned on a computer.

## 9. Tests

- Core (Rust unit tests): every rule and threshold; tiers only added; dates kept on merge; a damaged file; the first-run credit; each counter event; the doctor sequence; the account cache dropped on another player.
- Engine (Go): DTO rules table; store suite (memory and Postgres under its DSN) for the merge; third-party refusal; the row cap; merge of two accounts.
- UI (Vitest): the two medal lists, the tag, the saved line both ways, the three link states, the Minecraft section signed out and linked.
- IPC contract regenerated.

## 10. Calls made without the owner

- The 14 thresholds and rims above (taken from the approved mockup).
- What "Arregla un cierre" means: 10 minutes of play after a diagnosed crash.
- Counters start at the update; only measures already on disk are credited back.
- 403 for partner sessions, the 200-row cap, the 2026-01-01 floor.
- The Perland medals' dates are the time the launcher first saw the stats reach the tier, not when the player did it in game.
- Medals the account brought from another computer are not copied into this computer's `earned`: they show while that account is signed in. Copying them would hand one account's medals to the next person who signs in on a shared computer.

## 11. Release

Launcher release notes (`release-notes/<slug>.json`, kind `feature`). The Engine change ships first; the launcher works without it (sync just never succeeds) and the release notes do not mention the account until the Engine is deployed.
