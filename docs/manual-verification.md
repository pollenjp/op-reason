# Checking op-reason with the real 1Password app

日本語版: [manual-verification.ja.md](manual-verification.ja.md)

CI runs op-reason with fake `op-reason.exe`, `powershell.exe` and `op.exe` only: the 1Password desktop app cannot run
there, and the Windows runners have no WSL2. These checks need a person, a Windows machine with the 1Password app and
the 1Password CLI, and WSL. The WSL checks (1–14 and R) take about 30 minutes and show about fifteen approval prompts.
The checks of the references (F) add about ten minutes, the checks of the SSH relay (S) about forty.

## Rules while checking

- Use commands whose output holds no secret: `vault list`, `whoami`, `item template get`, `item create --dry-run`.
  Send their output to `/dev/null` unless a step says otherwise. Do not run `op read`, `item get` or `--reveal`.
- Answer each prompt as the step says. If a prompt appears that the step does not expect, deny it and note it.
- When you are done, lock 1Password once.

## Preparation

Unpack the Windows zip (`op-reason-windows.zip` of a release, or the `op-reason-windows` artifact of a pull request's
Windows exe workflow) into a Windows folder and check its SHA-256 as the README shows. In WSL:

```sh
cargo build --release
export PATH=$PWD/target/release:$PATH
ln -sf op-reason target/release/op && ln -sf op-reason target/release/op-reason-log
command -v op            # …/target/release/op
op.exe --version         # the real 1Password CLI answers
export OPREASON_EXE=/mnt/c/Users/<you>/<the folder>/op-reason.exe   # not needed when it is on PATH
export OPREASON_LANG=en  # or ja
```

Where `cargo build` stops with ``linker `cc` not found`` (no C compiler on the machine), build in the repository's
development shell instead: `nix develop "path:$PWD/nix" --command cargo build --release`.

