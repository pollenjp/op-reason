# op-reason launcher. An op-reason call that goes through it starts a fresh Windows PowerShell 5.1 running this script:
# the 1Password desktop app binds a CLI approval to the process that starts op.exe, so starting op.exe from this
# short-lived process keeps one approval from reaching later calls or other processes. From WSL every op.exe would
# otherwise be a child of the wslhost that all panes share; on Windows, one process starting op.exe twice reuses the
# approval too.
#
# op-reason.exe --run follows the same contract and is what calls from WSL use first; this script is the fallback where
# the exe cannot run (Smart App Control, for example), and the second half of every call through the PowerShell entry.
# Both callers start it with -File (docs/superpowers/specs/2026-11-15-op-reason-design.md, section 5, and
# 2026-11-15-wsl-through-exe-design.md): this file is UTF-8 with a byte order mark; without one Windows PowerShell 5.1
# reads it in the ANSI code page.
#
# Input: $env:OPREASON_PAYLOAD (JSON), removed from the environment before op.exe starts.
#   v         contract version (1)
#   mode      desktop | help | service | notify
#   op        Windows path of op.exe
#   args64    op's arguments, each one UTF-8 then base64 (PowerShell 7's ConvertFrom-Json turns date-like strings into
#             DateTime, so plain strings would not round-trip in the tests on pwsh)
#   status    path of the status JSON written at the end (optional)
#   lang      en | ja: the language of the window and the toast, unless one was chosen in op-reason.exe's window
#             (%LOCALAPPDATA%\op-reason\settings.json, {"v":1,"lang":"en"|"ja"}): the chosen one comes first. This
#             script only reads that file (it has no switch) and reports what it holds as status chosenLang (en, ja,
#             or null when nothing is chosen), from which the WSL executable keeps its copy
#             (docs/superpowers/specs/2026-11-15-language-toggle-design.md)
#   lock      held: take the named mutex Local\op-reason, so that calls from WSL and Windows run one at a time;
#             skipped: do not (the sender of a pipe whose receiver holds the lock), and do not close the sessions
#   lockWait  seconds to wait for the named mutex
#   stderr    inherit: op.exe writes to this process's stderr directly; scan: this script relays op.exe's stderr to
#             its own and reports whether op denied or timed out the approval (the Windows entry cannot tee stderr)
#   facts     desktop: time, caller, herdr, place, reason, command, notes, refs; this script builds the texts. refs
#             (optional): the op:// references the call reads (total, from, the first 8 in list, the call's
#             --account), shown after the command line; one whose vault and item are IDs is a link that opens the
#             item in the 1Password app (onepassword://view-item?a=<account>&v=<vault>&i=<item>), with the account ID
#             from %LOCALAPPDATA%\op-reason\account.json (docs/superpowers/specs/2026-11-17-refs-in-window-design.md)
#   notify    notify: kind, when, cleared, time
#
# Modes:
#   desktop  show the window (it never takes the focus) and the toast, run op.exe, then close the CLI sessions
#            (whoami --format json, then signout --all only when signed in: whoami does not prompt without a
#            session). signout --all closes the sessions of other processes too, so a call with lock "skipped" does
#            not close them. The account ID and the sign-in host of a whoami that succeeded are kept in account.json
#            (nothing else of its output), so that later windows can link the references.
#   help     calls that need no reason (--version, --help): run op.exe without the window, then close the sessions
#   service  service account or Connect token (the desktop app is not involved): run op.exe only
#   notify   show the toast only
#
# The lock: while holding the mutex the launcher writes %LOCALAPPDATA%\op-reason\holder.json (time, caller, reason,
# command, pid). When the wait runs out it starts nothing, writes status error "lock-timeout", lock "timeout" and the
# holder, and exits 75. An abandoned mutex (its holder died) counts as taken.
#
# op.exe inherits stdin, stdout and stderr of this process as they are (ProcessStartInfo without redirection, so no
# PowerShell pipeline touches the bytes; with stderr "scan" only stderr is redirected and relayed). CreateNoWindow must
# stay false: with true the child gets no stdin.
# This script writes nothing to stdout or stderr except one "op-reason: launcher: ..." line when it cannot start op.exe
# and one when it waits for the mutex longer than a second.
# The first line below sets $ProgressPreference before anything else, or PowerShell writes CLIXML progress records to
# stderr.
#
# Exit code: op's exit code. 127 when op.exe is missing, 75 when the mutex wait runs out, 1 for other failures before
# op.exe starts. notify exits 0.
# Write it for Windows PowerShell 5.1 (no ??, ?., ternary, pipeline chains, $IsWindows). The tests run it on pwsh 7.
$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Stop'

$script:status = [ordered]@{
  started = $false
  exit    = $null
  error   = ''
  window  = 'skipped'
  toast   = 'skipped'
  others  = 0
  signout = 'skipped'
  lock    = 'skipped'
}
$script:statusPath = $null
$script:mutex = $null
$script:onWindows = ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT)

