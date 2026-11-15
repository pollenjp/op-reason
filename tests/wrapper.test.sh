#!/usr/bin/env bash
# shellcheck shell=bash
#
# Black-box test of the WSL wrapper (the op-reason binary) with fake op-reason.exe, powershell.exe, op.exe, wslpath and
# herdr. Ported from the dotfiles' test of the bash version; every check of the original is kept, with the new names.
#
#   bash tests/wrapper.test.sh [<op-reason binary>]
#
# Needs bash, coreutils, jq, git, flock and setsid (util-linux). No real op-reason.exe, powershell.exe, op.exe or
# 1Password (the exe is tested on Windows by crates/op-reason-win/tests/exe.rs, the launcher by launcher.test.sh, the
# binary and the real launcher together by e2e.test.sh).
# OPREASON_LANG=ja is set so that the Japanese messages of the original checks stay; a few checks use English.
#
# ## What it checks
#
#   Reason
#     - no --reason, an empty one or one of only spaces: exit 2, usage on stderr, no runner, refused (no-reason)
#     - --reason X / --reason=X anywhere: op gets the rest; a --reason after -- is op's
#     - newline, tab and control characters fold into one space, ends trimmed; the window gets 200 characters, the log 500
#   Calls without a reason (--version, --help, -h, help, --help before --) run in help mode (no window) through
#     the runner
#   No op arguments (also --reason alone): the wrapper's usage on stdout, exit 0, no runner, no op.exe (not even the
#     watchdog's whoami), no log
#   The wrapper's own options: op-reason --help / --version (only under the name op-reason), --skill, --completion fish,
#     --completion zsh (exit 2), op-reason-log by its name; op --help goes to op
#   Passing through
#     - binary stdin reaches op and comes out on stdout; stderr has only op's; the exit code is op's
#     - the runner is op-reason.exe (found in PATH) with --run as its only argument, and no OPREASON_SCRIPT
#   desktop
#     - payload mode desktop, op is the Windows path of op.exe, args64 are op's arguments, lang, lock held
#     - WSLENV is the caller's plus OPREASON_PAYLOAD; no token (removed from the environment too)
#     - facts: time, caller, place, reason, command
#   service account and Connect
#     - OP_SERVICE_ACCOUNT_TOKEN: service mode without facts, WSLENV adds OP_SERVICE_ACCOUNT_TOKEN/w and
#       OP_BIOMETRIC_UNLOCK_ENABLED/w, OP_BIOMETRIC_UNLOCK_ENABLED=false; log kind service-account
#     - OP_CONNECT_HOST and OP_CONNECT_TOKEN: service (kind connect); only one of them: desktop
#     - tokens are never logged
#   Masking: password=…, --session and --secret-key values become *** in the log and the facts (op gets them unchanged);
#     after -- only the command name and value-less flags stay
#   Too long (base64 over 28000): exit 2, no runner, refused (too-long)
#   Results: 0 → approved; denial text → denied; timeout text → timeout; others → failed; no status → launcher-failed.
#     Only op's "[ERROR] …" lines count (a child's "Connection timed out" is failed). start and end share the id; end's
#     seconds is an integer ≥ 0
#   Caller: a claude ancestor in proc (with its pid), else human; herdr workspace label and pane; git repo and branch;
#     CLAUDE_CODE_SESSION_ID in start and end
#   Lock: another call holding the lock past the limit → exit 75 with the seconds and the holder's reason, no runner,
#     refused (lock-timeout); TERM while waiting → exit 143, refused (interrupted); a broken current.json → "(分からない)"
#   Pipes: op … | op … (also with cat in between) never deadlocks, whatever starts first or takes the lock first
#     - sender takes the lock first: the receiver finds it and drains stdin (larger than a pipe buffer)
#     - sender starts late and takes the lock first: the receiver finds it while waiting
#     - receiver takes the lock first: the sender runs without the lock (start lock skipped, payload lock skipped, a
#       lock-skipped note with the receiver's reason)
#     - a pipe without op behind it (a writer that never closes) is not drained (after waiting for the lock too)
#     - a lock holder that does not read our output: wait and give up with 75
#     - four unordered rounds never deadlock
#   Shared approval watchdog: a direct op.exe whoami that succeeds → signout --all, shared-approval logged, notify (the
#     runner's notify mode and herdr); otherwise no signout
#   op.exe missing → 127 (refused); powershell.exe is not needed while op-reason.exe runs
#   The runner (OPREASON_RUNNER, design 2026-11-15-wsl-through-exe-design.md §4)
#     - auto (default): op-reason.exe, end runner exe; launcher mode: powershell.exe -File <Windows path of launcher.ps1>
#       next to op-reason.exe, end runner launcher
#     - auto falls back to the launcher only when op-reason.exe left no status file: an exe that writes nothing (with
#       stdin still whole for the launcher, also stdin drained from another op-reason), an exe that cannot be started, an
#       exe missing next to launcher.ps1; each prints one line and records end fallback (why, exit)
#     - no fallback when the exe wrote the status file (failing before op.exe, or dying after it); op.exe runs exactly
#       once in every case
#     - no fallback either when the call's run directory is gone (another call removed it: no status file proves nothing)
#     - exe mode never falls back; nothing found → 127 (no-exe); launcher mode without launcher.ps1 / powershell.exe →
#       127 (no-launcher / no-powershell); a fallback without the launcher → 127, end launcher-failed
#     - wslpath -w gives nothing for the status file → refused (no-wslpath), no runner: an empty status path would
#       leave no trace
#     - the shared approval toast falls back the same way
#   TERM → end interrupted, exit 143; a TERM to the wrapper alone is forwarded to the runner; the runner starts
#     with no blocked signals (so a TERM or HUP to the process group reaches it); a powershell.exe that cannot be started
#     (launcher mode) gives 127 with a message
#   Closed stdin: nothing extra on stderr
#   Log rotation at 1 MiB; nothing left under the state directory after a call (no run/ entries, no current.json, no
#     temporary files); run/ entries older than an hour are removed, unless their call's process still runs
#   Non-UTF-8 argument bytes reach op unchanged; a removed current directory still runs; a closed stdout does not panic
#   The language (design 2026-11-15-language-toggle-design.md §2 and §5)
#     - LANG / LC_ALL alone no longer choose Japanese
#     - a status with chosenLang "en" writes <state>/lang.json, and the next call speaks English despite
#       OPREASON_LANG=ja (its refusal, its payload's lang, op-reason-log); the launcher's status counts the same
#     - no chosenLang, an unknown language or a malformed value leave lang.json as it is (and the call's result stays);
#       null removes it, and OPREASON_LANG counts again
#
# ## How
#
# One fake runner script is installed twice: as op-reason.exe (in FAKE_WIN, which is in PATH, next to a launcher.ps1)
# and as powershell.exe. As op-reason.exe it first writes the status file, as the real exe does (FAKE_EXE=blocked:
# writes nothing and exits 1; marker: writes the status file and exits 1; die-after-op: exits 9 after op without the
# final status; rundir-gone: removes the call's run directory and exits 1). A notify payload goes to FAKE_DIR/notify.<runner>.<pid>.payload (exit 0). Otherwise it writes its
# arguments, its name, OPREASON_PAYLOAD, WSLENV and the token variables to FAKE_DIR/ps.<n>.*, decodes args64, runs the
# fake op.exe (the payload's op through the fake wslpath) with stdio inherited, writes the status (with chosenLang set to
# the JSON value FAKE_CHOSEN, when it is set) and exits with op's code (FAKE_PS_NO_STATUS=1: no status, exit 1). The fake op.exe logs whoami (0 when FAKE_DIR/shared exists) and
# signout (removes it) to FAKE_DIR/op.direct; other calls add a line to FAKE_DIR/op.calls, write their arguments to
# FAKE_DIR/op.args, copy FAKE_OUT, FAKE_OUT_FILE and stdin (unless FAKE_NO_STDIN=1) to stdout, write FAKE_STDERR to
# stderr and exit with FAKE_RC. The fake wslpath maps /x/y to C:\fake\x\y and back. The fake proc tree is passed with
# OPREASON_PROC_ROOT and OPREASON_PROC_START.

# No -e: a failed check must not stop the checks after it.
set -u -o pipefail

here=$(
  cd -- "$(dirname "$0")" &>/dev/null || exit
  pwd -P
)
# shellcheck source=tests/lib.sh
. "${here}/lib.sh"
bin=${1:-${OPREASON_BIN:-${here}/../target/debug/op-reason}}
bin=$(cd -- "$(dirname "${bin}")" && pwd -P)/$(basename "${bin}")
work=$(mktemp -d "${TMPDIR:-/tmp}/op-reason-wrapper-test.XXXXXX")
work=$(cd "${work}" && pwd -P)
trap 'rm -rf "${work}"' EXIT