Checks 1–14 run with the default (no `OPREASON_RUNNER`), through `op-reason.exe`. The window is the Direct2D window
(it looks a little different from the launcher's WPF window).

## Checks

| # | Do | Expect |
| --- | --- | --- |
| 1 | `op --reason "manual check 1: list vaults" vault list >/dev/null`, approve | The window appears at the top right with time, caller (Human), place, reason and command; a toast appears; one prompt; exit 0. `op-reason-log` shows `approved` |
| 2 | Run check 1 again right away, approve | A prompt appears again (the first approval was not reused) |
| 3 | `op.exe whoami; echo $?` (directly, not through op-reason) | Not signed in, exit code not 0 (no approval is left for WSL) |
| 4 | `OPREASON_LANG=ja op --reason "確認 4: 日本語の窓" vault list >/dev/null`, approve | The window and the toast are in Japanese and readable |
| 5 | `op --reason "manual check 5: deny" vault list`, **deny** | `[ERROR] … authorization prompt dismissed` on stderr, exit not 0; `op-reason-log` shows `denied` |
| 6 | `op --reason "manual check 6: timeout" vault list`, do not answer until 1Password gives up | `op-reason-log` shows `timed out` |
| 7 | Terminal A: `op --reason "manual check 7A" vault list >/dev/null`, do not answer yet. Terminal B: `op --reason "manual check 7B" vault list >/dev/null` | B prints "Waiting for another op call …". After you approve A, B's window and prompt appear; approve B |
| 8 | As check 7, but B with `OPREASON_LOCK_WAIT=10` and A unanswered for 15 seconds, then deny A | B exits with 75 and names A's reason; `op-reason-log` shows `refused (lock wait timed out)` |
| 9 | `op --reason "manual check 9: template" item template get Login \| op --reason "manual check 9: preview from the template" item create --dry-run --vault Private - >/dev/null`, approve both prompts | Both windows appear, nothing hangs, exit 0; `op-reason-log` shows two `approved`. When the receiver took the lock first, the sender's window has a note that it runs without the lock and its row shows `(lock skipped)` |
| 10 | `op.exe vault list >/dev/null` (directly), approve; then `op --reason "manual check 10: watchdog" whoami` | Before the call op-reason clears the shared approval: a notification "Cleared a shared 1Password CLI approval"; `op-reason-log` shows `cleared a shared approval`. (`whoami` itself then reports that it is not signed in) |
| 11 | `timeout 5 op --reason "manual check 11: interrupted" vault list`, do not answer | Note what happens to the window and the prompt (as in R3); deny the prompt if it stays. `op-reason-log` shows `interrupted` |
| 12 | `op --version`, `op --help`, `op item get --help` | No reason needed, no window, no prompt |
| 13 | `op item get x` (no reason) | Exit 2, the usage with the hint for AI agents; `op-reason-log` shows `refused (no reason)` |
| 14 (optional) | With a service account: put the token in `OP_SERVICE_ACCOUNT_TOKEN` without showing it, then `op --reason "manual check 14: service account" vault list >/dev/null` | No window, no prompt, exit 0; `op-reason-log` shows `ok (SA)` |

## Checks from WSL through op-reason.exe

CI cannot start Windows programs from WSL, so these can only be checked by hand. They take about 15 minutes and show
about six prompts. Where the expectation says "note", nobody has seen it yet: write down what you see.

| # | Do | Expect |
| --- | --- | --- |
| R1 | `time op --version` three times, then three times with `OPREASON_RUNNER=launcher` (calls without a reason or a prompt) | Note each `real`. The exe should be much shorter than the launcher (started from Windows directly, the exe takes tens of milliseconds, the launcher about half a second) |
| R2 | `op --reason "check R2: time to the window" vault list >/dev/null`, approve; then the same with `OPREASON_RUNNER=launcher` | Note how soon the window appears after Enter (at once, about half a second, a second or more), for both |
| R3 | `op --reason "check R3: TERM" vault list & pid=$!`; when the prompt shows, do not answer and `kill -TERM $pid` | Note: does the window close, does the prompt stay, is anything left in `tasklist.exe /FI "IMAGENAME eq op-reason.exe"` and `tasklist.exe /FI "IMAGENAME eq op.exe"`. `op-reason-log` shows `interrupted` (143). Deny the prompt if it stays. Do the same with `OPREASON_RUNNER=launcher` and note the difference |
| R4 | Press Ctrl+C in the WSL terminal while the prompt of `op --reason "check R4: Ctrl+C" vault list` is open | Note as in R3. `op-reason-log` shows `interrupted` (130). Does `op.exe whoami` then say that it is not signed in? |
| R5 | On a machine where Smart App Control blocks `op-reason.exe`: `op --reason "check R5: fallback" vault list >/dev/null`, approve. On a machine where it does not: copy `C:\Windows\System32\whoami.exe` as `op-reason.exe` into an empty folder, put the zip's `launcher.ps1` next to it, point `OPREASON_EXE` at that copy, and do the same | Note what WSL prints (the interop's message, if any, when Windows refuses the exe). Then one line says the call runs through launcher.ps1 instead, and the launcher's window appears. Approve: exit 0. `op-reason-log` shows `approved (fallback)`, and the last call of `op-reason-log --json` has `fallback` in its `end` |
| R6 | With the setup of R5: `printf 'plain text\n' \| op --reason "check R6: stdin through the fallback" inject`, approve | `plain text` comes out (stdin arrives whole after a fallback) |
| R7 | With the setup of R5: `OPREASON_RUNNER=launcher op --reason "check R7: launcher only" vault list >/dev/null`, approve | The launcher's window appears without the fallback line (and without a Smart App Control notice) |

## Checks of the window's language

English by default; the switch `English | 日本語` at the bottom right of `op-reason.exe`'s window chooses
([2026-11-15-language-toggle-design.md](superpowers/specs/2026-11-15-language-toggle-design.md)). CI clicks the switch
with the mouse of its Windows runner, but cannot show a 1Password prompt next to it. These take about 10 minutes and
show about seven prompts. First, in WSL `unset OPREASON_LANG; rm -f ~/.local/state/op-reason/lang.json`, and in
PowerShell `Remove-Item "$env:LOCALAPPDATA\op-reason\settings.json" -ErrorAction SilentlyContinue`.

