#!/bin/sh
set -eu

deadline=$(($(date +%s) + ${KRABKA_SMOKE_TIMEOUT_SECONDS:-300}))
pending="metrics logs traces cross-signal profiles demo-produce demo-stream demo-consume"

check() {
  case "$1" in
    metrics) curl -fsS -H 'X-Scope-OrgID: demo' 'http://localhost:9090/api/v1/query?query=krabka_broker_api_requests_total' | jq -e '.status == "success" and (.data.result | length > 0)' >/dev/null ;;
    logs) curl -fsS -H 'X-Scope-OrgID: demo' 'http://localhost:3100/loki/api/v1/labels' | jq -e '.status == "success" and (.data | length > 0)' >/dev/null ;;
    traces) curl -fsS -H 'X-Scope-OrgID: demo' --get 'http://localhost:3200/api/search' --data-urlencode 'q={ resource.service.name != "" }' | jq -e '.traces | length > 0' >/dev/null ;;
    cross-signal) curl -fsS -H 'X-Scope-OrgID: demo' --get 'http://localhost:3200/api/search' --data-urlencode 'q={ resource.service.name = "demo-produce" && name = "produce_order" } && { resource.service.name = "demo-consume" && name = "process_order" }' | jq -e '.traces | length > 0' >/dev/null ;;
    profiles) curl -fsS -H 'X-Scope-OrgID: demo' -H 'content-type: application/json' -d '{}' 'http://localhost:4040/querier.v1.QuerierService/ProfileTypes' | jq -e '(.profileTypes // .profile_types) | length > 0' >/dev/null ;;
    demo-*) docker compose exec -T "$1" curl -fsS http://localhost:9404/metrics | grep -Eq '(krabka|crabka)_demo_' ;;
  esac
}

while [ -n "$pending" ] && [ "$(date +%s)" -lt "$deadline" ]; do
  next=""
  for signal in $pending; do
    if check "$signal" 2>/dev/null; then
      echo "ok $signal"
    else
      next="$next $signal"
    fi
  done
  pending=${next# }
  [ -z "$pending" ] || sleep 5
done

if [ -n "$pending" ]; then
  echo "failed: $pending" >&2
  exit 1
fi
