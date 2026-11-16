#!/usr/bin/env bash
# shellcheck shell=bash
# shellcheck disable=SC2016 # backquotes in single quotes are Markdown code
#
# Tests .github/scripts/release-notes.sh, which makes the notes of a release and checks the versions, on made-up
# repositories, then on this one.
#
#   bash tests/release-notes.test.sh
#
# Needs bash, coreutils and awk.
#
# ## What it checks
#
#   Notes
#     - "## Changes" with the CHANGELOG.md section of the version, without the blank lines around it, then "## Assets";
#       nothing of the sections before (Unreleased) and after it
#     - the version is the one in the [workspace.package] table of Cargo.toml, not in another table
#     - the wrapped lines of a paragraph and of a list item are joined; list items, nested items, headings, quotes, table
#       rows and fenced code keep their lines
#   Tags: v<version> and v<version>-<pre-release> pass; another version, a longer one, no v, an empty pre-release fail
#   Fails with nothing on stdout when crates/op-reason-win/Cargo.toml or windows/op-reason.ps1 has another version, when
#   Cargo.toml has none, when CHANGELOG.md has no section of the version or an empty one
#   This repository: the versions agree and CHANGELOG.md has their section

# No -e: a failed check must not stop the checks after it.
set -u -o pipefail

here=$(
  cd -- "$(dirname "$0")" &>/dev/null || exit
  pwd -P
)
# shellcheck source=tests/lib.sh
. "${here}/lib.sh"
script=${here}/../.github/scripts/release-notes.sh
work=$(mktemp -d "${TMPDIR:-/tmp}/op-reason-release-notes-test.XXXXXX")
trap 'rm -rf "${work}"' EXIT

# repo <name> [<version> [<op-reason-win version> [<entry version>]]]: a made-up repository in ${work}/<name> with a
# copy of the script. Every version is 1.2.3 by default; CHANGELOG.md has an Unreleased section, one of 1.2.3 with
# every kind of block, and one of 1.2.2.
repo() {
  local dir=${work}/$1
  mkdir -p "${dir}/.github/scripts" "${dir}/crates/op-reason-win" "${dir}/windows"
  cp "${script}" "${dir}/.github/scripts/"
  cat >"${dir}/Cargo.toml" <<EOF
[workspace]
members = ["a"]

[package]
version = "9.9.9"

[workspace.package]
version = "${2:-1.2.3}"
edition = "2024"

[workspace.dependencies]
serde = { version = "1.0" }
EOF
  printf '[workspace.package]\nversion = "%s"\n' "${3:-1.2.3}" >"${dir}/crates/op-reason-win/Cargo.toml"
  {
    printf '\xef\xbb\xbf'
    cat <<EOF
# The PowerShell entry
\$ProgressPreference = 'SilentlyContinue'
\$script:Version = '${4:-1.2.3}'
EOF
  } >"${dir}/windows/op-reason.ps1"
  cat >"${dir}/CHANGELOG.md" <<'EOF'
# Changelog

## Unreleased

- next

## 1.2.3 — 2027-01-01


- **One.** A first item, wrapped
  over two lines.
- Two
  - nested, wrapped
    too
- Three

A paragraph, wrapped
over lines.

### Below

> a quote
> of two lines

| a | b |
| --- | --- |

```text
code stays
  as it is
```


## 1.2.2 — 2026-12-01

- old
EOF
}

# run <name> [<tag>]: runs the script of a made-up repository; sets rc, out and err
run() {
  local dir=${work}/$1
  shift
  out=$(bash "${dir}/.github/scripts/release-notes.sh" "$@" 2>"${work}/err")
  rc=$?
  err=$(<"${work}/err")
}

want_changes='## Changes

- **One.** A first item, wrapped over two lines.
- Two
  - nested, wrapped too
- Three

A paragraph, wrapped over lines.

### Below

> a quote
> of two lines

| a | b |
| --- | --- |

```text
code stays
  as it is
```

## Assets
'

echo "## Notes"
repo good
run good
if [[ ${rc} == 0 && ${out} == "${want_changes}"* && -z ${err} ]]; then
  ok "the section of the version from [workspace.package], its lines joined where they are wrapped, then the assets"
