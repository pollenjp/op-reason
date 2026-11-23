#!/usr/bin/env bash
# shellcheck shell=bash
#
# Tests the fish completion that `op-reason --completion fish` prints, with the real fish and a fake op. Ported from
# the dotfiles' test.
#
#   bash tests/completion.test.sh [<op-reason binary>]
#
# Needs bash, coreutils and fish.
#
# ## What it checks
#
#   Premise: without an op.fish on the path, fish's shipped completion runs `op completion fish`, and op's stderr shows
#     up (why this completion is needed; also checks that the test's fish reads completions from the given directory)
#   Never runs op: for every line below the fake op is not called once and fish's stderr stays empty
#   op <Tab>: --reason and the top-level commands (item, read, run, vault, whoami, help, …); no files
#   op --<Tab>: --reason and op's global flags (--account, --format, --help, --version), and the wrapper options
#     (--skill, --log, --completion, --settings) as the first argument only
#   --reason: not offered again once written (--reason X, --reason=X); while its value is written, neither commands
#     nor files are offered
#   Command path: values of --reason, --account and the like are not read as commands, so op item <Tab> gives the
#     second level (get, list, create, …); op connect vault <Tab> (third level) does not give vault's second level;
#     op help <Tab> gives the top-level commands
#   Flags: op item get --<Tab> gives item get's flags (--vault, --fields, --reveal, --otp), the global flags and
#     --reason, not other commands' flags (--out-file); op --format <Tab> gives human-readable and json
#   Files: values of --out-file and of the short forms (op read -o, op inject -i), and op document create's argument
#   After -- (op run's child command): the child command's names and its own completion; no op flags. op -- <Tab> gives
#     neither op's commands nor --reason
#   op-reason as the command name: the same completion works for op-reason
#
# ## How
#
# The fake op appends its arguments to work/op.calls, writes a refusal to stderr and exits 2 (like the wrapper without
# a reason). fish starts with --no-config (fish_complete_path is empty); the completion under test (written as op.fish)
# and the child command's completion are put first on fish_complete_path, and complete -C lists the candidates. That is
# read before the shipped completion, the same as ~/.config/fish/completions.

# No -e: a failed check must not stop the checks after it.
set -u -o pipefail

here=$(
  cd -- "$(dirname "$0")" &>/dev/null || exit
  pwd -P
)
# shellcheck source=tests/lib.sh
. "${here}/lib.sh"
bin=${1:-${OPREASON_BIN:-${here}/../target/debug/op-reason}}
work=$(mktemp -d "${TMPDIR:-/tmp}/op-reason-completion-test.XXXXXX")
work=$(cd "${work}" && pwd -P)
trap 'rm -rf "${work}"' EXIT

export LC_ALL=C.UTF-8
export HOME=${work}/home
export XDG_CONFIG_HOME=${HOME}/.config XDG_DATA_HOME=${HOME}/.local/share XDG_CACHE_HOME=${HOME}/.cache
mkdir -p "${HOME}" "${work}/bin" "${work}/comp" "${work}/comp2" "${work}/empty" "${work}/cwd"

"${bin}" --completion fish >"${work}/comp/op.fish"
"${bin}" --completion fish op-reason >"${work}/comp2/op-reason.fish"
export PATH=${work}/bin:${PATH}

cat >"${work}/bin/op" <<EOF
#!${BASH}
printf '%s\n' "\$*" >>"${work}/op.calls"
echo "op-reason: 理由 (--reason) がありません。人が打つときも要ります。" >&2
exit 2
EOF
printf '#!%s\nexit 0\n' "${BASH}" >"${work}/bin/pjp-fake-child"
printf '#!%s\nexit 0\n' "${BASH}" >"${work}/bin/op-reason"
chmod +x "${work}/bin/op" "${work}/bin/pjp-fake-child" "${work}/bin/op-reason"
echo 'complete -c pjp-fake-child -l child-flag -d "child flag"' >"${work}/comp/pjp-fake-child.fish"
touch "${work}/cwd/afile.txt"

# cands <line> [<completion directory>]: the candidate names for the line, one per line, into cand; fish's stderr into cand_err
cands() {
  # shellcheck disable=SC2016 # a fish expression ($argv is read by fish)
  cand=$(cd "${work}/cwd" && fish --no-config -c 'set -p fish_complete_path $argv[1]; complete -C $argv[2]' \
    "${2:-${work}/comp}" "$1" 2>"${work}/fish.err" | cut -f1)
  cand_err=$(<"${work}/fish.err")
}

# has <name>...: every name is in cand. lacks <name>...: no name is in cand
has() {
  local w
  for w in "$@"; do
    [[ $'\n'${cand}$'\n' == *$'\n'"${w}"$'\n'* ]] || return 1
  done
}
lacks() {
  local w
  for w in "$@"; do
    [[ $'\n'${cand}$'\n' != *$'\n'"${w}"$'\n'* ]] || return 1
  done
}

shown() {
  printf 'cand=%s err=%s' "$(tr '\n' ' ' <<<"${cand}")" "${cand_err}"
}

echo "== Premise"
cands "op " "${work}/empty"
if [[ $(cat "${work}/op.calls" 2>/dev/null) == "completion fish" && ${cand_err} == *理由* ]]; then
  ok "without op.fish on the path, fish's shipped completion runs op completion fish and op's stderr shows"