export LC_ALL=C.UTF-8 OPREASON_LANG=ja
export HOME=${work}/home XDG_STATE_HOME=${work}/state
export FAKE_DIR=${work}/fake
fake_bin=${work}/bin
# Where the Windows zip would be unpacked: op-reason.exe and launcher.ps1 (found through PATH)
export FAKE_WIN=${work}/winapp
mkdir -p "${HOME}" "${XDG_STATE_HOME}" "${FAKE_DIR}" "${fake_bin}" "${FAKE_WIN}" "${work}/win" "${work}/names"
export PATH=${fake_bin}:${PATH}:${FAKE_WIN}
unset WSLENV HERDR_ENV HERDR_WORKSPACE_ID HERDR_PANE_ID CLAUDE_CODE_SESSION_ID OP_SERVICE_ACCOUNT_TOKEN \
  OP_CONNECT_HOST OP_CONNECT_TOKEN OP_BIOMETRIC_UNLOCK_ENABLED XDG_RUNTIME_DIR OPREASON_RUNNER OPREASON_EXE \
  OPREASON_POWERSHELL FAKE_EXE

# The binary under its three names
op=${work}/names/op
ln -s "${bin}" "${op}"
ln -s "${bin}" "${work}/names/op-reason"
ln -s "${bin}" "${work}/names/op-reason-log"

fake "${fake_bin}/wslpath" <<'EOF'
if [[ -n ${FAKE_WSLPATH_FAIL:-} && $2 == *"${FAKE_WSLPATH_FAIL}"* ]]; then exit 1; fi
case $1 in
  -w) p=${2//\//\\}; printf "C:\\\\fake%s\n" "${p}" ;;
  -u) p=${2#C:\\fake}; printf "%s\n" "${p//\\//}" ;;
  *) exit 2 ;;
esac
EOF

runner_body=$(
  cat <<'EOF'
d=${FAKE_DIR}
who=${0##*/}
mode=$(jq -r .mode <<<"${OPREASON_PAYLOAD}")
status_w=$(jq -r '.status // empty' <<<"${OPREASON_PAYLOAD}")
status=""
if [[ -n ${status_w} ]]; then status=$(wslpath -u "${status_w}"); fi
if [[ ${who} == op-reason.exe ]]; then
  if [[ ${FAKE_EXE:-} == blocked ]]; then
    echo "fake: op-reason.exe was blocked" >&2
    exit 1
  fi
  if [[ ${FAKE_EXE:-} == rundir-gone ]]; then
    rm -rf "${status%/*}"
    exit 1
  fi
  # The real exe writes the status file as soon as it has read the payload
  if [[ -n ${status} ]]; then printf '{"started":false,"error":"","runner":"exe"}' >"${status}"; fi
  if [[ ${FAKE_EXE:-} == marker ]]; then exit 1; fi
fi
if [[ ${mode} == notify ]]; then
  printf "%s" "${OPREASON_PAYLOAD}" >"${d}/notify.${who}.$$.payload"
  exit 0
fi
n=$(($(cat "${d}/ps.n" 2>/dev/null || echo 0) + 1))
echo "${n}" >"${d}/ps.n"
printf "%s\n" "$@" >"${d}/ps.${n}.argv"
echo "${who}" >"${d}/ps.${n}.runner"
printf "%s" "${OPREASON_PAYLOAD-}" >"${d}/ps.${n}.payload"
{
  echo "WSLENV=${WSLENV-<unset>}"
  echo "SCRIPT=${OPREASON_SCRIPT-<unset>}"
  echo "SA=${OP_SERVICE_ACCOUNT_TOKEN-<unset>}"
  echo "BIO=${OP_BIOMETRIC_UNLOCK_ENABLED-<unset>}"
  echo "CH=${OP_CONNECT_HOST-<unset>}"
  echo "CT=${OP_CONNECT_TOKEN-<unset>}"
} >"${d}/ps.${n}.env"
echo "$$" >"${d}/ps.${n}.pid"
# The mask this script inherited, read by a child (bash blocks SIGCHLD in itself while it waits, and passes its
# original mask to its children)
grep '^SigBlk' /proc/self/status >"${d}/ps.${n}.sigblk"
args=()
while IFS= read -r b; do
  a=$(printf '%s' "${b}" | base64 -d; printf x)
  args+=("${a%x}")
done < <(jq -r '.args64[]' <<<"${OPREASON_PAYLOAD}")
op=$(wslpath -u "$(jq -r .op <<<"${OPREASON_PAYLOAD}")")
rc=0
"${op}" "${args[@]}" || rc=$?
if [[ ${who} == op-reason.exe && ${FAKE_EXE:-} == die-after-op ]]; then exit 9; fi
if [[ ${who} == powershell.exe && -n ${FAKE_PS_NO_STATUS:-} ]]; then
  exit 1
fi
if [[ ${who} == powershell.exe && -n ${FAKE_PS_JUNK_STATUS:-} ]]; then
  printf 'not json' >"${status}"
  exit "${rc}"
fi
jq -cn --argjson rc "${rc}" --arg who "${who}" --arg chosen "${FAKE_CHOSEN-}" '{started: true, exit: $rc, error: "", window: "shown", toast: "shown", others: 0, signout: "ok"} + (if $who == "op-reason.exe" then {runner: "exe"} else {} end) + (if $chosen == "" then {} else {chosenLang: ($chosen | fromjson)} end)' >"${status}"
exit "${rc}"
EOF
)
fake "${fake_bin}/powershell.exe" <<<"${runner_body}"
fake "${FAKE_WIN}/op-reason.exe" <<<"${runner_body}"
touch "${FAKE_WIN}/launcher.ps1"

op_exe=${work}/win/op.exe
fake "${op_exe}" <<'EOF'
d=${FAKE_DIR}
case ${1-} in
  whoami)
    echo "$*" >>"${d}/op.direct"
    [[ -e ${d}/shared ]] && exit 0
    exit 1
    ;;
  signout)
    echo "$*" >>"${d}/op.direct"
    rm -f "${d}/shared"
    exit 0
    ;;
esac
echo call >>"${d}/op.calls"
printf "%s\0" "$@" >"${d}/op.args"
if [[ -n ${FAKE_SLEEP:-} ]]; then sleep "${FAKE_SLEEP}"; fi
printf "%s" "${FAKE_OUT:-}"
if [[ -n ${FAKE_OUT_FILE:-} ]]; then cat "${FAKE_OUT_FILE}"; fi
if [[ -z ${FAKE_NO_STDIN:-} ]]; then cat; fi
printf "%s\n" "${FAKE_STDERR:-fake-err}" >&2
exit "${FAKE_RC:-0}"
EOF
export OPREASON_OP_EXE=${op_exe}

fake "${fake_bin}/herdr" <<'EOF'
case "$1 $2" in
  "workspace get") printf "%s\n" "{\"result\":{\"workspace\":{\"workspace_id\":\"$3\",\"label\":\"作業 1 テスト\"}}}" ;;
  "notification show") printf "%s\n" "$*" >>"${FAKE_DIR}/herdr.notify" ;;
  *) exit 1 ;;
esac
EOF

# Fake proc tree: 1000 (bash) → 999 (<comm>) → 500 (fish) → 1
make_proc() {
  local root=${work}/proc
  rm -rf "${root}"
  mkdir -p "${root}/1000" "${root}/999" "${root}/500"
  echo "1000 (bash) S 999 1000 1000" >"${root}/1000/stat"
  echo bash >"${root}/1000/comm"
  echo "999 ($1) S 500 999 999" >"${root}/999/stat"
  echo "$1" >"${root}/999/comm"
  echo "500 (fish) S 1 500 500" >"${root}/500/stat"
  echo fish >"${root}/500/comm"
}
make_proc fish
export OPREASON_PROC_ROOT=${work}/proc OPREASON_PROC_START=1000

state=${XDG_STATE_HOME}/op-reason
log=${state}/log.jsonl

