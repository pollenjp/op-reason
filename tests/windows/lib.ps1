# Helpers shared by the Windows tests (dot-sourced).
$script:passCount = 0
$script:failCount = 0

function Pass([string]$name) {
  $script:passCount++
  Write-Host "  ok   $name"
}

function Fail([string]$name, [string]$detail) {
  $script:failCount++
  Write-Host "  FAIL $name"
  if ($detail) { Write-Host ('       ' + ($detail -replace "`n", "`n       ")) }
}

function Check([string]$name, [bool]$condition, [string]$detail) {
  if ($condition) { Pass $name } else { Fail $name $detail }
}

function Complete-Tests {
  Write-Host ''
  Write-Host "pass=$($script:passCount) fail=$($script:failCount)"
  if ($script:failCount -gt 0) { exit 1 }
  exit 0
}
