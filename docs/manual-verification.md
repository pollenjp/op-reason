# Checking op-reason with the real 1Password app

日本語版: [manual-verification.ja.md](manual-verification.ja.md)

CI runs op-reason with fake `op-reason.exe`, `powershell.exe` and `op.exe` only: the 1Password desktop app cannot run
there, and the Windows runners have no WSL2. These checks need a person, a Windows machine with the 1Password app and
the 1Password CLI, and WSL. The WSL checks (1–14 and R) take about 30 minutes and show about fifteen approval prompts.
The checks of the references (F) add about ten minutes, the checks of the SSH relay (S) about an hour, the checks of
the settings (G) about twenty minutes, the checks of the look (V) about fifteen minutes.

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
clicks its buttons, but cannot show 1Password's prompts. These take about an hour and show ten to thirteen prompts of
1Password. They connect to GitHub with `ssh -T git@github.com` (it authenticates with your key and prints
`Hi <user>!`, then closes: nothing on GitHub changes). Where a step says "note", nobody has seen it yet. The results of
the runs so far are below the table.

Preparation:

1. **The Windows zip of this pull request**: in the pull request's checks, open the run of "Windows exe", download the
   artifact `op-reason-windows`, and unpack the `op-reason-windows.zip` in it into a new folder, for example
   `$dir = "$env:LOCALAPPDATA\Programs\op-reason-ssh-check"` (or `gh run download <run id> -n op-reason-windows -D $dir`
   and unpack). Check it with `& "$dir\op-reason-windows\op-reason.exe" --version`, and below call
   `$exe = "$dir\op-reason-windows\op-reason.exe"`.
2. **WSL**: build this branch as in the preparation above and add `ln -sf op-reason target/release/op-reason-ssh`.
   In a WSL shell kept for these checks (its Linux `ssh` and `ssh-add` lose their agent), hand the relay's pipe to
   `ssh.exe`:
   `export SSH_AUTH_SOCK='\\.\pipe\op-reason-ssh-agent' WSLENV="${WSLENV:+$WSLENV:}SSH_AUTH_SOCK"`. Write a script for
   the loop of S4: `printf '%s\n' 'for i in 1 2 3 4 5; do op-reason-ssh -T git@github.com; done' > /tmp/s4.sh`.
3. In a second PowerShell window: `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'`. Nothing else may serve that
   name (stop a relay of an earlier try).
4. 1Password: **Settings → Developer → Use the SSH agent** is on, a key that GitHub knows is in it, and **Ask approval
   for each new** is **application and terminal session** (put it back so after S10 and S18).
5. **Start the relay** in a PowerShell window that stays open (its parent must stay alive): `& $exe --ssh-agent`. It
   prints `the SSH relay listens on \\.\pipe\op-reason-ssh-agent (per-call) and passes requests on to …` and stays;
   Ctrl+C stops it. Run the checks in a second PowerShell window and in WSL.

