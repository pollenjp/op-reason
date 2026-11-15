#Requires -Version 7
# Makes the Windows release asset, op-reason-windows.zip, and its SHA-256 file (the format of sha256sum, with a LF line
# end). The zip holds one folder, op-reason-windows:
#
#   op-reason.exe        the Windows entry, and the runner of calls from WSL
#   launcher.ps1         the fallback of calls from WSL (the WSL binary looks for it next to op-reason.exe)
#   SKILL.md, README.md, README.ja.md, LICENSE-MIT, LICENSE-APACHE
#   powershell-entry\    the PowerShell entry, for Windows where Smart App Control blocks op-reason.exe:
#                        op-reason.ps1, op-reason.cmd, launcher.ps1, SKILL.md
#
# The PowerShell entry has a folder of its own because PowerShell prefers op-reason.ps1 to op-reason.exe in the same
# folder: typing op-reason would run the fallback (or fail under the default execution policy).
#
# The release workflow attaches the zip to the draft release; the pull request job (windows-exe.yml) runs the same
# script, so that this step is tested before a tag needs it.
#
#   pwsh -File .github/scripts/windows-dist.ps1 -Exe crates\op-reason-win\target\release\op-reason.exe -Out dist
param(
  [Parameter(Mandatory)] [string] $Exe,
  [Parameter(Mandatory)] [string] $Out
)
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path "$PSScriptRoot/../..").Path
$name = 'op-reason-windows'
$top = @('launcher/launcher.ps1', 'skill/SKILL.md', 'README.md', 'README.ja.md', 'LICENSE-MIT', 'LICENSE-APACHE')
$entry = @('windows/op-reason.ps1', 'windows/op-reason.cmd', 'launcher/launcher.ps1', 'skill/SKILL.md')

New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$stage = Join-Path $Out $name
if (Test-Path -LiteralPath $stage) { Remove-Item -Recurse -Force -LiteralPath $stage }
New-Item -ItemType Directory -Path (Join-Path $stage 'powershell-entry') | Out-Null
Copy-Item -LiteralPath $Exe -Destination (Join-Path $stage 'op-reason.exe')
foreach ($f in $top) { Copy-Item -LiteralPath (Join-Path $root $f) -Destination $stage }
foreach ($f in $entry) { Copy-Item -LiteralPath (Join-Path $root $f) -Destination (Join-Path $stage 'powershell-entry') }

# The name matters: called as op-reason.exe, --version is the wrapper's own and needs no op.exe.
& (Join-Path $stage 'op-reason.exe') --version
if ($LASTEXITCODE -ne 0) { throw "op-reason.exe --version exited with $LASTEXITCODE" }

$zip = Join-Path $Out "$name.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -Force -LiteralPath $zip }
Add-Type -AssemblyName System.IO.Compression.FileSystem
# includeBaseDirectory: the entries are op-reason-windows/<file>, with / as the separator
[IO.Compression.ZipFile]::CreateFromDirectory($stage, $zip, [IO.Compression.CompressionLevel]::Optimal, $true)
Remove-Item -Recurse -Force -LiteralPath $stage

# What the zip must hold, and nothing else
$want = @(
  @('op-reason.exe') + ($top | ForEach-Object { Split-Path -Leaf $_ })
  $entry | ForEach-Object { 'powershell-entry/' + (Split-Path -Leaf $_) }
) | ForEach-Object { "$name/$_" } | Sort-Object
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try { $have = @($archive.Entries | ForEach-Object { $_.FullName }) | Sort-Object } finally { $archive.Dispose() }
if (Compare-Object $want $have) { throw "unexpected entries in ${name}.zip: $($have -join ', ')" }

$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$zip.sha256", "$hash  $name.zip`n")
$lines = @("${name}.zip SHA-256: $hash", '', 'Entries:', '') + ($have | ForEach-Object { "- ``$_``" })
$lines -join "`n"
if ($env:GITHUB_STEP_SUMMARY) { $lines -join "`n" | Add-Content -Path $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
