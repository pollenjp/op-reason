# Runs the call table (tests/cases/calls.json) against the Windows entry's planner (op-reason.ps1 with
# OPREASON_ENTRY_PLAN_ONLY=1), the same table the Rust core tests read. Runs on Windows PowerShell 5.1 and on
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
Remove-Item Env:OPREASON_ENTRY_PLAN_ONLY, Env:OPREASON_ENTRY_NAME -ErrorAction SilentlyContinue

# A script that names the automatic input variable makes powershell.exe -File read all of a redirected stdin before the
# script runs (seen on Windows PowerShell 5.1), so the shim's stdin would never reach op.exe
$named = Select-String -LiteralPath $entry -Pattern ('\$' + 'input\b')
Check 'op-reason.ps1 never names the automatic input variable' ($null -eq $named) ($named | Out-String)
Complete-Tests
