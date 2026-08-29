#!/usr/bin/env bash
# Create the roadmap project board and put every roadmap issue on it.
#
# A GitHub Projects v2 board exists only behind the GraphQL API, which the REST
# API cannot reach. This script drives it through the `gh` CLI instead.
#
# Prerequisites:
#   gh auth login --scopes project,read:project,repo
#
# The script is idempotent. If the board already exists it reuses it, and
# `gh project item-add` does not duplicate an item that is already on the board.

set -euo pipefail

OWNER="${OWNER:-krabka-io}"
REPO="${REPO:-krabka-o11y-demo}"
TITLE="${TITLE:-krabka-o11y-demo roadmap}"

command -v gh >/dev/null || { echo "gh is not installed: https://cli.github.com" >&2; exit 1; }
command -v jq >/dev/null || { echo "jq is not installed" >&2; exit 1; }

if ! gh auth status >/dev/null 2>&1; then
  echo "gh is not authenticated. Run: gh auth login --scopes project,read:project,repo" >&2
  exit 1
fi

if ! gh auth status 2>&1 | grep -q 'project'; then
  echo "The gh token has no project scope. Run: gh auth refresh -s project,read:project" >&2
  exit 1
fi

echo "==> Looking for an existing board named '${TITLE}'"
number="$(gh project list --owner "${OWNER}" --format json \
  | jq -r --arg t "${TITLE}" '.projects[] | select(.title == $t) | .number' | head -1)"

if [ -z "${number}" ]; then
  echo "==> Creating the board"
  number="$(gh project create --owner "${OWNER}" --title "${TITLE}" --format json | jq -r '.number')"
  echo "    created project #${number}"
else
  echo "    reusing project #${number}"
fi

gh project edit "${number}" --owner "${OWNER}" \
  --readme "$(printf 'The roadmap for %s/%s.\n\nThe issues are the source of truth. docs/ROADMAP.md records what they say.\nGroup by Milestone to see the sequence, or by Labels to see the areas.\n' "${OWNER}" "${REPO}")" \
  >/dev/null

echo "==> Adding every roadmap issue"
added=0
while read -r url; do
  [ -n "${url}" ] || continue
  gh project item-add "${number}" --owner "${OWNER}" --url "${url}" >/dev/null
  added=$((added + 1))
  printf '\r    %d issues added' "${added}"
done < <(
  gh issue list --repo "${OWNER}/${REPO}" --state open --limit 200 \
    --label type:epic --json url --jq '.[].url'
  gh issue list --repo "${OWNER}/${REPO}" --state open --limit 200 \
    --json url,labels \
    --jq '.[] | select([.labels[].name] | any(startswith("area:"))) | .url'
)
printf '\n'

echo
echo "Board ready: https://github.com/orgs/${OWNER}/projects/${number}"
echo
echo "Projects v2 exposes Milestone, Labels and Assignees as built-in fields, so the"
echo "board needs no custom Area or Size field. In the board view:"
echo "  - Group by Milestone for the M1 to M4 sequence."
echo "  - Group by Labels, or filter on 'label:area:stack', for one area at a time."
echo "  - Filter on 'label:\"good first issue\"' for the approachable work."