# Texts of the window and the toast. Labels stay short: the window is 520 pixels wide.
function Get-Texts([string]$lang) {
  if ($lang -eq 'ja') {
    return @{
      title         = '1Password CLI の承認の依頼'
      time          = '時刻'
      caller        = '呼び手'
      herdr         = 'herdr'
      place         = '場所'
      reason        = '理由'
      command       = 'コマンド'
      note          = '注意'
      human         = '人 (端末)'
      humanShort    = '人'
      herdrLine     = 'workspace「{0}」 · pane {1}'
      footer        = 'この窓が出ていない 1Password の承認は拒否してください'
      others        = 'ほかにも op.exe が動いています (この窓の無いもの)。どれの承認か分からなければ拒否してください'
      toastReason   = '理由: {0}'
      sharedCleared = '打つ前に、WSL のどの pane からも使える承認が残っていたので消しました'
      lockSkipped   = 'パイプの先の op (理由「{0}」) が lock を持ったまま、この op の出力を待っています。lock を待たずに打つので、その op の窓も同時に出ています'
      clearedTitle  = '共有の 1Password CLI の承認を消しました'
      clearedBody   = 'WSL のどの pane からも使える承認が残っていたので、op.exe signout --all で消しました ({0})。窓の出ていない承認の画面を許可したか、道具が op.exe を直に打ったのかもしれません。'
      failedTitle   = '共有の 1Password CLI の承認を消せませんでした'
      failedBody    = 'WSL のどの pane からも使える承認が残っています ({0})。WSL で op.exe signout --all を打つか、1Password をロックしてください。'
      whenStart     = 'op を打つ前'
      whenEnd       = 'op を打った後'
      refs          = '読む参照 ({0} 件・{1} から)'
      refsArgs      = '引数'
      refsStdin     = '標準入力'
      refsSep       = '、'
      refsMore      = 'ほか {0} 件'
      refsHint      = '項目へのリンクは、承認が通った呼び出しを 1 回終えると付きます (そのとき、op が既定で使うアカウントの ID を覚えます)'
      holderLine    = '{0} {1} 理由「{2}」 {3}'
      unknown       = '(分からない)'
      waiting       = 'op-reason: launcher: ほかの op-reason の呼び出しの承認待ちが終わるのを待っています (最大 {0} 秒)。'
    }
  }
  return @{
    title         = '1Password CLI approval request'
    time          = 'Time'
    caller        = 'Caller'
    herdr         = 'herdr'
    place         = 'Place'
    reason        = 'Reason'
    command       = 'Command'
    note          = 'Note'
    human         = 'Human (terminal)'
    humanShort    = 'Human'
    herdrLine     = 'workspace "{0}" · pane {1}'
    footer        = 'Deny any 1Password approval prompt that comes without this window'
    others        = 'Other op.exe processes are running (without this window). If you cannot tell which approval this is, deny it'
    toastReason   = 'Reason: {0}'
    sharedCleared = 'Before this call, an approval usable from every WSL pane was left; it was cleared'
    lockSkipped   = 'The op call after this one in the pipe (reason "{0}") holds the lock and waits for this output. This call runs without the lock, so that call''s window is open at the same time'
    clearedTitle  = 'Cleared a shared 1Password CLI approval'
    clearedBody   = 'An approval usable from every WSL pane was left, so op-reason ran op.exe signout --all ({0}). Someone may have allowed a prompt without the op-reason window, or a tool ran op.exe directly.'
    failedTitle   = 'Could not clear a shared 1Password CLI approval'
    failedBody    = 'An approval usable from every WSL pane is left ({0}). Run op.exe signout --all in WSL, or lock 1Password.'
    whenStart     = 'before the call'
    whenEnd       = 'after the call'
    refs          = 'References to read ({0}, from {1})'
    refsArgs      = 'arguments'
    refsStdin     = 'stdin'
    refsSep       = ', '
    refsMore      = 'and {0} more'
    refsHint      = 'Links to the items appear once an approved call has finished (op-reason then learns the ID of the account op uses by default)'
    holderLine    = '{0} {1} reason "{2}" {3}'
    unknown       = '(unknown)'
    waiting       = 'op-reason: launcher: waiting for another op-reason call to finish its approval (up to {0} seconds).'
  }
}

# Cut text longer than $n UTF-16 units and append an ellipsis, without splitting a surrogate pair.
function Get-Clip([string]$s, [int]$n) {
  if ($null -eq $s) { return '' }
  if ($s.Length -le $n) { return $s }
  $cut = $n
  if ($cut -gt 0 -and [char]::IsHighSurrogate($s[$cut - 1])) { $cut-- }
  return $s.Substring(0, $cut) + '…'
}

# A JSON value as text. PowerShell 7 may hand back date-like strings as DateTime.
function Get-Text($v) {
  if ($null -eq $v) { return '' }
  if ($v -is [DateTime]) { return $v.ToString('yyyy-MM-dd HH:mm:ss') }
  return [string]$v
}

# The short caller (Claude Code or a person, and the herdr label) for the toast and the holder file.
function Get-CallerShort($facts, $T) {
  $short = $T.humanShort
  if ($null -ne $facts -and $null -ne $facts.caller -and (Get-Text $facts.caller.type) -eq 'claude') { $short = 'Claude Code' }
  if ($null -ne $facts -and $null -ne $facts.herdr) {
    $label = Get-Text $facts.herdr.label
    if ($label) { $short = $short + ' · ' + $label }
  }
  return $short
}

# The account copy (account.json in the state directory, design 2026-11-17-refs-in-window-design.md section 5):
# {"v":1,"accounts":[{"id":"...","host":"..."}]}, the account the last whoami reported first, at most 5. op-reason.exe
# keeps the same file by the same rules (crates/op-reason-win/src/account.rs).
function Get-AccountFile {
  return (Join-Path (Get-StateDir) 'account.json')
}

# The host of a sign-in address (https://Example.1password.com/ -> example.1password.com); '' when it is not a host.
function Get-HostOf([string]$url) {
  if ($null -eq $url) { return '' }
  $s = $url.Trim()
  $i = $s.IndexOf('://')
  if ($i -ge 0) { $s = $s.Substring($i + 3) }
  $s = ($s -split '[/?#]', 2)[0]
  $s = ($s -split ':', 2)[0]
  if ($s.Length -eq 0 -or $s.Length -gt 253 -or $s -cnotmatch '^[A-Za-z0-9.-]+\z') { return '' }
  return $s.ToLowerInvariant()
}

