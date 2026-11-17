# Runs launcher/launcher.ps1 on Windows PowerShell 5.1 with a fake op.exe (tests/windows/fake-op.cs), and takes
# screenshots of the window (Japanese and English) into screenshots/ for the pull request.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tests/windows/launcher.test.ps1
#
# What it checks:
#   - tricky arguments (empty, spaces, quotes, trailing \, & | < > ^ % !, Japanese, newline, tab, emoji) reach op.exe one
#     by one; binary stdin comes out on stdout (cmd.exe redirection); stderr has only op's; the exit code is op's; the
#     payload variable is not inherited; the window is shown
#   - after op, whoami and (when signed in) signout --all; their output stays out of the launcher's stdout and stderr
#   - the named mutex Local\op-reason: while another process holds it, a call waits lockWait seconds, says so once on
#     stderr, starts nothing and exits 75 with status lock "timeout" and the holder from holder.json; a lock-skipped call
#     runs anyway and does not sign out
#   - stderr "scan": op's stderr reaches the launcher's stderr and a denial is reported in status.stderrMatch
#   - service mode passes OP_SERVICE_ACCOUNT_TOKEN and runs neither whoami nor signout
#   - the language chosen in op-reason.exe's window (%LOCALAPPDATA%\op-reason\settings.json) beats the payload's lang
#     and is reported as status chosenLang; without the file, the payload's language and chosenLang null
#   - the references a call reads (docs/superpowers/specs/2026-11-17-refs-in-window-design.md): whoami's JSON keeps only
#     the account ID and host in account.json; status.ui.refs links an ID reference once the account is known, and
#     says why there is no link before; on CI only (it moves the mouse and registers a handler of onepassword: URLs in
#     HKCU, unless one exists), a real click on a link opens exactly the item's URL without making the window the
#     foreground window
#   - screenshots: the window with the block of references (links and "and N more"; the hint)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$launcher = Join-Path $root 'launcher\launcher.ps1'
$ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('op-reason-launcher-' + [guid]::NewGuid().ToString('n'))
$fakeDir = Join-Path $work 'fake'
$binDir = Join-Path $work 'bin'
[void](New-Item -ItemType Directory -Force -Path $fakeDir, $binDir)
$shots = Join-Path $root 'screenshots'
[void](New-Item -ItemType Directory -Force -Path $shots)
$env:FAKE_DIR = $fakeDir
$env:LOCALAPPDATA = Join-Path $work 'localappdata'
$opExe = Join-Path $binDir 'op.exe'
Add-Type -TypeDefinition ([System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'fake-op.cs'))) -OutputAssembly $opExe -OutputType ConsoleApplication
$statusFile = Join-Path $work 'status.json'
$holderFile = Join-Path $env:LOCALAPPDATA 'op-reason\holder.json'

function Reset-Fake {
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $fakeDir 'calls.jsonl'), $statusFile
  foreach ($n in @('FAKE_RC', 'FAKE_WHOAMI_RC', 'FAKE_WHOAMI_OUT', 'FAKE_ERR_TEXT', 'FAKE_SLEEP_MS', 'FAKE_OUT', 'FAKE_NO_STDIN', 'OP_SERVICE_ACCOUNT_TOKEN')) {
    [Environment]::SetEnvironmentVariable($n, $null, 'Process')
  }
}

# Callers wrap the result in @(): Windows PowerShell 5.1 unrolls a one-element array returned from a function, and a
# single PSCustomObject has no Count there.
function Get-Calls {
  $f = Join-Path $fakeDir 'calls.jsonl'
  if (-not (Test-Path -LiteralPath $f)) { return @() }
  return @([System.IO.File]::ReadAllLines($f, [System.Text.Encoding]::UTF8) | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json })
}

function Get-Status {
  if (-not (Test-Path -LiteralPath $statusFile)) { return $null }
  return ([System.IO.File]::ReadAllText($statusFile) | ConvertFrom-Json)
}

