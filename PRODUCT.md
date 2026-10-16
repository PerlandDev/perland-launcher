# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(A Tauri 2 desktop app: the UI is a React page rendered in the OS webview (WebView2 on Windows, WebKitGTK on Linux), with a frameless window and a custom title bar. It runs as a desktop window, not a website: no URLs, no responsive phone layout. The minimum window size is 960 × 600.)

## Users

- **Perland players (primary).** These are people who play on Perland-hosted Minecraft servers, starting with the Asalith Fields community. Many are 12–25 and casual, not technical, and many speak Spanish. They want to open the launcher, press Play and be in the game. Copy is short, plain and friendly. Launcher vocabulary ("JVM", "classpath", "natives") never appears in the main UI; technical detail sits only behind a "Details" toggle.
- **General Minecraft players (secondary).** Anyone who wants a free, modern launcher for vanilla, Fabric, Quilt, Forge or NeoForge instances. They know words like "instance", "mods", "Fabric" and "memory", but nothing deeper.

## Product Purpose

Perland Launcher is a free, open-source Minecraft launcher. Perland's mission is to fill what Minecraft is missing, starting with free servers, and the launcher is how players get into those servers without managing files by hand. Today it installs and launches vanilla, Fabric, Quilt, Forge and NeoForge instances, installs and syncs packs, and updates itself, with offline/dev accounts. Microsoft sign-in comes in a later plan.

Success means a new player goes from opening the launcher to playing in as few clicks as possible, and every failure tells them, in their language, what happened and what to do next.

## Positioning

Packs stay in sync automatically, so a Perland player never manages mod versions. It is also the free, clean, bilingual launcher that Perland servers point to. It is built from scratch on Tauri and Rust, is not a fork of Prism or the Modrinth App, and is not tied to any marketplace.

## Operating Context

- **Platforms:** Windows first, then Linux. macOS comes later.
- **Where data lives:** `%APPDATA%\PerlandLauncher` (or `~/.local/share/PerlandLauncher`). Instances live in `instances/<id>/minecraft`.
- **While playing:** the launcher sits in the background during a session. It minimizes when the game starts and comes back on exit or crash, and it may run more than one instance at a time.
- **First run:** pick a language (en/es, detected from the OS), add an account, create a first instance.
- **Typical session:** open the launcher, see the last instance, press Play, watch progress (first install downloads hundreds of MB), play, close.
- **Network:** players may be on slow or flaky connections. Metadata is cached, so the launcher starts offline.

## Capabilities and Constraints

- **Shipped in the core (Plan 1):**
  - vanilla/Fabric/Quilt install
  - Mojang's official Java runtimes
  - instances (validated slug ids)
  - offline/dev accounts
  - launch with redacted logs
  - typed errors (category, plain message, optional fix, technical detail)
- **In this UI plan:**
  - home with the instance rail and Play
  - the activity drawer with progress, log and crash card
  - new instance, instance settings
  - a local mods list (the jar files in `mods/`)
  - accounts, settings, first run
- **Built since (Plans 3a/3b, Browse in PR #16):**
  - Browse: search and install Modrinth modpacks, mods, resource packs and shaders
  - pack links, `.mrpack` files and pack sync, with pack art (icon and banner) when a pack ships it
  - Forge and NeoForge install and launch
  - the launcher's own updater and installers (the manual Release workflow)
  - import a whole instance from another launcher's folder (mods, worlds, settings and map data are copied; the original folder is only read)
- **Not built (never fake it):** Microsoft sign-in (Plan 4). It appears as a visible, disabled "coming soon" control. Everything else that is not built is absent.
- **Offline-account gate:** release builds unlock offline accounts only after one licensed Microsoft sign-in. Dev builds allow them freely.
- **Terminology:** "instance", "Play", "mods", "account", "loader" (Fabric, Quilt, Forge, NeoForge), "Minecraft version", "memory".
- **Languages:** English and Spanish. Every string, including core error messages (translated by stable error code), exists in both.

## Brand Commitments

- **Name:** **Perland Launcher** (org `PerlandDev`).
- **Mark:** the legacy PerlandDev **pixel land plots**: four stepped, terraced plots in lime, amber, coral-red and blue, with brown "dirt" steps beneath. The only source today is a compressed raster, `E:\PerlandDev-site\public\logo.png` (1000 × 1000, visible artifacts), so it must be redrawn as a clean vector on its pixel grid before use.
- **Not the Panel's look:** the launcher's look is its own player-facing system. It does not reuse the Panel's cyan chamfered mark or "Director HUD" language.
- **Owner-chosen direction (binding):** "game-forward". Full-bleed instance/pack hero art, a big friendly Play button, an instance icon rail on the left, a game-client feel (like Battle.net or Lunar), and a custom title bar. Dark only for v1.
- **Licensing and trademarks:**
  - GPL-3.0-only.
  - No Modrinth branding.
  - No Mojang artwork, skins, logos or fonts: hero art and avatars are generated.
  - A "not an official Minecraft product" notice in About.
- **Voice:** friendly, short and plain, like a helpful friend who knows the game. Spanish is neutral Latin American with informal *tú* ("Juega", "Tu cuenta"), avoiding Spain-only or strongly regional terms.

## Evidence on Hand

- The core library and its behavior: `crates/launcher-core`. The design spec: `docs/superpowers/specs/2026-10-09-launcher-core-design.md`.
- The legacy mark raster: `E:\PerlandDev-site\public\logo.png`.
- **Absent, never fabricate:** pack art a pack doesn't ship (it keeps its generated art), player counts, server status, user numbers, testimonials, screenshots of real servers.

## Product Principles

1. **Play is the point.** The path from opening the launcher to being in the game is always one obvious button. Everything else is secondary and stays out of its way.
2. **Honest about what exists.** Unbuilt features are clearly "coming soon" or absent, never mocked.
3. **Errors speak the player's language.** Every failure says what happened and what to do, in English or Spanish, with technical detail only on request.
4. **Never lose the player's stuff.** Destructive actions state their consequences first, and the launcher never touches files it doesn't own.
5. **Bilingual by default.** Spanish is a first-class language, not a translation pass at the end.

## Accessibility & Inclusion

- Target WCAG 2.2 AA inside the desktop window.
- Every control is reachable by keyboard, with visible focus.
- Status never relies on color alone.
- Motion respects `prefers-reduced-motion`.
- Copy suits young, non-native and non-technical readers. Text must survive Spanish's longer strings (roughly +30% length) without truncating key actions.
