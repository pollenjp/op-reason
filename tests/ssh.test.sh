#!/usr/bin/env bash
# shellcheck shell=bash
#
# Black-box test of op-reason-ssh (the op-reason binary called by that name; design
# docs/superpowers/specs/2026-11-17-ssh-relay-design.md §7) with a fake ssh.exe, a fake herdr and a fake /proc.
#
#   bash tests/ssh.test.sh [<op-reason binary>]
#
# Needs bash, coreutils, jq and cmp. No real ssh.exe, no relay (the relay is tested on Windows by
# crates/op-reason-win/tests/relay.rs).
#
# ## What it checks
#
#   - ssh.exe gets the arguments unchanged (spaces, an empty one, --, -V, non-ASCII), binary stdin comes out whole on
#     stdout, stderr is ssh.exe's alone, and the exit code is ssh.exe's
#   - WSLENV keeps the caller's items and adds OPREASON_SSH_CALL
#   - OPREASON_SSH_CALL from a fake /proc (OPREASON_PROC_ROOT, OPREASON_PROC_START): the rule, the root and the shell
#     with their PIDs and start times, the caller (a person, or Claude Code with its PID and session), herdr's workspace
#     label and pane, the repository and branch, the distribution and the normalized reason; ASCII
#   - a /proc it cannot read: ssh.exe still runs, without OPREASON_SSH_CALL and with WSLENV as it was
#   - OPREASON_SSH_EXE chooses ssh.exe; no ssh.exe at all: exit 127 and one line on stderr

set -u -o pipefail

here=$(
  cd -- "$(dirname "$0")" &>/dev/null || exit
  pwd -P
)
# shellcheck source=tests/lib.sh
. "${here}/lib.sh"

bin=$(realpath "${1:-${OPREASON_BIN:-${here}/../target/debug/op-reason}}")
work=$(mktemp -d)
trap 'rm -rf "${work}"' EXIT

mkdir -p "${work}/bin" "${work}/rec" "${work}/other"
ln -s "${bin}" "${work}/bin/op-reason-ssh"

# The fake ssh.exe: records its arguments (NUL after each), WSLENV and OPREASON_SSH_CALL, copies stdin to stdout,
# writes one line on stderr and exits with FAKE_RC.
fake "${work}/bin/ssh.exe" <<'EOF'
dir=${FAKE_SSH_DIR:?}
printf '%s\0' "$@" >"${dir}/args"
printf '%s' "${WSLENV-<unset>}" >"${dir}/wslenv"
printf '%s' "${OPREASON_SSH_CALL-<unset>}" >"${dir}/call"
echo "${FAKE_SSH_NAME:-ssh.exe}" >"${dir}/which"
cat
echo "FAKE-SSH-ERR" >&2
exit "${FAKE_RC:-0}"
EOF
cp "${work}/bin/ssh.exe" "${work}/other/ssh-other.exe"

fake "${work}/bin/herdr" <<'EOF'
if [[ $1 == workspace && $2 == get && $3 == w1 ]]; then
  echo '{"result":{"workspace":{"label":"デプロイ"}}}'
fi
EOF

# A repository for the place: origin o/r, branch feat/x
repo="${work}/repo"
mkdir -p "${repo}/.git" "${repo}/sub"
echo 'ref: refs/heads/feat/x' >"${repo}/.git/HEAD"
printf '[remote "origin"]\n\turl = git@github.com:o/r.git\n' >"${repo}/.git/config"

# proc_entry <root> <pid> <comm> <ppid> <start> <argv>...: one process of a fake /proc (stat, comm, cmdline)
proc_entry() {
  local root=$1 pid=$2 comm=$3 ppid=$4 start=$5
  shift 5
  mkdir -p "${root}/${pid}"
  printf '%d (%s) S %d 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 %d 0 0\n' "${pid}" "${comm}" "${ppid}" "${start}" \
    >"${root}/${pid}/stat"
  printf '%s\n' "${comm}" >"${root}/${pid}/comm"
  printf '%s\0' "$@" >"${root}/${pid}/cmdline"
}

person="${work}/proc-person"
proc_entry "${person}" 1000 op-reason-ssh 999 300 op-reason-ssh host
proc_entry "${person}" 999 git 998 200 git push origin feat/x
proc_entry "${person}" 998 fish 1 100 fish

claude="${work}/proc-claude"
proc_entry "${claude}" 2000 op-reason-ssh 1999 400 op-reason-ssh host
proc_entry "${claude}" 1999 git 1998 300 git fetch
proc_entry "${claude}" 1998 bash 1997 200 /bin/bash -c -l "eval 'git fetch'"
proc_entry "${claude}" 1997 claude 1996 100 claude
proc_entry "${claude}" 1996 fish 1 50 fish

# run [env assignments...] -- <op-reason-ssh arguments...>: runs op-reason-ssh from the repository's subdirectory with
# the fake ssh.exe first in PATH; stdin from ${work}/in, stdout to ${work}/stdout, stderr to ${work}/stderr; sets rc.
run() {
  local -a envs=()
  while [[ $1 != -- ]]; do
    envs+=("$1")
    shift
  done
  shift
  rm -f "${work}/rec/"*
  (
    cd "${repo}/sub" || exit 99
    env -u OPREASON_SSH_CALL -u OPREASON_SSH_EXE -u CLAUDE_CODE_SESSION_ID -u HERDR_ENV \
      PATH="${work}/bin:${PATH}" FAKE_SSH_DIR="${work}/rec" HOME="${work}" WSL_DISTRO_NAME=Ubuntu-Test \
      "${envs[@]}" "${work}/bin/op-reason-ssh" "$@" <"${work}/in" >"${work}/stdout" 2>"${work}/stderr"
  )
  rc=$?
}

