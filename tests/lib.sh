# shellcheck shell=bash
#
# Helpers shared by the black-box tests (sourced, not run).

pass=0
fail=0

ok() {
  pass=$((pass + 1))
  echo "  ok   $1"
}

ng() {
  fail=$((fail + 1))
  echo "  FAIL $1"
  if [[ -n ${2:-} ]]; then
    echo "       ${2//$'\n'/$'\n'       }"
  fi
}

# check <description> <command>...: ok when the command succeeds, FAIL otherwise (with $detail when set)
check() {
  local desc=$1
  shift
  if "$@"; then
    ok "${desc}"
  else
    ng "${desc}" "${detail:-}"
  fi
}

# fake <path> <<'EOF' … EOF: writes an executable script with a shebang made from $BASH (the Nix sandbox and some CI
# images have no /usr/bin/env)
fake() {
  {
    printf '#!%s\n' "${BASH}"
    cat
  } >"$1"
  chmod +x "$1"
}

# finish: prints the totals and returns non-zero when a check failed
finish() {
  echo
  echo "pass=${pass} fail=${fail}"
  [[ ${fail} == 0 ]]
}

# wait_file <path> [<tries of 0.05 s>]: waits until the path exists (default 400 tries = 20 s)
wait_file() {
  local i
  for ((i = 0; i < ${2:-400}; i++)); do
    [[ ! -e $1 ]] || return 0
    sleep 0.05
  done
  [[ -e $1 ]]
}
