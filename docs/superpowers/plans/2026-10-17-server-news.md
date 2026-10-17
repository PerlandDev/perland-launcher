# Server News on the Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A server's latest news shows on its card and on its Home, with a new dot and event times, and opens a news dialog. Nothing shows while the Engine has news off (PerlandDev/perland-launcher#51).

**Architecture:**
- **Core:** `launcher-core` gains a `news` client (Engine discovery through the existing `InviteClient`) with a short memo and a 429 back-off. It also gains a `news_seen` store for the seen markers.
- **App:** `launcher-app` adds three commands.
- **UI:** pure helpers decide the line, the dot and the event label. A `NewsLine` and a `NewsDialog` render on `PublicServerCard` and on Home's `ServerPanel`.

**Tech Stack:** Rust (tokio, reqwest, serde, wiremock tests), Tauri 2, React 19 + TanStack Query + zod, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-17-server-news-design.md` (read it first). The Engine contract is `docs/superpowers/specs/2026-10-16-server-news-design.md` in PerlandEngine (read-only clone at `/home/user/perlandengine`); see also `internal/minecraft/model/DTOs/news/news.go` and `internal/minecraft/news/modpack.go`.

## Global Constraints

- **Crate boundaries:** `launcher-core` and `launcher-app` never depend on Tauri or UI crates; `src-tauri` is wiring only, with no tests.
- **TDD:** write the failing test first.
  - Pure logic: `#[cfg(test)]` unit tests.
  - HTTP behaviour: `crates/launcher-core/tests/*.rs` with wiremock.
  - UI: Vitest + Testing Library.
- **Local tests:** keep inputs tiny; no fuzz, stress or load tests.
- **Strings:**
  - Every UI string goes in both `ui/src/i18n/en.json` and `es.json`: neutral Latin American Spanish, informal *tú*.
  - A new `ErrorCode` needs `errors.<code>.message` in both. This plan adds none.
- **Contract:** `ui/src/ipc/contract/*.json` is generated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`; never hand-edit it.
- **Tokens:** invite tokens never appear in a `Debug` impl, a log, an error message, a param or a detail. Errors carry the Engine host only.
- **Engine route:** `GET <engine>/api/v1/invites/{token}/news?limit=&before=`.
  - "No news here" (`Ok(None)`): 404, or 503 with or without `data.code: news_disabled`.
  - Line page: limit 20. Dialog pages: limit 20. The `limit` argument is clamped to 1-50.
  - Body cap 256 KiB. `before` is sent only when it is ASCII digits.
- **Memo and back-off:**
  - one in-flight request per invite;
  - its answer is remembered for 60 s;
  - after any 429, for 60 s: first pages answer `Ok(None)` without a request, and a page with `before` set answers `invite_rate_limited`.
- **Picture:** `GET <engine>/api/v1/invites/{token}/news/{postId}/image`.
  - At most 2 MiB.
  - The signature must be PNG (`\x89PNG\r\n\x1a\n`) or JPEG (`\xFF\xD8\xFF`).
  - Cache of 64 entries, keyed by (canonical invite, post id, image version).
  - A post id that isn't a UUID makes no request.
- **Event rules:**
  - An event is over at `endsAt`, else `startsAt` + 3 h.
  - The line shows the newest event that isn't over; if there is none, the newest post.
  - Labels: "Now" once started; "Today {time}"; "Tomorrow {time}"; within 6 days, "{weekday} {time}"; later, "{date}, {time}". Built with `Intl.DateTimeFormat` in the UI language.
- **New dot:**
  - newest `createdAt` > seen marker;
  - with no marker, only when the newest `createdAt` is within 7 days of now;
  - the marker only moves forward;
  - the dot uses the `blue` info hue, never amber, and its text alternative is "New" / "Nuevo".
- **Seen file:** `<data root>/news-seen.json`, at most 500 entries (the oldest marker goes first), written atomically with `fsutil::write_atomic`. A corrupt file reads as empty.
- **Plain text:** a post body renders as plain text (`white-space: pre-line`), never as HTML or Markdown.
- **News off:** news off, 404, no posts, offline and 429 all render nothing: no toast, no error, no empty box.
- **Before every commit**, run the CLAUDE.md "Before committing" checks for what the task touched. Task 6 runs the full list.
- **Commit trailers** (end every commit message with):
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01Jhqr9QaewJmCwnRRyNsTJ9
  ```