# The remembered accounts (ordered hashtables with id and host), in order; none when the file is missing or broken.
function Read-Accounts {
  try {
    $path = Get-AccountFile
    if (-not [System.IO.File]::Exists($path)) { return @() }
    $data = [System.IO.File]::ReadAllText($path) | ConvertFrom-Json
    $out = New-Object System.Collections.ArrayList
    foreach ($a in @($data.accounts)) {
      if ($null -eq $a) { continue }
      $id = [string]$a.id
      if ($id -cmatch '^[A-Za-z0-9]{26}\z') { [void]$out.Add([ordered]@{ id = $id; host = (Get-HostOf ([string]$a.host)) }) }
    }
    return $out.ToArray()
  } catch {
    return @()
  }
}

# The account ID for the links of a call: with $want (the call's --account), the first account whose ID, host or the
# host's first label equals it (ignoring case, https:// and a trailing /), else none; without, the first one.
function Select-Account($accounts, [string]$want) {
  $list = @($accounts)
  $w = ''
  if ($want) { $w = $want.Trim().ToLowerInvariant() }
  if (-not $w) {
    if ($list.Count -gt 0) { return $list[0].id }
    return $null
  }
  if ($w.StartsWith('https://')) { $w = $w.Substring(8) }
  $w = $w.TrimEnd('/')
  foreach ($a in $list) {
    if ($a.id -eq $w) { return $a.id }
    if ($a.host -and ($a.host -eq $w -or ($a.host -split '\.')[0] -eq $w)) { return $a.id }
  }
  return $null
}

# Keeps the account of a whoami that succeeded (its JSON output: account_uuid and the host of url, nothing else).
function Save-Account([string]$json) {
  try {
    $who = $json | ConvertFrom-Json
    $id = [string]$who.account_uuid
    if ($id -cnotmatch '^[A-Za-z0-9]{26}\z') { return }
    $list = New-Object System.Collections.ArrayList
    [void]$list.Add([ordered]@{ id = $id; host = (Get-HostOf ([string]$who.url)) })
    foreach ($a in @(Read-Accounts)) {
      if ($a.id -ne $id -and $list.Count -lt 5) { [void]$list.Add($a) }
    }
    $entries = @($list | ForEach-Object { '{"id":"' + $_.id + '","host":"' + $_.host + '"}' })
    Write-FileReplacing (Get-AccountFile) ('{"v":1,"accounts":[' + ($entries -join ',') + ']}')
  } catch { }
}

# Puts $text in place of the file: a temporary file in the same folder renamed over it (retried for a moment when
# another process has the old one open), so that a reader never sees half a file.
function Write-FileReplacing([string]$path, [string]$text) {
  $dir = Split-Path -Parent $path
  if (-not (Test-Path -LiteralPath $dir)) { [void](New-Item -ItemType Directory -Force -Path $dir) }
  $tmp = $path + '.' + $PID + '.tmp'
  [System.IO.File]::WriteAllText($tmp, $text, (New-Object System.Text.UTF8Encoding($false)))
  for ($i = 0; $i -lt 5; $i++) {
    try {
      if ([System.IO.File]::Exists($path)) { [System.IO.File]::Replace($tmp, $path, $null) } else { [System.IO.File]::Move($tmp, $path) }
      return
    } catch {
      Start-Sleep -Milliseconds 20
    }
  }
  Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
}

# The vault and item IDs of a reference whose vault and item are both IDs (26 lowercase letters or digits, as
# written), or $null.
function Get-LinkIds([string]$ref) {
  if ($ref -cmatch '^op://([a-z0-9]{26})/([a-z0-9]{26})(/|\?|\z)') { return @($Matches[1], $Matches[2]) }
  return $null
}

# The URL that opens an item in the 1Password app, built from the three IDs only, or $null.
function Get-ItemUrl([string]$account, [string]$vault, [string]$item) {
  if ($account -cmatch '^[A-Za-z0-9]{26}\z' -and $vault -cmatch '^[a-z0-9]{26}\z' -and $item -cmatch '^[a-z0-9]{26}\z') {
    return ('onepassword://view-item?a=' + $account + '&v=' + $vault + '&i=' + $item)
  }
  return $null
}

# Opens an item's URL with the handler Windows has for it (only onepassword://view-item? URLs).
function Open-ItemLink([string]$url) {
  if (-not $url -or -not $url.StartsWith('onepassword://view-item?') -or $url -match '\s') { return }
  try {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $url
    $psi.UseShellExecute = $true
    [void][System.Diagnostics.Process]::Start($psi)
  } catch { }
}