reset() {
  rm -rf "${FAKE_DIR:?}"/* "${state}"
}

# run_op <name> <arg>...: runs the wrapper as op. stdin from stdin_file (default /dev/null); out, err, rc
run_op() {
  local name=$1
  shift
  rc=0
  "${op}" "$@" <"${stdin_file:-/dev/null}" >"${work}/${name}.out" 2>"${work}/${name}.err" || rc=$?
}

# run_as <invocation name> <name> <arg>...: the same under another name
run_as() {
  local as=$1 name=$2
  shift 2
  rc=0
  "${work}/names/${as}" "$@" </dev/null >"${work}/${name}.out" 2>"${work}/${name}.err" || rc=$?
}

# How many runners started (notify calls not counted), which runner started the n-th time, how often op.exe ran (the
# watchdog's whoami and signout not counted)
ps_count() {
  cat "${FAKE_DIR}/ps.n" 2>/dev/null || echo 0
}
runner_of() {
  cat "${FAKE_DIR}/ps.${1:-1}.runner" 2>/dev/null
}
op_runs() {
  if [[ -e ${FAKE_DIR}/op.calls ]]; then wc -l <"${FAKE_DIR}/op.calls"; else echo 0; fi
}
payload_of() {
  cat "${FAKE_DIR}/ps.${1:-1}.payload" 2>/dev/null
}
payload_field() {
  jq -r "$1" <<<"$(payload_of "${2:-1}")" 2>/dev/null
}
# The payload's args64 decoded, NUL-separated
payload_args() {
  local b a
  while IFS= read -r b; do
    a=$(
      printf '%s' "${b}" | base64 -d
      printf x
    )
    printf '%s\0' "${a%x}"
  done < <(jq -r '.args64[]' <<<"$(payload_of "${1:-1}")" 2>/dev/null)
}
expect_payload_args() {
  cmp -s <(payload_args) <(printf '%s\0' "$@")
}
env_of() {
  cat "${FAKE_DIR}/ps.${1:-1}.env" 2>/dev/null
}
last() {
  jq -c "select(.event == \"$1\")" "${log}" 2>/dev/null | tail -n 1
}
last_field() {
  jq -r "$2" <<<"$(last "$1")" 2>/dev/null
}

echo "== Reason"
reset
run_op noreason item get a
if [[ ${rc} == 2 && $(<"${work}/noreason.err") == *--reason* && $(<"${work}/noreason.err") == *"op-reason --skill"* &&
$(ps_count) == 0 && $(last_field refused '.why') == no-reason ]]; then
  ok "no --reason: exit 2, usage (with the agent hint), no runner, refused (no-reason)"
else
  ng "no --reason: exit 2, usage (with the agent hint), no runner, refused (no-reason)" "rc=${rc} ps=$(ps_count) err=$(cat "${work}/noreason.err")"
fi
reset
run_op empty --reason "" item get a
rc_empty=${rc}
run_op blank --reason $'  \t ' item get a
if [[ ${rc_empty} == 2 && ${rc} == 2 && $(ps_count) == 0 && $(<"${work}/blank.err") == *"理由 (--reason) が空です"* ]]; then
  ok "an empty reason or one of only spaces: exit 2"
else
  ng "an empty reason or one of only spaces: exit 2" "empty=${rc_empty} blank=${rc} ps=$(ps_count)"
fi
for form in front middle eq; do
  reset
  case ${form} in
    front) run_op where --reason X a b ;;
    middle) run_op where a --reason X b ;;
    eq) run_op where a b --reason=X ;;
  esac
  if [[ ${rc} == 0 ]] && expect_payload_args a b; then
    ok "--reason anywhere (${form}): op gets a b"
  else
    ng "--reason anywhere (${form}): op gets a b" "rc=${rc} args=$(payload_args | tr '\0' '|') err=$(cat "${work}/where.err")"
  fi
done
reset
run_op dashdash a -- b --reason X
detail="rc=${rc} ps=$(ps_count)"
check "a --reason after -- belongs to op (refused as no reason)" test "${rc}" == 2 -a "$(ps_count)" == 0
reset
long=$(printf 'あ%.0s' $(seq 600))
run_op norm --reason $'  直す\tため\nに\x01読む  '"${long}" item get a
reason_line=$(payload_field '.facts.reason')
log_reason=$(last_field start '.reason')
if [[ ${log_reason} == "直す ため に 読む "* && ${#log_reason} == 501 && ${log_reason} == *… &&
  ${#reason_line} == 201 && ${reason_line} == "直す ため に 読む "* ]]; then
  ok "the reason folds into one line; the window gets 200 characters, the log 500"
else
  ng "the reason folds into one line; the window gets 200 characters, the log 500" "log(${#log_reason})=${log_reason:0:40} ui(${#reason_line})=${reason_line:0:40}"
fi

echo "== Calls without a reason"
for c in "--version" "--help" "item get --help" "help item"; do
  reset
  read -r -a words <<<"${c}"
  run_op help "${words[@]}"
  if [[ ${rc} == 0 && $(payload_field '.mode') == help && $(payload_field '.facts') == null &&
  $(last_field start '.kind') == help ]]; then
    ok "\"op ${c}\" without a reason runs in help mode (no window)"
  else
    ng "\"op ${c}\" without a reason runs in help mode (no window)" "rc=${rc} payload=$(payload_of) err=$(cat "${work}/help.err")"
  fi
done

echo "== No op arguments"
for c in "" "--reason 理由だけ"; do
  reset
  read -r -a words <<<"${c}"
  run_op noargs "${words[@]}"
  usage_text=$(<"${work}/noargs.out")
  if [[ ${rc} == 0 && ${usage_text} == *'op --reason "'* && ${usage_text} == *--version* &&
    ${usage_text} == *op-reason-log* && ${usage_text} == *'op --help'* && ! -s ${work}/noargs.err ]]; then
    ok "\"op ${c}\" prints the wrapper's usage on stdout, exit 0 (stderr empty)"
  else
    ng "\"op ${c}\" prints the wrapper's usage on stdout, exit 0 (stderr empty)" "rc=${rc} out=${usage_text} err=$(cat "${work}/noargs.err")"
  fi
  if [[ $(ps_count) == 0 && ! -e ${FAKE_DIR}/op.direct && ! -e ${FAKE_DIR}/op.args && ! -e ${log} ]]; then
    ok "\"op ${c}\" starts neither a runner nor op.exe (not even whoami) and logs nothing"
  else
    ng "\"op ${c}\" starts neither a runner nor op.exe (not even whoami) and logs nothing" "ps=$(ps_count) files=$(ls "${FAKE_DIR}") log=$(cat "${log}" 2>&1)"
  fi
done

echo "== The wrapper's own options"
reset
run_as op-reason ophelp --help
help_text=$(<"${work}/ophelp.out")
run_as op-reason opversion --version
if [[ ${rc} == 0 && $(<"${work}/opversion.out") == "op-reason "[0-9]* && ${help_text} == *op-reason-log* &&
$(ps_count) == 0 && ! -e ${log} ]]; then
  ok "op-reason --help and --version are the wrapper's (no runner, no log)"
else
  ng "op-reason --help and --version are the wrapper's (no runner, no log)" "rc=${rc} out=$(cat "${work}/opversion.out") ps=$(ps_count)"
fi
reset
OPREASON_LANG=en run_as op-reason ophelpen -h
detail="$(head -3 "${work}/ophelpen.out")"
check "OPREASON_LANG=en gives the English usage" grep -q '^Usage: op --reason' "${work}/ophelpen.out"
reset
run_op opdashhelp --help
detail="rc=${rc} payload=$(payload_of)"
check "op --help (under the name op) goes to op in help mode" test "$(payload_field '.mode')" == help -a "${rc}" == 0
reset
run_as op-reason skill --skill
skill_head=$(head -n 2 "${work}/skill.out")
run_as op-reason comp --completion fish
comp_ok=no
grep -q '^complete -c op ' "${work}/comp.out" && comp_ok=yes
run_as op-reason comp2 --completion fish op-reason
grep -q '^complete -c op-reason ' "${work}/comp2.out" || comp_ok=no
run_as op-reason compzsh --completion zsh
if [[ ${skill_head} == $'---\nname: op-reason' && ${comp_ok} == yes && ${rc} == 2 && $(ps_count) == 0 ]]; then
  ok "--skill prints the skill, --completion fish [name] the completion, --completion zsh exits 2"
else
  ng "--skill prints the skill, --completion fish [name] the completion, --completion zsh exits 2" "skill=${skill_head} comp=${comp_ok} rc=${rc}"
fi
reset
run_as op-reason-log logname
detail="rc=${rc} out=$(cat "${work}/logname.out")"
check "op-reason-log (by its name) runs the log viewer" test "${rc}" == 0 -a "$(grep -c 記録がありません "${work}/logname.out")" == 1

echo "== Passing through"
reset
stdin_file=${work}/in.bin
for i in $(seq 0 255); do
  printf -v oct '\\%03o' "${i}"
  printf '%b' "${oct}"
done >"${stdin_file}"
printf '\r\n\0end\n' >>"${stdin_file}"
export FAKE_RC=3 FAKE_STDERR="op の stderr"
run_op pass --reason "読むため" document get x
unset FAKE_RC FAKE_STDERR
stdin_file=""
detail="out=$(stat -c %s "${work}/pass.out")"
check "binary stdin goes through op to stdout unchanged" cmp -s "${work}/in.bin" "${work}/pass.out"
detail="$(cat "${work}/pass.err")"
check "stderr has only op's" test "$(<"${work}/pass.err")" == "op の stderr"
detail="rc=${rc}"
check "the exit code is op's" test "${rc}" == 3
if [[ $(<"${FAKE_DIR}/ps.1.argv") == --run && $(runner_of 1) == op-reason.exe && $(ps_count) == 1 ]] \
  && grep -qx 'SCRIPT=<unset>' <<<"$(env_of)"; then
  ok "the runner is op-reason.exe (found in PATH) with --run as its only argument, and no OPREASON_SCRIPT"
else
  ng "the runner is op-reason.exe (found in PATH) with --run as its only argument, and no OPREASON_SCRIPT" "runner=$(runner_of 1) argv=$(cat "${FAKE_DIR}/ps.1.argv") $(env_of)"
fi
detail="$(cat "${FAKE_DIR}/ps.1.sigblk" 2>&1)"
check "the runner starts with no blocked signals (a TERM or HUP to the group reaches it)" grep -qE '^SigBlk:[[:space:]]+0+$' "${FAKE_DIR}/ps.1.sigblk"

echo "== desktop"
reset
export WSLENV="WT_SESSION:WT_PROFILE_ID:"
export OP_CONNECT_HOST=https://connect.example
run_op desktop --reason "設定を読む" item get "a b"
unset WSLENV OP_CONNECT_HOST
if [[ $(payload_field '.mode') == desktop && $(payload_field '.op') == 'C:\fake'"${op_exe//\//\\}" &&
$(payload_field '.lang') == ja && $(payload_field '.lock') == held && $(payload_field '.v') == 1 ]] \
  && expect_payload_args item get "a b"; then
  ok "payload: mode desktop, op is the Windows path, args64 are op's arguments, lang, lock held"
else
  ng "payload: mode desktop, op is the Windows path, args64 are op's arguments, lang, lock held" "$(payload_of)"
fi
if grep -qx 'WSLENV=WT_SESSION:WT_PROFILE_ID:OPREASON_PAYLOAD' <<<"$(env_of)" \
  && grep -qx 'SA=<unset>' <<<"$(env_of)" && grep -qx 'CH=<unset>' <<<"$(env_of)"; then
  ok "WSLENV is the caller's plus OPREASON_PAYLOAD; no token or Connect variable"
else
  ng "WSLENV is the caller's plus OPREASON_PAYLOAD; no token or Connect variable" "$(env_of)"
fi
if [[ $(payload_field '.facts.reason') == 設定を読む && $(payload_field '.facts.command') == 'op item get "a b"' &&
$(payload_field '.facts.time') =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}\ [0-9]{2}:[0-9]{2}:[0-9]{2}$ &&
$(payload_field '.facts.place.cwd') != "" && $(payload_field '.facts.caller.type') == human ]]; then
  ok "facts: time, caller, place, reason, command"
else
  ng "facts: time, caller, place, reason, command" "$(payload_field '.facts')"
fi

echo "== service account and Connect"
reset
export OP_SERVICE_ACCOUNT_TOKEN=tok-SECRET
run_op sa --reason "CI で読む" vault list
unset OP_SERVICE_ACCOUNT_TOKEN
if [[ ${rc} == 0 && $(payload_field '.mode') == service && $(payload_field '.facts') == null ]] \
  && grep -qx 'WSLENV=OPREASON_PAYLOAD:OP_SERVICE_ACCOUNT_TOKEN/w:OP_BIOMETRIC_UNLOCK_ENABLED/w' <<<"$(env_of)" \
  && grep -qx 'SA=tok-SECRET' <<<"$(env_of)" && grep -qx 'BIO=false' <<<"$(env_of)" \
  && [[ $(last_field start '.kind') == service-account ]]; then
  ok "service account: service mode, token and OP_BIOMETRIC_UNLOCK_ENABLED=false passed with /w, kind service-account"
else
  ng "service account: service mode, token and OP_BIOMETRIC_UNLOCK_ENABLED=false passed with /w, kind service-account" "rc=${rc} $(env_of) $(payload_of)"
fi
reset
export OP_CONNECT_HOST=https://connect.example OP_CONNECT_TOKEN=ctok-SECRET
run_op connect --reason "Connect で読む" item get a
unset OP_CONNECT_HOST OP_CONNECT_TOKEN
if [[ $(payload_field '.mode') == service && $(last_field start '.kind') == connect ]] \
  && grep -qx 'WSLENV=OPREASON_PAYLOAD:OP_CONNECT_HOST/w:OP_CONNECT_TOKEN/w:OP_BIOMETRIC_UNLOCK_ENABLED/w' <<<"$(env_of)" \
  && grep -qx 'SA=<unset>' <<<"$(env_of)"; then
  ok "Connect with both variables: service (kind connect)"
else
  ng "Connect with both variables: service (kind connect)" "$(env_of) $(payload_of)"
fi
detail="$(cat "${log}")"
check "tokens are never logged" test "$(grep -cE 'tok-SECRET|ctok-SECRET' "${log}")" == 0

echo "== Masking"
reset
run_op mask --reason "項目を作る" item create password=hunter2 --session sess1 --secret-key=A3-XYZ op://v/i/f?attribute=otp
masked=$(last_field start '.args | join(" ")')
cmdline=$(payload_field '.facts.command')
if [[ ${masked} == 'item create password=*** --session *** --secret-key=*** op://v/i/f?attribute=otp' &&
  ${cmdline} == "op ${masked}" ]] && ! grep -qE 'hunter2|sess1|A3-XYZ' "${log}" \
  && ! grep -qE 'hunter2|sess1|A3-XYZ' <<<"$(payload_field '.facts')" \
  && expect_payload_args item create password=hunter2 --session sess1 --secret-key=A3-XYZ op://v/i/f?attribute=otp; then
  ok "password=…, --session and --secret-key values become *** in the log and the facts; op gets them unchanged"
else
  ng "password=…, --session and --secret-key values become *** in the log and the facts; op gets them unchanged" "log=${masked} ui=${cmdline}"
fi

echo "== Results"
check_result() {
  local want=$1
  reset
  run_op result --reason "結果を見る" item get a
  local id_s id_e secs
  id_s=$(last_field start '.id')
  id_e=$(last_field end '.id')
  secs=$(last_field end '.seconds')
  if [[ $(last_field end '.result') == "${want}" && -n ${id_s} && ${id_s} == "${id_e}" && ${secs} =~ ^[0-9]+$ ]]; then
    ok "$2 → ${want} (start and end share the id)"
  else
    ng "$2 → ${want} (start and end share the id)" "$(cat "${log}" 2>&1)"
  fi
}
check_result approved "op exits 0"
export FAKE_RC=1 FAKE_STDERR="[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again"
check_result denied "the denial text"
export FAKE_STDERR="[ERROR] 2026/11/08 00:00:00 authorization timeout"
check_result timeout "the timeout text"
export FAKE_STDERR="[ERROR] 2026/11/08 00:00:00 \"a\" isn't an item"
check_result failed "another failure"
export FAKE_STDERR="curl: (28) Connection timed out after 1000 ms"
check_result failed "a child command's \"Connection timed out\""
unset FAKE_RC FAKE_STDERR
export OPREASON_RUNNER=launcher FAKE_PS_NO_STATUS=1
check_result launcher-failed "the launcher writes no status"
unset FAKE_PS_NO_STATUS
export FAKE_PS_JUNK_STATUS=1
check_result launcher-failed "the launcher writes a status that is not JSON"
unset FAKE_PS_JUNK_STATUS OPREASON_RUNNER

echo "== Caller"
reset
make_proc claude
export HERDR_ENV=1 HERDR_WORKSPACE_ID=wX HERDR_PANE_ID=wX:p1 CLAUDE_CODE_SESSION_ID=s1
git init -q -b feat/x "${work}/repo"
git -C "${work}/repo" remote add origin git@github.com:o/r.git
(cd "${work}/repo" && "${op}" --reason "呼び手を見る" item get a </dev/null >/dev/null 2>&1)
unset HERDR_ENV HERDR_WORKSPACE_ID HERDR_PANE_ID CLAUDE_CODE_SESSION_ID
start=$(last start)
if [[ $(jq -r '.caller.type' <<<"${start}") == claude && $(jq -r '.caller.claude_pid' <<<"${start}") == 999 &&
$(jq -r '.caller.herdr.label' <<<"${start}") == "作業 1 テスト" && $(jq -r '.caller.herdr.pane' <<<"${start}") == wX:p1 &&
$(jq -r '.caller.repo' <<<"${start}") == o/r && $(jq -r '.caller.branch' <<<"${start}") == feat/x &&
$(jq -r '.session' <<<"${start}") == s1 && $(last_field end '.session') == s1 ]]; then
  ok "claude ancestor, herdr label and pane, git repo and branch, session"
else
  ng "claude ancestor, herdr label and pane, git repo and branch, session" "${start}"
fi
if [[ $(payload_field '.facts.herdr.label') == "作業 1 テスト" && $(payload_field '.facts.herdr.pane') == wX:p1 &&
$(payload_field '.facts.caller.type') == claude && $(payload_field '.facts.caller.pid') == 999 &&
$(payload_field '.facts.caller.session') == s1 && $(payload_field '.facts.place.repo') == o/r &&
$(payload_field '.facts.place.branch') == feat/x ]]; then
  ok "facts carry Claude Code (pid), herdr workspace and pane, repo and branch"
else
  ng "facts carry Claude Code (pid), herdr workspace and pane, repo and branch" "$(payload_field '.facts')"
fi
reset
make_proc fish
run_op human --reason "人が打つ" item get a
if [[ $(last_field start '.caller.type') == human && $(last_field start '.caller.herdr') == null &&
$(payload_field '.facts.caller.type') == human && $(payload_field '.facts.herdr') == null ]]; then
  ok "no claude ancestor → human; outside herdr, herdr is null"
else
  ng "no claude ancestor → human; outside herdr, herdr is null" "$(last start)"
fi

echo "== Lock"
reset
mkdir -p "${state}"
printf '%s\n' '{"time":"00:00:00","caller":"Claude Code","reason":"前の理由","command":"op item get z"}' >"${state}/current.json"
flock "${state}/lock" sleep 3 &
holder=$!
sleep 0.5
export OPREASON_LOCK_WAIT=1
run_op lock --reason "待つ" item get a
unset OPREASON_LOCK_WAIT
err=$(<"${work}/lock.err")
if [[ ${rc} == 75 && ${err} == *"1 秒"* && ${err} == *前の理由* && $(ps_count) == 0 &&
$(last_field refused '.why') == lock-timeout ]]; then
  ok "past the lock limit: exit 75 with the seconds and the holder's reason, no runner, refused (lock-timeout)"
else
  ng "past the lock limit: exit 75 with the seconds and the holder's reason, no runner, refused (lock-timeout)" "rc=${rc} ps=$(ps_count) err=${err}"
fi
wait "${holder}" 2>/dev/null
reset
mkdir -p "${state}"
printf 'not json' >"${state}/current.json"
flock "${state}/lock" sleep 3 &
holder=$!
sleep 0.5
OPREASON_LOCK_WAIT=1 run_op lockjunk --reason "待つ" item get a
detail="rc=${rc} err=$(cat "${work}/lockjunk.err")"
check "a broken current.json: the holder is \"(分からない)\"" test "${rc}" == 75 -a "$(grep -c '(分からない)' "${work}/lockjunk.err")" == 1
wait "${holder}" 2>/dev/null

echo "== Shared approval watchdog"
reset
touch "${FAKE_DIR}/shared"
export HERDR_ENV=1 HERDR_WORKSPACE_ID=wX HERDR_PANE_ID=wX:p1
run_op shared --reason "見張りを見る" item get a
unset HERDR_ENV HERDR_WORKSPACE_ID HERDR_PANE_ID
notify_file=""
for _ in $(seq 50); do
  for f in "${FAKE_DIR}"/notify.*.payload; do
    [[ ! -e ${f} ]] || notify_file=${f}
  done
  [[ -z ${notify_file} ]] || break
  sleep 0.1
done
if grep -qx 'signout --all' "${FAKE_DIR}/op.direct" && [[ $(last_field shared-approval '.when') == start &&
$(last_field shared-approval '.cleared') == true && $(last_field end '.watch.start') == cleared &&
${notify_file} == */notify.op-reason.exe.* && $(jq -r '.notify.cleared' "${notify_file}" 2>/dev/null) == true &&
-s ${FAKE_DIR}/herdr.notify && $(payload_field '.facts.notes[0].kind') == shared-cleared ]]; then
  ok "a direct whoami that succeeds → signout --all, logged, notified (the runner's notify and herdr), noted in the window"
