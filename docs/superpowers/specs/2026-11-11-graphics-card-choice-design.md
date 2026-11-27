# Perland Launcher: Graphics Card Choice, Design

- **Date:** 2026-11-11
- **Builds on:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (launch, settings), `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md` (Settings screen), `DESIGN.md`.
- **Status:** short design for a small change. Decisions the owner did not state are marked **our call**.

## 1. Why

On a laptop with two graphics cards, the system often starts Minecraft on the integrated one, and the game runs badly. Today the fix is buried in Windows' own settings, and the player has to find the launcher's Java to apply it. The owner asked for a setting in the launcher: which graphics card the game uses, integrated or dedicated, locked when the computer has only one.

## 2. Outcome

Settings gets one select, **Graphics card**, under default memory:

| Choice | Meaning |
|---|---|
| Automatic (default) | The launcher does nothing; the system decides, as today |
| Dedicated | The game is started on the high-performance card |
| Integrated | The game is started on the power-saving card |

- The system does not know "integrated" and "dedicated": it knows a power-saving card and a high-performance one. In a tower with two dedicated cards, "Dedicated" is the stronger one and "Integrated" the other. With three or more, the ones in between can't be chosen.
- When no card is known to be built into the processor, the two choices are named by what they do: **More performance** and **Less power** (the owner's pick, 2026-11-11).
- Under the select, a line names the cards found ("Found: Intel(R) UHD Graphics 770, NVIDIA GeForce RTX 4060."), so the player sees what the choice is between.
- With one graphics card, the select is disabled and the line says the computer has only one.
- On a system where the launcher cannot choose (macOS), it is disabled and the line says the system decides.
- The choice applies to every instance, from the next time a game starts. A game already running is not touched.

## 3. Goals, non-goals, anti-goals

- **Goals:** the select in Settings; the lock with one card; the choice honoured at launch on Windows and Linux; en and es strings; a release note; working in `dev:mock`.
- **Non-goals:** a per-instance choice (**our call**: the owner asked for it in Settings; it can be added later on top of this); picking one card by its name (it can be built on the names already found; on Windows it needs a newer preference format we cannot test yet); macOS support; showing which card the game ended up on.
- **Anti-goals:** changing anything for a player who never opens the setting; a failure to apply the choice stopping the game from starting.

## 4. Approaches

1. **Per-app system preference on Windows, environment variables on Linux (picked).** These are the mechanisms each system offers for exactly this. Nothing is injected into the game.
2. **Environment variables everywhere.** Windows has none for the graphics card, so this would not work for most players.
3. **Exporting `NvOptimusEnablement` / `AmdPowerXpressRequestHighPerformance` from the process.** These are symbols of the executable, and the game runs in a Java the launcher does not build. Not possible.

## 5. How

### 5.1 The setting (`launcher-core::settings`)

`Settings.graphics_card: GraphicsCard`, serialized `graphicsCard`, values `automatic` | `dedicated` | `integrated`. `Automatic` is the default and what an unknown value from a newer launcher loads as (`serde(other)`, like `Theme`).

### 5.2 Finding the cards (`launcher-core::graphics`, new)

`GraphicsSupport { status, cards, integrated }`: `status` is `choose` | `singleCard` | `unsupported`, `cards` the names found, `integrated` whether one is built into the processor. Pure logic takes the list of cards; the system-specific part only produces that list:

- **Windows:** `HKLM\HARDWARE\DEVICEMAP\VIDEO`, which Windows rebuilds at every start, so a card that was taken out is not counted. Each `\Device\Video<N>` names a key of its adapter (`…\Control\Video\{GUID}\000N`); adapters are told apart by the GUID. Only adapters whose `MatchingDeviceId` is a PCI device count, so remote-desktop and virtual displays do not. The name is `DriverDesc`.
- **Linux:** `/sys/class/drm/card<N>/device/vendor` for each `card<N>` (connectors like `card0-eDP-1` are skipped, and so is a card without a maker, such as `simpledrm`). The name is the maker (Intel, NVIDIA, AMD). The reader takes the directory as a parameter, so tests use a temp folder.
- **macOS and anything else:** `unsupported`.
- **Integrated** is only said when certain (Intel that is not Arc; AMD's "Radeon Graphics" and "Vega" names). A miss only makes the words more general.
- Cards that can't be read count as unknown and **do not lock** the select (**our call**: a wrong lock is worse than a choice that does nothing).

### 5.3 Applying it at launch

`LaunchCommand` gains `env: Vec<(String, String)>`, which `spawn` adds to the game's environment. `play::command` fills it from the choice; `play::start` applies the Windows preference.

- **Linux, Dedicated:** `DRI_PRIME=1`, `__NV_PRIME_RENDER_OFFLOAD=1`, `__GLX_VENDOR_LIBRARY_NAME=nvidia`, `__VK_LAYER_NV_optimus=NVIDIA_only`. **Integrated:** `DRI_PRIME=0`. **Automatic:** nothing; variables the player set themselves pass through.
- **Windows:** before the game starts, write the per-app preference for the Java executable about to run: a value named with its full path under `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`, data `GpuPreference=2;` (Dedicated) or `GpuPreference=1;` (Integrated).
- **Ownership:** `graphics-preferences.json` in the launcher's folder lists the executables whose preference the launcher wrote. Automatic removes those, and only while they still hold what the launcher writes. A preference the player set in Windows themselves is never taken away, including one equal to the launcher's.
- A failed write is logged as a warning and the game starts anyway.
- **Known limit:** the Windows preference belongs to the Java executable. With the Java the launcher installs, only the launcher's games are affected. An instance set to a Java installed elsewhere shares the preference with every other program that uses that Java.

### 5.4 App and UI

- `AppInfo` gains `graphics` (the `GraphicsSupport`), found once at start by `src-tauri` calling `graphics::detect()`. No new command (the contract is regenerated with `UPDATE_CONTRACT=1`). The runner reads `graphicsCard` from the settings file as the game starts.
- `Settings.tsx`: the existing `Select`, which gains an optional `hint` line. No new component and no new styling.
- The mock backend takes `?graphics=one|dedicated|unsupported|unknown` for tests and screenshots.

Strings (en / es):

| Key | en | es |
|---|---|---|
| label | Graphics card | Tarjeta gráfica |
| automatic | Automatic | Automática |
| dedicated | Dedicated (more performance) | Dedicada (más rendimiento) |
| integrated | Integrated (saves battery) | Integrada (ahorra batería) |
| performance | More performance | Más rendimiento |
| saving | Less power | Menos consumo |
| hint | The graphics card Minecraft runs on. It applies the next time you start a game. | La tarjeta gráfica con la que corre Minecraft. Se aplica la próxima vez que inicies un juego. |
| found | Found: {{cards}}. | Detectadas: {{cards}}. |
| single | This computer has only one graphics card. | Este equipo solo tiene una tarjeta gráfica. |
| unsupported | On this computer, the system chooses the graphics card. | En este equipo, el sistema elige la tarjeta gráfica. |

## 6. Tests

- Core: settings round-trip and unknown value; card counting from fixture lists and a temp `drm` folder; the choice-to-action mapping for each system; `spawn` passing `env` to the process (a tiny script that prints a variable).
- App: `graphics` in the `app_info` contract fixture.
- UI (Vitest): the select saves each choice; disabled with each reason; both languages.
- **Not testable here:** the Windows registry code (reading the cards and writing the preference) and the real effect on any system. Before release, someone checks on a laptop with a dedicated card that Windows' Graphics settings list the launcher's Java with the chosen preference and that the game's F3 screen names the chosen card.

## 7. Settled while building

The Windows registry is read and written with `windows-registry` (already in `Cargo.lock` through Tauri).
