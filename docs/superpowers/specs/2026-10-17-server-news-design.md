# Perland Launcher: Server News on the Card, Design

- **Date:** 2026-10-17
- **Issue:** PerlandDev/perland-launcher#51, part of the Perland integration roadmap (#47; snapshot `docs/superpowers/specs/2026-10-16-perland-integration-ideas.md`).
- **Needs (merged):** PerlandDev/PerlandEngine#432 through PerlandDev/PerlandEngine#436 (server news). The contract is `docs/superpowers/specs/2026-10-16-server-news-design.md` in PerlandEngine; this spec never changes it.
- **Builds on:**
  - `docs/superpowers/specs/2026-10-17-public-server-list-design.md` (#50, merged in PerlandDev/perland-launcher#54): the Servers screen and its cards, `PublicListClient`, the calm closed state;
  - `docs/superpowers/specs/2026-10-11-join-by-link-design.md`: server instances, `InviteClient`, Home's server panel;
  - `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner brainstormed the idea with Claude on 2026-10-16 and pre-approved this session's recommendations. Every decision the issue does not settle is marked **our call**.

## 1. Why

Players hear about a server's events and updates through Discord or not at all. Owners now write news in the panel (#432), and a modpack change writes its own post. The launcher should show that news where the player already looks: on the server's card and on its page, with no separate news tab. The bar is that a 12-year-old gets it without reading anything.

## 2. Outcome

- **The card:** a server card (Servers screen) and a server instance's Home both show the server's latest news in one line. A small dot marks something the player hasn't seen yet.
- **Events:** an event post shows its date and time ("Saturday 8 pm") until it is over.
- **All the news:** pressing the news line opens the server's news, a dialog with every post: title, short text, optional picture, and the date or event time. Opening it marks the news as seen, and the dot goes away.
- **Modpack posts:** the automatic post reads in the player's language, for example "Modpack updated: 3 mods added, 1 removed".
- **News off on the Engine:** while the Engine has news off (the default) or is older, nothing about news shows. No error, no empty box.

## 3. Goals, non-goals, anti-goals

- **Goals:**
  - the latest-news line with its new dot, on public cards and on a server instance's Home;
  - event date and time until over;
  - the news dialog with pictures and paging;
  - modpack posts in the player's language;
  - hidden when news is off;
  - every string in en and es;
  - working in `dev:mock`.
- **Non-goals (issue):** comments, likes, notifications while the launcher is closed, a news tab.
- **Further non-goals:**
  - read state on the Engine (the Engine leaves "seen" to the launcher);
  - a dot on the rail;
  - news for networks (the launcher joins stacks only);
  - writing news (panel, PerlandDev/PerlandPanel#206);
  - news inside the game (#433).
- **Anti-goals:**
  - a feed that competes with Play;
  - an error or an empty "No news" box on servers without news or with news off;
  - a second lit button;
  - rendering a post's text as HTML or Markdown (it is plain text);
  - an invite token in logs, errors or toasts.

## 4. Facts this rests on (PerlandEngine `main`, d233a31)

**The public route**
- `GET <engine>/api/v1/invites/{token}/news?limit=&before=` is public (the token is the credential).
- **Answer:** `{posts, nextBefore?}`, newest first, standard envelope.
- **Paging:** `limit` 1-50, default 20; `before` is the previous page's `nextBefore`, absent on the last page.
- **Caching and limits:** `Cache-Control: public, max-age=60`; 60 requests a minute per IP (429, no `Retry-After`).
- **Errors:**
  - `503` with `data.code: news_disabled` while `NEWS_ENABLED` is off, or without a database;
  - `404` (fixed `not found`) for an unknown or disabled token;
  - an older Engine has no route at all, also `404`;
  - `400` `invalid_news_post` for a bad `limit` or `before`.

**A post**
- **Fields:**
  - `id`;
  - `type`: `update`, `event` or `modpack`; any other value is treated as `update` (our call);
  - `title` (1-100 characters);
  - `body` (0-1000 characters, plain text with newlines);
  - `startsAt?` and `endsAt?` (events);
  - `imageVersion?`;
  - `modpack?`, which is `{fromCommit, toCommit, changes?: {added, removed, updated: {count, names[≤50]}}}`;
  - `createdAt` and `updatedAt`.
- The public route never sends `createdBy`.
- **Events:** an event is **over** at `endsAt`, or 3 hours after `startsAt` without one (Engine spec).
- **Modpack posts:** they carry an English fallback title and body. Clients build their own text from `modpack.changes`; without `changes`, "The modpack changed."
- **Picture:** `GET <engine>/api/v1/invites/{token}/news/{postId}/image`. A PNG or JPEG of at most 1280 px, `ETag "<imageVersion>"`, `max-age=300`, not rate limited, 404 without one.
- **Who can read:** any enabled invite, listed or not. So every server instance (it holds the invite) and every public card (it holds the token) can read its news.

## 5. Decisions

| Question | Decision |
|---|---|
| Where news shows | On each public card (Servers screen) and on a server instance's Home: the issue's "card and its page". No tab, no rail dot. **Our call** on Home being "its page": a public server has no page of its own, and Home is where a joined server lives |
| What the line shows | The **latest** post, one line under the server's name or status:<br>- an event that isn't over: its title plus its time, as in "Build contest · Sat 8 pm" ("Now" once started);<br>- otherwise the latest post's title;<br>- a modpack post: the localized summary.<br>If the newest post is not an event but an event is coming up or under way, the event wins: it's what a player can still act on. **Our call** |
| How the event time reads | Today: "Today 8 pm"; tomorrow: "Tomorrow 8 pm"; within 6 days: weekday and time ("Sat 8 pm"); later: date and time ("Oct 30, 8 pm"); started and not over: "Now". The time is in the player's computer time zone, through `Intl.DateTimeFormat` in the UI language (en or es) with a 12- or 24-hour clock as the locale says. **Our call** |
| The new dot | A small dot (the `blue` info hue, never amber, with a text alternative "New") when the newest post's `createdAt` is after this server's **seen marker**. The marker is the `createdAt` of the newest post the player had when they last opened the news. It is Engine time, not the computer's clock, so a computer clock that is wrong can't hide new posts or keep a stale dot (builder check). A server whose news was never opened shows the dot only for a newest post from the last 7 days by the computer's clock, so the first visit isn't a wall of dots. **Our call** |
| Where "seen" is kept | The launcher's own data folder, `news-seen.json`: a map from canonical invite to the seen marker (an RFC3339 time), at most 500 entries (the oldest go). The core reads and writes it. It isn't kept in UI storage, so it survives a WebView reset. **Our call** |
| The dialog | The existing `Dialog`, titled with the server's name. Posts newest first, each with:<br>- title;<br>- event time (or "Posted Oct 16" for others);<br>- picture when there is one (max 100% width, never upscaled);<br>- body as plain text with its line breaks.<br>20 at a time, with **Show more** while `nextBefore` exists. No lit button: Close is ghost. **Our call** |
| Pictures | Fetched through the core (`news_image(invite, postId, imageVersion)` → data URL), at most 2 MiB, PNG or JPEG signature checked, cached in memory per (invite, post, version), 64 entries. The UI asks only when `imageVersion` is present. **Our call** |
| Modpack text | Built in the UI from `modpack.changes` with i18n plurals:<br>- title: "Modpack updated";<br>- line: "3 mods added, 1 removed, 2 updated" (parts with 0 left out);<br>- in the dialog: up to 10 names per part, then "and N more".<br>Without `changes`: "The modpack changed." The Engine's English fallback is never shown in es |
| News off, an older Engine, a gone invite, no posts | `503 news_disabled`, `404`, or an empty `posts` all mean "no news": the line is not rendered at all, and no dialog can open. Offline or 429 also hide the line for now (a later poll fills it), with no toast: news is a bonus, never a blocker. Only the dialog's own **Show more** shows an inline error with Retry. **Our call**, following the owner's release requirement |
| How often it's read | One page of 20 (the newest posts; "event wins" is best effort within them) per server, and only while its card is **on screen** (an IntersectionObserver) or its Home is open. A read is kept for 5 minutes (`staleTime` and `refetchInterval`), so revisiting the screen doesn't read again.<br>The Engine's public news limit is 60 a minute **per IP across all servers**, and a page holds up to 50 cards (builder check). So the core also shares one request per server (concurrent callers wait for it), remembers its answer for 60 s, and after any 429 answers "no news" for 60 s without calling the Engine. The dialog reads pages of 20. **Our call** |
| Network instances | Never asked: the launcher only joins stacks |

## 6. Design

### 6.1 launcher-core

**`news` module (new, `crates/launcher-core/src/news.rs`)**

- **Mirror types:** `NewsPost`, `NewsModpack` and `NewsChanges` are serde mirrors of `PostDTO`. `NewsPage { posts, next_before: Option<String> }`. Every optional field has a default, unknown fields are ignored, and the `Debug` impls print no token.
- **`NewsClient`:** one per process, sharing the `InviteClient` (its HTTP client and Engine discovery).
  - `page(link: &InviteLink, limit: u8, before: Option<&str>) -> Result<Option<NewsPage>>`:
    - `Ok(None)` means "no news here": a 404, or a 503 (with or without `data.code: news_disabled`);
    - one shared in-flight request per invite, its answer remembered for 60 s (memory only);
    - after a 429 it answers `Ok(None)` for 60 s without a request (the line just hides), and returns `invite_rate_limited` only to the dialog's **Show more** (`before` set);
    - the body is capped at 256 KiB;
    - `before` must be ASCII digits, or it isn't sent.
  - `image(link, post_id, image_version) -> Option<Vec<u8>>`: at most 2 MiB, the PNG or JPEG signature checked, cached (64 entries), never an error. `post_id` must be a UUID, or nothing is requested.
  - **Errors** carry the Engine's host only:
    - 429 → `invite_rate_limited`;
    - 5xx other than `news_disabled` → `host_trouble`;
    - offline → `offline`;
    - a body too large or invalid → `meta_unexpected`.
- **`NewsSeen`** (`news_seen.rs`) reads and writes `<data>/news-seen.json`:
  - `seen(canonical) -> Option<OffsetDateTime>`;
  - `mark_seen(canonical, newest_created_at)`, which only moves forward;
  - atomic writes (the existing `fsutil` helper), 500 entries max.

### 6.2 launcher-app

| Command | Returns | Notes |
|---|---|---|
| `server_news(invite, before, limit)` | `ServerNews \| null` | `{ posts: NewsPostView[], nextBefore, seen: string \| null }`. `null` when there's no news here. `invite` is a canonical invite URL (a card's `invite`, or a server instance's `source.invite`). `limit` is clamped to 1-50. `seen` comes from `NewsSeen` |
| `server_news_opened(invite, newest)` | `null` | Moves the seen marker forward to `newest`, the newest post's `createdAt` the dialog showed |
| `news_image(invite, postId, imageVersion)` | `string \| null` | A data URL, or null |

- **`NewsPostView`:** `{ id, type, title, body, startsAt, endsAt, imageVersion, modpack: {added, removed, updated: {count, names}} \| null, createdAt }`. `modpack` is `null` without `changes`. The `type` of an unknown post is `update`.
- **Implementation:** the three commands go in `crates/launcher-app/src/news.rs`, and the `Runner` gets `news_page` and `news_image` so tests use fakes.
- **Contract:** fixtures `server_news.json` (an event, an update with a picture, a modpack post with changes, a modpack post without) and the request shapes, written with `UPDATE_CONTRACT=1`.

### 6.3 UI (through `/impeccable`)

- **`useServerNews(invite, enabled)`:** `useQuery(['news', invite], limit 20)`, `staleTime` and `refetchInterval` 5 min, `enabled` only while on screen (cards) or open (Home), `retry: false`, and no `onError` toast. Errors and `null` render nothing.
- **`NewsLine({ invite, name })`:**
  - the latest-news line: the dot, the text, and the event time;
  - a button whose accessible name is "News: {{line}}" (plus ", new" when the dot shows);
  - pressing it opens `NewsDialog`;
  - rendered on `PublicServerCard` (under the status) and in Home's `ServerPanel` (under the status pill and address).
- **`NewsDialog`:**
  - pages of 20 through `useInfiniteQuery(['newsAll', invite])`;
  - calls `server_news_opened(invite, newest createdAt)` when it opens, then invalidates `['news', invite]` so the dot clears;
  - each post is a plain-text `<p>` with `white-space: pre-line`, so a body is never parsed as HTML or Markdown.
- **Helpers (pure):**
  - `eventLabel(post, now, t, locale)`;
  - `isOver(post, now)`;
  - `latestLine(posts, now)`, which picks the post the line shows;
  - `isNew(posts, seen, now)`;
  - `modpackSummary(changes, t)`.
  Each gets Vitest unit tests.
- **Mock:**
  - `mockServers.ts` gives each public card and the seeded server instance a few posts: an upcoming event, an update with a picture, and a modpack post;
  - one card has none;
  - a mock scenario `news-off` answers `null` everywhere.
- **i18n:**
  - `news.*`: line, dialog, "Show more", "New", "Now", "Today", "Tomorrow", "Posted {{date}}";
  - modpack plurals: `news.modpack.added_one` / `_other`, and so on;
  - en and es (neutral Latin American, *tú*).

## 7. Testing

- **launcher-core**, wiremock in `tests/news.rs`:
  - a page parses, minimal and full;
  - `before` passes through;
  - 503 `news_disabled`, 404 and an empty list all give `Ok(None)`;
  - 429, 500, oversize and invalid bodies map to their codes;
  - no error names the token;
  - the picture checks its signature, its cap, a 404 and its cache;
  - a bad post id makes no request.
- **launcher-core**, unit tests: `NewsSeen` round-trips, caps at 500, and survives a corrupt file (treated as empty).
- **launcher-app:**
  - `server_news` maps posts;
  - `seen` comes from the file;
  - `server_news_opened` moves it forward only;
  - a `null` page stays `null`.
- **UI:**
  - the helpers (event labels across today, tomorrow, the week and later, "Now", over-ness with and without `endsAt`, the latest-line choice, the new-dot rules, modpack plurals in en and es);
  - `NewsLine` renders on a card and on Home and is absent with news off;
  - the dialog lists posts, pages, marks opened and clears the dot;
  - a body with `<b>` renders as text;
  - no toast with news off.

## 8. Owner to do

- Set `NEWS_ENABLED=true` on the production Engine (PerlandEngine#436's to-do). Until then no news shows, by design.
- Write a post and an event for a listed server in the panel once PerlandDev/PerlandPanel#206 ships, then check the line, the dot and the dialog.

## 9. Left to the builder

- File and component names inside the stated modules.
- The dot's exact look and the dialog's layout (through impeccable).
- The test structure.
