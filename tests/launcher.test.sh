#!/usr/bin/env bash
# shellcheck shell=bash
#
# Runs launcher/launcher.ps1 (the fallback runner, started in a fresh powershell.exe) on PowerShell 7 for
# Linux with a fake op. Ported from the dotfiles' launcher test.
#
#   bash tests/launcher.test.sh [<launcher.ps1>]
#
# Needs pwsh and jq. No real Windows PowerShell, op.exe, window or toast: on Linux the launcher shows neither and
# writes "skipped". The launcher started by the binary (powershell.exe -File, the fallback of a WSL call) is covered by
# tests/e2e.test.sh.
#
# ## What it checks
#
#   What op receives (desktop mode)
#     - arguments with empty strings, spaces, quotes, a trailing \, \", & | < > ^ % !, Japanese, leading and trailing
#       spaces, newline, tab, only ", only \ and an emoji arrive one by one, in order (the launcher builds the Windows
#       command line, .NET splits it back with the same rules)
#     - binary stdin (0-255, \r\n, \0) comes out on stdout unchanged (no PowerShell pipeline)
#     - stderr has only what op wrote (no CLIXML, no progress records)
#     - the exit code is op's
#     - OPREASON_PAYLOAD is not inherited by op
#   Closing the sessions
#     - desktop runs whoami --format json after op and signout --all only when whoami succeeds (status signout ok /
#       not-signed-in)
#     - whoami and signout write nothing to the launcher's stdout or stderr
#     - help closes the same way; service runs neither (status signout skipped)
#     - a call with lock "skipped" (the sender of a pipe) runs neither (status signout skipped)
#   service passes OP_SERVICE_ACCOUNT_TOKEN to op as it is
#   On Linux no window and no toast (status window and toast are skipped); status started is true, exit is op's
#   op missing → 127, stderr is one line starting with "op-reason: launcher:", status started false
#   notify exits 0 and does not start op
#   Texts built from the facts (status.ui)
#     - Japanese: labels 時刻・呼び手・(herdr)・場所・理由・コマンド・注意, caller "Claude Code (pid 999)" / "人 (端末)",
#       place "o/r (feat/x) · ~/w", footer with この窓が出ていない, toast of 3 texts (理由: … cut at 100 characters)
#     - English: labels Time, Caller, Place, Reason, Command, caller "Human (terminal)", footer "… without this window"
#     - notify: toast title and body for cleared and not cleared, before and after the call
#   The language chosen in op-reason.exe's window (settings.json in the launcher's state directory: on Linux,
#     ~/.local/share/op-reason): it beats the payload's lang in the window and toast texts (desktop and notify) and is
#     reported as status.chosenLang; no file or a broken file → the payload's language and chosenLang null; the launcher
#     never writes the file
#   The references a call reads (design 2026-11-17-refs-in-window-design.md): status.ui.refs (head, items with linked,
#     more, hint) in Japanese and English; the hint and no links without account.json; whoami's JSON keeps only the
#     account ID and host in account.json; then ID references are links, also with --account naming the account by its
#     host's first label, not with another account; the shared link table (tests/cases/refs.json); no URL in the status;
#     no block without references
#   stderr scan: op's stderr reaches the launcher's stderr; a denial or a timeout from op's [ERROR] lines is reported in
#     status.stderrMatch (a child's "Connection timed out" is not); stdin and stdout stay binary-safe
#   The named mutex Local\op-reason (.NET supports named mutexes on Linux too): a call takes it (status lock held) and
#     removes its holder.json; while another process holds it, a call waits lockWait seconds, says so once on stderr and
#     exits 75 (status lock timeout, error lock-timeout, the holder from holder.json); a lock-skipped call runs anyway
#
# ## How
#
# The fake op (bash) counts its calls, writes its arguments NUL-separated to FAKE_DIR/call.<n> and whether
# OPREASON_PAYLOAD was inherited and OP_SERVICE_ACCOUNT_TOKEN to FAKE_DIR/env.<n>. whoami prints FAKE_WHOAMI_OUT and
# exits with FAKE_WHOAMI_RC (default 1); signout writes SIGNOUT-OUT / SIGNOUT-ERR and exits 0; anything else copies stdin to stdout,
# writes "FAKE-ERR 日本語" to stderr and exits with FAKE_RC (default 0).