function New-Payload([string]$mode, [string[]]$opArgs, [string]$lang = 'ja', [string]$lock = 'held', [int]$lockWait = 90, [string]$stderr = 'inherit', $facts = $null) {
  $p = [ordered]@{
    v = 1; mode = $mode; op = $opExe; status = $statusFile; lang = $lang; lock = $lock; lockWait = $lockWait; stderr = $stderr
    args64 = @($opArgs | ForEach-Object { [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([string]$_)) })
  }
  if ($null -ne $facts) { $p.facts = $facts }
  $json = ConvertTo-Json -InputObject $p -Compress -Depth 6
  $sb = New-Object System.Text.StringBuilder
  foreach ($c in $json.ToCharArray()) {
    if ([int]$c -lt 128) { [void]$sb.Append($c) } else { [void]$sb.Append([char]92).Append('u').Append(([int]$c).ToString('x4')) }
  }
  return $sb.ToString()
}

function New-Facts([string]$type, [string]$reason, [string]$command, $herdr = $null, $notes = @()) {
  $pid_ = $null
  if ($type -eq 'claude') { $pid_ = 4242 }
  return [ordered]@{
    time   = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
    caller = [ordered]@{ type = $type; pid = $pid_; session = $null; detail = 'pwsh.exe ← cmd.exe ← claude.exe' }
    herdr  = $herdr
    place  = [ordered]@{ repo = 'pollenjp/op-reason'; branch = 'feat/windows-entry'; cwd = '~\src\op-reason' }
    reason = $reason
    command = $command
    notes  = @($notes)
  }
}

# Run cmd.exe with an exact command line (redirections included).
function Invoke-Cmd([string]$cmdline) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = Join-Path $env:SystemRoot 'System32\cmd.exe'
  $psi.Arguments = '/d /s /c "' + $cmdline + '"'
  $psi.UseShellExecute = $false
  $p = [System.Diagnostics.Process]::Start($psi)
  $p.WaitForExit()
  return $p.ExitCode
}

# Run the launcher with -File and the payload; stdin from a file (default NUL), stdout and stderr to files.
function Invoke-Launcher([string]$payload, [string]$name, [string]$stdin = 'NUL') {
  $env:OPREASON_PAYLOAD = $payload
  $out = Join-Path $work "$name.out"
  $err = Join-Path $work "$name.err"
  $cmdline = '"' + $ps + '" -NoProfile -NonInteractive -ExecutionPolicy Bypass -OutputFormat Text -File "' + $launcher + '" < "' + $stdin + '" > "' + $out + '" 2> "' + $err + '"'
  $rc = Invoke-Cmd $cmdline
  Remove-Item Env:OPREASON_PAYLOAD -ErrorAction SilentlyContinue
  return @{ rc = $rc; out = $out; err = $err }
}

function Test-SameBytes([string]$a, [string]$b) {
  $x = [System.IO.File]::ReadAllBytes($a)
  $y = [System.IO.File]::ReadAllBytes($b)
  if ($x.Length -ne $y.Length) { return $false }
  for ($k = 0; $k -lt $x.Length; $k++) { if ($x[$k] -ne $y[$k]) { return $false } }
  return $true
}

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class WindowRect
{
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindow(string className, string title);
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr window, out RECT rect);
    // DWMWA_EXTENDED_FRAME_BOUNDS (9): the visible frame, without the invisible resize borders around it
    [DllImport("dwmapi.dll")]
    public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out RECT rect, int size);
}
'@
# Save the window with this title as a PNG (only the window: nothing of the runner's screen around it). Falls back to
# the top right of the work area, where the window opens, when the window cannot be found.
function Save-WindowShot([string]$path, [string]$title) {
  $screen = [System.Windows.Forms.Screen]::PrimaryScreen
  $b = $screen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $g.Dispose()
  $rect = $null
  # [NullString]::Value: PowerShell would pass $null to a string parameter as an empty string
  $h = [WindowRect]::FindWindow([NullString]::Value, $title)
  $r = New-Object WindowRect+RECT
  $found = $false
  if ($h -ne [IntPtr]::Zero) {
    $found = ([WindowRect]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) -eq 0)
    if (-not $found) { $found = [WindowRect]::GetWindowRect($h, [ref]$r) }
  }
  if ($found) {
    $x = [Math]::Max(0, $r.Left)
    $y = [Math]::Max(0, $r.Top)
    $w = [Math]::Min($b.Width, $r.Right) - $x
    $hh = [Math]::Min($b.Height, $r.Bottom) - $y
    if ($w -gt 0 -and $hh -gt 0) { $rect = New-Object System.Drawing.Rectangle $x, $y, $w, $hh }
  }
  if ($null -eq $rect) {
    $wa = $screen.WorkingArea
    $w = [Math]::Min(620, $wa.Width)
    $hh = [Math]::Min(460, $wa.Height)
    $rect = New-Object System.Drawing.Rectangle ($wa.Right - $w), $wa.Top, $w, $hh
    Write-Host "       (window not found by its title; saved the top right of the screen)"
  }
  $crop = $bmp.Clone($rect, $bmp.PixelFormat)
  $crop.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $crop.Dispose()
  $bmp.Dispose()
}

