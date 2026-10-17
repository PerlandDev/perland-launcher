# Perland Launcher: Sign in with Perland and Your Servers, Design

- **Date:** 2026-10-17
- **Issue:** PerlandDev/perland-launcher#48, part of the Perland integration roadmap (#47; snapshot `docs/superpowers/specs/2026-10-16-perland-integration-ideas.md`, L1).
- **Needs (merged):**
  - PerlandDev/PerlandEngine#437 (player accounts, #429). Contract: `docs/superpowers/specs/2026-10-16-player-accounts-design.md` in PerlandEngine, its two "Launcher contract" sections.
  - PerlandDev/PerlandEngine#438 (Your servers, the Engine side of #48). Contract: `docs/superpowers/specs/2026-10-17-your-servers-design.md` in PerlandEngine, and the `/api/v1/player/servers*` rows of its CLAUDE.md.
  - PerlandDev/PerlandEngine#440 (free servers, #430): a player's free server shows in Your servers tagged `owner`. Creating one is #49, which gets its own spec.
  - This spec never changes those contracts.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-17-public-server-list-design.md` (#50, merged in PerlandDev/perland-launcher#54): the Servers screen, `PublicServerCard`, `PublicListClient`, the Engine found through the site's `config.json`, and the calm closed state.
  - `docs/superpowers/specs/2026-10-17-server-news-design.md` (#51, PR #55): `NewsLine` on cards.
  - `docs/superpowers/specs/2026-10-11-join-by-link-design.md`: server instances, `InviteLink`, `join_add`.
  - `docs/superpowers/specs/2026-10-09-launcher-core-design.md` §4: the planned shape of account secret storage.
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner brainstormed the idea with Claude on 2026-10-16 and pre-approved this session's recommendations. Every decision the issue does not settle is marked **our call**.

## 1. Why

Players find a server through a friend's link or the public list, play once, and then have to find it again. A Perland account lets the launcher remember the servers that are theirs: the ones they own, the ones a friend invited them to, and the public ones they play on. It is also the identity free servers (#49) hang on. Players are mostly 12 to 25 and casual: signing in must feel like "Sign in with Google" in other apps, with no password typed into the launcher, and nothing may change for a player who never signs in.

## 2. Outcome

- **Accounts screen:** a *Perland account* card sits next to the Minecraft accounts.
  - Signed out, it offers **Sign in with Perland**: *Continue with Discord* (the system browser) and *Continue with email* (a 6-digit code typed in the launcher).
  - Signed in, it shows the player's name (or email) and **Sign out**.
- **Discord sign-in:** the browser opens Discord's page, the player approves, and the browser says "You're signed in, you can close this tab". The launcher never sees a password.
- **Email sign-in:** the player types an email and then the code that arrives.
- **Your servers:** one list at the top of the Servers screen, above the public list, holding the servers that are theirs.
  - **Owner**: they run it, including their free server.
  - **Invited**: they joined it through an invite link while signed in.
  - **Followed**: public servers they played, shown with no tag.
  - Each card has the same Play as a public card.
- **Following without a follow step:** Play on a public server follows it. A followed server's card has a menu with **Unfollow**.
- **Signed out:** everything works exactly as before. The only difference is one quiet line in the Your servers slot, "Sign in with Perland to keep your servers here", and only while the Engine has player accounts on.
- **Engine feature off or older Engine:** the Perland account card and the Your servers slot are not shown. There is no error, toast or empty box.

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - Discord sign-in through the system browser with PKCE and a loopback redirect, exactly as the Engine's launcher contract says;
  - email-code sign-in in the launcher;
  - the player token stored as an account secret, never logged;
  - sign-out, and a dead token treated as signed out;
  - Your servers with Owner and Invited tags;
  - follow on Play, Unfollow in the card's menu;
  - recording "invited" when a signed-in player joins through an invite link;
  - all of it working in `pnpm --filter ui dev:mock`.
- **Non-goals:**
  - creating a free server (#49);
  - linking the Minecraft account (#52, needs Microsoft sign-in);
  - personal invites (PerlandEngine#434);
  - a profile screen, a rename, or an avatar picker;
  - syncing the news "seen" state through the Engine (`POST /servers/seen`; the dot stays local as #51 built it);
  - several Perland accounts at once;
  - a sign-in page inside the launcher's webview.
- **Anti-goals** (a failure even if it works):
  - the launcher asks for or receives a password;
  - a `plt_` token or an email code in a log, an error, the diagnostics export or a URL;
  - a signed-out player sees an error, a broken box, or a nag on every screen;
  - three separate lists, a social feed, or a friends list;
  - an extra "follow" step the player has to learn;
  - a token sent to any host other than the Engine it came from.

## 4. Facts this rests on (PerlandEngine `main`, 9810488)

- All player routes are under `{engine}/api/v1/player`, and answers use the envelope `{success, data, error: {…}, meta}`. The code of a failure is in the error data, as for the public list.
- **`GET /config`** is public and answers `{enabled, methods}`. `methods` lists `discord` and `email`, in that order, when each is available; it is `enabled: false, methods: []` when accounts are off. An older Engine answers 404.
- **Discord sign-in:**
  - The browser goes to `GET /discord/authorize?redirect_uri=http://127.0.0.1:{port}/callback&code_challenge={S256}&code_challenge_method=S256&state={state}`.
  - The rules for `redirect_uri`: host exactly `127.0.0.1` or `[::1]` (`localhost` is refused), port 1024-65535, a path, and no query.
  - The browser comes back to `…/callback?code={43 chars}&state=…` or `…/callback?error={access_denied|discord_unavailable|server_error}&state=…`.
  - `POST /token {code, codeVerifier, redirectUri}` answers 200 `{token, player, newPlayer}` or 400 `invalid_code`.
  - A state the Engine cannot read (more than 10 minutes old) never comes back, so wait at most 10 minutes.