# No -e: a failed check must not stop the checks after it.
set -u -o pipefail

here=$(
  cd -- "$(dirname "$0")" &>/dev/null || exit
  pwd -P
)
# shellcheck source=tests/lib.sh
. "${here}/lib.sh"
launcher=${1:-${here}/../launcher/launcher.ps1}
work=$(mktemp -d "${TMPDIR:-/tmp}/op-reason-launcher-test.XXXXXX")
work=$(cd "${work}" && pwd -P)
trap 'rm -rf "${work}"' EXIT

export HOME=${work}/home
mkdir -p "${HOME}"
export POWERSHELL_TELEMETRY_OPTOUT=1 POWERSHELL_UPDATECHECK=Off DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1
export LC_ALL=C.UTF-8

fake_dir=${work}/fake
mkdir -p "${fake_dir}"
export FAKE_DIR=${fake_dir}
fake_op=${fake_dir}/op
fake "${fake_op}" <<'EOF'
d=${FAKE_DIR}
n=$(($(cat "${d}/n" 2>/dev/null || echo 0) + 1))
echo "${n}" >"${d}/n"
printf '%s\0' "$@" >"${d}/call.${n}"
{
  if [[ -n ${OPREASON_PAYLOAD+x} ]]; then echo "payload=present"; else echo "payload=absent"; fi
  echo "sa=${OP_SERVICE_ACCOUNT_TOKEN-<unset>}"
} >"${d}/env.${n}"
case ${1-} in
  whoami)
    if [[ -n ${FAKE_WHOAMI_OUT:-} ]]; then printf '%s\n' "${FAKE_WHOAMI_OUT}"; fi
    exit "${FAKE_WHOAMI_RC:-1}"
    ;;
  signout)
    echo SIGNOUT-OUT
    echo SIGNOUT-ERR >&2
    exit 0
    ;;
esac
if [[ -n ${FAKE_SLEEP:-} ]]; then sleep "${FAKE_SLEEP}"; fi
cat
printf '%s\n' "${FAKE_ERR_TEXT:-FAKE-ERR 日本語}" >&2
exit "${FAKE_RC:-0}"
EOF

# payload <mode> <op> <arg>...: the payload JSON (args64: UTF-8 then base64 per argument). FACTS (JSON) and LANG_CODE
# change the facts and the language.
payload() {
  local mode=$1 op=$2 a
  shift 2
  local -a enc=()
  for a in "$@"; do
    enc+=("$(printf '%s' "${a}" | base64 -w0)")
  done
  jq -cn --arg mode "${mode}" --arg op "${op}" --arg status "${work}/status.json" --arg lang "${LANG_CODE:-ja}" \
    --argjson facts "${FACTS:-null}" --arg lock "${LOCK_MODE:-held}" --argjson wait "${LOCK_WAIT:-90}" \
    --arg stderr "${STDERR_MODE:-inherit}" \
    '{v: 1, mode: $mode, op: $op, status: $status, lang: $lang, lock: $lock, lockWait: $wait, stderr: $stderr,
      args64: $ARGS.positional} + (if $facts == null then {} else {facts: $facts} end)' --args -- "${enc[@]}"
}

# run <name> <payload>: runs the launcher with -File. stdin from stdin_file (default /dev/null); stdout and stderr to
# <name>.out and <name>.err; the exit code to rc
run() {
  local name=$1 json=$2
  rm -f "${fake_dir}"/n "${fake_dir}"/call.* "${fake_dir}"/env.* "${work}/status.json"
  rc=0
  OPREASON_PAYLOAD=${json} pwsh -NoProfile -NonInteractive -OutputFormat Text -File "${launcher}" \
    <"${stdin_file:-/dev/null}" >"${work}/${name}.out" 2>"${work}/${name}.err" || rc=$?
}