# The block of references (op-reason.exe's ref_lines in crates/op-reason-win/src/ui.rs): the head with the count and
# the sources, at most 8 references (an ID reference gets its item's URL when the account is known), "and N more",
# and the hint when a reference shown could be a link but none got one.
function Build-RefLines($refs, $T, [string]$account) {
  $from = New-Object System.Collections.ArrayList
  foreach ($src in @($refs.from)) {
    if ($null -eq $src) { continue }
    $kind = Get-Text $src.kind
    $name = Get-Text $src.name
    if ($kind -eq 'args') { [void]$from.Add($T.refsArgs) }
    elseif ($kind -eq 'stdin') { [void]$from.Add($T.refsStdin) }
    elseif (($kind -eq 'file' -or $kind -eq 'env-file') -and $name) { [void]$from.Add($name) }
  }
  $fromText = '?'
  if ($from.Count -gt 0) { $fromText = $from.ToArray() -join $T.refsSep }
  $list = @(@($refs.list) | Where-Object { $null -ne $_ } | ForEach-Object { Get-Text $_ })
  $total = 0
  if ($null -ne $refs.total) { $total = [int]$refs.total }
  if ($list.Count -gt $total) { $total = $list.Count }
  $items = New-Object System.Collections.ArrayList
  $couldLink = $false
  $linked = $false
  foreach ($text in @($list | Select-Object -First 8)) {
    $url = $null
    $ids = Get-LinkIds $text
    if ($null -ne $ids) {
      $couldLink = $true
      if ($account) { $url = Get-ItemUrl $account $ids[0] $ids[1] }
    }
    if ($url) { $linked = $true }
    [void]$items.Add([ordered]@{ text = $text; url = $url })
  }
  $more = $null
  if ($total -gt $items.Count) { $more = $T.refsMore -f ($total - $items.Count) }
  $hint = $null
  if ($couldLink -and -not $linked) { $hint = $T.refsHint }
  return [ordered]@{ head = ($T.refs -f $total, $fromText); items = $items.ToArray(); more = $more; hint = $hint }
}

# The texts as the status records them: the block of references says linked, not the URL.
function Get-StatusUi($ui) {
  if ($null -eq $ui -or -not $ui.Contains('refs')) { return $ui }
  $copy = [ordered]@{}
  foreach ($k in @($ui.Keys)) {
    if ($k -ne 'refs' -and $k -ne 'refsAt') { $copy[$k] = $ui[$k] }
  }
  $r = $ui.refs
  $copy.refs = [ordered]@{
    head  = $r.head
    items = @(@($r.items) | ForEach-Object { [ordered]@{ text = $_.text; linked = [bool]$_.url } })
    more  = $r.more
    hint  = $r.hint
  }
  return $copy
}

# Build the window and toast texts from the facts of a desktop call. $account: the account ID for the links.
function Build-Ui($facts, [string]$lang, [string]$account) {
  $T = Get-Texts $lang
  $lines = New-Object System.Collections.ArrayList
  [void]$lines.Add(@($T.time, (Get-Text $facts.time)))

  $caller = $facts.caller
  $isClaude = ($null -ne $caller -and (Get-Text $caller.type) -eq 'claude')
  if ($isClaude) {
    $callerDesc = 'Claude Code'
    if ($null -ne $caller.pid) { $callerDesc = 'Claude Code (pid ' + (Get-Text $caller.pid) + ')' }
    $short = 'Claude Code'
  } else {
    $callerDesc = $T.human
    $short = $T.humanShort
  }
  if ($null -ne $caller -and $caller.detail) { $callerDesc = $callerDesc + ' · ' + (Get-Text $caller.detail) }
  [void]$lines.Add(@($T.caller, $callerDesc))

  if ($null -ne $facts.herdr) {
    $label = Get-Text $facts.herdr.label
    $pane = Get-Text $facts.herdr.pane
    $labelShown = $label
    if (-not $labelShown) { $labelShown = '?' }
    if (-not $pane) { $pane = '?' }
    [void]$lines.Add(@($T.herdr, ($T.herdrLine -f $labelShown, $pane)))
    if ($label) { $short = $short + ' · ' + $label }
  }

  $place = Get-Text $facts.place.cwd
  if ($facts.place.repo) {
    $branch = Get-Text $facts.place.branch
    if (-not $branch) { $branch = '?' }
    $place = (Get-Text $facts.place.repo) + ' (' + $branch + ') · ' + $place
  }
  [void]$lines.Add(@($T.place, $place))
  [void]$lines.Add(@($T.reason, (Get-Text $facts.reason)))
  [void]$lines.Add(@($T.command, (Get-Text $facts.command)))
  $refsAt = $lines.Count
  $refs = $null
  if ($null -ne $facts.refs -and @(@($facts.refs.list) | Where-Object { $null -ne $_ }).Count -gt 0) {
    $refs = Build-RefLines $facts.refs $T $account
  }
  foreach ($note in @($facts.notes)) {
    if ($null -eq $note) { continue }
    $kind = Get-Text $note.kind
    if ($kind -eq 'shared-cleared') { [void]$lines.Add(@($T.note, $T.sharedCleared)) }
    elseif ($kind -eq 'lock-skipped') { [void]$lines.Add(@($T.note, ($T.lockSkipped -f (Get-Text $note.holderReason)))) }
  }

  $toast = @(
    $T.title,
    ($T.toastReason -f (Get-Clip (Get-Text $facts.reason) 100)),
    ($short + ' · ' + (Get-Clip (Get-Text $facts.command) 80))
  )
  $ui = [ordered]@{
    title  = $T.title
    lines  = $lines.ToArray()
    footer = $T.footer
    others = $T.others
    toast  = $toast
  }
  if ($null -ne $refs) {
    $ui.refs = $refs
    $ui.refsAt = $refsAt
  }
  return $ui
}

# Build the toast texts of a notify call (a shared approval found by the WSL watchdog).
function Build-NotifyUi($notify, [string]$lang) {
  $T = Get-Texts $lang
  $when = $T.whenEnd
  if ((Get-Text $notify.when) -eq 'start') { $when = $T.whenStart }
  if ($notify.cleared -eq $true) {
    $title = $T.clearedTitle
    $body = $T.clearedBody -f $when
  } else {
    $title = $T.failedTitle
    $body = $T.failedBody -f $when
  }
  return [ordered]@{ title = $title; lines = @(, @($T.time, (Get-Text $notify.time))); toast = @($title, $body) }
}

# Quote one argument the way Go's syscall.EscapeArg does, so op.exe (a Go program) and .NET split it back unchanged.
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

