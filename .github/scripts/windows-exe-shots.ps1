#Requires -Version 7
# Screenshots and timings on a Windows runner, with the fake op.exe: op-reason.exe (its entry face, and its runner face
# `--run` as a WSL call starts it) next to the PowerShell entry of the same tree (-Entry op-reason.ps1, -EntryCmd
# op-reason.cmd) and its launcher started directly (-Launcher launcher.ps1: the second half of every call through the
# PowerShell entry, and the fallback of a WSL call). The numbers help to choose a Windows entry (README).
#
#   pwsh -File .github/scripts/windows-exe-shots.ps1 -Exe target\release\op-reason.exe -FakeOp target\debug\op.exe `
#     -Launcher launcher\launcher.ps1 -Entry windows\op-reason.ps1 -EntryCmd windows\op-reason.cmd -Out shots
#
# Writes <Out>\*.png (the window per scene, and the whole screen once), <Out>\timing.json and <Out>\timing.md, and
# appends timing.md to the job summary. Every window of the exe shows its language switch ("English | " and the
# Japanese name); one scene clicks the Japanese name in an English window with the real mouse and shows the window
# after the switch. The scenes with an op inject template show the references it reads: ID references as links (an
# account.json is put in place first), "and N more", and, without account.json, the line that says why there are no
# links (docs/superpowers/specs/2026-11-17-refs-in-window-design.md). Timings are black-box: from starting the process until a visible window with the expected title
# exists (FindWindow), and until the process exits.
#
# With -FakeSsh (crates/op-reason-win/fake-ssh), the SSH relay (op-reason.exe --ssh-agent) too: its window for a call
# from WSL (a copy of fake-ssh named wsl.exe stands for the interop host, OPREASON_SSH_CALL for op-reason-ssh's facts),
# per-call for Claude Code in Japanese and for a person's command in English (with the button that allows the
# terminal), and show-only in English (docs/superpowers/specs/2026-11-17-ssh-relay-design.md). Then its toasts in
# Japanese: two calls allowed with a click each, then three signatures of the first and one of the second pass without
# a window; the toast window and the whole screen are saved with CAPTUREBLT, which layered windows need
# (docs/superpowers/specs/2026-11-18-pass-toasts-design.md).
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
  [string] $FakeSsh = '',
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
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
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

# The payload of one desktop call (the launcher contract, docs/architecture/README.md). $Herdr and $Notes are what only a
# WSL call sends.
function New-Payload([string[]] $OpArgs, [string] $Lang, [hashtable] $Caller, [string] $Reason, [string] $Command, $Herdr = $null, $Notes = @(), $Refs = $null) {
  $facts = [ordered]@{
    time    = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
    caller  = $Caller
    herdr   = $Herdr
    place   = [ordered]@{ repo = $repo; branch = $branch; cwd = $cwd }
    reason  = $Reason
    command = $Command
    notes   = @($Notes)
  }
  if ($Refs) { $facts.refs = $Refs }
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
  $payload | ConvertTo-Json -Compress -Depth 6 -EscapeHandling EscapeNonAscii
}

# Starts the launcher for one desktop call.
function Start-Launcher([string[]] $OpArgs, [string] $Lang, [hashtable] $Caller, [string] $Reason, [string] $Command, [hashtable] $Env) {
  $all = @{ OPREASON_PAYLOAD = (New-Payload $OpArgs $Lang $Caller $Reason $Command) }
  if ($Env) { foreach ($k in $Env.Keys) { $all[$k] = $Env[$k] } }
  Start-Proc 'powershell.exe' @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-OutputFormat', 'Text', '-File', $Launcher) $all
}

