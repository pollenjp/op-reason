#!/usr/bin/env bash
# shellcheck shell=bash
#
# Runs the op-reason binary end to end with the real launcher (launcher/launcher.ps1, started with powershell.exe -File
# on PowerShell 7 for Linux, as a WSL call starts its fallback) and only a fake op. When the contract between the
# binary and the launcher (payload, status, environment, the -File command line) drifts, this test fails:
# wrapper.test.sh uses a fake runner and launcher.test.sh runs the launcher alone, so both could follow the same
# mistake. Ported from the dotfiles.
#
#   bash tests/e2e.test.sh [<op-reason binary>]
#
# Needs pwsh, jq, git and flock. The fake powershell.exe runs pwsh with the arguments it gets; the fake wslpath returns
# the path unchanged. launcher.ps1 is linked into the folder where OPREASON_EXE points (where the Windows zip puts it
# next to op-reason.exe), and OPREASON_RUNNER=launcher, except in the last check. No window or toast on Linux.
#
# ## What it checks
#
#   desktop
#     - tricky arguments (spaces, quotes, Japanese, empty) reach op unchanged, binary stdin comes out on stdout, stderr
#       has only op's, the exit code is op's
#     - after op the launcher runs whoami and, when signed in, signout --all; the log's end is approved with signout ok
#   denial: op's denial text → end denied, signout not-signed-in (the launcher's whoami fails)
#   service account: the token and OP_BIOMETRIC_UNLOCK_ENABLED=false reach op; the launcher runs neither whoami nor
#     signout (kind service-account, signout skipped)
#   pipe: in "op … | op …" with the receiver holding the lock, the sender's launcher does not sign out
#   auto: an op-reason.exe that writes no status → the real launcher runs the call once (end runner launcher, fallback
#     no-status, one stderr line)
#
# ## How
#
# The fake op.exe writes its arguments NUL-separated to FAKE_DIR/call.<n>, and who started it (the binary's watchdog
# when the parent's comm is op or op-reason, otherwise the launcher) and the token variables to FAKE_DIR/env.<n>. whoami
# exits with FAKE_WHOAMI_RC only when the launcher runs it (the watchdog's direct whoami always fails); signout exits 0;
# anything else copies stdin to stdout, writes FAKE_STDERR to stderr and exits with FAKE_RC.

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
work=$(mktemp -d "${TMPDIR:-/tmp}/op-reason-e2e-test.XXXXXX")
work=$(cd "${work}" && pwd -P)
trap 'rm -rf "${work}"' EXIT

export LC_ALL=C.UTF-8
export HOME=${work}/home XDG_STATE_HOME=${work}/state FAKE_DIR=${work}/fake
export POWERSHELL_TELEMETRY_OPTOUT=1 POWERSHELL_UPDATECHECK=Off DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1
unset WSLENV HERDR_ENV CLAUDE_CODE_SESSION_ID OP_SERVICE_ACCOUNT_TOKEN OP_CONNECT_HOST OP_CONNECT_TOKEN XDG_RUNTIME_DIR
mkdir -p "${HOME}" "${XDG_STATE_HOME}" "${FAKE_DIR}" "${work}/bin"
export PATH=${work}/bin:${PATH}
export OPREASON_OP_EXE=${work}/bin/op.exe
op=${work}/bin/op
ln -s "${bin}" "${op}"
# The Windows zip's folder: launcher.ps1 next to where op-reason.exe would be
mkdir -p "${work}/winapp"
ln -s "${here}/../launcher/launcher.ps1" "${work}/winapp/launcher.ps1"
export OPREASON_EXE=${work}/winapp/op-reason.exe OPREASON_RUNNER=launcher

fake "${work}/bin/powershell.exe" <<'EOF'
exec pwsh "$@"
EOF
fake "${work}/bin/wslpath" <<'EOF'
printf '%s\n' "$2"
EOF
fake "${work}/bin/op.exe" <<'EOF'
d=${FAKE_DIR}
n=$(($(cat "${d}/n" 2>/dev/null || echo 0) + 1))
echo "${n}" >"${d}/n"
printf '%s\0' "$@" >"${d}/call.${n}"
role=launcher
case $(cat "/proc/${PPID}/comm" 2>/dev/null) in
  op | op-reason) role=watch ;;