call_field() {
  jq -r "$1" "${work}/rec/call"
}

# is_json <file>: the file holds one JSON value
is_json() {
  jq -e . "$1" >/dev/null
}

# ascii_only <file>: no byte outside the printable ASCII range
ascii_only() {
  ! LC_ALL=C grep -q '[^ -~]' "$1"
}

echo "== arguments, streams and the exit code"
head -c 200000 /dev/urandom >"${work}/in"
args=(-o 'SendEnv=GIT_PROTOCOL' '' 'git@github.com' "git-upload-pack 'me/r.git'" -- 'a  b' '日本語' -V)
run OPREASON_PROC_ROOT="${person}" OPREASON_PROC_START=1000 FAKE_RC=7 -- "${args[@]}"
printf '%s\0' "${args[@]}" >"${work}/want-args"
check "the exit code is ssh.exe's (7)" test "${rc}" = 7
check "ssh.exe gets the arguments unchanged" cmp -s "${work}/want-args" "${work}/rec/args"
check "binary stdin comes out whole" cmp -s "${work}/in" "${work}/stdout"
detail=$(cat "${work}/stderr")
check "stderr is ssh.exe's alone" test "$(cat "${work}/stderr")" = FAKE-SSH-ERR
unset detail

echo "== OPREASON_SSH_CALL for a person's command"
: >"${work}/in"
run OPREASON_PROC_ROOT="${person}" OPREASON_PROC_START=1000 WSLENV='FOO/u:BAR:' \
  OPREASON_SSH_REASON=$'  push\tthe  fix\nto review  ' HERDR_ENV=1 HERDR_WORKSPACE_ID=w1 HERDR_PANE_ID=w1:p2 -- host
detail=$(cat "${work}/rec/call")
check "the call is JSON" is_json "${work}/rec/call"
check "it is ASCII" ascii_only "${work}/rec/call"
check "WSLENV keeps the caller's items and adds OPREASON_SSH_CALL" \
  test "$(cat "${work}/rec/wslenv")" = 'FOO/u:BAR:OPREASON_SSH_CALL'
check "version 1, the distribution" test "$(call_field '"\(.v) \(.distro)"')" = '1 Ubuntu-Test'
check "rule 2: the root is the shell's child (git, pid 999, start 200)" \
  test "$(call_field '"\(.rule) \(.root.pid) \(.root.start) \(.root.name)"')" = 'shell 999 200 git'
check "the root's command line" test "$(call_field .root.cmd)" = 'git push origin feat/x'
check "the shell (fish, pid 998, start 100)" test "$(call_field '"\(.shell.pid) \(.shell.start) \(.shell.name)"')" = '998 100 fish'
check "a person, no Claude Code" test "$(call_field '"\(.caller.type) \(.caller.claude_pid)"')" = 'human null'
check "herdr's label and pane" test "$(call_field '"\(.caller.herdr.label) \(.caller.herdr.pane)"')" = 'デプロイ w1:p2'
check "the repository and branch" test "$(call_field '"\(.caller.repo) \(.caller.branch)"')" = 'o/r feat/x'
# shellcheck disable=SC2088 # the literal ~ that op-reason-ssh writes for the home directory
check "the directory with ~" test "$(call_field .caller.cwd)" = '~/repo/sub'
check "the reason, normalized" test "$(call_field .reason)" = 'push the fix to review'
unset detail

echo "== OPREASON_SSH_CALL for Claude Code"
run OPREASON_PROC_ROOT="${claude}" OPREASON_PROC_START=2000 CLAUDE_CODE_SESSION_ID=sess-1 -- host
detail=$(cat "${work}/rec/call")
check "rule 1: the root is Claude Code's child (bash, pid 1998)" \
  test "$(call_field '"\(.rule) \(.root.pid) \(.root.name)"')" = 'claude 1998 bash'
check "no shell for rule 1" test "$(call_field .shell)" = null
check "Claude Code with its PID and session" \
  test "$(call_field '"\(.caller.type) \(.caller.claude_pid) \(.session)"')" = 'claude 1997 sess-1'
check "no reason given: empty" test "$(call_field .reason)" = ''
check "WSLENV of a caller without one" test "$(cat "${work}/rec/wslenv")" = 'OPREASON_SSH_CALL'
unset detail

echo "== a /proc it cannot read"
run OPREASON_PROC_ROOT="${work}/no-such-proc" WSLENV='FOO/u' FAKE_RC=3 -- host
check "ssh.exe still runs (its exit code)" test "${rc}" = 3
check "without OPREASON_SSH_CALL" test "$(cat "${work}/rec/call")" = '<unset>'
check "WSLENV as it was" test "$(cat "${work}/rec/wslenv")" = 'FOO/u'
check "nothing but ssh.exe's line on stderr" test "$(cat "${work}/stderr")" = FAKE-SSH-ERR

echo "== which ssh.exe"
run OPREASON_SSH_EXE="${work}/other/ssh-other.exe" FAKE_SSH_NAME=other -- host
check "OPREASON_SSH_EXE chooses it" test "$(cat "${work}/rec/which")" = other
mv "${work}/bin/ssh.exe" "${work}/ssh.exe.away"
run OPREASON_PROC_ROOT="${person}" OPREASON_PROC_START=1000 PATH="${work}/bin:/nonexistent" -- host
mv "${work}/ssh.exe.away" "${work}/bin/ssh.exe"
detail=$(cat "${work}/stderr")
check "no ssh.exe: exit 127" test "${rc}" = 127
check "one line that names ssh.exe" test "$(wc -l <"${work}/stderr")" = 1
check "it names ssh.exe" grep -q 'ssh.exe' "${work}/stderr"
unset detail

finish
