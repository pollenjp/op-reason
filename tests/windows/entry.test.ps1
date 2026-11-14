# Runs the Windows entry (windows/op-reason.ps1 and windows/op-reason.cmd) end to end on Windows PowerShell 5.1 with
# the real launcher and a fake op.exe (tests/windows/fake-op.cs). The call table itself is checked by
# entry-plan.test.ps1.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tests/windows/entry.test.ps1
#
# What it checks:
#   - from PowerShell (& op-reason.ps1 ...): op gets the arguments, op's output can be captured in a variable, the exit
#     code is op's, the log (%LOCALAPPDATA%\op-reason\log.jsonl) has start (mode, kind, masked args, reason, caller with
#     cwd, repository and branch) and end (approved), the caller's environment variables and console encoding are put
#     back
#   - refusals: no reason → exit 2, usage on stderr, a refused record
#   - through op-reason.cmd: binary stdin and stdout byte for byte; empty arguments and -- arrive unchanged; a denial is
#     logged as denied and op's stderr reaches the caller
#   - a held mutex: exit 75 and an end record with lock-timeout
#   - service account: op gets the token and OP_BIOMETRIC_UNLOCK_ENABLED=false; kind service-account
#   - --skill and --version
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$entry = Join-Path $root 'windows\op-reason.ps1'
$shim = Join-Path $root 'windows\op-reason.cmd'
$ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ('op-reason-entry-' + [guid]::NewGuid().ToString('n'))
$fakeDir = Join-Path $work 'fake'
$binDir = Join-Path $work 'bin'
[void](New-Item -ItemType Directory -Force -Path $fakeDir, $binDir)
$env:FAKE_DIR = $fakeDir
$env:LOCALAPPDATA = Join-Path $work 'localappdata'
$opExe = Join-Path $binDir 'op.exe'
Add-Type -TypeDefinition ([System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'fake-op.cs'))) -OutputAssembly $opExe -OutputType ConsoleApplication
$env:OPREASON_OP_EXE = $opExe
$env:OPREASON_LANG = 'en'
$log = Join-Path $env:LOCALAPPDATA 'op-reason\log.jsonl'

function Reset-Fake {
  Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $fakeDir 'calls.jsonl'), $log
  foreach ($n in @('FAKE_RC', 'FAKE_WHOAMI_RC', 'FAKE_ERR_TEXT', 'FAKE_SLEEP_MS', 'FAKE_OUT', 'FAKE_NO_STDIN', 'OP_SERVICE_ACCOUNT_TOKEN', 'OP_CONNECT_HOST', 'OPREASON_LOCK_WAIT')) {
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

function Get-Records([string]$event) {
  if (-not (Test-Path -LiteralPath $log)) { return @() }
  return @([System.IO.File]::ReadAllLines($log, [System.Text.Encoding]::UTF8) | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.event -eq $event })
}

function Invoke-Cmd([string]$cmdline) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = Join-Path $env:SystemRoot 'System32\cmd.exe'
  $psi.Arguments = '/d /s /c "' + $cmdline + '"'
  $psi.UseShellExecute = $false
  $p = [System.Diagnostics.Process]::Start($psi)
  $p.WaitForExit()
  return $p.ExitCode
}

function Test-SameBytes([string]$a, [string]$b) {
  $x = [System.IO.File]::ReadAllBytes($a)
  $y = [System.IO.File]::ReadAllBytes($b)
  if ($x.Length -ne $y.Length) { return $false }
  for ($k = 0; $k -lt $x.Length; $k++) { if ($x[$k] -ne $y[$k]) { return $false } }
  return $true
}

