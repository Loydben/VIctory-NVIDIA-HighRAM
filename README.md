# VIctory-RTX

Runs an Xbox Series title natively on Windows from **your own copy**, through
[VIctory](https://github.com/Marvisak/VIctory), tuned for **NVIDIA RTX cards on an ordinary amount of memory**:
32 GB of RAM and a 12 GB card. It is built on [Vice-Tay](https://github.com/TaynanGT/Vice-Tay) (TaynanGT), with
**AMD FSR**, the card-side buffer copies that remove the wait for the graphics card, a movable performance panel
and a one-window launcher.

Measured with GTA VI (Xbox Series build 1.0.2.1, your own dump): **about 17-18 fps in the open world, up to 21**, on
an RTX 5090 **Laptop** GPU with a laptop processor.

**No game files, code, keys or assets are in this repository**, and nothing here downloads them. You need your own
copy. Not affiliated with or endorsed by the game's developer or publisher, Microsoft, AMD or NVIDIA. See
[docs/asset-policy.md](docs/asset-policy.md).

## Results (2026-11-26)

One PC: Intel Core Ultra 9 275HX, NVIDIA GeForce RTX 5090 Laptop GPU (24 GB), 64 GB RAM, Windows 11, driver 621.14.
Series S profile in a window, standing in town, 7-minute runs, from minute 3 on; memory after loading.

| Build | Median frame | Best 30 s | Worst 10 % | RAM (avg / peak) | Video memory (avg / peak) |
|---|---|---|---|---|---|
| **This one: Vice-Tay + card copies + AMD FSR** | **56.4 ms (17.7 fps)** | 48 ms (20.7 fps) | 86 ms | 29.0 / 30.4 GB | 8.0 / 10.6 GB |
| Vice-Tay + card copies, no FSR | 57.7 ms (17.3 fps) | 54 ms | 65 ms | 28.2 / 29.1 GB | 7.5 / 8.9 GB |
| Vice-Tay as it is (its own NVIDIA settings) | 110 ms (9 fps) | 92 ms | 151 ms | 26.5 / 29.1 GB | 6.1 / 7.8 GB |
| This repository's previous build (branch `legacy-avatarmanz`) | 68.9 ms (14.5 fps) | 63 ms | 82 ms | 27.2 / 28.1 GB | 8.4 / 9.4 GB |

> [!NOTE]
> **AMD FSR is optional, and off gives the better picture.** With FSR on, the picture is softer and less detailed,
> for about the same speed (17.7 against 17.3 fps here) and about 1 GB more RAM. Turn it on only if you prefer a
> few percent more frames to a sharper image.

Where the time goes: on this PC the graphics card is not the limit. Vice-Tay prepares each frame on the processor
faster than the previous build (30-45 ms against 47-51), but as it ships it waits 45-60 ms a frame for the card;
`VICTORY_CARD_ALL=1` (every title buffer gets a copy in the card's memory) takes that wait to 0 ms.

What is known to be wrong:

- Ray-traced lighting (`VICTORY_BVH_BUILDER=1`) looks right but loses the device within minutes on this NVIDIA
  laptop; it is off.
- Textures and objects pop in late while moving; the first start of a new build stutters while pipelines are made.
- RAM can pass 30 GB for a moment with AMD FSR on; 32 GB of RAM is the least to try.

## Getting started

### The launcher (easiest)

Double-click **`launcher\Start-Launcher.cmd`**, point it at your game folder, leave **AMD FSR** off for the best
picture (or tick it and pick AMD's DLL), press **Build**, **Prepare shaders**, then **PLAY**. It ships and downloads nothing. Details:
[launcher/README.md](launcher/README.md).

### By hand

You need Windows 10/11 x64, an AVX2 processor, an NVIDIA card with mesh shaders and shader model 6.6,
[Rust](https://rustup.rs/), Visual Studio Build Tools (Desktop development with C++), and your own dump of the title
**outside** this folder.

```powershell
git clone https://github.com/Loydben/VIctory-RTX
cd VIctory-RTX
$env:VICTORY_GAME_DIR = '<your game folder>'
cargo build --locked --profile diag -p victory --features diagnostics
target\diag\victory.exe shaders "$env:VICTORY_GAME_DIR\shaders.rpf"
scripts\play-fsr.ps1 -Exe "$env:VICTORY_GAME_DIR\game_scarlett_master.exe" -FsrDll <path>\amd_fidelityfx_upscaler_dx12.dll
```

Leave out `-FsrDll` for the best picture. `scripts\play-fsr.ps1` options: `-FsrDll` (AMD FSR: a softer picture for
about the same speed; the DLL comes from AMD's FidelityFX SDK and is not shipped here),
`-SeriesX` (sharper, slower), `-FullScreen`, `-NoPanel`, `-LowMemory` (without the card copies, for cards under
8 GB; slower), `-More @{VICTORY_X='1'}`. It runs Vice-Tay's `scripts\play.ps1` underneath.

A firewall rule that blocks `target\diag\victory.exe` from the network is a good idea: the title tries to send crash
reports. Run it from your normal account, not as administrator.

### In the game

| Key | What it does |
|---|---|
| **F3** | Shows or hides the performance panel (FPS, frame times, CPU, GPU, VRAM, RAM) |
| **Ctrl + drag** on the panel | Moves it anywhere on the picture |
| **Ctrl + Plus / Ctrl + Minus** | Makes the panel bigger or smaller (0.5x to 3x) |
| **Ctrl + 0** | Puts the panel back in the upper left corner |

## What is in it

This is other people's work, combined and measured on one NVIDIA laptop. The history is kept: every commit names its
author.

| From | What |
|---|---|
| [TaynanGT/Vice-Tay](https://github.com/TaynanGT/Vice-Tay) (the base) | The title's list calls only written down and translated on threads beside the title's (`VICTORY_THIN`), several threads recording, landing and mapping, the next frame prepared while the card works, the wave-wide form for the 64-lane programs, the performance panel, the scripted test sessions. Its own README: [docs/README-Vice-Tay.md](docs/README-Vice-Tay.md); everyone it builds on: [docs/credits.md](docs/credits.md). |
| [Marvisak/VIctory](https://github.com/Marvisak/VIctory) | The compatibility layer itself. |
| [cobrez/Victory-Nvidia-4090-laptop](https://github.com/cobrez/Victory-Nvidia-4090-laptop) | `VICTORY_CARD_ALL` / `VICTORY_FETCH_CHECK`: every title buffer copied to the card, relay results checked. On this PC it removes the 45-60 ms a frame spent waiting for the card. |
| [zm0kiemedia/Neptune_VI](https://github.com/zm0kiemedia/Neptune_VI) | The FidelityFX redirect: the title's AMD FSR calls answered by AMD's PC DLL with the host device, list and textures. |
| This fork ([Loydben](https://github.com/Loydben)) | The FidelityFX redirect ported and made to work with Vice-Tay's written-down lists (the dispatch runs in order on the list the calls are translated onto); the panel moved and resized with Ctrl; `scripts\play-fsr.ps1`; the launcher; the measurements. |
| Branch [`legacy-avatarmanz`](../../tree/legacy-avatarmanz) | This repository's previous build and its credits: avatarmanz/VIctory-7900XTX, trustno1-coder/VIctory-NVIDIA, cobrez, fxceless/victory-fork pull requests 854 (PiloteCode) and 927 (tippythetaidum), Belobug pull request 181, ESAT (avatarmanz pull request 121). |

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not covered is named in
[docs/asset-policy.md](docs/asset-policy.md).
