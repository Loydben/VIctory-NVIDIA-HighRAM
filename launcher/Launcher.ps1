# A small window that builds VIctory from this repository and starts your own copy of the game with the settings
# of scripts\play-fsr.ps1. It ships nothing and downloads nothing: you point it at your game folder (and, if you
# have it, AMD's FSR DLL), press Build once, Prepare shaders once, then Play. See launcher\README.md.
# Started by Start-Launcher.cmd (double-click). Your choices are kept in %LOCALAPPDATA%\victory\launcher.json,
# outside the repository; saves and the shader cache go to VIctory's defaults under %LOCALAPPDATA%\victory.
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$root = Split-Path $PSScriptRoot -Parent
$victory = Join-Path $root 'target\diag\victory.exe'
$play = Join-Path $root 'scripts\play-fsr.ps1'
$stateDir = Join-Path $env:LOCALAPPDATA 'victory'
$stateFile = Join-Path $stateDir 'launcher.json'
$logFile = Join-Path $stateDir 'launcher-last.log'
New-Item -ItemType Directory -Force $stateDir | Out-Null

$state = @{ game = ''; fsr = ''; useFsr = $false; seriesX = $false; panel = $true }
if (Test-Path $stateFile) {
    try { (Get-Content $stateFile -Raw | ConvertFrom-Json).PSObject.Properties | ForEach-Object { $state[$_.Name] = $_.Value } } catch {}
}
function Save-State { $state | ConvertTo-Json | Set-Content $stateFile -Encoding utf8 }

# --- the window -------------------------------------------------------------------------------------------------
$font = New-Object System.Drawing.Font('Segoe UI', 10)
$form = New-Object System.Windows.Forms.Form
$form.Text = 'VIctory Launcher'
$form.ClientSize = New-Object System.Drawing.Size(640, 580)
$form.StartPosition = 'CenterScreen'
$form.FormBorderStyle = 'FixedSingle'; $form.MaximizeBox = $false
$form.Font = $font
$form.BackColor = [System.Drawing.Color]::FromArgb(24, 26, 30); $form.ForeColor = [System.Drawing.Color]::Gainsboro

function New-Label($text, $x, $y, $w = 600, $h = 22) {
    $l = New-Object System.Windows.Forms.Label; $l.Text = $text; $l.Location = "$x,$y"; $l.Size = "$w,$h"; $form.Controls.Add($l); $l
}
function New-Box($x, $y, $w) {
    $b = New-Object System.Windows.Forms.TextBox; $b.Location = "$x,$y"; $b.Size = "$w,26"
    $b.BackColor = [System.Drawing.Color]::FromArgb(40, 43, 50); $b.ForeColor = [System.Drawing.Color]::White
    $form.Controls.Add($b); $b
}
function New-Button($text, $x, $y, $w = 110, $h = 32) {
    $b = New-Object System.Windows.Forms.Button; $b.Text = $text; $b.Location = "$x,$y"; $b.Size = "$w,$h"
    $b.FlatStyle = 'Flat'; $b.BackColor = [System.Drawing.Color]::FromArgb(52, 56, 66); $b.ForeColor = [System.Drawing.Color]::White
    $form.Controls.Add($b); $b
}

$title = New-Label 'VIctory Launcher' 20 14 600 30
$title.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 15)
New-Label 'Your own copy of the game only. Nothing is downloaded or shipped by this launcher.' 20 46 | Out-Null

New-Label '1.  Your game folder (the one with game_scarlett_master.exe)' 20 82 | Out-Null
$gameBox = New-Box 20 106 490; $gameBox.Text = $state.game
$gameBrowse = New-Button 'Browse...' 520 104 100 28
$gameOk = New-Label '' 20 134 600 20

