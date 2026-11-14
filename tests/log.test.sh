#!/usr/bin/env bash
# shellcheck shell=bash
# shellcheck disable=SC2016 # $now and similar in single quotes are jq variables
#
# Tests the log viewer (op-reason-log) with made-up records. Ported from the dotfiles' test of the bash version.
#
#   bash tests/log.test.sh [<op-reason binary>]
#
# Needs bash, coreutils and jq.
#
# ## What it checks
#
#   Period
#     - by default only calls that started in the last hour (not 61 minutes ago)
#     - --since 2h shows 61 minutes ago; --all also reads the previous generation, log.1.jsonl
#   Rows
#     - start and end merge into one row with the result (許可・拒否・時間切れ・成功) and the seconds; a service account
#       call is "成功 (SA)"
#     - a start without an end is "実行中か中断"; refused rows are "断った (理由なし)" / "断った (始める前に止められた)";
#       shared-approval rows are "共有の承認を消した"; a desktop call whose window failed gets "(窓なし)"; a call that
#       skipped the lock gets "(lock を待たず)"
#     - the caller is Claude / 人 with the herdr label and pane; the reason stays Japanese; the command starts with op
#     - the time is HH:MM:SS today and MM/DD HH:MM:SS on other days; lines that are not JSON (also a broken last line)
#       are skipped
#   --json prints one JSON object per call (the start with its end)
#   No log, or nothing in the period: says so, exit 0. Unknown arguments, a bad --since: exit 2 with the usage. -h prints
#   the usage on stdout, exit 0
#   English (OPREASON_LANG=en): headers, results and refusals in English
#   Columns are aligned by display width (Japanese characters count 2)
#
# ## How
#
# The clock is OPREASON_LOG_NOW (1800000000 = 2027-01-15 08:00:00 UTC) and the time zone is UTC.

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
work=$(mktemp -d "${TMPDIR:-/tmp}/op-reason-log-test.XXXXXX")
trap 'rm -rf "${work}"' EXIT
viewer=${work}/op-reason-log
ln -s "${bin}" "${viewer}"

export LC_ALL=C.UTF-8 TZ=UTC0 OPREASON_LANG=ja
export XDG_STATE_HOME=${work}/state
export OPREASON_LOG_NOW=1800000000
now=${OPREASON_LOG_NOW}
dir=${XDG_STATE_HOME}/op-reason
mkdir -p "${dir}"

# rec <jq expression> [--arg …]…: one record (now is passed)
rec() {
  local expr=$1
  shift
  jq -cn --argjson now "${now}" "$@" "${expr}"
}

claude='{type: "claude", claude_pid: 9, herdr: {workspace: "wX", label: "作業 1 テスト", pane: "wX:p1"}, cwd: "/w", repo: "o/r", branch: "b"}'
human='{type: "human", claude_pid: null, herdr: null, cwd: "/w", repo: null, branch: null}'
{
  rec "{v: 1, id: \"a\", event: \"start\", epoch: (\$now - 600), kind: \"desktop\", caller: ${claude}, reason: \"設定を読む\", args: [\"item\", \"get\", \"x\"]}"
  rec '{v: 1, id: "a", event: "end", epoch: ($now - 590), result: "approved", exit: 0, seconds: 10}'
  rec "{v: 1, id: \"b\", event: \"start\", epoch: (\$now - 3660), kind: \"desktop\", caller: ${human}, reason: \"古い理由\", args: [\"vault\", \"list\"]}"
  rec '{v: 1, id: "b", event: "end", epoch: ($now - 3655), result: "denied", exit: 1, seconds: 5}'
  rec "{v: 1, id: \"c\", event: \"start\", epoch: (\$now - 300), kind: \"desktop\", caller: ${human}, reason: \"止めた\", args: [\"item\", \"list\"]}"
  echo 'not json'
  rec "{v: 1, id: \"d\", event: \"refused\", epoch: (\$now - 200), why: \"no-reason\", caller: ${claude}, reason: \"\", args: [\"item\", \"get\", \"y\"]}"
  rec '{v: 1, id: "e", event: "shared-approval", epoch: ($now - 100), when: "end", cleared: true}'
  rec "{v: 1, id: \"h\", event: \"start\", epoch: (\$now - 60), kind: \"desktop\", caller: ${human}, reason: \"窓が出ない\", args: [\"whoami\"]}"
  rec '{v: 1, id: "h", event: "end", epoch: ($now - 55), result: "approved", exit: 0, seconds: 5, window: "failed: no WPF"}'
  rec "{v: 1, id: \"k\", event: \"refused\", epoch: (\$now - 50), why: \"interrupted\", caller: ${human}, reason: \"止めた2\", args: [\"read\", \"x\"]}"
  rec "{v: 1, id: \"g\", event: \"start\", epoch: (\$now - 120), kind: \"service-account\", caller: ${human}, reason: \"CI\", args: [\"read\", \"op://v/i/f\"]}"
  rec '{v: 1, id: "g", event: "end", epoch: ($now - 119), result: "ok", exit: 0, seconds: 1}'
  rec "{v: 1, id: \"m\", event: \"start\", epoch: (\$now - 40), kind: \"desktop\", lock: \"skipped\", caller: ${claude}, reason: \"パイプの前\", args: [\"item\", \"list\"]}"
  rec '{v: 1, id: "m", event: "end", epoch: ($now - 38), result: "approved", exit: 0, seconds: 2}'
  printf '{"v":1,"id":"z","event":"start","epoch":'
} >"${dir}/log.jsonl"
{
  rec "{v: 1, id: \"f\", event: \"start\", epoch: (\$now - 90000), kind: \"desktop\", caller: ${human}, reason: \"昨日の理由\", args: [\"whoami\"]}"
  rec '{v: 1, id: "f", event: "end", epoch: ($now - 89900), result: "timeout", exit: 1, seconds: 100}'
} >"${dir}/log.1.jsonl"