$bin = Join-Path $work 'in.bin'
$bytes = New-Object byte[] (256 * 40 + 6)
for ($k = 0; $k -lt 256 * 40; $k++) { $bytes[$k] = [byte]($k % 256) }
$tail = [byte[]](13, 10, 0, 101, 110, 100)
[Array]::Copy($tail, 0, $bytes, 256 * 40, 6)
[System.IO.File]::WriteAllBytes($bin, $bytes)

Write-Host '== What op receives (desktop)'
Reset-Fake
$tricky = @('', 'a b', 'x"y', 'tr\\', 'q\"z', '&|<>^%PATH%!', '日本語 の 引数', '  lead', 'trail  ', "new`nline", "tab`there", '"', '\', [char]::ConvertFromUtf32(0x1F600))
$env:FAKE_RC = '42'
$r = Invoke-Launcher (New-Payload 'desktop' $tricky 'ja' 'held' 90 'inherit' (New-Facts 'human' 'read the settings' 'op item get x')) 'desktop' $bin
$calls = @(Get-Calls)
$st = Get-Status
$gotArgs = @()
if ($calls.Count -gt 0) { $gotArgs = @($calls[0].args) }
Check 'tricky arguments arrive one by one, in order' (($gotArgs -join [char]0x1F) -ceq ($tricky -join [char]0x1F)) ("got: " + ($gotArgs -join '|'))
Check 'binary stdin comes out on stdout unchanged' (Test-SameBytes $bin $r.out) ("out bytes: " + (Get-Item $r.out).Length)
$errText = [System.IO.File]::ReadAllText($r.err).Trim()
Check 'stderr has only what op wrote' ($errText -ceq 'FAKE-ERR') $errText
Check "the exit code is op's" ($r.rc -eq 42) ("rc=" + $r.rc)
Check 'OPREASON_PAYLOAD is not inherited by op' ($calls.Count -gt 0 -and $calls[0].payload -eq $false) ''
Check 'the window is shown on Windows' ($null -ne $st -and $st.window -eq 'shown' -and $st.started -eq $true -and $st.exit -eq 42) ("status: " + ($st | ConvertTo-Json -Compress))
Write-Host ('       toast: ' + $st.toast)

Write-Host '== Closing the sessions'
Reset-Fake
$env:FAKE_WHOAMI_RC = '0'
$env:FAKE_NO_STDIN = '1'
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'get', 'x') 'en' 'held' 90 'inherit' (New-Facts 'human' 'r' 'op item get x')) 'closes'
$calls = @(Get-Calls)
$st = Get-Status
$seq = @($calls | ForEach-Object { @($_.args) -join ' ' }) -join ','
Check 'desktop runs whoami after op and signout --all when signed in' ($seq -ceq 'item get x,whoami --format json,signout --all' -and $st.signout -eq 'ok') "calls=$seq signout=$($st.signout)"
$both = [System.IO.File]::ReadAllText($r.out) + [System.IO.File]::ReadAllText($r.err)
Check "whoami and signout write nothing to the launcher's stdout or stderr" (-not $both.Contains('SIGNOUT')) $both

