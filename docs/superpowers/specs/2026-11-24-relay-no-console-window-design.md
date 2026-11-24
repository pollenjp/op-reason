# op-reason: no console window when no terminal starts op-reason.exe — design

- Date: 2026-11-24
- Status: approved (the checks with the real app before the settings' pull requests were merged found a console window
  that comes to the front for a moment whenever Task Scheduler starts the SSH relay; the user chose to fix it after the
  merge, in a pull request of its own, with the manifest's console allocation policy alone (choice A below), and to
  release after it. This document records the research behind the choice, the design and the implementer's decisions).
- Builds on: `2026-11-23-settings-design.md` (§9 op-reason's task, §11.1 opening the window, §12 the icon, §21 the
  consoles of the window and the icon), `2026-11-17-ssh-relay-design.md` (§8 and §14: the relay lets go of a console of
  its own), `2026-11-22-front-after-allow-design.md` (§1 and §3.1: what 1Password names for a relay that a task started),
  `2026-11-15-windows-exe-design.md` (§8.2: `op.exe` shares the console) and `2026-11-15-wsl-through-exe-design.md`
  (calls from WSL).

## 1. What and why

`op-reason.exe` is a console program (PE subsystem 3, as Rust builds it), so that terminals wait for it and give it
their console. A console program that starts without a console to inherit gets one that Windows makes for it, and
Windows shows it: in its own window, or handed to Windows Terminal when that is the default terminal, whose window comes
to the front and takes the focus.

Task Scheduler starts the SSH relay without a console in three places, all through op-reason's task (its action
`op-reason.exe --ssh-agent --task <its name>`):

- at logon, and within a minute when the relay has stopped (the task's triggers);
- **Apply** in the settings window and `relay restart` (the layer runs the task: `IRegisteredTask::Run`);
- **Restart the relay** in the icon's menu (`op-reason.exe --settings relay restart` under Explorer, which runs the task).

The relay lets go of a console that only it uses (`FreeConsole`, `2026-11-17-ssh-relay-design.md` §14), but only after
it has read its settings, made its mutex and its pipe and written its first line: until then the console's window is
there. The checks with the real app on Windows 11 (build 26300, Windows Terminal the default terminal) saw it come to
the front for a moment at each of the three. The dotfiles' task starts the same exe (`--ssh-agent`, with a trigger of
every minute), so it flashes the same way.