| # | Do | Expect |
| --- | --- | --- |
| L1 | `LANG=ja_JP.UTF-8 op --reason "check L1: English by default" vault list >/dev/null`, approve | The window and the toast are in English (`LANG` does not choose Japanese). At the bottom right of the window: `English \| 日本語`, English bold and black, 日本語 blue and underlined; the pointer becomes a hand over 日本語 only |
| L2 | `op --reason "check L2: switch" vault list >/dev/null`. While the prompt is open, click 日本語 in op-reason's window, then look at the prompt before answering it | The window switches to Japanese at once (caption, labels, footer; now 日本語 is bold and English is the link) and keeps its top right corner. It does not take the focus: the prompt stays the active window (its title bar stays active, op-reason's caption stays gray) and still answers the keyboard (if it has a text field, the cursor still blinks there; do not type your password). Approve. `Get-Content "$env:LOCALAPPDATA\op-reason\settings.json"` shows `{"v":1,"lang":"ja"}` |
| L3 | `op --reason "check L3: remembered" vault list >/dev/null`, approve; then `op item get x` (no reason) and `op-reason-log` | The window and the toast are in Japanese from the start; so are the refusal and `op-reason-log`. `cat ~/.local/state/op-reason/lang.json` shows `{"v":1,"lang":"ja"}` |
| L4 | `OPREASON_LANG=en op --reason "check L4: the choice beats OPREASON_LANG" vault list >/dev/null`, approve | Still Japanese |
| L5 | Click English in the window of `op --reason "check L5: back to English" vault list >/dev/null`, approve. Then in PowerShell: `op-reason.exe --reason "check L5 from Windows" vault list > $null`, approve | The window of the call from Windows is in English and has the switch too (one choice for WSL and Windows); `op-reason.exe --log` is in English |
| L6 | `OPREASON_RUNNER=launcher OPREASON_LANG=ja op --reason "check L6: the launcher reads the choice" vault list >/dev/null`, approve | The launcher's window (the WPF one) is in English, the chosen language, and has no switch |
| L7 | Delete both files as at the start of this section; `OPREASON_LANG=ja op --reason "check L7: reset" vault list >/dev/null`, approve; `unset OPREASON_LANG` and run it again, approve | First Japanese (`OPREASON_LANG` counts again while nothing is chosen), then English |

## Checks of the references in the window

The window lists the `op://` references a call reads, and an ID reference opens its item in the 1Password app
([2026-11-17-refs-in-window-design.md](superpowers/specs/2026-11-17-refs-in-window-design.md)). CI checks the list,
the account copy and a click with a fake `op.exe` and a fake `onepassword:` handler, but not the real app. These take
about 10 minutes and show about seven prompts. **Deny** every prompt except F2: the window shows before the prompt, so
nothing is read. The `inject` lines send op's output to `/dev/null` (`$null` in PowerShell), so that a prompt approved
by mistake prints no value.

First: in 1Password, pick an item you do not mind seeing, choose "Copy Private Link" from its "…" menu, and note the
values after `v=` (the vault ID) and `i=` (the item ID); do not paste the link anywhere else. In WSL, in an empty
folder, `V=<the v value>; I=<the i value>` and
`printf 'A={{ op://%s/%s/password }}\nB={{ op://Private/no-such-item/password }}\n' "$V" "$I" > tpl.env`. In
PowerShell, `Remove-Item "$env:LOCALAPPDATA\op-reason\account.json" -ErrorAction SilentlyContinue`.

| # | Do | Expect |
| --- | --- | --- |
| F1 | `op --reason "check F1: references, first call" inject -i tpl.env >/dev/null`, deny | Under the command line: "References to read (2, from tpl.env):", the two references indented and black (no link), and the line "Links to the items appear once an approved call has finished (…)" |
| F2 | `op --reason "check F2: learn the account" vault list >/dev/null`, approve | `Get-Content "$env:LOCALAPPDATA\op-reason\account.json"` shows `{"v":1,"accounts":[{"id":"…","host":"…"}]}` with the private link's `a=` as `id` and your sign-in address as `host`, and nothing else (no email address) |
| F3 | `op --reason "check F3: a link" inject -i tpl.env >/dev/null`. The first reference is blue and underlined, with the hand pointer over it; the second stays black; no hint line. While the prompt is open, select another item in 1Password, then click the link. Look at the prompt, then deny | The 1Password app comes to the front with the item of the link open; the prompt stays in front of the app and can still be answered. op-reason's window does not take the focus (its caption stays gray) |
| F4 | Lock 1Password, run the command of F3 again, and click the link while the prompt asks to unlock | Nothing comes forward: the app stays locked, in the back. The prompt stays and can still be answered; deny |
| F5 | In PowerShell, in a folder with a copy of `tpl.env`: `Get-Content tpl.env \| op-reason.exe --reason "check F5: a template on stdin" inject > $null`, deny | "References to read (2, from stdin)" with the link (the exe read the template from stdin) |
| F6 | `OPREASON_RUNNER=launcher op --reason "check F6: the launcher" inject -i tpl.env >/dev/null`; click the link; deny | The launcher's (WPF) window shows the same block and link; a click brings the app to the front with the item, the prompt in front of it; the launcher's window never becomes the foreground window |
| F7 | `op-reason-log`, then `op-reason-log --json \| tail -n 1` | The rows of F1, F3, F4 and F6 end with "(2 refs)"; the last JSON object has both references in `refs.list` |