# run <name> <arg>...: out, err, rc
run() {
  local name=$1
  shift
  rc=0
  "${viewer}" "$@" >"${work}/${name}.out" 2>"${work}/${name}.err" || rc=$?
}

# row <name> <text>: the output lines that contain the text
row() {
  grep -F -- "$2" "${work}/$1.out"
}

echo "== Period"
run default
out=$(<"${work}/default.out")
if [[ ${rc} == 0 && ${out} == *設定を読む* && ${out} == *止めた* && ${out} == *CI* && ${out} != *古い理由* && ${out} != *昨日の理由* ]]; then
  ok "by default only the last hour (not 61 minutes ago)"
else
  ng "by default only the last hour (not 61 minutes ago)" "rc=${rc} ${out}"
fi
run since --since 2h
out=$(<"${work}/since.out")
if [[ ${out} == *古い理由* && ${out} != *昨日の理由* ]]; then
  ok "--since 2h shows 61 minutes ago"
else
  ng "--since 2h shows 61 minutes ago" "${out}"
fi
run all --all
out=$(<"${work}/all.out")
if [[ ${out} == *昨日の理由* && $(row all 昨日の理由) == *時間切れ* ]]; then
  ok "--all also reads log.1.jsonl"
else
  ng "--all also reads log.1.jsonl" "${out}"
fi

echo "== Rows"
a=$(row default 設定を読む)
if [[ ${a} == *許可* && ${a} == *" 10 "* && ${a} == *"Claude 作業 1 テスト wX:p1"* && ${a} == *"op item get x"* &&
  ${a} == "07:50:00 "* ]]; then
  ok "start and end in one row: result, seconds, caller, reason, command, today's time (HH:MM:SS)"
else
  ng "start and end in one row: result, seconds, caller, reason, command, today's time (HH:MM:SS)" "${a}"
fi
if [[ $(row since 古い理由) == *拒否* && $(row default CI) == *"成功 (SA)"* ]]; then
  ok "denied, and a service account call's 成功 (SA)"
else
  ng "denied, and a service account call's 成功 (SA)" "$(row since 古い理由) / $(row default CI)"
fi
if [[ $(row default 止めた) == *実行中か中断* && $(row default "op item get y") == *"断った (理由なし)"* &&
-n $(row default 共有の承認を消した) ]]; then
  ok "a start without an end is 実行中か中断; refused and shared-approval rows too"
else
  ng "a start without an end is 実行中か中断; refused and shared-approval rows too" "$(cat "${work}/default.out")"
fi
if [[ $(row default 窓が出ない) == *"許可 (窓なし)"* && $(row default 止めた2) == *"断った (始める前に止められた)"* ]]; then
  ok "(窓なし) for a failed window; a name for refused-before-start"
else
  ng "(窓なし) for a failed window; a name for refused-before-start" "$(cat "${work}/default.out")"
fi
if [[ $(row default パイプの前) == *"許可 (lock を待たず)"* && $(row default 設定を読む) != *"lock を待たず"* ]]; then
  ok "(lock を待たず) for a call that skipped the lock"
else
  ng "(lock を待たず) for a call that skipped the lock" "$(cat "${work}/default.out")"