else
  ng "without op.fish on the path, fish's shipped completion runs op completion fish and op's stderr shows" \
    "calls=$(cat "${work}/op.calls" 2>&1) $(shown)"
fi
rm -f "${work}/op.calls"

# From here on, with op.fish in place: fish's stderr is empty for every line, and at the end op was never called
quiet() {
  [[ -z ${cand_err} ]]
}

echo "== Top level"
cands "op "
if has --reason item read run vault whoami help && lacks afile.txt && quiet; then
  ok "op <Tab> gives --reason and the top-level commands, no files"
else
  ng "op <Tab> gives --reason and the top-level commands, no files" "$(shown)"
fi
cands "op --"
if has --reason --account --format --help --version --skill --log --completion --settings && quiet; then
  ok "op --<Tab> gives --reason, the global flags and the wrapper options"
else
  ng "op --<Tab> gives --reason, the global flags and the wrapper options" "$(shown)"
fi

echo "== --reason"
cands 'op --reason "a b" '
if has item read && lacks --reason && quiet; then
  ok "after --reason \"a b\": the top-level commands, no --reason again"
else
  ng "after --reason \"a b\": the top-level commands, no --reason again" "$(shown)"
fi
cands "op --reason "
if lacks item read afile.txt && quiet; then
  ok "while the value of --reason is written: no commands, no files"
else
  ng "while the value of --reason is written: no commands, no files" "$(shown)"
fi
cands "op --reason=x item get --"
if has --vault && lacks --reason && quiet; then
  ok "a line with --reason=x does not offer --reason"
else
  ng "a line with --reason=x does not offer --reason" "$(shown)"
fi

echo "== Command path"
cands 'op --reason "a b" item '
if has get list create edit delete && lacks read user && quiet; then
  ok "op --reason \"a b\" item <Tab> gives item's second level (not the top level or vault's)"
else
  ng "op --reason \"a b\" item <Tab> gives item's second level (not the top level or vault's)" "$(shown)"
fi
cands "op --account my vault "
if has list get create user && lacks item && quiet; then
  ok "the value of --account is not a command: op --account my vault <Tab> gives vault's second level"
else
  ng "the value of --account is not a command: op --account my vault <Tab> gives vault's second level" "$(shown)"
fi
cands "op connect vault "
if lacks create list user item && quiet; then
  ok "op connect vault <Tab> (third level) gives neither vault's second level nor the top level"
else
  ng "op connect vault <Tab> (third level) gives neither vault's second level nor the top level" "$(shown)"
fi
cands "op help "
if has item vault read && lacks get && quiet; then
  ok "op help <Tab> gives the top-level commands"
else
  ng "op help <Tab> gives the top-level commands" "$(shown)"
fi

echo "== Flags"
cands "op item get --"
if has --vault --fields --reveal --otp --account --reason && lacks --out-file --skill --log --settings && quiet; then
  ok "op item get --<Tab> gives item get's flags, the global flags and --reason, not other commands' flags"
else
  ng "op item get --<Tab> gives item get's flags, the global flags and --reason, not other commands' flags" "$(shown)"
fi
cands "op --format "
if has human-readable json && lacks item && quiet; then
  ok "op --format <Tab> gives human-readable and json"
else
  ng "op --format <Tab> gives human-readable and json" "$(shown)"
fi

echo "== Files"
cands "op read --out-file "
if has afile.txt && quiet; then
  ok "the value of --out-file gives files"
else
  ng "the value of --out-file gives files" "$(shown)"
fi
for line in "op read -o " "op inject -i "; do
  cands "${line}"
  if has afile.txt && quiet; then
    ok "${line}<Tab> (value of a short flag) gives files"
  else
    ng "${line}<Tab> (value of a short flag) gives files" "$(shown)"
  fi
done
cands "op document create "
if has afile.txt && quiet; then
  ok "op document create <Tab> gives files"
else
  ng "op document create <Tab> gives files" "$(shown)"
fi

echo "== After --"
cands "op --reason x run -- pjp-fake-ch"
if has pjp-fake-child && quiet; then
  ok "after op run --: the child command names"
else
  ng "after op run --: the child command names" "$(shown)"
fi
cands "op --reason x run --env-file .env -- pjp-fake-child --"
if has --child-flag && lacks --reason --account --env-file && quiet; then
  ok "the child command's flags, not op's"
else
  ng "the child command's flags, not op's" "$(shown)"
fi
# After -- any command name is offered, so op commands that share a name with other commands (read is a fish builtin,
# whoami is in coreutils) are not checked
cands "op -- "
if lacks item vault events-api --reason && quiet; then
  ok "op -- <Tab> gives neither op's commands nor --reason"
else
  ng "op -- <Tab> gives neither op's commands nor --reason" "$(shown)"
fi

echo "== op-reason as the command name"
cands "op-reason --reason x item " "${work}/comp2"
if has get list create && lacks read && quiet; then
  ok "the completion for op-reason works the same way"
else
  ng "the completion for op-reason works the same way" "$(shown)"
fi

echo "== Never runs op"
if [[ ! -e ${work}/op.calls ]]; then
  ok "op is never called by any completion above"
else
  ng "op is never called by any completion above" "$(cat "${work}/op.calls")"
fi

finish