call_args() {
  cat "${fake_dir}/call.$1" 2>/dev/null
}

expect_args() {
  local n=$1
  shift
  cmp -s <(call_args "${n}") <(printf '%s\0' "$@")
}

calls() {
  cat "${fake_dir}/n" 2>/dev/null || echo 0
}

status_field() {
  jq -r "$1" "${work}/status.json" 2>/dev/null
}

# shellcheck disable=SC1003 # arguments that end with \ are what is checked
tricky=(
  "" "a b" 'x"y' 'tr\\' 'q\"z' '&|<>^%PATH%!' "日本語 の 引数" "  lead" "trail  " $'new\nline' $'tab\there' '"' '\' "😀"
)
in_bin=${work}/in.bin
for _ in $(seq 40); do
  for i in $(seq 0 255); do
    printf -v oct '\\%03o' "${i}"
    printf '%b' "${oct}"
  done
done >"${in_bin}"
printf '\r\n\0end\n' >>"${in_bin}"

claude_facts='{"time":"2026-11-15 12:34:56","caller":{"type":"claude","pid":999,"session":"s1","detail":null},
  "herdr":{"workspace":"wX","label":"作業 1 テスト","pane":"wX:p1"},
  "place":{"repo":"o/r","branch":"feat/x","cwd":"~/w"},"reason":"設定を読む","command":"op item get x",
  "notes":[{"kind":"shared-cleared"},{"kind":"lock-skipped","holderReason":"受ける"}]}'
human_facts='{"time":"2026-11-15 12:34:56","caller":{"type":"human","pid":null,"session":null,"detail":null},
  "herdr":null,"place":{"repo":null,"branch":null,"cwd":"~"},"reason":"read the settings","command":"op item get x",
  "notes":[]}'

echo "== What op receives (desktop)"
stdin_file=${in_bin}
export FAKE_RC=42
FACTS=${human_facts} run desktop "$(FACTS=${human_facts} payload desktop "${fake_op}" "${tricky[@]}")"
unset FAKE_RC
stdin_file=""
detail="$(call_args 1 | tr '\0' '|')"
check "tricky arguments arrive one by one, in order" expect_args 1 "${tricky[@]}"
detail="in=$(stat -c %s "${in_bin}") out=$(stat -c %s "${work}/desktop.out")"
check "binary stdin comes out on stdout unchanged" cmp -s "${in_bin}" "${work}/desktop.out"
detail="$(head -c 400 "${work}/desktop.err")"
check "stderr has only what op wrote" test "$(<"${work}/desktop.err")" == "FAKE-ERR 日本語"
detail="rc=${rc}"
check "the exit code is op's" test "${rc}" == 42
detail="$(cat "${fake_dir}/env.1" 2>&1)"
check "OPREASON_PAYLOAD is not inherited by op" grep -qx "payload=absent" "${fake_dir}/env.1"
detail="$(cat "${work}/status.json" 2>&1)"
if [[ $(status_field '.window') == skipped && $(status_field '.toast') == skipped &&
$(status_field '.started') == true && $(status_field '.exit') == 42 ]]; then
  ok "on Linux no window and no toast; status has started and op's exit code"
else
  ng "on Linux no window and no toast; status has started and op's exit code" "${detail}"
fi

echo "== Closing the sessions"
export FAKE_WHOAMI_RC=0
run closes "$(payload desktop "${fake_op}" item get x)"
if [[ $(calls) == 3 ]] && expect_args 1 item get x && expect_args 2 whoami --format json && expect_args 3 signout --all \
  && [[ $(status_field '.signout') == ok ]]; then
  ok "desktop runs whoami after op and signout --all when signed in"