## Checks on Windows: op-reason.exe

Use `op-reason.exe` from the folder of the R checks, by its full name `op-reason.exe`. They take about 15 minutes and
show about ten prompts.

| # | Do | Expect |
| --- | --- | --- |
| E1 | In PowerShell: `op-reason.exe --version` | The version. If Windows refuses to start it (Smart App Control) or warns (SmartScreen), note the exact message: the other E checks cannot run, and this machine needs the PowerShell entry (the W checks) |
| E2 | `op-reason.exe --reason "check E2: list vaults" vault list > $null`, approve | Window at the top right and a toast, one prompt, exit 0; `op-reason.exe --log` shows `approved` |
| E3 | Run E2 again | A prompt again |
| E4 | In cmd.exe: `op-reason.exe --reason "check E4: from cmd" whoami`, approve | Works the same from cmd.exe |
| E5 | Terminal A (WSL): `op --reason "check E5 WSL" vault list >/dev/null`, do not answer. Terminal B (PowerShell): `op-reason.exe --reason "check E5 exe" vault list > $null` | B says it waits for another op call; after you answer A, B's window and prompt appear |
| E6 | As E5 with `$env:OPREASON_LOCK_WAIT = 10` in B and A unanswered for 15 seconds, then deny A | B exits with 75 and names A's reason; `op-reason.exe --log` shows B as `refused (lock wait timed out)` |
| E7 | `op-reason.exe --reason "check E7: deny" vault list`, **deny** | Exit not 0, op's error on stderr; `op-reason.exe --log` shows `denied` |
| E8 | Press Ctrl+C while the prompt of `op-reason.exe --reason "check E8: interrupt" vault list` is open | The window disappears; deny the prompt if it stays. `op-reason.exe --log --json` shows `interrupted` with exit 130, and `op.exe whoami` says that it is not signed in |
| E9 | End `op-reason.exe` in Task Manager while the prompt of `op-reason.exe --reason "check E9: kill" vault list` is open | The window disappears, and `op.exe` ends too, with its prompt (the Job Object). If the prompt stays, note it and deny it |

## Checks on Windows: the PowerShell entry (fallback)

Add the zip's `powershell-entry` folder to `PATH` in front of the folder of `op-reason.exe` (README, "If Smart App
Control blocks op-reason.exe").

| # | Do | Expect |
| --- | --- | --- |
| W1 | In PowerShell: `op-reason --reason "check W1: list vaults" vault list > $null`, approve | Window and toast, one prompt, exit 0; `%LOCALAPPDATA%\op-reason\log.jsonl` has `start` and `end` (`approved`) |
| W2 | Run W1 again | A prompt again |
| W3 | In cmd.exe: `op-reason.cmd --reason "check W3: from cmd" whoami`, approve | Works the same through the shim |
| W4 | Terminal A (WSL): `op --reason "check W4 WSL" vault list >/dev/null`, do not answer. Terminal B (PowerShell): `op-reason --reason "check W4 Windows" vault list > $null` | B says it waits for another op-reason call; after you answer A, B's window and prompt appear |
| W5 | As W4 with `$env:OPREASON_LOCK_WAIT = 10` in B and A unanswered for 15 seconds, then deny A | B exits with 75 and names A's reason; B's `end` record says `lock-timeout` |
| W6 | `op-reason --reason "check W6: deny" vault list`, **deny** | Exit not 0, op's error on stderr; the `end` record says `denied` |
| W7 | `$x = op-reason --reason "check W7: capture" whoami --format json`, approve | `$x` holds the JSON text |
| W8 | Press Ctrl+C while the prompt of `op-reason --reason "check W8: interrupt" vault list` is open | The window disappears; deny the prompt; the `end` record says `interrupted` |

## Checks of the SSH relay

`op-reason.exe --ssh-agent` in front of the 1Password SSH agent
([2026-11-17-ssh-relay-design.md](superpowers/specs/2026-11-17-ssh-relay-design.md)). CI runs it with a fake agent and
clicks its buttons, but cannot show 1Password's prompts. These take about 40 minutes and show five to eight prompts of
1Password. They connect to GitHub with `ssh -T git@github.com` (it authenticates with your key and prints
`Hi <user>!`, then closes: nothing on GitHub changes). Where a step says "note", nobody has seen it yet.