- **Email sign-in:**
  - `POST /email/start {email, language}` answers 202 `{expiresIn: 600, resendIn: 60}`, 400 `invalid_email`, 429 `too_many_codes` (`retryAfter`), 502 `email_unavailable` or 503 `sign_in_method_unavailable`.
  - `POST /email/verify {email, code}` answers 200 `TokenDTO` or 400 `invalid_code` with `attemptsLeft` (0 means ask for a new code).
- **Profile:**
  - `GET /me` with `Authorization: Bearer plt_…` answers `{id, displayName, email?, discord?{id, username, globalName, avatarUrl}, minecraft?, createdAt}`. `displayName` is `""` for an email-only player.
  - A missing, unknown or expired token is a plain 401.
  - `POST /sign-out` deletes the token's session.
  - A token lives 90 days from its last use.
- **Feature off:** every player route except `/config` answers 503 `player_accounts_disabled` while accounts are off.
- **Your servers routes:**
  - `GET /servers` answers `{servers: [YourServerDTO]}` with at most 200 entries. An entry is the public list's `ServerDTO` (the `PublicServer` launcher-core already parses) plus `relation` (`owner|invited|followed`; an unknown value means `followed`), `following` (bool) and `news` (`{latestAt, unseen}`, absent while news is off).
  - `status` can also be `offline` there (the public list never sends it); the launcher keeps it as `Offline` and treats only an unknown status like `sleeping`.
  - The order is owner, invited, followed, each by name. Do not poll faster than once a minute.
  - It answers 503 `player_servers_unavailable` when the Engine has no invite service.
  - `POST /servers/follow {token}` answers `{following: true}` and is idempotent; it is meant to be called on every Play, fire and forget. It answers 404 for a server that is not public now and 409 `too_many_player_servers`.
  - `POST /servers/unfollow {token}` answers `{following: false}`.
  - `POST /servers/joined {token}` answers `{recorded: true}`, with 404 and 409 as for follow. *Invited* shows only while the invite keeps that exact token.
  - The token is the invite's 43-character token, always in the JSON body, never in a path.