function Get-Short([string]$s, [int]$n) {
  $t = ($s -replace '\s+', ' ').Trim()
  if ($t.Length -gt $n) { $t = $t.Substring(0, $n) + '...' }
  return $t
}

function Write-Status {
  if (-not $script:statusPath) { return }
  $script:status.chosenLang = Get-ChosenLang
  try {
    $json = $script:status | ConvertTo-Json -Compress -Depth 6
    [System.IO.File]::WriteAllText($script:statusPath, $json, (New-Object System.Text.UTF8Encoding($false)))
  } catch { }
}

# Run op.exe with redirected I/O (whoami, signout): nothing reaches the caller's stdout or stderr.
function Invoke-Quiet([string]$exe, [string]$argLine) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $exe
  $psi.Arguments = $argLine
  $psi.UseShellExecute = $false
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.CreateNoWindow = $true
  $proc = [System.Diagnostics.Process]::Start($psi)
  $proc.StandardInput.Close()
  $out = $proc.StandardOutput.ReadToEndAsync()
  $err = $proc.StandardError.ReadToEndAsync()
  $proc.WaitForExit()
  return @{ code = $proc.ExitCode; out = $out.Result; err = $err.Result }
}

# Close the CLI sessions: whoami first (it does not prompt without a session), then signout --all only when signed
# in (plain signout closes only the most recently used account). signout --all closes other processes' sessions too.
function Close-Session([string]$op) {
  try {
    $who = Invoke-Quiet $op 'whoami --format json'
    if ($who.code -ne 0) {
      $script:status.signout = 'not-signed-in'
      return
    }
    Save-Account $who.out
    $out = Invoke-Quiet $op 'signout --all'
    if ($out.code -eq 0) { $script:status.signout = 'ok' }
    else { $script:status.signout = 'failed: exit ' + $out.code + ': ' + (Get-Short $out.err 120) }
  } catch {
    $script:status.signout = 'failed: ' + (Get-Short $_.Exception.Message 120)
  }
}

function Start-Op([string]$op, $argList) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $op
  $psi.Arguments = @($argList | ForEach-Object { Format-WinArg ([string]$_) }) -join ' '
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $false
  if ($script:scan) { $psi.RedirectStandardError = $true }
  $proc = [System.Diagnostics.Process]::Start($psi)
  $script:status.started = $true
  if ($script:scan) { Start-StderrPump $proc }
  return $proc
}

function Get-OtherCount([string]$name, [int]$selfId) {
  try {
    return @(Get-Process -Name $name -ErrorAction SilentlyContinue | Where-Object { $_.Id -ne $selfId }).Count
  } catch {
    return 0
  }
}

function Show-Toast($texts) {
  if (-not $script:onWindows) { return 'skipped' }
  try {
    $null = [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime]
    $null = [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime]
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.Append('<toast><visual><binding template="ToastGeneric">')
    foreach ($t in @($texts)) {
      [void]$sb.Append('<text>' + [System.Security.SecurityElement]::Escape([string]$t) + '</text>')
    }
    [void]$sb.Append('</binding></visual></toast>')
    $xml = New-Object Windows.Data.Xml.Dom.XmlDocument
    $xml.LoadXml($sb.ToString())
    $toast = New-Object Windows.UI.Notifications.ToastNotification $xml
    # The AppUserModelID that Windows PowerShell registers for itself (no registration of our own).
    $aumid = '{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe'
    [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier($aumid).Show($toast)
    return 'shown'
  } catch {
    return 'failed: ' + (Get-Short $_.Exception.Message 120)
  }
}

function New-TextLine([string]$text, [double]$size, [bool]$bold, $brush) {
  $tb = New-Object System.Windows.Controls.TextBlock
  $tb.Text = $text
  $tb.TextWrapping = [System.Windows.TextWrapping]::Wrap
  $tb.Margin = New-Object System.Windows.Thickness(0, 2, 0, 2)
  if ($size -gt 0) { $tb.FontSize = $size }
  if ($bold) { $tb.FontWeight = [System.Windows.FontWeights]::Bold }
  if ($null -ne $brush) { $tb.Foreground = $brush }
  return $tb
}

# The block of references after the command line: the head in bold, then each reference indented, a link as a WPF
# Hyperlink (blue, underlined, the hand cursor); "and N more" and the hint in dark gray. The Hyperlink is not focusable:
# a focusable one makes WPF call SetFocus on a click, which would activate the window.
function Add-RefLines($panel, $refs) {
  [void]$panel.Children.Add((New-TextLine ([string]$refs.head + ':') 0 $true $null))
  $blue = New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.Color]::FromRgb(0x00, 0x66, 0xCC))
  $dim = New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.Color]::FromRgb(0x55, 0x55, 0x55))
  foreach ($item in @($refs.items)) {
    $tb = New-TextLine '' 0 $false $null
    $tb.Margin = New-Object System.Windows.Thickness(12, 2, 0, 2)
    if ($item.url) {
      $link = New-Object System.Windows.Documents.Hyperlink (New-Object System.Windows.Documents.Run ([string]$item.text))
      $link.Foreground = $blue
      $link.Focusable = $false
      $link.Tag = [string]$item.url
      $link.Add_Click({ param($sender, $e) Open-ItemLink ([string]$sender.Tag) })
      [void]$tb.Inlines.Add($link)
    } else {
      $tb.Text = [string]$item.text
    }
    [void]$panel.Children.Add($tb)
  }
  foreach ($text in @($refs.more, $refs.hint)) {
    if (-not $text) { continue }
    $tb = New-TextLine ([string]$text) 0 $false $dim
    $tb.Margin = New-Object System.Windows.Thickness(12, 2, 0, 2)
    [void]$panel.Children.Add($tb)
  }
}

