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

## Calls made while building

Filled in as the launcher part is built; see the end of this file's history.
