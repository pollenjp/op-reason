# Runs the call table (tests/cases/calls.json) against the Windows entry's planner (op-reason.ps1 with
# OPREASON_ENTRY_PLAN_ONLY=1), the same table the Rust core tests read, and the reference table (tests/cases/refs.json)
# against its reference collector (the files of a case in a scratch directory that is the current directory; the cases
# that need stdin are skipped: the PowerShell entry never reads stdin). Runs on Windows PowerShell 5.1 and on
# PowerShell 7 (Linux too).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tests/windows/entry-plan.test.ps1
#
# The arguments are passed by splatting an array (& op-reason.ps1 @argv), which hands '--' to the script as a value.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib.ps1')

$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$entry = Join-Path $root 'windows/op-reason.ps1'
$table = [System.IO.File]::ReadAllText((Join-Path $root 'tests/cases/calls.json'), [System.Text.Encoding]::UTF8) | ConvertFrom-Json

$envNames = @('OP_SERVICE_ACCOUNT_TOKEN', 'OP_CONNECT_HOST', 'OP_CONNECT_TOKEN')
$env:OPREASON_ENTRY_PLAN_ONLY = '1'
foreach ($case in $table.cases) {
  foreach ($e in $envNames) { [Environment]::SetEnvironmentVariable($e, $null, 'Process') }
  if ($null -ne $case.env) {
    foreach ($p in $case.env.PSObject.Properties) { [Environment]::SetEnvironmentVariable($p.Name, [string]$p.Value, 'Process') }
  }
  $env:OPREASON_ENTRY_NAME = [string]$case.invokedAs
  $argv = [string[]]@($case.argv | ForEach-Object { [string]$_ })
  $json = & $entry @argv
  $got = ($json -join "`n") | ConvertFrom-Json
  $problems = @()
  $want = $case.expect
  foreach ($field in @('action', 'why', 'exit', 'mode', 'kind', 'command', 'reason', 'shell', 'completionCommand')) {
    if ($null -ne $want.PSObject.Properties[$field]) {
      if ([string]$got.$field -cne [string]$want.$field) { $problems += "$field=[$($got.$field)] want [$($want.$field)]" }
    }
  }
  foreach ($field in @('opArgs', 'masked', 'logArgs')) {
    if ($null -ne $want.PSObject.Properties[$field]) {
      $g = @($got.$field) -join [char]0x1F
      $w = @($want.$field) -join [char]0x1F
      if ($g -cne $w) { $problems += "$field=[$(@($got.$field) -join '|')] want [$(@($want.$field) -join '|')]" }
    }
  }
  if ($problems.Count -eq 0) { Pass $case.name } else { Fail $case.name ($problems -join '; ') }
}
foreach ($e in $envNames) { [Environment]::SetEnvironmentVariable($e, $null, 'Process') }

$refsTable = [System.IO.File]::ReadAllText((Join-Path $root 'tests/cases/refs.json'), [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ('op-reason-refs-' + [guid]::NewGuid().ToString('n'))
$env:OPREASON_ENTRY_NAME = 'op'
foreach ($case in $refsTable.cases) {
  if ($case.powershell -eq $false) { continue }
  $dir = Join-Path $scratch ([guid]::NewGuid().ToString('n'))
  [void](New-Item -ItemType Directory -Force -Path $dir)
  if ($null -ne $case.files) {
    foreach ($f in $case.files.PSObject.Properties) {
      $path = Join-Path $dir $f.Name
      [void](New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path))
      [System.IO.File]::WriteAllText($path, [string]$f.Value, (New-Object System.Text.UTF8Encoding($false)))
    }
  }
  $argv = [string[]](@('--reason', 'references') + @($case.args | ForEach-Object { [string]$_ }))
  Push-Location -LiteralPath $dir
  try {
    $got = ((& $entry @argv) -join "`n") | ConvertFrom-Json
  } finally {
    Pop-Location
  }
  $problems = @()
  $gotList = @($got.refs.list | Where-Object { $null -ne $_ })
  $wantList = @($case.expect.list | Where-Object { $null -ne $_ })
  if (($gotList -join [char]0x1F) -cne ($wantList -join [char]0x1F)) { $problems += "list=[$($gotList -join '|')] want [$($wantList -join '|')]" }
  $gotFrom = ConvertTo-Json -InputObject @($got.refs.from | Where-Object { $null -ne $_ }) -Compress -Depth 4
  $wantFrom = ConvertTo-Json -InputObject @($case.expect.from | Where-Object { $null -ne $_ }) -Compress -Depth 4
  if ($gotFrom -cne $wantFrom) { $problems += "from=$gotFrom want $wantFrom" }
  if ([string]$got.refs.account -cne [string]$case.expect.account) { $problems += "account=[$($got.refs.account)] want [$($case.expect.account)]" }
  if ($problems.Count -eq 0) { Pass ('references: ' + $case.name) } else { Fail ('references: ' + $case.name) ($problems -join '; ') }
}
Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item Env:OPREASON_ENTRY_PLAN_ONLY, Env:OPREASON_ENTRY_NAME -ErrorAction SilentlyContinue

# A script that names the automatic input variable makes powershell.exe -File read all of a redirected stdin before the
# script runs (seen on Windows PowerShell 5.1), so the shim's stdin would never reach op.exe
$named = Select-String -LiteralPath $entry -Pattern ('\$' + 'input\b')
Check 'op-reason.ps1 never names the automatic input variable' ($null -eq $named) ($named | Out-String)
Complete-Tests