# Starts op-reason.exe --run as a WSL call starts it (without a status file: the payload names none).
function Start-Run([string] $Payload, [hashtable] $Env) {
  $all = @{ OPREASON_PAYLOAD = $Payload }
  if ($Env) { foreach ($k in $Env.Keys) { $all[$k] = $Env[$k] } }
  Start-Proc $Exe @('--run') $all
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

function Save-Rect([int] $Left, [int] $Top, [int] $Width, [int] $Height, [string] $Path, [switch] $Layered) {
  $bmp = [Drawing.Bitmap]::new($Width, $Height)
  $g = [Drawing.Graphics]::FromImage($bmp)
  if ($Layered) {
    $g.CopyFromScreen($Left, $Top, 0, 0, $bmp.Size, [Drawing.CopyPixelOperation]::SourceCopy -bor [Drawing.CopyPixelOperation]::CaptureBlt)
  } else {
    $g.CopyFromScreen($Left, $Top, 0, 0, $bmp.Size)
  }
  $bmp.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose()
  $bmp.Dispose()
}

function Save-Window([IntPtr] $Handle, [string] $Path, [switch] $Layered) {
  $r = [OpReasonShots+RECT]::new()
  [void][OpReasonShots]::GetWindowRect($Handle, [ref]$r)
  $m = 8
  Save-Rect ($r.Left - $m) ($r.Top - $m) ($r.Right - $r.Left + 2 * $m) ($r.Bottom - $r.Top + 2 * $m) $Path -Layered:$Layered
}

function Save-Screen([string] $Path, [switch] $Layered) {
  $b = [Windows.Forms.Screen]::PrimaryScreen.Bounds
  Save-Rect $b.Left $b.Top $b.Width $b.Height $Path -Layered:$Layered
}

# Waits until a visible window of the class exists. Returns the handle (or zero on time-out).
function Wait-ClassWindow([string] $Class, [int] $TimeoutMs = 10000) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
    $h = [OpReasonShots]::FindWindowW($Class, [NullString]::Value)
    if ($h -ne [IntPtr]::Zero -and [OpReasonShots]::IsWindowVisible($h)) { return $h }
    [Threading.Thread]::Sleep(5)
  }
  return [IntPtr]::Zero
}

# --- screenshots

$reasonJa = & $u '\u30c7\u30d7\u30ed\u30a4\u306e\u305f\u3081\u306b Proxmox \u306e API \u306e token \u3092\u8aad\u3080'
$longReason = 'Read the API token of the staging cluster to rotate the deploy key, then write the new key back to the vault item so that the nightly job can pick it up without anyone copying secrets by hand'
$readArgs = @('read', 'op://Private/proxmox/token')
$claudeEnv = @{ OPREASON_LANG = 'ja'; CLAUDECODE = '1'; CLAUDE_PID = '4242'; CLAUDE_CODE_SESSION_ID = 'demo' }
# What a WSL call from Claude Code in a herdr pane sends, after the watchdog cleared a shared approval
$wslPayload = New-Payload $readArgs 'ja' ([ordered]@{ type = 'claude'; pid = 4242; session = 'demo'; detail = $null }) $reasonJa `
  'op read op://Private/proxmox/token' ([ordered]@{ workspace = 'w1'; label = (& $u '\u30c7\u30d7\u30ed\u30a4'); pane = 'w1:p2' }) `
  @([ordered]@{ kind = 'shared-cleared' })
