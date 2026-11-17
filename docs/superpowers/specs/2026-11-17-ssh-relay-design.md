# op-reason: who and why for SSH agent requests (the SSH relay) — design

- Date: 2026-11-17
- Status: approved (the user chose per-call decisions in a relay over setting 1Password to ask for every request,
  asked for a switch to a show-only mode for people who do set it so, and chose to build it in this repository; the
  design was agreed before this document was written; this document records it, the answers to the builder check,
  and what is left to the implementer)
- Builds on: `2026-11-15-op-reason-design.md` §1.1 (approval binding, who is asking), `2026-11-15-windows-exe-design.md`
  (the window, the toast and the lock of `op-reason.exe`), `2026-11-15-wsl-through-exe-design.md` (facts collected on
  WSL, shown on Windows) and `2026-11-15-language-toggle-design.md` (clicks that never activate the window).

## 1. What and why

The user's request: as op-reason does for `op`, every use of a key in the 1Password SSH agent should say who asks and
for what.

Why op-reason's way for `op` does not carry over as is:

- **Who is asking.** The 1Password prompt for an SSH key names the requesting application and the key, nothing else.
  From WSL it names the terminal. It does not name the caller (a person or an AI agent), the destination or the
  reason.
- **One approval reaches later requests.** 1Password's setting "Ask approval for each new" has three values:
  application and terminal session (the author's), application, and request. With the first two, one approval lets
  every later request of the same application (and terminal session) through without a prompt until 1Password locks:
  after a person approved once, an agent can use the key silently.
- **"request" is not usable for every workflow.** It asks for every signature. ansible on WSL runs Windows `ssh.exe`
  (the author's setup), which cannot share connections (Win32-OpenSSH has no `ControlMaster`; its issue 405), and
  ansible opens new connections per task and host: one playbook run would show dozens to hundreds of prompts.
- **The approval does not bind to a call.** For `op`, 1Password binds an approval to the process that started
  `op.exe`, so op-reason confines it to one call by starting a fresh process. SSH clients all talk to one pipe
  (`\\.\pipe\openssh-ssh-agent`), and 1Password sees whichever process connects to it. op-reason can be that process.

## 2. Approaches

| Approach | What it means | Trade-off |
| --- | --- | --- |
| 1Password at "request", nothing else | every signature prompts | no who and why; ansible on WSL prompts per connection |
| P: "request", ansible on Linux `ssh` with `ControlMaster`, a relay that only shows | the decision stays in 1Password's prompt; the relay shows facts next to it | one prompt per host per run; ansible's settings change; every reconnect of a program prompts |
| **Q (chosen): a relay that decides per call** | 1Password approves the relay once until it locks; the relay asks once per call in its own window | one click per call with who, what and why; the decision moves from 1Password's prompt to op-reason's window, and a program of the same user can pass as an allowed call (§10) |

The user chose Q because only a relay can see calls: 1Password sees one request at a time. The user also asked for a
switch to a show-only mode (P's relay) for people who set 1Password to "request". The relay goes into this repository
(rather than a new one) to share the window, the log, the language, the lock and the release.

## 3. Goal and non-goals

Goal:

1. **The relay.** `op-reason.exe --ssh-agent` (a third face of the exe) runs resident, serves the SSH agent protocol
   on its own pipe (default `\\.\pipe\op-reason-ssh-agent`) and passes requests on to the 1Password agent
   (`\\.\pipe\openssh-ssh-agent`). Clients use it through `IdentityAgent` or `SSH_AUTH_SOCK` (set by the author's
   dotfiles, §12).
2. **Two modes**, chosen when the relay starts:
   - `per-call` (default): the relay decides. The first signature of a call (§5) opens the window with who, what and
     why, and Allow / Deny buttons. Once allowed, the call's later signatures pass without the window. 1Password stays
     at "application and terminal session" or "application" and approves the relay once until it locks.
   - `show-only`: for people whose 1Password asks for every request. The relay decides nothing: for every signature
     it shows the window, without buttons, while 1Password's prompt is open, as op-reason's window does for `op`.
3. **Who, what and why** in the window and the log (§6).
4. **WSL.** `op-reason-ssh` (the WSL binary under that name, as `op-reason-log`) runs `ssh.exe` with the facts of the
   call in its environment (§7).
5. **Records** of every signature and decision in the Windows log, shown by `op-reason.exe --log` (§9).

Non-goals:

- A relay on the remote Linux hosts that agent forwarding reaches, which would carry a remote call's facts (a later
  plan). Forwarded requests are decided one by one (§5.3).
- Pointing clients at the relay and starting it at logon: the author's dotfiles, after a release (§12).
- Claude Code hooks or rules that ask agents for a reason (the dotfiles).
- A fallback relay in PowerShell. Where Smart App Control blocks `op-reason.exe` there is no relay; the README says so.
- Git Bash's `ssh` (MSYS2; it does not use the Windows pipe), and `op-ssh-sign` if it talks to the 1Password app
  directly (C3).
- Requests other than signatures (listing keys, adding and removing keys, lock and unlock): passed on unchanged,
  without a window.
- Any change to `op` calls.
- A version, a tag or a release.

## 4. Facts and checks

### 4.1 Facts this design relies on

- Windows OpenSSH (`C:\Program Files\OpenSSH\ssh.exe`, 10.4p1 on the author's machine) talks to
  `\\.\pipe\openssh-ssh-agent` unless `IdentityAgent` or `SSH_AUTH_SOCK` names another pipe. The 1Password app serves
  that pipe; the Windows `ssh-agent` service is disabled.
- OpenSSH clients since 8.9 send the extension `session-bind@openssh.com` on an agent connection before they use it
  for a host: the host key, the session identifier, the server's signature and a flag that says the connection is a
  forwarding.
- From WSL, `ssh.exe` is started by the interop host that every pane shares (op-reason design §1.1): the Windows process
  chain cannot tell WSL callers apart.
- The data of a signature tells what it is for: user authentication (it holds the user name on the server) or an
  SSHSIG signature (it starts with `SSHSIG` and names a namespace, such as `git`).
- Users of the 1Password agent on the author's machine (2026-11-17): a terminal program's core that connects to remote
  hosts with `ssh.exe -o BatchMode=yes … -T <host>` (herdr-core; `ForwardAgent yes` for those hosts), people in
  terminals (Windows and WSL), ansible on WSL, and requests forwarded back from remote hosts. Claude Code's git uses
  HTTPS for GitHub, not SSH.
- Smart App Control is off on the author's machine; `op-reason.exe` 0.2.0 is installed from the release zip.
- Seen through a throwaway pass-through relay on the author's machine (2026-11-17, before this design was built):
  - `GetNamedPipeClientProcessId`, a Toolhelp snapshot and `NtQueryInformationProcess` (`ProcessCommandLineInformation`)
    give the client's PID, its chain and its command line to a relay of the same user.
  - A Claude Code tool call on Windows: `ssh.exe ← pwsh.exe ← cmd.exe ← claude.exe ← pwsh.exe ← herdr-core.exe ← …`
    (rule 1 gives `cmd.exe`).
  - `ssh.exe` run from WSL inside herdr: `ssh.exe ← wsl.exe ← wsl.exe ← herdr-core.exe ← …`, command line `ssh.exe …`
    without a path. The interop host is `wsl.exe` here.
  - The 1Password agent answers `SSH_AGENT_FAILURE` to `session-bind@openssh.com` (ssh goes on); the relay reads the
    extension before passing it on.
  - The answer to the key list carries each key's comment (the name shown in 1Password).
  - The data of a sign request gave `userauth user=git` for GitHub.

### 4.2 Checks with the real app, first

The implementer has these checked first (a pass-through relay is enough), before the rest is built. When an answer
differs, the implementer stops and reports: the design depends on them.

- **C1.** With "application and terminal session", once the relay is approved, signatures from other clients
  (Windows and WSL) through the relay pass without a prompt until 1Password locks, and the prompt names the relay.
- **C2.** With "request", one prompt per signature through the relay.
- **C3.** Whether `op-ssh-sign-wsl.exe` (git commit signing from WSL) uses the agent pipe. If not, signing stays
  outside the relay (the README says so).
- **C4.** Windows `ssh.exe` with `ForwardAgent` opens an agent connection per forwarded channel and sends
  `session-bind@openssh.com` with the forwarding flag on it.

## 5. Calls: the unit of a decision

### 5.1 The mark

A request belongs to a call, identified by its mark: the root process, the key and the reason.

- The **root process** by its PID and its start time, so that a reused PID never matches.
- The **key** by the SHA-256 fingerprint of the key blob in the sign request.
- The **reason** as normalized by the core (the rules of `--reason`); no reason is a value of its own.

The same reason from another process never reuses a decision: the root differs.

### 5.2 Finding the root

The chain is walked from the client process (`GetNamedPipeClientProcessId`) up, from one Toolhelp snapshot. A parent
counts only when it started before its child (a reused PID is not followed).

1. A `claude.exe` ancestor: the root is its child on the chain. Claude Code starts `claude.exe → cmd.exe → pwsh.exe`
   afresh for every tool call (op-reason design §1.2), so one tool call is one call.
2. Else an interactive shell ancestor: the root is the shell's child on the chain, the command a person typed (`git
   push`, `ansible-playbook`, `ssh`).
3. Else the client's parent: a program that starts `ssh.exe` itself (herdr-core).

WSL callers (the client's parent is the WSL interop host, and the client's environment holds `OPREASON_SSH_CALL`) use
the root that `op-reason-ssh` found on Linux with the same three rules (§7). A WSL `ssh.exe` without `OPREASON_SSH_CALL`
(run directly, not through `op-reason-ssh`) has no root the relay can see: each of its connections is a call of its
own.

The lists of shells and terminal hosts behind rule 2 are the implementer's, held in a shared table of chains and their
roots that the tests run (as `tests/cases/calls.json` is shared).

### 5.3 Forwarded requests

A connection whose `session-bind@openssh.com` says forwarding carries requests from a remote host. Its client is the
local `ssh.exe` that holds the forwarding, whose root says nothing about the remote caller. Every signature on such a
connection is a call of its own (per-call mode asks every time), and the window says "forwarded from <host>".

### 5.4 Remembering and forgetting

- A decision (allow or deny) applies to the whole call: later requests of a denied call fail at once, without a
  window, so that a denied playbook run does not ask once per connection.
- Decisions live in memory only, never on disk.
- A decision is forgotten when its root process ends (Windows roots: checked on lookup and once a minute), after the
  time limit (default 60 minutes from the decision, an option of the relay), or when the relay exits. WSL roots: the
  relay cannot see Linux processes, so only the time limit applies; the start time in the mark keeps a later process
  with the same PID apart.
- A window that gets no answer in 60 seconds fails that request and is not remembered (the next request asks again).
- For a person's command (rule 2 of §5.2) the window offers one more answer: allow the terminal for the time limit.
  That decision is keyed by the interactive shell (PID and start time) and the key. It covers only the later calls
  found by rule 2 whose nearest interactive shell is that shell: the commands a person types in that terminal, so that
  `git pull` and `ssh` typed by hand are not asked for one by one. Calls found by rule 1 or rule 3 under that shell
  (Claude Code started from it, a program started from it) are not covered: they ask as before. It is not offered for
  Claude Code's calls nor for programs.

### 5.5 While a window is open

- One window at a time. Requests of the same mark wait for its answer (ansible's parallel forks see one window);
  requests of other marks queue, first come first asked; the window says how many calls wait.
- The later requests of an allowed call pass at once: no window, no lock.
- The relay takes op-reason's lock (`Local\op-reason`) while its window is open and while the first request of a call
  waits for 1Password (which may prompt), and writes `holder.json` as `op-reason.exe` does, so that op-reason's windows
  and 1Password's prompts come in pairs. It waits for the lock up to `OPREASON_LOCK_WAIT` seconds; then the request
  fails (`lock-timeout`).
- After 1Password locks, it asks to unlock on the next request, which may be a request the relay passes without a
  window. When a passed request has no answer from 1Password within a second, the relay shows the window without
  buttons until the answer comes, so that a 1Password prompt never stands alone. (The implementer may drop this
  rule if it proves unreliable, and records why.)

## 6. The window

The look and behavior of `op-reason.exe`'s window: Direct2D, at the top right, never activated, the language switch,
texts in English and Japanese.

**per-call mode**

- Head: "SSH key: allow this call?" / 「SSH の鍵: この呼び出しを許可しますか」.
- Who: the caller (Claude Code and its session, a person in a terminal, or a program), the root process and its
  command line (clipped), and for WSL callers the distribution, herdr's workspace and pane, the repository, the branch
  and the directory that `op-reason-ssh` collected. For Windows callers the implementer may read the root's directory
  and find the repository and branch from git files (the core's place detection).
- What: the `ssh.exe` command line (clipped to 300 characters), the destination (from the command line, and the host
  key's fingerprint from `session-bind@openssh.com`), "forwarded from <host>" for forwarded connections, what the
  signature is for (user authentication and the user name, or SSHSIG and its namespace), and the key (its comment from
  the agent's key list, and its fingerprint).
- Why: the reason, or "no reason given" / 「理由なし」.
- Buttons: "Allow this call" / 「この呼び出しを許可」 and "Deny" / 「拒否」, and for a person's command "Allow this
  terminal for N minutes" / 「このターミナルを N 分許可」 (§5.4). Only a mouse click (press and release on the button)
  answers. The window never takes the focus and has no default button, so a key typed elsewhere never
  answers it.
- Footer: the mode, the time limit, how many calls wait, and the language switch.

**show-only mode**

- The same facts without buttons; head "SSH key: 1Password is asking" / 「SSH の鍵: 1Password が確認しています」; the
  window closes when 1Password answers.
- When a signature succeeds in under half a second, 1Password most likely did not ask: its setting is not "request",
  and one approval of the relay lets every call through. The relay says so in a toast, once per run of the relay,
  and records it.

A toast goes with each window, as for `op` calls. The relay reads the language for each window (`settings.json`,
then `OPREASON_LANG`), so a click on the switch applies to the next window.

## 7. WSL: `op-reason-ssh`

- The WSL binary under the name `op-reason-ssh` (chosen by `argv[0]`, as `op-reason-log`).
- It collects the call's facts: the root on Linux (§5.2's rules on `/proc`: a `claude` ancestor's child; else an
  interactive shell's child; else its parent), which rule found it, the root's PID, start time and command line, for
  rule 2 the interactive shell's PID and start time (for the terminal answer of §5.4), the caller (as the `op` wrapper
  collects it: Claude Code's PID and session, herdr's workspace and pane, repository, branch and directory),
  `WSL_DISTRO_NAME`, and the reason from `OPREASON_SSH_REASON`.
- It puts them into `OPREASON_SSH_CALL` (ASCII JSON, bounded in size), adds `OPREASON_SSH_CALL` to `WSLENV`, and
  `exec`s `ssh.exe` (`OPREASON_SSH_EXE`, else `ssh.exe` in `PATH`) with the arguments unchanged. Standard streams are
  untouched: git and ansible talk through them.
- It never makes an ssh call fail: when it cannot collect, it runs `ssh.exe` without `OPREASON_SSH_CALL`.

Windows callers give a reason in `OPREASON_SSH_REASON`; for WSL callers the reason inside `OPREASON_SSH_CALL` counts.
The relay reads both, and the command line, from the client's process (its environment and parameters).

## 8. The relay process

- `op-reason.exe --ssh-agent` with options for the mode, the pipe names (its own and the upstream) and the time limit.
  A mode is changed by starting the relay again with another option; the window has no switch for it.
- One relay per logon session: a second start finds it running and exits with a message.
- No console window while resident: the dotfiles start it from a logon task.
- The pipe is created with `FILE_FLAG_FIRST_PIPE_INSTANCE` (the relay does not start when the name is taken), for the
  current user only (a DACL), with `PIPE_REJECT_REMOTE_CLIENTS`, and any number of instances.
- One upstream connection per client connection. When 1Password's pipe is missing (1Password not running), each
  request gets `SSH_AGENT_FAILURE`, recorded as `no-upstream`; the relay keeps serving and tries again on the next
  connection.
- Messages follow the agent protocol's framing (a 32-bit length, at most 256 KiB, as OpenSSH's agent; longer closes
  the connection). Only sign requests are held for a decision. `session-bind@openssh.com` is read and passed on;
  answers to the key list are read for the keys' comments and passed on; everything else passes unchanged. The relay
  never changes a message it passes on.
- Many connections at once (ansible's forks); a decision waits without blocking other connections.
- A way for the dotfiles to tell whether the relay runs (for example `op-reason.exe --ssh-agent --status`, exit 0 when
  its pipe answers).
- The relay needs a live parent process (§15): started from a logon task, its parent is the Task Scheduler's host.
  When its parent is missing at start or ends later, the relay says so (a toast and a record), because 1Password
  would leave its requests unanswered for 60 seconds. How the relay notices is the implementer's.

## 9. Records

SSH records go to a file of their own next to the Windows log: `%LOCALAPPDATA%\op-reason\ssh-log.jsonl`, with its own
rotation and lock (the implementer chooses the size and the number of old files). One playbook run makes hundreds of
records, and in `log.jsonl` (rotated at 1 MiB, by the PowerShell entry too) they would push the records of `op` calls
out.

- At least one record per signature, with the time, the mode, the result (allowed by a click, passed as an allowed
  call's later request, denied, timed out, failed upstream, no upstream, lock time-out), the seconds, the call (the root
  and, for WSL, the distribution), the caller, the reason (≤ 500 characters), the client (PID, executable, command line
  ≤ 1000), the key (fingerprint and comment), what the signature is for, the host (name and host key fingerprint) and
  whether it was forwarded. A passed request may refer to the record of its call's decision instead of repeating its
  facts.
- Never the data to sign or the signature.
- `op-reason.exe --log` shows both files, merged by time, one line each. The WSL `op-reason-log` stays as it is (the
  records live on Windows).

Command lines (the client's and the root's, in the window and in the records) go through the masking of `op` calls
(the core's rules, such as `name=value` → `name=***`). The remote command after the destination stays, masked the same
way: `git-upload-pack '…'` is worth seeing.

## 10. Safety

- **What it stops:** an AI agent or a script using a key without a person seeing who, what and why: the first signature
  of every call waits for a click. Every signature is recorded.
- **What it does not stop:** a program of the same user that wants to get around it. It can fake `OPREASON_SSH_CALL`
  to pass as an allowed WSL call (a WSL root is taken on trust), start under an allowed Windows root, inject into the
  relay or send clicks. As for `op`, op-reason shows facts, not proofs.
- **A client that talks to 1Password's pipe directly** is a new application for 1Password, which asks with the user's
  setting: a 1Password prompt without op-reason's window is the sign. The README says so.
- **The relay holds an approval** of 1Password until it locks (per-call mode). Whoever passes its decision gets
  signatures; hence the root in the mark, the click-only buttons and the pipe for the user only.
- **No secrets pass:** the agent protocol carries public keys and signatures, and the relay keeps neither the data it
  signs nor the signatures.
- **show-only mode** decides nothing and is as strict as 1Password's "request"; the toast of §6 catches a mismatch.
- **A change of op-reason's rule, stated:** so far the decision is made in 1Password's prompt, and op-reason's window
  shows facts and never asks a second question. per-call mode is the one place where op-reason asks, because 1Password
  cannot see calls and asking it per signature is not usable (§1). show-only mode keeps the old rule. The README and
  the architecture page say so.

## 11. Tests

- **Core and WSL binary (Linux):** the agent protocol's framing and the messages the relay reads (sign request,
  `session-bind@openssh.com`, the key list), from byte vectors; the decision table (allow, deny, time-out, the root
  ended, the time limit, the same mark waiting, the queue) with a fake clock and fake process checks; the shared table
  of chains and roots (Windows chains and `/proc` chains); `OPREASON_SSH_CALL` built and read; the window texts of both
  modes in both languages; the records and the viewer's lines; `op-reason-ssh` with a fake `ssh.exe` (arguments and
  streams unchanged, `WSLENV`, no failure when collecting fails).
- **Windows (CI runner):** the relay end to end with a fake upstream agent (a test pipe server) and a test client that
  speaks the protocol: requests pass unchanged, a sign request waits for the window, a click on Allow and on Deny (as
  the CI clicks the language switch), the same-mark requests wait for one answer, denial covers the call, the time
  limit, no upstream, the pipe's DACL and the first-instance rule, one relay per session, and the lock shared with
  `op-reason.exe`. Screenshots of both modes on the pull request, as for the window of `op` calls.
- **By hand** (`docs/manual-verification.md` and its Japanese version, a new letter): C1–C4; per-call: a playbook run
  (or a loop of ssh) asks once, a second run asks again, a denial fails the run without more windows, forwarded
  requests ask each time; show-only with "request": one prompt and one window per signature; show-only with the
  default setting: the toast; 1Password locked: the window comes with the unlock prompt; a terminal program's
  reconnect.

## 12. The dotfiles, after a release (not this repository)

- Install the release and start `op-reason.exe --ssh-agent` at logon (a scheduled task, hidden, restarted on failure),
  from a scheduled task, so that its parent stays alive (not from a script that exits).
- Point clients at the relay, including the terminal program's own connections (so that forwarded requests come
  through it).
- Run `op-reason-ssh` instead of `ssh.exe` from WSL.
- Decide what clients do when the relay does not run (they have no agent then).
- Tell agents to set `OPREASON_SSH_REASON`.
- Record the change of rule (§10) in the dotfiles' own decision records.

## 13. Left to the implementer

The lists of shells and terminal hosts of §5.2; how the relay reads another process's environment, parameters and
directory; the names of options, the status command and the log's fields; the window's layout and wording; how the
face runs without a console; the thread model; the second rule of §5.5; the threshold of §6's toast; the test vectors.
Each goes into the table of implementer's decisions below.

## 14. Implementer's decisions

The first rows answer the builder check (decided before the implementation); the implementer adds the rest.

| Decision | Why |
| --- | --- |
| The terminal answer (§5.4) covers only rule-2 calls whose nearest interactive shell is the allowed one | read literally, it would let Claude Code or a program started later from that shell sign silently: the problem of §1 |
| SSH records in `ssh-log.jsonl`, not in `log.jsonl` | a playbook run must not push the records of `op` calls out of a 1 MiB log; the PowerShell entry, which rotates `log.jsonl` itself, stays as it is |
| Command lines masked with the rules of `op` calls; the remote command stays | the README promises masked values in the log; the remote command (`git-upload-pack '…'`) tells what the call does |

## 15. Checked with the real app, and not yet

- C1 and C4, checked with a throwaway pass-through relay (2026-11-17 and 18; 1Password at "application and terminal
  session"; the author's machine):
  - First try: the relay had been started from a short-lived shell that had exited by the second signature. The
    first signature showed one prompt and passed once approved (21.9 s); each of the next four waited 60 seconds
    without a prompt, and 1Password closed the connection.
  - Second try, the relay started from Explorer (`pwsh.exe ← cmd.exe ← explorer.exe`, all alive): the first signature
    showed one prompt and passed once approved (18.4 s); the next ones passed without a prompt in 0.1–0.2 s: from
    Windows, from WSL, the authentication to a remote host, and a signature forwarded back from that host (C1).
  - A forwarded agent connection is a new connection from the local `ssh.exe` that starts with
    `session-bind@openssh.com` with the forwarding flag set and the remote host's key; the remote client's own
    `session-bind@openssh.com` (flag clear, the destination's host key) follows on it (C4). `ssh -A` also opens
    connections that close without a message.
  - So 1Password binds its approval to the relay, but it leaves a relay whose parent process has ended unanswered
    (most likely: the parent was the only difference between the two tries).
- Not yet: C2, C3 (§4.2) and the manual checks of §11.
