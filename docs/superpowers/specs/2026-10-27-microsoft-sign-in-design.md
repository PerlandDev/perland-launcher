# Microsoft sign-in and the end of offline accounts: design

Status: design approved in conversation on 2026-10-27; awaiting review of this document. Updated 2026-10-28: offline accounts are removed from every build, dev builds included (decision 1).

## Why

Mojang turned down the launcher's AppID review. The review form now says:

> Applications must verify the user's game ownership and require authentication before enabling gameplay or access to servers or sites. Applications that bypass or disable authentication, ownership or license checks, or other safety features will not be approved. This requirement applies regardless of server configuration.

Every public build so far (`v0.2.0-beta.5` to `beta.25`) is a beta built with `PERLAND_BUILD=beta`. In those builds `offline_allowed()` (`crates/launcher-core/src/accounts.rs`) lets anyone type a name and play, with no Microsoft account and no ownership check. Offline accounts are also the only way to play today: `runs.rs` refuses any account that is not offline, because Microsoft sign-in was never built.

The goal is a launcher that passes the review. Every public build starts Minecraft only for a player signed in with a Microsoft account that owns it. We then re-apply once, after that build is public.

## Goals

- In beta and stable builds, the game starts only for a signed-in Microsoft account whose ownership was checked. Every route to Play counts: an instance, the public server list, a join link (`perland://join`) and Your servers.
- The session is checked online before every launch, and refreshed when needed. The game never starts on an old, empty or made-up token.
- Microsoft sign-in works end to end as soon as Mojang approves the client ID, with no new release.
- Until then, the update installs and keeps working: instances, pack sync, Browse, settings, the server list to look at, and every Perland feature. Every sign-in runs the whole chain for real; when Minecraft turns it down, the player sees each step that passed and that Minecraft hasn't approved the launcher yet.
- Public wording (README, `PRODUCT.md`, the UI strings in English and Spanish) no longer mentions offline accounts or "coming soon".

## Non-goals

- PerlandEngine and PerlandPanel are not touched. Whether Perland servers run in offline mode is outside the launcher's review.
- Perland sign-in (Discord, GitHub, email) and the features behind it (Your servers, Create a server, managing servers) stay as they are. They need only a Perland sign-in.
- Skins, capes and profile editing. Avatars stay generated from the UUID.
- Playing without internet. See the anti-goals.
- Releasing. Publishing the beta that carries this is the owner's call (every publish is public).

## Anti-goals

These would make the work a failure even if it technically worked:

- **Any way into the game without a checked Microsoft session, in any build.** That includes an offline account surviving in some corner (import, the accounts file edited by hand, an old `active` pointing at one), a cached token used when the online check fails, and a "skip" on the sign-in step that leads to Play.
- **Using another app's client ID.** That includes the official launcher's, or another approved launcher's.
- **Tokens in logs, crash reports, error details, events sent to the UI, or the accounts file.**
- **An embedded sign-in window.** The player signs in in their own browser.

## Constraints

- `launcher-core` holds the logic and `launcher-app` the state and run machine. Neither may depend on Tauri. `src-tauri` is wiring only.
- TDD. Pure logic gets `#[cfg(test)]` unit tests. HTTP behaviour gets `tests/*.rs` with wiremock, with every Microsoft, Xbox and Minecraft base URL injectable. UI gets Vitest and Testing Library.
- Errors shown to players are `LauncherError` with a stable `ErrorCode`. Every new code needs `errors.<code>.message` in both `ui/src/i18n/en.json` and `es.json` (neutral Latin American Spanish, informal "tú").
- The IPC contract (`ui/src/ipc/contract/*.json`) is regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, never edited by hand.
- UI styling goes through the `/impeccable` skill and `DESIGN.md`.
- Until Mojang approves the client ID, `POST api.minecraftservices.com/authentication/login_with_xbox` answers 403. Everything before it (Microsoft OAuth, Xbox Live, XSTS) works and can be tried with a real account.

## Approaches considered

How the player proves who they are to Microsoft:

- **A. Browser sign-in returning to a loopback address, with PKCE (chosen).** It is the flow Perland sign-in already uses (`perland/loopback.rs`, `perland/pkce.rs`, `sign_in_in_browser` in `launcher-app/src/perland.rs`), it is Microsoft's recommended flow for desktop apps, and it gives reviewers nothing to question. The cost is one redirect address on the Azure app registration.
- **B. Device code** (the player types a code at microsoft.com/link). It needs no local listener, but every player pays an extra step and it is a second flow beside Perland's.
- **C. A sign-in window inside the launcher.** Microsoft discourages it, password managers don't fill it, and it looks like phishing to the reviewers.

The owner chose A.

## What the owner decided

