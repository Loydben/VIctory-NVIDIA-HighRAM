# Starts the title the way this fork is played (README.md): Vice-Tay's scripts\play.ps1 for the PC's card, plus
# every title buffer copied to the card (VICTORY_CARD_ALL, relay results checked by VICTORY_FETCH_CHECK) and, with
# -FsrDll, AMD FSR through the PC's FidelityFX DLL. Series S profile in a window by default, as measured.
#   scripts\play-fsr.ps1 -Exe <path of your own copy's executable> [-FsrDll <path of amd_fidelityfx_upscaler_dx12.dll>]
# -FsrDll      AMD FSR: the title's upscaler calls go to this PC DLL (AMD's FidelityFX SDK, not shipped here)
# -SeriesX     the Series X profile (sharper, slower) instead of Series S
# -FullScreen  the whole screen instead of a window
# -NoPanel     without the performance panel (F3 shows/hides it; Ctrl+drag moves it, Ctrl+Plus/Minus resizes it)
# -Safe        Vice-Tay's safe set (the first thing to try when something is wrong)
# -LowMemory   without VICTORY_CARD_ALL / VICTORY_FETCH_CHECK (cards under 8 GB; slower: the card is waited for)
# -More        other settings: -More @{VICTORY_X='1'}
param(
    [Parameter(Mandatory)][string]$Exe,
    [string]$FsrDll = '',
    [switch]$SeriesX,
    [switch]$FullScreen,
    [switch]$NoPanel,
    [switch]$LowMemory,
    [switch]$Safe,
    [hashtable]$More = @{}
)
$extra = @{}
if (-not $LowMemory) { $extra.VICTORY_CARD_ALL = '1'; $extra.VICTORY_FETCH_CHECK = '1' }
if ($FsrDll) {
    if (-not (Test-Path $FsrDll)) { Write-Host "No $FsrDll."; exit 1 }
    $extra.VICTORY_FSR_DLL = (Resolve-Path $FsrDll).Path
}
foreach ($k in $More.Keys) { $extra[$k] = $More[$k] }
$play = @{ Exe = $Exe; Console = $(if ($SeriesX) { 'series-x' } else { 'series-s' }); More = $extra }
if (-not $FullScreen) { $play.Windowed = $true }
if ($NoPanel) { $play.NoPanel = $true }
if ($Safe) { $play.Safe = $true }
& (Join-Path $PSScriptRoot 'play.ps1') @play