else
  ng "a direct whoami that succeeds → signout --all, logged, notified (the runner's notify and herdr), noted in the window" \
    "direct=$(tr '\n' ' ' <"${FAKE_DIR}/op.direct" 2>&1) notify=${notify_file} herdr=$(cat "${FAKE_DIR}/herdr.notify" 2>&1) log=$(cat "${log}")"
fi
reset
run_op noshared --reason "見張りを見る" item get a
if ! grep -q '^signout' "${FAKE_DIR}/op.direct" 2>/dev/null && [[ $(grep -c whoami "${FAKE_DIR}/op.direct") == 2 &&
$(last_field end '.watch.start') == none && $(last_field end '.watch.end') == none ]]; then
  ok "a direct whoami that fails: no signout (checked at start and end)"
else
  ng "a direct whoami that fails: no signout (checked at start and end)" "$(tr '\n' ' ' <"${FAKE_DIR}/op.direct" 2>&1)"
fi

echo "== Missing tools"
reset
OPREASON_OP_EXE=${work}/win/no-such-op.exe run_op noop --reason "無い" item get a
if [[ ${rc} == 127 && $(last_field refused '.why') == no-op-exe && $(ps_count) == 0 ]]; then
  ok "op.exe missing → 127 (refused no-op-exe)"
