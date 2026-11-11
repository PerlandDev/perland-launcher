# Anonymous usage counts and error reports (design, 2026-11-11)

## Why

Perland has no idea how many people use the launcher, which versions and systems they run, or what goes wrong for them: logs stay in a file on the player's computer. Earlier specs said "no telemetry in v1". This design replaces that with the smallest thing that answers those questions without identifying anyone.

## Decisions

1. **Anonymous by construction.** No install ID, no account ID, nothing that identifies a player or a computer. Uniqueness is decided on the player's computer: the launcher knows whether it already pinged today, this week or this month and says so with three booleans; the server adds them up and keeps only totals (no row per ping, no IP address).
2. **One toggle, on by default.** Settings gets one switch (`share_anonymous_data`, default on) with a plain explanation and a "What is sent" link to the README section. The first run mentions it in one line with the same link. Players who already use the launcher learn about it from What's new (the release note). Make the default one constant so it can be flipped to off-by-default in one line.
3. **Dev builds never send**, and `PERLAND_LAUNCHER_TELEMETRY=0` turns it off for any build. A test or `PERLAND_LAUNCHER_ROOT` scratch run must never reach a real server; provide an explicit override for testing against a chosen endpoint.
4. **Where it goes:** the engine behind the default Perland site, resolved the way the public server list resolves it. No new host, no third party.

## Anti-goals

- A token, email, player or account name, IP address, home-directory path, OS user name, server name or address, or file content in any payload.
- An on-device identifier.
- Telemetry that can fail, slow or block the launcher, or show the player an error.

## Wire contract (engine, public, no credential)

JSON bodies. A valid request answers `202` with `{"success": true, "data": {"accepted": <n>}, ...}`. Treat any 2xx as delivered. 400 or 413: drop that payload (never retry it). 429, 5xx, timeouts and network errors: keep it for the next start. Per-IP limits are 30/min (errors) and 10/min (ping), so batch.

### POST {engine}/api/v1/telemetry/errors  (body at most 64 KiB)

```json
{
  "source": "launcher",
  "version": "launcher version, <= 64 chars",
  "os": "windows | linux | macos | other",
  "arch": "<= 16 chars",
  "locale": "<= 8 chars",
  "reports": [
    {
      "kind": "exception | panic | request_error | unhandled_rejection | app_error | game_crash",
      "message": "1..500 chars",
      "code": "optional, <= 64 of [a-z0-9_.:-]",
      "stack": "optional, <= 8192 chars",
      "context": { "key": "value" },
      "count": 1
    }
  ]
}
```

`reports`: 1..20 per request. `context`: optional, at most 16 keys, key `^[a-zA-Z][a-zA-Z0-9_]{0,31}$`, string values at most 200 chars. `count`: optional 1..1000 (identical reports collapsed on the client), default 1.

Kinds the launcher uses: `app_error` (a `LauncherError` shown to the player: `code` is its stable error code, `message` its plain message, `context.category`, plus redacted detail and allow-listed params), `panic` (the panic hook: message, location, redacted backtrace), `exception` / `unhandled_rejection` (UI errors, sent over one IPC command; the webview's CSP stays as it is, all HTTP happens in Rust), `game_crash` (the crash doctor's cause id as `code`, with Minecraft version and loader in `context`; never log text).

### POST {engine}/api/v1/telemetry/launcher/ping  (body at most 8 KiB)

At most once per UTC day, at the first start of that day.

```json
{
  "version": "<= 32 chars",
  "channel": "stable | beta | dev",
  "os": "windows | linux | macos",
  "arch": "<= 16 chars",
  "locale": "<= 8 chars",
  "signedIn": true,
  "firstEver": false,
  "firstThisWeek": true,
  "firstThisMonth": false,
  "installWeek": "2026-W46",
  "counters": { "game_launches": 4 }
}
```

`firstThisWeek` / `firstThisMonth` are relative to the last SUCCESSFUL ping: ISO week (Monday start) and calendar month, both UTC. `firstEver` is the first successful ping of the install. `installWeek` is the ISO week (UTC) of the install's first run with this feature, `YYYY-Www`, never before `2026-W40`. `signedIn` says whether a Perland account is signed in. `counters`: at most 40 keys, key `^[a-z][a-z0-9_]{0,39}$`, value 0..100000; sums since the last successful ping, cleared only when the ping succeeded. A clock that went backwards must not produce a second ping for a day already pinged or a negative anything.

