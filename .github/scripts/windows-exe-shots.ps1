#Requires -Version 7
# Screenshots and timings on a Windows runner, with the fake op.exe: op-reason.exe next to the PowerShell entry of the
# same tree (-Entry op-reason.ps1, -EntryCmd op-reason.cmd) and its launcher started directly (-Launcher launcher.ps1,
# the second half of every call through the PowerShell entry). The numbers help to choose a Windows entry (README).
#
#   pwsh -File .github/scripts/windows-exe-shots.ps1 -Exe target\release\op-reason.exe -FakeOp target\debug\op.exe `
#     -Launcher launcher\launcher.ps1 -Entry windows\op-reason.ps1 -EntryCmd windows\op-reason.cmd -Out shots
#
# Writes <Out>\*.png (the window per scene, and the whole screen once), <Out>\timing.json and <Out>\timing.md, and
# appends timing.md to the job summary. Timings are black-box: from starting the process until a visible window with
# the expected title exists (FindWindow), and until the process exits.
#
# ASCII only: Japanese strings are written as \u escapes (Windows PowerShell would misread a file without a BOM).
[CmdletBinding()]
param(
  [Parameter(Mandatory)] [string] $Exe,
  [Parameter(Mandatory)] [string] $FakeOp,
  [Parameter(Mandatory)] [string] $Launcher,
  [Parameter(Mandatory)] [string] $Entry,
  [Parameter(Mandatory)] [string] $EntryCmd,
  [Parameter(Mandatory)] [string] $Out,
  [int] $Runs = 10
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class OpReasonShots {
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
'@
[void][OpReasonShots]::SetProcessDPIAware()

$u = { param($s) [regex]::Unescape($s) }
$TitleJa = & $u '1Password CLI \u306e\u627f\u8a8d\u306e\u4f9d\u983c'
$TitleEn = '1Password CLI approval request'

$Exe = (Resolve-Path $Exe).Path
$FakeOp = (Resolve-Path $FakeOp).Path
$Launcher = (Resolve-Path $Launcher).Path
$Entry = (Resolve-Path $Entry).Path
$EntryCmd = (Resolve-Path $EntryCmd).Path
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$work = Join-Path ([IO.Path]::GetTempPath()) ("op-reason-shots-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $work | Out-Null

# Starts one process. LOCALAPPDATA points at a scratch directory so that records stay out of the runner profile.
function Start-Proc([string] $File, [string[]] $Arguments, [hashtable] $Env) {
  $psi = [Diagnostics.ProcessStartInfo]::new($File)
  foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.Environment['OPREASON_OP_EXE'] = $FakeOp
  $psi.Environment['LOCALAPPDATA'] = $work
  foreach ($k in @('OP_SERVICE_ACCOUNT_TOKEN', 'OP_CONNECT_HOST', 'OP_CONNECT_TOKEN', 'CLAUDECODE', 'CLAUDE_PID', 'WT_SESSION')) {
    [void]$psi.Environment.Remove($k)
  }
  if ($Env) { foreach ($k in $Env.Keys) { $psi.Environment[$k] = [string]$Env[$k] } }
  $p = [Diagnostics.Process]::Start($psi)
  $p.StandardInput.Close()
  [void]$p.StandardOutput.ReadToEndAsync()
  [void]$p.StandardError.ReadToEndAsync()
  return $p
}

# The place as both entries show it (repo and branch from git, the directory with ~).
$origin = (git config --get remote.origin.url) -replace '\.git$', '' -replace '/$', ''
$repo = ($origin -split '[:/]')[-2] + '/' + ($origin -split '[:/]')[-1]
$branch = git symbolic-ref --short -q HEAD
if (-not $branch) { $branch = '(detached ' + (git rev-parse --short=7 HEAD) + ')' }
$cwd = (Get-Location).Path
if ($env:USERPROFILE -and $cwd.StartsWith($env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase)) {
  $cwd = '~' + $cwd.Substring($env:USERPROFILE.Length)
}

# Starts the launcher for one desktop call with the payload of the launcher contract (docs/architecture/README.md).
function Start-Launcher([string[]] $OpArgs, [string] $Lang, [hashtable] $Caller, [string] $Reason, [string] $Command, [hashtable] $Env) {
  $facts = [ordered]@{
    time    = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
    caller  = $Caller
    herdr   = $null
    place   = [ordered]@{ repo = $repo; branch = $branch; cwd = $cwd }
    reason  = $Reason
    command = $Command
    notes   = @()
  }
  $payload = [ordered]@{
    v        = 1
    mode     = 'desktop'
    op       = $FakeOp
    args64   = @($OpArgs | ForEach-Object { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($_)) })
    lang     = $Lang
    lock     = 'held'
    lockWait = 90
    stderr   = 'inherit'
    facts    = $facts
  }
  $json = $payload | ConvertTo-Json -Compress -Depth 6 -EscapeHandling EscapeNonAscii
  $all = @{ OPREASON_PAYLOAD = $json }
  if ($Env) { foreach ($k in $Env.Keys) { $all[$k] = $Env[$k] } }
  Start-Proc 'powershell.exe' @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-OutputFormat', 'Text', '-File', $Launcher) $all
}

# The PowerShell entry as a program or cmd.exe runs it (the shim starts powershell.exe -File op-reason.ps1), and
# powershell.exe -File op-reason.ps1 without cmd.exe in front.
function Start-EntryCmd([string[]] $Arguments, [hashtable] $Env) {
  Start-Proc 'cmd.exe' (@('/d', '/c', $EntryCmd) + $Arguments) $Env
}
function Start-EntryPs1([string[]] $Arguments, [hashtable] $Env) {
  Start-Proc 'powershell.exe' (@('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $Entry) + $Arguments) $Env
}

# Waits until a visible top-level window with the title exists. Returns the handle (or zero on time-out).
function Wait-Window([string] $Title, [int] $TimeoutMs = 30000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
    # [NullString]::Value: PowerShell would pass $null as "" (a class named ""), which matches no window
    $h = [OpReasonShots]::FindWindowW([NullString]::Value, $Title)
    if ($h -ne [IntPtr]::Zero -and [OpReasonShots]::IsWindowVisible($h)) { return $h }
    [Threading.Thread]::Sleep(1)
  }
  return [IntPtr]::Zero
}

function Save-Rect([int] $Left, [int] $Top, [int] $Width, [int] $Height, [string] $Path) {
  $bmp = [Drawing.Bitmap]::new($Width, $Height)
  $g = [Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($Left, $Top, 0, 0, $bmp.Size)
  $bmp.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose()
  $bmp.Dispose()
}

function Save-Window([IntPtr] $Handle, [string] $Path) {
  $r = [OpReasonShots+RECT]::new()
  [void][OpReasonShots]::GetWindowRect($Handle, [ref]$r)
  $m = 8
  Save-Rect ($r.Left - $m) ($r.Top - $m) ($r.Right - $r.Left + 2 * $m) ($r.Bottom - $r.Top + 2 * $m) $Path
}

function Save-Screen([string] $Path) {
  $b = [Windows.Forms.Screen]::PrimaryScreen.Bounds
  Save-Rect $b.Left $b.Top $b.Width $b.Height $Path
}

# --- screenshots

$reasonJa = & $u '\u30c7\u30d7\u30ed\u30a4\u306e\u305f\u3081\u306b Proxmox \u306e API \u306e token \u3092\u8aad\u3080'
$longReason = 'Read the API token of the staging cluster to rotate the deploy key, then write the new key back to the vault item so that the nightly job can pick it up without anyone copying secrets by hand'
$readArgs = @('read', 'op://Private/proxmox/token')
$claudeEnv = @{ OPREASON_LANG = 'ja'; CLAUDECODE = '1'; CLAUDE_PID = '4242'; CLAUDE_CODE_SESSION_ID = 'demo' }
$scenes = @(
  @{ name = '01-exe-claude-ja'; title = $TitleJa; env = $claudeEnv; args = @('--reason', $reasonJa) + $readArgs },
  @{ name = '02-entry-claude-ja'; title = $TitleJa; entry = $true; env = $claudeEnv; args = @('--reason', $reasonJa) + $readArgs },
  @{ name = '03-exe-person-en'; title = $TitleEn; env = @{ OPREASON_LANG = 'en'; WT_SESSION = '00000000-0000-0000-0000-000000000000' };
     args = @('--reason', 'Read the Proxmox API token to create a VM') + $readArgs },
  @{ name = '04-exe-others-ja'; title = $TitleJa; env = @{ OPREASON_LANG = 'ja' }; others = $true;
     args = @('--reason', $reasonJa, 'item', 'get', 'proxmox', '--fields', 'password') },
  @{ name = '05-exe-long-en'; title = $TitleEn; env = @{ OPREASON_LANG = 'en' };
     args = @('--reason', $longReason, 'item', 'create', '--category', 'login', '--title', 'deploy key', 'username=ci', 'password=not-shown') }
)
# Minimize the runner's own windows (its console shows job logs), so that the screen shot shows only our window.
(New-Object -ComObject Shell.Application).MinimizeAll()
[Threading.Thread]::Sleep(1000)
$screenTaken = $false
foreach ($s in $scenes) {
  $other = $null
  if ($s.others) {
    # an op.exe that is not ours: the window shows the orange line
    $other = Start-Proc $FakeOp @('vault', 'list') @{ FAKE_SLEEP_MS = '15000' }
    [Threading.Thread]::Sleep(300)
  }
  if ($s.entry) {
    $p = Start-EntryPs1 $s.args (@{ FAKE_SLEEP_MS = '6000' } + $s.env)
  } else {
    $p = Start-Proc $Exe $s.args (@{ FAKE_SLEEP_MS = '4000' } + $s.env)
  }
  $h = Wait-Window $s.title
  if ($h -eq [IntPtr]::Zero) { throw "no window for scene $($s.name)" }
  [Threading.Thread]::Sleep(800)
  Save-Window $h (Join-Path $Out "$($s.name).png")
  if (-not $screenTaken) {
    Save-Screen (Join-Path $Out '00-screen.png')
    $screenTaken = $true
  }
  $p.WaitForExit()
  if ($p.ExitCode -ne 0) { throw "scene $($s.name) exited with $($p.ExitCode)" }
  if ($other) { $other.Kill(); $other.WaitForExit() }
}

# --- timings

function Measure-Runs([string] $Name, [scriptblock] $One) {
  $vals = @()
  for ($i = 0; $i -lt $Runs; $i++) { $vals += & $One }
  $sorted = $vals | Sort-Object
  [pscustomobject]@{
    name   = $Name
    runs   = $Runs
    min    = [math]::Round($sorted[0])
    median = [math]::Round($sorted[[int][math]::Floor(($sorted.Count - 1) / 2)])
    max    = [math]::Round($sorted[-1])
  }
}

function Measure-Window([scriptblock] $Start) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = & $Start
  $h = Wait-Window $TitleEn
  $t = $sw.Elapsed.TotalMilliseconds
  $p.WaitForExit()
  if ($h -eq [IntPtr]::Zero) { throw 'no window while timing' }
  $t
}

function Measure-Whole([scriptblock] $Start) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = & $Start
  $p.WaitForExit()
  $sw.Elapsed.TotalMilliseconds
}

$argsTiming = @('--reason', 'timing', 'vault', 'list')
$humanCaller = [ordered]@{ type = 'human'; pid = $null; session = $null; detail = $null }
$results = @()
$results += Measure-Runs 'op-reason.exe: start -> window visible' {
  Measure-Window { Start-Proc $Exe $argsTiming @{ OPREASON_LANG = 'en'; FAKE_SLEEP_MS = '1500' } }
}
$results += Measure-Runs 'op-reason.exe: whole desktop call (window, toast, op, whoami, signout)' {
  Measure-Whole { Start-Proc $Exe $argsTiming @{ OPREASON_LANG = 'en'; FAKE_WHOAMI_RC = '0' } }
}
$results += Measure-Runs 'op-reason.exe: help call (op vault --help: op.exe, no window)' {
  Measure-Whole { Start-Proc $Exe @('vault', '--help') @{ OPREASON_LANG = 'en' } }
}
$results += Measure-Runs 'op-reason.exe: op-reason --version (no op.exe)' {
  Measure-Whole { Start-Proc $Exe @('--version') @{} }
}
$results += Measure-Runs 'PowerShell entry via op-reason.cmd: start -> window visible' {
  Measure-Window { Start-EntryCmd $argsTiming @{ OPREASON_LANG = 'en'; FAKE_SLEEP_MS = '1500' } }
}
$results += Measure-Runs 'PowerShell entry via op-reason.cmd: whole desktop call' {
  Measure-Whole { Start-EntryCmd $argsTiming @{ OPREASON_LANG = 'en'; FAKE_WHOAMI_RC = '0' } }
}
$results += Measure-Runs 'PowerShell entry via powershell -File op-reason.ps1: start -> window visible' {
  Measure-Window { Start-EntryPs1 $argsTiming @{ OPREASON_LANG = 'en'; FAKE_SLEEP_MS = '1500' } }
}
$results += Measure-Runs 'PowerShell entry via powershell -File op-reason.ps1: whole desktop call' {
  Measure-Whole { Start-EntryPs1 $argsTiming @{ OPREASON_LANG = 'en'; FAKE_WHOAMI_RC = '0' } }
}
$results += Measure-Runs 'PowerShell entry via powershell -File op-reason.ps1: op-reason --version' {
  Measure-Whole { Start-EntryPs1 @('--version') @{} }
}
$results += Measure-Runs 'launcher alone: start -> window visible' {
  Measure-Window { Start-Launcher @('vault', 'list') 'en' $humanCaller 'timing' 'op vault list' @{ FAKE_SLEEP_MS = '1500' } }
}
$results += Measure-Runs 'launcher alone: whole desktop call (window, toast, op, whoami, signout)' {
  Measure-Whole { Start-Launcher @('vault', 'list') 'en' $humanCaller 'timing' 'op vault list' @{ FAKE_WHOAMI_RC = '0' } }
}
# what one more powershell.exe start costs (the PowerShell entry starts one before the launcher)
$results += Measure-Runs 'reference: powershell.exe -NoProfile -Command exit' {
  Measure-Whole { Start-Proc 'powershell.exe' @('-NoProfile', '-NonInteractive', '-Command', 'exit') @{} }
}
$results += Measure-Runs 'reference: the fake op.exe alone' {
  Measure-Whole { Start-Proc $FakeOp @('vault', 'list') @{} }
}

# what the last desktop call of the exe recorded (window, toast, sign-out)
$lastEnd = Get-Content (Join-Path $work 'op-reason\log.jsonl') |
  Where-Object { $_ -match '"event":"end"' -and $_ -notmatch '"window":"skipped"' } | Select-Object -Last 1
$size = (Get-Item $Exe).Length
$os = (Get-CimInstance Win32_OperatingSystem).Caption
$md = @(
  '### op-reason.exe and the PowerShell entry: timings',
  '',
  "Runner: $os. $Runs runs each, in milliseconds. Fake op.exe; black-box from process start.",
  'The PowerShell entry rows run op-reason.ps1 as a program or cmd.exe would; the launcher rows start the launcher directly (the second half of such a call).',
  '',
  '| measurement | min | median | max |',
  '| --- | ---: | ---: | ---: |'
)
foreach ($r in $results) { $md += "| $($r.name) | $($r.min) | $($r.median) | $($r.max) |" }
$md += ''
$md += "op-reason.exe (release): $([math]::Round($size / 1KB)) KiB"
$md += ''
$md += 'Last desktop `end` record of the exe:'
$md += ''
$md += ('```json')
$md += $lastEnd
$md += ('```')
$md -join "`n" | Set-Content -Path (Join-Path $Out 'timing.md') -Encoding utf8
@{ runner = $os; exe_bytes = $size; results = $results } | ConvertTo-Json -Depth 4 |
  Set-Content -Path (Join-Path $Out 'timing.json') -Encoding utf8
if ($env:GITHUB_STEP_SUMMARY) { $md -join "`n" | Add-Content -Path $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
$md -join "`n"
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
