# op-reason Windows entry: calls the 1Password CLI (op.exe) with a required reason, through a fresh powershell.exe that
# runs the launcher (launcher.ps1), so that the 1Password approval covers this call only. The same rules as the WSL
# executable (docs/superpowers/specs/2026-11-15-op-reason-design.md, sections 4, 5 and 7):
#
#   op-reason --reason "<why>" <op arguments...>
#
# Two ways in:
#   - from PowerShell (op-reason.ps1 on PATH, or & path\op-reason.ps1): the arguments come from PowerShell. PowerShell
#     takes an unquoted -- for itself; write '--' (quoted) for op run's child command. The launcher runs as a native
#     command, so its output can be captured ($x = op-reason ...).
#   - from op-reason.cmd (cmd.exe, other programs): the shim sets OPREASON_ENTRY=cmd and the entry reads its arguments
#     from the process command line (PowerShell's -File binding drops empty arguments and --). The launcher inherits
#     the standard handles, so stdin and stdout stay binary-safe.
#
# What it does: plan the arguments (the call table tests/cases/calls.json is shared with the Rust core), collect the
# caller (Claude Code, Windows Terminal, the process chain, herdr), the place (repository and branch read from git
# files: Windows may have no git.exe), log to %LOCALAPPDATA%\op-reason\log.jsonl (the same format as WSL), and start the
# launcher with the payload (lock held, stderr scan). The launcher takes the named mutex Local\op-reason, so calls from
# Windows and WSL run one at a time.
#
# Exit codes: op's; 2 = no reason, empty reason, too long, wrong wrapper option; 75 = another call's approval is still
# pending; 127 = op.exe, powershell.exe or the launcher not found; 1 = the launcher failed before op.exe started.
# A call stopped by Ctrl+C is logged as interrupted (exit 130) when PowerShell lets the cleanup run.
#
# Environment: OPREASON_LANG (en | ja), OPREASON_LOCK_WAIT (seconds, default 90), OPREASON_OP_EXE (op.exe),
# OPREASON_POWERSHELL (powershell.exe). Set by the shim: OPREASON_ENTRY=cmd, OPREASON_ENTRY_NAME (its file name).
# For tests: OPREASON_ENTRY_PLAN_ONLY=1 prints the plan as JSON and exits.
#
# Windows PowerShell 5.1 syntax (no ??, ?., ternary, pipeline chains, $IsWindows); also runs on PowerShell 7.
# UTF-8 with a byte order mark: without one, Windows PowerShell 5.1 reads the file in the ANSI code page.
$ProgressPreference = 'SilentlyContinue'
$script:Version = '0.1.0'

# --- Language and texts

function Get-Lang {
  $v = $env:OPREASON_LANG
  if ($v) {
    if ($v.ToLowerInvariant().StartsWith('ja')) { return 'ja' }
    return 'en'
  }
  try {
    if ((Get-UICulture).TwoLetterISOLanguageName -eq 'ja') { return 'ja' }
  } catch { }
  return 'en'
}