# An op inject template with ten references, three of them ID references (made-up IDs), and the account copy that
# links them
$tplRefs = @(
  'op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password',
  'op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/username',
  'op://Private/proxmox/token',
  'op://$APP_ENV/sentry/dsn',
  'op://Shared/aws/access-key-id',
  'op://Shared/aws/secret-access-key',
  'op://zyxwvutsrqponmlkjihgfedcba/abcdefghij0123456789klmnop/token',
  'op://Shared/slack/webhook',
  'op://Private/ssh/private key',
  'op://Shared/smtp/password'
)
$tplPath = Join-Path $work 'app.env.tpl'
$tplLines = @('DB_HOST=db.internal') + @($tplRefs | ForEach-Object { 'VALUE_' + [array]::IndexOf($tplRefs, $_) + '={{ ' + $_ + ' }}' })
Set-Content -LiteralPath $tplPath -Value ($tplLines -join "`n") -Encoding utf8NoBOM
New-Item -ItemType Directory -Force -Path (Join-Path $work 'op-reason') | Out-Null
Set-Content -LiteralPath (Join-Path $work 'op-reason\account.json') -NoNewline -Encoding utf8NoBOM `
  -Value '{"v":1,"accounts":[{"id":"ABCDEFGHIJKLMNOPQRSTUVWXYZ","host":"example.1password.com"}]}'
$firstDir = Join-Path $work 'first-call'
New-Item -ItemType Directory -Force -Path $firstDir | Out-Null
$reasonTpl = & $u '\u30c6\u30f3\u30d7\u30ec\u30fc\u30c8\u304b\u3089 .env \u3092\u4f5c\u308b'
$injectArgs = @('inject', '-i', $tplPath, '-o', '.env')
$injectCommand = 'op inject -i ' + $tplPath + ' -o .env'
$wslRefs = [ordered]@{ total = 10; from = @([ordered]@{ kind = 'stdin' }); list = @($tplRefs | Select-Object -First 8) }
$wslRefsPayload = New-Payload @('inject', '-o', '.env') 'ja' ([ordered]@{ type = 'claude'; pid = 4242; session = 'demo'; detail = $null }) $reasonTpl `
  'op inject -o .env' $null @() $wslRefs
$scenes = @(
  @{ name = '01-exe-claude-ja'; title = $TitleJa; env = $claudeEnv; args = @('--reason', $reasonJa) + $readArgs },
  @{ name = '02-entry-claude-ja'; title = $TitleJa; entry = $true; env = $claudeEnv; args = @('--reason', $reasonJa) + $readArgs },
  @{ name = '03-exe-person-en'; title = $TitleEn; env = @{ OPREASON_LANG = 'en'; WT_SESSION = '00000000-0000-0000-0000-000000000000' };
     args = @('--reason', 'Read the Proxmox API token to create a VM') + $readArgs },
  @{ name = '04-exe-others-ja'; title = $TitleJa; env = @{ OPREASON_LANG = 'ja' }; others = $true;
     args = @('--reason', $reasonJa, 'item', 'get', 'proxmox', '--fields', 'password') },
  @{ name = '05-exe-long-en'; title = $TitleEn; env = @{ OPREASON_LANG = 'en' };
     args = @('--reason', $longReason, 'item', 'create', '--category', 'login', '--title', 'deploy key', 'username=ci', 'password=not-shown') },
  @{ name = '06-run-wsl-ja'; title = $TitleJa; run = $wslPayload; env = @{} },
  @{ name = '09-exe-refs-links-ja'; title = $TitleJa; env = $claudeEnv; args = @('--reason', $reasonTpl) + $injectArgs },
  @{ name = '10-exe-refs-first-call-en'; title = $TitleEn; env = @{ OPREASON_LANG = 'en'; LOCALAPPDATA = $firstDir };
     args = @('--reason', 'Write .env from the template') + $injectArgs },
  @{ name = '11-run-refs-wsl-ja'; title = $TitleJa; run = $wslRefsPayload; env = @{} },
  @{ name = '12-entry-refs-links-ja'; title = $TitleJa; entry = $true; env = $claudeEnv; args = @('--reason', $reasonTpl) + $injectArgs }
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
  } elseif ($s.run) {
    $p = Start-Run $s.run (@{ FAKE_SLEEP_MS = '4000' } + $s.env)
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

# Clicks the Japanese name at the right end of the window's language switch with the real mouse: 26 DIPs left of the client area's
# right edge and 21 DIPs above its bottom (as crates/op-reason-win/tests/exe.rs does).
function Invoke-SwitchClick([IntPtr] $Handle) {
  $rc = [OpReasonShots+RECT]::new()
  [void][OpReasonShots]::GetClientRect($Handle, [ref]$rc)
  $k = [math]::Max(96, [OpReasonShots]::GetDpiForWindow($Handle)) / 96.0
  $pt = [OpReasonShots+POINT]::new()
  $pt.X = $rc.Right - [int][math]::Round(26 * $k)
  $pt.Y = $rc.Bottom - [int][math]::Round(21 * $k)
  [void][OpReasonShots]::ClientToScreen($Handle, [ref]$pt)
  [void][OpReasonShots]::SetCursorPos($pt.X, $pt.Y)
  [Threading.Thread]::Sleep(100)
  [OpReasonShots]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero)
  [Threading.Thread]::Sleep(60)
  [OpReasonShots]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
}