# Keeps clicks (on a link) from activating the window, so that the 1Password prompt keeps the focus: WPF's Popup
# answers WM_MOUSEACTIVATE with MA_NOACTIVATE in its compiled message filter, and the window takes that filter as a
# hook. A script block cannot be the hook (PowerShell does not write back the hook's ref parameter, seen on CI), and a
# WPF window cannot get WS_EX_NOACTIVATE without compiled code. When the filter is not there, a click activates the
# window, as it always did.
function Set-NoActivate($win) {
  try {
    $flags = [System.Reflection.BindingFlags]'NonPublic, Instance'
    $filter = [System.Windows.Controls.Primitives.Popup].GetMethod('PopupFilterMessage', $flags)
    if ($null -eq $filter) { return }
    $popup = New-Object System.Windows.Controls.Primitives.Popup
    $hook = [Delegate]::CreateDelegate([System.Windows.Interop.HwndSourceHook], $popup, $filter)
    $source = [System.Windows.Interop.HwndSource]::FromHwnd((New-Object System.Windows.Interop.WindowInteropHelper $win).Handle)
    $source.AddHook($hook)
  } catch { }
}

# The window: on top of other windows without taking the focus (ShowActivated, and Set-NoActivate for clicks), at the
# top right of the work area so that it does not cover the 1Password prompt in the middle of the screen. It asks
# nothing; the decision is made in the 1Password prompt only.
function New-ApprovalWindow($ui) {
  Add-Type -AssemblyName PresentationFramework
  $win = New-Object System.Windows.Window
  $win.Title = [string]$ui.title
  $win.Width = 520
  $win.SizeToContent = [System.Windows.SizeToContent]::Height
  $win.WindowStyle = [System.Windows.WindowStyle]::ToolWindow
  $win.ResizeMode = [System.Windows.ResizeMode]::NoResize
  $win.Topmost = $true
  $win.ShowActivated = $false
  $win.ShowInTaskbar = $false
  $win.WindowStartupLocation = [System.Windows.WindowStartupLocation]::Manual
  $area = [System.Windows.SystemParameters]::WorkArea
  $win.Left = $area.Right - $win.Width - 16
  $win.Top = $area.Top + 16
  $win.Background = [System.Windows.Media.Brushes]::LightYellow
  $win.Add_SourceInitialized({ param($sender, $e) Set-NoActivate $sender })

  $panel = New-Object System.Windows.Controls.StackPanel
  $panel.Margin = New-Object System.Windows.Thickness(14, 10, 14, 12)
  [void]$panel.Children.Add((New-TextLine ([string]$ui.title) 16 $true $null))
  $refsAt = -1
  if ($ui.Contains('refs')) { $refsAt = [int]$ui.refsAt }
  $k = 0
  foreach ($line in @($ui.lines)) {
    if ($k -eq $refsAt) { Add-RefLines $panel $ui.refs }
    $k++
    $tb = New-TextLine '' 0 $false $null
    $label = New-Object System.Windows.Documents.Run ([string]$line[0] + ': ')
    $label.FontWeight = [System.Windows.FontWeights]::Bold
    [void]$tb.Inlines.Add($label)
    [void]$tb.Inlines.Add((New-Object System.Windows.Documents.Run ([string]$line[1])))
    [void]$panel.Children.Add($tb)
  }
  if ($refsAt -ge $k) { Add-RefLines $panel $ui.refs }
  $others = New-TextLine ([string]$ui.others) 0 $true ([System.Windows.Media.Brushes]::DarkOrange)
  $others.Visibility = [System.Windows.Visibility]::Collapsed
  [void]$panel.Children.Add($others)
  [void]$panel.Children.Add((New-TextLine ([string]$ui.footer) 0 $true ([System.Windows.Media.Brushes]::Firebrick)))
  $win.Content = $panel
  return @{ window = $win; others = $others }
}

# Keep the window responsive until op.exe exits. Every 250 ms: count op.exe processes other than ours (a prompt
# without this window may be one of them) and leave the frame when ours has exited.
function Wait-WithWindow($proc, $view, [string]$procName) {
  $script:waitProc = $proc
  $script:waitView = $view
  $script:waitName = $procName
  $script:waitFrame = New-Object System.Windows.Threading.DispatcherFrame
  $script:waitTimer = New-Object System.Windows.Threading.DispatcherTimer
  $script:waitTimer.Interval = [TimeSpan]::FromMilliseconds(250)
  $script:waitTimer.Add_Tick({
      Invoke-StderrPump
      $n = Get-OtherCount $script:waitName $script:waitProc.Id
      if ($n -gt $script:status.others) { $script:status.others = $n }
      if ($n -gt 0) { $script:waitView.others.Visibility = [System.Windows.Visibility]::Visible }
      if ($script:waitProc.HasExited) {
        $script:waitTimer.Stop()
        $script:waitFrame.Continue = $false
      }
    })
  $script:waitTimer.Start()
  [System.Windows.Threading.Dispatcher]::PushFrame($script:waitFrame)
}

function Invoke-Desktop([string]$op, $argList, $ui) {
  $view = $null
  if ($script:onWindows -and $null -ne $ui) {
    try {
      $view = New-ApprovalWindow $ui
      $procName = [System.IO.Path]::GetFileNameWithoutExtension($op)
      $n = Get-OtherCount $procName -1
      if ($n -gt 0) {
        $script:status.others = $n
        $view.others.Visibility = [System.Windows.Visibility]::Visible
      }
      $view.window.Show()
      $script:status.window = 'shown'
    } catch {
      $script:status.window = 'failed: ' + (Get-Short $_.Exception.Message 120)
      $view = $null
    }
    $script:status.toast = Show-Toast $ui.toast
  }
  $proc = Start-Op $op $argList
  if ($null -ne $view) {
    try {
      Wait-WithWindow $proc $view ([System.IO.Path]::GetFileNameWithoutExtension($op))
    } catch {
      $script:status.window = 'failed: ' + (Get-Short $_.Exception.Message 120)
    }
  }
  Wait-Plain $proc
  Wait-Stderr
  $proc.WaitForExit()
  if ($null -ne $view) {
    try { $view.window.Close() } catch { }
  }
  return $proc.ExitCode
}