New-Label '2.  AMD FSR upscaling (optional)' 20 160 | Out-Null
$fsrNote = New-Label 'OFF = the best picture (recommended).  ON = a softer, less detailed picture for about the same speed (measured: 17.7 against 17.3 fps) and a little more RAM.' 20 182 600 38
$fsrNote.ForeColor = [System.Drawing.Color]::FromArgb(245, 208, 66)
$useFsr = New-Object System.Windows.Forms.CheckBox; $useFsr.Text = 'Use AMD FSR'; $useFsr.Location = '20,224'; $useFsr.Size = '130,26'
$form.Controls.Add($useFsr); $useFsr.Checked = [bool]$state.useFsr
$fsrBox = New-Box 150 224 360; $fsrBox.Text = $state.fsr
$fsrBrowse = New-Button 'Browse...' 520 222 100 28
New-Label 'amd_fidelityfx_upscaler_dx12.dll from AMD''s FidelityFX SDK (not shipped here)' 150 252 470 20 | Out-Null
function Update-Fsr { $fsrBox.Enabled = $useFsr.Checked; $fsrBrowse.Enabled = $useFsr.Checked }
$useFsr.Add_CheckedChanged({ Update-Fsr }); Update-Fsr

$seriesS = New-Object System.Windows.Forms.RadioButton; $seriesS.Text = 'Series S (faster)'; $seriesS.Location = '20,284'; $seriesS.Size = '170,24'
$seriesX = New-Object System.Windows.Forms.RadioButton; $seriesX.Text = 'Series X (sharper, slower)'; $seriesX.Location = '200,284'; $seriesX.Size = '220,24'
$panel = New-Object System.Windows.Forms.CheckBox; $panel.Text = 'FPS panel (F3)'; $panel.Location = '440,284'; $panel.Size = '190,24'
$form.Controls.AddRange(@($seriesS, $seriesX, $panel))
$seriesX.Checked = [bool]$state.seriesX; $seriesS.Checked = -not $seriesX.Checked; $panel.Checked = [bool]$state.panel

$build = New-Button 'Build' 20 322 140 40
$shaders = New-Button 'Prepare shaders' 170 322 160 40
$playBtn = New-Button 'PLAY' 480 322 140 40
$playBtn.BackColor = [System.Drawing.Color]::FromArgb(36, 140, 74); $playBtn.Font = New-Object System.Drawing.Font('Segoe UI Semibold', 12)
$status = New-Label '' 20 370 600 22

$log = New-Object System.Windows.Forms.TextBox
$log.Multiline = $true; $log.ReadOnly = $true; $log.ScrollBars = 'Vertical'; $log.Location = '20,396'; $log.Size = '600,168'
$log.BackColor = [System.Drawing.Color]::FromArgb(14, 15, 18); $log.ForeColor = [System.Drawing.Color]::Silver
$log.Font = New-Object System.Drawing.Font('Consolas', 9)
$form.Controls.Add($log)

# --- checks -----------------------------------------------------------------------------------------------------
function Get-GameExe { if ($gameBox.Text) { Join-Path $gameBox.Text.Trim('"', ' ') 'game_scarlett_master.exe' } }
function Update-Checks {
    $exe = Get-GameExe
    if ($exe -and (Test-Path $exe)) {
        $inside = (Resolve-Path $gameBox.Text.Trim('"', ' ')).Path.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)
        if ($inside) { $gameOk.Text = 'Move the game outside this repository folder: VIctory refuses game files inside it.'; $gameOk.ForeColor = 'Orange' }
        else { $gameOk.Text = 'Found game_scarlett_master.exe'; $gameOk.ForeColor = 'LightGreen' }
    } elseif ($gameBox.Text) { $gameOk.Text = 'No game_scarlett_master.exe in that folder.'; $gameOk.ForeColor = 'Orange' }
    else { $gameOk.Text = '' }
    $built = Test-Path $victory
    $build.Text = if ($built) { 'Rebuild' } else { 'Build' }
    $shaders.Enabled = $built -and $exe -and (Test-Path $exe)
    $playBtn.Enabled = $shaders.Enabled
    if (-not $built) { $status.Text = 'Step 3: press Build (first time only, 5-15 minutes).' }
    elseif (-not $playBtn.Enabled) { $status.Text = 'Pick your game folder.' }
    else { $status.Text = 'Ready. First time: Prepare shaders (about a minute), then PLAY.' }
}
function Write-Log($text) { $log.AppendText("$text`r`n") }