# A call from WSL in English; a click on the Japanese name switches the window (its own LOCALAPPDATA: the choice is written there,
# and the scenes and timings that follow stay English).
$clickDir = Join-Path $work 'click'
New-Item -ItemType Directory -Force -Path $clickDir | Out-Null
$clickPayload = New-Payload $readArgs 'en' ([ordered]@{ type = 'human'; pid = $null; session = $null; detail = $null }) `
  'Read the Proxmox API token to create a VM' 'op read op://Private/proxmox/token'
$p = Start-Run $clickPayload @{ FAKE_SLEEP_MS = '6000'; LOCALAPPDATA = $clickDir }
$h = Wait-Window $TitleEn
if ($h -eq [IntPtr]::Zero) { throw 'no window for the click scene' }
[Threading.Thread]::Sleep(800)
Save-Window $h (Join-Path $Out '07-run-before-click-en.png')
$before = [OpReasonShots]::GetForegroundWindow()
Invoke-SwitchClick $h
$hJa = Wait-Window $TitleJa 3000
$after = [OpReasonShots]::GetForegroundWindow()
[Threading.Thread]::Sleep(300)
if ($hJa -ne [IntPtr]::Zero) {
  Save-Window $hJa (Join-Path $Out '08-run-after-click-ja.png')
} else {
  Write-Warning 'the click on the language switch did not switch the window'
  Save-Window $h (Join-Path $Out '08-run-after-click-not-switched.png')
}
$p.WaitForExit()
$clickNote = "Click on the language switch: switched=$($hJa -ne [IntPtr]::Zero), foreground unchanged=$($before -eq $after), " +
  "settings.json: $(Get-Content -Raw -ErrorAction SilentlyContinue (Join-Path $clickDir 'op-reason\settings.json'))"
Write-Host $clickNote

# --- the SSH relay

if ($FakeSsh) {
  $FakeSsh = (Resolve-Path $FakeSsh).Path
  $relayLad = Join-Path $work 'relay'
  $agentDir = Join-Path $work 'agent'
  $wslDir = Join-Path $work 'wslhost'
  New-Item -ItemType Directory -Force -Path $relayLad, $agentDir, $wslDir | Out-Null
  Copy-Item -LiteralPath $FakeSsh -Destination (Join-Path $wslDir 'wsl.exe')
  $pipe = '\\.\pipe\op-reason-shots-' + $PID
  $up = '\\.\pipe\op-reason-shots-up-' + $PID
  $agent = Start-Proc $FakeSsh @('agent', $up, $agentDir) @{}
  $sw = [Diagnostics.Stopwatch]::StartNew()
  while (-not (Test-Path (Join-Path $agentDir 'agent.ready')) -and $sw.ElapsedMilliseconds -lt 10000) { Start-Sleep -Milliseconds 50 }

  # One call from WSL: the relay in a mode and a language, the client through the fake wsl.exe, the window saved.
  function Show-Relay([string] $Name, [string] $Mode, [string] $Lang, [string] $Title, $Call) {
    $relay = Start-Proc $Exe @('--ssh-agent', '--mode', $Mode, '--pipe', $pipe, '--upstream', $up, '--answer-wait', '30s') `
      @{ LOCALAPPDATA = $relayLad; OPREASON_LANG = $Lang }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
      Start-Sleep -Milliseconds 100
      & $Exe --ssh-agent --status --pipe $pipe *> $null
    } while ($LASTEXITCODE -ne 0 -and $sw.ElapsedMilliseconds -lt 10000)
    $json = $Call | ConvertTo-Json -Compress -Depth 6 -EscapeHandling EscapeNonAscii
    $client = Start-Proc (Join-Path $wslDir 'wsl.exe') @('spawn', $FakeSsh, 'client', $pipe, 'list', 'bind', 'sign') `
      @{ OPREASON_SSH_CALL = $json }
    $h = Wait-Window $Title
    if ($h -eq [IntPtr]::Zero) { Write-Warning "no window for scene $Name" } else {
      [Threading.Thread]::Sleep(800)
      Save-Window $h (Join-Path $Out "$Name.png")
    }
    foreach ($q in @($client, $relay)) { if (-not $q.HasExited) { $q.Kill() }; $q.WaitForExit() }
  }

  $herdrFacts = [ordered]@{ workspace = 'w1'; label = (& $u '\u30c7\u30d7\u30ed\u30a4'); pane = 'w1:p2' }
  $claudeCall = [ordered]@{
    v = 1; distro = 'Ubuntu'; rule = 'claude'
    root = [ordered]@{ pid = 31337; start = 1; name = 'bash'; cmd = 'bash -c -l "git push origin fix/login"' }
    caller = [ordered]@{ type = 'claude'; claude_pid = 4242; herdr = $herdrFacts; cwd = '~/src/op-reason'; repo = 'pollenjp/op-reason'; branch = 'fix/login' }
    session = 'demo'; reason = (& $u '\u4fee\u6b63\u3092 GitHub \u306b push \u3059\u308b')
  }
  Show-Relay '13-relay-claude-ja' 'per-call' 'ja' `
    (& $u 'SSH \u306e\u9375: \u3053\u306e\u547c\u3073\u51fa\u3057\u3092\u8a31\u53ef\u3057\u307e\u3059\u304b') $claudeCall
  $personCall = [ordered]@{
    v = 1; distro = 'Ubuntu'; rule = 'shell'
    root = [ordered]@{ pid = 2718; start = 2; name = 'git'; cmd = 'git pull --rebase' }
    shell = [ordered]@{ pid = 2000; start = 1; name = 'fish'; cmd = '' }
    caller = [ordered]@{ type = 'human'; claude_pid = $null; herdr = $null; cwd = '~/src/infra'; repo = 'me/infra'; branch = 'main' }
    session = $null; reason = ''
  }
  Show-Relay '14-relay-person-en' 'per-call' 'en' 'SSH key: allow this call?' $personCall

  # Clicks the first button of the relay's window (Allow this call) with the real mouse: 20 DIPs inside the client
  # area's left edge and 26 DIPs above its bottom (as crates/op-reason-win/tests/relay.rs does).
  function Invoke-AllowClick([IntPtr] $Handle) {
    $rc = [OpReasonShots+RECT]::new()
    [void][OpReasonShots]::GetClientRect($Handle, [ref]$rc)
    $k = [math]::Max(96, [OpReasonShots]::GetDpiForWindow($Handle)) / 96.0
    $pt = [OpReasonShots+POINT]::new()
    $pt.X = [int][math]::Round(20 * $k)
    $pt.Y = $rc.Bottom - [int][math]::Round(26 * $k)
    [void][OpReasonShots]::ClientToScreen($Handle, [ref]$pt)
    [void][OpReasonShots]::SetCursorPos($pt.X, $pt.Y)
    [Threading.Thread]::Sleep(100)
    [OpReasonShots]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero)
    [Threading.Thread]::Sleep(60)
    [OpReasonShots]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
    # out of the way of the screenshots
    [void][OpReasonShots]::SetCursorPos(400, 600)
  }

  # The toasts: the calls of $Calls are allowed one by one, then each signs $Signs[i] times without a window.
  function Show-Toasts([string] $Name, [string] $Lang, [string] $Title, $Calls, [int[]] $Signs) {
    $relay = Start-Proc $Exe @('--ssh-agent', '--mode', 'per-call', '--pipe', $pipe, '--upstream', $up, '--answer-wait', '30s') `
      @{ LOCALAPPDATA = $relayLad; OPREASON_LANG = $Lang }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
      Start-Sleep -Milliseconds 100
      & $Exe --ssh-agent --status --pipe $pipe *> $null
    } while ($LASTEXITCODE -ne 0 -and $sw.ElapsedMilliseconds -lt 10000)
    $jsons = @($Calls | ForEach-Object { $_ | ConvertTo-Json -Compress -Depth 6 -EscapeHandling EscapeNonAscii })
    foreach ($json in $jsons) {
      $client = Start-Proc (Join-Path $wslDir 'wsl.exe') @('spawn', $FakeSsh, 'client', $pipe, 'list', 'bind', 'sign') `
        @{ OPREASON_SSH_CALL = $json }
      $h = Wait-Window $Title
      if ($h -eq [IntPtr]::Zero) { Write-Warning "no window to allow for scene $Name" } else {
        [Threading.Thread]::Sleep(700)
        Invoke-AllowClick $h
      }
      if (-not $client.WaitForExit(15000)) { $client.Kill() }
    }
    for ($i = 0; $i -lt $jsons.Count; $i++) {
      $steps = @('bind') + @(1..$Signs[$i] | ForEach-Object { 'sign' })
      $client = Start-Proc (Join-Path $wslDir 'wsl.exe') (@('spawn', $FakeSsh, 'client', $pipe) + $steps) @{ OPREASON_SSH_CALL = $jsons[$i] }
      if (-not $client.WaitForExit(15000)) { $client.Kill() }
    }
    $t = Wait-ClassWindow 'op-reason-pass-toast' 5000
    if ($t -eq [IntPtr]::Zero) { Write-Warning "no toast for scene $Name" } else {
      # the last card's slide (240 ms) and glow (420 ms) are over
      [Threading.Thread]::Sleep(700)
      Save-Window $t (Join-Path $Out "$Name.png") -Layered
      Save-Screen (Join-Path $Out "$Name-screen.png") -Layered
    }
    if (-not $relay.HasExited) { $relay.Kill() }
    $relay.WaitForExit()
  }

  Show-Toasts '16-relay-toasts-ja' 'ja' `
    (& $u 'SSH \u306e\u9375: \u3053\u306e\u547c\u3073\u51fa\u3057\u3092\u8a31\u53ef\u3057\u307e\u3059\u304b') `
    @($personCall, $claudeCall) @(3, 1)
  # show-only: the fake agent answers after six seconds, as while 1Password asks
  Set-Content -LiteralPath (Join-Path $agentDir 'sign-delay-ms') -Value '6000' -NoNewline
  $ansibleCall = [ordered]@{
    v = 1; distro = 'Ubuntu'; rule = 'shell'
    root = [ordered]@{ pid = 1414; start = 3; name = 'ansible-playboo'; cmd = 'python3 /usr/bin/ansible-playbook site.yml --limit db' }
    shell = [ordered]@{ pid = 1000; start = 1; name = 'fish'; cmd = '' }
    caller = [ordered]@{ type = 'human'; claude_pid = $null; herdr = $null; cwd = '~/src/infra'; repo = 'me/infra'; branch = 'main' }
    session = $null; reason = 'Rotate the database password on the db hosts'
  }
  Show-Relay '15-relay-show-only-en' 'show-only' 'en' 'SSH key: 1Password is asking' $ansibleCall
  $agent.Kill(); $agent.WaitForExit()
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
$runTiming = New-Payload @('vault', 'list') 'en' $humanCaller 'timing' 'op vault list'
$results += Measure-Runs 'op-reason.exe --run (as a WSL call starts it, without WSL): start -> window visible' {
  Measure-Window { Start-Run $runTiming @{ FAKE_SLEEP_MS = '1500' } }
}
$results += Measure-Runs 'op-reason.exe --run: whole desktop call (window, toast, op, whoami, signout)' {
  Measure-Whole { Start-Run $runTiming @{ FAKE_WHOAMI_RC = '0' } }
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
$md += $clickNote
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