# The directory of the holder file: %LOCALAPPDATA%\op-reason (the same as the Windows entry's log).
function Get-StateDir {
  $base = $env:LOCALAPPDATA
  if (-not $base) { $base = [Environment]::GetFolderPath('LocalApplicationData') }
  return (Join-Path $base 'op-reason')
}

# The language chosen in op-reason.exe's window: en or ja from settings.json in the state directory, $null when nothing
# is chosen (no file, not a JSON object, no or an unknown lang). Any v is read. Only op-reason.exe writes the file.
function Get-ChosenLang {
  try {
    $path = Join-Path (Get-StateDir) 'settings.json'
    if (-not [System.IO.File]::Exists($path)) { return $null }
    $settings = [System.IO.File]::ReadAllText($path) | ConvertFrom-Json
    if ($settings -isnot [System.Management.Automation.PSCustomObject]) { return $null }
    $chosen = $settings.lang
    if ($chosen -is [string] -and ($chosen -ceq 'en' -or $chosen -ceq 'ja')) { return $chosen }
  } catch { }
  return $null
}

# The holder of the mutex as one line in the call's language (from holder.json; "(unknown)" when it cannot be read).
function Get-HolderLine($T) {
  try {
    $h = [System.IO.File]::ReadAllText((Join-Path (Get-StateDir) 'holder.json')) | ConvertFrom-Json
    return ($T.holderLine -f (Get-Text $h.time), (Get-Text $h.caller), (Get-Text $h.reason), (Get-Text $h.command))
  } catch {
    return $T.unknown
  }
}

# Take the named mutex, waiting up to $seconds. Returns $true when taken. Says once on stderr that it waits when the
# wait passes a second. An abandoned mutex (the holder died) counts as taken.
function Enter-GlobalLock([int]$seconds, $T) {
  $script:mutex = New-Object System.Threading.Mutex($false, 'Local\op-reason')
  $started = [DateTime]::UtcNow
  $told = $false
  while ($true) {
    try {
      if ($script:mutex.WaitOne(250)) { return $true }
    } catch [System.Threading.AbandonedMutexException] {
      return $true
    }
    $waited = ([DateTime]::UtcNow - $started).TotalSeconds
    if (-not $told -and $waited -ge 1) {
      [Console]::Error.WriteLine(($T.waiting -f $seconds))
      $told = $true
    }
    if ($waited -ge $seconds) {
      $script:mutex.Dispose()
      $script:mutex = $null
      return $false
    }
  }
}

# Write holder.json for the call that holds the mutex.
function Write-Holder($facts, $T, [string]$mode) {
  try {
    $dir = Get-StateDir
    [void](New-Item -ItemType Directory -Force -Path $dir)
    $reason = '(no reason: help)'
    if ($mode -ne 'help' -and $null -ne $facts) { $reason = Get-Clip (Get-Text $facts.reason) 100 }
    $command = ''
    if ($null -ne $facts) { $command = Get-Clip (Get-Text $facts.command) 100 }
    $holder = [ordered]@{
      time    = (Get-Date).ToString('HH:mm:ss')
      caller  = (Get-CallerShort $facts $T)
      reason  = $reason
      command = $command
      pid     = $PID
    }
    [System.IO.File]::WriteAllText((Join-Path $dir 'holder.json'), ($holder | ConvertTo-Json -Compress), (New-Object System.Text.UTF8Encoding($false)))
  } catch { }
}

# Release the mutex and remove holder.json when it is ours.
function Exit-GlobalLock {
  if ($null -eq $script:mutex) { return }
  try {
    $path = Join-Path (Get-StateDir) 'holder.json'
    $h = [System.IO.File]::ReadAllText($path) | ConvertFrom-Json
    if ([int]$h.pid -eq $PID) { Remove-Item -LiteralPath $path -Force }
  } catch { }
  try { $script:mutex.ReleaseMutex() } catch { }
  $script:mutex.Dispose()
  $script:mutex = $null
}

# stderr "scan": relay op.exe's stderr to ours while keeping its tail. Pump-Stderr copies what has arrived so far
# without blocking; Wait-Stderr drains the rest after op.exe has exited.
function Start-StderrPump($proc) {
  $script:errStream = $proc.StandardError.BaseStream
  $script:errOut = [Console]::OpenStandardError()
  $script:errBuf = New-Object byte[] 8192
  $script:errTail = New-Object System.IO.MemoryStream
  $script:errTask = $script:errStream.ReadAsync($script:errBuf, 0, $script:errBuf.Length)
}

function Invoke-StderrPump {
  while ($null -ne $script:errTask -and $script:errTask.IsCompleted) {
    $n = 0
    try { $n = $script:errTask.Result } catch { $n = 0 }
    if ($n -le 0) {
      $script:errTask = $null
      break
    }
    try {
      $script:errOut.Write($script:errBuf, 0, $n)
      $script:errOut.Flush()
    } catch { }
    $script:errTail.Write($script:errBuf, 0, $n)
    if ($script:errTail.Length -gt 524288) {
      $keep = $script:errTail.ToArray()
      $script:errTail = New-Object System.IO.MemoryStream
      $script:errTail.Write($keep, $keep.Length - 262144, 262144)
    }
    $script:errTask = $script:errStream.ReadAsync($script:errBuf, 0, $script:errBuf.Length)
  }
}

