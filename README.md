# VIctory on NVIDIA, for PCs with plenty of RAM and video memory

A fork of [VIctory](https://github.com/Marvisak/VIctory), the compatibility layer that runs an Xbox
Series title natively on Windows from files you supply, put together for **NVIDIA cards on PCs with
a lot of memory**. It runs GTA VI (Xbox Series build 1.0.2.1, your own copy) at about
**13-20 fps in free roam and 24-25 fps in cutscenes** on one RTX 5090 Laptop GPU, with AMD FSR
upscaling, using about **25 GB of RAM and 7-9 GB of video memory**.

**This repository contains no game files, code or assets.** You need your own copy of the game.
It is not affiliated with or endorsed by the game's developer or publisher, Microsoft or NVIDIA.
See [docs/asset-policy.md](docs/asset-policy.md).

## Where it stands (2026-11-26)

Tested on one machine only: Windows 11, Intel Core Ultra 9 275HX, NVIDIA GeForce RTX 5090 Laptop
GPU (24 GB), 64 GB RAM, Series S profile, `scripts\play-highram.ps1 -FsrDll ...` (AMD FSR).

| | |
|---|---|
| Menus, videos, loading screens | 30 fps |
| Opening cutscenes | 40-42 ms a frame (**24-25 fps**) |
| Free roam in town (5-minute run) | 49-92 ms a frame, median 70 ms (**13-20 fps**, about 14 most of the time) |
| Indoors (the opening house) | about 64 ms a frame (**16-20 fps**) |
| First mission, on the boat | 60-75 ms a frame (**13-16 fps**) |
| Memory in use (free roam) | **25.3 GB of RAM on average, 27.5 GB at most; 7.4 GB of video memory on average, 9.1 GB at most** |
| Longest session so far | 30 minutes and still running, no crash and no device loss |
| Waiting for the graphics card each frame | 0 ms (it was 58 ms before `VICTORY_CARD_ALL`): the processor is what limits it |
| Mouths move in cutscenes | yes |
| Boats float | yes |

New on 2026-11-26, from this morning's build:

- **AMD FSR**: the title's own FidelityFX calls go to AMD's PC DLL (`-FsrDll`, hand port of
  zm0kiemedia/Neptune_VI's redirect). The Series S profile renders 1280x720 and FSR upscales it
  to 1080p, as the console does.
- **No colour banding, no white rooms, faces move**: trustno1-coder/VIctory-NVIDIA's picture
  fixes (render-target format variants, NaN scrub, the ray-traced resolve and shadow shaders
  skipped while the structure builder cannot run on NVIDIA, the face network's 16-bit values).
- **Sampler heaps** backed with 2,048 slots and a null slot at the end (ESAT, avatarmanz pull
  request 121).
- **The launcher runs again**: the published `scripts\play-highram.ps1` set `VICTORY_THIN` twice,
  which PowerShell refuses ("Duplicate keys"), so it stopped before starting the game.
- No `VICTORY_PIPELINE_WAIT_MS` any more: waiting for new pipelines dropped the skin-bake draws
  and turned faces black.

The same build with the parent forks' settings ran at 9-13 fps on this PC; the jump came from one
switch, `VICTORY_CARD_ALL=1` (below).

New on 2026-11-26: **no blue blocks in the sky and no black slabs on the water.** NVIDIA's
driver refused eight of the title's compute programs ("Shader wants wave lane count of 64":
its waves have 32 lanes, the console's 64). fxceless pull request 854 (PiloteCode) translates
such programs in a paired form, each console wave in one host wave with two lanes a thread; it
is ported here, and the alpha transmission prefilter in that form removes the blue blocks in
the sky. The same form for the water's transmission volumetrics drew black slabs under
islands, piers and bridges, so those stay out (`VICTORY_PAIRED_ONLY` in the launcher); and the
PR also moved 60 programs that already worked (the temporal resolves among them) to that
form, which here melted the picture: they keep their earlier translation. Float compares now
tell NaN by its bits (`VICTORY_NAN_EXACT`), as NVIDIA's driver may fold a compare's NaN case
away. The frame rate shows on the picture: a see-through card in the upper left
(`VICTORY_PANEL=1`; `VICTORY_PANEL=full` starts it detailed, `VICTORY_PANEL_OPACITY` sets how see-through).
**F3** shows or hides it, **F4** switches between compact (frame rate, frame time, a frame-time
graph) and detailed (also RAM, video memory, the card's load and where each frame's time goes).
Measured the same speed as without it (median 76 ms against 79 ms in town).

Also new: **no more melting.** With the title's list calls recorded on threads of their own
(`VICTORY_THIN=1`, the base fork's default), NVIDIA's capped descriptor pool ran full: about
500 descriptor tables a frame refused and 300 indirect draws dropped, so the water went
missing in tiles and the picture smeared and melted the longer one played (the same as
avatarmanz/VIctory-7900XTX issues 35, 62 and 112 on other NVIDIA cards). The launcher now sets
`VICTORY_THIN=0`: 11 refusals a frame on average, a clean picture, about 1.5 fps slower.

What is known to be wrong:

- Now and then a texture flickers or the frame stutters when a new area loads, and textures and
  objects pop in late while walking.
- Lighting is flatter than on the console and mirrors are black: the title's ray-traced light and
  reflections are skipped (`scripts\rt_off_hashes.txt`). With the structure builder on
  (`VICTORY_BVH_BUILDER=1`) ray tracing does run and looks right, but at 3-5 fps, and the card
  resets after a few minutes of walking with this launcher's speed settings.
- The phone's screen is washed out, and some doorways show blocky patches on the floor.
- The water is the darker blue of the parent forks: its transmission volumetrics are off
  until their paired form draws right.
- In-world screens (the boat's navigation display) stay dark.
- The picture is softer than the Series X profile: the world is rendered at the Series S size and
  upscaled. `-SeriesX` is sharper and, with AMD FSR, about 10 fps.
- The first run with a new build or driver stutters while pipelines are made.

## Why it needs a lot of memory

`VICTORY_CARD_ALL=1` gives every title buffer a copy in the graphics card's memory. Without it,
buffers that live only in the PC's memory make the thread that prepares each frame wait for the
card to finish before it may read them: 58 ms of a 104 ms frame on this PC. With it the wait is
gone, at the cost of memory. Measured on 2026-11-26 in free roam: **25.3 GB of RAM on average
(27.5 GB at most) and 7.4 GB of video memory (9.1 GB at most)**, well under the 40-46 GB the
parent fork measured. So 32 GB of RAM is the least to try and 64 GB is what this was tuned on;
a 12 GB card should hold it. On smaller cards, leave `-HighRam` off in the launcher (below) and
compare.

## How to run it

You need Windows 10/11 x64, a CPU with AVX2, an NVIDIA card with mesh shaders and shader model 6.6,
Rust (the toolchain is pinned), the MSVC build tools, and your own dump of the title.

```powershell
git clone https://github.com/Loydben/VIctory-NVIDIA-HighRAM
cd VIctory-NVIDIA-HighRAM
copy victory.example.toml victory.toml      # then set your paths, outside this folder
cargo build --locked --profile diag -p victory --features diagnostics
target\diag\victory.exe shaders <your game folder>\shaders.rpf   # every shader translated, about a minute
scripts\play-highram.ps1 -Exe <your game folder>\game_scarlett_master.exe -FsrDll <path>\amd_fidelityfx_upscaler_dx12.dll
```

`scripts\play-highram.ps1` options:

- `-FsrDll` turns on AMD FSR: the path of `amd_fidelityfx_upscaler_dx12.dll` from AMD's FidelityFX
  SDK (GPUOpen). It is not in this repository. Without it the title's own upscaler runs.
- `-HighRam` is on by default; `-HighRam:$false` leaves out `VICTORY_CARD_ALL` and
  `VICTORY_FETCH_CHECK` (for smaller cards: slower, as the parent forks ran).
- `-HalfScale` renders the world at half the Series S size (faster, blurrier). Scales other
  than 0.5 and the title's own make the title stop at startup on this build.
- `-SeriesX` uses the Series X profile (sharper, much slower).
- `-More @{VICTORY_X='1'}` passes other settings.

Run it from a normal window, not as administrator. A firewall rule that blocks
`target\diag\victory.exe` from the network is a good idea: the title tries to send crash reports.

## What is in it

This is a tree of other people's work, combined and tested on one NVIDIA laptop. The history is
kept: every commit names its author, and commits taken from elsewhere say where from.

| From | What |
|---|---|
| [avatarmanz/VIctory-7900XTX](https://github.com/avatarmanz/VIctory-7900XTX) (the base) | Threads for the title's list calls (`VICTORY_THIN`), submissions prepared on a thread of their own, write relay, card fills, textures on a copy queue and shown only after their upload ran (`VICTORY_SHOW_AFTER`), texture read-back for the water (boats float). Its own credits: fxceless/victory-fork, DexNom, Belobug, ZJOU-VICTORY, myokex, Geckomacabre and more ([docs/README-avatarmanz-cobrez.md](docs/README-avatarmanz-cobrez.md)). |
| [trustno1-coder/VIctory-NVIDIA](https://github.com/trustno1-coder/VIctory-NVIDIA) | NVIDIA's view heap is capped at 1,000,000: more dynamic descriptor slots (`VICTORY_DYNAMIC_SLOTS`), which ended thousands of dropped draws a frame; the Series S interface plane (`VICTORY_COMPOSE`, after fxceless pull request 1189 by Tissma95); render scale, the title's own upscaler and the watchdog patches (after ZigZagBoi1/ScarlettPort); ray walks ended at the root (`VICTORY_BVH_MISS`). From their commits 77e13a97, d3cd650c and 033be27f: render-target format variants (`VICTORY_TARGET_VARIANTS`, after fxceless a35c557393), the NaN scrub, `VICTORY_AMBIENT_MOST`, the ray-tracing skip list (`scripts\rt_off_hashes.txt`), the intro video fix, launchers per console and resolution, and `VICTORY_ADAPTER`. |
| [zm0kiemedia/Neptune_VI](https://github.com/zm0kiemedia/Neptune_VI) | The FidelityFX redirect (`VICTORY_FSR_DLL`): the title's AMD FSR calls answered by AMD's PC DLL with the host device, list and textures (hand port). |
| ESAT (avatarmanz pull request 121) | Host sampler heaps of 2,048 slots with the last one kept as a null slot. |
| [cobrez/Victory-Nvidia-4090-laptop](https://github.com/cobrez/Victory-Nvidia-4090-laptop) | The high-performance adapter on laptops with two GPUs, idle lists released when memory is low, and the `-Exact` pair `VICTORY_FETCH_CHECK` / `VICTORY_CARD_ALL` (relay results and title copies reach the right submission), which is what made this PC fast. |
| fxceless/victory-fork pull request 854 (PiloteCode) | Wave64 programs in a paired form on 32-lane hosts, mesh programs over 32 KiB linked with their pixel program. Here only for programs no other form takes, and only those listed in `VICTORY_PAIRED_ONLY`. |
| fxceless/victory-fork pull request 927 (tippythetaidum) | Halves moved through 16-bit buffer views keep their bits: the network that moves the faces works, so mouths move. Combined here with the base's own form for views picked from the heap. |
| Belobug/VIctory-Belobug-AMD pull request 181 | The BCPack BC7 dictionary's first entry read whole: no more coloured 4x4 blocks on clothes and faces. |
| This fork | `VICTORY_F16_NEAREST` in integer arithmetic (16-bit float results rounded to nearest on NVIDIA, which rounds toward zero: no banding), quad reads in pixel programs (no lines along triangle edges), a guard for a script crash in the first boat mission (`VICTORY_SCRIPT_NULL_COPY`), NaN-exact float compares (`VICTORY_NAN_EXACT`), the paired form's group sync for one-wave groups (`VICTORY_PAIRED_SYNC`) and per-program choice (`VICTORY_PAIRED_ONLY`), the merge, the measurements, the AMD FSR port and `scripts\play-highram.ps1`. |

## License

GPL-3.0-or-later, as VIctory. See [LICENSE](LICENSE); the one file that is not covered is named in
[docs/asset-policy.md](docs/asset-policy.md).