else
  ng "desktop runs whoami after op and signout --all when signed in" "calls=$(calls) status=$(cat "${work}/status.json" 2>&1)"
fi
if ! grep -q SIGNOUT "${work}/closes.out" "${work}/closes.err"; then
  ok "whoami and signout write nothing to the launcher's stdout or stderr"
else
  ng "whoami and signout write nothing to the launcher's stdout or stderr" \
    "out=$(cat "${work}/closes.out") err=$(cat "${work}/closes.err")"
fi
export FAKE_WHOAMI_RC=1
run notsigned "$(payload desktop "${fake_op}" item get x)"
if [[ $(calls) == 2 ]] && expect_args 2 whoami --format json && [[ $(status_field '.signout') == not-signed-in ]]; then
  ok "no signout without a session (status not-signed-in)"
else
  ng "no signout without a session (status not-signed-in)" "calls=$(calls) status=$(cat "${work}/status.json" 2>&1)"
fi
export FAKE_WHOAMI_RC=0
run help "$(payload help "${fake_op}" --version)"
if [[ $(calls) == 3 ]] && expect_args 1 --version && expect_args 2 whoami --format json && expect_args 3 signout --all \
  && [[ $(status_field '.window') == skipped && $(status_field '.toast') == skipped && $(status_field '.ui') == null ]]; then
  ok "help closes the sessions the same way, without window, toast or texts"
else
  ng "help closes the sessions the same way, without window, toast or texts" "calls=$(calls) status=$(cat "${work}/status.json" 2>&1)"
fi
LOCK_MODE=skipped run skipped "$(LOCK_MODE=skipped payload desktop "${fake_op}" item list)"
if [[ $(calls) == 1 ]] && expect_args 1 item list && [[ $(status_field '.signout') == skipped ]]; then
  ok "a call with lock skipped does not close the sessions (the lock holder's signout --all does)"
else
  ng "a call with lock skipped does not close the sessions (the lock holder's signout --all does)" "calls=$(calls) status=$(cat "${work}/status.json" 2>&1)"
fi
export OP_SERVICE_ACCOUNT_TOKEN=tok-test
run service "$(payload service "${fake_op}" vault list)"
unset OP_SERVICE_ACCOUNT_TOKEN
if [[ $(calls) == 1 ]] && expect_args 1 vault list && [[ $(status_field '.signout') == skipped ]]; then
  ok "service runs neither whoami nor signout (status signout skipped)"
else
  ng "service runs neither whoami nor signout (status signout skipped)" "calls=$(calls) status=$(cat "${work}/status.json" 2>&1)"
fi
detail="$(cat "${fake_dir}/env.1" 2>&1)"
check "service passes OP_SERVICE_ACCOUNT_TOKEN to op as it is" grep -qx "sa=tok-test" "${fake_dir}/env.1"
unset FAKE_WHOAMI_RC

echo "== Cannot start op, notify"
run missing "$(payload desktop "${work}/no-such-op" item get x)"
err=$(<"${work}/missing.err")
if [[ ${rc} == 127 && ${err} == "op-reason: launcher:"* && ${err} != *$'\n'* && $(status_field '.started') == false &&
$(calls) == 0 ]]; then
  ok "op missing → 127, one 'op-reason: launcher:' line on stderr, started false"
else
  ng "op missing → 127, one 'op-reason: launcher:' line on stderr, started false" "rc=${rc} err=${err} status=$(cat "${work}/status.json" 2>&1)"
fi
notify_payload() {
  jq -cn --arg status "${work}/status.json" --arg lang "$1" --arg when "$2" --argjson cleared "$3" \
    '{v: 1, mode: "notify", status: $status, lang: $lang, lock: "skipped", lockWait: 0, stderr: "inherit",
      notify: {kind: "shared-approval", when: $when, cleared: $cleared, time: "12:00:00"}}'
}
run notify "$(notify_payload ja start true)"
if [[ ${rc} == 0 && $(calls) == 0 && ! -s ${work}/notify.err ]]; then
  ok "notify exits 0 and does not start op"