function Get-Messages([string]$lang) {
  if ($lang -eq 'ja') {
    return @{
      usage = @"
使い方: op-reason --reason "<なぜ op を呼ぶか>" <op の引数…>

op-reason は、呼ぶ理由を必須にした 1Password CLI の wrapper です。呼び出しごとに新しい
powershell.exe を挟んで op.exe を起こし、1Password の承認をその 1 回に閉じます。理由は、
承認の画面と一緒に出る窓と、記録 (%LOCALAPPDATA%\op-reason\log.jsonl) に残ります。

理由 (--reason)
  何のために何を読む・書くかを 1 文で書きます。無ければ 2 で断ります。
  --reason "<理由>" でも --reason=<理由> でもよく、-- より前ならどこに置いてもかまいません。
  例: op-reason --reason "staging の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"

理由なしで通るもの (承認の画面は出ません)
  op-reason --help      この使い方
  op-reason --version   op-reason の版
  op-reason help        op 本体の使い方 (op-reason <コマンド> --help も)

wrapper の option (最初の引数のときだけ)
  op-reason --skill     AI エージェント向けの手引き (SKILL.md)

PowerShell では、op run の -- を '--' と引用符で囲んでください (囲まない -- は PowerShell が取ります)。
cmd.exe やほかの program からは op-reason.cmd を呼びます (cmd.exe が引数を読み直すので、空白の無い
& | < > ^ % を含む引数は崩れることがあります)。

AI エージェントは、最初に op-reason --skill を打って呼び方を読んでください。

op-reason の窓が出ていない 1Password CLI の承認の画面は、拒否してください。
op-reason {0} は非公式の道具です (1Password の製品ではありません)。
"@
      usageShort = @"
使い方: op-reason --reason "<なぜ op を呼ぶか>" <op の引数…>
  op-reason は、呼ぶ理由を必須にした wrapper です。理由は 1Password の承認の画面と一緒に出る窓と、記録に残ります。
  何のために何を読む・書くかを 1 文で書いてください。
  理由なしで通るのは --version と help (--help・-h・help) だけです。詳しくは op-reason --help。
  AI エージェントは op-reason --skill で手引きを読んでください。
"@
      emptyReason   = '理由 (--reason) が空です。なぜ op を呼ぶかを 1 文で書いてください。'
      noReason      = '理由 (--reason) がありません。人が打つときも要ります。'
      noOpExe       = 'op.exe が見つかりません{0}。1Password CLI を入れる (winget install AgileBits.1Password.CLI) か、OPREASON_OP_EXE に op.exe のパスを入れてください。'
      noPowerShell  = '{0} が見つかりません。'
      noLauncher    = 'launcher.ps1 が見つかりません ({0})。op-reason.ps1 と同じ所に置いてください。'
      tooLong       = 'op の引数が長すぎます (base64 で {0} 字。Windows の環境変数に載るのは 32767 字まで)。'
      payloadLong   = '呼び出しが大きすぎて launcher に渡せません ({0} 字。上限は 32000 字)。'
      longHint      = '長い値は op の --template やファイル (op document create <パス> など) で渡してください。'
      mutexTimeout  = 'ほかの op-reason の呼び出し (Windows か WSL) の承認待ちが終わらないので、打たずに終えました。'
      waitingFor    = '待たせている呼び出し: {0}'
      answerFirst   = 'その承認の画面に応えてから (窓の出ていない画面なら拒否してから)、打ち直してください。'
      unknown       = '(分からない)'
      completion    = 'fish の補完は WSL の op-reason (op-reason --completion fish) が出します。'
      completionUsage = '--completion は fish だけです: --completion fish [<コマンド>] (渡されたもの: {0})'
      logInfo       = 'Windows の記録は {0} にあります (1 行 1 件の JSON)。PowerShell なら: Get-Content {0} | ConvertFrom-Json | Format-Table time, event, reason'
      helpReason    = '(理由なし: help)'
    }
  }
  return @{
    usage = @"
Usage: op-reason --reason "<why you call op>" <op arguments...>

op-reason is a wrapper for the 1Password CLI that requires a reason. Every call
starts a fresh powershell.exe that starts op.exe, so a 1Password approval covers
that one call only. The reason appears in a window next to the approval prompt
and in the log (%LOCALAPPDATA%\op-reason\log.jsonl).

Reason (--reason)
  One sentence: what for, and what you read or write. Without it the call is
  refused (exit 2). --reason "<reason>" and --reason=<reason> both work,
  anywhere before --.
  Example: op-reason --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token"

Without a reason (no approval prompt)
  op-reason --help      this usage
  op-reason --version   op-reason's version
  op-reason help        op's own help (and op-reason <command> --help)

Wrapper options (first argument only)
  op-reason --skill     a guide for AI agents (SKILL.md)

In PowerShell, quote the -- of op run as '--' (PowerShell takes an unquoted --).
From cmd.exe and other programs, call op-reason.cmd (cmd.exe re-parses the
arguments: & | < > ^ % in an argument without spaces can break).

AI agents: run op-reason --skill once to learn how to call op here.

Deny any 1Password CLI approval prompt that comes without the op-reason window.
op-reason {0} is unofficial: it is not made or supported by 1Password.
"@
    usageShort = @"
Usage: op-reason --reason "<why you call op>" <op arguments...>
  op-reason is a wrapper for the 1Password CLI: every call needs a reason. The reason appears in the window next to
  the 1Password approval prompt and in the log. Write what for and what you read or write, in one sentence.
  Only --version and help (--help, -h, help) work without a reason. See op-reason --help.
  AI agents: run op-reason --skill for the full guide.
"@
    emptyReason   = 'The reason (--reason) is empty. Write in one sentence why you call op.'
    noReason      = 'There is no reason (--reason). People need it too.'
    noOpExe       = 'op.exe was not found{0}. Install the 1Password CLI (winget install AgileBits.1Password.CLI) or set OPREASON_OP_EXE to the path of op.exe.'
    noPowerShell  = '{0} was not found.'
    noLauncher    = 'launcher.ps1 was not found ({0}). Keep it next to op-reason.ps1.'
    tooLong       = 'The arguments of op are too long ({0} characters of base64; a Windows environment variable holds at most 32767).'
    payloadLong   = 'The call is too large to pass to the launcher ({0} characters; the limit is 32000).'
    longHint      = 'Pass long values with op''s --template or a file (op document create <path>, for example).'
    mutexTimeout  = 'Another op-reason call (from Windows or WSL) is still waiting for its approval, so this call was not run.'
    waitingFor    = 'Waiting for: {0}'
    answerFirst   = 'Answer that approval prompt first (deny it if it came without the window), then run this again.'
    unknown       = '(unknown)'
    completion    = 'The fish completion comes from op-reason on WSL (op-reason --completion fish).'
    completionUsage = '--completion supports only fish: --completion fish [<command>] (got: {0})'
    logInfo       = 'The Windows log is {0} (one JSON object per line). In PowerShell: Get-Content {0} | ConvertFrom-Json | Format-Table time, event, reason'
    helpReason    = '(no reason: help)'
  }
}

function Write-Err([string]$text) {
  try { [Console]::Error.WriteLine('op-reason: ' + $text) } catch { }
}

# --- Arguments (the same rules as crates/op-reason-core/src/args.rs)

