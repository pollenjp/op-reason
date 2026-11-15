#Requires -Version 7
# Puts op-reason.exe and its SHA-256 file (the format of sha256sum, with a LF line end) into a directory. The release
# workflow attaches them to the draft release; the pull request job (windows-exe.yml) runs the same script, so that
# this step is tested before a tag needs it.
#
#   pwsh -File .github/scripts/windows-exe-dist.ps1 -Exe target\release\op-reason.exe -Out dist
param(
  [Parameter(Mandatory)] [string] $Exe,
  [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$dest = Join-Path (Resolve-Path $Out).Path 'op-reason.exe'
Copy-Item -LiteralPath $Exe -Destination $dest
# The name matters: called as op-reason.exe, --version is the wrapper's own and needs no op.exe.
& $dest --version
if ($LASTEXITCODE -ne 0) { throw "op-reason.exe --version exited with $LASTEXITCODE" }
$hash = (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$dest.sha256", "$hash  op-reason.exe`n")
$line = "op-reason.exe SHA-256: $hash"
$line
if ($env:GITHUB_STEP_SUMMARY) { $line | Add-Content -Path $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