| # | Do | Expect |
| --- | --- | --- |
| S1 | In the second PowerShell: `$env:OPREASON_SSH_REASON = 'check S1: who am I on GitHub'; ssh -T git@github.com`. Click **Allow this call**, then approve 1Password's prompt | op-reason's window "SSH key: allow this call?" at the top right with Caller "Human (terminal) · pwsh.exe" (or powershell.exe), Root "ssh.exe (pid …)", the reason, the command, Destination "git@github.com · host key SHA256:…", For "logging in as git", Key "<the key's name> · SHA256:…" and three buttons (Allow this call, Allow this terminal for 60 minutes, Deny). After the click its title becomes "SSH key: 1Password is asking" until 1Password answers. **1 prompt**, which names the terminal program the relay was started in, not op-reason.exe; then `Hi <user>!` (exit 1 is GitHub's). If Caller says "Program: pwsh.exe" instead, note the shell's command line (`(Get-CimInstance Win32_Process -Filter "ProcessId=$PID").CommandLine`): it was not taken for an interactive shell, and in S2 the call then passes without a window |
| S2 (C1) | Run S1's command again, this time through the configuration form instead of `SSH_AUTH_SOCK`: `Remove-Item Env:SSH_AUTH_SOCK; ssh -o 'IdentityAgent=\\\\.\pipe\op-reason-ssh-agent' -T git@github.com; $env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'`; click **Allow this call** | The window again (a new call); **no 1Password prompt**: it passes within a second. This also shows that `IdentityAgent` with the leading pair of backslashes doubled reaches the relay; if ssh says it cannot connect to the agent instead, note it |
| S3 (C1) | In WSL: `OPREASON_SSH_REASON='check S3 from WSL' op-reason-ssh -T git@github.com`; click **Allow this call** | The window has a WSL line (the distribution), a herdr line when the shell runs inside herdr (`HERDR_ENV=1`), Caller "Human (terminal) · <your shell>", and the reason; **no prompt** |
| S4 | In WSL: `OPREASON_SSH_REASON='check S4: five connections' bash /tmp/s4.sh`; click **Allow this call** once | One window for the five connections (Root "bash (pid …) · bash /tmp/s4.sh"); five `Hi <user>!`; no prompt. Run it again: one window again (a new call) |
| S5 | As S4 with `check S5: deny`; click **Deny** | All five fail (`Permission denied (publickey)`) without more windows, at once after the click |
| S6 (optional) | With ansible, a playbook for a host that takes your key: `ANSIBLE_SSH_EXECUTABLE=op-reason-ssh OPREASON_SSH_REASON='check S6: playbook' ansible-playbook …`; Allow; run it again | One window per run, whatever the number of tasks and hosts. Note how many windows without buttons and toasts come while the forks run |
| S7 | **Run S8 first**: after the terminal answer of S7, ssh typed in that terminal passes without a window, S8's command too. In the second PowerShell, without a reason (`Remove-Item Env:OPREASON_SSH_REASON`): `ssh -T git@github.com`; click **Allow this terminal for 60 minutes**; run it again; then `git ls-remote git@github.com:<you>/<a repository of yours>.git` with Git for Windows pointed at Windows OpenSSH (`$env:GIT_SSH = (Get-Command ssh.exe).Source`; Git's own ssh cannot reach the pipe); then run the ssh command in a new PowerShell tab (with `SSH_AUTH_SOCK` set there too) | Reason "no reason given". The second run and `git ls-remote` pass without a window (commands typed in that terminal); the new tab gets the window again (Deny it) |
| S8 | Before S7 (or in a new PowerShell tab with `SSH_AUTH_SOCK` set): S1's command with `check S8: no answer`; do not click for 60 seconds | The window closes after 60 seconds and ssh fails; running it again asks again |
| S9 (C4) | `ssh -A <a host of yours that allows agent forwarding>` (Allow its own call; after S7 it passes on the terminal answer), then on that host `ssh -T git@github.com` twice | Each signature on the host gets a window: Caller "unknown (forwarded from a remote host)", Destination "forwarded from <host> (host key …) · onward to the host with key …", footer "this answer covers this signature only"; Allow each. No prompt |
| S10 (C2) | Stop the relay (Ctrl+C). Set 1Password's **Ask approval for each new** to **request**. Start `& $exe --ssh-agent --mode show-only` and run S1's command (`check S10: request`) twice, approving each prompt | For each run, op-reason's window "SSH key: 1Password is asking" (no buttons, footer "show-only mode: decide in 1Password's prompt …") and one 1Password prompt at the same time: **2 prompts**. Then set 1Password back to **application and terminal session** |
| S11 | With the show-only relay still running: run S1's command (`check S11: default`) twice (approve a prompt if one comes). If neither a prompt nor an answer comes (it happened once right after 1Password's setting was changed), wait for the 60-second timeout and run it again | The second signature passes in under half a second: one toast "SSH key: 1Password did not ask" (only once); `& $exe --log` shows "SSH: 1Password did not ask (show-only)" |
| S12 | Stop the relay and start `& $exe --ssh-agent` again. Lock 1Password. Run S1's command (`check S12: locked`); click **Allow this call**; answer 1Password's access request (Windows Hello) | One access request unlocks 1Password and approves the relay at once (no second prompt). Until then op-reason's window stays as "SSH key: 1Password is asking"; it closes when 1Password answers |
| S13 (optional) | Point a terminal program's own ssh connection at the relay (`IdentityAgent \\\\.\pipe\op-reason-ssh-agent` for its host in `~/.ssh/config`), connect, then make it reconnect | The first connection asks (Caller "Program: <its exe>"); its reconnects pass until the program exits. Note what the program shows while the window waits |
| S14 (C3) | In WSL, in a scratch repository with commit signing through 1Password (`gpg.format ssh`, `gpg.ssh.program` = `op-ssh-sign-wsl.exe`, `commit.gpgsign true`) and the key set in the repository: `git config user.signingkey "key::$(ssh-add.exe -L \| grep '<the key's name>' \| head -n1)"` (in the shell of the preparation, Linux's `ssh-add` has no agent, so a `gpg.ssh.defaultKeyCommand` that asks it finds no key). Then `git commit --allow-empty -m 'check S14'` | No window of op-reason and no record of the relay: `op-ssh-sign-wsl.exe` does not use `SSH_AUTH_SOCK` and talks to the 1Password app directly, so signing stays outside the relay. Only 1Password's prompt comes, and the commit is signed (`git cat-file -p HEAD` shows `gpgsig -----BEGIN SSH SIGNATURE-----`) |
| S15 | Stop the relay. In PowerShell: `cmd /c start "" /b $exe --ssh-agent` (cmd exits at once, the relay stays without a parent). Within a few seconds note the toast; then run S1's command (`check S15: no parent`), Allow and approve 1Password's prompt | A toast "op-reason's SSH relay has no parent process" or "… lost its parent process", which says that 1Password most likely takes the relay itself for the application now, and where it keeps its prompt. After Allow, op-reason's window comes to the front and 1Password's prompt comes at once, naming op-reason (before the window came to the front, 1Password kept that prompt behind its icon, unseen, and the request went unanswered after about 60 seconds: see the results below). Stop that relay: `Stop-Process -Id ((& $exe --ssh-agent --status) -replace '.*pid (\d+).*', '$1')` |
| S16 | `& $exe --log --since 2h`, then `& $exe --log --json --since 2h` | One line per signature among the `op` calls ("SSH allowed", "SSH passed", "SSH denied", "SSH timed out", "SSH signed", "SSH no answer from 1Password"), with caller and reason (a passed row shows those of its call); the relay's own lines (started, its parent, 1Password did not ask). The JSON has neither the data signed nor a signature |
| S17 (optional) | Open WSL through a distribution's own launcher (such as Ubuntu from the Start menu), set `SSH_AUTH_SOCK` and `WSLENV` as in the preparation, and run S3's command (`check S17: launcher`) | Note whether the window has the WSL line. If it does not, note the parent of `ssh.exe` while the window is open: `Get-CimInstance Win32_Process -Filter "Name='ssh.exe'" \| ForEach-Object { (Get-Process -Id $_.ParentProcessId).Path }` |
| S18 | A client that talks to 1Password directly. Start the relay again as in step 5 of the preparation (the relay, the second PowerShell and a third terminal opened for this check must all be terminals of one terminal program). Lock 1Password. In the second PowerShell run S1's command (`check S18: through the relay`), click **Allow this call** and answer 1Password. Then in the third terminal, straight to 1Password's pipe: `ssh -o 'IdentityAgent=\\\\.\pipe\openssh-ssh-agent' -T git@github.com`; deny the prompt that comes. Then set 1Password's **Ask approval for each new** to **application**, lock 1Password and run both commands again in the same terminals (as in S11, if a request right after the change gets no answer, wait for the timeout and run it again). Finally `& $exe --log --since 30m` | At **application and terminal session**: the direct client gets 1Password's prompt, naming the terminal program, without op-reason's window; denied, ssh says `agent refused operation`. At **application**: once the relay is approved, the direct client passes without a prompt (`Hi <user>!`). The log shows the two calls through the relay and nothing of the direct client. Then set 1Password back to **application and terminal session** |
| S19 | In WSL, with that terminal in front: `OPREASON_SSH_REASON='check S19: toasts' bash /tmp/s4.sh` (five connections of one call); click **Allow this call** once | The first signature has the window and no toast. The other four pass without a window, in one card at the top left of the screen of the WSL terminal, 12 DIPs from the work area's edges: `Passed · Human`, a green tag ×2 to ×4 as they come, line 2 `ssh.exe … → git@github.com` (the command cut with "…" so that the destination stays whole), line 3 `allowed for this call · check S19: toasts`. The card is drawn at 90% opacity: the text behind it shows through faintly, through the face, the text and the lines alike. At each signature a green ring lights up around it (the card itself stays at 90%), and a thin green line shortens under it. About four seconds after the last one it fades out in under a second. While it is shown, keys go to the terminal and a click on the card reaches the window under it. Nothing is added to the notification center (Win+N) |
| S20 | In a new PowerShell tab (no reason set there): `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; ssh -T git@github.com`, and click **Allow this terminal for 60 minutes**. Then, in that tab, four commands at once: `1..4 \| ForEach-Object { Start-Process -NoNewWindow ssh.exe '-T git@github.com' }`. When the cards have gone, ten at once: `1..10 \| ForEach-Object { Start-Process -NoNewWindow ssh.exe '-T git@github.com' }` | The four commands are four calls that the terminal answer covers: four cards, the newest at the top and the others moving down; the fourth makes the first go at once (never more than three), and a small `+1` comes under the third card until about four seconds after the first one's signature. Each says `Passed · Human` and `allowed for this terminal · no reason given`, and fades four seconds after its signature. The ten commands: three cards and `+7` under the third; the number goes down as the four seconds of each card pushed out pass (they came a moment apart) and is gone before the three cards fade |
| S21 | Turn Windows' animation effects off (Settings → Accessibility → Visual effects → Animation effects), run S20's four commands again in its tab, then turn the effects back on | The cards appear at 90% at once, move and go without sliding, fading or glowing; `+1` comes and goes at once too |
| S22 (optional, two screens) | Move the WSL terminal of the preparation to the screen that is not the main one and click into it. Run `OPREASON_SSH_REASON='check S22: the other screen' bash /tmp/s4.sh` and click **Allow this call**; then, in that terminal, `op --reason "check S22: op window" vault list >/dev/null`, approve | op-reason's windows (the relay's and the `op` call's) open at the top right of that screen, and the toast of the four later signatures at its top left. Note on which screen 1Password's prompt for the `op` call appears (design 2026-11-18 §5: if it is not that screen, the choice of the screen is to be discussed) |
| S23 | In a new PowerShell tab: `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; $env:OPREASON_SSH_REASON = 'check S23: notification'; ssh -T git@github.com`. While the window is open, look at the bottom right; click **Deny**; open the notification center (Win+N). Then in WSL `op --reason "check S23: op" vault list >/dev/null`, approve, and open the notification center again | A Windows notification "SSH key: allow this call?" comes with the window and goes with it at the click: the notification center has nothing of it. The same for the `op` call once its window has closed. What stays from the earlier checks: the notification of S8's time-out and the one-time notices (S11, S15) |
| S24 | In a new PowerShell tab: `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; $env:OPREASON_SSH_REASON = 'check S24: unanswered'; ssh -T git@github.com`; do not click for 60 seconds; open the notification center | The window closes after 60 seconds and ssh fails; its notification stays in the notification center, the sign of a request that went unanswered |

Put everything back: stop the relay, 1Password's setting at **application and terminal session**, lock 1Password once.

### The window in front after Allow (S25–S31)

After **Allow this call** or **Allow this terminal**, op-reason's window comes to the front until 1Password answers,
so that 1Password shows its prompt at once
([2026-11-22-front-after-allow-design.md](superpowers/specs/2026-11-22-front-after-allow-design.md)). 1Password keeps
the prompt of an application that it does not see in front behind its icon at the right of the taskbar (a dot on the
icon, **SSH request waiting** in its menu), and for a relay started from a scheduled task that application is
op-reason. S25–S28 and S31 need such a relay: stop the relay of the preparation (Ctrl+C), then in PowerShell register a
task without a trigger (it runs when started) and start it:

```powershell
$a = New-ScheduledTaskAction -Execute $exe -Argument '--ssh-agent'
$s = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -Hidden
Register-ScheduledTask -TaskName op-reason-ssh-check -Action $a -Settings $s | Out-Null
Start-ScheduledTask -TaskName op-reason-ssh-check
& $exe --ssh-agent --status
```

Lock 1Password and unlock it before S25, so that it has approved nothing yet. If op-reason's window stops answering
(white inside, "Not Responding" in its caption), run `& $exe --log --since 10m` before anything else and note the line
"SSH: op-reason's window stuck (<step>)": the step its thread waits in.

| # | Do | Expect |
| --- | --- | --- |
| S25 | In the second PowerShell, with that terminal in front: `$env:OPREASON_SSH_REASON = 'check S25: from a task'; ssh -T git@github.com`. While op-reason's window is open, type a few letters (no Enter); click **Allow this call**; approve 1Password's prompt | The window opens without taking the focus: the letters go to the terminal. At the click its caption turns active (it has the focus), and 1Password's prompt comes at once, without a dot on the 1Password icon, naming op-reason, not the terminal program. After the approval `Hi <user>!`, and the terminal has the focus again (type to see it). If it does not, note which window has it |
| S26 | S25's command with `check S26: again`; Allow | The window again (a new call), which comes to the front at the click; no 1Password prompt: it passes at once, and the terminal has the focus again right after |
| S27 | S25's command with `check S27: deny`; click **Deny** | The window never takes the focus (its caption stays inactive); ssh fails at once |
| S28 | Start the task's relay again: `Stop-Process -Id ((& $exe --ssh-agent --status) -replace '.*pid (\d+).*', '$1'); Start-ScheduledTask -TaskName op-reason-ssh-check`. Then S25's command with `check S28: a new relay`; Allow; approve | 1Password asks again (a new process of the relay needs its own approval), and its prompt comes at once, naming op-reason |
| S29 | Stop the task's relay (`Stop-Process` as in S28) and start the relay in a terminal, as in step 5 of the preparation. In another terminal of the same terminal program, with that program in front: S1's command with `check S29: started in a terminal`; Allow; approve | op-reason's window does not take the focus at the click: the program that started the relay is in front. 1Password's prompt comes at once, naming that terminal program, as in S1 |
| S30 (optional) | With the relay of S29, lock 1Password and unlock it (or the request passes on S29's approval, without a prompt). S1's command with `check S30: another program in front`. Before clicking Allow, click an empty part of the taskbar (or another program's window); then Allow | op-reason's window comes to the front at the click, and 1Password's prompt comes at once, naming the terminal program: 1Password counts op-reason's window, a process under the terminal program that it takes for the application (seen on 2026-11-22). If the prompt waits behind the icon instead, note it |
| S31 | Stop the relay of S29 (Ctrl+C) and have the task start a show-only relay: `Set-ScheduledTask -TaskName op-reason-ssh-check -Action (New-ScheduledTaskAction -Execute $exe -Argument '--ssh-agent --mode show-only') \| Out-Null; Start-ScheduledTask -TaskName op-reason-ssh-check`. Lock 1Password and unlock it. S1's command with `check S31: show-only from a task`; leave it for 60 seconds. Run it again, and this time open the prompt from the 1Password icon and approve | op-reason's window "SSH key: 1Password is asking" never takes the focus; its last line reads "If 1Password's prompt does not come: the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting". The icon has a dot. Left alone, the request closes after about 60 seconds, ssh fails, and the toast "SSH key: 1Password did not answer" says that 1Password keeps such a prompt behind its icon (SSH request waiting), not that the relay needs a task. The second time, the prompt opened from the icon names op-reason, and `Hi <user>!` |

Put everything back: remove the task, stop the relay if it still runs, and lock 1Password once.

```powershell
Stop-ScheduledTask -TaskName op-reason-ssh-check
Unregister-ScheduledTask -TaskName op-reason-ssh-check -Confirm:$false
if ((& $exe --ssh-agent --status) -match 'pid (\d+)') { Stop-Process -Id $Matches[1] }
```

### The guide after Allow (S32–S36)

After **Allow**, op-reason's window watches for 1Password's prompt
([2026-11-26-prompt-guide-design.md](superpowers/specs/2026-11-26-prompt-guide-design.md)): when a window of 1Password
comes, it says "Approve in 1Password's window"; when none comes within two seconds, it lights up, counts down about 60
seconds, and a small window next to the notification area points at the 1Password icon (SSH request waiting). The case
that led to it: 1Password had just been started again and was locked, and it kept the prompt behind its icon although
op-reason's window had been clicked. These checks need a per-call relay started from a scheduled task, as in S25.
With the relay of S29 stopped (Ctrl+C) and the task of S25–S31 removed, register and start it again:

```powershell
$a = New-ScheduledTaskAction -Execute $exe -Argument '--ssh-agent'
$s = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -Hidden
Register-ScheduledTask -TaskName op-reason-ssh-check -Action $a -Settings $s | Out-Null
Start-ScheduledTask -TaskName op-reason-ssh-check
& $exe --ssh-agent --status
```

The last record of a signature: `Get-Content "$env:LOCALAPPDATA\op-reason\ssh-log.jsonl" | Select-Object -Last 1`.

| # | Do | Expect |
| --- | --- | --- |
| S32 | Quit 1Password (its icon's menu → Quit) and start it again; leave it locked. In the second PowerShell: `$env:OPREASON_SSH_REASON = 'check S32: locked'; ssh -T git@github.com`; click **Allow this call**, and do nothing else for five seconds | Note what comes: 1Password's unlock or its prompt in front (then op-reason's window says "Approve in 1Password's window" with a sketch of a dialog), or, within two seconds of the click, op-reason's window lit up (a yellow border, "1Password's prompt did not come to the front", the seconds left) and the pointer next to the notification area, its arrow at it. Then open the prompt from the 1Password icon as the pointer says (under ^ when hidden), unlock and approve: `Hi <user>!`, and the pointer goes with the window. Note the record's `prompt` (`front`, `seen`, `hint`) |
| S33 | Lock 1Password and unlock it (it asks again for the relay). S32's command with `check S33: unlocked`; Allow; before approving, look at op-reason's window and where 1Password's prompt opened | 1Password's prompt comes at once; op-reason's window says "Approve in 1Password's window" with the sketch, without a border or a pointer. Note whether the prompt opened in the middle of the screen, as the bands say. The record: `"seen"` under 2, `"hint":null` |
| S34 | Quit and start 1Password again, locked. S32's command with `check S34: left alone`; Allow; open nothing for 70 seconds | The pointer and the lit window count down from about 58; when 1Password closes the request (about 60 seconds after the click), the window and the pointer go, ssh fails, and the toast "SSH key: 1Password did not answer" comes. The record: `no-answer`, `"seen":null`, a `"hint"` |
| S35 | S33 again, but click another window (the desktop) right after Allow, so that 1Password's prompt may open behind it | If the prompt still comes to the front: as S33. If not: the pointer within two seconds; note what 1Password showed (a dot on its icon, a window in the taskbar) |
| S36 (optional) | Move the taskbar to the top (or a side) where Windows allows it, and repeat S34 for ten seconds | The pointer sits next to the notification area there, its arrow at it, inside the screen |

Put everything back as after S31 (the block above): remove the task, stop the relay if it still runs, and lock
1Password once.

### A call of Claude Code's PowerShell tool, and the process tree (S37–S41)

Claude Code's PowerShell tool on Windows runs its script through a launcher (`cmd.exe` running `chcp.com 65001` and
then PowerShell, the script in an environment variable), whose line holds no command: the window tells that launcher,
shows the ssh command in its box on the ssh process's row of the tree, and asks for each ssh
([2026-11-26-modern-window-design.md](superpowers/specs/2026-11-26-modern-window-design.md) §4.7, rule 3, and §13;
[2026-11-27-relay-process-tree-design.md](superpowers/specs/2026-11-27-relay-process-tree-design.md)). It needs a
per-call relay on `\\.\pipe\op-reason-ssh-agent` (step 5 of the preparation, or the task of S32) and Claude Code on
Windows with its PowerShell tool; S40 needs WSL with `op-reason-ssh`.

| # | Do | Expect |
| --- | --- | --- |
| S37 | Ask Claude Code to run, in one call of its PowerShell tool, `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; $env:OPREASON_SSH_REASON = 'check S37: PowerShell tool'; ssh -T git@github.com`. Click **Allow this ssh** and approve 1Password's prompt | The title reads "SSH key: allow this ssh?". Under "Process tree": Claude Code (`claude.exe · pid …`, a purple dot) → `cmd.exe` (pid …) with the pill "Claude Code's PowerShell tool" and the mark "Root" → `pwsh.exe` (or `powershell.exe`) → `ssh.exe` with the box `ssh.exe -T git@github.com` under it. A dashed frame "this call" runs from `cmd.exe` down, a solid frame "this ssh" holds `ssh.exe` and its box. The buttons: **Allow this ssh** (filled), **Allow this call**, **Deny**; after the seconds left, "Allowing covers the frame you choose (this ssh or this call) until it ends (at most 60 minutes)". `Hi <user>!` comes back to Claude Code. The record: `"answer":"allow-ssh"`, and `call.root.cmd` holds the launcher's line. If the root's row shows `chcp.com 65001 >nul &` in a box instead, note `claude --version`: the launcher may have changed |
| S38 | As S37, with two ssh in the one call: `…; ssh -T git@github.com; ssh -T git@github.com`. Click **Allow this ssh** in the first window, then in the second | The second ssh asks in a window of its own (another `ssh.exe` PID in its tree) after the first has signed; both print `Hi <user>!`. Two records `allowed` with `"answer":"allow-ssh"`, and no `passed` |
| S39 | S38 again, but click **Allow this call** in the first window | The second ssh signs without a window; a toast at the top left: "Passed · Claude Code", the ssh command, "allowed for this call". The second record: `passed`, `"by":"allow"`, its `decision` the first record's `id`. `op-reason.exe --log` shows both |
| S40 | In WSL, ask Claude Code to run, in one call of its Bash tool, `OPREASON_SSH_REASON='check S40: WSL tree' op-reason-ssh -T git@github.com`; look at the window, then **Allow this call** | The tree: Claude Code (`pid …`, the PID on Linux) → the root (the shell that runs the tool's command, the mark "Root", the box with the command as Claude Code ran it, the value after `=` masked, the rest of the root's line small under the box) → "… in WSL, not seen" → `ssh.exe` (its Windows PID) with the ssh command small under it; one solid frame "this call" from the root down; the buttons **Allow this call** and **Deny**, as before |
| S41 | In Windows Terminal (PowerShell), type `$env:OPREASON_SSH_REASON = 'check S41: terminal tree'; ssh -T git@github.com`; look at the window in the dark theme, switch Windows to the light theme while it is open (Settings → Personalization → Colors), then **Deny** | The tree: Human (terminal) (`pwsh.exe · pid …`, a green dot) → `ssh.exe` (the mark "Root", the box with the command); a solid frame "this call" around `ssh.exe` and a dashed frame "this terminal" from the shell down; the buttons **Allow this call**, **Allow this terminal for 60 minutes**, **Deny**. The tree's lines, frames and texts stay readable in both themes |

### Results of 2026-11-18

op-reason 0.3.0 from this pull request (Windows: the zip of its Windows exe workflow; WSL: a release build of the
branch), the 1Password CLI 2.41.1, the 1Password app 8.12.46.29 (Microsoft Store), Windows OpenSSH 10.4p1, Windows 11
build 26300. 1Password at **application and terminal session**; at **request** for S10, and at **application** for the
second half of S18, which was added and run after the others. The relay and the shells ran in a terminal program
(PowerShell 7 and the bash of a WSL distribution); the relay's PowerShell stayed open. The steps and expectations above
are corrected where the run showed otherwise.

| # | Result | What happened |
| --- | --- | --- |
| S1 | As expected | All the lines and the three buttons. One prompt, which named the terminal program, not op-reason.exe (the expectation said op-reason.exe) |
| S2 | As expected | `IdentityAgent` with the doubled pair reached the relay; no prompt. The first two tries timed out after 60.2 seconds: the window, at the top right of the main display, went unnoticed |
| S3 | As expected | The WSL line, Caller "Human (terminal) · bash" and the reason; no prompt. No herdr line: the shell was not inside herdr |
| S4 | As expected | One window for the five connections (Root `bash /tmp/s4.sh`); the other four passed on the same decision in 0.1 seconds |
| S5 | As expected | One Deny; the other four were denied on the same decision at once, all five `Permission denied` within 5 seconds of the click |
| S6 | Skipped | No ansible |
| S7 | As expected | No reason; after the terminal answer, ssh and `git ls-remote` passed without a window; a new tab got the window again (denied). Git for Windows had to be pointed at Windows OpenSSH (now in the step) |
| S8 | As expected | Run before S7 (in the same terminal after S7 it passes on the terminal answer; now in the step). The window closed after 60 seconds and ssh failed; the next run asked again (seen with S2's timeouts too) and was denied |
| S9 | As expected | The login to the host that allows agent forwarding passed on S7's terminal answer. The two signatures from that host each got a window (Caller "unknown (forwarded from a remote host)", the keys of the forwarding host and of the next one, this signature only) and were allowed; no prompt |
| S10 | As expected | Both runs: the window without buttons (title and footer of show-only) together with 1Password's prompt: 2 prompts |
| S11 | As expected, on a second try | Right after 1Password's setting was changed back, the first request got neither a prompt nor an answer for 60 seconds; the relay kept its window, showed the timeout toast and failed the request. The next run had the prompt as usual; after the approval the signatures passed in 0.3 and 0.2 seconds, and the record "1Password did not ask (show-only)" and its toast came once. The toast went unnoticed among the other toasts in the notification center |
| S12 | As expected | One access request (Windows Hello) unlocked 1Password and approved the relay at once; until then the window stayed as "1Password is asking", and it closed after the authentication |
| S13 | Skipped | Optional |
| S14 | Outside the relay | The commit was signed, but the relay recorded nothing and showed no window: `op-ssh-sign-wsl.exe` does not use `SSH_AUTH_SOCK` and talks to the 1Password app directly; only 1Password's prompt came. The key had to be set in the repository (now in the step) |
| S15 | As expected | Started with `cmd /c start "" /b`: the record and the toast that the relay has no parent process. After Allow, 1Password showed no prompt; after 59.7 seconds the relay recorded `no-answer`, ssh failed, and the toast "1Password did not answer" gave the cause |
| S16 | As expected | One line per signature, with every result seen: allowed, allowed (terminal), allowed (forwarded), passed, denied, timed out, signed, no answer from 1Password; passed rows show the reason of their call; the relay's own lines. The 42 `ssh-*` records of the JSON hold neither the data signed nor a signature |
| S17 | Skipped | Optional |
| S18 | Not asked at application | Added after the first run, and run in three terminals of the same terminal program (the relay, the call through it, the direct client). At application and terminal session, after the relay had been allowed and approved (from locked), the direct client got 1Password's prompt without op-reason's window and was denied (`agent refused operation`). At application, after the same, it passed without a prompt in 2 seconds, and the relay recorded nothing of it. 1Password's prompts named the terminal program, for the relay's request and for the direct one. The README had said that such a client is a new application for 1Password; it now says when one is asked |

At the end the relay was stopped, 1Password's setting checked and 1Password locked once.

### Results of 2026-11-19 and 2026-11-20

op-reason 0.4.0 from the pull request that added S19–S24 (Windows: the zip of its Windows exe workflow; WSL: a release
build of the branch), with the real 1Password app, as in the preparation. The checks ran on three builds of that pull
request:

- 2026-11-19, the first build (head `7bfa01c`: opaque cards, no `+N`): S19, S20, S22 and S23. After S19 the user asked
  for cards that let the text behind show through (70% opacity), and after S20 for the number of the cards pushed out
  (`+N`). S21 was left for the build with these two changes.
- 2026-11-20, the second build (head `d7dd317`: 70% opacity and `+N`): S24, S19 and S20 again, and S21. In S19 the
  cards were too faint, and the opacity became 90%.
- 2026-11-20, the third build (head `3e8c62e`: 90% opacity): S19 again.

S20 and S21 hold for the third build too: neither `+N` nor the cards without motion depend on the opacity.

| # | Result | What happened |
| --- | --- | --- |
| S19 | As expected (third build) | First build (2026-11-19): one window, at the top right of the screen of the foreground window, which was not the primary one, and 1Password's prompt on the same screen (design 2026-11-18 §5); the later signatures came as one card at the top left of that screen (×4), gone about 4.5 seconds after the last one. As expected, but the text behind the card could not be read: the user asked for 70%. Second build, 70% (2026-11-20): one card at the top left, gone 4.6 seconds after the last signature, but too faint (the step now expects 90%). The relay used 3.7% of one core while the card was shown (0.36 seconds of CPU in 9.8). Third build, 90% (2026-11-20): the five connections passed in 14 seconds; one window, at the top right of the screen of the foreground window, and the first signature passed a second after Allow; the later ones came in one card, ×2 to ×4 with the ring at each. Its text could be read, and the text behind showed through |
| S20 | As expected (second build) | Four commands at once: three cards and `+1`. Ten at once: `+7` (`+5`, then `+7` as they came). The window of the first command opened at the top right of the screen of the foreground window and was answered with **Allow this terminal for 60 minutes**. The relay used 3.8% of one core while cards were shown. With the first build, four at once had shown three cards, as expected then |
| S21 | As expected (second build) | With the animation effects off, the cards appeared and went without sliding or fading; the relay used no CPU in the 3.8 seconds they were shown |
| S22 | As expected (first build) | op-reason's windows (the relay's and the `op` call's) at the top right of the screen that is not the main one, the toast at its top left |
| S23 | As expected (first build) | The notification came with the window and went with it at the click; the notification center kept nothing of it, nor of the `op` call's |
| S24 | As expected (second build) | With no foreground window (the desktop), the window opened at the top right of the primary screen. It closed after 60 seconds and ssh failed; its notification stayed in the notification center |

### Results of 2026-11-22

op-reason 0.5.0 from the pull request that added S25–S31 (Windows: the zip of its Windows exe workflow), with the real
1Password app at **application and terminal session**, run by the person who asked for the change with a script that
did the steps and noted the window in front and op-reason's window every 0.2 seconds. The task's relay ran on a pipe of
its own, from a task with a logon trigger and a trigger that repeats every minute; the relay of S29 and S30 was started
in a terminal program. Three builds of that pull request:

- the first build (head `f295109`): the first window of the task's relay stopped answering before it was drawn (white
  inside, "Not Responding"), twice, so no check went on. The relay's windows now draw without the GPU
  ([design](superpowers/specs/2026-11-22-front-after-allow-design.md) §12);
- the second build (head `3f8f0a0`): S25's window waited for a click until its 60 seconds ran out, and no click came
  (the script did not find the window);
- the third build (head `14c0724`, the relay's windows drawn into their device context): S25, S26, S28, S29 and S30.

| # | Result | What happened |
| --- | --- | --- |
| S25 | As expected (third build) | The window opened 1.5 seconds after ssh started, without taking the front from the terminal program. At the click on Allow it was in front at once, and 1Password's prompt came half a second later, naming op-reason, without a trip to the icon. After the approval (Windows Hello) `Hi <user>!`, and the terminal program had the front again |
| S26 | As expected (third build) | No prompt: the window closed right after the click, too soon to be seen in front, and the terminal program kept the front. Before the click, the 1Password icon had no dot |
| S27 | Not run | |
| S28 | As expected (third build) | The relay, stopped with `Stop-Process`, was started again by the task's trigger that repeats every minute, 48 seconds later (not by `Start-ScheduledTask`). 1Password asked again (it had also been locked and unlocked): its prompt came 0.4 seconds after the click, naming op-reason. Before the click, the icon had no dot |
| S29 | As expected (third build) | With the terminal program in front the window did not come forward at the click; 1Password's prompt came 0.6 seconds later, naming the terminal program, which kept the front |
| S30 | Prompt at once (third build) | The first try passed without a prompt: 1Password still held S29's approval of that relay (the step now locks 1Password first). With a new relay in the terminal program and 1Password locked and unlocked, the taskbar clicked, then Allow: the window came to the front, and 1Password's prompt came 0.4 seconds later, naming the terminal program. At the end the front went to the window that had it before the taskbar was clicked |
| S31 | Not run | |

With the third build no window stopped answering (six windows), and 1Password showed no prompt and no dot before a
click. The notification that comes with the window names "Windows PowerShell" as its sender (op-reason borrows that
AppUserModelID). At the end the task was removed and the relays stopped.

## Checks of the settings (G1–G19)

`op-reason --settings` ([2026-11-23-settings-design.md](superpowers/specs/2026-11-23-settings-design.md)). CI makes a
scheduled task under a test name on GitHub's runners, which run as an administrator, and fakes `wsl.exe`; these checks
are for a user without administrator rights, a real logon, a real WSL and the real 1Password. About 30 minutes, with
two logoffs and three 1Password prompts.

Preparation:

1. The Windows zip of this pull request, unpacked as in the preparation of the SSH relay; below `$exe`. Run every
   command in a PowerShell **without administrator rights**, with `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'`.
2. No relay may run: `& $exe --ssh-agent --status` exits 1. **Where another scheduled task starts the relay** (a task
   of your own, or of your dotfiles), turn it off and stop its relay first, and turn it on again after the checks:
   `Disable-ScheduledTask -TaskPath '<its folder>' -TaskName '<its name>'` and
   `Stop-ScheduledTask -TaskPath '<its folder>' -TaskName '<its name>'` (afterwards `Enable-ScheduledTask` and
   `Start-ScheduledTask`).
3. Keep a copy of `%LOCALAPPDATA%\op-reason\settings.json` if there is one.
4. **Skills**: if you do not want op-reason's guide in `%USERPROFILE%\.claude\skills` (or in a WSL distribution's
   `~/.claude/skills`), run G9 with a chosen folder instead (`skill.folder.$env:TEMP\op-reason-skill-check`) and skip
   G10.