# A git repository for the place (no git.exe needed: the entry reads the files)
$repo = Join-Path $work 'repo'
[void](New-Item -ItemType Directory -Force -Path (Join-Path $repo '.git'))
[System.IO.File]::WriteAllText((Join-Path $repo '.git\HEAD'), "ref: refs/heads/feat/x`n")
[System.IO.File]::WriteAllText((Join-Path $repo '.git\config'), "[remote `"origin`"]`n`turl = git@github.com:o/r.git`n")

Write-Host '== From PowerShell'
Reset-Fake
$env:FAKE_OUT = 'hello from op'
$env:FAKE_NO_STDIN = '1'
$env:OP_CONNECT_HOST = 'https://connect.example'
$encBefore = [Console]::OutputEncoding.CodePage
Push-Location $repo
$out = & $entry --reason 'read the settings' item get 'a b' password=hunter2
$rc = $LASTEXITCODE
Pop-Location
$calls = @(Get-Calls)
$start = @(Get-Records 'start') | Select-Object -Last 1
$end = @(Get-Records 'end') | Select-Object -Last 1
Check "op gets the arguments; its output can be captured; the exit code is op's" (
  $rc -eq 0 -and (@($out) -join "`n") -ceq 'hello from op' -and $calls.Count -ge 1 -and
  (@($calls[0].args) -join '|') -ceq 'item|get|a b|password=hunter2') ("rc=$rc out=$out calls=" + (@($calls | ForEach-Object { @($_.args) -join ' ' }) -join ','))
Check 'start record: mode, kind, masked args, reason, caller with repository and branch' (
  $null -ne $start -and $start.mode -eq 'desktop' -and $start.kind -eq 'desktop' -and $start.lock -eq 'held' -and
  (@($start.args) -join ' ') -ceq 'item get a b password=***' -and $start.reason -ceq 'read the settings' -and
  $start.caller.repo -eq 'o/r' -and $start.caller.branch -eq 'feat/x' -and $start.caller.type -eq 'human' -and
  $start.v -eq 1) ($start | ConvertTo-Json -Compress -Depth 5)
Check 'end record: approved, same id, window shown' ($null -ne $end -and $end.id -eq $start.id -and $end.result -eq 'approved' -and $end.window -eq 'shown') ($end | ConvertTo-Json -Compress -Depth 5)
Check "the caller's environment and console encoding are put back" ($env:OP_CONNECT_HOST -eq 'https://connect.example' -and [Console]::OutputEncoding.CodePage -eq $encBefore -and -not $env:OPREASON_PAYLOAD) "OP_CONNECT_HOST=$env:OP_CONNECT_HOST codepage=$([Console]::OutputEncoding.CodePage) before=$encBefore"

Write-Host '== Refusal'
Reset-Fake
$errFile = Join-Path $work 'refuse.err'
$rc = Invoke-Cmd ('"' + $shim + '" item get a 2> "' + $errFile + '"')
$refused = @(Get-Records 'refused') | Select-Object -Last 1
$errText = [System.IO.File]::ReadAllText($errFile)
Check 'no reason: exit 2, usage with the hint for agents, a refused record' ($rc -eq 2 -and $errText.Contains('op-reason --skill') -and $null -ne $refused -and $refused.why -eq 'no-reason' -and @(Get-Calls).Count -eq 0) ("rc=$rc err=$errText")

Write-Host '== Through op-reason.cmd'
Reset-Fake
$bin = Join-Path $work 'in.bin'
$bytes = New-Object byte[] (256 * 40 + 6)
for ($k = 0; $k -lt 256 * 40; $k++) { $bytes[$k] = [byte]($k % 256) }
[Array]::Copy([byte[]](13, 10, 0, 101, 110, 100), 0, $bytes, 256 * 40, 6)
[System.IO.File]::WriteAllBytes($bin, $bytes)
$outFile = Join-Path $work 'shim.out'
$rc = Invoke-Cmd ('"' + $shim + '" --reason "binary check" document get x < "' + $bin + '" > "' + $outFile + '"')
Check 'binary stdin and stdout pass byte for byte through the shim' ($rc -eq 0 -and (Test-SameBytes $bin $outFile)) ("rc=$rc out=" + (Get-Item $outFile).Length)
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$rc = Invoke-Cmd ('"' + $shim + '" --reason "args check" run -- child "" "a b" "q\"z" < NUL > NUL')
$calls = @(Get-Calls)
$got = ''
if ($calls.Count -gt 0) { $got = @($calls[0].args) -join '|' }
Check 'empty arguments, quotes and -- arrive unchanged through the shim' ($rc -eq 0 -and $got -ceq 'run|--|child||a b|q"z') "rc=$rc got=$got"
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:FAKE_RC = '1'
$env:FAKE_ERR_TEXT = '[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again'
$errFile = Join-Path $work 'denied.err'
$rc = Invoke-Cmd ('"' + $shim + '" --reason "denial check" item get x < NUL > NUL 2> "' + $errFile + '"')
$end = @(Get-Records 'end') | Select-Object -Last 1
$errText = [System.IO.File]::ReadAllText($errFile)
Check "a denial is logged as denied and op's stderr reaches the caller" ($rc -eq 1 -and $end.result -eq 'denied' -and $errText.Contains('authorization prompt dismissed')) ("rc=$rc err=$errText end=" + ($end | ConvertTo-Json -Compress))

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
$env:OPREASON_LOCK_WAIT = '2'
$errFile = Join-Path $work 'mutex.err'
$rc = Invoke-Cmd ('"' + $shim + '" --reason "mutex check" item get x < NUL > NUL 2> "' + $errFile + '"')
$end = @(Get-Records 'end') | Select-Object -Last 1
$errText = [System.IO.File]::ReadAllText($errFile)
Check 'a held mutex: exit 75, end lock-timeout, the message names the wait' ($rc -eq 75 -and $end.result -eq 'lock-timeout' -and $end.exit -eq 75 -and $errText.Contains('still waiting') -and @(Get-Calls).Count -eq 0) ("rc=$rc err=$errText end=" + ($end | ConvertTo-Json -Compress))
Stop-Process -Id $holder.Id -Force -ErrorAction SilentlyContinue

Write-Host '== Service account'
Reset-Fake
$env:FAKE_NO_STDIN = '1'
$env:OP_SERVICE_ACCOUNT_TOKEN = 'tok-SECRET'
$out = & $entry --reason 'CI reads' vault list
$rc = $LASTEXITCODE
$calls = @(Get-Calls)
$start = @(Get-Records 'start') | Select-Object -Last 1
$logText = [System.IO.File]::ReadAllText($log)
Check 'service account: op gets the token and OP_BIOMETRIC_UNLOCK_ENABLED=false; kind service-account; no token in the log' (
  $rc -eq 0 -and $calls.Count -eq 1 -and $calls[0].sa -eq 'tok-SECRET' -and $calls[0].bio -eq 'false' -and
  $start.kind -eq 'service-account' -and -not $logText.Contains('tok-SECRET')) ("rc=$rc calls=" + ($calls | ConvertTo-Json -Compress))

Write-Host '== Wrapper options'
Reset-Fake
$skill = (& $entry --skill) -join "`n"
$version = (& $entry --version) -join "`n"
Check '--skill prints the skill; --version prints the version' ($skill.StartsWith("---`nname: op-reason") -and $version -match '^op-reason \d+\.\d+\.\d+$') ("skill=" + $skill.Substring(0, [Math]::Min(40, $skill.Length)) + " version=$version")

Reset-Fake
Complete-Tests