else
  ng "the section of the version from [workspace.package], its lines joined where they are wrapped, then the assets" "rc=${rc} err=${err}
${out}"
fi
assets=${out#"${want_changes}"}
if [[ $(grep -c . <<<"${assets}") == 1 && ${assets} == *'`op-reason-<arch>-linux` is the WSL executable.'*'`.sha256` file.' ]]; then
  ok "the assets are one paragraph on one line, <arch> in code"
else
  ng "the assets are one paragraph on one line, <arch> in code" "${assets}"
fi
if [[ ${out} != *'- next'* && ${out} != *'- old'* && ${out} != *Unreleased* && ${out} != *1.2.2* ]]; then
  ok "nothing of the sections before and after"
else
  ng "nothing of the sections before and after" "${out}"
fi

echo "## Tags"
for tag in v1.2.3 v1.2.3-rc.1 v1.2.3-beta; do
  run good "${tag}"
  if [[ ${rc} == 0 && ${out} == "${want_changes}"* ]]; then
    ok "${tag} passes"
  else
    ng "${tag} passes" "rc=${rc} err=${err}"
  fi
done
for tag in v1.2.4 v1.2.30 v1.2 1.2.3 v1.2.3- x; do
  run good "${tag}"
  if [[ ${rc} == 1 && -z ${out} && ${err} == *"the tag ${tag} is neither v1.2.3 nor v1.2.3-<pre-release>"* ]]; then
    ok "${tag} fails"
  else
    ng "${tag} fails" "rc=${rc} err=${err} out=${out}"
  fi
done

echo "## Versions that do not agree"
# fails <description> <name> <expected on stderr> [<tag>]
fails() {
  local desc=$1 name=$2 want=$3
  shift 3
  run "${name}" "$@"
  if [[ ${rc} == 1 && -z ${out} && ${err} == *"${want}"* ]]; then
    ok "${desc}"
  else
    ng "${desc}" "rc=${rc} err=${err} out=${out}"
  fi
}
repo win 1.2.3 1.2.4
fails "op-reason-win has another version" win "crates/op-reason-win/Cargo.toml has version '1.2.4', Cargo.toml has 1.2.3"
repo entry 1.2.3 1.2.3 1.3.0
fails "the PowerShell entry has another version" entry "windows/op-reason.ps1 has version '1.3.0', Cargo.toml has 1.2.3"
fails "also with a tag of the version" entry "windows/op-reason.ps1 has version '1.3.0'" v1.2.3
repo none
printf '[workspace]\nmembers = ["a"]\n\n[package]\nversion = "1.2.3"\n' >"${work}/none/Cargo.toml"
fails "Cargo.toml has no version in [workspace.package]" none "Cargo.toml has no version in its [workspace.package] table"

echo "## CHANGELOG.md"
repo missing 2.0.0 2.0.0 2.0.0
fails "no section of the version" missing 'CHANGELOG.md has no section "## 2.0.0 — <date>", or it is empty'
repo empty
printf '# Changelog\n\n## 1.2.3 — 2027-01-01\n\n\n## 1.2.2 — 2026-12-01\n\n- old\n' >"${work}/empty/CHANGELOG.md"
fails "an empty section" empty 'CHANGELOG.md has no section "## 1.2.3 — <date>", or it is empty'
repo last
printf '# Changelog\n\n## 1.2.3\n\nThe first\nrelease.\n' >"${work}/last/CHANGELOG.md"
run last v1.2.3
if [[ ${rc} == 0 && ${out} == $'## Changes\n\nThe first release.\n\n## Assets\n'* ]]; then
  ok "the last section, and a heading without a date"
else
  ng "the last section, and a heading without a date" "rc=${rc} err=${err} out=${out}"
fi

echo "## This repository"
out=$(bash "${script}" 2>"${work}/err")
rc=$?
if [[ ${rc} == 0 && ${out} == '## Changes'$'\n\n'?*$'\n\n''## Assets'$'\n\n'?* ]]; then
  ok "the versions agree and CHANGELOG.md has their section"
else
  ng "the versions agree and CHANGELOG.md has their section" "rc=${rc} err=$(<"${work}/err")"
fi

finish