| # | Do | Expect |
| --- | --- | --- |
| G1 | `& $exe --settings list` | One line per setting: `relay.autostart off`, `relay.running off`, a line per WSL distribution (those that do not run: "not checked"); the records folder and the accounts at the end. If another task starts a relay (preparation 2), `relay.autostart` names it |
| G2 | `& $exe --settings set relay.autostart on` | `relay.autostart: off → on`; then `& $exe --ssh-agent --status` exits 0. Task Scheduler (**View → Show Hidden Tasks**) has `op-reason-ssh-relay-<your SID>` in its root folder: at log on of your user, and every minute; "Do not start a new instance"; your user, "Run only when user is logged on", not with highest privileges |
| G3 | `& $exe --settings check relay.autostart=on relay.running=on --json; $LASTEXITCODE` | `"same":true`, then 0 |
| G4 | Lock 1Password and unlock it. `$env:OPREASON_SSH_REASON = 'check G4'; ssh -T git@github.com`; **Allow this call**; approve 1Password | As S25: op-reason's window comes to the front at the click and 1Password's prompt comes at once, naming op-reason; `Hi <user>!` |
| G5 | `$env:OPREASON_SSH_REASON = 'check G5'; ssh -T git@github.com`, and while op-reason's window waits, in another PowerShell: `& $exe --settings relay stop; $LASTEXITCODE`. Then click **Allow this call** | `relay.running: not changed (off): the relay has a window open, …`, then 1; the relay still serves: the Allow goes through |
| G6 | `& $exe --settings relay stop`; wait two minutes; `& $exe --ssh-agent --status` | Exit 1 after two minutes: the trigger of every minute does not start a stopped relay. `& $exe --settings get relay.running --json` says `task-off`; `& $exe --log --since 10m` has "SSH relay stopped by op-reason --settings" and a `settings` line with you as the caller |
| G7 | Log off and on again; `& $exe --ssh-agent --status`. Then `& $exe --settings relay start` | Still stopped after the logon. `relay start` answers within 15 seconds |
| G8 | `& $exe --settings set relay.mode show-only`; then an ssh as in G4 | `relay.mode: per-call → show-only` and a line that the relay was started again; `get relay.mode --json` has no `relay-differs`. 1Password asks for the signature (show-only), with op-reason's window showing who and why. `set relay.mode default` to go back |
| G9 | `& $exe --settings set skill.windows on` (or the chosen folder of preparation 4); open the `SKILL.md` it names; in Claude Code on Windows, `/skills` | The guide, with `<!-- placed by op-reason <version>: …` on the line after the closing `---`; Claude Code lists the skill op-reason. `set skill.windows off` removes it (and the empty `op-reason` folder) |
| G10 | In WSL, with this branch's `op-reason`: `op-reason --settings set "skill.wsl.$WSL_DISTRO_NAME" on`, then `off` | `~/.claude/skills/op-reason/SKILL.md` with the mark, then gone. Where `~/.claude/skills` is a symbolic link (home-manager): refused, nothing written |
| G11 | `& $exe --settings` | The prompt comes back at once, and the settings window opens in front, in the language of `lang`; while it reads, **Refresh** is greyed. The relay's line says "Running · pid … · per-call · started by op-reason's task". A second `& $exe --settings` brings the window to the front; a double click on `op-reason.exe` in Explorer does the same (before Windows 11 24H2 its console shows for a moment; N6) |
| G12 | In the window: choose **日本語**, then **English** again | The window speaks Japanese at once, then English; `& $exe --settings get lang` says `ja` in between |
| G13 | In the window: **Place or update** on the Windows row (or the chosen folder of preparation 4), then **Remove**; **Check** on a WSL distribution that does not run | The row's state follows (`placed (<version>)`, `not placed`); the checked distribution starts and its row is read. `& $exe --log --since 10m` has `settings` lines from the window. With the chosen folder of preparation 4, use **Add a folder…** instead: it places the skill as it adds the folder, so the new row says `placed (<version>)` at once and **Place or update** stays greyed (there is nothing to place); **Remove** removes the skill and the row goes (the folder is forgotten) |
| G14 | In the window: change the mode to `show-only`; **Apply**, answer the question with OK; then an ssh as in G4 | The note by Apply says that the relay runs with other values until Apply; after Apply the relay's line says `show-only` and 1Password asks for the signature. Change the mode back and **Apply** again |
| G15 | `& $exe --settings set start-menu on`; press the Windows key and type `op-reason` | The Start menu finds **op-reason**; it opens the settings window (before Windows 11 24H2 its console shows in the taskbar only for a moment; N7). `set start-menu off` removes it |
| G16 | The notification area (on Windows 11 behind `^`): the relay's key | Its tooltip "op-reason SSH relay · per-call". A click opens the settings window; the right-click menu has the relay's line, **Open op-reason settings**, **Open the records folder**, **Restart the relay**, **Stop the relay**, **Hide this icon**. **Restart the relay**: within 15 seconds a relay with another process ID answers (`& $exe --ssh-agent --status` names it); **Hide this icon**: the icon goes at once, and `set relay.tray-icon on` brings it back |
| G17 | Restart Explorer (Task Manager → **Windows Explorer** → **Restart**) | The relay's icon comes back within a few seconds |
| G18 | With the relay running (G2's task), log off and on again | The relay's icon is in the notification area after the logon (the task may start the relay before the taskbar is there) |
| G19 | `& $exe --settings set relay.autostart off`; restore the `settings.json` of preparation 3; turn the other task of preparation 2 on again | The task is gone, no relay of op-reason's task answers, and its icon is gone |

## Checks of the console window (N1–N11)

No console window when no terminal starts `op-reason.exe`
([2026-11-24-relay-no-console-window-design.md](superpowers/specs/2026-11-24-relay-no-console-window-design.md)). CI
watches for console windows on GitHub's runner (Windows Server 2025) and starts the zip's exe on Windows Server 2022;
these checks are for a real logon, Windows Terminal as the default terminal, a real WSL and the real 1Password. About
15 minutes, with one logoff and four 1Password prompts.

Preparation:

1. Windows 11 24H2 or later: `winver` shows build 26100 or later. (On an older Windows, N1–N4 and N6–N7 show a console
   window for a moment, as before, and the `start` records say `console: own`.)
2. The Windows zip of this pull request, unpacked as in the preparation of the SSH relay; below `$exe`. Run every
   command in a PowerShell **without administrator rights**, with `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'`.
3. No relay may run: `& $exe --ssh-agent --status` exits 1. **Where another scheduled task starts the relay** (a task
   of your own, or of your dotfiles), turn it off and stop its relay first, as in preparation 2 of the settings'
   checks, and turn it on again in N11: that task's relay runs the exe the task names, not this pull request's.
4. Keep the terminal you type in away from the middle of the screen: a console window that comes to the front for a
   moment is easier to see there.

The `start` record of the relay that answers now:
`Get-Content "$env:LOCALAPPDATA\op-reason\ssh-log.jsonl" | Select-String '"what":"start"' | Select-Object -Last 1`.

| # | Do | Expect |
| --- | --- | --- |
| N1 | `& $exe --settings set relay.autostart on` | `relay.autostart: off → on`, and `& $exe --ssh-agent --status` exits 0. No console window (nor a Windows Terminal window or tab) comes, not even for a moment. The `start` record has `"detail":"console: none"` |
| N2 | Log off and on again | After the logon the relay answers (`& $exe --ssh-agent --status` exits 0); no console window came; its `start` record says `console: none` |
| N3 | `& $exe --settings`; in the window change the mode to `show-only`, **Apply**, OK | The relay's line says `show-only`; no console window at the restart. Change the mode back and **Apply** again: no console window either |
| N4 | The relay's icon in the notification area: **Restart the relay** | Within 15 seconds a relay with another process ID answers; no console window |
| N5 | Lock 1Password and unlock it. `$env:OPREASON_SSH_REASON = 'check N5'; ssh -T git@github.com`; **Allow this call**; approve 1Password | As G4: op-reason's window comes to the front at the click and 1Password's prompt comes at once, naming op-reason; `Hi <user>!` |
| N6 | A double click on `op-reason.exe` in Explorer; then pin it to the taskbar (right click → **Show more options** → **Pin to taskbar**), close the window and start it from the taskbar | The settings window opens, or comes to the front, both times; no console window, not even in the taskbar. Unpin it afterwards if you do not keep it |
| N7 | `& $exe --settings set start-menu on`; press the Windows key, type `op-reason` and open it | The settings window; no console window in the taskbar. `set start-menu off` afterwards if you do not keep it |
| N8 | In WSL, with `OPREASON_EXE` set to the WSL path of `$exe`: `op-reason --reason 'check N8' vault list </dev/null 2>&1 \| cat` (no terminal for the call: WSL starts the exe with `CREATE_NO_WINDOW` then) | op-reason's window and 1Password's prompt as usual; the vaults come through `cat`; no console window for `op.exe` |
| N9 | In PowerShell: `& $exe --reason 'check N9' vault list; $LASTEXITCODE`, and `& $exe --ssh-agent --status; $LASTEXITCODE` | The vaults and 0, the prompt back when the call ends, as before; the relay's line and 0 |
| N10 | A call without a console: `Start-Process $exe -ArgumentList '--reason "check N10" vault list'` (one string, the reason in double quotes: `Start-Process` joins an array's items with spaces and does not quote them); approve 1Password; then `& $exe --log --since 5m` | op-reason's window and 1Password's prompt as usual; no console window at all (`Start-Process` starts it with a console of its own before Windows 11 24H2, with none now, and `op.exe` then gets one that is never shown); the log has the call's end with exit 0 |
| N11 | `& $exe --settings set relay.autostart off`; then `& $exe --ssh-agent` in the same PowerShell, Ctrl+C after its first line; turn the other task of preparation 3 on again | The task is gone. The relay started by hand prints its line, stays in the terminal and stops on Ctrl+C; its `start` record says `console: shared`. The other task's relay answers again |

## Checks of the look of the windows (V1–V6)

The windows of `op-reason.exe` and of the relay in Windows' dark and light app mode
([2026-11-26-modern-window-design.md](superpowers/specs/2026-11-26-modern-window-design.md)). CI takes screenshots of
both modes but cannot show 1Password's prompt next to them, nor a person's desktop. About 15 minutes, with about five
prompts. Use the Windows zip of the pull request (as in the preparation; `$exe` below), and the relay of the SSH checks'
preparation (started in a terminal with `$exe`). Start in the dark mode: Settings → Personalization → Colors → Choose
your mode → Dark.

| # | Do | Expect |
| --- | --- | --- |
| V1 | `op --reason "check V1: dark" vault list >/dev/null`; before approving, look at op-reason's window, and note where 1Password's prompt opens | A dark window at the top right: a caption without a title (only its close button) in the window's color, rounded corners; a blue tile with a lock, the title, "op-reason · asking 1Password for approval", the time and the date; "Reason" and the reason in large letters; chips for the caller, the repository and its branch, and the directory; the command in a box with `op vault` in blue; the band with the picture of the screen; the red line with a shield; the switch as a pill at the bottom right. Note whether 1Password's prompt opened in the middle of the screen, as the band says |
| V2 | `op --reason "check V2: light while open" vault list >/dev/null`; while its window is open, switch the mode to Light in Settings; approve | The window turns light at once (colors and caption) and keeps its place and layout; 1Password's prompt keeps the focus |
| V3 | In the light mode, in a PowerShell: `$env:OPREASON_SSH_REASON = 'check V3: light'; ssh -T git@github.com`; look at the window, then click **Allow this call** and approve | A light window: a tile with a key; the reason; chips for the person, the repository and the directory; the root command `ssh -T git@github.com` in the box; "logging in as git", the destination and the key in small type; the bar of the seconds left and "Allowing covers this ssh until it ends (at most 60 minutes)"; the band; the buttons **Allow this call** (blue), **Allow this terminal for 60 minutes** (outlined) and **Deny** (outlined in red) at the right. After the click: "SSH key: 1Password is asking" with the band that names the tray |
| V4 | In a Claude Code session in WSL whose `ssh` goes through `op-reason-ssh`: ask it to run `git ls-remote origin` in a repository with an SSH remote; **Deny** | The box shows `git ls-remote origin`, not the shell that Claude Code wraps it in; under the box the root line in small type (`bash (pid …) · bash -c "source …`); the line says "Allowing covers this git ls-remote until it ends (at most 60 minutes)" |
| V5 | Set the display's scale to 150% (Settings → System → Display), then `op --reason "check V5: a long reason that goes on and on so that it takes two or three lines of the window at this scale, to see the layout" vault list >/dev/null`, approve; set the scale back | The window is sharp at 150%; the reason wraps, nothing overlaps the switch or the band, and the window stays inside the screen |
| V6 | S20's four commands, once in the dark mode and once in the light mode | The cards in the windows' colors: a dark (light) face with a thin border, "Passed" and the check in green, the command and its destination in a monospace font; places, sizes and times as before |

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
