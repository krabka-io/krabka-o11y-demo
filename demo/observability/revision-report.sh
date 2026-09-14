#!/bin/sh
set -eu

out="${1:-revision-report.txt}"
tmp="${out}.tmp"
{
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "compose_project=${COMPOSE_PROJECT_NAME:-krabka-observability-demo}"
  docker compose images --format json | jq -r '
    if type == "array" then .[] else . end |
    [.Service, .Repository, .Tag, .ID] | @tsv' | sort
} >"${tmp}"
mv "${tmp}" "${out}"
cat "${out}"
