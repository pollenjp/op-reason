# Checking op-reason with the real 1Password app

日本語版: [manual-verification.ja.md](manual-verification.ja.md)

CI runs op-reason with fake `powershell.exe` and `op.exe` only: the 1Password desktop app cannot run there. These
checks need a person, a Windows machine with the 1Password app and the 1Password CLI, and WSL. They take about 20
minutes and show about ten approval prompts.

## Rules while checking

- Use commands whose output holds no secret: `vault list`, `whoami`, `item template get`, `item create --dry-run`.
  Send their output to `/dev/null` unless a step says otherwise. Do not run `op read`, `item get` or `--reveal`.
- Answer each prompt as the step says. If a prompt appears that the step does not expect, deny it and note it.
- When you are done, lock 1Password once.

## Preparation

```sh
cargo build --release
export PATH=$PWD/target/release:$PATH
ln -sf op-reason target/release/op && ln -sf op-reason target/release/op-reason-log
command -v op            # …/target/release/op
op.exe --version         # the real 1Password CLI answers
export OPREASON_LANG=en  # or ja
```

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
| 11 | `timeout 5 op --reason "manual check 11: interrupted" vault list`, do not answer | After 5 seconds the window disappears and the prompt stays: deny it. `op-reason-log` shows `interrupted` |
| 12 | `op --version`, `op --help`, `op item get --help` | No reason needed, no window, no prompt |
| 13 | `op item get x` (no reason) | Exit 2, the usage with the hint for AI agents; `op-reason-log` shows `refused (no reason)` |
| 14 (optional) | With a service account: put the token in `OP_SERVICE_ACCOUNT_TOKEN` without showing it, then `op --reason "manual check 14: service account" vault list >/dev/null` | No window, no prompt, exit 0; `op-reason-log` shows `ok (SA)` |

## Checks on Windows

Use the release zip (or `windows/op-reason.ps1`, `windows/op-reason.cmd` and `launcher/launcher.ps1` from the
repository, side by side in one folder on `PATH`).

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