- **Styling:** all UI styling goes through the `/impeccable` skill against `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.

## Review Focus

1. **News off in a released build** (503 `news_disabled`, or 404 from an older Engine): no line, no dialog, no toast, no log noise. Tests: Task 1 `no_news_is_none`; Task 5 `nothing shows with news off`.
2. **A page of 50 cards against the shared limit** of 60 a minute per IP: only on-screen cards ask, and a 429 backs everything off quietly. Tests: Task 1 `a_rate_limit_backs_off_quietly`; Task 5 `only cards on screen ask for news`.
3. **A computer clock that is wrong:** the seen marker is Engine time, so opening the news clears the dot whatever the clock. Tests: Task 2 `the_marker_only_moves_forward`; Task 4 `isNew uses the marker, not the clock`.
4. **A post body holding `<b>` or `[link](x)`:** it is shown literally. Test: Task 5 `a body is plain text`.
5. **Spanish and the 960 × 600 window:** the card's news line truncates with an ellipsis on one line and never pushes Play off the card. Check: Task 5's impeccable finish review at 960 × 600 in es.

---

### Task 1: Core `news` client

**Files:**
- Create: `crates/launcher-core/src/news.rs`
- Create: `crates/launcher-core/tests/news.rs`
- Modify: `crates/launcher-core/src/lib.rs`
- Modify: `crates/launcher-core/src/invite/client.rs` (only if a helper must become `pub(crate)`)

**Interfaces:**
- Consumes: `InviteClient::engine_for(&InviteLink)`, `invite_route(&Url, &InviteLink, suffix)`, and the error helpers in `invite/client.rs` (`request_failed`, `rate_limited`, `host_trouble`, `host_of`), plus `read_capped`.
- Produces:
  ```rust
  #[derive(Clone, Default, Deserialize)] #[serde(rename_all = "camelCase", default)]
  pub struct NewsPost { pub id: String, #[serde(rename = "type")] pub kind: String, pub title: String,
      pub body: String, pub starts_at: Option<String>, pub ends_at: Option<String>,
      pub image_version: Option<String>, pub modpack: Option<NewsModpack>, pub created_at: String }
  #[derive(Clone, Default, Deserialize)] #[serde(default)] pub struct NewsModpack { pub changes: Option<NewsChanges> }
  #[derive(Clone, Default, Deserialize)] #[serde(default)] pub struct NewsChanges { pub added: NameList, pub removed: NameList, pub updated: NameList }
  #[derive(Clone, Default, Deserialize)] #[serde(default)] pub struct NameList { pub count: u32, pub names: Vec<String> }
  #[derive(Clone, Default)] pub struct NewsPage { pub posts: Vec<NewsPost>, pub next_before: Option<String> }
  #[derive(Clone)] pub struct NewsClient;
  impl NewsClient {
      pub fn new(invites: InviteClient) -> Self;
      pub async fn page(&self, link: &InviteLink, limit: u8, before: Option<&str>) -> Result<Option<NewsPage>>;
      pub async fn image(&self, link: &InviteLink, post_id: &str, image_version: &str) -> Option<Vec<u8>>;
  }
  ```
  Each type's `Debug` holds no token (a post holds none, but `NewsClient` caches keys by invite).

- [ ] **Step 1: Write the failing tests** in `tests/news.rs`. Follow `tests/public_list.rs`: wiremock, a `config.json` route, fake tokens of `'A'` × 43.
  - `parses_a_full_and_a_minimal_page`:
    - The full page is the Engine spec's example post, plus a modpack post with `changes`.
    - The minimal page is `{"posts":[{"id":"…uuid…","type":"update","title":"t","createdAt":"2026-10-16T18:00:00Z"}]}`.
  - `limit_and_before_pass_through`:
    - `page(link, 20, Some("41"))` hits `/news?limit=20&before=41`;
    - `limit` 0 becomes 1 and 99 becomes 50;
    - a `before` of `"4x"` is not sent.
  - `no_news_is_none`: a 404, a 503 with `{"success":false,"data":{"code":"news_disabled"}}`, and a bare 503 each give `Ok(None)`.
  - `troubles_map_to_codes`: a 500 → `HostTrouble`, a body over 256 KiB → `MetaUnexpected`, a body that isn't JSON → `MetaUnexpected`.
  - `a_page_is_shared_for_a_minute`: two `page` calls within the memo time make one request. With a test seam (`NewsClient::with_timing(memo, backoff)`) and a 50 ms memo, a call after the memo makes a second request.
  - `a_rate_limit_backs_off_quietly`:
    - a 429 on a first page → `Ok(None)`;
    - the next first page within the back-off makes no request and is `Ok(None)`;
    - a `before` page within the back-off → `InviteRateLimited`.
  - `no_error_names_the_token`.
  - `image_checks_signature_size_and_caches`:
    - a PNG → `Some`, and a second call makes no request;
    - a JPEG → `Some`;
    - a GIF → `None`;
    - a 404 → `None`;
    - a body over 2 MiB + 1 → `None`;
    - post id `"../x"` → `None` with no request.
- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-core --test news`. Expected: compile failure.
- [ ] **Step 3: Implement** `news.rs` as above:
  - The memo is `Mutex<HashMap<canonical, (Instant, Option<NewsPage>)>>` for first pages only. Pages with `before` are never memoized.
  - The in-flight sharing uses one `tokio::sync::Mutex` slot per canonical invite, like `InviteClient::view_at`.
  - The back-off is a `Mutex<Option<Instant>>`.
  - The image cache is a `VecDeque` of 64 entries.