Write-Host '== The named mutex'
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$holdScript = Join-Path $work 'hold.ps1'
$ready = Join-Path $work 'mutex.ready'
[System.IO.File]::WriteAllText($holdScript, @'
$m = New-Object System.Threading.Mutex($false, 'Local\op-reason')
[void]$m.WaitOne()
New-Item -ItemType File -Path $env:READY | Out-Null
Start-Sleep -Seconds 30
'@)
$env:READY = $ready
$holder = Start-Process -FilePath $ps -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $holdScript) -PassThru -WindowStyle Hidden
for ($k = 0; $k -lt 200 -and -not (Test-Path -LiteralPath $ready); $k++) { Start-Sleep -Milliseconds 100 }
[void](New-Item -ItemType Directory -Force -Path (Split-Path -Parent $holderFile))
[System.IO.File]::WriteAllText($holderFile, '{"time":"12:00:00","caller":"Human","reason":"the earlier call","command":"op vault list","pid":1}')
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'get', 'x') 'en' 'held' 2 'inherit' (New-Facts 'human' 'r' 'op item get x')) 'locktimeout'
$st = Get-Status
$errText = [System.IO.File]::ReadAllText($r.err)
Check 'a held mutex: wait, then exit 75, lock timeout, the holder, nothing started, one waiting line' (
  $r.rc -eq 75 -and $st.lock -eq 'timeout' -and $st.error -eq 'lock-timeout' -and $st.started -eq $false -and
  ([string]$st.holder).Contains('the earlier call') -and @(Get-Calls).Count -eq 0 -and $errText.Contains('waiting for another op-reason call')) ("rc=$($r.rc) err=$errText status=" + ($st | ConvertTo-Json -Compress))
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:FAKE_WHOAMI_RC = '0'
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'list') 'en' 'skipped' 2 'inherit' (New-Facts 'human' 'r' 'op item list')) 'lockskipped'
$st = Get-Status
Check 'lock skipped: runs while the mutex is held and does not sign out' ($r.rc -eq 0 -and $st.lock -eq 'skipped' -and @(Get-Calls).Count -eq 1) ("rc=$($r.rc) status=" + ($st | ConvertTo-Json -Compress))
Stop-Process -Id $holder.Id -Force -ErrorAction SilentlyContinue
Remove-Item -Force -ErrorAction SilentlyContinue $holderFile
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'get', 'x') 'en' 'held' 5 'inherit' (New-Facts 'human' 'r' 'op item get x')) 'lockfree'
$st = Get-Status
Check 'an abandoned or free mutex is taken (status lock held) and holder.json is removed' ($r.rc -eq 0 -and $st.lock -eq 'held' -and -not (Test-Path -LiteralPath $holderFile)) ("status=" + ($st | ConvertTo-Json -Compress))

Write-Host '== stderr scan'
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:FAKE_RC = '1'
$env:FAKE_ERR_TEXT = '[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again'
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'get', 'x') 'en' 'held' 90 'scan' (New-Facts 'human' 'r' 'op item get x')) 'scan'
$st = Get-Status
$errText = [System.IO.File]::ReadAllText($r.err)
Check "scan: op's stderr is relayed and a denial is reported" ($r.rc -eq 1 -and $st.stderrMatch -eq 'denied' -and $errText.Contains('authorization prompt dismissed')) ("rc=$($r.rc) err=$errText status=" + ($st | ConvertTo-Json -Compress))

Reset-Fake
$r = Invoke-Launcher (New-Payload 'desktop' @('document', 'get', 'x') 'en' 'held' 90 'scan' (New-Facts 'human' 'r' 'op document get x')) 'scanbin' $bin
Check 'scan: binary stdin still comes out on stdout unchanged' (Test-SameBytes $bin $r.out) ("out bytes: " + (Get-Item $r.out).Length)

Write-Host '== service'
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:FAKE_WHOAMI_RC = '0'
$env:OP_SERVICE_ACCOUNT_TOKEN = 'tok-test'
$r = Invoke-Launcher (New-Payload 'service' @('vault', 'list') 'en') 'service'
$calls = @(Get-Calls)
$st = Get-Status
Check 'service passes the token and runs neither whoami nor signout' ($calls.Count -eq 1 -and $calls[0].sa -eq 'tok-test' -and $st.signout -eq 'skipped') ("calls=$($calls.Count) status=" + ($st | ConvertTo-Json -Compress))

