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

- With one graphics card, the select is disabled and a line says the computer has only one.
- On a system where the launcher cannot choose (macOS), it is disabled and a line says the system decides.
- The choice applies to every instance, from the next time a game starts. A game already running is not touched.

## 3. Goals, non-goals, anti-goals

- **Goals:** the select in Settings; the lock with one card; the choice honoured at launch on Windows and Linux; en and es strings; a release note; working in `dev:mock`.
- **Non-goals:** a per-instance choice (**our call**: the owner asked for it in Settings; it can be added later on top of this); picking a card by name on a computer with three or more; macOS support; showing which card the game ended up on.
- **Anti-goals:** changing anything for a player who never opens the setting; a failure to apply the choice stopping the game from starting.

## 4. Approaches

1. **Per-app system preference on Windows, environment variables on Linux (picked).** These are the mechanisms each system offers for exactly this. Nothing is injected into the game.
2. **Environment variables everywhere.** Windows has none for the graphics card, so this would not work for most players.
3. **Exporting `NvOptimusEnablement` / `AmdPowerXpressRequestHighPerformance` from the process.** These are symbols of the executable, and the game runs in a Java the launcher does not build. Not possible.

## 5. How

### 5.1 The setting (`launcher-core::settings`)

`Settings.graphics_card: GraphicsCard`, serialized `graphicsCard`, values `automatic` | `dedicated` | `integrated`. `Automatic` is the default and what an unknown value from a newer launcher loads as (`serde(other)`, like `Theme`).

### 5.2 Counting cards (`launcher-core::graphics`, new)

`GraphicsSupport { can_choose: bool, reason: Option<WhyNot> }`, with `WhyNot` = `single_card` | `unsupported_system`. Pure logic takes a list of detected cards; the system-specific part only produces that list:

- **Windows:** the display-adapter class in the registry (`HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\NNNN`), keeping entries with a hardware id on PCI (`MatchingDeviceId` starting `pci\`), so remote-desktop and virtual display drivers are not counted. Distinct cards are counted.
- **Linux:** `/sys/class/drm/card<N>/device/vendor` for each `card<N>` (connectors like `card0-eDP-1` are skipped). The reader takes the directory as a parameter, so tests use a temp folder.
- **macOS and anything else:** `unsupported_system`.
- Detection that fails counts as unknown and **does not lock** the select on Windows and Linux (**our call**: a wrong lock is worse than a choice that does nothing).

### 5.3 Applying it at launch

`LaunchCommand` gains `env: Vec<(String, String)>`, which `spawn_with_log` passes to the process. A pure function maps the choice to what to do:

- **Linux, Dedicated:** `DRI_PRIME=1`, `__NV_PRIME_RENDER_OFFLOAD=1`, `__GLX_VENDOR_LIBRARY_NAME=nvidia`, `__VK_LAYER_NV_optimus=NVIDIA_only`. **Integrated:** `DRI_PRIME=0`. **Automatic:** nothing; variables the player set themselves pass through.
- **Windows:** before the game starts, write the per-app preference for the Java executable about to run: value named with its full path under `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`, data `GpuPreference=2;` (Dedicated) or `GpuPreference=1;` (Integrated). On Automatic, the value is removed only if it holds exactly what the launcher writes, so a preference the player set in Windows for other reasons is left alone as far as we can tell.
- A failed write is logged as a warning and the game starts anyway.
- **Known limit:** the Windows preference belongs to the Java executable. With the Java the launcher installs, only the launcher's games are affected. An instance set to a Java installed elsewhere shares the preference with every other program that uses that Java.

### 5.4 App and UI

- `launcher-app` exposes `graphics_support` (new IPC command; contract regenerated with `UPDATE_CONTRACT=1`) and reads `graphics_card` from settings when it builds the launch.
- `Settings.tsx`: a select built from the existing field components, with the hint line below it. Styling goes through `/impeccable` and adds no new component.
- The mock backend gets a switch for one card, two cards and unsupported, for tests and screenshots.

Strings (en / es):

| Key | en | es |
|---|---|---|
| label | Graphics card | Tarjeta gráfica |
| automatic | Automatic | Automática |
| dedicated | Dedicated (more performance) | Dedicada (más rendimiento) |
| integrated | Integrated (saves battery) | Integrada (ahorra batería) |
| hint | Which graphics card Minecraft uses. It applies the next time you start a game. | Qué tarjeta gráfica usa Minecraft. Se aplica la próxima vez que inicies un juego. |
| single | This computer has only one graphics card. | Este equipo solo tiene una tarjeta gráfica. |
| unsupported | On this system, the system chooses the graphics card. | En este sistema, el sistema elige la tarjeta gráfica. |

## 6. Tests

- Core: settings round-trip and unknown value; card counting from fixture lists and a temp `drm` folder; the choice-to-action mapping for each system; `spawn` passing `env` to the process (a tiny script that prints a variable).
- App: `graphics_support` in the contract; the launch built with the setting.
- UI (Vitest): the select saves each choice; disabled with each reason; both languages.
- **Not testable here:** the real effect on Windows. Before release, someone checks on a laptop with a dedicated card that Windows' Graphics settings list the launcher's Java with the chosen preference and that the game's F3 screen names the chosen card.

## 7. Left to the builder

The exact Windows registry crate (one already in `Cargo.lock` is preferred), the field component used for the select, and the mock switch's name.