esac
{
  echo "role=${role}"
  echo "sa=${OP_SERVICE_ACCOUNT_TOKEN-<unset>}"
  echo "bio=${OP_BIOMETRIC_UNLOCK_ENABLED-<unset>}"
} >"${d}/env.${n}"
case ${1-} in
  whoami)
    [[ ${role} == launcher ]] && exit "${FAKE_WHOAMI_RC:-1}"
    exit 1
    ;;
  signout) exit 0 ;;
esac
cat
printf '%s\n' "${FAKE_STDERR:-FAKE-ERR}" >&2
exit "${FAKE_RC:-0}"
EOF

log=${XDG_STATE_HOME}/op-reason/log.jsonl

reset() {
  rm -rf "${FAKE_DIR:?}"/* "${XDG_STATE_HOME}/op-reason"
}

run_op() {
  local name=$1
  shift
  rc=0
  "${op}" "$@" <"${stdin_file:-/dev/null}" >"${work}/${name}.out" 2>"${work}/${name}.err" || rc=$?
}

# calls_from <launcher | watch>: the numbers of the fake op's calls from there
calls_from() {
  local f
  for f in "${FAKE_DIR}"/env.*; do
    [[ -e ${f} ]] || continue
    if grep -qx "role=$1" "${f}"; then
      echo "${f##*.}"
    fi
  done | sort -n
}
call_is() {
  local n=$1
  shift
  cmp -s "${FAKE_DIR}/call.${n}" <(printf '%s\0' "$@")
}
end_field() {
  jq -r "select(.event == \"end\") | $1" "${log}" 2>/dev/null | tail -n 1
}

stdin_file=${work}/in.bin
for i in $(seq 0 255); do
  printf -v oct '\\%03o' "${i}"
  printf '%b' "${oct}"
done >"${stdin_file}"
printf '\r\n\0end\n' >>"${stdin_file}"

echo "== desktop"
reset
export FAKE_WHOAMI_RC=0
args=(item get "a b" 'x"y' "日本語 の 値" "")
run_op desktop --reason "端から端" "${args[@]}"
mapfile -t from_pwsh < <(calls_from launcher)
if [[ ${rc} == 0 ]] && cmp -s "${stdin_file}" "${work}/desktop.out" && [[ $(<"${work}/desktop.err") == FAKE-ERR ]] \
  && ((${#from_pwsh[@]} == 3)) && call_is "${from_pwsh[0]}" "${args[@]}"; then
  ok "arguments, binary stdin, stderr and the exit code go through end to end"
else
  ng "arguments, binary stdin, stderr and the exit code go through end to end" \
    "rc=${rc} err=$(cat "${work}/desktop.err") calls=${from_pwsh[*]}"
fi
if ((${#from_pwsh[@]} == 3)) && call_is "${from_pwsh[1]}" whoami && call_is "${from_pwsh[2]}" signout --all \
  && [[ $(end_field .result) == approved && $(end_field .signout) == ok && $(end_field .window) == skipped ]]; then
  ok "the launcher runs whoami and then signout --all; the log says approved, signout ok"
else
  ng "the launcher runs whoami and then signout --all; the log says approved, signout ok" \
    "calls=${from_pwsh[*]} $(cat "${log}" 2>&1)"
fi
stdin_file=""

echo "== Denial"
reset
export FAKE_WHOAMI_RC=1 FAKE_RC=1 FAKE_STDERR="[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again"
run_op denied --reason "拒否を見る" item get a
unset FAKE_RC FAKE_STDERR
if [[ ${rc} == 1 && $(end_field .result) == denied && $(end_field .signout) == not-signed-in ]]; then
  ok "the denial text: log denied, signout not-signed-in"
else
  ng "the denial text: log denied, signout not-signed-in" "rc=${rc} $(cat "${log}" 2>&1)"
fi

echo "== service account"
reset
export OP_SERVICE_ACCOUNT_TOKEN=tok-e2e FAKE_WHOAMI_RC=0
run_op service --reason "SA で読む" vault list
unset OP_SERVICE_ACCOUNT_TOKEN
mapfile -t from_pwsh < <(calls_from launcher)
if [[ ${rc} == 0 ]] && ((${#from_pwsh[@]} == 1)) && call_is "${from_pwsh[0]}" vault list \
  && grep -qx 'sa=tok-e2e' "${FAKE_DIR}/env.${from_pwsh[0]}" && grep -qx 'bio=false' "${FAKE_DIR}/env.${from_pwsh[0]}" \
  && [[ $(end_field .signout) == skipped && $(jq -r 'select(.event == "start") | .kind' "${log}") == service-account ]]; then
  ok "the token and OP_BIOMETRIC_UNLOCK_ENABLED=false reach op; the launcher does not close sessions"
else
  ng "the token and OP_BIOMETRIC_UNLOCK_ENABLED=false reach op; the launcher does not close sessions" \
    "rc=${rc} calls=${from_pwsh[*]} $(cat "${FAKE_DIR}"/env.* 2>&1)"
fi

echo "== Pipe with the receiver holding the lock"
reset
export FAKE_WHOAMI_RC=0 OPREASON_LOCK_WAIT=20
{
  { wait_file "${FAKE_DIR}/call.1" && "${op}" --reason "送る" item list </dev/null 2>/dev/null; } \
    | "${op}" --reason "受ける" item get - 2>/dev/null
} >"${work}/pipe.out"
unset OPREASON_LOCK_WAIT
whoami_calls=0
signout_calls=0
for n in $(calls_from launcher); do
  call_is "${n}" whoami && whoami_calls=$((whoami_calls + 1))
  call_is "${n}" signout --all && signout_calls=$((signout_calls + 1))
done
send_end=$(jq -r 'select(.event == "start" and .reason == "送る") | .id' "${log}" | head -n 1)
send_signout=$(jq -r --arg id "${send_end}" 'select(.event == "end" and .id == $id) | .signout' "${log}")
if [[ ${whoami_calls} == 1 && ${signout_calls} == 1 && ${send_signout} == skipped &&
  $(jq -r 'select(.event == "start" and .reason == "送る") | .lock' "${log}") == skipped ]]; then
  ok "the sender that skipped the lock does not sign out; only the receiver does"
else
  ng "the sender that skipped the lock does not sign out; only the receiver does" \
    "whoami=${whoami_calls} signout=${signout_calls} send_signout=${send_signout} $(cat "${log}")"
fi
unset FAKE_WHOAMI_RC

echo "== auto: an op-reason.exe that writes no status"
reset
fake "${work}/winapp/op-reason.exe" <<'EOF'
echo "blocked (fake)" >&2
exit 1
EOF
export FAKE_WHOAMI_RC=0
stdin_file=${work}/in.bin
OPREASON_RUNNER="" run_op fallback --reason "回避経路で読む" item get x
stdin_file=""
unset FAKE_WHOAMI_RC
mapfile -t from_pwsh < <(calls_from launcher)
if [[ ${rc} == 0 ]] && cmp -s "${work}/in.bin" "${work}/fallback.out" && ((${#from_pwsh[@]} == 3)) \
  && call_is "${from_pwsh[0]}" item get x && [[ $(end_field .runner) == launcher && $(end_field .fallback.why) == no-status &&
  $(end_field .result) == approved && $(grep -c 'so it runs through launcher.ps1 instead' "${work}/fallback.err") == 1 ]]; then
  ok "the real launcher runs the call once after the exe wrote nothing (end runner launcher, fallback no-status)"
else
  ng "the real launcher runs the call once after the exe wrote nothing (end runner launcher, fallback no-status)" \
    "rc=${rc} calls=${from_pwsh[*]} err=$(cat "${work}/fallback.err") $(cat "${log}" 2>&1)"
fi
rm -f "${work}/winapp/op-reason.exe"

finish