## Launcher

- `launcher-core`: a `telemetry` module with the payload types, the redactor (home path, OS user name, invite tokens, account names, URLs with queries; reuse `launch::Redactor`, `download::redact_url` and the diagnostics export's masking), a bounded on-disk state in the launcher's data folder (at most 50 queued reports, identical ones collapsed with `count`; the counters; the last ping date; the install week), and the sender over the existing HTTP client and user agent.
- `launcher-app` wires it: the panic hook, every `LauncherError` that reaches the player, UI errors over one IPC command, game crashes as the crash doctor's cause only, the daily ping, and counters. Pick a short list of counters that say how the launcher is used and document each in the README, for example: `game_launches`, `instances_created`, `packs_installed`, `joins_by_link`, `perland_sign_ins`, `microsoft_sign_ins`, `free_servers_created`, `servers_opened_from_list`, `skins_changed`, `crash_fixes_applied`, `updates_installed`. No counter may encode which server, pack or account.
- Settings UI: the one toggle with its explanation and link, in both languages; first run mentions it. Turning it off stops sending at once and deletes the queued reports and counters.
- README section "What the launcher sends": every field, when, to where, how to turn it off, and what is never sent.
- Shipping this to players needs a public release, which the owner approves; do not release.

## Approaches considered

1. A random install ID with opt-out (what most launchers do): exact counts and retention, but an identifier on the player's computer and a harder privacy story for a launcher whose players are often minors. Rejected.
2. Opt-in only: the cleanest story, but the counts would describe the few who opt in, which does not answer "how many people use the launcher". Rejected, with the default kept as one constant so this can change.
3. Client-side uniqueness flags with server-side totals (chosen): calendar-week and calendar-month actives and weekly cohort retention with no identifier at all.

## Addition, 2026-11-11: the player is told before anything is sent

From the orchestrating session, after the design above; no wire change. Perland's privacy policy promises to tell players about a significant change "in the launcher or by email before it takes effect". So:

- An install that updates into this version collects and sends nothing until the player has been shown the notice, and then only from the launcher's next start, so a player who reads it and turns the switch off never sent anything.
- A fresh install collects and sends nothing until the first run is finished (its welcome step carries the one-line mention), with the same next-start rule.
- "Notice shown" is kept as a date in the telemetry state.

How it is built:

- `telemetry.json` gains `noticeShown` (a UTC day). Collecting is allowed in a run only if it was already there when the launcher started.
- **Updated install:** What's new shows the release note `anonymous-data`. While the notice is due, the dialog shows that note whatever the "last seen" marker says (alone, or on top of the other news), so a player who skipped a release or has no marker still sees it. When the dialog opens with the note in it, the window tells Rust (`telemetry_notice_shown`). If the dialog can't open at that start (a link opened another screen, a game is starting), the notice stays due and nothing is collected.
- **Fresh install:** saving the settings with `firstRunDone` going from false to true is the notice.
- **Turning the switch on by hand** (off to on in Settings) is a notice too: the player is looking at the switch and its explanation.
- With the switch off, or in a build that never sends, no notice is due and none is forced.
- A build whose bundled notes don't carry `anonymous-data` as released (it can't happen in a release that has this feature, but the code allows for it) shows nothing extra and therefore never sends.

## Calls made while building

The wire contract above is unchanged. These are the decisions the design left open.

**What is sent**