else
  ng "op.exe missing → 127 (refused no-op-exe)" "rc=${rc} err=$(cat "${work}/noop.err")"
fi
reset
OPREASON_POWERSHELL=${work}/no-such-powershell.exe run_op nops --reason "無い" item get a
if [[ ${rc} == 0 && $(runner_of 1) == op-reason.exe && $(last_field end '.result') == approved ]]; then
  ok "powershell.exe missing does not matter while op-reason.exe runs"
else
  ng "powershell.exe missing does not matter while op-reason.exe runs" "rc=${rc} err=$(cat "${work}/nops.err")"
fi
reset
OPREASON_RUNNER=launcher OPREASON_POWERSHELL=${work}/no-such-powershell.exe run_op nops2 --reason "無い" item get a
if [[ ${rc} == 127 && $(last_field refused '.why') == no-powershell && $(ps_count) == 0 ]]; then
  ok "launcher mode with powershell.exe missing → 127 (refused no-powershell)"
else
  ng "launcher mode with powershell.exe missing → 127 (refused no-powershell)" "rc=${rc} err=$(cat "${work}/nops2.err")"
fi
reset
printf '#!/nonexistent/interpreter\n' >"${work}/broken-powershell.exe"
chmod +x "${work}/broken-powershell.exe"
OPREASON_RUNNER=launcher OPREASON_POWERSHELL=${work}/broken-powershell.exe run_op cannotstart --reason "起こせない" item get a
if [[ ${rc} == 127 && $(<"${work}/cannotstart.err") == *起こせません* && $(last_field end '.result') == launcher-failed ]]; then
  ok "powershell.exe that cannot be started (launcher mode) → 127 with a message, end launcher-failed"
else
  ng "powershell.exe that cannot be started (launcher mode) → 127 with a message, end launcher-failed" "rc=${rc} err=$(cat "${work}/cannotstart.err") log=$(cat "${log}" 2>&1)"
fi

