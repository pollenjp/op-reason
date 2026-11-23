# op-reason: settings from the command line and a window — design

- Date: 2026-11-23
- Status: approved (the user chose the approach, the entries, the controls, the scope and the split into two pull
  requests; this document records them, the design and the implementer's decisions). The first pull request brings the
  layer, the command line and this document with its plan; the second the window, the Start menu entry and the icon in
  the notification area.
- Builds on: `2026-11-15-language-toggle-design.md` (`settings.json`), `2026-11-17-ssh-relay-design.md` (§8 the relay
  process, §12 the dotfiles), `2026-11-22-front-after-allow-design.md` (a relay started from a scheduled task),
  `2026-11-17-refs-in-window-design.md` (`account.json`) and `2026-11-15-windows-exe-design.md` (§7 the windows).

## 1. What and why

op-reason has grown things to set up by hand: the scheduled task that keeps the SSH relay running, the relay's mode and
times, the language, and the guide for AI agents (`op-reason --skill`), which agents find only when it is copied into
their skills folder. Each is done today with a different tool: PowerShell for the task (the README's "Starting it"),
an option on the relay's command line for its mode, a click in a call's window for the language, a copy by hand for
the skill. The author's dotfiles script does the task, the mode and the relay's start and stop itself.

The user asked to set these from a window, and to define first a common way to set them, so that the command line and
the window are two faces of it and the dotfiles only call the command line.

## 2. Goal, non-goals and anti-goals

Goal:

- **One list of settings** (§4) where every setting has the same three operations: read the value now, make it so, and
  compare a wanted value with the value now. Two kinds: values that op-reason reads itself, kept in `settings.json`,
  and things that live where Windows, Task Scheduler or Claude Code read them (a scheduled task, the running relay, a
  shortcut, a skill file), read from there and changed there.
- **A command line** on that list: `op-reason --settings list|get|set|check …` (§7), with JSON and exit codes that tell
  "the same", "different" and "failed", so that a script can check and then apply, and running `set` again changes
  nothing.
- **A window** (`op-reason --settings` without more arguments, §11) built from Windows' standard controls, which opens
  from the Start menu (§13), from a double click on `op-reason.exe`, and from an **icon in the notification area** that
  the SSH relay shows (§12).
- **op-reason's own scheduled task for the relay** (§9): started at logon and again within a minute when it stops, as
  the README recommends, made, compared, turned off and removed by op-reason. The relay reads its mode and times from
  `settings.json`. Starting, stopping and restarting the relay go through that task.
- **The skill placed where agents read it** (§10): Claude Code's skills folder on Windows, in each WSL distribution,
  and folders the user chooses. op-reason marks its copies with its version, offers an update when they are older, and
  removes only what it placed.

Non-goals:

- Registering op-reason as the sender of its notifications (its own AppUserModelID, `2026-11-15-windows-exe-design.md`
  §7.4): a change of its own.
- Installing or updating op-reason itself (the dotfiles and the README do that), and settings of the WSL executable of
  its own: on WSL, `op-reason --settings` runs `op-reason.exe --settings` (§7.5).
- Settings that only environment variables carry today (`OPREASON_LOCK_WAIT`, `OPREASON_RUNNER`, …): they stay
  environment variables.
- A Linux version of the window or of the task.

Anti-goals (what would make it a failure even if it works):

- The relay stopped or restarted while its window waits for a click or 1Password asks: the request in progress would
  fail. Every stop and restart takes op-reason's lock first (§9.4).
- A task, a relay or a skill file that op-reason did not make, changed or removed by it: another task that starts a
  relay is shown, never touched; a `SKILL.md` without op-reason's mark is never replaced or removed (§14).
- Writing through a link: a skills folder that is a junction or a symbolic link (home-manager makes `~/.claude/skills`
  one) is reported and left alone.
- Settings that an agent changes without anyone seeing it: every change is recorded in the log with its caller, and the
  skill tells agents not to change settings unless the user asks (§15).
- Breaking what the dotfiles read today: `op-reason.exe --ssh-agent --status` and the relay's options keep their output
  and meaning; options given to the relay still win over `settings.json`.

## 3. Approaches

| Question | Considered | Chosen, and why |
| --- | --- | --- |
| The common way to set things | (a) one shared settings file; (b) a layer with read, make-so and compare per setting, over a file and over the OS; (c) each face does its own | **(b)** (the user's choice of order: define the layer, then the command line and the window). A file alone cannot carry the task, the shortcut or the skill files, which Windows and Claude Code read elsewhere; (c) would let the faces drift |
| Where the layer lives | (a) all in `op-reason-win`; (b) the list, the values, the file, the comparison and the decisions in `op-reason-core`, the operating system behind a trait that `op-reason-win` implements | **(b)**: what to stop, start, write or refuse is decided in portable code and tested on Linux with a fake system; Windows CI tests the thin calls |
| Entries | window, notification area icon, Start menu | **all three** (the user's choice B) |
| Controls of the window | Windows' standard controls; Direct2D like the call windows | **standard controls** (the user's choice A), made as in the author's `type-clipboard`: comctl32 v6, Per-Monitor V2 |
| Pull requests | one; two | **two** (the user's choice A): the layer and the command line, then the window, the Start menu and the icon |
| Scope | core only; core and the skill in WSL distributions | **core and WSL distributions** (the user's choice C) |
| Registering the task | (a) Task Scheduler's COM API with an XML definition; (b) `schtasks.exe`; (c) PowerShell's ScheduledTasks module | **(a)**: no localized output to parse and no quoting through a shell; (c) starts PowerShell for every read, and its module was missing in Windows PowerShell 5.1 started from PowerShell 7 (seen by the dotfiles) |
| What "stop" means with a trigger that repeats every minute | (a) end the process (the task starts it again within a minute); (b) turn the task off until a start; (c) stopped until the next logon | **(b)** (§9.3): (a) is a restart; (c) needs op-reason to change its task at each logon, or a relay that exits at once every minute while stopped, and every start of the console program flashes a console window |
| Marking op-reason's copy of the skill | (a) a line in the file, after the front matter; (b) a file next to it | **(a)**: the copy carries its own mark; (b) can be left behind or lost when the folder is copied |
| The skill in a WSL distribution | (a) `wsl.exe -d <name> --exec sh -c <script>`; (b) the distribution's own `op-reason`; (c) the `\\wsl.localhost\<name>` share | **(a)**: works without op-reason in the distribution; (b) may be missing or older; (c) still needs the home directory from inside, and links read through the share are not reliable |
| Opening the window without a console window | (a) `op-reason.exe` starts a copy of itself without a console and exits; (b) a second, GUI-subsystem exe; (c) free the console and stay | **(a)** (§11.1): one exe in the zip; (b) doubles the size and adds an unsigned exe for Smart App Control to judge; (c) keeps a terminal waiting until the window closes |

## 4. The settings

| Name | Kind | Values | Default | Where |
| --- | --- | --- | --- | --- |
| `lang` | file | `en`, `ja`, `default` | `default`: `OPREASON_LANG`, else English | `settings.json` `lang` |
| `relay.mode` | file | `per-call`, `show-only` | `per-call` | `settings.json` `relay.mode` |
| `relay.time-limit` | file | `<n>[s\|m\|h]`, a bare number is minutes | `60m` | `settings.json` `relay.timeLimit` (seconds) |
| `relay.answer-wait` | file | `<n>[s\|m]`, a bare number is seconds | `60s` | `settings.json` `relay.answerWait` (seconds) |
| `relay.tray-icon` | file | `on`, `off` | `on` | `settings.json` `relay.trayIcon` (second pull request) |
| `relay.autostart` | system | `on`, `off` | `off` | op-reason's scheduled task (§9) |
| `relay.running` | system | `on`, `off` | none | the process that answers on `\\.\pipe\op-reason-ssh-agent`; op-reason's task turned on or off |
| `skill.windows` | system | `on`, `off` | `off` | `%USERPROFILE%\.claude\skills\op-reason\SKILL.md` |
| `skill.wsl.<distribution>` | system | `on`, `off` | `off` | `~/.claude/skills/op-reason/SKILL.md` of the distribution's default user |
| `skill.folder.<folder>` | system | `on`, `off` | `off` | `<folder>\op-reason\SKILL.md`; the folder is remembered in `settings.json` `skill.folders` |
| `start-menu` | system | `on`, `off` | `off` | `%APPDATA%\Microsoft\Windows\Start Menu\Programs\op-reason.lnk` (second pull request) |

Two more lines in `list` and the window are information with an action, not settings: the folder of the records
(`%LOCALAPPDATA%\op-reason`, opened by `open-records`) and the number of accounts remembered for the links of
references (`account.json`, forgotten by `forget-accounts`).

- A file setting that is missing from `settings.json` has its default. `set <name> default` removes it.
- A value in `settings.json` that op-reason does not accept is ignored (the default counts) and `list` says so; the
  relay starts with the default rather than not at all.
- `relay.running` is a state rather than a choice, but it has the same three operations, so that a script can ask for
  a running relay in the same `check` and `set` as the rest. `relay start|stop|restart` are its words for people
  (§7.1).
- `skill.folder.<folder>` takes an absolute folder; the names of folders compare without regard to case. A folder in a
  WSL distribution's share (`\\wsl.localhost\…`, `\\wsl$\…`) is refused: links cannot be told through the share
  (§3), and `skill.wsl.<distribution>` is the place for a distribution's own skills folder.
  `skill.wsl.<distribution>` takes a name of `wsl.exe --list` (letters, digits, `.`, `_`, `-`).

## 5. The layer

`op-reason-core` gets the portable part, `op-reason-win` the Windows part:

- **The list** (`settings::catalog`): the names, kinds, values, defaults and where each is kept; parsing and checking a
  value; the text of a value as `get` prints it (time limits in minutes when they are whole minutes, answer waits in
  seconds).
- **The file** (`settings::file`): `settings.json` read into values and written back with other fields kept (§6).
- **The decisions** (`settings::engine`): `list`, `get`, `set` and `check` over a trait, `System`, that reads and
  changes what lives outside the file: op-reason's task and the other tasks that run a relay, the relay that answers on
  the pipe and the values it started with (§8.2), op-reason's lock, starting, stopping and waiting for the relay, the
  skill places, the WSL distributions, the shortcut, the records. The engine decides what to do from what the trait
  reads (for example: stop the relay only when op-reason's task runs it, and only with the lock), so that every
  decision is tested on Linux with a fake `System`.
- **The task** (`settings::task`): the XML definition of op-reason's task, and the shape that a task read back from Task
  Scheduler must have, compared field by field (§9.1).
- **The skill** (`settings::skill`): the mark, the states of a place, placing and removing in a local folder with the
  checks for links, and the shell script that does the same inside a WSL distribution (§10).
- **The texts** of the command line in English and Japanese, and the JSON.

`op-reason-win` implements `System` with Task Scheduler's COM API, the relay's pipe, named objects, `wsl.exe` and the
shell, and runs the command line and the window on the engine. The WSL executable passes `--settings` on to
`op-reason.exe` (§7.5).

## 6. `settings.json`

```json
{"v":1,"lang":"ja","relay":{"mode":"per-call","timeLimit":3600,"answerWait":60,"trayIcon":true},"skill":{"folders":["D:\\agents\\skills"]}}
```

- The format stays version 1: the new fields are optional, and the readers of 0.6.0 and before (the exe, the launcher,
  the PowerShell entry) read `lang` only and keep the fields they do not know when they write.
- Writers read the file, change their fields and put the new file in place through a temporary file renamed over it,
  as today. On Windows they hold the named mutex `Local\op-reason-settings` while they do, so that a click on the
  language in a call's window and a change in the settings window do not undo each other.
- Durations are whole seconds above zero. `relay.trayIcon` is a boolean. `skill.folders` is a list of absolute folders.

## 7. The command line

### 7.1 Words

```text
op-reason --settings                              the window (§11; before the second pull request, this usage)
op-reason --settings help                         this usage
op-reason --settings list [--json]                every setting: its value now, its default, where it is kept
op-reason --settings get <name> [--json]          the value of one setting now
op-reason --settings set <name> <value>           make it so; also <name>=<value> …, several at once
op-reason --settings check <name>=<value> … [--json]   compare wanted values with the values now
op-reason --settings relay start|stop|restart     set relay.running on / off, or stop and start it
op-reason --settings forget-accounts              forget the accounts remembered for links (account.json)
op-reason --settings open-records                 open the folder of the records in Explorer
```

- `--settings` is a wrapper option like `--skill`: the first argument only, under any name (`op` too). op has no
  global flag of that name (`--account`, `--cache`, `--config`, `--debug`, `--encoding`, `--format`, `--help`,
  `--iso-timestamps`, `--no-color`, `--session`, `--version`, op 2.41.1); `--config` would have taken op's.
- `set` applies the values in a fixed order: the file settings in one write, then `relay.autostart`, `relay.running`
  and a restart of the relay when its file values changed (§9.2), then the skill places, then `start-menu`.
- The text output follows the language (`lang`, then `OPREASON_LANG`); names, values and JSON are the same in both.

### 7.2 Output

`list` prints one line per setting: name, value now, default, and where it is kept or what it is now (the relay's
process ID and who started it, a skill's version, why a value is not what it should be). A distribution that is not
running is listed as not checked: `list` never starts one. Naming a distribution in `get`, `set` or `check` lets
`wsl.exe` start it.

`get` prints the value alone (`on`, `per-call`, `60m`); a value that could not be read prints nothing and a line on
stderr. For the relay's file settings the value is the one in `settings.json` (what the relay starts with); when the
running relay's own value is known (§8.2) and differs, `list` and `get --json` say so (`relay-differs`).

`check` prints one line per wanted value: `same` or `different`, with the value now and why (for example
`relay.mode: different: the running relay (pid 4242) uses show-only: restart it`). A relay file setting is the same when
`settings.json` holds the wanted value and the running relay, when its values are known, runs with it; a relay of
0.6.0, which does not tell its values, does not make it different.

`--json` (`list`, `get`, `check`):

```json
{"v":1,"same":false,"settings":[{"name":"relay.mode","kind":"file","wanted":"per-call","value":"per-call","same":false,"state":"relay-differs","detail":{"relay":"show-only","pid":4242},"where":"C:\\Users\\<you>\\AppData\\Local\\op-reason\\settings.json"}]}
```

`state` is a code (`ok`, `missing`, `relay-differs`, `task-differs`, `task-off`, `other-task`, `old`, `changed`,
`foreign`, `link`, `not-running`, `unreadable`, …); `same` and `wanted` appear in `check` only. Paths are Windows
paths, also when the command came from WSL.

### 7.3 Exit codes

| Command | 0 | 1 | 2 |
| --- | --- | --- | --- |
| `check` | every value is as wanted | some value differs | a usage mistake, or a value could not be read |
| `set`, `relay …`, `forget-accounts`, `open-records` | done: every value is now as asked | something could not be done (each says why; the rest was done) | a usage mistake: nothing was done |
| `get` | read | — | a usage mistake, or the value could not be read |
| `list`, `help` | read (a value that could not be read says so on its line) | — | a usage mistake |

`check` follows `diff` and `cmp` (0 same, 1 different, 2 trouble), so that `check` and then `set` replaces a script's
own comparison.

### 7.4 Records

Every `set` and relay action writes a `settings` record to `log.jsonl` (§15): which values changed from what to what,
whether each was done, and the caller (a person or Claude Code, as for `op` calls).

### 7.5 From WSL, and from the PowerShell entry

- On WSL, `op-reason --settings …` runs `op-reason.exe --settings …` (found as for calls: `OPREASON_EXE`, else `PATH`)
  with the same arguments, the terminal's input and output, and returns its exit code; 127 when it cannot be started.
  A folder given as a WSL path (`skill.folder./mnt/d/agents/skills`) is turned into a Windows path with `wslpath -w`;
  one inside the distribution becomes a `\\wsl.localhost\…` path, which `op-reason.exe` refuses (§4).
  It passes the caller's kind (Claude Code or a person) in `OPREASON_SETTINGS_CALLER` through `WSLENV`, for the record.
  After a `set` of `lang` that succeeded, it updates its own copy of the language (`lang.json`).
- The PowerShell entry (the fallback where Smart App Control blocks `op-reason.exe`) knows `--settings` as a wrapper
  option and says that the settings come from `op-reason.exe` (exit 2). The call table `tests/cases/calls.json` keeps
  the two in step.

## 8. The relay and its settings

### 8.1 Reading them

`op-reason.exe --ssh-agent` starts with the values of `settings.json` (`relay.mode`, `relay.timeLimit`,
`relay.answerWait`; invalid ones ignored) and then its options, which win: `--ssh-agent --mode show-only` stays
show-only whatever the file says. The values are read once, at start: a change applies when the relay starts again,
which drops what it remembered and makes 1Password ask once more (1Password approves each run of the relay). The tray
icon setting (second pull request) applies at once (§12).

### 8.2 What it tells

- At start, once its pipe is made, the relay writes `%LOCALAPPDATA%\op-reason\relay.json`: its process ID, its exe, its
  pipe, the mode and times it runs with, and its arguments. It removes the file when it exits by itself. A reader trusts
  the file only when its process ID is the one that answers on the pipe (`GetNamedPipeServerProcessId`); a relay of
  0.6.0 writes none, and its values are then unknown.
- The relay waits on a named event, `Local\op-reason-ssh-relay-stop-<pid>`. When it is set, the relay fails new
  requests at once (`SSH_AGENT_FAILURE`), lets the requests that are already with 1Password finish (at most 25 seconds:
  an unlock prompt may be up), records `ssh-relay` `stop`, removes `relay.json` and exits with 0. op-reason sets it only
  while it holds the lock (§9.4), so no window of the relay is open then; the requests that the lock does not cover are
  those passing on an earlier answer and those that wait for 1Password without a window (`2026-11-17-ssh-relay-design.md`
  §5.5), and the wait lets them finish.

`--status`, its output and its exit codes do not change.

## 9. op-reason's scheduled task

### 9.1 Its name and shape

- **Name**: `\op-reason-ssh-relay-<SID>` in the root folder, `<SID>` the user's security identifier (as per-user tasks
  of other programs are named): a user without administrator rights may make tasks in the root folder but not new
  folders, and task names are shared by the users of a machine. It does not meet the dotfiles' `\pjp\…`.
- **Shape** (the README's recommendation, the same as the dotfiles' task): the action `<op-reason.exe> --ssh-agent`,
  the exe being the one that ran `set`; a trigger at the logon of this user, and a trigger from the time it was made
  that repeats every minute without end; the user's interactive token, least privilege; no time limit; on batteries
  too, not stopped when going on batteries; "Do not start a new instance" (`IgnoreNew`); hidden; priority 5; no restart
  on failure. Its description says that op-reason made it and how to change it.
- It is registered from an XML definition through `ITaskService` (`RegisterTask`, create or update, the interactive
  token, no password) and read back through the task's definition object. `relay.autostart` is `on` when the task is
  there; its shape is compared field by field: the exe (without regard to case), the arguments, the triggers (the start
  time of the repeating trigger is not compared: it changes every time the task is made), the principal, the time
  limit, the rule for instances, the batteries, hidden, the priority. A difference (most often an exe that moved)
  shows as `task-differs` with the fields, and `set relay.autostart on` makes the task again.

### 9.2 Making it so

| Wanted | What op-reason does |
| --- | --- |
| `relay.autostart on` | makes the task when it is missing, and then starts the relay when none answers; makes it again when its shape differs, and when op-reason's task runs the relay, stops it with the lock and starts it again, so that the new shape (the new exe) runs. A task that is there and right is left as it is, also when it is turned off. While a relay that op-reason's task did not start answers (another task's, a terminal's), the task is made turned off and the note says how to start it later: turned on, it would start `op-reason.exe` every minute, which exits at once while the other relay runs, and each start of the console program flashes a console window |
| `relay.autostart off` | stops the relay when op-reason's task runs it (with the lock), and removes the task |
| `relay.running on` | nothing when a relay answers (whoever started it); else turns op-reason's task on and runs it, and waits up to 15 seconds for the pipe to answer. Without op-reason's task: fails, saying to turn `relay.autostart` on (op-reason starts the relay from its task only, so that the relay has a live parent: `2026-11-17-ssh-relay-design.md` §8 and §15) |
| `relay.running off` | turns op-reason's task off and stops the relay when op-reason's task runs it (with the lock); turns the task off when no relay answers; when the relay that answers was started otherwise (another task, a terminal), turns op-reason's task off and fails for that relay, which it never stops |
| `relay restart` | with the lock: stops the relay of op-reason's task and runs the task, waits for the pipe |
| a relay file value in `set` | after the file is written: when op-reason's task runs the relay and the relay's values (§8.2) differ from the file's, restarts it, whether or not this `set` changed the file (so that `check` and then `set` end the same); when its values are unknown (a relay of 0.6.0), restarts it only when this `set` changed the file; when another task or a terminal started the relay and its values differ, reports it (exit 1) |

### 9.3 Stop

A stop turns op-reason's task off and ends the relay: it stays stopped, also after the next logon, until `relay start`
(or `set relay.running on`, or `set relay.autostart on` after `off`). The window and the menu of the icon say so. A
task turned off is not a difference of `relay.autostart` (the task is there and right) but of `relay.running`
(`task-off`) when no relay answers: a script that wants the relay running finds it in its `check`. `relay.running on` is
the same as long as some relay answers on the pipe. While op-reason's task is on and another relay answers, `list` and
`check` warn on `relay.autostart` (`other-relay`): the task starts `op-reason.exe` every minute, which exits at once.

### 9.4 The lock

Stopping and restarting take op-reason's lock, the named mutex `Local\op-reason` that every `op` call and the relay's
windows hold (the relay holds it while its window is open and while 1Password answers the first request of a call),
waiting at most two seconds. Without it nothing is stopped: "the relay has a window open, or an op call waits for
1Password: answer it and try again" (exit 1). The lock is held until the relay has ended, and for a restart until the
new relay answers, so that no window opens in between.

The relay ends through its stop event (§8.2); op-reason waits up to 30 seconds for it, then ends it with
`TerminateProcess` (a relay of 0.6.0 has no stop event and is ended so at once).

### 9.5 Other tasks

A relay that answers while op-reason's task does not run it was started otherwise. op-reason looks among the running
tasks (`GetRunningTasks`) for one whose action is an `op-reason.exe --ssh-agent` (the dotfiles' task, for example) and
names it; else it says "started outside op-reason's task" (a terminal). It never stops, changes or removes such a relay
or task. `list` and the window also name tasks that are not running but would start a relay (a walk through all task
folders): two tasks that each start a relay at logon are a likely leftover of a move to op-reason's task.

## 10. The skill

### 10.1 The mark and the states

op-reason places `op-reason --skill` (the same `SKILL.md`) with one line after the front matter, so that Claude Code
still finds the front matter on the first line:

```text
<!-- placed by op-reason 0.7.0: op-reason --settings replaces this file when op-reason is updated, and removes it when the skill is turned off -->
```

| State | Meaning | `on` | `off` |
| --- | --- | --- | --- |
| `missing` | no `SKILL.md` | place it | same |
| `ok` | op-reason's mark, the same bytes as this op-reason would place | same | remove it |
| `old` | op-reason's mark of another version | replace it ("update") | remove it |
| `changed` | op-reason's mark of this version, other bytes (edited by hand) | replace it | remove it |
| `foreign` | a `SKILL.md` without op-reason's mark | refuse | same (nothing of op-reason's is there) |
| `link` | the skills folder, its parent, the `op-reason` folder or the file is a junction or a symbolic link (a reparse point) | refuse | refuse, unless nothing is there |

- Placing writes a temporary file in the `op-reason` folder and renames it over `SKILL.md`; it makes the folders when
  they are missing and looks for links again after making them.
- Removing removes `SKILL.md` and then the `op-reason` folder when it is empty; never other files.

### 10.2 The places

- `skill.windows`: `%USERPROFILE%\.claude\skills` (Claude Code on Windows).
- `skill.folder.<folder>`: a folder the user chose (another agent's skills folder, a project's `.claude\skills`).
  `set … on` remembers the folder in `settings.json` and `off` forgets it, so that `list` and the window show it.
- `skill.wsl.<distribution>`: `~/.claude/skills` of the distribution's default user, done by a POSIX shell script run as
  `wsl.exe -d <distribution> --exec sh -c <script> op-reason-skill <look|place|remove> <version>`, with the expected
  file on its input. The script finds the home directory (`$HOME`, else `getent passwd`), checks for links with `-L`,
  compares with `cmp`, writes with `mv`, and prints one line: `op-reason-skill 1 <state> <version or -> <path>`. It
  needs no op-reason in the distribution.
- The distributions are those of `wsl.exe --list --quiet` (`--running` for the ones that run), read as UTF-16 or UTF-8
  (`WSL_UTF8=1` is set); `docker-desktop` and `docker-desktop-data` are left out (they are not shells of the user).
  Without WSL there are none.

## 11. The window (second pull request)

### 11.1 Opening it

- `op-reason --settings` with no more arguments brings an open settings window to the front, or starts
  `op-reason.exe --settings window` without a console (`DETACHED_PROCESS`), lets it take the front
  (`AllowSetForegroundWindow`), and exits at once: a terminal gets its prompt back.
- **A double click on `op-reason.exe`** (no arguments, a console of its own, its output a console) does the same instead
  of printing the usage into a console that closes at once. The plan of the call table does not change (no arguments is
  still the usage everywhere else).
- One window per logon session: the window holds `Local\op-reason-settings-window`; a second one finds the first by its
  class and brings it to the front.
- The Start menu shortcut starts `op-reason.exe --settings` minimized, so that its short-lived console shows only for a
  moment in the taskbar.

### 11.2 What it shows

A normal window (it takes the focus, unlike the windows of calls), in the language of `lang`, built from Windows'
standard controls as `type-clipboard` builds them: comctl32 version 6 from the manifest, Per-Monitor V2 DPI, the
system's message font, `WC_BUTTONW` (buttons, check boxes, group boxes), `WC_COMBOBOXW`, `WC_EDITW` with `UPDOWN_CLASSW`,
`WC_LISTVIEWW` and `WC_STATICW`, tab order and keyboard as in a dialog (`IsDialogMessage`).

```text
op-reason settings                                                      [_][x]
┌ Language ─────────────────────────────────────────────────────────────────┐
│ Language   [English            ▼]   (English / 日本語 / From OPREASON_LANG) │
└───────────────────────────────────────────────────────────────────────────┘
┌ SSH relay ────────────────────────────────────────────────────────────────┐
│ Running · pid 4242 · per-call · started by op-reason's task               │
│ [Start] [Stop] [Restart]                                                  │
│ [x] Start at logon, and again within a minute when it stops               │
│ [x] Show its icon in the notification area                                │
│ Mode [per-call ▼]   Time limit [ 60 ]÷ min   Answer wait [ 60 ]÷ s        │
│ The mode and the times apply when the relay starts again.  [Apply]        │
└───────────────────────────────────────────────────────────────────────────┘
┌ Skill for AI agents (SKILL.md) ───────────────────────────────────────────┐
│ Place                   State             Where                          │
│ Windows (Claude Code)   placed (0.7.0)    C:\Users\<you>\.claude\skills   │
│ WSL: Ubuntu             not placed        /home/<you>/.claude/skills      │
│ WSL: Debian             not checked (stopped)                             │
│ [Place or update] [Remove] [Check] [Add a folder…]                        │
└───────────────────────────────────────────────────────────────────────────┘
┌ Start menu and records ───────────────────────────────────────────────────┐
│ [x] op-reason settings in the Start menu                                  │
│ Records  C:\Users\<you>\AppData\Local\op-reason   [Open]                  │
│ Accounts remembered for links: 1                  [Forget]                │
└───────────────────────────────────────────────────────────────────────────┘
                                                        [Refresh] [Close]
```

- Every control calls the engine of §5 (the same `set` as the command line, recorded with `from: window`). Slow
  actions (the relay, a distribution) run on a worker thread; the window shows "working…" and stays responsive.
- The relay's line is read when the window opens, after each action and with **Refresh**; the buttons that cannot act
  are greyed out with the reason in the line (for example, Stop when another task started the relay).
- Changing the mode or the times writes them; **Apply** restarts the relay of op-reason's task (with the lock), after
  a question that says that 1Password will ask again and that remembered answers are forgotten.
- Stopped distributions are not checked when the window opens (that would start them); **Check** on one starts it.
- **Add a folder…** opens the folder picker (`IFileOpenDialog` with `FOS_PICKFOLDERS`).

## 12. The icon in the notification area (second pull request)

- The relay shows it while it runs, unless `relay.tray-icon` is `off`: a thread of its own with a hidden top-level
  window (`op-reason-ssh-relay-tray`), `Shell_NotifyIconW` with `NOTIFYICON_VERSION_4`, and an icon drawn at run time
  (the exe has no icon resource).
- It is added again when Explorer starts again (`TaskbarCreated`), and tried every two seconds until it is added (at
  logon the task may start the relay before Explorer has a taskbar).
- Its tooltip: "op-reason SSH relay · per-call". A left click opens the settings window; the menu: the relay's line
  (greyed), **Open op-reason settings**, **Open the records folder**, **Restart the relay**, **Stop the relay** (until
  started again), **Hide this icon**. The actions start `op-reason.exe --settings …` in a process of its own (the window
  never opens inside the relay); Stop and Restart are greyed when op-reason's task does not run the relay.
- `relay.tray-icon` applies at once: a writer posts the registered message `op-reason-settings-changed` to the windows
  of the class, and the icon thread reads the file again.

## 13. The Start menu (second pull request)

- `start-menu on` makes `%APPDATA%\Microsoft\Windows\Start Menu\Programs\op-reason.lnk` (`IShellLinkW`): target the exe
  that ran `set`, arguments `--settings`, its folder as the working folder, shown minimized, description "op-reason
  settings". Windows' search finds it by its name.
- `same` when the shortcut is there with that target (without regard to case) and those arguments; another target (the
  exe moved) is `shortcut-differs`, and `set` makes the shortcut again.
- A shortcut of that name that does not start an `op-reason.exe` with `--settings` is `foreign`: never replaced or
  removed.

## 14. Safety

- Everything runs as the user who runs it, without administrator rights (the manifest's `asInvoker`); the task runs
  with the user's interactive token at least privilege.
- op-reason changes only what it made: its task (its name), its shortcut, and `SKILL.md` files with its mark. Other
  tasks, relays, shortcuts and files are shown, never changed.
- No writing through junctions or symbolic links in a skill place (§10.1).
- The relay is stopped only when op-reason's task runs it, and only with the lock (§9.4).
- Arguments reach other programs as separate arguments, never inside a command string: the WSL script gets the action
  and the version as `$1` and `$2` and the file on its input; distribution names are checked before they reach
  `wsl.exe`.
- `settings.json` holds no secret. Everything a settings change writes is in the user's own profile.
- A process of the same user can still change `settings.json`, stop the relay or run `op-reason --settings` (the README's
  "What it does not stop"): the records (§15) and the skill's rule make such a change visible, not impossible.

## 15. Records

`log.jsonl` gets a record per `set` or relay action:

```json
{"v":1,"id":"…","event":"settings","time":"…","epoch":0,"session":null,"from":"cli","caller":{"type":"claude","claude_pid":1234,"herdr":null,"cwd":"C:\\work","repo":null,"branch":null},"changes":[{"name":"relay.mode","from":"per-call","to":"show-only","result":"done"}]}
```

`from` is `cli`, `window` or `tray`; `result` is `done` or `failed` (with `why`). `op-reason --log` shows one line per
record: `settings` as the result, the caller, and the changes (`relay.mode per-call → show-only`).

The skill gets a rule: change op-reason's settings only when the user asks for that change.

## 16. Tests

- Linux (core): the list, values and their texts; `settings.json` kept and merged; the task's XML and shape comparison;
  the skill's mark and states, placing and removing in temporary folders with symbolic links; the WSL script run with
  `sh` against temporary home folders, and its output parsed; the engine with a fake `System` for every row of §9.2 and
  §10.1, the lock refused, a relay of another task, a relay of 0.6.0 without `relay.json`; the command line's
  arguments, output, JSON and exit codes; the WSL listing in UTF-16 and UTF-8; the `settings` record and its line in
  the log viewer; the call table's new row.
- Linux (black box): the WSL executable passes `--settings` on to a fake `op-reason.exe` with its arguments, exit code,
  the caller and a folder path turned by `wslpath`.
- Windows CI (`crates/op-reason-win/tests/settings.rs`, on the runner's desktop): `set relay.autostart on` makes a task
  (a test name through `OPREASON_RELAY_TASK`, honored only for tests: `CI` set) whose relay answers; `get`, `check`,
  `relay stop` (the task off, the relay gone), `relay start`, `relay restart` with the lock held by another process
  (refused), `set relay.autostart off`; a relay started outside the task is named and left alone; `relay.json` and the
  stop event; the relay's mode from `settings.json` and from its options; the skill on Windows and in a chosen folder
  (with `USERPROFILE` in a temporary folder, a junction refused); the WSL glue against a fake `wsl.exe`
  (`OPREASON_WSL_EXE`). Second pull request: the window opens, one window only, its controls change the file; the
  shortcut; the icon added and its menu; screenshots of the window in English and Japanese and of the icon's menu on the
  pull request.

## 17. Documents

- README.md and README.ja.md: a section "Settings" (the command line, the window, the list), the wrapper option, the
  task in "Starting it" made by `set relay.autostart on`, the skill placed by `set skill.windows on`.
- CHANGELOG.md (Unreleased), `docs/manual-verification.md` and `.ja.md` (a section of checks, §19), the architecture page
  (the layer), `docs/README.md` and the documents map, the skill (§15).

## 18. The dotfiles, after a release (not this repository)

The author's dotfiles script (`win/op-reason/Install-OpReason.ps1`) can then call the command line instead of its own
task code:

| The script today | With op-reason's command line |
| --- | --- |
| `Get-OpReasonRelayWant`, `Get-OpReasonRelayTask`, `Get-OpReasonRelayTriggerText`, `Compare-OpReasonRelayTask` (the wanted task, read and compared) | `op-reason.exe --settings check relay.autostart=on relay.mode=<mode> relay.running=on --json` (0 same, 1 different, 2 failed) |
| `Get-OpReasonRelayPlan` and `Register-OpReasonRelayTask` (register, update, start, restart), `Unregister-OpReasonRelayTask` (off) | `op-reason.exe --settings set relay.autostart=on relay.mode=<mode> relay.running=on`; `-SshRelay off`: `set relay.autostart=off` |
| the mode in the task's arguments (`--ssh-agent --mode show-only`) | `relay.mode` in `settings.json`; the task's arguments are `--ssh-agent` |
| `Get-OpReasonRelayPid` (`--ssh-agent --status`) | unchanged, or `get relay.running --json` (its process ID) |
| `Stop-OpReasonRelayLocked` before replacing the folder (the lock, the task off, the process stopped) and the start after it | `op-reason.exe --settings relay stop` before (exit 1 while a window is open), `<new folder>\op-reason.exe --settings relay start` after. The script may take the lock itself after the stop to keep calls out while it replaces the folder; `relay start` does not need it |
| `Format-OpReasonRelayState` (the lines of the state) | the lines of `check` |

The move, once: stop the relay of `\pjp\op-reason-ssh-relay` (with the lock), remove that task, then
`set relay.autostart=on …`, which makes op-reason's task and starts its relay. In the other order, op-reason makes its
task turned off while the old relay runs (§9.2), names the old task (§9.5), and `relay start` starts op-reason's relay
once the old one is gone.

## 19. Checks with the real app

To add to `docs/manual-verification.md` (G1–G…): `set relay.autostart on` from a terminal without administrator rights
makes the task (Task Scheduler shows it with "Show Hidden Tasks") and the relay answers; after a logoff and logon the
relay runs; 1Password's prompt comes at Allow as before (the task's relay, `2026-11-22-front-after-allow-design.md`);
`relay stop` while a window is open is refused; `relay stop` keeps the relay stopped after a logon; the mode from the
file; the WSL distribution's skill; and in the second pull request the window, the Start menu search, the icon after
Explorer restarts and at logon. On a machine where another task starts the relay, turn that task off and stop its
relay before these checks, and turn it on again afterwards.

## 20. Left to the implementer

The texts of the command line and the window; the exact codes of `state`; the layout of the window within §11.2; the
drawing of the icon; how the engine's trait is cut; the timeouts other than those stated; the names of the internal
words (`--settings window`). Each goes into the table below.

## 21. Implementer's decisions

| Decision | Why |
| --- | --- |
| A stop drains: new requests fail, those with 1Password finish within 25 seconds, then the relay exits; op-reason waits 30 seconds before `TerminateProcess` (builder check) | the lock covers the relay's windows and the first request of a call, not requests passing on an earlier answer or waiting for 1Password without a window; cutting one of those while 1Password shows an unlock prompt is the anti-goal of §2 |
| `get` prints the file's value of a relay setting; `check` and `set` also look at the running relay's values, and `set` restarts op-reason's task's relay whenever they differ from the file (builder check) | the file is what the relay starts with; `check` and then `set` must end the same, also after the window changed the file without a restart |
| With another relay answering, `relay.autostart on` makes the task turned off; `relay.running on` is the same; `relay.running off` turns op-reason's task off and fails for the other relay (builder check) | a task turned on would start `op-reason.exe` every minute, which exits at once and flashes a console window, until the other relay stops |
| `skill.folder.` refuses folders of the WSL share (builder check) | links cannot be told through the share; `skill.wsl.<distribution>` is the place inside a distribution |
| `SkillNow` keeps the state behind a link and names the link apart (not a state of its own) | `off` must tell "nothing of op-reason's is there" (the same) from "op-reason's file is behind a link" (refused) |
| `op-reason --settings` without more arguments prints the usage of `--settings` until the window comes (second pull request) | the first pull request has no window; the word is kept for it |
| `list` walks every task folder for other tasks that start a relay and names them on `relay.autostart` (`other-tasks`); `get`, `check` and `set` look only at the running tasks | §9.5; a walk takes a moment on a machine with many tasks, and only `list` and the window show it |
| The writers of `settings.json` wait up to five seconds for `Local\op-reason-settings`, and write without it after that | a writer that hangs must not keep the language switch of a call's window from working; the write is still atomic (a temporary file renamed over it) |
| `relay.json` is removed by the relay only when it holds the relay's own process ID; a reader trusts it only for the process that answers on the default pipe | a relay that was killed leaves the file; a later relay writes its own |
| The relay's stop is recorded as `ssh-relay` `stop` (with `detail` naming the requests still in progress after 25 seconds); `op-reason --log` shows "SSH relay stopped by op-reason --settings" | the log tells a stop by the settings from an exit (Ctrl+C, a closed console) |
| The record's `why` codes: `locked`, `not-ours`, `no-task`, `no-answer`, `foreign`, `link`, `failed: <message>` | short and stable for `op-reason --log --json` readers |
| On WSL, the caller's kind goes in `OPREASON_SETTINGS_CALLER` (`claude` or `human`) through `WSLENV`; `op-reason.exe` takes it for the record with its other signs of Claude Code | `op-reason.exe` started through WSL interop cannot see Claude Code's process or variables |
| A WSL path is turned into a Windows path only in `skill.folder./…` names, and in pairs only when the value is `on` or `off` | a folder may hold `=`; values never do |
| The test name of the task (`OPREASON_RELAY_TASK`) is honored only where `CI` is set | the tests on GitHub's runners must not meet a user's task, and a desktop must not take a variable for it |
| The guard for links covers the chosen folder's parent too (`guard_parent` for every place) | the same rule as `.claude` of `.claude\skills`: a chosen folder whose parent is a junction is refused |
| `docker-desktop` and `docker-desktop-data` (any name starting with `docker-desktop`) are left out of the distributions, and lines that are not names (a message of `wsl.exe`) too | they are not shells of the user; `wsl.exe --list` prints a message instead of names when there are none |
