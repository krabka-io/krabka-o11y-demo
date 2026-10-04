#!/bin/sh
set -eu

# Search by span name, then check each span's resource and parent in the full
# trace. The service-name search index describes a trace's root service.
trace_ids=$(curl -fsS -m 10 -H 'X-Scope-OrgID: demo' --get \
  'http://localhost:3200/api/search' --data-urlencode 'start=0' \
  --data-urlencode "end=$(date +%s)" \
  --data-urlencode 'q={ name = "orders publish" } && { name = "orders process" }' |
  jq -r '.traces[]?.traceID')

for trace_id in $trace_ids; do
  if curl -fsS -m 10 -H 'X-Scope-OrgID: demo' -H 'Accept: application/json' \
    "http://localhost:3200/api/traces/$trace_id" | jq -e '
      [.trace.resourceSpans[]
       | select(any(.resource.attributes[]; .key == "service.name" and .value.stringValue == "demo-produce"))
       | .scopeSpans[].spans[] | select(.name == "orders publish")
       | {spanId, traceId}] as $publishers
      | any(.trace.resourceSpans[]
        | select(any(.resource.attributes[]; .key == "service.name" and .value.stringValue == "demo-consume"))
        | .scopeSpans[].spans[] | select(.name == "orders process");
          . as $consumer | any($publishers[];
            .spanId == $consumer.parentSpanId and .traceId == $consumer.traceId))
    ' >/dev/null; then
    exit 0
  fi
done
exit 1
