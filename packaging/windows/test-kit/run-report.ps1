# OpenAMP Windows test report.
#
# Runs the player and the CLI from the folder above this one, records what happens, and takes
# screenshots. Everything goes into .\report\ next to this script; nothing outside it is
# written: the player's settings, data and cache are redirected into report\sandbox.
#
#   powershell -ExecutionPolicy Bypass -File .\run-report.ps1            # full run
#   powershell -ExecutionPolicy Bypass -File .\run-report.ps1 -NoSound   # skip the audible check
#
# Dot-source with -LoadOnly to get the helpers (Start-Player, Get-PlayerWindows,
# Save-Screenshot, Invoke-Drag, Invoke-Click, Send-Keys) for further checks by hand:
#   . .\run-report.ps1 -LoadOnly
#
# Windows PowerShell 5.1 compatible.

param(
    [switch]$NoSound,
    [switch]$LoadOnly
)

$ErrorActionPreference = 'Stop'
$Kit = Split-Path -Parent $MyInvocation.MyCommand.Path
$App = Split-Path -Parent $Kit
$Player = Join-Path $App 'openamp.exe'
$Cli = Join-Path $App 'openamp-cli.exe'
$Report = Join-Path $Kit 'report'
$Sandbox = Join-Path $Report 'sandbox'

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class OpenAmpWin32 {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    public delegate bool EnumProc(IntPtr hwnd, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lparam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT r);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int max);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, int data, IntPtr extra);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern uint GetDpiForSystem();

    public static List<IntPtr> Windows(uint pid) {
        var found = new List<IntPtr>();
        EnumWindows((h, l) => {
            uint p;
            GetWindowThreadProcessId(h, out p);
            if (p == pid && IsWindowVisible(h)) found.Add(h);
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static string Title(IntPtr h) {
        var sb = new StringBuilder(256);
        GetWindowText(h, sb, sb.Capacity);
        return sb.ToString();
    }
}
'@
# Physical pixels, like the player (per-monitor aware), so screenshots and window rectangles
# line up. -4 is DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2; older systems get system aware.
try {
    if (-not [OpenAmpWin32]::SetProcessDpiAwarenessContext([IntPtr](-4))) { [void][OpenAmpWin32]::SetProcessDPIAware() }
} catch { [void][OpenAmpWin32]::SetProcessDPIAware() }

$script:Results = New-Object System.Collections.ArrayList
$script:PlayerProc = $null

function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $mark = if ($Ok) { 'PASS' } else { 'FAIL' }
    Write-Host ("  {0}  {1}  {2}" -f $mark, $Name, $Detail)
    [void]$script:Results.Add([pscustomobject]@{ check = $Name; ok = $Ok; detail = $Detail })
}

function Set-SandboxEnv {
    $env:OPENAMP_CONFIG_DIR = Join-Path $Sandbox 'config'
    $env:OPENAMP_DATA_DIR = Join-Path $Sandbox 'data'
    $env:OPENAMP_CACHE_DIR = Join-Path $Sandbox 'cache'
    $env:OPENAMP_MUSIC_DIR = Join-Path $Sandbox 'music'
    $env:OPENAMP_LOG = 'info'
    New-Item -ItemType Directory -Force -Path (Join-Path $Sandbox 'music') | Out-Null
}

# Runs the CLI, logging the command, exit code and output to report\cli.txt.
function Invoke-Cli([string[]]$CliArgs, [int]$TimeoutSec = 60) {
    $out = Join-Path $Report 'cli-last-out.txt'
    $err = Join-Path $Report 'cli-last-err.txt'
    $quoted = $CliArgs | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
    $p = Start-Process -FilePath $Cli -ArgumentList $quoted -NoNewWindow -PassThru `
        -RedirectStandardOutput $out -RedirectStandardError $err
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        $p.Kill()
        $code = 'timeout'
    } else {
        $code = $p.ExitCode
    }
    $text = [string](Get-Content $out -Raw -ErrorAction SilentlyContinue) + [string](Get-Content $err -Raw -ErrorAction SilentlyContinue)
    Add-Content -Path (Join-Path $Report 'cli.txt') -Value ("> openamp-cli {0}`r`nexit: {1}`r`n{2}`r`n" -f ($quoted -join ' '), $code, $text)
    return [pscustomobject]@{ code = $code; text = $text }
}

# A 3 s, 1 kHz, 16-bit mono WAV at about -18 dBFS.
function New-ToneWav([string]$Path, [int]$Seconds = 3) {
    $rate = 44100
    $frames = $rate * $Seconds
    $fs = [System.IO.File]::Create($Path)
    $w = New-Object System.IO.BinaryWriter($fs)
    $w.Write([Text.Encoding]::ASCII.GetBytes('RIFF')); $w.Write([int](36 + $frames * 2))
    $w.Write([Text.Encoding]::ASCII.GetBytes('WAVEfmt ')); $w.Write([int]16)
    $w.Write([int16]1); $w.Write([int16]1); $w.Write([int]$rate); $w.Write([int]($rate * 2))
    $w.Write([int16]2); $w.Write([int16]16)
    $w.Write([Text.Encoding]::ASCII.GetBytes('data')); $w.Write([int]($frames * 2))
    for ($i = 0; $i -lt $frames; $i++) {
        $w.Write([int16][math]::Round(4096 * [math]::Sin(2 * [math]::PI * 1000 * $i / $rate)))
    }
    $w.Close()
}

function Start-Player([string[]]$PlayerArgs = @()) {
    Stop-Player
    Set-SandboxEnv
    $log = Join-Path $Report ('player-' + (Get-Date -Format 'HHmmss') + '.log')
    $quoted = $PlayerArgs | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
    $startArgs = @{ FilePath = $Player; PassThru = $true; RedirectStandardError = $log }
    if ($quoted) { $startArgs.ArgumentList = $quoted }
    $script:PlayerProc = Start-Process @startArgs
    # Wait for the main, equalizer and playlist windows.
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline -and -not $script:PlayerProc.HasExited) {
        if ((Get-PlayerWindows).Count -ge 3) { break }
        Start-Sleep -Milliseconds 250
    }
    Start-Sleep -Milliseconds 800
    return $script:PlayerProc
}

function Stop-Player {
    if ($script:PlayerProc -and -not $script:PlayerProc.HasExited) {
        $script:PlayerProc.CloseMainWindow() | Out-Null
        if (-not $script:PlayerProc.WaitForExit(3000)) { $script:PlayerProc.Kill() }
    }
    $script:PlayerProc = $null
}

# The player's visible windows: title, rectangle, DPI, extended style.
function Get-PlayerWindows {
    if (-not $script:PlayerProc -or $script:PlayerProc.HasExited) { return @() }
    $list = @()
    foreach ($h in [OpenAmpWin32]::Windows([uint32]$script:PlayerProc.Id)) {
        $r = New-Object OpenAmpWin32+RECT
        [void][OpenAmpWin32]::GetWindowRect($h, [ref]$r)
        $list += [pscustomobject]@{
            hwnd = $h; title = [OpenAmpWin32]::Title($h)
            x = $r.Left; y = $r.Top; w = $r.Right - $r.Left; h = $r.Bottom - $r.Top
            dpi = [OpenAmpWin32]::GetDpiForWindow($h)
            exstyle = ('0x{0:x8}' -f [OpenAmpWin32]::GetWindowLong($h, -20))
        }
    }
    return , $list
}

# Saves the whole desktop, and a crop around the player's windows, as PNGs in report\.
function Save-Screenshot([string]$Name) {
    $vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap($vs.Width, $vs.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($vs.Left, $vs.Top, 0, 0, $bmp.Size)
    $g.Dispose()
    $bmp.Save((Join-Path $Report "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $wins = Get-PlayerWindows
    if ($wins.Count -gt 0) {
        $l = ($wins | Measure-Object -Property x -Minimum).Minimum - 20
        $t = ($wins | Measure-Object -Property y -Minimum).Minimum - 20
        $r = ($wins | ForEach-Object { $_.x + $_.w } | Measure-Object -Maximum).Maximum + 20
        $b = ($wins | ForEach-Object { $_.y + $_.h } | Measure-Object -Maximum).Maximum + 20
        $rect = New-Object System.Drawing.Rectangle(($l - $vs.Left), ($t - $vs.Top), ($r - $l), ($b - $t))
        $rect.Intersect((New-Object System.Drawing.Rectangle(0, 0, $vs.Width, $vs.Height)))
        if ($rect.Width -gt 0 -and $rect.Height -gt 0) {
            $crop = $bmp.Clone($rect, $bmp.PixelFormat)
            $crop.Save((Join-Path $Report "$Name-player.png"), [System.Drawing.Imaging.ImageFormat]::Png)
            $crop.Dispose()
        }
    }
    $bmp.Dispose()
}

function Invoke-Click([int]$X, [int]$Y, [switch]$Right) {
    [void][OpenAmpWin32]::SetCursorPos($X, $Y)
    Start-Sleep -Milliseconds 100
    $down = if ($Right) { 0x0008 } else { 0x0002 }
    [OpenAmpWin32]::mouse_event($down, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [OpenAmpWin32]::mouse_event($down * 2, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 300
}

# Presses at (X1, Y1), moves to (X2, Y2) in steps, releases.
function Invoke-Drag([int]$X1, [int]$Y1, [int]$X2, [int]$Y2, [int]$Steps = 20) {
    [void][OpenAmpWin32]::SetCursorPos($X1, $Y1)
    Start-Sleep -Milliseconds 100
    [OpenAmpWin32]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
    for ($i = 1; $i -le $Steps; $i++) {
        [void][OpenAmpWin32]::SetCursorPos([int]($X1 + ($X2 - $X1) * $i / $Steps), [int]($Y1 + ($Y2 - $Y1) * $i / $Steps))
        Start-Sleep -Milliseconds 20
    }
    [OpenAmpWin32]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 400
}

# Sends virtual-key codes, for example Send-Keys 0x1B for Escape or Send-Keys 0x58 for X.
function Send-Keys([byte[]]$Keys) {
    foreach ($k in $Keys) {
        [OpenAmpWin32]::keybd_event($k, 0, 0, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 40
        [OpenAmpWin32]::keybd_event($k, 0, 2, [IntPtr]::Zero)
        Start-Sleep -Milliseconds 120
    }
}

if ($LoadOnly) { return }

# ---------------------------------------------------------------------------------------
Remove-Item -Recurse -Force $Report -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $Report | Out-Null
Set-SandboxEnv
Write-Host "OpenAMP test report -> $Report"

# System facts (no user names or paths).
$os = Get-CimInstance Win32_OperatingSystem
$screens = [System.Windows.Forms.Screen]::AllScreens | ForEach-Object {
    "{0}  bounds {1}  work area {2}" -f $(if ($_.Primary) { 'primary' } else { 'other  ' }), $_.Bounds, $_.WorkingArea
}
$audio = Get-CimInstance Win32_SoundDevice -ErrorAction SilentlyContinue | ForEach-Object { $_.Name }
@(
    "OS: $($os.Caption) $($os.Version) build $($os.BuildNumber) ($($os.OSArchitecture))"
    "System DPI: $([OpenAmpWin32]::GetDpiForSystem())"
    "Monitors:"; $screens | ForEach-Object { "  $_" }
    "Sound devices:"; $audio | ForEach-Object { "  $_" }
    "PowerShell: $($PSVersionTable.PSVersion)"
) | Set-Content -Path (Join-Path $Report 'system.txt')
Get-Content (Join-Path $Report 'system.txt') | ForEach-Object { Write-Host "  $_" }

Write-Host "CLI"
$r = Invoke-Cli @('--version')
Add-Result 'cli --version' ($r.code -eq 0) ($r.text.Trim())

$r = Invoke-Cli @('paths')
Add-Result 'cli paths (sandboxed)' ($r.code -eq 0 -and $r.text -match [regex]::Escape($Sandbox)) ''

$tone = Join-Path $Sandbox 'music\tone.wav'
New-ToneWav $tone
$r = Invoke-Cli @('probe', $tone)
Add-Result 'cli probe wav' ($r.code -eq 0) (($r.text -split "`n" | Select-Object -First 1).Trim())

$decoded = Join-Path $Sandbox 'decoded.wav'
$r = Invoke-Cli @('decode', $tone, '-o', $decoded)
$len = if (Test-Path $decoded) { (Get-Item $decoded).Length } else { 0 }
# 3 s of 32-bit float stereo at 44.1 kHz is 1,058,400 bytes plus a small header.
Add-Result 'cli decode wav' ($r.code -eq 0 -and [math]::Abs($len - 1058400) -lt 2000) "$len bytes"

$unicode = Join-Path $Sandbox ("music\" + [char]0x30C6 + [char]0x30B9 + [char]0x30C8 + " " + [char]0x00FC + ".wav")
Copy-Item $tone $unicode
$r = Invoke-Cli @('probe', $unicode)
Add-Result 'cli probe non-ASCII file name' ($r.code -eq 0) ''

$r = Invoke-Cli @('devices')
Add-Result 'cli devices' ($r.code -eq 0 -and $r.text.Trim().Length -gt 0) ((($r.text -split "`n") | Where-Object { $_.Trim() } | Measure-Object).Count.ToString() + ' lines')

if (-not $NoSound) {
    # Audible: a quiet 3 s tone on the default device.
    $r = Invoke-Cli @('play', '--volume', '20', $tone) 30
    Add-Result 'cli play on the default device (quiet tone, 3 s)' ($r.code -eq 0) "exit $($r.code)"
}
$r = Invoke-Cli @('play', '--sink', 'fast', $tone, $unicode)
Add-Result 'cli play to null sink' ($r.code -eq 0) ''

$r = Invoke-Cli @('library', 'add', (Join-Path $Sandbox 'music'))
$r2 = Invoke-Cli @('library', 'scan')
$r3 = Invoke-Cli @('library', 'stats')
Add-Result 'cli library add/scan/stats' ($r.code -eq 0 -and $r2.code -eq 0 -and $r3.text -match '^2 tracks') ($r3.text.Trim())

Write-Host "Player"
$p = Start-Player @('--audio', 'null')
$wins = Get-PlayerWindows
Add-Result 'player starts and shows 3 windows' ((-not $p.HasExited) -and $wins.Count -ge 3) "$($wins.Count) windows, exited=$($p.HasExited)"
Save-Screenshot '01-start'
$wins | Select-Object title, x, y, w, h, dpi, exstyle | Format-Table | Out-String | Set-Content (Join-Path $Report 'windows-01-start.txt')

if ($wins.Count -ge 3) {
    $main = $wins | Sort-Object { $_.y } | Select-Object -First 1
    # Drag the main window by its title bar: docked windows must follow by the same amount.
    $before = $wins
    Invoke-Drag ($main.x + 60) ($main.y + 6) ($main.x + 60 + 150) ($main.y + 6 + 100)
    $after = Get-PlayerWindows
    $moved = @()
    foreach ($b in $before) {
        $a = $after | Where-Object { $_.hwnd -eq $b.hwnd }
        if ($a) { $moved += "{0},{1}" -f ($a.x - $b.x), ($a.y - $b.y) }
    }
    $same = ($moved | Select-Object -Unique).Count -eq 1
    Add-Result 'drag main window moves the docked group together' ($same -and $moved[0] -ne '0,0') ($moved -join ' ')
    Save-Screenshot '02-dragged'
    $after | Select-Object title, x, y, w, h | Format-Table | Out-String | Set-Content (Join-Path $Report 'windows-02-dragged.txt')

    # Right-click opens the main menu (a fourth, topmost window); Escape closes it.
    $main = Get-PlayerWindows | Sort-Object { $_.y } | Select-Object -First 1
    Invoke-Click ($main.x + 100) ($main.y + 40) -Right
    Start-Sleep -Milliseconds 500
    $n = (Get-PlayerWindows).Count
    Save-Screenshot '03-menu'
    Add-Result 'right-click opens a menu window' ($n -gt $wins.Count) "$n windows"
    Send-Keys @(0x1B)
    Start-Sleep -Milliseconds 400
    $n2 = (Get-PlayerWindows).Count
    Add-Result 'Escape closes the menu' ($n2 -eq $wins.Count) "$n2 windows"

    # The playlist editor toggles with Alt+E in the classic player; here we just check that
    # the player still responds after the input above.
    Add-Result 'player still running after input' (-not $script:PlayerProc.HasExited) ''
}
Stop-Player

# Double size, and the same playlist with a track so the title and time show.
$p = Start-Player @('--audio', 'null', '--scale', '2', $tone)
Save-Screenshot '04-scale2-with-track'
$wins = Get-PlayerWindows
$main = $wins | Sort-Object { $_.y } | Select-Object -First 1
Add-Result 'scale 2 main window is 550x232' ($main -and $main.w -eq 550 -and $main.h -eq 232) $(if ($main) { "$($main.w)x$($main.h)" } else { 'no window' })
Stop-Player

$results = $script:Results
$results | ConvertTo-Json | Set-Content (Join-Path $Report 'results.json')
Remove-Item (Join-Path $Report 'cli-last-*.txt') -ErrorAction SilentlyContinue
$failed = ($results | Where-Object { -not $_.ok }).Count
Write-Host ""
Write-Host ("{0} checks, {1} failed. Report folder: {2}" -f $results.Count, $failed, $Report)