else
  ng "notify exits 0 and does not start op" "rc=${rc} calls=$(calls) err=$(cat "${work}/notify.err")"
fi

echo "== Texts built from the facts"
FACTS=${claude_facts} run ja "$(FACTS=${claude_facts} payload desktop "${fake_op}" item get x)"
labels=$(status_field '[.ui.lines[][0]] | join(",")')
detail="$(status_field '.ui')"
check "Japanese labels" test "${labels}" == "時刻,呼び手,herdr,場所,理由,コマンド,注意,注意"
line_of() {
  status_field ".ui.lines[] | select(.[0] == \"$1\") | .[1]" | head -n 1
}
if [[ $(line_of 呼び手) == "Claude Code (pid 999)" && $(line_of herdr) == "workspace「作業 1 テスト」 · pane wX:p1" &&
$(line_of 場所) == "o/r (feat/x) · ~/w" && $(line_of 理由) == 設定を読む && $(line_of コマンド) == "op item get x" &&
$(status_field '.ui.footer') == *この窓が出ていない* && $(status_field '.ui.toast | length') == 3 &&
$(status_field '.ui.toast[2]') == "Claude Code · 作業 1 テスト · op item get x" &&
$(status_field '[.ui.lines[] | select(.[0] == "注意") | .[1]] | last') == *「受ける」* ]]; then
  ok "Japanese caller, herdr, place, reason, command, footer, toast and notes"
else
  ng "Japanese caller, herdr, place, reason, command, footer, toast and notes" "${detail}"