Preparation:

1. **The Windows zip of this pull request**: in the pull request's checks, open the run of "Windows exe", download the
   artifact `op-reason-windows`, and unpack the `op-reason-windows.zip` in it into a new folder, for example
   `$dir = "$env:LOCALAPPDATA\Programs\op-reason-ssh-check"` (or `gh run download <run id> -n op-reason-windows -D $dir`
   and unpack). Check it with `& "$dir\op-reason-windows\op-reason.exe" --version`, and below call
   `$exe = "$dir\op-reason-windows\op-reason.exe"`.
2. **WSL**: build this branch as in the preparation above and add `ln -sf op-reason target/release/op-reason-ssh`.
   Write a script for the loop of S4: `printf '%s\n' 'for i in 1 2 3 4 5; do op-reason-ssh -o "IdentityAgent=$PIPE" -T git@github.com; done' > /tmp/s4.sh`
   and `export PIPE='\\.\pipe\op-reason-ssh-agent'`.
3. In Windows PowerShell or PowerShell 7, the same pipe: `$pipe = '\\.\pipe\op-reason-ssh-agent'`. Nothing else may
   serve that name (stop a relay of an earlier try).
4. 1Password: **Settings → Developer → Use the SSH agent** is on, a key that GitHub knows is in it, and **Ask approval
   for each new** is **application and terminal session** (put it back so after S10).
5. **Start the relay** in a PowerShell window that stays open (its parent must stay alive): `& $exe --ssh-agent`. It
   prints `the SSH relay listens on \\.\pipe\op-reason-ssh-agent (per-call) and passes requests on to …` and stays;
   Ctrl+C stops it. Run the checks in a second PowerShell window and in WSL.

