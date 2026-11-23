# op-reason's skill in this WSL distribution (design docs/superpowers/specs/2026-11-23-settings-design.md §10.2).
# op-reason.exe runs it as
#   wsl.exe -d <distribution> --exec sh -c "<this script>" op-reason-skill <look|place|remove> <version>
# with the file that it would place on stdin, and reads one line:
#   op-reason-skill 1 <missing|ok|old|changed|foreign> <version or -> <claude|skills|op-reason|SKILL.md or -> <path>
#   op-reason-skill 1 error - - <message>
# The fourth field names the first symbolic link on the way to the file. Nothing is written through a link, and only a
# file that carries op-reason's mark is replaced or removed; a refused action reports the state it left.
set -u
act=${1:-look}
version=${2:-}
error() {
  printf 'op-reason-skill 1 error - - %s\n' "$1"
  exit 0
}
home=${HOME:-}
if [ -z "$home" ]; then
  home=$(getent passwd "$(id -u)" 2>/dev/null | cut -d: -f6)
fi
if [ -z "$home" ] || [ ! -d "$home" ]; then
  error "no home folder"
fi
claude=$home/.claude
skills=$claude/skills
dir=$skills/op-reason
file=$dir/SKILL.md
want=$(mktemp 2>/dev/null) || want=${TMPDIR:-/tmp}/op-reason-skill.$$
trap 'rm -f "$want"' EXIT
cat >"$want" || error "could not read the file to place"
link() {
  if [ -L "$claude" ]; then
    echo claude
  elif [ -L "$skills" ]; then
    echo skills
  elif [ -L "$dir" ]; then
    echo op-reason
  elif [ -L "$file" ]; then
    echo SKILL.md
  else
    echo -
  fi
}
# The state and the version of op-reason's mark
state() {
  if [ ! -e "$file" ]; then
    echo "missing -"
    return
  fi
  if [ -f "$file" ] && cmp -s "$file" "$want"; then
    echo "ok $version"
    return
  fi
  v=$(head -n 40 "$file" 2>/dev/null | sed -n 's/^<!-- placed by op-reason \([^:]*\):.*/\1/p' | head -n 1)
  if [ -z "$v" ]; then
    echo "foreign -"
  elif [ "$v" = "$version" ]; then
    echo "changed $v"
  else
    echo "old $v"
  fi
}
report() {
  printf 'op-reason-skill 1 %s %s %s\n' "$(state)" "$(link)" "$file"
  exit 0
}
case $act in
look)
  report
  ;;
place)
  if [ "$(link)" != - ]; then
    report
  fi
  case $(state) in
  foreign*) report ;;
  esac
  mkdir -p "$dir" || error "could not make $dir"
  if [ "$(link)" != - ]; then
    report
  fi
  tmp=$dir/.SKILL.md.$$
  if ! cp "$want" "$tmp" || ! mv -f "$tmp" "$file"; then
    rm -f "$tmp"
    error "could not write $file"
  fi
  report
  ;;
remove)
  if [ "$(link)" != - ]; then
    report
  fi
  case $(state) in
  ok* | old* | changed*)
    rm -f "$file" || error "could not remove $file"
    rmdir "$dir" 2>/dev/null
    ;;
  esac
  report
  ;;
*)
  error "unknown action $act"
  ;;
esac
