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
  foreach ($n in @('FAKE_RC', 'FAKE_WHOAMI_RC', 'FAKE_ERR_TEXT', 'FAKE_SLEEP_MS', 'FAKE_OUT', 'FAKE_NO_STDIN', 'OP_SERVICE_ACCOUNT_TOKEN')) {
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
# Save the top right of the work area (where the window opens) as a PNG.
function Save-WindowShot([string]$path) {
  $screen = [System.Windows.Forms.Screen]::PrimaryScreen
  $b = $screen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $g.Dispose()
  $wa = $screen.WorkingArea
  $w = [Math]::Min(620, $wa.Width)
  $h = [Math]::Min(460, $wa.Height)
  $rect = New-Object System.Drawing.Rectangle ($wa.Right - $w), $wa.Top, $w, $h
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
Check 'desktop runs whoami after op and signout --all when signed in' ($seq -ceq 'item get x,whoami,signout --all' -and $st.signout -eq 'ok') "calls=$seq signout=$($st.signout)"
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

Write-Host '== service'
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:FAKE_WHOAMI_RC = '0'
$env:OP_SERVICE_ACCOUNT_TOKEN = 'tok-test'
$r = Invoke-Launcher (New-Payload 'service' @('vault', 'list') 'en') 'service'
$calls = @(Get-Calls)
$st = Get-Status
Check 'service passes the token and runs neither whoami nor signout' ($calls.Count -eq 1 -and $calls[0].sa -eq 'tok-test' -and $st.signout -eq 'skipped') ("calls=$($calls.Count) status=" + ($st | ConvertTo-Json -Compress))

Write-Host '== Screenshots'
$shotCases = @(
  @{ file = '01-window-ja.png'; lang = 'ja'; facts = (New-Facts 'claude' 'staging の VM を作るのに Proxmox の API の token を読む' 'op read op://Private/proxmox/token' ([ordered]@{ workspace = 'w1'; label = 'op-reason の作業'; pane = 'w1:p2' }) @([ordered]@{ kind = 'shared-cleared' })) },
  @{ file = '02-window-en.png'; lang = 'en'; facts = (New-Facts 'human' 'read the Proxmox API token to create the staging VM' 'op read op://Private/proxmox/token') }
)
foreach ($c in $shotCases) {
  Reset-Fake
  $env:FAKE_NO_STDIN = '1'
  $env:FAKE_SLEEP_MS = '6000'
  $env:OPREASON_PAYLOAD = New-Payload 'desktop' @('read', 'op://Private/proxmox/token') $c.lang 'held' 90 'inherit' $c.facts
  $proc = Start-Process -FilePath $ps -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $launcher) -PassThru -WindowStyle Hidden
  Remove-Item Env:OPREASON_PAYLOAD -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 4
  $path = Join-Path $shots $c.file
  try { Save-WindowShot $path } catch { Write-Host ('       screenshot failed: ' + $_.Exception.Message) }
  $proc.WaitForExit()
  $st = Get-Status
  Check ("window for the screenshot " + $c.file) ((Test-Path -LiteralPath $path) -and $null -ne $st -and $st.window -eq 'shown') ("status=" + ($st | ConvertTo-Json -Compress))
}
Reset-Fake
Complete-Tests