fi
long=$(printf 'あ%.0s' $(seq 150))
long_facts=$(jq -c --arg r "${long}" '.reason = $r' <<<"${human_facts}")
LANG_CODE=en FACTS=${long_facts} run en "$(LANG_CODE=en FACTS=${long_facts} payload desktop "${fake_op}" item get x)"
labels=$(status_field '[.ui.lines[][0]] | join(",")')
toast1=$(status_field '.ui.toast[1]')
if [[ ${labels} == "Time,Caller,Place,Reason,Command" && $(status_field '.ui.lines[1][1]') == "Human (terminal)" &&
$(status_field '.ui.footer') == *"without this window"* && ${toast1} == "Reason: "* && ${toast1} == *… &&
${#toast1} == $((8 + 100 + 1)) && $(status_field '.ui.toast[2]') == "Human · op item get x" ]]; then
  ok "English labels, human caller, footer, and the toast reason cut at 100 characters"
else
  ng "English labels, human caller, footer, and the toast reason cut at 100 characters" "$(status_field '.ui')"
fi
run notify-en "$(notify_payload en end false)"
title=$(status_field '.ui.toast[0]')
body=$(status_field '.ui.toast[1]')
run notify-ja "$(notify_payload ja start true)"
if [[ ${title} == "Could not clear a shared 1Password CLI approval" && ${body} == *"after the call"* &&
  $(status_field '.ui.toast[0]') == 共有の\ 1Password\ CLI\ の承認を消しました && $(status_field '.ui.toast[1]') == *"op を打つ前"* ]]; then
  ok "notify toast texts for cleared / not cleared, before / after the call"
else
  ng "notify toast texts for cleared / not cleared, before / after the call" "en=${title} / ${body} ja=$(status_field '.ui')"
fi

echo "== The language chosen in op-reason.exe's window"
settings_file=${HOME}/.local/share/op-reason/settings.json
mkdir -p "$(dirname "${settings_file}")"
rm -f "${settings_file}"
FACTS=${claude_facts} run nochoice "$(FACTS=${claude_facts} payload desktop "${fake_op}" item get x)"
nochoice=$(status_field '[.ui.title, (.chosenLang | tostring), (has("chosenLang") | tostring)] | join(" / ")')
printf '%s' '{"v":1,"lang":"en"}' >"${settings_file}"
FACTS=${claude_facts} run chosen-en "$(FACTS=${claude_facts} payload desktop "${fake_op}" item get x)"
chosen_en=$(status_field '[.ui.title, .ui.lines[0][0], .ui.toast[0], .chosenLang] | join(" / ")')
if [[ ${nochoice} == "1Password CLI の承認の依頼 / null / true" &&
  ${chosen_en} == "1Password CLI approval request / Time / 1Password CLI approval request / en" &&
  $(<"${settings_file}") == '{"v":1,"lang":"en"}' ]]; then
  ok "settings.json en beats the payload's ja and is reported as chosenLang; without it, the payload's language and null"
else
  ng "settings.json en beats the payload's ja and is reported as chosenLang; without it, the payload's language and null" \
    "no file: ${nochoice} | en: ${chosen_en} | file: $(cat "${settings_file}")"
fi
printf 'broken {' >"${settings_file}"
FACTS=${claude_facts} run chosen-broken "$(FACTS=${claude_facts} payload desktop "${fake_op}" item get x)"
broken=$(status_field '[.ui.title, (.chosenLang | tostring)] | join(" / ")')
printf '%s' '{"v":9,"lang":"ja","later":true}' >"${settings_file}"
run notify-chosen "$(notify_payload en start true)"
if [[ ${broken} == "1Password CLI の承認の依頼 / null" && $(status_field '.ui.toast[0]') == 共有の\ 1Password\ CLI\ の承認を消しました &&
$(status_field '.chosenLang') == ja ]]; then
  ok "a broken settings.json counts as no choice; a later format's lang is still read (notify toast)"
else
  ng "a broken settings.json counts as no choice; a later format's lang is still read (notify toast)" \
    "broken: ${broken} | notify: $(cat "${work}/status.json" 2>&1)"
fi
rm -f "${settings_file}"

echo "== The references a call reads"
account_file=${HOME}/.local/share/op-reason/account.json
rm -f "${account_file}"
id_ref=op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password
whoami_json='{"url":"https://example.1password.com/","email":"someone@example.com","user_uuid":"USERUSERUSERUSERUSERUSER12","account_uuid":"ABCDEFGHIJKLMNOPQRSTUVWXYZ","user_type":"HUMAN"}'
account_json='{"v":1,"accounts":[{"id":"ABCDEFGHIJKLMNOPQRSTUVWXYZ","host":"example.1password.com"}]}'
# refs_facts <list JSON> [<account>]: the human facts with references (10 in all, from the arguments and a template)
refs_facts() {
  jq -c --argjson list "$1" --arg account "${2-}" \
    '. + {refs: ({total: 10, from: [{kind: "args"}, {kind: "file", name: "app.tpl"}], list: $list}
      + (if $account == "" then {} else {account: $account} end))}' <<<"${human_facts}"
}
eight=$(jq -cn --arg id "${id_ref}" '[$id, "op://Private/proxmox/token", "op://$ENV/db/password"] + [range(5) | "op://v/i/f\(.)"]')
facts=$(refs_facts "${eight}")
FAKE_WHOAMI_RC=0 FAKE_WHOAMI_OUT=${whoami_json} run refs-first "$(FACTS=${facts} payload desktop "${fake_op}" inject)"
first=$(status_field '.ui.refs | [.head, ([.items[].linked] | map(tostring) | join(",")), .more, .hint] | join(" / ")')
first_items=$(status_field '[.ui.refs.items[].text] | join(",")')
copied=$(cat "${account_file}" 2>/dev/null)
detail="first: ${first} | items: ${first_items} | copy: ${copied} | $(cat "${work}/status.json")"
if [[ ${first} == "読む参照 (10 件・引数、app.tpl から) / false,false,false,false,false,false,false,false / ほか 2 件 / 項目へのリンクは、承認が通った呼び出しを 1 回終えると付きます (そのとき、op が既定で使うアカウントの ID を覚えます)" &&
  ${first_items} == "$(jq -r 'join(",")' <<<"${eight}")" && ${copied} == "${account_json}" ]] \
  && expect_args 2 whoami --format json; then
  ok "the first call: the block without links and the hint; whoami keeps only the account ID and host in account.json"
else
  ng "the first call: the block without links and the hint; whoami keeps only the account ID and host in account.json" "${detail}"
fi
LANG_CODE=en run refs-second "$(LANG_CODE=en FACTS=${facts} payload desktop "${fake_op}" inject)"
second=$(status_field '.ui.refs | [.head, ([.items[].linked] | map(tostring) | join(",")), .more, (.hint | tostring)] | join(" / ")')
detail="${second} | $(cat "${work}/status.json")"
if [[ ${second} == "References to read (10, from arguments, app.tpl) / true,false,false,false,false,false,false,false / and 2 more / null" ]] \
  && ! grep -q 'onepassword:' "${work}/status.json"; then
  ok "the next call links the ID reference (English texts); the status says linked, not the URL"
else
  ng "the next call links the ID reference (English texts); the status says linked, not the URL" "${detail}"
fi
run refs-other "$(FACTS=$(refs_facts "${eight}" other) payload desktop "${fake_op}" inject)"
other=$(status_field '[.ui.refs.items[0].linked, (.ui.refs.hint != null)] | map(tostring) | join(",")')
run refs-named "$(FACTS=$(refs_facts "${eight}" Example) payload desktop "${fake_op}" inject)"
named=$(status_field '.ui.refs.items[0].linked | tostring')
detail="other=${other} named=${named}"
check "--account of an account not remembered: no link and the hint; by the host's first label: the link" \
  test "${other}" == "false,true" -a "${named}" == true
link_problems=""
while IFS=$'\t' read -r ref want; do
  run refs-link "$(FACTS=$(refs_facts "$(jq -cn --arg r "${ref}" '[$r]')") payload desktop "${fake_op}" read)"
  got=$(status_field '.ui.refs.items[0].linked | tostring')
  [[ ${got} == "${want}" ]] || link_problems+="${ref}: ${got} (want ${want}); "
done < <(jq -r '.links[] | [.ref, (.vault != null | tostring)] | @tsv' "${here}/cases/refs.json")
detail=${link_problems}
check "the shared link table (tests/cases/refs.json): an ID reference is a link, nothing else is" test -z "${link_problems}"
run refs-none "$(FACTS=${human_facts} payload desktop "${fake_op}" item get x)"
detail="$(cat "${work}/status.json")"
check "no references: no block" test "$(status_field '.ui | has("refs")')" == false
rm -f "${account_file}"

echo "== stderr scan (the Windows entry)"
STDERR_MODE=scan FAKE_RC=1 FAKE_ERR_TEXT="[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again" \
  run scan-denied "$(STDERR_MODE=scan payload desktop "${fake_op}" item get x)"
if [[ ${rc} == 1 && $(status_field '.stderrMatch') == denied &&
$(<"${work}/scan-denied.err") == *"authorization prompt dismissed"* ]]; then
  ok "scan: op's stderr is relayed and a denial is reported (stderrMatch denied)"
else
  ng "scan: op's stderr is relayed and a denial is reported (stderrMatch denied)" "rc=${rc} err=$(cat "${work}/scan-denied.err") status=$(cat "${work}/status.json")"
fi
STDERR_MODE=scan FAKE_RC=1 FAKE_ERR_TEXT="[ERROR] 2026/11/08 00:00:00 authorization timeout" \
  run scan-timeout "$(STDERR_MODE=scan payload desktop "${fake_op}" item get x)"
detail="$(cat "${work}/status.json")"
check "scan: a timeout is reported (stderrMatch timeout)" test "$(status_field '.stderrMatch')" == timeout
STDERR_MODE=scan FAKE_RC=1 FAKE_ERR_TEXT="curl: (28) Connection timed out after 1000 ms" \
  run scan-other "$(STDERR_MODE=scan payload desktop "${fake_op}" item get x)"
run inherit-mode "$(payload desktop "${fake_op}" item get x)"
if [[ $(status_field '.stderrMatch') == null ]] && [[ $(jq -r 'has("stderrMatch")' "${work}/status.json") == false ]]; then
  ok "scan ignores a child's \"Connection timed out\"; inherit mode reports no stderrMatch"
else
  ng "scan ignores a child's \"Connection timed out\"; inherit mode reports no stderrMatch" "$(cat "${work}/status.json")"
fi
stdin_file=${in_bin}
STDERR_MODE=scan run scan-bin "$(STDERR_MODE=scan payload desktop "${fake_op}" item get x)"
stdin_file=""
detail="in=$(stat -c %s "${in_bin}") out=$(stat -c %s "${work}/scan-bin.out")"
check "scan: stdin and stdout stay binary-safe" cmp -s "${in_bin}" "${work}/scan-bin.out"

echo "== The named mutex"
holder_file=${HOME}/.local/share/op-reason/holder.json
run lock-normal "$(payload desktop "${fake_op}" item get x)"
if [[ $(status_field '.lock') == held && ! -e ${holder_file} ]]; then
  ok "a call takes the mutex (status lock held) and removes its holder.json"
else
  ng "a call takes the mutex (status lock held) and removes its holder.json" "$(cat "${work}/status.json") holder=$(cat "${holder_file}" 2>&1)"
fi
ready=${work}/mutex.ready
# shellcheck disable=SC2016 # PowerShell code
READY=${ready} pwsh -NoProfile -NonInteractive -Command '
  $m = New-Object System.Threading.Mutex($false, "Local\op-reason")
  [void]$m.WaitOne()
  New-Item -ItemType File -Path $env:READY | Out-Null
  Start-Sleep -Seconds 12
  $m.ReleaseMutex()' &
mutex_pid=$!
wait_file "${ready}" 400 || true
mkdir -p "$(dirname "${holder_file}")"
printf '%s' '{"time":"12:00:00","caller":"人","reason":"前の呼び出し","command":"op vault list","pid":1}' >"${holder_file}"
LOCK_WAIT=1 run lock-timeout "$(LOCK_WAIT=1 payload desktop "${fake_op}" item get x)"
if [[ ${rc} == 75 && $(status_field '.lock') == timeout && $(status_field '.error') == lock-timeout &&
$(status_field '.started') == false && $(status_field '.holder') == *前の呼び出し* && $(calls) == 0 &&
$(<"${work}/lock-timeout.err") == *待っています* ]]; then
  ok "a held mutex: wait, then exit 75 with lock timeout, the holder from holder.json and one waiting line"
else
  ng "a held mutex: wait, then exit 75 with lock timeout, the holder from holder.json and one waiting line" "rc=${rc} calls=$(calls) err=$(cat "${work}/lock-timeout.err") status=$(cat "${work}/status.json")"
fi
LOCK_MODE=skipped LOCK_WAIT=1 run lock-skipped "$(LOCK_MODE=skipped LOCK_WAIT=1 payload desktop "${fake_op}" item get x)"
if [[ ${rc} == 0 && $(calls) == 1 && $(status_field '.lock') == skipped && -e ${holder_file} ]]; then
  ok "lock skipped: runs while the mutex is held and leaves the holder's file alone"
else
  ng "lock skipped: runs while the mutex is held and leaves the holder's file alone" "rc=${rc} calls=$(calls) status=$(cat "${work}/status.json")"
fi
kill "${mutex_pid}" 2>/dev/null
wait "${mutex_pid}" 2>/dev/null
rm -f "${holder_file}"

finish