Write-Host "== The language chosen in op-reason.exe's window"
$settingsFile = Join-Path $env:LOCALAPPDATA 'op-reason\settings.json'
[void](New-Item -ItemType Directory -Force -Path (Split-Path -Parent $settingsFile))
[System.IO.File]::WriteAllText($settingsFile, '{"v":1,"lang":"en"}')
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'get', 'x') 'ja' 'held' 90 'inherit' (New-Facts 'human' 'r' 'op item get x')) 'chosen'
$st = Get-Status
Check 'settings.json en beats the payload ja and is reported as chosenLang (the file is left as it was)' (
  $r.rc -eq 0 -and $st.ui.title -ceq '1Password CLI approval request' -and $st.chosenLang -ceq 'en' -and
  [System.IO.File]::ReadAllText($settingsFile) -ceq '{"v":1,"lang":"en"}') ("status=" + ($st | ConvertTo-Json -Compress -Depth 5))
Remove-Item -Force $settingsFile
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$r = Invoke-Launcher (New-Payload 'desktop' @('item', 'get', 'x') 'ja' 'held' 90 'inherit' (New-Facts 'human' 'r' 'op item get x')) 'unchosen'
$st = Get-Status
Check 'without settings.json: the payload language and chosenLang null' (
  $r.rc -eq 0 -and $st.ui.title -ceq '1Password CLI の承認の依頼' -and $null -eq $st.chosenLang -and
  @($st.PSObject.Properties.Name) -contains 'chosenLang') ("status=" + ($st | ConvertTo-Json -Compress -Depth 5))

Write-Host '== The references a call reads'
$accountFile = Join-Path $env:LOCALAPPDATA 'op-reason\account.json'
$accountJson = '{"v":1,"accounts":[{"id":"ABCDEFGHIJKLMNOPQRSTUVWXYZ","host":"example.1password.com"}]}'
$idRef = 'op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password'
$itemUrl = 'onepassword://view-item?a=ABCDEFGHIJKLMNOPQRSTUVWXYZ&v=abcdefghijklmnopqrstuvwxyz&i=0123456789abcdefghijklmnop'
# The same URL as the Windows shell hands it to the handler: it adds the / of an empty path before the query
$itemUrlShell = 'onepassword://view-item/?a=ABCDEFGHIJKLMNOPQRSTUVWXYZ&v=abcdefghijklmnopqrstuvwxyz&i=0123456789abcdefghijklmnop'
function New-RefsFacts([string]$reason, [string]$command) {
  $f = New-Facts 'human' $reason $command
  $f.refs = [ordered]@{
    total = 10
    from  = @([ordered]@{ kind = 'args' }, [ordered]@{ kind = 'file'; name = 'app.tpl' })
    list  = @($idRef, 'op://Private/proxmox/token', 'op://$APP_ENV/db/password', 'op://Shared/github/token', 'op://Shared/aws/access-key',
      'op://Shared/aws/secret-key', 'op://Private/ssh/private key', 'op://Shared/slack/webhook')
  }
  return $f
}
Remove-Item -Force -ErrorAction SilentlyContinue $accountFile
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:FAKE_WHOAMI_RC = '0'
$env:FAKE_WHOAMI_OUT = '{"url":"https://example.1password.com/","email":"someone@example.com","user_uuid":"USERUSERUSERUSERUSERUSER12","account_uuid":"ABCDEFGHIJKLMNOPQRSTUVWXYZ","user_type":"HUMAN"}'
$r = Invoke-Launcher (New-Payload 'desktop' @('inject') 'ja' 'held' 90 'inherit' (New-RefsFacts '参照の欄' 'op inject')) 'refs-first'
$st = Get-Status
$copy = ''
if (Test-Path -LiteralPath $accountFile) { $copy = [System.IO.File]::ReadAllText($accountFile) }
$linkedFirst = @($st.ui.refs.items | ForEach-Object { [string]$_.linked }) -join ','
Check 'the first call: no links and the hint; whoami keeps only the account ID and host in account.json' (
  $r.rc -eq 0 -and $st.ui.refs.head -ceq '読む参照 (10 件・引数、app.tpl から)' -and $linkedFirst -ceq 'False,False,False,False,False,False,False,False' -and
  $st.ui.refs.more -ceq 'ほか 2 件' -and ([string]$st.ui.refs.hint).StartsWith('項目へのリンクは') -and $copy -ceq $accountJson) ("copy=$copy status=" + ($st | ConvertTo-Json -Compress -Depth 6))
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$r = Invoke-Launcher (New-Payload 'desktop' @('inject') 'en' 'held' 90 'inherit' (New-RefsFacts 'refs' 'op inject')) 'refs-second'
$st = Get-Status
$statusText = [System.IO.File]::ReadAllText($statusFile)
Check 'the next call links the ID reference; the status says linked, not the URL' (
  $r.rc -eq 0 -and $st.ui.refs.head -ceq 'References to read (10, from arguments, app.tpl)' -and $st.ui.refs.items[0].linked -eq $true -and
  $st.ui.refs.items[1].linked -eq $false -and $null -eq $st.ui.refs.hint -and -not $statusText.Contains('onepassword:')) $statusText

