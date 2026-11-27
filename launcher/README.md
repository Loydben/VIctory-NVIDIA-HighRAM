# Launcher

A small window for people who would rather not type commands. It builds VIctory from this repository and
starts **your own copy** of the game with the settings of `scripts\play-fsr.ps1`.

It ships nothing and downloads nothing: no game files, no DLLs. There are two forms, doing the same thing:

- **`VIctory-Launcher.exe`** (source: [`apps/launcher`](../apps/launcher)): from [Releases](../../../releases), built
  by GitHub Actions from this source, with its SHA-256 in the release notes; or build it with
  `cargo build --release -p victory-launcher`. Put it anywhere inside the folder you cloned this repository into.
  It is not code-signed yet, so Windows SmartScreen may warn the first time: check the SHA-256, then "More info" and
  "Run anyway".
- **`launcher\Start-Launcher.cmd`**: the same window as a PowerShell script you can read.

## Use it

1. Install, once: [Rust](https://rustup.rs/) and Visual Studio 2022 (or newer) Build Tools with
   **Desktop development with C++**. The launcher tells you if either is missing.
2. Double-click **`VIctory-Launcher.exe`** (or `launcher\Start-Launcher.cmd`).
3. **Your game folder**: the folder that holds `game_scarlett_master.exe` (your own dump, kept **outside** this
   repository: VIctory refuses game files inside it).
4. **AMD FSR** (optional, **off by default**). Off gives the **best picture**. On gives a **softer, less detailed
   picture** for about the same speed (measured: 17.7 against 17.3 fps) and a little more RAM. To use it, tick
   **Use AMD FSR** and pick `amd_fidelityfx_upscaler_dx12.dll` from AMD's FidelityFX SDK.
5. **Series S** (faster) or **Series X** (sharper, slower), and the **FPS panel** on or off.
6. Press **Build** (first time only, 5-15 minutes), then **Prepare shaders** (first time only, about a minute),
   then **PLAY**.

**Advanced** (folded, in the .exe): full screen, low-memory mode (cards under 8 GB; slower), safe mode, ray tracing
(experimental: on NVIDIA it loses the card within minutes), and a box for extra `VICTORY_NAME=value` settings, one
per line.

The launcher remembers your choices in `%LOCALAPPDATA%\victory` (`launcher.txt` for the .exe, `launcher.json` for
the script). Saves and the shader cache go to
VIctory's defaults under `%LOCALAPPDATA%\victory`.

## In the game

| Key | What it does |
|---|---|
| **F3** | Shows or hides the performance panel |
| **Ctrl + drag** on the panel | Moves it anywhere on the picture |
| **Ctrl + Plus / Ctrl + Minus** | Makes the panel bigger or smaller (0.5x to 3x) |
| **Ctrl + 0** | Puts the panel back in the upper left corner at its normal size |

Where you put the panel is remembered (`%LOCALAPPDATA%\victory\panel.txt`).

## Good to know

- The first start of a new build stutters while the card builds its pipelines; judge the speed from the second.
- A firewall rule that blocks `target\diag\victory.exe` from the network is a good idea: the game tries to send
  crash reports.
- Run it from your normal account, not as administrator.
