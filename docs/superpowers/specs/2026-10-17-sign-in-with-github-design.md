# Sign in to the Launcher with Your Perland Account (GitHub), Design

- **Date:** 2026-10-17
- **Repos:** PerlandDev/PerlandEngine (most of the work) and PerlandDev/perland-launcher (a third sign-in button). The Engine part ships first; the launcher part needs it merged.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-17-perland-sign-in-your-servers-design.md` (#48): the launcher's Perland sign-in, the loopback Discord flow and Your servers.
  - In PerlandEngine: `docs/superpowers/specs/2026-10-16-player-accounts-design.md` (player accounts, #429; its "Later" note on the operator link is what this design settles) and `docs/superpowers/specs/2026-10-17-your-servers-design.md` (`owned()` in `internal/playerservers`).
- **Status:** brainstormed with the owner on 2026-10-17. The owner decided: people log in to the launcher with their existing Perland account, and one person has one Perland account, never a second one for the launcher. Everything else marked **our call** was settled while writing this.

## 1. Why

The launcher's Your servers tags a server **Owner** when the signed-in player runs it. For a free server this always works. For an operator's server (a team in the Panel) it works only through Discord: the Engine matches the player's Discord ID against `discord_links`, which the Panel fills when an operator links Discord through the Perlita integration (`OwnedInvites` in `internal/playerservers/gorm_store.go`). An operator who signs in to the launcher with email, whose install does not run the Discord integration, or who never linked Discord in the Panel sees none of their servers.

The cause is that Perland has two kinds of account with no direct tie between them. Operators (`users`) sign in to the Panel with GitHub. Players (`players`) sign in to the launcher with Discord or an email code. The player-accounts spec left the tie as "Later". This design ties them through the one identity every operator already has: GitHub.

## 2. Outcome

- The launcher's *Perland account* card offers **Continue with GitHub** next to *Continue with Discord* and *Continue with email*.
- It opens the system browser at the same GitHub page the Panel uses. The player approves and comes back signed in, as with Discord. The launcher never sees a password, a GitHub token or an operator credential.
- Once signed in, every server of every team the person belongs to in the Panel shows in Your servers tagged **Owner**. This needs no Discord integration and no linking step in the Panel.
- One person, one player account. Signing in with GitHub finds the player account the person already has (through their Panel Discord link or their email) and adds GitHub to it, so followed servers, the free server and the Minecraft link carry over. From then on, Discord, email and GitHub all sign in to that same account.
- Someone with a GitHub account but no Panel account can use the button too. They get an ordinary player account, just as with Discord.

## 3. Goals, non-goals, anti-goals

**Goals**
- An operator sees their teams' servers in Your servers after one GitHub sign-in, whichever integrations the install runs.
- An existing player account is reused, never duplicated, when it can be matched safely.
- The Discord path in `owned()` keeps working for players who never use GitHub.

**Non-goals**
- Signing in to the **Panel** with Discord or email. The Panel stays GitHub-only.
- Linking or unlinking GitHub from an already signed-in launcher session (a "Link GitHub" button). Signing in with GitHub does the link. Unlinking can come later.
- Merging two existing player accounts that both belong to the same person (see §5.3).
- Showing the person's team role (owner or member) in the launcher. Both read as **Owner**, as `OwnedInvites` does today.

**Anti-goals** (a failure even if it works)
- The launcher token gains any operator power: a GitHub sign-in in the launcher must yield the same player-scoped `plt_` token as Discord, never an operator session, JWT or API key.
- A GitHub sign-in in the launcher creates an operator user or a team, or bypasses `SIGNUP_POLICY`.
- The Engine stores the GitHub access token from a player sign-in.
- An account is joined to another on an unverified email.
- A player who never touches GitHub sees any change.

## 4. Today's pieces this reuses

- **GitHub App sign-in (Engine, `internal/authapi/github.go`).** `OAuthCallback` decodes the sealed state, exchanges the code with the Engine's own PKCE verifier (`st.Verifier`), reads the profile (`github.UserProfile{ID, Login, Email, EmailVerified}`) and then hands the result to a handler chosen by the state's purpose. Other packages already register their own purposes through `authapi.Config.Purposes` (`cmd/server/main.go`). So the player flow needs **no new GitHub App callback URL**: it is one more purpose on the existing callback.
- **Player Discord sign-in (Engine, `internal/players/discord.go`).** `GET /api/v1/player/discord/authorize` checks the loopback redirect, the launcher's S256 challenge and state, seals them into the OAuth state (`PurposePlayerSignIn`), and the callback resolves the player, parks a one-time code and redirects to the loopback with `code` and `state`. `POST /token` redeems the code with the verifier (`RedeemCode`). The state is single-use (`usedStates`).
- **Operator identities (Engine).** `oauth_identities` rows with `provider = 'github'` and `provider_id` = the GitHub numeric user ID tie a GitHub account to a `users` row. `users.email` is unique and comes from a verified GitHub email (sign-in refuses an unverified one, `github_email_unverified`).
- **Launcher (`crates/launcher-core/src/perland/`, `crates/launcher-app/src/perland.rs`).** `loopback.rs` and `pkce.rs` run the browser flow; `perland_sign_in_discord` drives it for Discord; `PlayerConfig.methods` ignores methods it does not know, so an older launcher simply never shows GitHub.

## 5. Engine design

### 5.1 Data

New columns on `players` (auto-migrated like the rest of the table):

| Column | Type | Notes |
|---|---|---|
| `github_user_id` | `bigint` null, unique | GitHub's numeric user ID. The tie to an operator is through this, never through a stored `users.id`, so a GitHub account that becomes an operator later is matched at once (**our call**) |
| `github_login` | text, not null, default `''` | Snapshot from the last GitHub sign-in |
| `github_avatar_url` | text, not null, default `''` | Snapshot, as above |

`DisplayName()` becomes: Discord global name, else Discord username, else GitHub login, else `""` (**our call**).

### 5.2 Routes

- **`GET /api/v1/player/config`**: `methods` lists `discord`, `github`, `email` in that order, `github` only when the GitHub App is configured (`authapi.Config.GitHub != nil`) (**our call** on the order: Discord stays first for the players who are most of the audience).
- **`GET /api/v1/player/github/authorize`**: same query and same checks as `/discord/authorize` (`redirect_uri` loopback or a configured web redirect, `code_challenge`, `code_challenge_method=S256`, `state`), same rate limit. It seals a new purpose `PurposePlayerGitHubSignIn` (`"player-github-signin"`) holding the redirect, the launcher's challenge, its state and a fresh Engine-side GitHub PKCE verifier, then 302s to the GitHub App's authorize URL. `ErrMethodUnavailable` (503, the same code and `data.method: github` that `/discord/authorize` answers) when the App is not configured.
- **Callback:** the existing `GET /api/v1/auth/oauth/github/callback`. `players` registers a `PurposeHandler` for `PurposePlayerGitHubSignIn` in `cmd/server/main.go`. The handler resolves the player (§5.3), parks a code exactly as `DiscordCallback` does, and redirects to the loopback with `code` and `state`. It does not call `saveGrant` and drops `done.Grant`.
- **Failures before the handler.** Today `OAuthCallback` sends every failure that happens before the purpose handler runs (GitHub's `error`, a failed code exchange, an unreadable profile) to the Panel's `/login`. For the player purpose these must instead redirect to the state's loopback URI with `error` and the client's `state`, or the launcher would wait until its timeout:
  - `error=access_denied` when GitHub reports the person declined;
  - `error=github_unavailable` when GitHub did not answer or the profile could not be read;
  - `error=server_error` for anything else.

  The single-use check (`usedStates`) and the allowed-redirect check run first, as in `DiscordCallback`. A state that fails them gets the Engine's error page, never a redirect.
- **`POST /api/v1/player/token`**: unchanged (it redeems any parked code).
- **`GET /api/v1/player/me`**: `PlayerDTO` gains `github?{id, login, avatarUrl}`, present when the player has GitHub.

### 5.3 Account resolution for a GitHub sign-in

Given the GitHub profile `g` (`ID`, `Login`, `Email`, `EmailVerified`), in a `withRetry` like the Discord resolution:

1. **Already linked:** a player with `github_user_id = g.ID`. Refresh the snapshot; if the player has no email and `g.Email` is verified and free, set it. Done.
2. **Through the Panel:** find the operator user whose `oauth_identities` row is `(github, g.ID)` and who is not disabled. If there is one:
   1. a player whose `discord_user_id` is that user's `discord_links.discord_user_id`;
   2. else a player whose email equals that user's `users.email` (lower-cased).

   The first match that has no `github_user_id` yet gets GitHub added (snapshot, and email if missing and free). Done.
3. **Through GitHub's own email:** if `g.EmailVerified`, a player whose email equals `g.Email` (lower-cased) and who has no `github_user_id`. GitHub is added. Done.
4. **New player:** `github_user_id`, snapshot, and `g.Email` when verified and no player holds it. Logged as `player account created`, method `github`.

Rules:
- A candidate that already holds a *different* `github_user_id` is skipped, never overwritten, and resolution moves to the next step (**our call**).
- When steps 2.1 and 2.2 would find two different players, 2.1 (Discord) wins and the other account stays as it is. Two existing player accounts are never merged (**our call**: merging follows, free servers and Minecraft links has its own conflicts and no one has asked for it).
- Every email used for a match is verified on both sides: player emails are verified by code or by Discord, `users.email` by the operator's GitHub sign-in, and `g.Email` only when `g.EmailVerified`.
- Discord and email sign-ins keep their resolution exactly as today. A later Discord or email sign-in reaches the GitHub-linked player through its Discord ID or email, which is what makes it one account.

### 5.4 Owned servers

`playerservers.Service.owned` gains a third source, between the player's own free servers and the Discord path:

- `Store.OwnedInvitesByGitHub(githubUserID int64, limit int)`: the same query as `OwnedInvites`, except it starts from `oauth_identities` (`provider = 'github'`, `provider_id` = the ID in decimal) instead of `discord_links`. It keeps the same filters: user enabled, membership role owner or member, enabled stack invites, ordered by `lower(name), id`.
- It runs whenever the player has a `github_user_id`, whether or not the Discord integration is on.
- The Discord path stays as it is, behind `discordOn`. The merge keeps dedupe by invite ID and the `MaxServers` cap, with free servers first, then GitHub, then Discord.

### 5.5 Tests (Engine)

- `players`: each resolution step, the skip-on-different-GitHub rule, the Discord-wins tie, an unverified `g.Email` never matching, the retry on a unique conflict, the state being single-use, and failures before the handler reaching the loopback with the right `error`.
- `playerservers`: `OwnedInvitesByGitHub` against the store suite (disabled user, other provider, member role, disabled invite); `owned` with GitHub only, Discord only, both overlapping, and Discord off.
- `playersapi`: config lists `github` only with the App configured; `/github/authorize` validation mirrors Discord's; `/me` carries `github`.
- `authapi`: the new purpose is supported; the player purpose never redirects to `/login`.

### 5.6 Docs (Engine)

Engine `CLAUDE.md` route tables (player routes, the callback's purposes), Swagger (`make docs`), `site/spec/overlay.yaml` examples, changelog and roadmap entries, as that repo's CLAUDE.md asks.

## 6. Launcher design

### 6.1 Core and app

- `SignInMethod` gains `GitHub` (`"github"`). `Player` gains `github: Option<GitHubAccount { id: u64, login, avatar_url }>`.
- The Discord flow becomes provider-generic: one `perland_sign_in_browser(method, language)` drives the loopback, PKCE and timeout for both, with `authorize_url` taking the provider path (`/discord/authorize` or `/github/authorize`) (**our call**: one flow, not a copy). The existing `perland_sign_in_discord` command keeps its name and behaviour as a thin wrapper, and a `perland_sign_in_github` command sits next to it, so the IPC contract only grows.
- The loopback callback treats `error=github_unavailable` like `discord_unavailable`: a `sign_in_failed` error with no new `ErrorCode` (**our call**). `access_denied` is a cancel, not an error, as today.
- `PerlandAccount` (the DTO and the `perland-account.json` marker) gains `github: bool`, like `discord: bool`.
- Regenerate `ui/src/ipc/contract/*.json` with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

### 6.2 UI

- `PerlandAccountCard`: `METHODS` becomes `['discord', 'github', 'email']`. The card shows only the methods the Engine's `/config` lists, as now. The GitHub step reuses the Discord step's "Finish in your browser" state, Cancel and "Open the page again".
- Signed in, the second line reads "GitHub" when the account has GitHub and no email, after the same rule Discord uses (**our call**).
- No GitHub logo or brand colour: a plain text button like the Discord and email ones (**our call**).
- New strings in `en.json` and `es.json` (neutral Latin American Spanish, "tú"): the button (*Continue with GitHub* / *Continuar con GitHub*), the waiting line if it names the provider, and *GitHub* for the signed-in line.
- The mock backend (`mockPerland.ts`) offers GitHub; its sign-in behaves like the mock Discord one.

### 6.3 Tests (launcher)

- Core (wiremock): config parsing with `github`; `/me` with and without `github`; the GitHub authorize URL path and parameters.
- App: GitHub sign-in opens `/github/authorize`, exchanges, and stores `github: true`; cancel is `None`; `github_unavailable` is `sign_in_failed`.
- UI (Vitest): the button shows only when listed, starts the GitHub command, and the signed-in line.

## 7. Rollout

1. Engine PR (§5). With the GitHub App configured, `/config` starts listing `github`; launchers released before this ignore it.
2. Launcher PR (§6), once the Engine PR is merged and deployed.
3. No new GitHub App setting is needed: the callback URL and the App are the ones the Panel already uses.

## 8. Left to the builder

- Exact names of types, functions and test files.
- Whether `OwnedInvitesByGitHub` and `OwnedInvites` share one query builder.
- How the failure redirect for the player purpose is wired into `OAuthCallback` (a per-purpose failure hook, or the player handler taking over earlier), as long as no player failure reaches `/login` and no other purpose changes.
- Wording of the new strings, within the i18n rules.
