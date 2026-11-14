#!/usr/bin/env bash
# shellcheck shell=bash
#
# Runs every black-box test against one op-reason binary.
#
#   bash tests/run-all.sh [<op-reason binary>]     (default: target/debug/op-reason)
#
# Needs the tools of the development shell (nix develop path:./nix): pwsh, fish, jq, flock, setsid, git.

# No -e: a failed check must not stop the checks after it.
set -u -o pipefail

here=$(
  cd -- "$(dirname "$0")" &>/dev/null || exit
  pwd -P
)
bin=${1:-${OPREASON_BIN:-${here}/../target/debug/op-reason}}
failed=()
for t in wrapper e2e log completion launcher; do
  echo "### ${t}"
  if [[ ${t} == launcher ]]; then
    bash "${here}/${t}.test.sh" || failed+=("${t}")
  else
    bash "${here}/${t}.test.sh" "${bin}" || failed+=("${t}")
  fi
done
echo "### entry-plan (the call table against the Windows entry, on PowerShell 7)"
pwsh -NoProfile -NonInteractive -File "${here}/windows/entry-plan.test.ps1" || failed+=(entry-plan)
if ((${#failed[@]} > 0)); then
  echo "failed: ${failed[*]}"
  exit 1
fi
echo "all black-box tests passed"