- [ ] **Step 4: Run.** Run `cargo test -p launcher-core`, `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit.** Commit `feat(core): read a server's news (#51)`.

### Task 2: Core `news_seen` store

**Files:**
- Create: `crates/launcher-core/src/news_seen.rs` (with `#[cfg(test)]` tests)
- Modify: `crates/launcher-core/src/lib.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const NEWS_SEEN_FILE: &str = "news-seen.json";
  pub const MAX_SEEN: usize = 500;
  pub struct NewsSeen; // holds the file path and a tokio Mutex
  impl NewsSeen {
      pub fn new(path: PathBuf) -> Self;
      pub async fn seen(&self, canonical: &str) -> Option<String>; // RFC3339
      pub async fn mark_seen(&self, canonical: &str, newest_created_at: &str) -> Result<()>;
  }
  ```

- [ ] **Step 1: Write the failing unit tests.**
  - `a_marker_round_trips`.
  - `the_marker_only_moves_forward`:
    1. mark `2026-10-16T18:00:00Z`;
    2. mark `2026-10-10T00:00:00Z`;
    3. `seen` is still the first.
  - `an_invalid_time_is_ignored`: `"soon"` writes nothing.
  - `the_oldest_markers_go_past_the_cap`: with `MAX_SEEN` as a constructor-time override of 3 for the test, a 4th server drops the oldest marker.
  - `a_corrupt_file_reads_as_empty`: the file holds `{nope`; `seen` is `None`, and `mark_seen` rewrites the file valid.
- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-core news_seen`. Expected: compile failure.
- [ ] **Step 3: Implement.** Store a JSON object `{canonical: rfc3339}`. Compare times by parsing with the crate's existing time library (check `Cargo.toml`; `time` or `chrono`, whichever the core already uses). Write with `fsutil::write_atomic`.
- [ ] **Step 4: Run.** Run `cargo test -p launcher-core`, plus fmt and clippy. Expected: PASS.
- [ ] **Step 5: Commit.** Commit `feat(core): remember which server news was seen (#51)`.

### Task 3: App commands and Tauri wiring

**Files:**
- Create: `crates/launcher-app/src/news.rs` (with `#[cfg(test)]` tests)
- Modify:
  - `crates/launcher-app/src/lib.rs`;
  - `runner.rs` (`news_page` and `news_image` on the trait and on `CoreRunner`; `CoreRunner` builds `NewsClient::new(syncer.sources.invites.clone())`);
  - `testing.rs` (`FakeRunner` gets settable answers and records calls);
  - `app.rs` (`AppCore` owns a `NewsSeen` at `config.dirs.root().join(NEWS_SEEN_FILE)`);
  - `dto.rs`;
  - `tests/contract.rs`;
  - `src-tauri/src/commands.rs` and `src-tauri/src/main.rs`.

**Interfaces:**
- Consumes: Task 1's `NewsClient` and `NewsPage`, Task 2's `NewsSeen`, and `InviteLink::parse`/`canonical`.
- Produces:
  - **DTOs** (`Serialize`, camelCase):
    ```rust
    pub struct ServerNews { pub posts: Vec<NewsPostView>, pub next_before: Option<String>, pub seen: Option<String> }
    pub struct NewsPostView { pub id: String, #[serde(rename = "type")] pub kind: String /* update|event|modpack; unknown → update */,
        pub title: String, pub body: String, pub starts_at: Option<String>, pub ends_at: Option<String>,
        pub image_version: Option<String>, pub modpack: Option<NewsChangesView> /* None without changes */, pub created_at: String }
    pub struct NewsChangesView { pub added: NameListView, pub removed: NameListView, pub updated: NameListView }
    pub struct NameListView { pub count: u32, pub names: Vec<String> }
    ```
  - **`AppCore`:**
    - `server_news(&self, invite: &str, before: Option<String>, limit: u8) -> Result<Option<ServerNews>>`;
    - `server_news_opened(&self, invite: &str, newest: &str) -> Result<()>`;
    - `news_image(&self, invite: &str, post_id: &str, image_version: &str) -> Option<String>` (a data URL whose MIME type is chosen by the signature).
  - **Tauri commands:**
    - `server_news(invite, before, limit)`;
    - `server_news_opened(invite, newest)`;
    - `news_image(invite, postId, imageVersion)`.
  - An `invite` that doesn't parse → `server_news` is `Ok(None)`; `server_news_opened` is the existing `invite_link_invalid` error.

- [ ] **Step 1: Write the failing tests** in `news.rs`:
  - `posts_map_to_views` (unknown type → update; modpack without changes → `modpack: None`);
  - `seen_comes_from_the_file`;
  - `opened_moves_the_marker`;
  - `no_news_stays_none`;
  - `a_bad_invite_is_no_news`;
  - `the_image_is_a_data_url` (PNG → `data:image/png;base64,`, JPEG → `data:image/jpeg;base64,`).
- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p launcher-app news`. Expected: compile failure.
- [ ] **Step 3: Implement.** Register the Tauri commands like `servers_icon`. In the contract, add `check("server_news", …)` with four posts:
  1. an event with `endsAt`;
  2. an update with an `imageVersion`;
  3. a modpack post with changes (3 added with names, 1 removed, 0 updated);
  4. a modpack post without changes.

  Use `nextBefore: "41"` and `seen: "2026-10-15T00:00:00Z"`. Regenerate with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.
- [ ] **Step 4: Run.** Run `cargo test -p launcher-app`, `cargo build -p perland-launcher`, fmt and clippy. Expected: PASS.
- [ ] **Step 5: Commit.** Commit `feat(app): server_news, server_news_opened and news_image commands (#51)`.

### Task 4: UI data layer, helpers, mock and strings

**Files:**
- Create:
  - `ui/src/features/news/newsRules.ts`;
  - `ui/src/features/news/newsRules.test.ts`.
- Modify:
  - `ui/src/ipc/schema.ts` (zod `serverNews`, `newsPostView`, `newsChangesView`);
  - `ui/src/ipc/commands.ts`;
  - `ui/src/ipc/contract.test.ts`;
  - `ui/src/state/queries.ts`;
  - `ui/src/ipc/mock.ts` and `ui/src/ipc/mockServers.ts`;
  - `en.json` and `es.json`.

**Interfaces:**
- **Commands:**
  - `commands.serverNews(invite: string, before: string | null, limit: number) => Promise<ServerNews | null>`;
  - `commands.serverNewsOpened(invite: string, newest: string) => Promise<null>`;
  - `commands.newsImage(invite: string, postId: string, imageVersion: string) => Promise<string | null>`.
- **Queries:**
  - `useServerNews(invite: string | null, enabled: boolean)`: key `['news', invite]`, `limit` 20, `staleTime` and `refetchInterval` 5 min, `retry: false`, enabled only with an invite and `enabled`;
  - `useServerNewsPages(invite, open)`: `useInfiniteQuery` keyed `['newsAll', invite]`, 20 a page, with `before` from `nextBefore`;
  - `useNewsImage(invite, postId, imageVersion)`: `staleTime: Infinity`, enabled only with an `imageVersion`.
- **Pure helpers** in `newsRules.ts`:
  ```ts
  export const EVENT_DEFAULT_MS = 3 * 60 * 60 * 1000;
  export function isOver(post: NewsPostView, now: Date): boolean;
  export function latestLine(posts: NewsPostView[], now: Date): NewsPostView | null; // newest not-over event, else newest post
  export function isNew(posts: NewsPostView[], seen: string | null, now: Date): boolean;
  export function eventLabel(post: NewsPostView, now: Date, t: TFunction, locale: string): string | null; // null for non-events
  export function modpackSummary(changes: NewsChangesView | null, t: TFunction): string; // "3 mods added, 1 removed" or "The modpack changed."
  ```
- **Strings** (en / es):

  | Key | en | es |
  |---|---|---|
  | `news.label` | News | Novedades |
  | `news.lineLabel` | News: {{line}} | Novedades: {{line}} |
  | `news.new` | New | Nuevo |
  | `news.now` | Now | Ahora |
  | `news.today` | Today {{time}} | Hoy {{time}} |
  | `news.tomorrow` | Tomorrow {{time}} | Mañana {{time}} |
  | `news.posted` | Posted {{date}} | Publicado el {{date}} |
  | `news.showMore` | Show more | Ver más |
  | `news.close` | Close | Cerrar |
  | `news.loadFailed` | Couldn't load more news. | No se pudieron cargar más novedades. |
  | `news.retry` | Try again | Reintentar |
  | `news.modpack.title` | Modpack updated | Modpack actualizado |
  | `news.modpack.added_one` / `_other` | {{count}} mod added / {{count}} mods added | {{count}} mod agregado / {{count}} mods agregados |
  | `news.modpack.removed_one` / `_other` | {{count}} removed | {{count}} quitado / {{count}} quitados |
  | `news.modpack.updated_one` / `_other` | {{count}} updated | {{count}} actualizado / {{count}} actualizados |
  | `news.modpack.changed` | The modpack changed. | El modpack cambió. |
  | `news.modpack.andMore` | and {{count}} more | y {{count}} más |

  The modpack summary joins its parts with ", " and drops parts with a count of 0. An event title with its time reads "{{title}} · {{time}}".

- [ ] **Step 1: Write the failing tests:**
  - `newsRules.test.ts`, with fixed `now = new Date('2026-10-17T12:00:00Z')` and `locale` `en` and `es`:
    - `isOver` with `endsAt`, and without it (+3 h);
    - `latestLine` prefers a not-over event over a newer update, and falls back to the newest post when every event is over;
    - `isNew uses the marker, not the clock`;
    - the first visit's 7-day rule;
    - `eventLabel`: Now, Today, Tomorrow, a weekday within 6 days, a date later;
    - `modpackSummary` plurals in en and es, the parts with 0 left out, and the null-changes text.
  - The contract test parses `server_news.json`.
  - The mock answers `server_news` for the seeded instance and the public cards; the `news-off` scenario gives `null`.
- [ ] **Step 2: Run them to see them fail.** Run `pnpm --filter ui test`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `eventLabel` runs on the computer's time zone, and tests set `process.env.TZ = 'UTC'` in `ui/src/test/setup.ts` only if it isn't already set (check the file first).
  - In the mock, give the seeded server instance and most public cards a few posts (an upcoming event, an update with a picture using an existing mock image, a modpack post), and give one card none.
- [ ] **Step 4: Run.** Run `pnpm --filter ui test`, `typecheck` and `lint`. Expected: PASS.
- [ ] **Step 5: Commit.** Commit `feat(ui): server news data, rules, mock and strings (#51)`.

### Task 5: NewsLine and NewsDialog on the card and Home (through `/impeccable`)

**Files:**
- Create:
  - `ui/src/features/news/NewsLine.tsx`;
  - `ui/src/features/news/NewsDialog.tsx`;
  - `ui/src/features/news/News.test.tsx`.
- Modify:
  - `ui/src/features/servers-list/PublicServerCard.tsx` (the line under the status, asking only while the card is on screen);
  - `ui/src/features/servers/ServerPanel.tsx` (the line under the status pill and address; the invite comes from `instance.source.invite` when `source.kind === 'perland'`);
  - styles where impeccable puts them.

**Interfaces:**
- Consumes: Task 4's hooks, helpers and strings, plus the existing `Dialog` and `Button`.
- Produces:
  - `NewsLine({ invite, name, visible }: { invite: string | null; name: string; visible: boolean })`;
  - `NewsDialog({ invite, name, open, onClose })`.

- [ ] **Step 1: Write the failing tests** in `News.test.tsx`, with the mock backend:
  - `a card shows the latest news line`: the mock's upcoming event reads "Build contest · Sat 8 pm", or whatever the fixed mock time gives;
  - `the dot shows for unseen news and clears after opening`;
  - `the dialog lists posts, pages with Show more, and shows pictures`;
  - `a body is plain text`: the mock post body `"<b>bold</b>"` renders literally;
  - `a modpack post reads in the player's language`: the es text "3 mods agregados, 1 quitado";
  - `nothing shows with news off`: the `news-off` scenario; no line, no `alert`, no toast;
  - `only cards on screen ask for news`: stub `IntersectionObserver` so one card is visible; `server_news` is invoked for that card's invite only;
  - `Home shows the line for a server instance`.
- [ ] **Step 2: Run them to see them fail.** Run `pnpm --filter ui test -- News`. Expected: FAIL.
- [ ] **Step 3: Build with `/impeccable`.** Invoke the `impeccable` skill for this component and follow the surface rules:
  - the line is quiet, one line, ellipsis on overflow;
  - the dot is blue info, with text "New" for screen readers;
  - no lit button;
  - the dialog's Close is ghost;
  - pictures are never upscaled.
- [ ] **Step 4: Run.** Run `pnpm --filter ui test`, `typecheck`, `lint` and `build`. Expected: PASS.
- [ ] **Step 5: Look at it.** Take screenshots in `pnpm --filter ui dev:mock` (Chromium at `/opt/pw-browsers/chromium`) at 960 × 600 in en and es:
  - the Servers screen with news lines and a dot;
  - the dialog;
  - Home with the line;
  - the `news-off` scenario.

  Fix what the finish review flags.
- [ ] **Step 6: Commit.** Commit `feat(ui): server news on the card and on Home (#51)`.

### Task 6: Whole-branch verification

- [ ] **Step 1: Run every CLAUDE.md "Before committing" check.** Expected: all pass.
  - `pnpm --filter ui build`
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm -r lint`
  - `pnpm -r typecheck`
  - `pnpm -r test`
- [ ] **Step 2: Scan for tokens.** Run `git diff origin/main | grep -oE "[A-Za-z0-9_-]{43}" | sort -u`. Only fake tokens, base64 fragments and long test names are allowed.
- [ ] **Step 3: Commit** any fixes with `chore: verification fixes (#51)`.