function Wait-Stderr {
  while ($null -ne $script:errTask) {
    try { [void]$script:errTask.Wait(100) } catch { }
    Invoke-StderrPump
  }
}

# What op's own "[ERROR] ..." lines say about the approval: denied, timeout, or $null.
function Get-StderrMatch {
  if ($null -eq $script:errTail) { return $null }
  $text = [System.Text.Encoding]::UTF8.GetString($script:errTail.ToArray())
  $timeout = $false
  foreach ($line in ($text -split "`n")) {
    $l = $line.ToLowerInvariant()
    if (-not $l.StartsWith('[error] ')) { continue }
    if ($l.Contains('authorization prompt dismissed')) { return 'denied' }
    $at = $l.IndexOf('authoriz')
    if ($at -ge 0) {
      $after = $l.Substring($at + 8)
      if ($after.Contains('timeout') -or $after.Contains('timed out')) { $timeout = $true }
    }
  }
  if ($timeout) { return 'timeout' }
  return $null
}

# Wait for op.exe without a window (pumping stderr in scan mode).
function Wait-Plain($proc) {
  if ($null -eq $script:errTask) {
    $proc.WaitForExit()
    return
  }
  while (-not $proc.HasExited) {
    Invoke-StderrPump
    Start-Sleep -Milliseconds 50
  }
}

# PowerShell 7.5 and later can keep date-like strings as strings; Windows PowerShell 5.1 never converts them.
function ConvertFrom-Payload([string]$raw) {
  $cmd = Get-Command ConvertFrom-Json
  if ($cmd.Parameters.ContainsKey('DateKind')) { return ($raw | ConvertFrom-Json -DateKind String) }
  return ($raw | ConvertFrom-Json)
}

$exitCode = 1
try {
  $raw = $env:OPREASON_PAYLOAD
  Remove-Item Env:OPREASON_PAYLOAD -ErrorAction SilentlyContinue
  if (-not $raw) { throw 'OPREASON_PAYLOAD is empty' }
  $payload = ConvertFrom-Payload $raw
  $mode = [string]$payload.mode
  $lang = [string]$payload.lang
  $chosen = Get-ChosenLang
  if ($chosen) { $lang = $chosen }
  if ($payload.status) { $script:statusPath = [string]$payload.status }
  # A call that did not wait for the lock (the sender of a pipe whose receiver holds it) leaves the sessions alone:
  # signout --all would close the receiver's session while the receiver's op.exe still runs. The receiver ends later
  # and its own signout --all closes this call's session too.
  $closeSessions = ([string]$payload.lock -ne 'skipped')
  $script:scan = ([string]$payload.stderr -eq 'scan')
  $lockWait = 90
  if ($null -ne $payload.lockWait) { $lockWait = [int]$payload.lockWait }

  if ($mode -eq 'notify') {
    if ($null -ne $payload.notify) {
      $ui = Build-NotifyUi $payload.notify $lang
      $script:status.ui = $ui
      $script:status.toast = Show-Toast $ui.toast
    }
    $exitCode = 0
  } elseif (@('desktop', 'help', 'service') -notcontains $mode) {
    throw ('unknown mode: ' + $mode)
  } else {
    $op = [string]$payload.op
    $argList = @()
    foreach ($a in @($payload.args64)) {
      if ($null -ne $a) { $argList += [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String([string]$a)) }
    }
    $ui = $null
    if ($mode -eq 'desktop' -and $null -ne $payload.facts) {
      $account = $null
      if ($null -ne $payload.facts.refs) { $account = Select-Account @(Read-Accounts) ([string]$payload.facts.refs.account) }
      $ui = Build-Ui $payload.facts $lang $account
      $script:status.ui = Get-StatusUi $ui
    }
    $T = Get-Texts $lang
    $locked = $true
    if ($closeSessions) {
      $locked = Enter-GlobalLock $lockWait $T
      if ($locked) {
        $script:status.lock = 'held'
        Write-Holder $payload.facts $T $mode
      }
    }
    if (-not $locked) {
      $script:status.error = 'lock-timeout'
      $script:status.lock = 'timeout'
      $script:status.holder = Get-HolderLine $T
      $exitCode = 75
    } elseif (-not $op -or -not (Test-Path -LiteralPath $op -PathType Leaf)) {
      $script:status.error = 'op.exe not found: ' + $op
      [Console]::Error.WriteLine('op-reason: launcher: op.exe not found: ' + $op)
      $exitCode = 127
    } elseif ($mode -eq 'desktop') {
      $exitCode = Invoke-Desktop $op $argList $ui
      $script:status.exit = $exitCode
      if ($closeSessions) { Close-Session $op }
    } else {
      $proc = Start-Op $op $argList
      Wait-Plain $proc
      Wait-Stderr
      $proc.WaitForExit()
      $exitCode = $proc.ExitCode
      $script:status.exit = $exitCode
      if ($mode -eq 'help' -and $closeSessions) { Close-Session $op }
    }
    if ($script:scan -and $script:status.started) { $script:status.stderrMatch = Get-StderrMatch }
  }
} catch {
  $script:status.error = Get-Short $_.Exception.Message 300
  if ($script:status.started) {
    if ($null -ne $script:status.exit) { $exitCode = [int]$script:status.exit }
  } else {
    [Console]::Error.WriteLine('op-reason: launcher: ' + $script:status.error)
    $exitCode = 1
  }
}
Exit-GlobalLock
Write-Status
exit $exitCode
