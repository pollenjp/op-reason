#!/usr/bin/env bash
# Posts the window screenshots taken by the Windows CI job as one sticky comment on the pull request (adapted from a
# template for end-to-end screenshot comments). The images are pushed to a branch per pull request
# (screenshots/pr-<number>) and referenced by the commit that stores them; .github/workflows/screenshots-cleanup.yml
# deletes the branch when the pull request closes.
#
#   pr-screenshots.sh <pr_number> <head_sha> <job_status>
#
# Needs GITHUB_REPOSITORY and GITHUB_TOKEN (gh and git push). Optional: SHOTS_DIR (default screenshots),
# MAX_ARCHIVED (15), GRID_COLS (4), IMG_WIDTH (200), SHOTS_BRANCH (screenshots/pr-<number>), SHOTS_MARKER,
# SHOTS_TITLE, SHOTS_SOURCE.
#
# Check the markup without any API call or push: RENDER_TEST=1 .github/scripts/pr-screenshots.sh
set -eu -o pipefail

# To post two sets of screenshots in one repository, give each set its own SHOTS_MARKER and SHOTS_BRANCH.
MARKER=${SHOTS_MARKER:-"<!-- window-screenshots -->"}
STATE_PREFIX="e2e-shots-state:"
SHOTS_DIR=${SHOTS_DIR:-screenshots}
SHOTS_TITLE=${SHOTS_TITLE:-"Window screenshots"}
SHOTS_SOURCE=${SHOTS_SOURCE:-}
MAX_ARCHIVED=${MAX_ARCHIVED:-15}
GRID_COLS=${GRID_COLS:-4}
IMG_WIDTH=${IMG_WIDTH:-200}

# --- Rendering (state JSON -> comment body) ---------------------------------------

# $1: entries JSON (newest first). Prints the body.
render_body() {
  local entries="$1"

  echo "${MARKER}"
  echo "### ${SHOTS_TITLE}"
  echo
  echo "Screenshots of the latest run. Earlier commits are kept under archived (up to ${MAX_ARCHIVED})."
  if [ -n "${SHOTS_SOURCE}" ]; then
    echo "Made by \`${SHOTS_SOURCE}\`; the images live on the \`${SHOTS_BRANCH}\` branch."
  else
    echo "The images live on the \`${SHOTS_BRANCH}\` branch."
  fi
  echo

  local count
  count=$(jq 'length' <<<"$entries")
  if [ "$count" -eq 0 ]; then
    echo "(no screenshots yet)"
  else
    render_entry "$(jq -c '.[0]' <<<"$entries")"
    if [ "$count" -gt 1 ]; then
      echo "<details>"
      echo "<summary>archived ($((count - 1)) earlier)</summary>"
      echo
      local i
      for ((i = 1; i < count; i++)); do
        render_entry "$(jq -c ".[$i]" <<<"$entries")"
      done
      echo "</details>"
    fi
  fi

  echo
  echo "<!-- ${STATE_PREFIX} $(jq -c . <<<"$entries") -->"
}

# $1: entry JSON {sha, result, time, files[]}
render_entry() {
  local entry="$1" sha result time short emoji
  sha=$(jq -r '.sha' <<<"$entry")
  result=$(jq -r '.result' <<<"$entry")
  time=$(jq -r '.time' <<<"$entry")
  short=${sha:0:7}
  emoji="✅"
  [ "$result" = "success" ] || emoji="❌"

  echo "<details>"
  echo "<summary><code>${short}</code> ${emoji} ${time}</summary>"
  echo
  render_grid "$sha" "$entry"
  echo
  echo "</details>"
}

# $1: sha, $2: entry JSON. Lays the files out in a <table> of GRID_COLS columns.
render_grid() {
  local sha="$1" entry="$2"
  local repo="${GITHUB_REPOSITORY:-owner/repo}"
  # Images are referenced by the commit that stored them (a permalink): the branch tip moves every run.
  # Entries without shots_commit fall back to the branch.
  local ref
  ref=$(jq -r '.shots_commit // ""' <<<"$entry")
  [ -n "$ref" ] || ref="$SHOTS_BRANCH"
  # <img> uses github.com/.../raw (rendered for signed-in browsers even in private repositories); links use blob.
  local raw_base="https://github.com/${repo}/raw/${ref}"
  local blob_base="https://github.com/${repo}/blob/${ref}"
  local files n i url blob_url
  mapfile -t files < <(jq -r '.files[]' <<<"$entry")
  n=${#files[@]}
  if [ "$n" -eq 0 ]; then
    echo "(no images: the run failed before the screenshots)"
    return
  fi
  echo "<table><tr>"
  for ((i = 0; i < n; i++)); do
    if [ "$i" -gt 0 ] && [ $((i % GRID_COLS)) -eq 0 ]; then
      echo "</tr><tr>"
    fi
    url="${raw_base}/${sha}/${files[$i]}"
    blob_url="${blob_base}/${sha}/${files[$i]}"
    echo "<td><a href=\"${blob_url}\"><img src=\"${url}\" width=\"${IMG_WIDTH}\" alt=\"${files[$i]}\"></a><br><sub>${files[$i]}</sub></td>"
  done
  echo "</tr></table>"
}

# --- RENDER_TEST: check the markup with made-up data ----------------------------

if [ "${RENDER_TEST:-}" = "1" ]; then
  SHOTS_BRANCH=${SHOTS_BRANCH:-screenshots/pr-0}
  entries=$(jq -n '[
    {sha: "aaaaaaa1111111111111111111111111111111111", result: "success",
     time: "2026-09-09 10:00 UTC",
     shots_commit: "cccccccc33333333333333333333333333333333",
     files: ["01-a.png","02-b.png","03-c.png","04-d.png","05-e.png"]},
    {sha: "bbbbbbb2222222222222222222222222222222222", result: "failure",
     time: "2026-09-09 09:00 UTC", files: ["01-a.png"]}
  ]')
  render_body "$entries"
  exit 0
fi

# --- Main ------------------------------------------------------------------------

PR_NUMBER=${1:?usage: pr-screenshots.sh <pr_number> <head_sha> <job_status>}
HEAD_SHA=${2:?head_sha required}
JOB_STATUS=${3:?job_status required}
REPO=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}
# One branch per pull request, so that runs of different pull requests do not overwrite each other
SHOTS_BRANCH=${SHOTS_BRANCH:-screenshots/pr-${PR_NUMBER}}