# --- long jobs: run beside the window, their output shown as it comes ----------------------------------------
$script:job = $null; $script:shown = 0
$timer = New-Object System.Windows.Forms.Timer; $timer.Interval = 700
$timer.Add_Tick({
    if (Test-Path $logFile) {
        $lines = @(Get-Content $logFile -ErrorAction SilentlyContinue)
        if ($lines.Count -gt $script:shown) { $lines[$script:shown..($lines.Count - 1)] | ForEach-Object { Write-Log $_ }; $script:shown = $lines.Count }
    }
    if ($script:job -and $script:job.HasExited) {
        $code = $script:job.ExitCode; $script:job = $null; $timer.Stop()
        Write-Log ($(if ($code -eq 0) { '--- done ---' } else { "--- stopped with code $code (see above) ---" }))
        $build.Enabled = $true; Update-Checks
    }
})
function Start-Job2($what, $commandLine) {
    if ($script:job) { return }
    Remove-Item $logFile -ErrorAction SilentlyContinue; $script:shown = 0
    Write-Log "--- $what ---"
    $script:job = Start-Process cmd -ArgumentList '/c', "$commandLine > `"$logFile`" 2>&1" -WorkingDirectory $root -WindowStyle Hidden -PassThru
    $build.Enabled = $false; $shaders.Enabled = $false; $playBtn.Enabled = $false
    $timer.Start()
}

$gameBrowse.Add_Click({
    $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = 'The folder that holds game_scarlett_master.exe'
    if ($d.ShowDialog() -eq 'OK') { $gameBox.Text = $d.SelectedPath }
})
$fsrBrowse.Add_Click({
    $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Filter = 'AMD FSR upscaler|amd_fidelityfx_upscaler_dx12.dll|DLL files|*.dll'
    if ($d.ShowDialog() -eq 'OK') { $fsrBox.Text = $d.FileName }
})
$gameBox.Add_TextChanged({ Update-Checks })

$build.Add_Click({
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        [System.Windows.Forms.MessageBox]::Show("Rust is not installed (no 'cargo').`n`nInstall Rust from https://rustup.rs and Visual Studio Build Tools with 'Desktop development with C++', then open this launcher again.", 'Build') | Out-Null
        return
    }
    Start-Job2 'Building VIctory (first time 5-15 minutes)' 'cargo build --locked --profile diag -p victory --features diagnostics'
})
$shaders.Add_Click({
    $env:VICTORY_GAME_DIR = $gameBox.Text.Trim('"', ' ')
    $rpf = Join-Path $env:VICTORY_GAME_DIR 'shaders.rpf'
    Start-Job2 'Translating the game''s shaders (about a minute)' "`"$victory`" shaders `"$rpf`""
})
$playBtn.Add_Click({
    if (Get-Process victory -ErrorAction SilentlyContinue) {
        [System.Windows.Forms.MessageBox]::Show('The game is already running. Close it first.', 'Play') | Out-Null; return
    }
    $state.game = $gameBox.Text.Trim('"', ' '); $state.fsr = $fsrBox.Text.Trim('"', ' '); $state.useFsr = $useFsr.Checked
    $state.seriesX = $seriesX.Checked; $state.panel = $panel.Checked; Save-State
    $env:VICTORY_GAME_DIR = $state.game
    $args2 = "& '$play' -Exe '$(Get-GameExe)'"
    if ($state.useFsr) {
        if (-not $state.fsr -or -not (Test-Path $state.fsr)) { [System.Windows.Forms.MessageBox]::Show('AMD FSR is ticked but no amd_fidelityfx_upscaler_dx12.dll is chosen. Pick it, or untick AMD FSR for the best picture.', 'Play') | Out-Null; return }
        $args2 += " -FsrDll '$($state.fsr)'"
    }
    if ($state.seriesX) { $args2 += ' -SeriesX' }
    if (-not $state.panel) { $args2 += ' -NoPanel' }
    Write-Log "--- PLAY: Series $(if ($state.seriesX) { 'X' } else { 'S' }), $(if ($state.useFsr) { 'AMD FSR on (softer picture)' } else { 'AMD FSR off (best picture)' }) ---"
    Start-Process powershell -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $args2 -WorkingDirectory $root -WindowStyle Minimized
    $status.Text = 'Starting... the game window opens in a moment (first start of a build takes longer).'
})
$form.Add_FormClosing({ $state.game = $gameBox.Text.Trim('"', ' '); $state.fsr = $fsrBox.Text.Trim('"', ' '); $state.useFsr = $useFsr.Checked; $state.seriesX = $seriesX.Checked; $state.panel = $panel.Checked; Save-State })

Update-Checks
[void]$form.ShowDialog()
