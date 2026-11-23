#!/usr/bin/env bash
# shellcheck shell=bash
#
# Black-box test of op-reason --settings on WSL (design docs/superpowers/specs/2026-11-23-settings-design.md §7.5) with
# a fake op-reason.exe and a fake wslpath.
#
#   bash tests/settings.test.sh [<op-reason binary>]
#
# Needs bash and coreutils. The settings themselves are tested by the core's tests and, on Windows, by
# crates/op-reason-win/tests/settings.rs.
#
# ## What it checks
#
#   - op-reason --settings <args> runs op-reason.exe --settings <args> (OPREASON_EXE) with the arguments unchanged,
#     stdout and the exit code passed through
#   - the caller's kind goes in OPREASON_SETTINGS_CALLER, added to WSLENV (claude under Claude Code, human otherwise)
#   - a chosen folder given as a WSL path is passed as the Windows path that wslpath -w gives
#   - set lang that succeeded updates the WSL copy of the language (lang.json); default removes it; a failed one does
#     not
#   - op as the name works too; without op-reason.exe: exit 127 and one line on stderr

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

mkdir -p "${work}/bin" "${work}/rec" "${work}/state"
ln -s "${bin}" "${work}/bin/op-reason"
ln -s "${bin}" "${work}/bin/op"

# The fake op-reason.exe: records its arguments (NUL after each), WSLENV and OPREASON_SETTINGS_CALLER, prints a line and
# exits with FAKE_RC.
fake "${work}/bin/op-reason.exe" <<'EOF2'
dir=${FAKE_DIR:?}
printf '%s\0' "$@" >"${dir}/args"
printf '%s' "${WSLENV-<unset>}" >"${dir}/wslenv"
printf '%s' "${OPREASON_SETTINGS_CALLER-<unset>}" >"${dir}/caller"
echo "from op-reason.exe"
exit "${FAKE_RC:-0}"
EOF2

# The fake wslpath: /mnt/<d>/<rest> becomes <D>:\<rest>; anything else fails
fake "${work}/bin/wslpath" <<'EOF2'
[[ $1 == -w ]] || exit 1
p=$2
[[ ${p} =~ ^/mnt/([a-z])/(.*)$ ]] || exit 1
d=${BASH_REMATCH[1]^^}
printf '%s:\\%s\n' "${d}" "${BASH_REMATCH[2]//\//\\}"
EOF2

run() {
  env -u CLAUDECODE -u CLAUDE_CODE_SESSION_ID PATH="${work}/bin:${PATH}" FAKE_DIR="${work}/rec" \
    OPREASON_EXE="${work}/bin/op-reason.exe" XDG_STATE_HOME="${work}/state" WSLENV="KEEP/u" "$@"
}

args() {
  tr '\0' '|' <"${work}/rec/args"
}

echo "== passing --settings on"
out=$(run "${work}/bin/op-reason" --settings get relay.mode --json)
rc=$?
detail="rc=${rc} out=${out} args=$(args)"
check "the arguments reach op-reason.exe after --settings" test "$(args)" == "--settings|get|relay.mode|--json|"
check "its stdout and exit code come back" test "${rc}:${out}" == "0:from op-reason.exe"
detail="wslenv=$(cat "${work}/rec/wslenv") caller=$(cat "${work}/rec/caller")"
check "WSLENV keeps the caller's items and adds OPREASON_SETTINGS_CALLER" \
  test "$(cat "${work}/rec/wslenv")" == "KEEP/u:OPREASON_SETTINGS_CALLER"
check "a person is the caller outside Claude Code" test "$(cat "${work}/rec/caller")" == "human"
run env CLAUDECODE=1 "${work}/bin/op" --settings list >/dev/null
check "Claude Code is the caller under Claude Code (and op works as the name)" \
  test "$(cat "${work}/rec/caller")" == "claude"
FAKE_RC=1 run "${work}/bin/op-reason" --settings check relay.running=on >/dev/null
rc=$?
check "op-reason.exe's exit code comes back" test "${rc}" == 1

echo "== a chosen folder as a WSL path"
run "${work}/bin/op-reason" --settings set "skill.folder./mnt/d/agents/skills" on >/dev/null
detail="args=$(args)"
check "the WSL path becomes the Windows path" test "$(args)" == "--settings|set|skill.folder.D:\\agents\\skills|on|"
run "${work}/bin/op-reason" --settings set "skill.folder./mnt/d/a=on" >/dev/null
detail="args=$(args)"
check "also in a name=value pair" test "$(args)" == "--settings|set|skill.folder.D:\\a=on|"

echo "== the WSL copy of the language"
copy="${work}/state/op-reason/lang.json"
run "${work}/bin/op-reason" --settings set lang ja >/dev/null
detail="copy=$(cat "${copy}" 2>&1)"
check "set lang ja writes the copy" grep -q '"lang":"ja"' "${copy}"
FAKE_RC=1 run "${work}/bin/op-reason" --settings set lang=en >/dev/null
check "a failed set leaves it" grep -q '"lang":"ja"' "${copy}"
run "${work}/bin/op-reason" --settings set relay.mode=show-only lang=default >/dev/null
check "lang=default removes it" test ! -e "${copy}"

echo "== without op-reason.exe"
err=$(env -u CLAUDECODE PATH="${work}/bin:${PATH}" OPREASON_EXE="${work}/nowhere/op-reason.exe" \
  XDG_STATE_HOME="${work}/state" "${work}/bin/op-reason" --settings list 2>&1 >/dev/null)
rc=$?
detail="rc=${rc} err=${err}"
check "exit 127 and one line on stderr" test "${rc}" == 127 -a "$(wc -l <<<"${err}")" == 1

finish
