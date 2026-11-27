# Launcher

A small window for people who would rather not type commands. It builds VIctory from this repository and
starts **your own copy** of the game with the settings of `scripts\play-fsr.ps1`.

It ships nothing and downloads nothing: no game files, no executables, no DLLs. It is two scripts you can read.

## Use it

1. Install, once: [Rust](https://rustup.rs/) and Visual Studio 2022 (or newer) Build Tools with
   **Desktop development with C++**. The launcher tells you if either is missing.
2. Double-click **`launcher\Start-Launcher.cmd`**.
3. **Your game folder**: the folder that holds `game_scarlett_master.exe` (your own dump, kept **outside** this
   repository: VIctory refuses game files inside it).
4. **AMD FSR DLL** (optional): `amd_fidelityfx_upscaler_dx12.dll` from AMD's FidelityFX SDK. Without it the
   game's own upscaler runs.
5. **Series S** (faster) or **Series X** (sharper, slower), and the **FPS panel** on or off.
6. Press **Build** (first time only, 5-15 minutes), then **Prepare shaders** (first time only, about a minute),
   then **PLAY**.

The launcher remembers your choices in `%LOCALAPPDATA%\victory\launcher.json`. Saves and the shader cache go to
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
