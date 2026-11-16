#!/usr/bin/env bash
# shellcheck shell=bash
#
# Prints the notes of a release: the section of CHANGELOG.md for the version, then what the assets are. The release
# workflow (release.yml) gives them to the draft release. ci.yml runs this script on every pull request, so that a
# version without its section fails there, before a tag needs it.
#
#   bash .github/scripts/release-notes.sh [<tag>]
#
# The version is the one in Cargo.toml. Fails (exit 1, the reason on stderr, nothing on stdout) unless
# crates/op-reason-win/Cargo.toml and windows/op-reason.ps1 have the same version, the tag (when given) is v<version>
# or v<version>-<pre-release> (such as v0.2.0-rc.1), and CHANGELOG.md has a section "## <version> — <date>" with
# something in it.
#
# GitHub shows every line break of a release's notes, so the lines of a paragraph or a list item, wrapped in the file,
# are joined. Lines that start a block (a list item, a heading, a quote, a table row, HTML) stay lines of their own,
# and fenced code is kept as it is.

set -eu -o pipefail

root=$(
  cd -- "$(dirname "$0")/../.." &>/dev/null || exit
  pwd -P
)

die() {
  echo "release-notes.sh: $*" >&2
  exit 1
}

# cargo_version <Cargo.toml>: the version in its [workspace.package] table
cargo_version() {
  awk '
    /^\[/ { table = $0; next }
    table == "[workspace.package]" && /^version *= *"/ { sub(/^version *= *"/, ""); sub(/".*/, ""); print; exit }
  ' "$1"
}

version=$(cargo_version "${root}/Cargo.toml")
[[ -n ${version} ]] || die "Cargo.toml has no version in its [workspace.package] table"
win=$(cargo_version "${root}/crates/op-reason-win/Cargo.toml")
[[ ${win} == "${version}" ]] || die "crates/op-reason-win/Cargo.toml has version '${win}', Cargo.toml has ${version}"
entry=$(awk -F "'" '/^\$script:Version = / { print $2; exit }' "${root}/windows/op-reason.ps1")
[[ ${entry} == "${version}" ]] || die "windows/op-reason.ps1 has version '${entry}', Cargo.toml has ${version}"

if (($# > 0)); then
  case $1 in
    "v${version}" | "v${version}-"?*) ;;
    *) die "the tag $1 is neither v${version} nor v${version}-<pre-release> (${version} is the version in Cargo.toml)" ;;
  esac
fi

# The lines between "## <version> …" and the next "## " heading, without the blank lines around them
section=$(awk -v version="${version}" '
  /^## / { if (found) exit; found = ($2 "" == version ""); next }
  found { line[++n] = $0; if ($0 ~ /[^ \t]/) { if (!first) first = n; last = n } }
  END { for (i = first; first && i <= last; i++) print line[i] }
' "${root}/CHANGELOG.md")
[[ -n ${section} ]] || die "CHANGELOG.md has no section \"## ${version} — <date>\", or it is empty"

# unwrap: joins the wrapped lines of paragraphs and list items (see the top)
unwrap() {
  awk '
    function flush() { if (buf != "") print buf; buf = "" }
    /^[ \t]*```/ { flush(); fence = !fence; print; next }
    fence { print; next }
    /^[ \t]*$/ { flush(); print ""; next }
    /^[ \t]*([-*+] |[0-9]+[.)] |#|>|\||<)/ { flush(); buf = $0; if ($0 ~ /^[ \t]*#/) flush(); next }
    { if (buf != "") { sub(/^[ \t]+/, ""); buf = buf " " $0 } else buf = $0 }
    END { flush() }
  '
}

{
  printf '## Changes\n\n%s\n\n' "${section}"
  # `<arch>` in code: outside of it, Markdown takes <arch> for an HTML tag and drops it
  cat <<'EOF'
## Assets

See the [README](https://github.com/pollenjp/op-reason#readme) for installation. `op-reason-<arch>-linux` is the WSL
executable. `op-reason-windows.zip` holds `op-reason.exe` (the Windows entry, and the window of calls from WSL; not
signed) and the PowerShell fallback for machines where Smart App Control blocks `op-reason.exe`. Install both on a WSL
machine. Each asset has a `.sha256` file.
EOF
} | unwrap
