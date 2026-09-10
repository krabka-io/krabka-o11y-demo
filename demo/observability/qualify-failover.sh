#!/bin/sh
set -eu

artifact_dir=${KRABKA_QUALIFICATION_ARTIFACT_DIR:-qualification-artifacts/$(date -u +%Y%m%dT%H%M%SZ)}
mkdir -p "$artifact_dir"
started=$(date +%s)
{
  printf 'krabka-o11y-demo '
  git -C ../.. rev-parse HEAD
  grep 'git = "https://github.com/krabka-io/' ../../Cargo.toml | sort -u
} >"$artifact_dir/source-revisions.txt"

finish() {
  status=$?
  trap - EXIT
  docker compose logs --no-color >"$artifact_dir/compose.log" 2>&1 || true
  docker compose config --images >"$artifact_dir/images.txt" 2>&1 || true
  docker compose config >"$artifact_dir/compose.yaml" 2>&1 || true
  finished=$(date +%s)
  printf '{"started_at_epoch":%s,"finished_at_epoch":%s,"recovery_seconds":%s,"exit_status":%s}\n' \
    "$started" "$finished" "$((finished - started))" "$status" >"$artifact_dir/result.json"
  (cd "$artifact_dir" && sha256sum ./* >SHA256SUMS)
  exit "$status"
}
trap finish EXIT

ledger() {
  for group in krabka-metrics-compactor krabka-traces-block-builder krabka-profiles-block-builder krabka-observability-compactor; do
    docker compose run --rm --no-deps --entrypoint /opt/kafka/bin/kafka-consumer-groups.sh topic-setup \
      --bootstrap-server broker:9092 --describe --group "$group" || true
  done
}

offset_sum() {
  awk '$4 ~ /^[0-9]+$/ {sum += $4; seen = 1} END {if (!seen) exit 1; print sum}' "$1"
}

seed_qualification_signals() {
  now=$(date +%s%N)
  end=$((now + 1000000))
  qualification_trace_id=$(tr -d '-' </proc/sys/kernel/random/uuid)
  qualification_root_span_id=$(printf '%s' "$qualification_trace_id" | cut -c 1-16)
  qualification_child_span_id=$(printf '%s' "$qualification_trace_id" | cut -c 17-32)
  printf '%s\n' "$qualification_trace_id" >"$artifact_dir/seed-trace-id.txt"
  cat >"$artifact_dir/seed-traces.json" <<EOF
{"resourceSpans":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"m20-qualification-producer"}}]},"scopeSpans":[{"spans":[{"traceId":"$qualification_trace_id","spanId":"$qualification_root_span_id","name":"m20_qualification_root","kind":3,"startTimeUnixNano":"$now","endTimeUnixNano":"$end"}]}]},{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"m20-qualification-consumer"}}]},"scopeSpans":[{"spans":[{"traceId":"$qualification_trace_id","spanId":"$qualification_child_span_id","parentSpanId":"$qualification_root_span_id","name":"m20_qualification_recovery_child","kind":4,"startTimeUnixNano":"$now","endTimeUnixNano":"$end"}]}]}]}
EOF
  cat >"$artifact_dir/seed-logs.json" <<EOF
{"resourceLogs":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"m20-qualification"}}]},"scopeLogs":[{"logRecords":[{"timeUnixNano":"$now","severityText":"INFO","body":{"stringValue":"M20 observability recovery qualification"}}]}]}]}
EOF
  docker compose exec -T demo-produce curl -fsS -H 'content-type: application/json' \
    --data-binary @- http://alloy:4318/v1/traces <"$artifact_dir/seed-traces.json" \
    >"$artifact_dir/seed-traces-response.json"
  docker compose exec -T demo-produce curl -fsS -H 'content-type: application/json' \
    --data-binary @- http://alloy:4318/v1/logs <"$artifact_dir/seed-logs.json" \
    >"$artifact_dir/seed-logs-response.json"
}

capture_queries() {
  phase=$1
  curl -fsS -H 'X-Scope-OrgID: demo' 'http://localhost:9090/api/v1/query?query=krabka_broker_api_requests_total' >"$artifact_dir/metrics-$phase.json"
  curl -fsS -H 'X-Scope-OrgID: demo' 'http://localhost:3100/loki/api/v1/labels' >"$artifact_dir/logs-$phase.json"
  log_deadline=$(($(date +%s) + ${KRABKA_SIGNAL_TIMEOUT_SECONDS:-120}))
  while ! curl -fsS -H 'X-Scope-OrgID: demo' --get \
    'http://localhost:3100/loki/api/v1/query_range' \
    --data-urlencode 'query={service_name="m20-qualification"} |= "M20 observability recovery qualification"' \
    >"$artifact_dir/seed-log-$phase.json" 2>/dev/null \
    || ! jq -e '.status == "success" and (.data.result | length > 0)' \
      "$artifact_dir/seed-log-$phase.json" >/dev/null; do
    [ "$(date +%s)" -lt "$log_deadline" ] || { echo "seeded qualification log not queryable during $phase" >&2; exit 1; }
    sleep 5
  done
  trace_deadline=$(($(date +%s) + ${KRABKA_SIGNAL_TIMEOUT_SECONDS:-120}))
  while ! curl -fsS -H 'X-Scope-OrgID: demo' \
    "http://localhost:3200/api/traces/$qualification_trace_id" \
    >"$artifact_dir/seed-trace-$phase.pb" 2>/dev/null; do
    [ "$(date +%s)" -lt "$trace_deadline" ] || { echo "seeded qualification trace not queryable during $phase" >&2; exit 1; }
    sleep 5
  done
  [ -s "$artifact_dir/seed-trace-$phase.pb" ]
  curl -fsS -H 'X-Scope-OrgID: demo' --get 'http://localhost:3200/api/search' --data-urlencode 'q={ resource.service.name != "" }' >"$artifact_dir/traces-$phase.json"
  curl -fsS -H 'X-Scope-OrgID: demo' -H 'content-type: application/json' -d '{}' 'http://localhost:4040/querier.v1.QuerierService/ProfileTypes' >"$artifact_dir/profiles-$phase.json"
  for service in demo-produce demo-stream demo-consume; do
    docker compose exec -T "$service" curl -fsS http://localhost:9404/metrics >"$artifact_dir/$service-$phase.prom"
  done
}

seed_qualification_signals
./smoke.sh | tee "$artifact_dir/before-smoke.log"
capture_queries before
ledger >"$artifact_dir/offsets-before.txt" 2>&1

docker compose kill -s KILL traces-block-builder
docker compose restart broker
docker compose up -d traces-block-builder
./smoke.sh | tee "$artifact_dir/after-recovery-smoke.log"
capture_queries after
ledger >"$artifact_dir/offsets-after.txt" 2>&1
before_offset=$(offset_sum "$artifact_dir/offsets-before.txt")
after_offset=$(offset_sum "$artifact_dir/offsets-after.txt")
[ "$after_offset" -ge "$before_offset" ]
printf 'before_offset_sum=%s\nafter_offset_sum=%s\n' "$before_offset" "$after_offset" >"$artifact_dir/reconciliation.txt"

docker compose stop traces-querier
alert_deadline=$(($(date +%s) + ${KRABKA_ALERT_TIMEOUT_SECONDS:-180}))
while ! curl -fsS http://localhost:3000/api/alertmanager/grafana/api/v2/alerts 2>/dev/null | jq -e 'any(.[]; .labels.alertname == "Observability service down")' >/dev/null; do
  [ "$(date +%s)" -lt "$alert_deadline" ] || { echo "service-down alert did not fire" >&2; exit 1; }
  sleep 5
done
curl -fsS http://localhost:3000/api/alertmanager/grafana/api/v2/alerts >"$artifact_dir/alerts-firing.json"

docker compose start traces-querier
resolve_deadline=$(($(date +%s) + ${KRABKA_ALERT_TIMEOUT_SECONDS:-180}))
while curl -fsS http://localhost:3000/api/alertmanager/grafana/api/v2/alerts 2>/dev/null | jq -e 'any(.[]; .labels.alertname == "Observability service down")' >/dev/null; do
  [ "$(date +%s)" -lt "$resolve_deadline" ] || { echo "service-down alert did not resolve" >&2; exit 1; }
  sleep 5
done
curl -fsS http://localhost:3000/api/alertmanager/grafana/api/v2/alerts >"$artifact_dir/alerts-resolved.json"

echo "$artifact_dir"