if [ ! -d "$SHOTS_DIR" ] || ! ls "$SHOTS_DIR"/*.png >/dev/null 2>&1; then
  echo "No screenshots (did the run fail before taking them?); no comment: $SHOTS_DIR"
  exit 0
fi

# Do nothing for a closed or merged pull request: a run that started just before closing must not push the branch
# again after the cleanup workflow deleted it.
pr_state=$(gh api "repos/${REPO}/pulls/${PR_NUMBER}" --jq '.state')
if [ "$pr_state" != "open" ]; then
  echo "Pull request ${PR_NUMBER} is ${pr_state}; skipped (do not bring a cleaned-up branch back)"
  exit 0
fi

# 1. Find the existing sticky comment (by its marker)
comments=$(gh api "repos/${REPO}/issues/${PR_NUMBER}/comments" --paginate)
comment_id=$(jq -r --arg m "$MARKER" \
  '[.[] | select(.body | startswith($m))][0].id // empty' <<<"$comments")

# 2. Read its state (an empty array when there is none)
prev_entries="[]"
if [ -n "$comment_id" ]; then
  prev_body=$(jq -r --arg m "$MARKER" \
    '[.[] | select(.body | startswith($m))][0].body' <<<"$comments")
  extracted=$(sed -n "s/.*<!-- ${STATE_PREFIX} \(.*\) -->.*/\1/p" <<<"$prev_body" | head -1)
  if [ -n "$extracted" ] && jq -e . >/dev/null 2>&1 <<<"$extracted"; then
    prev_entries="$extracted"
  fi
fi

# 3. Put this run first (a re-run of the same sha replaces it); keep the current one plus MAX archived
files_json=$(find "$SHOTS_DIR" -maxdepth 1 -name '*.png' -printf '%f\n' | sort | jq -R . | jq -sc .)
now=$(date -u '+%Y-%m-%d %H:%M UTC')
entries=$(jq -c \
  --arg sha "$HEAD_SHA" --arg result "$JOB_STATUS" --arg time "$now" \
  --argjson files "$files_json" --argjson max "$MAX_ARCHIVED" \
  '[{sha: $sha, result: $result, time: $time, files: $files}]
   + [.[] | select(.sha != $sha)] | .[0:($max + 1)]' <<<"$prev_entries")

# 4. Store the screenshots on SHOTS_BRANCH (append-only, never force-pushed). The tip holds only this run's images;
#    earlier images stay in the history and the comment refers to each entry's shots_commit.
remote="https://x-access-token:${GITHUB_TOKEN}@github.com/${REPO}.git"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
repo_dir="$work/shots"

push_shots() {
  rm -rf "$repo_dir"
  if git ls-remote --exit-code "$remote" "refs/heads/${SHOTS_BRANCH}" >/dev/null 2>&1; then
    # Full clone: a push from a shallow clone can be refused. The branch is small and deleted with the pull request.
    git clone --quiet --single-branch --branch "$SHOTS_BRANCH" "$remote" "$repo_dir"
  else
    mkdir -p "$repo_dir"
    git -C "$repo_dir" init --quiet -b "$SHOTS_BRANCH"
  fi
  git -C "$repo_dir" config user.name "github-actions[bot]"
  git -C "$repo_dir" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  # The tip holds only this run's images
  find "$repo_dir" -mindepth 1 -maxdepth 1 -not -name .git -exec rm -rf {} +
  mkdir -p "$repo_dir/$HEAD_SHA"
  cp "$SHOTS_DIR"/*.png "$repo_dir/$HEAD_SHA/"
  git -C "$repo_dir" add -A
  git -C "$repo_dir" commit --quiet -m "screenshots for ${HEAD_SHA}"
  git -C "$repo_dir" push --quiet "$remote" "HEAD:refs/heads/${SHOTS_BRANCH}"
}

# Runs of one pull request are serialized by the workflow's concurrency; retry once from a fresh clone anyway
if ! push_shots; then
  echo "The push of the screenshots was refused (a parallel run?); retrying once in 5 seconds" >&2
  sleep 5
  push_shots
fi
shots_commit=$(git -C "$repo_dir" rev-parse HEAD)
echo "pushed screenshots to ${SHOTS_BRANCH} @ ${shots_commit} (for ${HEAD_SHA})"

# Record the commit that stores this run's images (the comment's URLs point to it)
entries=$(jq -c --arg c "$shots_commit" '.[0].shots_commit = $c' <<<"$entries")

# 5. Build the comment body and create or update the comment
body_file="$work/body.md"
render_body "$entries" >"$body_file"
payload="$work/payload.json"
jq -n --rawfile b "$body_file" '{body: $b}' >"$payload"
if [ -n "$comment_id" ]; then
  gh api --method PATCH "repos/${REPO}/issues/comments/${comment_id}" --input "$payload" >/dev/null
  echo "updated comment ${comment_id}"
else
  gh api --method POST "repos/${REPO}/issues/${PR_NUMBER}/comments" --input "$payload" >/dev/null
  echo "created new comment"
fi
