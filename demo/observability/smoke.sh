#!/bin/sh
set -eu

deadline=$(($(date +%s) + ${KRABKA_SMOKE_TIMEOUT_SECONDS:-300}))
pending=${KRABKA_SMOKE_TARGETS:-"ready metrics logs traces cross-signal profiles gres demo-produce demo-stream demo-consume"}
# The `ready` target checks the live `/ready` endpoint in addition to each
# role's minimal executable Docker healthcheck.
ready_services=${KRABKA_SMOKE_READY_SERVICES:-"metrics-distributor metrics-block-builder metrics-compactor metrics-querier traces-distributor traces-block-builder traces-metrics-generator traces-querier logs-distributor logs-block-builder logs-querier profiles-distributor profiles-block-builder profiles-querier"}
not_ready=""
for target in $pending; do
  case "$target" in
    ready|metrics|logs|traces|cross-signal|profiles|gres|demo-produce|demo-stream|demo-consume) ;;
    *) echo "unknown smoke target: $target" >&2; exit 1 ;;
  esac
done

check() {
  case "$1" in
    ready)
      not_ready=""
      for service in $ready_services; do
        docker compose exec -T demo-produce curl -fsS -m 5 "http://$service:9404/ready" >/dev/null || not_ready="$not_ready $service"
      done
      [ -z "$not_ready" ]
      ;;
    metrics) curl -fsS -H 'X-Scope-OrgID: demo' 'http://localhost:9090/api/v1/query?query=krabka_broker_api_requests_total' | jq -e '.status == "success" and (.data.result | length > 0)' >/dev/null ;;
    logs) curl -fsS -H 'X-Scope-OrgID: demo' 'http://localhost:3100/loki/api/v1/labels' | jq -e '.status == "success" and (.data | length > 0)' >/dev/null ;;
    traces) curl -fsS -H 'X-Scope-OrgID: demo' --get 'http://localhost:3200/api/search' --data-urlencode 'start=0' --data-urlencode "end=$(date +%s)" --data-urlencode 'q={ resource.service.name != "" }' | jq -e '.traces | length > 0' >/dev/null ;;
    cross-signal) curl -fsS -H 'X-Scope-OrgID: demo' --get 'http://localhost:3200/api/search' --data-urlencode 'start=0' --data-urlencode "end=$(date +%s)" --data-urlencode 'q={ resource.service.name = "demo-produce" && name = "orders publish" } && { resource.service.name = "demo-consume" && name = "orders process" }' | jq -e '.traces | length > 0' >/dev/null ;;
    profiles) curl -fsS -H 'X-Scope-OrgID: demo' -H 'content-type: application/json' -d '{}' 'http://localhost:4040/querier.v1.QuerierService/ProfileTypes' | jq -e '(.profileTypes // .profile_types) | length > 0' >/dev/null ;;
    gres) curl -fsS -H 'X-Scope-OrgID: demo' --get 'http://localhost:3200/api/search' --data-urlencode 'start=0' --data-urlencode "end=$(date +%s)" --data-urlencode 'q={ resource.service.name = "gres" && span.db.system.name = "postgresql" }' | jq -e '.traces | length > 0' >/dev/null ;;
    demo-*) docker compose exec -T "$1" curl -fsS http://localhost:9404/metrics | grep -q 'krabka_demo_' ;;
    *) return 1 ;;
  esac
}

# The service check runs in each pass. An exit or a restart stops the smoke test
# at once. A service that is not healthy yet keeps the test waiting until the
# deadline.
check_services() {
  services_status=0
  services_report=$("$(dirname "$0")/check-services.sh") || services_status=$?
  case "$services_status" in
    0) services_ready=true ;;
    2) services_ready=false ;;
    *)
      printf '%s\n' "$services_report" >&2
      echo "failed: services" >&2
      exit 1
      ;;
  esac
}

services_ready=false
while [ "$(date +%s)" -lt "$deadline" ]; do
  check_services
  next=""
  for signal in $pending; do
    if check "$signal" 2>/dev/null; then
      echo "ok $signal"
    else
      next="$next $signal"
    fi
  done
  pending=${next# }
  if [ -z "$pending" ]; then
    check_services
    if [ "$services_ready" = true ]; then
      echo "ok services"
      break
    fi
  fi
  sleep 5
done

if [ -n "$pending" ] || [ "$services_ready" != true ]; then
  [ "$services_ready" = true ] || { printf '%s\n' "${services_report:-}" >&2; pending="$pending services"; }
  [ -z "$not_ready" ] || echo "not ready:$not_ready" >&2
  echo "failed: ${pending# }" >&2
  exit 1
fi