# Split a Windows command line the way the C runtime does (backslashes before a quote, "" inside quotes).
function Split-CommandLine([string]$line) {
  $result = New-Object System.Collections.Generic.List[string]
  $i = 0
  $n = $line.Length
  while ($i -lt $n) {
    while ($i -lt $n -and ($line[$i] -eq ' ' -or $line[$i] -eq "`t")) { $i++ }
    if ($i -ge $n) { break }
    $sb = New-Object System.Text.StringBuilder
    $inQuote = $false
    while ($i -lt $n -and ($inQuote -or ($line[$i] -ne ' ' -and $line[$i] -ne "`t"))) {
      $slashes = 0
      while ($i -lt $n -and $line[$i] -eq '\') { $slashes++; $i++ }
      if ($i -lt $n -and $line[$i] -eq '"') {
        [void]$sb.Append([char]92, [int][Math]::Floor($slashes / 2))
        if ($slashes % 2 -eq 1) {
          [void]$sb.Append('"')
          $i++
        } elseif ($inQuote -and $i + 1 -lt $n -and $line[$i + 1] -eq '"') {
          [void]$sb.Append('"')
          $i += 2
        } else {
          $inQuote = -not $inQuote
          $i++
        }
      } else {
        if ($slashes -gt 0) { [void]$sb.Append([char]92, $slashes) }
        if ($i -lt $n) {
          if (-not $inQuote -and ($line[$i] -eq ' ' -or $line[$i] -eq "`t")) { break }
          [void]$sb.Append($line[$i])
          $i++
        }
      }
    }
    $result.Add($sb.ToString())
  }
  return , $result.ToArray()
}

# The arguments after "-File <script>" in this process's command line (the shim's way in).
function Get-ShimArgs {
  $all = Split-CommandLine ([Environment]::CommandLine)
  for ($k = 0; $k -lt $all.Length - 1; $k++) {
    if ($all[$k] -ieq '-File') {
      $rest = @()
      if ($k + 2 -le $all.Length - 1) { $rest = $all[($k + 2)..($all.Length - 1)] }
      return , [string[]]$rest
    }
  }
  return , [string[]]@()
}

function Get-Normalized([string]$s) {
  $chars = $s.ToCharArray()
  for ($k = 0; $k -lt $chars.Length; $k++) {
    $c = [int]$chars[$k]
    if ($c -lt 32 -or $c -eq 127) { $chars[$k] = ' ' }
  }
  $t = (New-Object string (, $chars)) -replace ' +', ' '
  return $t.Trim(' ')
}

# Cut text longer than $n characters (Unicode code points) and append $ellipsis.
function Get-Clip([string]$s, [int]$n, [string]$ellipsis) {
  if ($null -eq $s) { return '' }
  $count = 0
  $k = 0
  while ($k -lt $s.Length) {
    if ($count -eq $n) { return $s.Substring(0, $k) + $ellipsis }
    if ([char]::IsHighSurrogate($s[$k]) -and $k + 1 -lt $s.Length -and [char]::IsLowSurrogate($s[$k + 1])) { $k += 2 } else { $k += 1 }
    $count++
  }
  return $s
}

function Test-IsHelp([string[]]$opArgs) {
  if ($opArgs.Length -eq 0) { return $false }
  if (@('--version', '-v', '--help', '-h', 'help') -ccontains $opArgs[0]) { return $true }
  foreach ($a in $opArgs) {
    if ($a -ceq '--') { return $false }
    if ($a -ceq '--help' -or $a -ceq '-h') { return $true }
  }
  return $false
}

function Get-Masked([string[]]$opArgs) {
  $masked = New-Object System.Collections.Generic.List[string]
  $nextSecret = $false
  $after = $false
  $firstAfter = $true
  foreach ($a in $opArgs) {
    if ($after) {
      if ($firstAfter -or $a -cmatch '^-[A-Za-z0-9-]+$') { $masked.Add($a) } else { $masked.Add('***') }
      $firstAfter = $false
      continue
    }
    if ($nextSecret) {
      $masked.Add('***')
      $nextSecret = $false
      continue
    }
    if ($a -ceq '--') {
      $masked.Add($a)
      $after = $true
    } elseif ($a -ceq '--session' -or $a -ceq '--secret-key') {
      $masked.Add($a)
      $nextSecret = $true
    } elseif ($a.StartsWith('--session=', [StringComparison]::Ordinal)) {
      $masked.Add('--session=***')
    } elseif ($a.StartsWith('--secret-key=', [StringComparison]::Ordinal)) {
      $masked.Add('--secret-key=***')
    } elseif ($a.StartsWith('-', [StringComparison]::Ordinal) -or $a.StartsWith('op://', [StringComparison]::Ordinal)) {
      $masked.Add($a)
    } elseif ($a.Contains('=')) {
      $masked.Add($a.Substring(0, $a.IndexOf('=')) + '=***')
    } else {
      $masked.Add($a)
    }
  }
  return , $masked.ToArray()
}

function Get-Display([string[]]$masked) {
  $sb = New-Object System.Text.StringBuilder
  [void]$sb.Append('op')
  foreach ($a in $masked) {
    # Control characters (newline, tab, escape, ...) become spaces
    $chars = $a.ToCharArray()
    for ($k = 0; $k -lt $chars.Length; $k++) { if ([char]::IsControl($chars[$k])) { $chars[$k] = ' ' } }
    $t = New-Object string (, $chars)
    [void]$sb.Append(' ')
    if ($t.Length -gt 0 -and $t -cmatch '^[A-Za-z0-9_./:=@%+,*?-]+$') {
      [void]$sb.Append($t)
    } else {
      [void]$sb.Append('"').Append($t.Replace('\', '\\').Replace('"', '\"')).Append('"')
    }
  }
  return $sb.ToString()
}

# Plan an invocation: the wrapper's options, the reason, the mode.
function Get-Plan([string]$name, [string[]]$argv) {
  $plan = @{ action = ''; why = $null; refusal = $null; mode = $null; kind = $null; reason = ''; haveReason = $false; opArgs = @(); rest = @(); shell = $null; command = $null; message = $null }
  if ($argv.Length -gt 0) {
    $first = $argv[0]
    $rest = @()
    if ($argv.Length -gt 1) { $rest = $argv[1..($argv.Length - 1)] }
    if ($first -ceq '--skill' -and $rest.Length -eq 0) { $plan.action = 'skill'; return $plan }
    if ($first -ceq '--completion') {
      if ($rest.Length -ge 1 -and $rest[0] -ceq 'fish' -and $rest.Length -le 2) {
        $plan.action = 'completion'
        $plan.shell = 'fish'
        $plan.command = 'op'
        if ($rest.Length -eq 2) { $plan.command = $rest[1] }
      } else {
        $plan.action = 'error'
        $plan.message = ($M.completionUsage -f ($rest -join ' '))
      }
      return $plan
    }
    if ($first -ceq '--log') { $plan.action = 'log'; $plan.rest = $rest; return $plan }
    if ($name -eq 'op-reason' -and $rest.Length -eq 0) {
      if ($first -ceq '--help' -or $first -ceq '-h') { $plan.action = 'usage'; return $plan }
      if ($first -ceq '--version' -or $first -ceq '-V') { $plan.action = 'version'; return $plan }
    }
  }
  $opArgs = New-Object System.Collections.Generic.List[string]
  $seenDashDash = $false
  $k = 0
  while ($k -lt $argv.Length) {
    $a = $argv[$k]
    if (-not $seenDashDash -and -not $plan.haveReason) {
      if ($a -ceq '--reason') {
        $plan.haveReason = $true
        if ($k + 1 -lt $argv.Length) { $plan.reason = $argv[$k + 1] } else { $plan.reason = '' }
        $k += 2
        continue
      }
      if ($a.StartsWith('--reason=', [StringComparison]::Ordinal)) {
        $plan.haveReason = $true
        $plan.reason = $a.Substring(9)
        $k += 1
        continue
      }
    }
    if ($a -ceq '--') { $seenDashDash = $true }
    $opArgs.Add($a)
    $k += 1
  }
  $plan.opArgs = $opArgs.ToArray()
  if ($plan.haveReason) { $plan.reason = Get-Normalized $plan.reason }
  if ($plan.opArgs.Length -eq 0) { $plan.action = 'usage'; return $plan }
  if ($plan.haveReason) {
    if ($plan.reason -eq '') {
      $plan.action = 'refuse'; $plan.why = 'no-reason'; $plan.refusal = 'empty'
    } elseif ($env:OP_SERVICE_ACCOUNT_TOKEN) {
      $plan.action = 'run'; $plan.mode = 'service'; $plan.kind = 'service-account'
    } elseif ($env:OP_CONNECT_HOST -and $env:OP_CONNECT_TOKEN) {
      $plan.action = 'run'; $plan.mode = 'service'; $plan.kind = 'connect'
    } else {
      $plan.action = 'run'; $plan.mode = 'desktop'; $plan.kind = 'desktop'
    }
  } elseif (Test-IsHelp $plan.opArgs) {
    $plan.action = 'run'; $plan.mode = 'help'; $plan.kind = 'help'
  } else {
    $plan.action = 'refuse'; $plan.why = 'no-reason'; $plan.refusal = 'missing'
  }
  return $plan
}

# --- Caller and place

function Invoke-WithTimeout([string]$file, [string[]]$arguments, [int]$ms) {
  try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $file
    $psi.Arguments = (@($arguments | ForEach-Object { Format-WinArg $_ }) -join ' ')
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    $psi.CreateNoWindow = $true
    $psi.StandardOutputEncoding = [System.Text.Encoding]::UTF8
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEndAsync()
    if (-not $p.WaitForExit($ms)) {
      try { $p.Kill() } catch { }
      return $null
    }
    if ($p.ExitCode -ne 0) { return $null }
    return $out.Result
  } catch {
    return $null
  }
}

# Process names from this process up its parents (up to $depth), with the pid of a claude.exe among them.
function Get-ProcessChain([int]$depth) {
  $names = @()
  $claudePid = $null
  try {
    $all = @{}
    foreach ($p in (Get-CimInstance -ClassName Win32_Process -Property ProcessId, ParentProcessId, Name -ErrorAction Stop)) {
      $all[[int]$p.ProcessId] = $p
    }
    $cur = $all[[int]$PID]
    $seen = @{}
    for ($k = 0; $k -lt $depth -and $null -ne $cur; $k++) {
      $id = [int]$cur.ProcessId
      if ($seen.ContainsKey($id)) { break }
      $seen[$id] = $true
      $names += [string]$cur.Name
      if ($null -eq $claudePid -and ([string]$cur.Name) -ieq 'claude.exe') { $claudePid = $id }
      $cur = $all[[int]$cur.ParentProcessId]
    }
  } catch { }
  return @{ names = $names; claudePid = $claudePid }
}

function Get-CallerInfo([string]$lang) {
  $chain = Get-ProcessChain 6
  $isClaude = ($env:CLAUDECODE -eq '1') -or [bool]$env:CLAUDE_CODE_SESSION_ID -or ($null -ne $chain.claudePid)
  $claudePid = $null
  if ($isClaude) {
    $n = 0
    if ($env:CLAUDE_PID -and [int]::TryParse($env:CLAUDE_PID, [ref]$n)) { $claudePid = $n } else { $claudePid = $chain.claudePid }
  }
  $detailParts = @()
  if ($chain.names.Count -gt 0) { $detailParts += ($chain.names -join ' ← ') }
  if ($env:WT_SESSION) { $detailParts += 'Windows Terminal' }
  $detail = $null
  if ($detailParts.Count -gt 0) { $detail = $detailParts -join ' · ' }
  $herdr = $null
  $label = ''
  if ($env:HERDR_ENV -eq '1') {
    $ws = [string]$env:HERDR_WORKSPACE_ID
    if ($ws) {
      $herdrCmd = Get-Command herdr -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
      if ($null -ne $herdrCmd) {
        $json = Invoke-WithTimeout $herdrCmd.Source @('workspace', 'get', $ws) 2000
        if ($json) {
          try { $label = [string](($json | ConvertFrom-Json).result.workspace.label) } catch { $label = '' }
          if ($null -eq $label) { $label = '' }
          $label = -join @($label.ToCharArray() | ForEach-Object { if ([char]::IsControl($_)) { ' ' } else { $_ } })
        }
      }
    }
    $herdr = [ordered]@{ workspace = $ws; label = $label; pane = [string]$env:HERDR_PANE_ID }
  }
  $session = $null
  if ($env:CLAUDE_CODE_SESSION_ID) { $session = [string]$env:CLAUDE_CODE_SESSION_ID }
  return @{ isClaude = $isClaude; claudePid = $claudePid; detail = $detail; herdr = $herdr; label = $label; session = $session }
}

# Repository and branch from git files (the same rules as crates/op-reason-core/src/place.rs).
function Get-GitPlace([string]$cwd) {
  $info = @{ repo = $null; branch = $null }
  try {
    $dir = $cwd
    $gitDir = $null
    $top = $null
    while ($dir) {
      $dotGit = Join-Path $dir '.git'
      if (Test-Path -LiteralPath $dotGit -PathType Container) {
        $gitDir = $dotGit; $top = $dir; break
      }
      if (Test-Path -LiteralPath $dotGit -PathType Leaf) {
        $line = (Get-Content -LiteralPath $dotGit -TotalCount 5 | Where-Object { $_ -like 'gitdir:*' } | Select-Object -First 1)
        if ($line) {
          $target = $line.Substring(7).Trim()
          if (-not [System.IO.Path]::IsPathRooted($target)) { $target = Join-Path $dir $target }
          $gitDir = $target; $top = $dir
        }
        break
      }
      $parent = Split-Path -Parent $dir
      if ($parent -eq $dir) { break }
      $dir = $parent
    }
    if (-not $gitDir) { return $info }
    $common = $gitDir
    $commonFile = Join-Path $gitDir 'commondir'
    if (Test-Path -LiteralPath $commonFile -PathType Leaf) {
      $c = (Get-Content -LiteralPath $commonFile -TotalCount 1).Trim()
      if ([System.IO.Path]::IsPathRooted($c)) { $common = $c } else { $common = Join-Path $gitDir $c }
    }
    $config = Join-Path $common 'config'
    $url = $null
    if (Test-Path -LiteralPath $config -PathType Leaf) {
      $inOrigin = $false
      foreach ($raw in (Get-Content -LiteralPath $config)) {
        $l = $raw.Trim()
        if ($l.StartsWith('[')) { $inOrigin = ($l.TrimStart('[').TrimEnd(']').Trim() -eq 'remote "origin"'); continue }
        if ($inOrigin -and $l.Contains('=')) {
          $key = $l.Substring(0, $l.IndexOf('=')).Trim()
          if ($key -ieq 'url') {
            $v = $l.Substring($l.IndexOf('=') + 1).Trim().Trim('"')
            if ($v) { $url = $v; break }
          }
        }
      }
    }
    if ($url) {
      $u = $url
      if ($u.EndsWith('/')) { $u = $u.Substring(0, $u.Length - 1) }
      if ($u.EndsWith('.git')) { $u = $u.Substring(0, $u.Length - 4) }
      $slash = $u.LastIndexOf('/')
      $head = ''
      $name = $u
      if ($slash -ge 0) { $head = $u.Substring(0, $slash); $name = $u.Substring($slash + 1) }
      $sep = $head.LastIndexOfAny([char[]]@(':', '/'))
      $owner = $head
      if ($sep -ge 0) { $owner = $head.Substring($sep + 1) }
      if ($owner) { $info.repo = $owner + '/' + $name } else { $info.repo = $name }
    } else {
      $info.repo = Split-Path -Leaf $top
    }
    $headFile = Join-Path $gitDir 'HEAD'
    if (Test-Path -LiteralPath $headFile -PathType Leaf) {
      $h = (Get-Content -LiteralPath $headFile -TotalCount 1).Trim()
      if ($h.StartsWith('ref:')) {
        $r = $h.Substring(4).Trim()
        if ($r.StartsWith('refs/heads/')) { $r = $r.Substring(11) }
        $info.branch = $r
      } elseif ($h.Length -ge 7 -and $h -match '^[0-9a-fA-F]+$') {
        $info.branch = '(detached ' + $h.Substring(0, 7) + ')'
      }
    }
  } catch { }
  return $info
}

function Get-Tilde([string]$path) {
  $home_ = $env:USERPROFILE
  if (-not $home_) { return $path }
  if ($path -ieq $home_) { return '~' }
  if ($path.StartsWith($home_ + '\', [StringComparison]::OrdinalIgnoreCase)) { return '~' + $path.Substring($home_.Length) }
  return $path
}

# --- Log (the same records as the WSL executable)

function Get-StateDir {
  $base = $env:LOCALAPPDATA
  if (-not $base) { $base = [Environment]::GetFolderPath('LocalApplicationData') }
  return (Join-Path $base 'op-reason')
}

function ConvertTo-JsonLine($obj) {
  return ($obj | ConvertTo-Json -Compress -Depth 6)
}

# Append one record under log.lock (opened for reading and writing; waits up to 5 seconds, then writes anyway) and
# rotate to log.1.jsonl at 1 MiB.
function Write-LogLine([string]$line) {
  try {
    $dir = Get-StateDir
    [void](New-Item -ItemType Directory -Force -Path $dir)
    $lock = $null
    $locked = $false
    try {
      $lock = [System.IO.File]::Open((Join-Path $dir 'log.lock'), [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::ReadWrite)
      $deadline = [DateTime]::UtcNow.AddSeconds(5)
      while (-not $locked) {
        try { $lock.Lock(0, 1); $locked = $true } catch [System.IO.IOException] {
          if ([DateTime]::UtcNow -ge $deadline) { break }
          Start-Sleep -Milliseconds 50
        }
      }
    } catch { }
    $log = Join-Path $dir 'log.jsonl'
    if ((Test-Path -LiteralPath $log) -and (Get-Item -LiteralPath $log).Length -ge 1048576) {
      Move-Item -LiteralPath $log -Destination (Join-Path $dir 'log.1.jsonl') -Force
    }
    [System.IO.File]::AppendAllText($log, $line + "`n", (New-Object System.Text.UTF8Encoding($false)))
    if ($locked) { $lock.Unlock(0, 1) }
    if ($null -ne $lock) { $lock.Dispose() }
  } catch { }
}

# --- Launcher

# Quote one argument the way Go's syscall.EscapeArg does (the launcher's rules).
function Format-WinArg([string]$s) {
  if ($s.Length -eq 0) { return '""' }
  if ($s.IndexOfAny([char[]]@([char]34, [char]92, [char]32, [char]9)) -lt 0) { return $s }
  $hasSpace = $s.IndexOfAny([char[]]@([char]32, [char]9)) -ge 0
  if ($s.IndexOfAny([char[]]@([char]34, [char]92)) -lt 0) { return '"' + $s + '"' }
  $sb = New-Object System.Text.StringBuilder
  if ($hasSpace) { [void]$sb.Append('"') }
  $slashes = 0
  foreach ($c in $s.ToCharArray()) {
    if ($c -eq [char]92) { $slashes++ }
    elseif ($c -eq [char]34) { [void]$sb.Append([char]92, $slashes + 1); $slashes = 0 }
    else { $slashes = 0 }
    [void]$sb.Append($c)
  }
  if ($hasSpace) { [void]$sb.Append([char]92, $slashes); [void]$sb.Append('"') }
  return $sb.ToString()
}

# JSON with every non-ASCII character as a \u escape (the payload travels in an environment variable).
function ConvertTo-AsciiJson([string]$json) {
  $sb = New-Object System.Text.StringBuilder
  foreach ($c in $json.ToCharArray()) {
    if ([int]$c -lt 128) { [void]$sb.Append($c) } else { [void]$sb.Append([char]92).Append('u').Append(([int]$c).ToString('x4')) }
  }
  return $sb.ToString()
}

function Find-Beside([string[]]$candidates) {
  foreach ($c in $candidates) {
    if (Test-Path -LiteralPath $c -PathType Leaf) { return (Resolve-Path -LiteralPath $c).ProviderPath }
  }
  return $null
}

# --- Main

$lang = Get-Lang
$M = Get-Messages $lang
$entryName = $env:OPREASON_ENTRY_NAME
if (-not $entryName) {
  # The name it was called by (an alias such as op counts); "&" and "." (call and dot-source) fall back to the file name
  $invoked = [string]$MyInvocation.InvocationName
  if (-not $invoked -or $invoked -eq '&' -or $invoked -eq '.') { $invoked = [string]$MyInvocation.MyCommand.Name }
  $entryName = [System.IO.Path]::GetFileNameWithoutExtension($invoked)
}
if ($entryName) { $entryName = $entryName.ToLowerInvariant() } else { $entryName = 'op-reason' }
$fromShim = ($env:OPREASON_ENTRY -eq 'cmd')
if ($fromShim) {
  $argv = Get-ShimArgs
  try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false) } catch { }
} else {
  $argv = [string[]]@($args | ForEach-Object { [string]$_ })
}
$plan = Get-Plan $entryName $argv
$masked = Get-Masked $plan.opArgs
$display = Get-Display $masked

if ($env:OPREASON_ENTRY_PLAN_ONLY -eq '1') {
  $exitForPlan = 0
  if ($plan.action -eq 'refuse' -or $plan.action -eq 'error') { $exitForPlan = 2 }
  $out = [ordered]@{
    action = $plan.action; why = $plan.why; exit = $exitForPlan; mode = $plan.mode; kind = $plan.kind
    opArgs = @($plan.opArgs); masked = @($masked); command = $display; reason = $plan.reason
    shell = $plan.shell; completionCommand = $plan.command; logArgs = @($plan.rest)
  }
  Write-Output (ConvertTo-Json -InputObject $out -Compress -Depth 4)
  exit 0
}

switch ($plan.action) {
  'usage' { Write-Output ($M.usage -f $script:Version); exit 0 }
  'version' { Write-Output ('op-reason ' + $script:Version); exit 0 }
  'skill' {
    $skill = Find-Beside @((Join-Path $PSScriptRoot 'SKILL.md'), (Join-Path $PSScriptRoot '..\skill\SKILL.md'))
    if ($null -eq $skill) { Write-Err 'SKILL.md was not found next to op-reason.ps1'; exit 127 }
    Write-Output ([System.IO.File]::ReadAllText($skill, [System.Text.Encoding]::UTF8))
    exit 0
  }
  'completion' { Write-Err $M.completion; exit 2 }
  'error' { Write-Err $plan.message; exit 2 }
  'log' { Write-Output ($M.logInfo -f (Join-Path (Get-StateDir) 'log.jsonl')); exit 0 }
}

# A call: collect the caller and the place, then refuse or run
$caller = Get-CallerInfo $lang
$cwd = $null
try {
  $loc = Get-Location
  if ($loc.Provider.Name -eq 'FileSystem') { $cwd = $loc.ProviderPath }
} catch { }
if (-not $cwd) { $cwd = [Environment]::CurrentDirectory }
$place = Get-GitPlace $cwd
$logCaller = [ordered]@{
  type       = $(if ($caller.isClaude) { 'claude' } else { 'human' })
  claude_pid = $caller.claudePid
  herdr      = $caller.herdr
  cwd        = $cwd
  repo       = $place.repo
  branch     = $place.branch
}
$now = Get-Date
$id = $now.ToString("yyyyMMdd'T'HHmmss") + '-' + $PID + '-' + (Get-Random -Minimum 0 -Maximum 32768)
$reasonLog = Get-Clip $plan.reason 500 '…'

function Get-Stamp {
  $t = [DateTimeOffset]::Now
  return @{ iso = $t.ToString("yyyy-MM-dd'T'HH:mm:sszzz"); epoch = $t.ToUnixTimeSeconds(); hms = $t.ToString('HH:mm:ss'); ymdhms = $t.ToString('yyyy-MM-dd HH:mm:ss') }
}

function Stop-Refused([string]$why, [int]$code, [string[]]$lines, [bool]$usage) {
  $st = Get-Stamp
  Write-LogLine (ConvertTo-JsonLine ([ordered]@{
        v = 1; id = $id; event = 'refused'; time = $st.iso; epoch = $st.epoch; session = $caller.session; why = $why
        caller = $logCaller; reason = $reasonLog; args = @($masked)
      }))
  if ($usage) { try { [Console]::Error.Write($M.usageShort) } catch { } }
  foreach ($l in $lines) { Write-Err $l }
  exit $code
}

if ($plan.action -eq 'refuse') {
  $line = $M.noReason
  if ($plan.refusal -eq 'empty') { $line = $M.emptyReason }
  Stop-Refused 'no-reason' 2 @($line) $true
}

$opExe = $env:OPREASON_OP_EXE
if (-not $opExe) {
  $cmd = Get-Command op.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -ne $cmd) { $opExe = $cmd.Source }
}
if (-not $opExe -or -not (Test-Path -LiteralPath $opExe -PathType Leaf)) {
  $shown = ''
  if ($opExe) { $shown = ' (' + $opExe + ')' }
  Stop-Refused 'no-op-exe' 127 @(($M.noOpExe -f $shown)) $false
}
$psExe = $env:OPREASON_POWERSHELL
if (-not $psExe) { $psExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe' }
if (-not (Test-Path -LiteralPath $psExe -PathType Leaf)) {
  $found = Get-Command $psExe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($null -eq $found) { Stop-Refused 'no-powershell' 127 @(($M.noPowerShell -f $psExe)) $false }
  $psExe = $found.Source
}
$launcher = Find-Beside @((Join-Path $PSScriptRoot 'launcher.ps1'), (Join-Path $PSScriptRoot '..\launcher\launcher.ps1'))
if ($null -eq $launcher) { Stop-Refused 'no-launcher' 127 @(($M.noLauncher -f $PSScriptRoot)) $false }

$b64Len = 0
foreach ($a in $plan.opArgs) { $b64Len += 4 * [Math]::Ceiling([System.Text.Encoding]::UTF8.GetByteCount($a) / 3) }
if ($b64Len -gt 28000) { Stop-Refused 'too-long' 2 @(($M.tooLong -f $b64Len), $M.longHint) $false }

$lockWait = 90
$n = 0
if ($env:OPREASON_LOCK_WAIT -and [int]::TryParse($env:OPREASON_LOCK_WAIT, [ref]$n) -and $n -ge 0) { $lockWait = $n }

$stateDir = Get-StateDir
$runDir = Join-Path (Join-Path $stateDir 'run') $id
$statusFile = Join-Path $runDir 'status.json'
$start = Get-Stamp
$facts = $null
if ($plan.mode -eq 'desktop') {
  $facts = [ordered]@{
    time   = $start.ymdhms
    caller = [ordered]@{ type = $logCaller.type; pid = $caller.claudePid; session = $caller.session; detail = $caller.detail }
    herdr  = $caller.herdr
    place  = [ordered]@{ repo = $place.repo; branch = $place.branch; cwd = (Get-Tilde $cwd) }
    reason = (Get-Clip $plan.reason 200 '…')
    command = (Get-Clip $display 300 '…')
    notes  = @()
  }
}
$args64 = @($plan.opArgs | ForEach-Object { [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes([string]$_)) })
$payload = [ordered]@{
  v = 1; mode = $plan.mode; op = $opExe; args64 = $args64; status = $statusFile; lang = $lang
  lock = 'held'; lockWait = $lockWait; stderr = 'scan'
}
# For tests only: OPREASON_STDERR_MODE=inherit lets op.exe write to stderr directly (no denial/timeout detection)
if ($env:OPREASON_STDERR_MODE -eq 'inherit') { $payload.stderr = 'inherit' }
if ($null -ne $facts) { $payload.facts = $facts }
$payloadJson = ConvertTo-AsciiJson (ConvertTo-Json -InputObject $payload -Compress -Depth 6)
if ($payloadJson.Length -gt 32000) { Stop-Refused 'too-long' 2 @(($M.payloadLong -f $payloadJson.Length), $M.longHint) $false }

# Log the start, then run the launcher. Everything changed in this process (environment, console encoding) is put
# back afterwards: from PowerShell the entry runs inside the caller's session.
$saved = @{}
function Set-EnvTemp([string]$name, $value) {
  if (-not $saved.ContainsKey($name)) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
  [Environment]::SetEnvironmentVariable($name, $value, 'Process')
}
$started = $false
$ended = $false
$exitCode = 1
$savedOutEnc = $null
$savedPsOutEnc = $null
try {
  [void](New-Item -ItemType Directory -Force -Path $runDir)
  Write-LogLine (ConvertTo-JsonLine ([ordered]@{
        v = 1; id = $id; event = 'start'; time = $start.iso; epoch = $start.epoch; session = $caller.session
        mode = $plan.mode; kind = $plan.kind; lock = 'held'; caller = $logCaller; reason = $reasonLog; args = @($masked)
      }))
  $started = $true

  Set-EnvTemp 'OPREASON_PAYLOAD' $payloadJson
  Set-EnvTemp 'OPREASON_ENTRY' $null
  Set-EnvTemp 'OPREASON_ENTRY_NAME' $null
  switch ($plan.kind) {
    'service-account' { Set-EnvTemp 'OP_BIOMETRIC_UNLOCK_ENABLED' 'false' }
    'connect' { Set-EnvTemp 'OP_SERVICE_ACCOUNT_TOKEN' $null; Set-EnvTemp 'OP_BIOMETRIC_UNLOCK_ENABLED' 'false' }
    default {
      Set-EnvTemp 'OP_SERVICE_ACCOUNT_TOKEN' $null
      Set-EnvTemp 'OP_CONNECT_HOST' $null
      Set-EnvTemp 'OP_CONNECT_TOKEN' $null
    }
  }
  $psArgs = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-OutputFormat', 'Text', '-File', $launcher)
  if ($fromShim) {
    # Inherit the standard handles of the shim: binary-safe stdin and stdout
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $psExe
    $psi.Arguments = (@($psArgs | ForEach-Object { Format-WinArg $_ }) -join ' ')
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $false
    $proc = [System.Diagnostics.Process]::Start($psi)
    $proc.WaitForExit()
    $exitCode = $proc.ExitCode
  } else {
    # A native command: PowerShell passes the output through, or captures it as text (UTF-8)
    try {
      $savedOutEnc = [Console]::OutputEncoding
      [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
    } catch { $savedOutEnc = $null }
    $savedPsOutEnc = $OutputEncoding
    $OutputEncoding = New-Object System.Text.UTF8Encoding($false)
    if ($MyInvocation.ExpectingInput) {
      $input | & $psExe @psArgs
    } else {
      & $psExe @psArgs
    }
    $exitCode = $LASTEXITCODE
  }

  $status = $null
  try { $status = [System.IO.File]::ReadAllText($statusFile) | ConvertFrom-Json } catch { $status = $null }
  $result = 'launcher-failed'
  if ($null -ne $status -and $status.started -eq $true) {
    if ($exitCode -eq 0) {
      if ($plan.mode -eq 'desktop') { $result = 'approved' } else { $result = 'ok' }
    } elseif ($status.stderrMatch -eq 'denied') {
      $result = 'denied'
    } elseif ($status.stderrMatch -eq 'timeout') {
      $result = 'timeout'
    } else {
      $result = 'failed'
    }
  } elseif ($null -ne $status -and $status.error -eq 'lock-timeout') {
    $result = 'lock-timeout'
    $holder = [string]$status.holder
    if (-not $holder) { $holder = $M.unknown }
    Write-Err $M.mutexTimeout
    Write-Err ($M.waitingFor -f $holder)
    Write-Err $M.answerFirst
  }
  $endSt = Get-Stamp
  $secs = [Math]::Max(0, $endSt.epoch - $start.epoch)
  $win = $null; $toast = $null; $others = $null; $signout = $null
  if ($null -ne $status) { $win = $status.window; $toast = $status.toast; $others = $status.others; $signout = $status.signout }
  Write-LogLine (ConvertTo-JsonLine ([ordered]@{
        v = 1; id = $id; event = 'end'; time = $endSt.iso; epoch = $endSt.epoch; session = $caller.session
        result = $result; exit = $exitCode; seconds = $secs; window = $win; toast = $toast; others = $others; signout = $signout
        watch = [ordered]@{ start = $null; end = $null }
      }))
  $ended = $true
} finally {
  if ($started -and -not $ended) {
    $endSt = Get-Stamp
    Write-LogLine (ConvertTo-JsonLine ([ordered]@{
          v = 1; id = $id; event = 'end'; time = $endSt.iso; epoch = $endSt.epoch; session = $caller.session
          result = 'interrupted'; exit = 130; seconds = [Math]::Max(0, $endSt.epoch - $start.epoch); window = $null; toast = $null
          others = $null; signout = $null; watch = [ordered]@{ start = $null; end = $null }
        }))
    $exitCode = 130
  }
  foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k], 'Process') }
  if ($null -ne $savedOutEnc) { try { [Console]::OutputEncoding = $savedOutEnc } catch { } }
  if ($null -ne $savedPsOutEnc) { $OutputEncoding = $savedPsOutEnc }
  try { Remove-Item -LiteralPath $runDir -Recurse -Force -ErrorAction SilentlyContinue } catch { }
}
exit $exitCode
