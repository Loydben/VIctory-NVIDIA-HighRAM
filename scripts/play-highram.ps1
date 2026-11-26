# Starts the title with the settings this fork is played with (README.md, "How to run it"): NVIDIA, Series S
# profile, the title's own render scale and upscaler, and with -HighRam (on by default) every title buffer copied
# to the card (VICTORY_CARD_ALL) and relay results checked (VICTORY_FETCH_CHECK).
#   scripts\play-highram.ps1 -Exe <path of your own copy's executable>
# -HighRam:$false  without VICTORY_CARD_ALL / VICTORY_FETCH_CHECK (smaller cards; slower)
# -HalfScale       the world at half the Series S size (faster, softer); other fixed scales stop the title
# -SeriesX         the Series X profile instead (sharper, much slower)
# -AutoStart       press Enter through the start screens (scripted keys, diagnostics build)
# -Build           target\diag (the default, the build measured) or target\release
# -More            other settings: -More @{VICTORY_PANEL='1'}
# Run it from a normal window, not as administrator: the driver then keeps its shader cache elsewhere.
param(
    [Parameter(Mandatory)][string]$Exe,
    [bool]$HighRam = $true,
    [switch]$HalfScale,
    [switch]$SeriesX,
    [switch]$AutoStart,
    [ValidateSet('diag', 'release')][string]$Build = 'diag',
    [hashtable]$More = @{}
)
$settings = @{
    # The base fork's play set (scripts\play.ps1, not -Safe).
    VICTORY_TEXTURE_NO_WAIT = '1'; VICTORY_RELAY = '2'; VICTORY_WIDE_BACK = '1'; VICTORY_CARD_REHASH_RUNS = '1'
    VICTORY_JOIN_COPIES = '1'; VICTORY_INDIRECT_WINDOW = '1'; VICTORY_RELAY_COPY = '1'; VICTORY_CARD_STEPS = '2'
    VICTORY_INDIRECT_BATCH = '2'; VICTORY_MAP_THREADS = '1'; VICTORY_TITLE_PRIORITIES = '0'; VICTORY_CARD_AWAKE = '1'
    VICTORY_SHOW_AFTER = '1'; VICTORY_AMBIENT = '0.8'; VICTORY_D3D12_DRED = 'light'; VICTORY_SKIP_PIPELINES = 'none'
    VICTORY_THIN = '1'; VICTORY_JOIN_PARTS = '8'; VICTORY_UPLOAD_AHEAD = '1'
    # VIctory-NVIDIA: wait for new shaders, more dynamic slots on NVIDIA's capped view heap, the title's own
    # upscaler, ray walks ended at the root, compares on the card, the Series S interface plane.
    VICTORY_PIPELINE_WAIT_MS = '10000'; VICTORY_DYNAMIC_SLOTS = '524288'; VICTORY_OWN_UPSCALER = '1'
    VICTORY_BVH_MISS = '1'; VICTORY_CARD_BUFFERS_MIN_MB = '1'; VICTORY_CARD_BUFFERS_MB = '1536'; VICTORY_COMPOSE = '1'
    # This fork: 16-bit float results rounded to nearest (no banding on NVIDIA).
    VICTORY_F16_NEAREST = '1'
}
if ($HighRam) { $settings.VICTORY_CARD_ALL = '1'; $settings.VICTORY_FETCH_CHECK = '1' }
if ($HalfScale) { $settings.VICTORY_RENDER_SCALE = '0.5' }
if ($AutoStart) { $settings.VICTORY_KEYS = '25:Enter,35:Enter,45:Enter,55:Enter,65:Enter,75:Enter,85:Enter,95:Enter,105:Enter,115:Enter' }
foreach ($k in $More.Keys) { $settings[$k] = $More[$k] }

$victory = Join-Path $PSScriptRoot "..\target\$Build\victory.exe"
if (-not (Test-Path $victory)) { Write-Host "No $victory. Build it first (README.md)."; return }
if (-not (Test-Path $Exe)) { Write-Host "No $Exe."; return }
# (Other settings left over in this window go; where your files are stays.)
$paths = 'VICTORY_CONFIG', 'VICTORY_GAME_DIR', 'VICTORY_CACHE_DIR', 'VICTORY_DATA_DIR'
Get-ChildItem Env:VICTORY_* | Where-Object { $_.Name -notin $paths } | ForEach-Object { Remove-Item "Env:$($_.Name)" }
foreach ($k in $settings.Keys) { Set-Item "Env:$k" $settings[$k] }
$Exe = (Resolve-Path $Exe).Path
$runArgs = @('run', $Exe, '--execute')
if (-not $SeriesX) { $runArgs += @('--console', 'series-s') }
Push-Location (Join-Path $PSScriptRoot '..')   # victory.toml is looked for here
try { & $victory @runArgs } finally { Pop-Location }