fi
if [[ $(row all 昨日の理由) == "01/14 "* ]]; then
  ok "other days show MM/DD HH:MM:SS"
else
  ng "other days show MM/DD HH:MM:SS" "$(row all 昨日の理由)"
fi
header=$(head -n 1 "${work}/default.out")
first=$(sed -n 2p "${work}/default.out")
# The reason column starts at the same display column in every row: after the header's "理由" and the row's reason
col_of() {
  local line=$1 word=$2 prefix
  prefix=${line%%"${word}"*}
  # display width of the prefix: 2 for each non-ASCII character used here (Japanese), 1 otherwise
  local ascii=${prefix//[^[:ascii:]]/}
  echo $((${#ascii} + 2 * (${#prefix} - ${#ascii})))
}
if [[ $(col_of "${header}" 呼び手) == $(col_of "${first}" Claude) ]]; then
  ok "columns are aligned by display width"
else
  ng "columns are aligned by display width" "${header}"$'\n'"${first}"
fi

echo "== --json"
run json --json
if [[ ${rc} == 0 && $(jq -s 'length' "${work}/json.out" 2>/dev/null) == 8 &&
$(jq -r 'select(.id == "a") | .end.result' "${work}/json.out" 2>/dev/null) == approved ]]; then
  ok "--json: one line per call (the start with its end)"
else
  ng "--json: one line per call (the start with its end)" "rc=${rc} $(cat "${work}/json.out")"
fi

echo "== Nothing to show, arguments"
OPREASON_LOG_NOW=$((now + 864000)) run empty
if [[ ${rc} == 0 && $(<"${work}/empty.out") == *記録はありません* ]]; then
  ok "nothing in the period: says so, exit 0"
else
  ng "nothing in the period: says so, exit 0" "rc=${rc} $(cat "${work}/empty.out")"
fi
XDG_STATE_HOME=${work}/none run nolog
if [[ ${rc} == 0 && $(<"${work}/nolog.out") == *記録がありません* ]]; then
  ok "no log: says so, exit 0"
else
  ng "no log: says so, exit 0" "rc=${rc} $(cat "${work}/nolog.out")"
fi
run bad --no-such
rc_bad=${rc}
run badsince --since 3x
if [[ ${rc_bad} == 2 && ${rc} == 2 && $(<"${work}/badsince.err") == *使い方* ]]; then
  ok "an unknown argument or a bad --since: exit 2 with the usage"
else
  ng "an unknown argument or a bad --since: exit 2 with the usage" "bad=${rc_bad} since=${rc} $(cat "${work}/badsince.err")"
fi
run help -h
if [[ ${rc} == 0 && $(<"${work}/help.out") == *使い方* ]]; then
  ok "-h prints the usage on stdout, exit 0"
else
  ng "-h prints the usage on stdout, exit 0" "rc=${rc}"
fi

echo "== Control characters and unusual --since"
esc=$(printf '\033')
# The log ends with a broken line on purpose (above): start a new line first
printf '\n' >>"${dir}/log.jsonl"
rec "{v: 1, id: \"x\", event: \"start\", epoch: (\$now - 30), kind: \"desktop\", caller: ${human}, reason: \"制御\", args: [\"item\", \"get\", \"a\u001b[2Jb\tc\"]}" >>"${dir}/log.jsonl"
run ctrl
if [[ $(row ctrl 制御) == *'a\x1b[2Jb\x09c'* ]] && ! grep -q "${esc}" "${work}/ctrl.out"; then
  ok "control characters in a logged value are shown as escapes, never sent to the terminal"
else
  ng "control characters in a logged value are shown as escapes, never sent to the terminal" "$(od -c "${work}/ctrl.out" | head)"
fi
run since-mb --since=5分
detail="rc=${rc} err=$(cat "${work}/since-mb.err")"
check "--since with a Japanese unit is a usage error (exit 2), not a crash" test "${rc}" == 2

echo "== English"
OPREASON_LANG=en run en
en_out=$(<"${work}/en.out")
if [[ $(head -n 1 "${work}/en.out") == Time* && $(row en 設定を読む) == *approved* &&
$(row en "op item get y") == *"refused (no reason)"* && $(row en 止めた) == *"running or interrupted"* &&
$(row en CI) == *"ok (SA)"* && $(row en 窓が出ない) == *"approved (no window)"* &&
$(row en "Claude 作業 1") == *Claude* ]]; then
  ok "English headers, results and refusals"
else
  ng "English headers, results and refusals" "${en_out}"
fi

finish