- **launcher-core today:**
  - It stores no secrets: `accounts.json` holds offline accounts only, and Microsoft sign-in is a later version.
  - `launcher-core-design.md` §4 plans the OS credential store (Windows Credential Manager, Secret Service) for account secrets.
  - `http::client`, `read_capped`, `InviteClient::engine_for_site` and the manual-`Debug` redaction convention exist (see the public list spec).
  - Nothing listens on a loopback port.
  - `tauri-plugin-opener` opens https URLs from Rust (`commands::open_link`).

## 5. Decisions

| Question | Decision |
|---|---|
| Which Engine | A sign-in starts against the Engine the public list uses, `InviteClient::engine_for_site` for the site (`PERLAND_SITE_URL` overrides the site). The stored session remembers that Engine's origin, and every player call afterwards goes to the session's origin and nowhere else, whatever `engine_for_site` answers later (it may answer an Engine learned from an instance file). A session whose Engine stopped answering stays until it answers 401 (**our call**: no "the site moved" check, which the cache could not answer reliably) |
| Token storage | The way the launcher-core design stores account secrets: the OS credential store (the `keyring` crate: Windows Credential Manager, Linux Secret Service; CI builds Windows and Linux only, so other platforms use the memory store). One entry (service `online.perland.launcher`, user `perland-account`) holds the token. A non-secret marker file `<root>/perland-account.json` (`{engine, name, email, discord}`, written atomically) says a session exists and holds the profile for showing it offline; the credential store is read only when the marker exists, and only on the first Perland call, never at start, so a player who never signs in never touches the credential store (on Linux a locked keyring could otherwise prompt). When the credential store is unavailable (a Linux desktop without Secret Service) the session lives in memory only and the player signs in again next launch, with no notice; no marker is written then (**our call**: an "encrypted file" whose key sits next to it protects nothing, and memory-only is the honest fallback) |
| Secrets in logs | `plt_` tokens, email codes, the PKCE verifier and the authorization code are never logged. Every type holding one (`Pkce`, the loopback `Callback`, the token and email-verify request bodies, `TokenAnswer`, `Session`) gets a manual `Debug` that prints `<redacted>`. Errors name the Engine's host only. The diagnostics export never includes the credential entry, which is not a file under the root anyway |
| Loopback | Bind `127.0.0.1` on an OS-chosen port (retry if it is below 1024), path `/callback`, accept connections for at most 10 minutes. Each request is read with a 5 s timeout and 8 KiB cap. Any other path gets a 404 and the wait goes on. A `state` mismatch gets a short "this sign-in link is old" page and the wait goes on. A match or an `error` ends it (**our call**) |
| Browser page | The loopback answers a tiny self-contained HTML page (no scripts, no external assets) in the launcher's language: "You're signed in to Perland. You can close this tab and go back to the launcher." / "Sign-in was canceled." / "Something went wrong. Go back to the launcher and try again." (**our call**) |
| Opening the browser | The app asks its host to open the authorize URL (a new `Host::open_url`, implemented in src-tauri with `tauri-plugin-opener`, and recorded by `RecordingHost` in tests). The URL is `http(s)` to the Engine only; it is built in core, never taken from the UI (**our call**) |
| Cancel | One sign-in at a time; starting one cancels the one before. The waiting card has **Cancel**, which drops the listener, and **Open the page again**, which re-opens the same URL |
| Language | The UI passes its current language (`i18n.language`: `es`, else `en`) to `perland_sign_in_discord` (for the loopback pages) and `perland_email_start` (for the email) |
| Where follow and joined happen | Fire and forget from the UI after the step succeeds; the app commands do the work. Play on a public card calls `your_servers_follow(invite)`. Every successful outcome of the Join server screen (which a `perland://join` link or a pasted link opens) calls `your_servers_joined(invite)`: a new server instance, opening the instance that already joins it (`existing`), moving an instance to the new link (relink, the path after an invite is regenerated) and linking a vanilla or imported instance. Signed out, these do nothing. Failures are logged quietly and never shown. No back-fill at sign-in: a player who joined before signing in gets *Invited* the next time they open the link (**our call**) |
| Invite to token | `InviteLink::token()`. A follow or joined is sent only when the invite's Engine is the session's Engine (`servers::server_source` gives an instance's Engine; a public card's Engine is the list's). Otherwise it is skipped |
| Your servers card | The public card component, made to take an optional tag (*Owner*, *Invited*) and an optional menu. Play behaves exactly as on a public card (join, start, open Home). It calls follow only on a `followed` entry; Play on an Owner or Invited card never follows, so an owner never gets Unfollow on their own server and an invite never turns into a follow (**our call**) |
| Offline servers | An owned or invited server can be `offline` (stopped or crashed). Its card reads "Offline" and Play stays enabled: the game opens with the server in its list, and nothing is woken, since joining does not start a stopped server. Starting a free server is #49 (**our call**) |
| Unfollow | A small "more" button on the card opens a menu with **Unfollow** only for a server whose Your servers entry has `relation: followed`: on that card, and on the public card with the same token. Unfollow takes effect at once (no confirmation: Play brings the follow back) and refreshes both lists (**our call**) |
| Duplicates | A followed public server shows in both Your servers and the public list. Hiding it from one would make the public list's count and filters lie (**our call**) |
| Empty Your servers | Signed in with no servers: the slot shows nothing (**our call**: the public list right below is the way to get some) |
| Signed-out nudge | One line in the Your servers slot, "Sign in with Perland to keep your servers here", with a **Sign in** link to Accounts, shown only while `/config` says accounts are on. It has no dismiss button: it is one line on one screen (**our call**) |
| Profile picture | Not shown: the player's initial in a round tile, as the Minecraft accounts do. The Discord CDN would be a new remote image source for the webview (**our call**) |
| Name shown | `displayName`, else the email, else "Perland player" |
| Polling | `GET /servers` while the Servers screen is open: on open, every 60 s, and after a follow, unfollow or join |
| Dead token | A 401 on any player route forgets the session (credential entry deleted) and the UI shows signed out. A 503 `player_accounts_disabled` keeps the session but hides everything, so it comes back when the Engine turns accounts on again. A network failure keeps the session and the last known profile |
| Sign-out | `POST /sign-out` (best effort, 5 s), then forget the session whatever it answered |
| News | Your servers cards show `NewsLine` like public cards (local seen state, #51). The Engine's `news.unseen` and `POST /servers/seen` are not used (**our call**: one source of truth for the dot) |

## 6. Design

### 6.1 launcher-core

- **`perland/mod.rs`** (new module `perland`):
  - `PlayerClient::new(http, engine)` over the shared `http::client`, with `request_timeout` 20 s and a body cap of 64 KiB, except `GET /servers` (up to 200 cards), which gets 1 MiB; 4 KiB for error bodies. Calls:
    - `config()`
    - `token(code, verifier, redirect)`
    - `email_start(email, language)`
    - `email_verify(email, code)`
    - `me(token)`
    - `sign_out(token)`
    - `servers(token)`
    - `follow(token, invite_token)`, `unfollow(token, invite_token)` and `joined(token, invite_token)`
  - The bearer is set per request and never put in a URL.
  - **DTOs**, serde mirrors of the contract:
    - `PlayerConfig {enabled, methods: Vec<SignInMethod>}`, where `SignInMethod` is `Discord|Email|Other` and an unknown method is ignored;
    - `Player {id, display_name, email, discord: Option<DiscordAccount>, created_at}`;
    - `TokenAnswer {token, player, new_player}` with a manual Debug;
    - `YourServer {server: PublicServer (flatten), relation: Relation, following, news: Option<NewsSummary>}`, where `Relation` is `Owner|Invited|Followed` and an unknown value reads as `Followed`.
  - **Error mapping.** Each case becomes a `LauncherError`; the new codes are in §6.4.
    - `/config`: a 404 (older Engine) or 503 is `Ok(PlayerConfig::off())`.
    - `player_accounts_disabled` is `player_accounts_closed`.
    - `sign_in_method_unavailable` is `sign_in_unavailable`, with a `method` param.
    - `invalid_email`, `too_many_codes` (param `retryAfter`) and `email_unavailable` are new codes of the same names.
    - `invalid_code` is `invalid_code` with param `attemptsLeft` when present.
    - A plain 401 is `perland_signed_out`.
    - `player_servers_unavailable` is `player_servers_closed`.
    - `too_many_player_servers` is a new code of the same name.
    - A 404 on follow or joined is `Ok(())`: the server simply isn't eligible.
    - Anything else maps as the public list does (`rate_limited`, `host_trouble`, `request_failed`, `unexpected`).
- **`perland/pkce.rs`:** `Pkce::new()`.
  - The verifier is 32 random bytes from `getrandom`, base64url without padding (43 characters).
  - The challenge is the base64url SHA-256 of the verifier.
  - The state is 16 random bytes, base64url.
  - `authorize_url(engine, redirect, pkce)` builds the URL with percent-encoded query values. The type has a manual Debug.
  - Unit tests use RFC 7636's test vector for the challenge.
- **`perland/loopback.rs`:** `Loopback::bind() -> (Loopback, redirect_uri)` and `Loopback::wait(state, language, deadline, cancel) -> Result<Callback>`, where `Callback` is `Code(String)` or `Error(String)`.
  - It uses a tokio `TcpListener` and a minimal request-line parser: the method must be GET, the path `/callback`, and the query is parsed with `percent-encoding`. There is no HTTP library.
  - It answers the pages of §5 with `Connection: close` and `Cache-Control: no-store`.
  - A deadline is `sign_in_timed_out`; a cancel returns `Canceled`, which the app turns into no error.
- **`perland/session.rs`:** `Session {engine, token, player}` with a manual Debug.
  - Storage goes through `trait SecretStore { load, save, delete }`, which carries one JSON string.
  - `KeyringStore` is the `keyring` crate. Its features are the native stores: `windows-native`, `apple-native`, and `sync-secret-service` with `crypto-rust` on Linux, since `dbus` is already in the lockfile.
  - `MemoryStore` is the fallback and is what tests use.
  - `SessionStore` holds the marker file (`<root>/perland-account.json`, non-secret) and the `SecretStore`. `load()` reads the marker and, only when it exists, the secret; a marker without a secret is cleared. `save()` writes the secret first, then the marker; a secret store that answers it has no backend makes the session memory-only (no marker). `forget()` deletes both.
  - Calls into the keyring run on a blocking thread.

### 6.2 launcher-app

- **`perland.rs`** adds an `AppCore` section, holding the session in a `tokio::sync::Mutex<Option<Session>>` loaded lazily on the first Perland command (never at start). A new `Runner` method gives the `PlayerClient` for the site's Engine, so tests use a fake.
- **Commands**, each with thin wrappers in `src-tauri/src/commands.rs` and registered in `main.rs`:
  - `perland_status() -> PerlandStatus {enabled, methods, account: Option<PerlandAccount>}`:
    - `enabled: false` hides everything;
    - `account` carries `{name, email, discord: bool, newPlayer}` and never the token;
    - `/me` is called at most once a minute; a 401 forgets the session.
  - `perland_sign_in_discord(language) -> Option<PerlandAccount>`:
    1. config check;
    2. PKCE;
    3. loopback bind;
    4. `Host::open_url`;
    5. wait;
    6. `POST /token`;
    7. save the session;
    8. `/me`.

    The command emits `AppEvent::PerlandSignIn {phase: waiting|exchanging}` so the UI can show "Finish in your browser". A cancel answers `Ok(None)`.
  - `perland_sign_in_reopen()` re-opens the waiting URL. `perland_sign_in_cancel()` cancels the sign-in.
  - `perland_email_start(email, language) -> {resendIn}` and `perland_email_verify(email, code) -> PerlandAccount`.
  - `perland_sign_out()`.
  - `your_servers() -> Option<YourServersPage {servers: Vec<YourServerCard>}>`:
    - `None` when signed out, accounts are off, or the Engine answers `player_servers_closed`;
    - a `YourServerCard` is `PublicServerCard` (built as `servers_list` builds it, `instance` included, except that `offline` stays `Offline` instead of folding into `Sleeping`) plus `relation` and `following`;
    - a 401 forgets the session and answers `None`.
  - `your_servers_follow(invite)`, `your_servers_unfollow(invite)` and `your_servers_joined(invite)`:
    - `invite` is the canonical invite URL the UI already holds, parsed with `InviteLink::parse`;
    - follow and joined swallow every error into a quiet log line;
    - unfollow reports its error.
- **Contract:** `PerlandStatus`, `PerlandAccount`, `YourServersPage`, `YourServerCard` and the new `AppEvent` variant get fixtures in `tests/contract.rs`. Regenerate with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

### 6.3 UI (through `/impeccable`)

- **Accounts** (`ui/src/features/accounts/`): a new `PerlandAccountCard` above or beside the Minecraft list, following the surface's direction and `DESIGN.md`. States:
  1. **Hidden** while `enabled` is false.
  2. **Signed out:** a short line ("Your servers and free servers go with you.") and one button per method in `methods`.
  3. **Waiting for the browser:** "Finish signing in in your browser", with **Open the page again** and **Cancel**.
  4. **Email:** the email field, then the code field (6 digits; spaces allowed), **Send a new code** enabled after `resendIn`, and inline errors (wrong code with attempts left, ask for a new code, check the address, wait N seconds, email couldn't be sent).
  5. **Signed in:** initial tile, name, the email or "Discord", and **Sign out**.
- **Servers** (`ui/src/features/servers-list/`):
  - `YourServers.tsx` becomes the real slot: a heading "Your servers" and the same grid of cards as the public list. While loading it shows nothing (no skeleton: it may well be empty).
  - When signed out it shows the nudge of §5, only while accounts are on.
  - `PublicServerCard` gains `tag` and `menu` props. The tag is a small label next to the status (*Owner*, *Invited*). The menu is a "more" icon button opening the `InstanceMenu`-style menu with **Unfollow**.
  - Play on a public card, and on a `followed` Your servers card, calls `commands.yourServersFollow(card.invite)` after a successful `joinAdd` (fire and forget). The Join server screen calls `yourServersJoined(invite)` after each successful outcome (add, existing, relink, link).
- **State:**
  - zod schemas for the new types;
  - React Query hooks `usePerlandStatus` and `useYourServers` (refetch every 60 s while mounted; `qk.yourServers` invalidated after follow, unfollow and join);
  - mutations for sign-in, email, sign-out and unfollow.
- **Mocks** (`ui/src/ipc/mock.ts`, a new `mockPerland.ts`). A `perland` option (`?perland=`):
  - `off`: accounts off;
  - `signedOut`, the default: Discord and email offered;
  - `signedIn`: a player with one owner, one invited and two followed servers;
  - `empty`: signed in with no servers;
  - `emailOnly`: only the email method.

  The mock Discord sign-in waits about 1.5 s on "Finish in your browser", then signs in. The email code `123456` works; any other code is wrong, with attempts left counting down.
- **Strings:** every string goes in both `ui/src/i18n/en.json` and `es.json` (neutral Latin American Spanish, informal "tú"), under `perland.*` and `servers.yours.*`, plus `errors.<code>.message` for each new code.

### 6.4 Error codes

All new codes go in `launcher-core/src/error.rs` under a `// Perland account` group:

| Code | Category |
|---|---|
| `player_accounts_closed` | network |
| `sign_in_unavailable` (`method`) | network |
| `sign_in_failed` | auth (Discord said no, `error=server_error`/`discord_unavailable`, or `POST /token` refused) |
| `sign_in_timed_out` | auth |
| `invalid_email` | auth |
| `invalid_code` (`attemptsLeft`) | auth |
| `too_many_codes` (`retryAfter`) | network |
| `email_unavailable` | network |
| `perland_signed_out` | auth |
| `player_servers_closed` | network |
| `too_many_player_servers` | network |

A canceled Discord sign-in (`error=access_denied`, or the player pressing Cancel) is not an error. Each code gets `errors.<code>.message` in en and es.

### 6.5 What waits

- **Free server creation (#49):** its own spec, plan and PR. The owner tag for a free server already works here.
- **The news dot through the Engine** (`/servers/seen`, `news.unseen`).
- **Microsoft sign-in and the Minecraft link (#52):** they will reuse `SecretStore`.

## 7. Testing

- **Core unit tests:**
  - PKCE against the RFC 7636 vector, plus lengths and alphabet;
  - authorize URL encoding;
  - the loopback parser: path, query, missing state, a request over 8 KiB;
  - `Relation` and `SignInMethod` reading unknown values;
  - manual Debug impls print no token.
- **Core HTTP tests** (`crates/launcher-core/tests/perland.rs`, wiremock):
  - every route's success;
  - the error mappings of §6.1 (401, 404 and 503 on `/config`, `player_accounts_disabled`, `invalid_code` with `attemptsLeft`, `too_many_codes` with `retryAfter`);
  - the bearer header set, and never a token in a path;
  - 404 on follow treated as `Ok`.
- **Loopback test:** a real bind on 127.0.0.1 with a tiny reqwest client playing the browser:
  - a wrong path, then a wrong state, then the right callback;
  - an `error=access_denied`;
  - a deadline.
- **App tests** with a fake player client and `MemoryStore`:
  - sign-in stores the session;
  - status after a 401 forgets it;
  - another Engine origin counts as signed out;
  - follow and joined are skipped when signed out or for another Engine's invite;
  - `your_servers` maps cards and keeps `instance`.
- **UI (Vitest):**
  - the Accounts card in each state, the email flow with a wrong code and then the right one, and Cancel;
  - Your servers: tags, the menu only on followed cards, Unfollow refreshing, the nudge only while accounts are on, nothing when off;
  - a public card's Play calls follow.
- **Contract test:** regenerated fixtures parse with the zod schemas.
- No fuzz or stress runs locally (CLAUDE.md). The loopback parser could join the nightly fuzz later.

## 8. Owner to do

- [ ] Turn on player accounts on the production Engine (`PLAYER_ACCOUNTS_ENABLED=true`, plus SMTP for email codes and the Discord integration for Discord), as PerlandDev/PerlandEngine#437 lists. Until then the launcher shows none of this.
- [ ] For *Followed* entries: `PUBLIC_SERVER_LIST_ENABLED=true`. For *Owner* entries of operator servers: the Discord integration, and the operator linking Discord in the panel.

## 9. Left to the builder

- Exact component structure and styling, through `/impeccable` and the surface's direction.
- Exact `keyring` version and the feature set that builds on the CI's Linux and Windows runners. If a native store crate will not build on one of them, fall back to `MemoryStore` on that platform and say so in the PR.
- Wording polish of the strings, keeping each one short and plain.