| # | Do | Expect |
| --- | --- | --- |
| S1 | In the second PowerShell: `$env:OPREASON_SSH_REASON = 'check S1: who am I on GitHub'; ssh -o "IdentityAgent=$pipe" -T git@github.com`. Click **Allow this call**, then approve 1Password's prompt | op-reason's window "SSH key: allow this call?" at the top right with Caller "Human (terminal) · pwsh.exe" (or powershell.exe), Root "ssh.exe (pid …)", the reason, the command, Destination "git@github.com · host key SHA256:…", For "logging in as git", Key "<the key's name> · SHA256:…" and three buttons (Allow this call, Allow this terminal for 60 minutes, Deny). After the click its title becomes "SSH key: 1Password is asking" until 1Password answers. **1 prompt** (it names op-reason.exe); then `Hi <user>!` (exit 1 is GitHub's). If Caller says "Program: pwsh.exe" instead, note the shell's command line (`(Get-CimInstance Win32_Process -Filter "ProcessId=$PID").CommandLine`): it was not taken for an interactive shell, and in S2 the call then passes without a window |
| S2 (C1) | Run S1's command again; click **Allow this call** | The window again (a new call); **no 1Password prompt**: it passes within a second |
| S3 (C1) | In WSL: `OPREASON_SSH_REASON='check S3 from WSL' op-reason-ssh -o "IdentityAgent=$PIPE" -T git@github.com`; click **Allow this call** | The window has a WSL line (the distribution), a herdr line inside herdr, Caller "Human (terminal) · <your shell>", and the reason; **no prompt** |
| S4 | In WSL: `OPREASON_SSH_REASON='check S4: five connections' bash /tmp/s4.sh`; click **Allow this call** once | One window for the five connections (Root "bash (pid …) · bash /tmp/s4.sh"); five `Hi <user>!`; no prompt. Run it again: one window again (a new call) |
| S5 | As S4 with `check S5: deny`; click **Deny** | All five fail (`Permission denied (publickey)`) without more windows, at once after the click |
| S6 (optional) | With ansible, a playbook for a host that takes your key: `ANSIBLE_SSH_EXECUTABLE=op-reason-ssh OPREASON_SSH_REASON='check S6: playbook' ansible-playbook …` (the pipe through `-o IdentityAgent=…` in `ansible_ssh_common_args`); Allow; run it again | One window per run, whatever the number of tasks and hosts |
| S7 | In the second PowerShell, without a reason: `ssh -o "IdentityAgent=$pipe" -T git@github.com`; click **Allow this terminal for 60 minutes**; run it again; then run it in a new PowerShell tab | Reason "no reason given". The second run passes without a window; the new tab gets the window again (Deny it) |
| S8 | S1's command with `check S8: no answer`; do not click for 60 seconds | The window closes after 60 seconds and ssh fails; running it again asks again |
| S9 (C4) | `ssh -A -o "IdentityAgent=$pipe" <a host of yours that allows agent forwarding>` (Allow its own call), then on that host `ssh -T git@github.com` twice | Each signature on the host gets a window: Caller "unknown (forwarded from a remote host)", Destination "forwarded from <host> (host key …) · onward to the host with key …", footer "this answer covers this signature only"; Allow each. No prompt |
| S10 (C2) | Stop the relay (Ctrl+C). Set 1Password's **Ask approval for each new** to **request**. Start `& $exe --ssh-agent --mode show-only` and run S1's command (`check S10: request`) twice, approving each prompt | For each run, op-reason's window "SSH key: 1Password is asking" (no buttons, footer "show-only mode: decide in 1Password's prompt …") and one 1Password prompt at the same time: **2 prompts**. Then set 1Password back to **application and terminal session** |
| S11 | With the show-only relay still running: run S1's command (`check S11: default`) twice (approve a prompt if one comes) | The second signature passes in under half a second: one toast "SSH key: 1Password did not ask" (only once); `& $exe --log` shows "SSH: 1Password did not ask (show-only)" |
| S12 | Stop the relay and start `& $exe --ssh-agent` again. Lock 1Password. Run S1's command (`check S12: locked`); click **Allow this call**; unlock 1Password when it asks (approve the relay if it asks) | While 1Password asks to unlock, op-reason's window stays as "SSH key: 1Password is asking"; it closes when 1Password answers. Note whether 1Password also asked to approve the relay after the unlock |
| S13 (optional) | Point a terminal program's own ssh connection at the relay (`IdentityAgent` for its host in `~/.ssh/config`), connect, then make it reconnect | The first connection asks (Caller "Program: <its exe>"); its reconnects pass until the program exits. Note what the program shows while the window waits |
| S14 (C3) | In WSL, in a scratch repository with commit signing through 1Password (`gpg.format ssh`, `gpg.ssh.program` = `op-ssh-sign-wsl.exe`): `git commit --allow-empty -m 'check S14'` | Note whether op-reason's window appears (purpose "a signature (SSHSIG, namespace git)"). If 1Password's prompt comes without it, signing does not use the agent pipe and stays outside the relay |
| S15 | Stop the relay. In PowerShell: `cmd /c start "" /b $exe --ssh-agent` (cmd exits at once, the relay stays without a parent). Within a few seconds note the toast; then run S1's command (`check S15: no parent`) and Allow | A toast "op-reason's SSH relay has no parent process" or "… lost its parent process". Note whether 1Password asks; when it does not answer within 60 seconds, ssh fails and a toast says "SSH key: 1Password did not answer …" (with the cause). Stop that relay: `Stop-Process -Id ((& $exe --ssh-agent --status) -replace '.*pid (\d+).*', '$1')` |
| S16 | `& $exe --log --since 2h`, then `& $exe --log --json --since 2h` | One line per signature among the `op` calls ("SSH allowed", "SSH passed", "SSH denied", "SSH timed out", "SSH signed", "SSH no answer from 1Password"), with caller and reason (a passed row shows those of its call); the relay's own lines (started, its parent, 1Password did not ask). The JSON has neither the data signed nor a signature |

Put everything back: stop the relay, 1Password's setting at **application and terminal session**, lock 1Password once.

## Open question: does plain `signout` close only one process's session?

op-reason ends each call with `op.exe signout --all`, which also closes sessions of other processes (for example an
`op` typed by hand in another tab). If plain `signout` (without `--all`) closes only the calling process's session,
op-reason could use it instead. To find out (in two Windows PowerShell tabs):

1. Tab 1: `op.exe vault list > $null`, approve. Then `op.exe whoami` → signed in.
2. Tab 2: `op.exe vault list > $null`, approve. Then `op.exe signout` (no `--all`).
3. Tab 1: `op.exe whoami` again. Signed in → plain `signout` is per process. Not signed in → it is not.
4. Run `op.exe signout --all` in either tab and lock 1Password.

## Recording the results

Note the date, the versions (`op-reason --version`, `op.exe --version`, Windows build) and, for each check, whether it
matched (and what happened when it did not). Open an issue with the results that did not match.