echo "== The runner: op-reason.exe and the fallback"
# winpath <path>: the fake wslpath's Windows form
winpath() {
  local p=${1//\//\\}
  printf 'C:\\fake%s' "${p}"
}
# fell_back <why>: end runner launcher with that fallback reason
fell_back() {
  [[ $(last_field end '.runner') == launcher && $(last_field end '.fallback.why') == "$1" ]]
}
stdin_file=${work}/runner-in.txt
printf 'line 1\nline 2 の入力\n' >"${stdin_file}"

reset
run_op exeok --reason "exe で打つ" item get a
if [[ ${rc} == 0 && $(ps_count) == 1 && $(runner_of 1) == op-reason.exe && $(op_runs) == 1 &&
$(last_field end '.runner') == exe ]] && ! jq -e 'select(.event == "end") | has("fallback")' "${log}" >/dev/null \
  && cmp -s "${stdin_file}" "${work}/exeok.out"; then
  ok "auto: op-reason.exe runs the call (end runner exe, no fallback field), stdin reaches op"
else
  ng "auto: op-reason.exe runs the call (end runner exe, no fallback field), stdin reaches op" "rc=${rc} ps=$(ps_count) runner=$(runner_of 1) op=$(op_runs) log=$(cat "${log}")"
fi

reset
OPREASON_RUNNER=launcher run_op launcherok --reason "launcher で打つ" item get a
want_argv=$(printf '%s\n' -NoProfile -NonInteractive -ExecutionPolicy Bypass -OutputFormat Text -File "$(winpath "${FAKE_WIN}/launcher.ps1")")
if [[ ${rc} == 0 && $(ps_count) == 1 && $(runner_of 1) == powershell.exe && $(<"${FAKE_DIR}/ps.1.argv") == "${want_argv}" &&
$(op_runs) == 1 && $(last_field end '.runner') == launcher && $(last_field end '.fallback') == null ]]; then
  ok "launcher mode: powershell.exe -NoProfile … -File <Windows path of launcher.ps1 next to op-reason.exe>"
else
  ng "launcher mode: powershell.exe -NoProfile … -File <Windows path of launcher.ps1 next to op-reason.exe>" "argv=$(cat "${FAKE_DIR}/ps.1.argv") want=${want_argv} log=$(cat "${log}")"
fi

reset
FAKE_EXE=blocked FAKE_RC=4 run_op blocked --reason "止められた exe" item get a
err=$(<"${work}/blocked.err")
if [[ ${rc} == 4 && $(ps_count) == 1 && $(runner_of 1) == powershell.exe && $(op_runs) == 1 &&
${err} == *"fake: op-reason.exe was blocked"* &&
${err} == *"op-reason.exe が呼び出しを始めずに終わった (終了コード 1) ので、launcher.ps1 で打ち直します"* &&
${err} == *OPREASON_RUNNER=launcher* ]] && fell_back no-status && [[ $(last_field end '.fallback.exit') == 1 ]] \
  && cmp -s "${stdin_file}" "${work}/blocked.out"; then
  ok "an exe that writes no status → the launcher runs once with the same stdin; one stderr line; end fallback no-status (exit 1)"
else
  ng "an exe that writes no status → the launcher runs once with the same stdin; one stderr line; end fallback no-status (exit 1)" "rc=${rc} ps=$(ps_count) op=$(op_runs) err=${err} log=$(cat "${log}")"
fi

reset
FAKE_EXE=marker run_op marker --reason "印だけの exe" item get a
if [[ ${rc} == 1 && $(ps_count) == 0 && $(op_runs) == 0 && $(last_field end '.result') == launcher-failed &&
$(last_field end '.runner') == exe && $(last_field end '.fallback') == null && $(<"${work}/marker.err") != *launcher.ps1* ]]; then
  ok "an exe that wrote the status file and failed before op.exe: no launcher, end launcher-failed"
else
  ng "an exe that wrote the status file and failed before op.exe: no launcher, end launcher-failed" "rc=${rc} ps=$(ps_count) op=$(op_runs) err=$(cat "${work}/marker.err") log=$(cat "${log}")"
fi

reset
FAKE_EXE=die-after-op run_op dieafter --reason "op の後に落ちる exe" item get a
if [[ ${rc} == 9 && $(ps_count) == 1 && $(runner_of 1) == op-reason.exe && $(op_runs) == 1 &&
$(last_field end '.result') == launcher-failed && $(last_field end '.fallback') == null ]]; then
  ok "an exe that dies after op.exe without the final status: op.exe ran once, no launcher"
else
  ng "an exe that dies after op.exe without the final status: op.exe ran once, no launcher" "rc=${rc} ps=$(ps_count) op=$(op_runs) log=$(cat "${log}")"
fi

reset
FAKE_EXE=rundir-gone run_op rundirgone --reason "run の dir を消される" item get a
if [[ ${rc} == 1 && $(ps_count) == 0 && $(op_runs) == 0 && $(last_field end '.result') == launcher-failed &&
$(last_field end '.fallback') == null && $(<"${work}/rundirgone.err") != *launcher.ps1* ]]; then
  ok "no fallback when the run directory is gone (a missing status file proves nothing then)"
else
  ng "no fallback when the run directory is gone (a missing status file proves nothing then)" "rc=${rc} ps=$(ps_count) op=$(op_runs) err=$(cat "${work}/rundirgone.err") log=$(cat "${log}")"
fi

reset
FAKE_WSLPATH_FAIL=status.json run_op nowslpath --reason "変換できない" item get a
if [[ ${rc} == 127 && $(ps_count) == 0 && $(op_runs) == 0 && $(last_field refused '.why') == no-wslpath &&
$(<"${work}/nowslpath.err") == *"wslpath -w で"*"status.json を変換できなかった"* ]]; then
  ok "wslpath -w gives nothing for the status file → refused (no-wslpath), no runner"
else
  ng "wslpath -w gives nothing for the status file → refused (no-wslpath), no runner" "rc=${rc} ps=$(ps_count) err=$(cat "${work}/nowslpath.err") log=$(cat "${log}" 2>&1)"
fi

reset
mkdir -p "${work}/badexe"
printf '#!/nonexistent/interpreter\n' >"${work}/badexe/op-reason.exe"
chmod +x "${work}/badexe/op-reason.exe"
touch "${work}/badexe/launcher.ps1"
OPREASON_EXE=${work}/badexe/op-reason.exe run_op notstarted --reason "起こせない exe" item get a
if [[ ${rc} == 0 && $(ps_count) == 1 && $(runner_of 1) == powershell.exe && $(op_runs) == 1 &&
$(<"${work}/notstarted.err") == *"op-reason.exe を起こせなかった ("* ]] && fell_back not-started; then
  ok "an exe that cannot be started → the launcher next to it; end fallback not-started"
else
  ng "an exe that cannot be started → the launcher next to it; end fallback not-started" "rc=${rc} err=$(cat "${work}/notstarted.err") log=$(cat "${log}")"
fi

reset
mkdir -p "${work}/onlylauncher"
touch "${work}/onlylauncher/launcher.ps1"
OPREASON_EXE=${work}/onlylauncher/op-reason.exe run_op notfound --reason "exe の無いフォルダ" item get a
if [[ ${rc} == 0 && $(ps_count) == 1 && $(runner_of 1) == powershell.exe && $(op_runs) == 1 &&
$(<"${FAKE_DIR}/ps.1.argv") == *"$(winpath "${work}/onlylauncher/launcher.ps1")" &&
$(<"${work}/notfound.err") == *"op-reason.exe が見つからない (${work}/onlylauncher/op-reason.exe) ので、launcher.ps1 で打ちます"* ]] \
  && fell_back not-found && [[ $(last_field end '.fallback.exit') == null ]]; then
  ok "auto without the exe where OPREASON_EXE points, launcher.ps1 there → the launcher; end fallback not-found"
else
  ng "auto without the exe where OPREASON_EXE points, launcher.ps1 there → the launcher; end fallback not-found" "rc=${rc} err=$(cat "${work}/notfound.err") log=$(cat "${log}")"
fi

reset
mkdir -p "${work}/nothing"
OPREASON_EXE=${work}/nothing/op-reason.exe run_op nothing --reason "何も無い" item get a
if [[ ${rc} == 127 && $(ps_count) == 0 && $(last_field refused '.why') == no-exe &&
$(<"${work}/nothing.err") == *"op-reason.exe が見つかりません (${work}/nothing/op-reason.exe)"* ]]; then
  ok "neither the exe nor launcher.ps1 → 127 (refused no-exe)"
else
  ng "neither the exe nor launcher.ps1 → 127 (refused no-exe)" "rc=${rc} err=$(cat "${work}/nothing.err")"
fi

reset
OPREASON_RUNNER=exe FAKE_EXE=blocked run_op exeonly --reason "exe だけ" item get a
if [[ ${rc} == 1 && $(ps_count) == 0 && $(op_runs) == 0 && $(last_field end '.result') == launcher-failed &&
$(last_field end '.runner') == exe && $(last_field end '.fallback') == null &&
$(<"${work}/exeonly.err") == *"OPREASON_RUNNER=exe なので launcher.ps1 には回しません"* ]]; then
  ok "exe mode never falls back (an exe that writes no status → end launcher-failed, one line says why)"
else
  ng "exe mode never falls back (an exe that writes no status → end launcher-failed, one line says why)" "rc=${rc} ps=$(ps_count) err=$(cat "${work}/exeonly.err") log=$(cat "${log}")"
fi
reset
OPREASON_RUNNER=exe OPREASON_EXE=${work}/onlylauncher/op-reason.exe run_op exemissing --reason "exe だけ" item get a
detail="rc=${rc} err=$(cat "${work}/exemissing.err")"
check "exe mode without the exe → 127 (refused no-exe), even with launcher.ps1 there" \
  test "${rc}" == 127 -a "$(last_field refused '.why')" == no-exe -a "$(ps_count)" == 0

reset
mkdir -p "${work}/nolauncher"
cp "${FAKE_WIN}/op-reason.exe" "${work}/nolauncher/"
OPREASON_RUNNER=launcher OPREASON_EXE=${work}/nolauncher/op-reason.exe run_op nolauncher --reason "launcher が無い" item get a
if [[ ${rc} == 127 && $(ps_count) == 0 && $(last_field refused '.why') == no-launcher &&
$(<"${work}/nolauncher.err") == *"launcher.ps1 が見つかりません (${work}/nolauncher/launcher.ps1)"* ]]; then
  ok "launcher mode without launcher.ps1 next to op-reason.exe → 127 (refused no-launcher)"
else
  ng "launcher mode without launcher.ps1 next to op-reason.exe → 127 (refused no-launcher)" "rc=${rc} err=$(cat "${work}/nolauncher.err")"
fi

reset
FAKE_EXE=blocked OPREASON_POWERSHELL=${work}/no-such-powershell.exe run_op nofallback --reason "回避も無い" item get a
err=$(<"${work}/nofallback.err")
if [[ ${rc} == 127 && $(ps_count) == 0 && $(op_runs) == 0 && $(last_field end '.result') == launcher-failed &&
${err} == *"回避経路も使えません"* && ${err} == *no-such-powershell.exe* ]]; then
  ok "the fallback is needed but powershell.exe is missing → 127, end launcher-failed, both lines"
else
  ng "the fallback is needed but powershell.exe is missing → 127, end launcher-failed, both lines" "rc=${rc} err=${err} log=$(cat "${log}")"
fi
stdin_file=""

reset
touch "${FAKE_DIR}/shared"
FAKE_EXE=blocked run_op notifyfb --reason "見張りの回避" item get a
notify_file=""
for _ in $(seq 50); do
  for f in "${FAKE_DIR}"/notify.powershell.exe.*.payload; do
    [[ ! -e ${f} ]] || notify_file=${f}
  done
  [[ -z ${notify_file} ]] || break
  sleep 0.1
done
if [[ -n ${notify_file} && $(jq -r '.notify.when' "${notify_file}") == start && $(jq -r '.status' "${notify_file}") == null ]] \
  && fell_back no-status; then
  ok "the shared approval toast falls back to the launcher too when the exe writes nothing"
else
  ng "the shared approval toast falls back to the launcher too when the exe writes nothing" "files=$(ls "${FAKE_DIR}") log=$(cat "${log}")"
fi

reset
export OPREASON_LOCK_WAIT=8 FAKE_EXE=blocked
head -c 300000 /dev/zero | tr '\0' z >"${work}/fb-big.out"
{
  FAKE_OUT_FILE=${work}/fb-big.out "${op}" --reason "回避で送る" item list </dev/null 2>/dev/null \
    | { wait_file "${FAKE_DIR}/op.args" && FAKE_OUT="" "${op}" --reason "回避で受ける" item get - 2>/dev/null; }
} >"${work}/fb-pipe.out"
unset OPREASON_LOCK_WAIT FAKE_EXE
if cmp -s "${work}/fb-pipe.out" "${work}/fb-big.out" && [[ $(op_runs) == 2 &&
$(jq -c 'select(.event == "end") | .fallback.why' "${log}" | sort -u) == '"no-status"' ]]; then
  ok "stdin drained from another op-reason goes to the launcher again after a fallback (op.exe once per call)"
else
  ng "stdin drained from another op-reason goes to the launcher again after a fallback (op.exe once per call)" "out=$(stat -c %s "${work}/fb-pipe.out") op=$(op_runs) log=$(cat "${log}")"
fi

echo "== TERM"
reset
export FAKE_SLEEP=2
"${op}" --reason "止める" item get a </dev/null >"${work}/term.out" 2>"${work}/term.err" &
pid=$!
for _ in $(seq 50); do
  [[ -e ${FAKE_DIR}/op.args ]] && break
  sleep 0.1
done
kill -TERM "${pid}"
rc=0
wait "${pid}" || rc=$?
unset FAKE_SLEEP
if [[ ${rc} == 143 && $(last_field end '.result') == interrupted && $(last_field end '.exit') == 143 ]]; then
  ok "TERM → end interrupted, exit 143"
else
  ng "TERM → end interrupted, exit 143" "rc=${rc} log=$(cat "${log}" 2>&1)"
fi
ps_pid=$(cat "${FAKE_DIR}/ps.1.pid" 2>/dev/null)
for _ in $(seq 40); do
  if [[ -z ${ps_pid} ]] || ! kill -0 "${ps_pid}" 2>/dev/null; then
    break
  fi
  sleep 0.05
done
if [[ -n ${ps_pid} ]] && ! kill -0 "${ps_pid}" 2>/dev/null; then
  ok "TERM to the wrapper alone is forwarded to the runner"
else
  ng "TERM to the wrapper alone is forwarded to the runner" "the fake runner ${ps_pid} still runs"
fi
sleep 2.5

echo "== Rotation and cleanup"
reset
mkdir -p "${state}"
pad=$(printf 'x%.0s' $(seq 1000))
for _ in $(seq 1100); do
  printf '{"v":1,"event":"pad","pad":"%s"}\n' "${pad}"
done >"${log}"
run_op rotate --reason "回す" item get a
if [[ -s ${state}/log.1.jsonl && $(jq -r .event "${log}" | tr '\n' ' ') == "start end " ]]; then
  ok "over 1 MiB the log moves to log.1.jsonl"
else
  ng "over 1 MiB the log moves to log.1.jsonl" "$(jq -r .event "${log}" | sort | uniq -c)"
fi
leftover=$(cd "${state}" && find . -mindepth 1 ! -name log.jsonl ! -name log.1.jsonl ! -name log.lock ! -name lock ! -path ./run | sort)
if [[ -z $(ls -A "${state}/run" 2>/dev/null) && -z ${leftover} ]]; then
  ok "nothing is left under the state directory after a call (run/ empty, no current.json, no temporary files)"
else
  ng "nothing is left under the state directory after a call (run/ empty, no current.json, no temporary files)" "${leftover} $(ls -lR "${state}/run")"
fi

echo "== Masking after --, too long"
reset
run_op runmask --reason "子のコマンド" run -- curl -s -H "Authorization: Bearer tok-RUN" https://example.com/?k=v
if [[ $(last_field start '.args | join(" ")') == 'run -- curl -s -H *** ***' ]] && ! grep -q 'tok-RUN' "${log}"; then
  ok "after --, only the command name and value-less flags stay"
else
  ng "after --, only the command name and value-less flags stay" "$(last_field start '.args')"
fi
reset
big=$(printf 'x%.0s' $(seq 25000))
run_op toolong --reason "長い" item create "notesPlain=${big}"
if [[ ${rc} == 2 && $(ps_count) == 0 && $(last_field refused '.why') == too-long && $(<"${work}/toolong.err") == *長すぎます* ]]; then
  ok "arguments that are too long: exit 2, no runner"
else
  ng "arguments that are too long: exit 2, no runner" "rc=${rc} ps=$(ps_count) err=$(cat "${work}/toolong.err")"
fi

echo "== Stopped while waiting for the lock"
reset
mkdir -p "${state}"
flock "${state}/lock" sleep 3 &
holder=$!
sleep 0.3
OPREASON_LOCK_WAIT=10 "${op}" --reason "待つ間に止める" item get a </dev/null >/dev/null 2>"${work}/lockterm.err" &
pid=$!
sleep 0.7
kill -TERM "${pid}"
rc=0
wait "${pid}" || rc=$?
if [[ ${rc} == 143 && $(last_field refused '.why') == interrupted && $(ps_count) == 0 ]]; then
  ok "TERM while waiting for the lock → exit 143, refused (interrupted)"
else
  ng "TERM while waiting for the lock → exit 143, refused (interrupted)" "rc=${rc} log=$(cat "${log}" 2>&1)"
fi
wait "${holder}" 2>/dev/null

echo "== Pipes"
# The sender (item list, reason 送る) feeds the receiver (item get -, reason 受ける). The start order is fixed so that
# either one takes the lock first. A deadlock makes the waiting side give up after OPREASON_LOCK_WAIT (8 seconds: never
# reached when nothing is stuck, even on a busy CI) with 75, and the receiver goes on with no or partial input. The exit
# marks (A-rc=, B-rc=) go to <name>.err, the receiver's stdout to <name>.out
export OPREASON_LOCK_WAIT=8
# Larger than a pipe buffer (64 KiB) plus what cat in between holds (a 128 KiB read and another pipe)
big=${work}/big.out
head -c 1048576 /dev/zero | tr '\0' x >"${big}"
printf '\nend-of-A\n' >>"${big}"
printf 'from-A' >"${work}/small.out"

send() {
  FAKE_OUT="" "${op}" --reason "送る" item list </dev/null 2>/dev/null || echo "A-rc=$?" >&2
}
recv() {
  FAKE_OUT="" FAKE_OUT_FILE="" "${op}" --reason "受ける" item get - 2>/dev/null || echo "B-rc=$?" >&2
}
pipe_check() {
  if cmp -s "${work}/$1.out" "$2" && [[ ! -s ${work}/$1.err ]]; then
    ok "$3"
  else
    ng "$3" "out=$(stat -c %s "${work}/$1.out") bytes (want $(stat -c %s "$2")) err=$(tr '\n' ' ' <"${work}/$1.err")"
  fi
}
start_of() {
  jq -c --arg r "$1" 'select(.event == "start" and .reason == $r)' "${log}" 2>/dev/null | tail -n 1
}
# payload_of_reason <reason>: the payload of the call with that reason
payload_of_reason() {
  local f
  for f in "${FAKE_DIR}"/ps.*.payload; do
    [[ -e ${f} ]] || continue
    jq -c --arg r "$1" 'select(.facts.reason == $r)' "${f}" 2>/dev/null
  done
}

reset
{ FAKE_OUT_FILE=${big} send | { wait_file "${FAKE_DIR}/op.args" && recv; }; } >"${work}/pipe-a.out" 2>"${work}/pipe-a.err"
pipe_check pipe-a "${big}" "the sender takes the lock first: the receiver drains it, larger than a pipe buffer"

reset
mkdir -p "${state}"
flock "${state}/lock" sleep 2 &
holder=$!
sleep 0.2
{ { sleep 0.5 && FAKE_OUT_FILE=${big} send; } | recv; } >"${work}/pipe-c.out" 2>"${work}/pipe-c.err"
wait "${holder}" 2>/dev/null
pipe_check pipe-c "${big}" "the sender starts late and takes the lock first: the receiver finds it while waiting"

reset
{ { wait_file "${FAKE_DIR}/op.args" && FAKE_OUT_FILE=${work}/small.out send; } | recv; } >"${work}/pipe-b.out" 2>"${work}/pipe-b.err"
pipe_check pipe-b "${work}/small.out" "the receiver takes the lock first: the sender runs without the lock"
sender=$(payload_of_reason 送る)
receiver=$(payload_of_reason 受ける)
if [[ $(jq -r '.lock' <<<"$(start_of 送る)") == skipped && $(jq -r '.lock' <<<"$(start_of 受ける)") == held &&
$(jq -r '.lock' <<<"${sender}") == skipped && $(jq -r '.lock' <<<"${receiver}") == held &&
$(jq -r '.facts.notes[] | select(.kind == "lock-skipped") | .holderReason' <<<"${sender}") == 受ける &&
$(jq -r '.facts.notes | length' <<<"${receiver}") == 0 ]]; then
  ok "the sender that skipped the lock: start lock skipped, payload lock skipped, a lock-skipped note with the receiver's reason"
else
  ng "the sender that skipped the lock: start lock skipped, payload lock skipped, a lock-skipped note with the receiver's reason" \
    "send=$(start_of 送る) recv=$(start_of 受ける) payload=${sender}"
fi

reset
{ FAKE_OUT_FILE=${big} send | cat | { wait_file "${FAKE_DIR}/op.args" && recv; }; } >"${work}/pipe-d1.out" 2>"${work}/pipe-d1.err"
pipe_check pipe-d1 "${big}" "with cat in between, the sender first: the receiver drains it"
reset
{ { wait_file "${FAKE_DIR}/op.args" && FAKE_OUT_FILE=${work}/small.out send; } | cat | recv; } >"${work}/pipe-d2.out" 2>"${work}/pipe-d2.err"
pipe_check pipe-d2 "${work}/small.out" "with cat in between, the receiver first: the sender runs without the lock"

# A pipe without op behind it: the writer (sleep) stays open for 60 seconds. Draining it would not end before the
# writer closes (waits 30 seconds to see)
hold_open() {
  sleep 60 &
  echo $! >"${work}/writer.pid"
  wait
}
nonop_recv() {
  FAKE_NO_STDIN=1 "${op}" --reason "閉じないパイプ" item get a >/dev/null 2>&1
  echo "rc=$?" >"${work}/nonop.rc"
}
for contended in 0 1; do
  reset
  holder=""
  if ((contended)); then
    mkdir -p "${state}"
    flock "${state}/lock" sleep 1 &
    holder=$!
    sleep 0.2
  fi
  hold_open | nonop_recv &
  nonop=$!
  label="a pipe without op behind it is not drained; the call ends before the writer closes"
  ((contended == 0)) || label+=" (after waiting for the lock too)"
  if wait_file "${work}/nonop.rc" 600 && [[ $(<"${work}/nonop.rc") == rc=0 ]]; then
    ok "${label}"
  else
    ng "${label}" "rc=$(cat "${work}/nonop.rc" 2>&1) log=$(cat "${log}" 2>&1)"
  fi
  kill "$(<"${work}/writer.pid")" 2>/dev/null
  wait "${nonop}" 2>/dev/null
  [[ -z ${holder} ]] || wait "${holder}" 2>/dev/null
done

# Another op holds the lock (does not read our output; holds it 30 seconds, then the whole process group is stopped).
# A sender in a pipe still waits and gives up after the limit (1 second) with 75
reset
FAKE_SLEEP=30 setsid "${op}" --reason "ほかの pane" item get z </dev/null >/dev/null 2>&1 &
other=$!
wait_file "${FAKE_DIR}/op.args"
OPREASON_LOCK_WAIT=1 FAKE_OUT="" "${op}" --reason "送る" item list </dev/null 2>"${work}/unrelated.err" | cat >/dev/null
rc=${PIPESTATUS[0]}
kill -TERM -- "-${other}" 2>/dev/null
wait "${other}" 2>/dev/null
if [[ ${rc} == 75 && $(last_field refused '.why') == lock-timeout && $(<"${work}/unrelated.err") == *ほかの\ pane* ]]; then
  ok "a lock holder that does not read our output: a sender in a pipe waits and gives up with 75"
else
  ng "a lock holder that does not read our output: a sender in a pipe waits and gives up with 75" "rc=${rc} err=$(cat "${work}/unrelated.err")"
fi

pipe_ok=1
for i in 1 2 3 4; do
  reset
  printf 'from-A-%s' "${i}" >"${work}/round.want"
  { FAKE_OUT_FILE=${work}/round.want send | recv; } >"${work}/round.out" 2>"${work}/round.err"
  if ! cmp -s "${work}/round.out" "${work}/round.want" || [[ -s ${work}/round.err ]]; then
    pipe_ok=0
    pipe_detail="round ${i}: out=$(cat "${work}/round.out") err=$(cat "${work}/round.err")"
  fi
done
if ((pipe_ok)); then
  ok "op … | op … never deadlocks in four unordered rounds"
else
  ng "op … | op … never deadlocks in four unordered rounds" "${pipe_detail}"
fi
unset OPREASON_LOCK_WAIT

echo "== Closed stdin, stale leftovers"
reset
"${op}" --reason "閉じた stdin" item get a <&- >/dev/null 2>"${work}/closed.err"
detail="$(cat "${work}/closed.err")"
check "a closed stdin adds nothing to stderr" test "$(<"${work}/closed.err")" == fake-err
reset
live="20260101T000000-$$-1"
mkdir -p "${state}/run/old-call" "${state}/run/${live}"
touch -d '2 hours ago' "${state}/run/old-call" "${state}/run/${live}"
run_op stale --reason "片付け" item get a
if [[ ! -e ${state}/run/old-call && -d ${state}/run/${live} && $(ls -A "${state}/run") == "${live}" ]]; then
  ok "run/ entries older than an hour are removed (not one whose call still runs), and the call leaves none"
else
  ng "run/ entries older than an hour are removed (not one whose call still runs), and the call leaves none" "$(ls -lR "${state}" 2>&1)"
fi

echo "== The language chosen in the window"
copy=${state}/lang.json
reset
OPREASON_LANG='' LC_ALL=ja_JP.UTF-8 LANG=ja_JP.UTF-8 run_op localeja item get a
detail="rc=${rc} err=$(cat "${work}/localeja.err")"
check "LANG and LC_ALL alone no longer choose Japanese" grep -q 'There is no reason (--reason)' "${work}/localeja.err"
reset
FAKE_CHOSEN='"en"' run_op chosen1 --reason "窓で English を選んだ" item get a
first_lang=$(payload_field '.lang')
copied=$(cat "${copy}" 2>/dev/null)
run_op chosen2 item get a
run_op chosen3 --reason "次の呼び出し" item get a
next_lang=$(payload_field '.lang' 2)
"${work}/names/op-reason-log" >"${work}/chosenlog.out" 2>&1
if [[ ${first_lang} == ja && ${copied} == '{"v":1,"lang":"en"}' && $(<"${work}/chosen2.err") == *"There is no reason (--reason)"* &&
${next_lang} == en ]] && grep -q '^Time ' "${work}/chosenlog.out"; then
  ok "chosenLang en in the status → lang.json; the next call speaks English despite OPREASON_LANG=ja (refusal, payload, log viewer)"
else
  ng "chosenLang en in the status → lang.json; the next call speaks English despite OPREASON_LANG=ja (refusal, payload, log viewer)" \
    "first=${first_lang} copy=${copied} next=${next_lang} err=$(cat "${work}/chosen2.err") log=$(head -n 1 "${work}/chosenlog.out")"
fi
run_op keep1 --reason "chosenLang の無い status" item get a
keep1=$(cat "${copy}" 2>/dev/null)
FAKE_CHOSEN='"fr"' run_op keep2 --reason "知らない言語" item get a
keep2=$(cat "${copy}" 2>/dev/null)
FAKE_CHOSEN=3 run_op keep3 --reason "形の違う chosenLang" item get a
keep3=$(cat "${copy}" 2>/dev/null)
if [[ ${keep1} == '{"v":1,"lang":"en"}' && ${keep2} == "${keep1}" && ${keep3} == "${keep1}" && ${rc} == 0 &&
  $(last_field end '.result') == approved ]]; then
  ok "no chosenLang, an unknown language or a malformed value leave lang.json (and the result)"
else
  ng "no chosenLang, an unknown language or a malformed value leave lang.json (and the result)" \
    "copies=${keep1} | ${keep2} | ${keep3} rc=${rc} $(last end)"
fi
FAKE_CHOSEN=null run_op cleared --reason "選んだ言語を消した" item get a
run_op cleared2 item get a
if [[ ! -e ${copy} && $(<"${work}/cleared2.err") == *"理由 (--reason) がありません"* ]]; then
  ok "chosenLang null removes lang.json, and OPREASON_LANG counts again"
else
  ng "chosenLang null removes lang.json, and OPREASON_LANG counts again" "copy=$(cat "${copy}" 2>&1) err=$(cat "${work}/cleared2.err")"
fi
reset
OPREASON_RUNNER=launcher FAKE_CHOSEN='"en"' run_op chosenps --reason "launcher の status" item get a
detail="runner=$(runner_of) copy=$(cat "${copy}" 2>&1)"
check "the launcher's chosenLang counts the same" test "$(runner_of)" == powershell.exe -a "$(cat "${copy}" 2>/dev/null)" == '{"v":1,"lang":"en"}'

echo "== Unusual inputs"
reset
run_op nonutf8 --reason $'読む\xff' item get $'\xffraw\xfe'
detail="rc=${rc} args=$(payload_args | od -An -tx1 | tr -s ' ')"
if [[ ${rc} == 0 ]] && cmp -s <(payload_args) <(printf 'item\0get\0\377raw\376\0') \
  && cmp -s <(tr '\0' '\n' <"${FAKE_DIR}/op.args") <(printf 'item\nget\n\377raw\376\n'); then
  ok "non-UTF-8 argument bytes reach op unchanged"
else
  ng "non-UTF-8 argument bytes reach op unchanged" "${detail}"
fi
reset
mkdir -p "${work}/gone"
(cd "${work}/gone" && rmdir "${work}/gone" && "${op}" --reason "消えた dir" item get a </dev/null >/dev/null 2>"${work}/gone.err")
gone_rc=$?
detail="rc=${gone_rc} err=$(cat "${work}/gone.err") $(last start)"
check "a removed current directory still runs" test "${gone_rc}" == 0 -a "$(last_field end '.result')" == approved
reset
head -c 300000 /dev/zero | tr '\0' y >"${work}/many.out"
FAKE_OUT_FILE=${work}/many.out "${op}" --reason "閉じた stdout" item list </dev/null 2>"${work}/epipe.err" | head -c 1 >/dev/null
epipe_rc=${PIPESTATUS[0]}
"${work}/names/op-reason" --skill 2>"${work}/skillpipe.err" | head -n 1 >/dev/null
skill_rc=${PIPESTATUS[0]}
if ! grep -qi panick "${work}/epipe.err" "${work}/skillpipe.err" && [[ ${skill_rc} == 0 &&
  ${epipe_rc} == "$(last_field end '.exit')" ]]; then
  ok "a closed stdout does not panic; the exit code is op's"
else
  ng "a closed stdout does not panic; the exit code is op's" "rc=${epipe_rc} skill=${skill_rc} err=$(cat "${work}/epipe.err" "${work}/skillpipe.err")"
fi

finish