# A real click on the first link (CI only): a handler of onepassword: URLs in HKCU (the fake op.exe, which appends each
# URL to a file) for the time of the test, unless one exists.
if ($env:CI) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class LinkClick
{
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
}
'@
  $key = 'HKCU:\Software\Classes\onepassword'
  if (Test-Path -LiteralPath $key) {
    Write-Host '  skip a click on a link (a handler of onepassword: URLs exists already)'
  } else {
    $opened = Join-Path $work 'opened.txt'
    try {
      [void](New-Item -Path $key -Force)
      Set-ItemProperty -LiteralPath $key -Name '(default)' -Value 'URL:onepassword'
      Set-ItemProperty -LiteralPath $key -Name 'URL Protocol' -Value ''
      [void](New-Item -Path ($key + '\shell\open\command') -Force)
      Set-ItemProperty -LiteralPath ($key + '\shell\open\command') -Name '(default)' -Value ('"' + $opExe + '" --opened "' + $opened + '" "%1"')
      [System.IO.File]::WriteAllText($accountFile, $accountJson)
      Reset-Fake
      $env:FAKE_NO_STDIN = '1'
      $env:FAKE_SLEEP_MS = '40000'
      $env:OPREASON_PAYLOAD = New-Payload 'desktop' @('inject') 'en' 'held' 90 'inherit' (New-RefsFacts 'click a link' 'op inject')
      $proc = Start-Process -FilePath $ps -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $launcher) -PassThru -WindowStyle Hidden
      Remove-Item Env:OPREASON_PAYLOAD -ErrorAction SilentlyContinue
      $h = [IntPtr]::Zero
      for ($k = 0; $k -lt 200 -and $h -eq [IntPtr]::Zero; $k++) {
        Start-Sleep -Milliseconds 100
        $h = [LinkClick]::FindWindow([NullString]::Value, '1Password CLI approval request')
      }
      Start-Sleep -Milliseconds 800
      $before = [LinkClick]::GetForegroundWindow()
      # Click down the left of the text, 40 DIPs in (past the padding and the references' indent), every 6 DIPs (less
      # than a line), until the handler has written; half a second after each click. The pointer's shape cannot tell
      # where the link is: the runner's desktop has no visible pointer.
      $found = $null
      if ($h -ne [IntPtr]::Zero) {
        $rc = New-Object LinkClick+RECT
        [void][LinkClick]::GetClientRect($h, [ref]$rc)
        $scale = [Math]::Max(96, [LinkClick]::GetDpiForWindow($h)) / 96.0
        $x = [int][Math]::Round(40 * $scale)
        $step = [int][Math]::Max(1, [Math]::Round(6 * $scale))
        for ($y = $step; $y -lt $rc.Bottom -and $null -eq $found; $y += $step) {
          $pt = New-Object LinkClick+POINT
          $pt.X = $x
          $pt.Y = $y
          [void][LinkClick]::ClientToScreen($h, [ref]$pt)
          [void][LinkClick]::SetCursorPos($pt.X, $pt.Y)
          Start-Sleep -Milliseconds 60
          [LinkClick]::mouse_event(0x2, 0, 0, 0, [UIntPtr]::Zero)
          Start-Sleep -Milliseconds 60
          [LinkClick]::mouse_event(0x4, 0, 0, 0, [UIntPtr]::Zero)
          for ($w = 0; $w -lt 20 -and $null -eq $found; $w++) {
            Start-Sleep -Milliseconds 25
            if (Test-Path -LiteralPath $opened) { $found = "client ($x, $y)" }
          }
        }
      }
      # a click on the link just before the handler wrote may have opened it again
      Start-Sleep -Seconds 2
      $lines = @()
      if (Test-Path -LiteralPath $opened) { $lines = @([System.IO.File]::ReadAllLines($opened) | Where-Object { $_ }) }
      $after = [LinkClick]::GetForegroundWindow()
      try { $proc.Kill() } catch { }
      $proc.WaitForExit()
      $onlyItem = ($lines.Count -gt 0 -and @($lines | Where-Object { $_ -cne $itemUrl -and $_ -cne $itemUrlShell }).Count -eq 0)
      Check 'a real click on a link opens exactly the item''s URL; the window does not take the focus' (
        $h -ne [IntPtr]::Zero -and $null -ne $found -and $onlyItem -and $after -ne $h) "window=$h found=$found opened=[$($lines -join '|')] before=$before after=$after"
    } finally {
      Remove-Item -LiteralPath $key -Recurse -Force -ErrorAction SilentlyContinue
      Remove-Item -Force -ErrorAction SilentlyContinue $accountFile
    }
  }
}