1. **Counters** (the final list, each documented in the README): `game_launches`, `game_crashes`, `game_repairs`, `instances_created` (also counts the instance made along with a new team server), `instances_imported`, `packs_installed`, `joins_by_link`, `perland_sign_ins`, `microsoft_sign_ins`, `free_servers_created`, `skins_changed`, `updates_installed`. `servers_opened_from_list` was dropped: the core adds a server the same way from a link and from the list, so `joins_by_link` counts both. `crash_fixes_applied` became `game_repairs`: Repair is the one crash fix the core can see (the others are ordinary edits made by the window). `game_crashes` was added because crash reports collapse and are capped, so they can't give a crash rate.
2. **`app_error` reports** put the scrubbed detail in `context.detail` (cut to 200 characters), the failing UI command in `context.command`, and allow-listed params as `context.p_<name>`: `version`, `loader`, `loader_version`, `status`, `item`, `field`, `step`, `method`, `component`, `reason`, `what`, `min`, `max`, `count`, and `host` only when it is a public service (Mojang, Modrinth, Microsoft and the like). Every other param (`name`, `id`, `file`, `path`, `address`, `url`, `uuid`, `value`, …) is never sent.
3. **Not reported:** `sign_in_canceled`, `sync_cancelled`, `import_cancelled` (the player's own choice) and `offline` (it can't be sent while it lasts, and repeats with every poll).
4. **`game_crash`** is `code` = the cause's `crash_*` code or `crash_unknown`, `message` = "The game crashed", `context.minecraft` and `context.loader` (`vanilla` without one). Nothing else.
5. **`panic`** carries the scrubbed message, `context.location` and the scrubbed backtrace as `stack`. It is written to disk in the hook without waiting for any lock and sent at the next start.
6. **`locale`** is the launcher's language when the player picked one, else the system's language tag cut to 8 characters (`es-MX`), else `und`.
7. **`channel`** is the update channel the launcher follows (`stable` or `beta`); `dev` only from a dev build sending to a test endpoint.
8. **`signedIn`** comes from the session marker file; the credential store is not read for it.
9. **Limits are applied in bytes**, not characters, so a field is never over the contract's limit however the Engine counts.

**When**

10. The ping is tried once per start (not again while the launcher stays open). A queued report is sent 30 seconds after it is recorded, with whatever else arrived. After any send that is not taken (429, 5xx, a timeout, no network), nothing more is sent until the next start.
11. A ping answered 400 or 413 drops the counters it carried and is not marked as taken; the next start sends a new one.
12. A clock set back sends no ping until it passes the last pinged day again. A clock that was wrong forwards therefore silences the ping until that date; this was preferred over ever counting a day twice.
13. The window may hand over at most 30 reports per run (and itself sends at most 10 distinct ones), so a loop can't flood.

**Off**

14. Until the settings are read at start, and until the start after the notice (see the addition above), nothing is collected. With sharing off, a dev build, `PERLAND_LAUNCHER_TELEMETRY=0` (also `false`, `off`, `no`), or `PERLAND_LAUNCHER_ROOT` set, nothing is recorded and no file is written.
15. `PERLAND_LAUNCHER_TELEMETRY_URL` is the testing override: an Engine address (https, or http on this computer) that a dev build or a scratch folder sends to. A value that can't be used turns telemetry off rather than falling back to the real Engine.
16. Turning the switch off deletes queued reports and counters and keeps `installWeek` and `lastPing`, so turning it back on doesn't count the install as new.
17. Tester builds have debug assertions on, so they are dev builds and never send.

**Scrubbing**

18. Shapes are recognised first: URLs, the data folder, the home folder, any other absolute path wherever it stands (drives, UNC, `\\?\`, `file:`, `~/`, with spaces in folder names), emails, IPv4 and IPv6, `name:port`, any dotted word that ends like a host name (unless it is a file name, a version, a public service or a Java-style name), and anything shaped like a credential (a JWT, `Bearer …`, `token=…`, `code=…`). Then everything the launcher knows by name, in plain and percent-encoded spelling: account names and ids, the Perland profile, session token and Engine host, instance names and folders, server names and addresses, invite tokens and sites. Names are learned when the launcher starts and whenever one is made or listed (a new, renamed or imported instance, an added account, a sign-in, a free server).
19. What is kept on purpose: the data folder's top-level name the launcher chose (`<data>/instances`), a public service's host without its path, the window's own bundled files (`<app>/assets/…`), and the part of a build-machine path every player shares (`<cargo>/…`, `<rustc>/…`).
20. The scrubber removes when in doubt (a few words after a path, a property name that looks like a host). Known limits: a relative path and a bare file name are not replaced, and an error message can name a mod or modpack. Names the launcher knows are still masked there. The README says so.
21. Text is scrubbed first and cut to the contract's limits after, so a name is never cut in half before it can be masked.
22. Settings that can't be read turn sharing off for that run: a damaged file must not opt a player back in.

**Elsewhere**

23. The state file is `telemetry.json` in the data folder: `version`, `installWeek`, `noticeShown`, `lastPing`, `counters`, `reports`. A damaged or edited file is read field by field and cut to the limits, never an error.
24. The Settings switch sits in its own "Privacy" section after Diagnostics. The first-run line is on the welcome step, before anything else is asked.
25. The Engine is found from the same site as the public server list (`PERLAND_SITE_URL` when set, else the default site).