1. **No build has offline accounts, dev builds included.** *(Owner's call, 2026-10-28, replacing "only dev builds keep them".)* The code is gone from this repository: a reviewer reading the source finds no way to start the game without a checked Microsoft session. The "unlock after one licensed sign-in" gate is dropped too. Prism Launcher's gate was considered and turned down: the form's wording and our history make it a likely second rejection. The team's offline testing happens in a separate private repository that never produces a build for players; nothing here depends on it.
2. **While approval is pending, the update keeps the launcher installable and useful.** Sign-in is never shown as disabled: every press runs the whole chain against Microsoft, Xbox and Minecraft, and only Minecraft's own refusal says the launcher isn't approved yet. *(Owner's call, 2026-10-27: Mojang should see the code in action.)*
3. **Perland features stay open with just a Perland sign-in.** Only starting the game (and so joining a server) needs the Microsoft account.
4. **Sign-in is approach A.**

## The design

### Build configuration

- The Microsoft client ID comes in at build time as `PERLAND_MS_CLIENT_ID`, read with `option_env!` the way `PERLAND_BUILD` is read. A runtime override is read the way `PERLAND_PANEL_URL` is, for local testing. It is not written in the source: Mojang approves a client ID, and forks of this public repository must not launch with Perland's.
- The Release workflow passes it from a repository variable (`vars.PERLAND_MS_CLIENT_ID`).
- *(My call.)* The runtime override is read **only in dev builds**. A beta or stable build always uses the ID it was built with, so nobody can run a public build under another app's client ID. To try the real chain with an optimized build, the owner builds with the variable set.
- A build without it answers every Microsoft sign-in with `microsoft_not_configured` ("Microsoft sign-in isn't set up in this build").
- There is no offline gate. `licensed_signin_seen` stops deciding anything. It stays readable in the file, so old files still parse.

### The sign-in chain (launcher-core, new module `msa`)

One attempt, from the player's click to a stored account:

1. **Microsoft OAuth, authorization code with PKCE.**
   - Open `login.microsoftonline.com/consumers/oauth2/v2.0/authorize` with `client_id`, `response_type=code`, `redirect_uri` (the loopback's), `scope=XboxLive.signin offline_access`, `code_challenge` (S256), `state` and `prompt=select_account`.
   - Reuse `Pkce` and `Loopback` as they are. The callback page uses the existing English and Spanish pages.
   - Redeem the code at `…/consumers/oauth2/v2.0/token`. The answer is a Microsoft access token and a refresh token.
2. **Xbox Live user token.** `POST user.auth.xboxlive.com/user/authenticate` with `AuthMethod: RPS`, `SiteName: user.auth.xboxlive.com`, `RpsTicket: d=<access token>`, `RelyingParty: http://auth.xboxlive.com`, `TokenType: JWT`. Keep `Token` and `DisplayClaims.xui[0].uhs`.
3. **XSTS token.** `POST xsts.auth.xboxlive.com/xsts/authorize` with `SandboxId: RETAIL`, the user token and `RelyingParty: rp://api.minecraftservices.com/`. Keep `Token` and the claims' `xid` (the xuid). A 401's `XErr` maps to player-facing codes:

   | `XErr` | Meaning | Code |
   |---|---|---|
   | 2148916233 | No Xbox profile on this Microsoft account | `xbox_no_profile` |
   | 2148916235 | Xbox Live isn't available in the player's country | `xbox_region_blocked` |
   | 2148916236, 2148916237 | Adult verification needed (South Korea) | `xbox_age_verification` |
   | 2148916238 | A child account, which must be added to a Microsoft family | `xbox_child_account` |

4. **Minecraft token.** `POST api.minecraftservices.com/authentication/login_with_xbox` with `identityToken: XBL3.0 x=<uhs>;<xsts token>`. Keep `access_token` and `expires_in`.
   - A **403** here is `microsoft_awaiting_approval`: Mojang has not approved the client ID.
   - 429 is `microsoft_rate_limited`. Other failures are the existing network codes.
5. **Ownership.** `GET …/entitlements/mcstore` and `GET …/minecraft/profile`, with the Minecraft token.
   - Profile 200: the account owns Java Edition (Game Pass included) and has a name. It is accepted.
   - Profile 404 with an entitlement for `game_minecraft` or `product_minecraft`: the player owns the game but has not chosen a name. `minecraft_no_profile` says to set one up on minecraft.net, and the account is not added.
   - Profile 404 with no entitlement: `minecraft_not_owned`. The account is not added.
6. **Store.** The account file gets `{provider: microsoft, username, uuid}` and the account becomes active. Secrets go to the OS credential store (below).

Cancel, a second sign-in, a closed browser and the deadline behave like Perland sign-in: `Ok(None)`, no error. Events to the UI reuse the Perland sign-in phases (waiting, exchanging) under a Microsoft variant, and each call of the chain is reported as it starts (`microsoft_sign_in_step`: `microsoft`, `xbox`, `xsts`, `minecraft`, `profile`). The window lists the five steps with their state while they run, and keeps the list after a refusal so the player sees what passed and where it stopped. A canceled sign-in stops reporting.

*(My call.)* The browser page shown after Microsoft's step does not say "signed in": the Xbox, ownership and approval checks still follow. It gets its own wording in English and Spanish: "Go back to Perland Launcher to finish signing in. You can close this tab." The Perland pages stay as they are.

**Knowing approval is pending.** The only signal is the 403 at step 4, and the launcher remembers nothing of it. *(Owner's call, 2026-10-27, replacing an earlier memo of the 403.)*
- Every sign-in runs the whole chain: the browser, Microsoft's token, Xbox Live, XSTS and Minecraft's login. Nothing skips a step because an earlier attempt was refused, and nothing is said up front.
- A 403 shows the steps that passed (Microsoft, Xbox Live, XSTS) and Minecraft's step as "waiting on approval", with "Minecraft hasn't approved Perland Launcher yet, so it turned the sign-in down. Your Microsoft account is fine: you'll be able to play as soon as it's approved." It is info, not a fault, and the key stays "Sign in with Microsoft".
- Nothing is polled.

### Secrets

- *(My call, after review.)* Only the Microsoft **refresh token** is kept across restarts, in the OS credential store under service `online.perland.launcher`, user `msa:<uuid>`. The Minecraft token, its expiry and the xuid live in memory only, so the first launch after a restart runs the refresh chain. That keeps the stored secret small: Windows Credential Manager caps a credential at 2560 bytes.
- If a refresh token is still too large for one credential on a platform, it is split across numbered entries (`msa:<uuid>:0`, `:1`, …) and joined on load. The builder measures real tokens and decides whether this is needed. It stays in the OS store either way, never in a file.
- `SecretStore` grows a key parameter, or one store is made per key. The Perland session keeps its one entry.
- `MemoryStore` stays the fallback where there is no credential store, so such a player signs in again after each restart.
- Removing an account deletes its entry.
- No secret is ever placed in the accounts file, a log line, `LauncherError` detail, a UI event or a crash report. `Session`'s `Debug` already redacts the token. `launch/redact.rs` must also cover the Minecraft token on the game's command line.

### Launching

`runs.rs`, where a run takes the active account:

- **No active account, or an active offline account an earlier beta left in the file:** `no_account`, in every build. The UI answers it with the sign-in prompt, not an error card.
- **A Microsoft account, before every launch:**
  1. Load its refresh token. If it is missing, the player signs in again (`microsoft_sign_in_again`).
  2. If there is no Minecraft token in memory, or it has less than an hour left, run the refresh chain: Microsoft refresh grant, then steps 2 to 4. Store the rotated refresh token.
  3. Always `GET /minecraft/profile` with the token. This is the online check: it proves the token works and the game is still owned, and it picks up a changed name.
     - 401: refresh once and try again.
     - Still refused, or the refresh is refused (`invalid_grant`): `microsoft_sign_in_again`.
     - Any network failure: `offline`, and the game does not start.
  4. Build `Session { username, uuid, access_token: minecraft token, user_type: "msa", xuid }` and pass the client ID as `clientid`.

Every path to Play goes through this one place, which stays the gate that counts. A test checks each entry point refuses without an account.

*(My call, after review.)* Flows that do real work before they reach Play ask first, so nobody downloads a modpack or wakes a server only to be refused at the end. These are the join flow (creates an instance and installs the pack), the free server (`usePlayFreeServer` starts it) and the public server card (joins, then plays).
- They call a cheap command, `account_ready`: whether the active account is a Microsoft account with a stored refresh token. It makes no network call.
- When it says no, they show the sign-in prompt before doing anything.
- This check is a convenience: the launch check above still decides.

### Accounts and the old offline accounts

- An earlier beta's `accounts.json` may hold offline accounts. `Provider::Offline` stays in the file format only so such a file still parses. `visible` leaves those accounts out in every build, and an `active` naming one reads as none. The entries stay in the file, unread.
- Nothing adds an offline account: there is no `add_offline`, no `account_add_offline` command, and the `offline_locked` and `offline_name_invalid` codes are retired.
- The import of other launchers' instances already leaves out accounts and credentials. A test pins that it still does.
- `app_info` has no `offline_allowed`. The contract test is regenerated.

### UI

- **First run, account step**, in every build: "Sign in with Microsoft" as the one primary action. A short line says why: the account must own Minecraft: Java Edition. "Skip" goes on to the first instance; Play will ask again.
- **Accounts:** "Add Microsoft account" replaces the "coming soon" placeholder, and it is the only way to add an account. Each Microsoft account shows its name, the generated avatar and Remove.
- **Play without a signed-in owner:** the run is refused with `no_account` or `microsoft_sign_in_again`. *(My call.)* The global error handler (`pushError`) recognises those two codes and opens one shared sign-in prompt instead of an error card, so every place that plays gets it: Home, ContinueKey, InstanceMenu, PublicServerCard, the free server, CrashFixes repair and ManualDownloads. The prompt starts the same flow. Once signed in, the player presses Play again; nothing starts by itself.
- **Players coming from an older beta:** offline accounts are dropped silently *(owner's call, 2026-10-27, replacing an earlier "offline accounts are gone" notice)*. The accounts they made simply aren't listed, nothing says they existed, and Play asks for a Microsoft sign-in like it does for anyone without an account. The accounts file keeps the entries, unread. First run is not shown again.
- **Waiting on approval:** `microsoft_awaiting_approval` is worded "Minecraft hasn't approved Perland Launcher yet, so it turned the sign-in down. Your Microsoft account is fine: you'll be able to play as soon as it's approved." It has no fix button, because there is nothing for the player to do.
- **New `ErrorCode`s:** `microsoft_not_configured`, `microsoft_awaiting_approval`, `microsoft_sign_in_again`, `microsoft_rate_limited`, `minecraft_not_owned`, `minecraft_no_profile`, `xbox_no_profile`, `xbox_region_blocked`, `xbox_age_verification`, `xbox_child_account`. Each gets `errors.<code>.message` (and `fix` where the player can act) in English and Spanish.
- **Retired strings:** `MicrosoftUnavailable` ("Microsoft accounts arrive in a later version") and the "coming soon" strings go.

### Wording outside the app

- README and `PRODUCT.md`: the launcher needs a Microsoft account that owns Minecraft: Java Edition, in every build. Remove "Microsoft sign-in comes in a later plan".
- The two older specs (`2026-10-09-launcher-core-design.md`, `2026-10-10-launcher-desktop-ui-design.md`) get one line each pointing to this document where they describe the offline gate. Their history is not rewritten.
- `CLAUDE.md` gets a line on `PERLAND_MS_CLIENT_ID` and on there being no offline accounts.

### The CLI (`perland-cli`)

*(My call.)* It is a dev harness and is not distributed, so it gets no Microsoft sign-in, and with offline accounts gone it no longer starts the game. It creates instances, adds and syncs packs, and `install` gets an instance ready to play (files and Java) without an account; Nightly uses `install` for its real-install checks. It stops using the retired `MicrosoftUnavailable` code.

### Azure app registration (the owner, outside the code)

- The app name must not contain Mojang, Minecraft, Microsoft, Live, Xbox, Discord or Hypixel.
- Supported accounts: personal Microsoft accounts (the `consumers` endpoint).
- A "Mobile and desktop applications" platform with the loopback redirect the launcher sends, `http://127.0.0.1:<port>/callback` (`Loopback::redirect_uri`). Microsoft ignores the port for loopback addresses. Confirmed against Microsoft's "Redirect URI (reply URL) best practices and limitations" (learn.microsoft.com/entra/identity-platform/reply-url, dated 2025-05-14): it recommends `127.0.0.1` over `localhost`, and the portal's Redirect URIs box refuses an `http` loopback address, so `http://127.0.0.1/callback` is added in the app manifest (`replyUrlsWithType`, type `InstalledClient`; `publicClient.redirectUris` in the Microsoft Graph format). README → Releasing → Azure app registration has the steps.
- Allow public client flows.
- Homepage and privacy policy URLs. The privacy policy is a page on perland.online; writing it is not part of this work.

## Left to the builder

- Module layout inside `launcher-core` (`msa/` with `oauth.rs`, `xbox.rs`, `minecraft.rs`, or one file) and how the base URLs are injected for wiremock.
- Whether `MemoryStore` or `KeyringStore` is chosen per platform at startup, following `perland/session.rs`.
- How the Tauri command surface is named (`account_sign_in_microsoft`, `account_sign_in_cancel`, …), matching the existing `perland_sign_in_*` commands.
- The exact refresh margin (an hour is the intent) and how concurrent launches share one refresh. One refresh at a time per account is the intent.
- The visual design of the sign-in prompt, through `/impeccable`.
- The mock backend (`pnpm --filter ui dev:mock`) answering the new commands, including a `microsoft_awaiting_approval` state that runs every step and is refused at Minecraft's.