The settings' second pull request already keeps the console programs that the window and the icon start from showing
a console (`CREATE_NO_WINDOW` for `wsl.exe` and the window's process); a process that Task Scheduler starts gets no
such flag.

## 2. Goal, non-goals and anti-goals

Goal:

- On Windows 11 24H2 and later and Windows Server 2025 (build 26100 and later), a relay that Task Scheduler starts
  (op-reason's task at logon, every minute, after Apply and after the icon's restart; any other task, such as the
  dotfiles') shows no console window, not even for a moment.
- On the same Windows, a double click on `op-reason.exe` and the Start menu's shortcut open the settings window without
  a console window.
- Everything else stays as it is: what terminals (PowerShell, cmd.exe, Windows Terminal, WSL, agents) see of a call,
  `--settings`, `--log` and `--ssh-agent --status` (output, exit code, the terminal waiting for the end, Ctrl+C), the
  relay's process tree, op-reason's task and `relay.json`.

Non-goals:

- Older Windows (Windows 10, Windows 11 23H2 and before): they keep today's behavior, a console for a moment that the
  relay lets go of. Windows 11 23H2 left servicing on 2025-11-11 (Home and Pro) and 2026-11-10 (Enterprise and
  Education), so every Windows 11 still serviced is 24H2 or later.
- A second exe, a wrapper in the task's action, or a change to the task.
- Signing the exe (Smart App Control stays as the README describes it).

Anti-goals (a failure even if the window goes away):

- A call from a terminal changes: its output, its exit code, the terminal waiting for it, or Ctrl+C.
- A call that a program starts without a terminal (WSL for an agent's command or a script, with `CREATE_NO_WINDOW`; a
  program without a console that passes pipes) shows a console window for `op.exe`: without op-reason's console,
  `op.exe` would get a console window of its own (§3.2).
- 1Password stops answering the relay, or names another application: the relay's parent, its process tree and its state
  after the start must stay as they are now (§5.4).

## 3. What Windows does

### 3.1 The console allocation policy

Microsoft's documentation ("Console Allocation Policy", Windows Console docs; the minimum is Windows 11 24H2,
build 26100, and Windows Server 2025) and the specification behind it (Windows Terminal's repository,
`doc/specs/#7335 - Console Allocation Policy.md`):

- An application manifest element, `<consoleAllocationPolicy
  xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>` in
  `<windowsSettings>`, applies to console-subsystem programs.
- `detached`: "The new process is not attached to a console session (similar to `DETACHED_PROCESS`) unless one was
  inherited." "All console inheritance will proceed as normal." It "will not present a console window when launched by
  Explorer, Task Scheduler, etc." The specification has the field disable the automatic allocation "regardless of the
  process creation flags passed to `CreateProcess`" and recommends that it override `CREATE_NEW_CONSOLE`, which
  `ShellExecute` passes by default.
- What GitHub's Windows Server 2025 runner (build 26100) did with this pull request's exe: a start without flags from a
  process without a console (Task Scheduler) got no console, and so did `CREATE_NEW_CONSOLE` (the double click's test
  failed until §5.3); `CREATE_NO_WINDOW` still got a console that is never shown (with the policy but without §5.2,
  `op.exe` of such a call showed no console window: it shared op-reason.exe's console).
- Shells still decide from the subsystem whether to wait: a console program with the policy still makes cmd.exe and
  PowerShell wait for it.
- Older Windows ignore the element and allocate consoles from the subsystem as before.
- Other programs ship it: Syncthing, Wireshark, FreeRDP, darktable, and `no-mistakes`, a daemon that a scheduled task
  starts, which keeps `FreeConsole` for older Windows as op-reason's relay does, and which rejected a launcher and a
  `conhost.exe --headless` wrapper because they break the task's identity.

### 3.2 Where op-reason.exe is started without a console to inherit

| Started by | Console today | With the policy |
| --- | --- | --- |
| A terminal (PowerShell, cmd.exe, Windows Terminal), an agent's shell | the terminal's (inherited) | the same |
| WSL with a terminal: interop gives it a pseudoconsole (`microsoft/WSL`, `src/windows/common/interop.cpp`) | the pseudoconsole | the same (a pseudoconsole is the console the process starts attached to) |
| WSL without a terminal: interop uses `CREATE_NO_WINDOW` and pipes | a console of its own, never shown; `op.exe` shares it | the same (§3.1) |
| A program without a console that passes pipes and no flags (an editor's extension, a GUI tool) | a console of its own, shown for the length of the call | **none**: `op.exe` must get a console of its own, never shown (§5.2) |
| Task Scheduler (the relay) | a console of its own, shown until `FreeConsole` | **none** (the fix) |
| Explorer: a double click, the Start menu's shortcut, `Start-Process` (`CREATE_NEW_CONSOLE`) | a console of its own, shown | **none**: the double click must still open the settings window (§5.3) |
| The icon's actions (`DETACHED_PROCESS`) | none | none |
| The settings window's process (`CREATE_NO_WINDOW`) | a console of its own, never shown | the same; `wsl.exe` has its own `CREATE_NO_WINDOW` anyway |

### 3.3 conhost.exe

`conhost.exe` is a GUI-subsystem program. `conhost.exe --headless <command>` (an argument for ConPTY, not documented
for other use) runs a command with a console that has no window, and ends when the last process attached to that
console lets go of it, as every console host does.

## 4. Approaches

| Approach | Old Windows | The relay and 1Password | What it costs | Chosen |
| --- | --- | --- | --- | --- |
| (1) The task's action wraps the relay in `conhost.exe --headless` | no window from Windows 10 1809 | the relay's parent becomes conhost; the relay's `FreeConsole` ends conhost, so the relay loses its parent and 1Password stops answering it; what 1Password names is unknown | an undocumented argument, a form that malware detection rules flag; the task's shape, the search for other tasks and the dotfiles' task change | no |
| (2) The manifest's `consoleAllocationPolicy` `detached` | as today (a console for a moment) | unchanged: the relay's parent, its tree, the task and `relay.json`; after its start the relay is in the state it reaches today with `FreeConsole` | two adjustments for starts without a console (§5.2, §5.3) | **yes** (the user's choice A) |
| (3a) The relay as a second, GUI-subsystem exe | no window anywhere | the exe's name changes what 1Password names (checked again with the real app) | the zip about doubles, a second unsigned exe for Smart App Control; the task, `relay.json` and the dotfiles change (`2026-11-23-settings-design.md` §3 rejected a second GUI exe for the same reasons) | no |
| (3b) A small GUI-subsystem launcher that starts the relay without a console and waits | no window anywhere | the launcher becomes the relay's parent: what 1Password names and which window it takes for the application's are to be checked again | a second unsigned exe; the task's shape, the search for other tasks and the dotfiles change | no |
| Others: PowerShell `-WindowStyle Hidden` (flashes too), VBScript (being removed from Windows), a Startup shortcut (out of the task's job: "Do not start a new instance" and the trigger of every minute no longer hold), a task that runs whether the user is logged on or not (no desktop for the relay's windows), op-reason.exe as a GUI program (terminals no longer wait for it) | | | | no |

The user chose (2) alone (no launcher for older Windows) after the comparison.

## 5. The design

### 5.1 The manifest

`crates/op-reason-win/build.rs` keeps building the manifest it builds today (the name and the version from
`Cargo.toml`, Per-Monitor V2, as the invoker, comctl32 version 6) and adds the element
`<consoleAllocationPolicy xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>`
inside `<asmv3:windowsSettings>`. `embed-manifest` 1.5.1 has no setter for it, so the build script renders the
builder's XML, adds the element before `</asmv3:windowsSettings>`, writes the result to `OUT_DIR` and embeds that file
(`embed_manifest_file`). The build fails when the rendered XML does not have that closing tag exactly once, so that a
change of `embed-manifest` cannot drop the policy silently.

### 5.2 op.exe when op-reason.exe has no console

`op.exe` (the call's own, started suspended in the Job Object) inherits op-reason.exe's console, as today, whenever
op-reason.exe has one (`2026-11-15-windows-exe-design.md` §8.2: a terminal's, a pseudoconsole, or one never shown).
When op-reason.exe has none (`GetConsoleProcessList` finds no console: a start without flags from a scheduled task or a
program without a console, `CREATE_NEW_CONSOLE`, `DETACHED_PROCESS`), `op.exe` starts with `CREATE_NO_WINDOW`: it gets
a console of its own that is never shown, instead of one that Windows would show for the length of the call. A call
that WSL starts without a terminal keeps op-reason.exe's console that is never shown (§3.1), and `op.exe` shares it as
before.
Its standard handles are op-reason.exe's, passed down as before. `whoami` and `signout` keep `CREATE_NO_WINDOW` as they
have it now.

### 5.3 The double click

`op-reason.exe` without arguments opens the settings window instead of printing the usage when nobody reads its output
(`2026-11-23-settings-design.md` §11.1). That is now either of:

- a console that only this process uses, with standard output on it (as before: Windows made the console for it);
- no console at all and no standard output (the policy: Explorer, the Start menu, a task).

A program that starts `op-reason.exe` without arguments and reads its output (a pipe, a file) still gets the usage.

### 5.4 The relay

The relay does what it does now. `detach_console` looks at the console the relay has at start:

- none (the policy: Task Scheduler, Explorer): nothing to let go of;
- a console of its own (older Windows): `FreeConsole`, as now;
- a console shared with other processes (a terminal): it stays attached, so that Ctrl+C stops it and the exit is
  recorded, as now.

After this the relay has no console in the first two cases, as now: its threads, its windows, its icon, its parent and
its records are the same. The `start` record of `ssh-relay` gets the console in its `detail`: `console: none`,
`console: own` or `console: shared` (the `detail` field came with 0.6.0; `op-reason --log` shows the start as before,
without it). The record tells a flash on older Windows from a fault, and lets the tests check the start itself.

### 5.5 What does not change

- op-reason's task (action, arguments, triggers, principal, rules, hidden, priority), the shape that `relay.autostart`
  compares, the search for other tasks that start `op-reason.exe --ssh-agent`, `relay.json`, the lock, the stop event.
- The dotfiles' task: once its exe is a release with this change, its relay shows no console window either, without a
  change to the task.
- The icon's actions (`DETACHED_PROCESS`, under Explorer), the window's process and `wsl.exe` (`CREATE_NO_WINDOW`).
- The zip (one exe), the skill, `--ssh-agent --status`, the WSL binary.

## 6. Records

| Record | Change |
| --- | --- |
| `ssh-relay` `start` | `detail`: `console: none` (Windows made none), `console: own` (Windows made one for the relay alone, which it let go of), `console: shared` (a terminal's) |

## 7. Tests

On GitHub's Windows runner (`windows-2025`: Windows Server 2025, build 26100, where the policy applies; its default
terminal hands consoles to Windows Terminal, and the test of the settings window's consoles caught both
`ConsoleWindowClass` and `CASCADIA_HOSTING_WINDOW_CLASS` there before its fix):

1. The exe carries the policy: the built `op-reason.exe` holds the element (its manifest is plain text in the exe's
   resources).
2. A relay of op-reason's task shows no console window and records `console: none`, when `set relay.autostart on`
   runs the task (as at logon), when `relay restart` runs it (what Apply does), and when the icon's **Restart the
   relay** runs it. The windows are watched every 2 ms for a visible console window whose title names the exe (the
   watcher of the settings window's test, shared).
3. A call started as WSL starts one without a terminal (`CREATE_NO_WINDOW`, pipes) and as a program without a console
   starts one (`DETACHED_PROCESS`, pipes: no console, as the policy leaves a start without flags), on the entry face and
   on the runner face with the fake `op.exe`: its output and exit code come through the pipes, and no console window
   names the fake `op.exe`.
4. The double click (`CREATE_NEW_CONSOLE` and a hidden window, as now) still opens the settings window.
5. Under a pseudoconsole (as WSL starts a call from a terminal), `op-reason.exe --version` prints its version through
   the pseudoconsole.
6. On `windows-2022` (Windows Server 2022, build 20348, which does not know the policy), the zip's `op-reason.exe`
   starts: `--version` prints the version and `--ssh-agent --status` exits 1 with its line.

Test 2 was pushed first with today's exe and failed on the runner (the relays of the task recorded `console: own`: the
bug). With the manifest alone, tests 1, 2 and 5 passed and test 4 failed (Explorer's start had no console), while test 3
passed: `CREATE_NO_WINDOW` keeps its console (§3.1). Test 3 then got the start with `DETACHED_PROCESS`, which needs
§5.2 on any Windows.

The existing tests stay as they are (the CLI's outputs and exit codes from a terminal-like parent, the task's start,
stop and restart, the icon's restart, the window's consoles).

## 8. Documents

- The READMEs (English and Japanese): "Starting it" says that on Windows 11 24H2 and later the relay of a task shows no
  console window and that older Windows show one for a moment; the Start menu's line about its console; a line in the
  notes of "On Windows" for starts without a terminal.
- `CHANGELOG.md` (Unreleased).
- `docs/manual-verification.md` and `.ja.md`: the checks of §9 (N1–N10), and G11 and G15's expected console.
- `docs/architecture/README.md`: the relay's start and the settings window's process.

## 9. Checks with the real app

With the zip of the pull request on the author's Windows (build 26300), the dotfiles' task turned off and its relay
stopped first (it runs the released exe):

- N1 `set relay.autostart on`: no console window; the `start` record says `console: none`.
- N2 log off and on: the relay answers; no console window at logon.
- N3 the window's **Apply** (after a mode change), N4 the icon's **Restart the relay**: no console window.
- N5 `ssh -T git@github.com` through the relay: op-reason's window, then 1Password's prompt at once, naming op-reason
  (as G4).
- N6 a double click on `op-reason.exe`, N7 the Start menu's **op-reason**: the settings window, no console window.
- N8 from WSL, a call without a terminal (`… </dev/null | cat`): the window, the output, no console window.
- N9 from PowerShell: a call's output and `$LASTEXITCODE`, `--ssh-agent --status`, Ctrl+C on a relay started by hand.
- N10 back as before: `set relay.autostart off`, the dotfiles' task turned on again.

## 10. The dotfiles and the release (not this pull request)

Nothing to change in the dotfiles: once they pin a release with this change, their task's relay shows no console
window on Windows 11 24H2 and later. The move to op-reason's own task is separate. The version is raised in the release
that follows this pull request.

## 11. Implementer's decisions

| Decision | Why |
| --- | --- |
| The policy goes into the one manifest that `build.rs` already embeds, added to the builder's XML before `</asmv3:windowsSettings>`; the build fails if that tag is not there exactly once | `embed-manifest` 1.5.1 has no setter for the element; a second manifest file kept by hand would drift from `Cargo.toml`'s version |
| `op.exe` gets `CREATE_NO_WINDOW` only when op-reason.exe has no console (`GetConsoleProcessList` returns 0) | with a console, sharing it stays as the Windows exe's design wants (stdin, Ctrl+C); without one, Windows would show `op.exe` a console window for the length of the call. The README requires 1Password's CLI integration, so `op.exe` asks through the 1Password app, not in a console |
| §5.2 stays although the runner showed that WSL's `CREATE_NO_WINDOW` keeps a console | the policy leaves a start without flags with no console (a task, a program without a console that passes pipes), and a later Windows may treat `CREATE_NO_WINDOW` as the specification's wording says |
| The double click is "no arguments, and a console of its own with standard output on it, or no console and no standard output" | Explorer starts the exe without a console now; a program that reads the output of a bare `op-reason.exe` must still get the usage |
| The relay's `start` record names its console (`none`, `own`, `shared`) in `detail` | the tests check the start itself, not only the windows they can see; it tells a flash on older Windows from a fault. The log viewer's label of `start` stays without it |
| No check of the Windows build at run time | the manifest does it: older Windows ignore the element |
| Older Windows keep today's flash; no launcher for them | the user's choice A; they are out of servicing |
| A program that starts `op-reason.exe` with arguments and without a console (a shortcut, `Start-Process` without `-NoNewWindow`) gets no console window on Windows 11 24H2 and later; its output goes nowhere | the policy is per exe; such a start only showed a console that closed at the exit; terminals are unchanged |
| Tests on `windows-2025` for the policy and on `windows-2022` for older Windows (a start of the zip's exe) | the runner of `windows-2025` applies the policy; the older one proves that the element does not stop the exe from starting where Windows does not know it |
