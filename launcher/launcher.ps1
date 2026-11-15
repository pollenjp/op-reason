# op-reason launcher. Every op-reason call starts a fresh Windows PowerShell 5.1 running this script: the 1Password
# desktop app binds a CLI approval to the process that starts op.exe, so starting op.exe from this short-lived process
# keeps one approval from reaching later calls or other processes. From WSL every op.exe would otherwise be a child of
# the wslhost that all panes share; on Windows, one process starting op.exe twice reuses the approval too.
#
# How the entries start it (docs/superpowers/specs/2026-11-15-op-reason-design.md, section 5):
#   - the WSL binary: -EncodedCommand with a short bootstrap that reads this script from $env:OPREASON_SCRIPT (base64 of
#     UTF-8) and runs it in the same scope. This script as -EncodedCommand would exceed the 32767-character command
#     line of Windows.
#   - the PowerShell entry: -File (this file is UTF-8 with a byte order mark; without one Windows PowerShell 5.1 reads
#     it in the ANSI code page).
#
# Input: $env:OPREASON_PAYLOAD (JSON), removed from the environment before op.exe starts.
#   v         contract version (1)
#   mode      desktop | help | service | notify
#   op        Windows path of op.exe
#   args64    op's arguments, each one UTF-8 then base64 (PowerShell 7's ConvertFrom-Json turns date-like strings into
#             DateTime, so plain strings would not round-trip in the tests on pwsh)
#   status    path of the status JSON written at the end (optional)
#   lang      en | ja: the language of the window and the toast
#   lock      held | skipped (used by the named mutex; see the design document)
#   lockWait  seconds to wait for the named mutex
#   stderr    inherit (op.exe writes to this process's stderr directly)
#   facts     desktop: time, caller, herdr, place, reason, command, notes; this script builds the texts
#   notify    notify: kind, when, cleared, time
#
# Modes:
#   desktop  show the window and the toast, run op.exe, then close the CLI sessions (whoami, then signout --all only
#            when signed in: whoami does not prompt without a session). signout --all closes the sessions of other
#            processes too, so a call with lock "skipped" does not close them.
#   help     calls that need no reason (--version, --help): run op.exe without the window, then close the sessions
#   service  service account or Connect token (the desktop app is not involved): run op.exe only
#   notify   show the toast only
#
# op.exe inherits stdin, stdout and stderr of this process as they are (ProcessStartInfo without redirection, so no
# PowerShell pipeline touches the bytes). CreateNoWindow must stay false: with true the child gets no stdin.
# This script writes nothing to stdout or stderr except one "op-reason: launcher: ..." line when it cannot start op.exe.
# The bootstrap sets $ProgressPreference before anything else (the first line below repeats it for the tests), or
# PowerShell writes CLIXML progress records to stderr.
#
# Exit code: op's exit code. 127 when op.exe is missing, 1 for other failures before op.exe starts. notify exits 0.
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
}
$script:statusPath = $null
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

# Build the window and toast texts from the facts of a desktop call.
function Build-Ui($facts, [string]$lang) {
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
  return [ordered]@{
    title  = $T.title
    lines  = $lines.ToArray()
    footer = $T.footer
    others = $T.others
    toast  = $toast
  }
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
  [void]$out.Result
  return @{ code = $proc.ExitCode; err = $err.Result }
}

# Close the CLI sessions: whoami first (it does not prompt without a session), then signout --all only when signed
# in (plain signout closes only the most recently used account). signout --all closes other processes' sessions too.
function Close-Session([string]$op) {
  try {
    $who = Invoke-Quiet $op 'whoami'
    if ($who.code -ne 0) {
      $script:status.signout = 'not-signed-in'
      return
    }
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
  $proc = [System.Diagnostics.Process]::Start($psi)
  $script:status.started = $true
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

# The window: on top of other windows without taking the focus (ShowActivated), at the top right of the work area so
# that it does not cover the 1Password prompt in the middle of the screen. It asks nothing; the decision is made in
# the 1Password prompt only.
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

  $panel = New-Object System.Windows.Controls.StackPanel
  $panel.Margin = New-Object System.Windows.Thickness(14, 10, 14, 12)
  [void]$panel.Children.Add((New-TextLine ([string]$ui.title) 16 $true $null))
  foreach ($line in @($ui.lines)) {
    $tb = New-TextLine '' 0 $false $null
    $label = New-Object System.Windows.Documents.Run ([string]$line[0] + ': ')
    $label.FontWeight = [System.Windows.FontWeights]::Bold
    [void]$tb.Inlines.Add($label)
    [void]$tb.Inlines.Add((New-Object System.Windows.Documents.Run ([string]$line[1])))
    [void]$panel.Children.Add($tb)
  }
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
  $proc.WaitForExit()
  if ($null -ne $view) {
    try { $view.window.Close() } catch { }
  }
  return $proc.ExitCode
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
  if ($payload.status) { $script:statusPath = [string]$payload.status }
  # A call that did not wait for the lock (the sender of a pipe whose receiver holds it) leaves the sessions alone:
  # signout --all would close the receiver's session while the receiver's op.exe still runs. The receiver ends later
  # and its own signout --all closes this call's session too.
  $closeSessions = ([string]$payload.lock -ne 'skipped')

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
      $ui = Build-Ui $payload.facts $lang
      $script:status.ui = $ui
    }
    if (-not $op -or -not (Test-Path -LiteralPath $op -PathType Leaf)) {
      $script:status.error = 'op.exe not found: ' + $op
      [Console]::Error.WriteLine('op-reason: launcher: op.exe not found: ' + $op)
      $exitCode = 127
    } elseif ($mode -eq 'desktop') {
      $exitCode = Invoke-Desktop $op $argList $ui
      $script:status.exit = $exitCode
      if ($closeSessions) { Close-Session $op }
    } else {
      $proc = Start-Op $op $argList
      $proc.WaitForExit()
      $exitCode = $proc.ExitCode
      $script:status.exit = $exitCode
      if ($mode -eq 'help' -and $closeSessions) { Close-Session $op }
    }
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
Write-Status
exit $exitCode