Write-Host '== Screenshots'
$shotCases = @(
  @{ file = '01-window-ja.png'; lang = 'ja'; title = '1Password CLI の承認の依頼'; facts = (New-Facts 'claude' 'staging の VM を作るのに Proxmox の API の token を読む' 'op read op://Private/proxmox/token' ([ordered]@{ workspace = 'w1'; label = 'op-reason の作業'; pane = 'w1:p2' }) @([ordered]@{ kind = 'shared-cleared' })) },
  @{ file = '02-window-en.png'; lang = 'en'; title = '1Password CLI approval request'; facts = (New-Facts 'human' 'read the Proxmox API token to create the staging VM' 'op read op://Private/proxmox/token') },
  @{ file = '03-window-refs-ja.png'; lang = 'ja'; title = '1Password CLI の承認の依頼'; account = $true; facts = (New-RefsFacts 'テンプレートから設定ファイルを作る' 'op inject -i app.tpl op://Private/proxmox/token') },
  @{ file = '04-window-refs-hint-en.png'; lang = 'en'; title = '1Password CLI approval request'; facts = (New-RefsFacts 'write the settings file from the template' 'op inject -i app.tpl op://Private/proxmox/token') }
)
foreach ($c in $shotCases) {
  Reset-Fake
  $env:FAKE_NO_STDIN = '1'
  $env:FAKE_SLEEP_MS = '6000'
  if ($c.account) { [System.IO.File]::WriteAllText($accountFile, $accountJson) } else { Remove-Item -Force -ErrorAction SilentlyContinue $accountFile }
  $env:OPREASON_PAYLOAD = New-Payload 'desktop' @('read', 'op://Private/proxmox/token') $c.lang 'held' 90 'inherit' $c.facts
  $proc = Start-Process -FilePath $ps -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $launcher) -PassThru -WindowStyle Hidden
  Remove-Item Env:OPREASON_PAYLOAD -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 4
  $path = Join-Path $shots $c.file
  try { Save-WindowShot $path $c.title } catch { Write-Host ('       screenshot failed: ' + $_.Exception.Message) }
  $proc.WaitForExit()
  $st = Get-Status
  Check ("window for the screenshot " + $c.file) ((Test-Path -LiteralPath $path) -and $null -ne $st -and $st.window -eq 'shown') ("status=" + ($st | ConvertTo-Json -Compress))
}
Reset-Fake
Complete-Tests
