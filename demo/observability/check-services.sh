#!/bin/sh
# Checks the state of each service in the Compose project.
#
# A long-running service must run, must have a restart count of zero, and must
# be healthy if it has a healthcheck. A one-shot service has `restart: "no"`,
# and it must exit with code 0.
#
# KRABKA_SMOKE_SERVICES names the long-running services that must have a
# container. The default is each service in the Compose file that does not have
# `restart: "no"`. The script also checks each other container of the project.
#
# The script writes one line for each service that is not in its expected
# state. The line names the service, the command, the image digest, the exit
# code, the health and the restart count.
#
# Exit status:
#   0  each service is in its expected state
#   1  a service exited, restarted, or has no container
#   2  a service is still starting, or it is not healthy yet
set -eu

config=$(docker compose config --format json)
oneshots=$(printf '%s' "$config" | jq -r '.services | to_entries[] | select(.value.restart == "no") | .key')
expected=${KRABKA_SMOKE_SERVICES:-$(printf '%s' "$config" | jq -r '.services | to_entries[] | select(.value.restart != "no") | .key')}

result=0

is_oneshot() {
  printf '%s\n' "$oneshots" | grep -qx "$1"
}

image_digest() {
  case "$1" in
    *@sha256:*) printf '%s\n' "${1##*@}"; return ;;
  esac
  repo_digest=$(docker image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' "$2" 2>/dev/null </dev/null | head -n 1)
  if [ -n "$repo_digest" ]; then
    printf '%s\n' "${repo_digest##*@}"
  else
    printf '%s\n' "$2"
  fi
}

# A container that the restart policy started again reports exit code 0 while
# it runs. The last `die` event keeps the exit code of the last stop.
last_exit_code() {
  code=$(docker events --since "$2" --until "$(date +%s)" --filter "container=$1" --filter event=die \
    --format '{{index .Actor.Attributes "exitCode"}}' 2>/dev/null </dev/null | tail -n 1)
  printf '%s\n' "${code:-$3}"
}

report() {
  verdict=$1 service=$2 reason=$3 state=$4 exit_code=$5 health=$6 restarts=$7 digest=$8 command=$9
  printf '%s service=%s reason="%s" state=%s exit_code=%s health=%s restarts=%s digest=%s command="%s"\n' \
    "$verdict" "$service" "$reason" "$state" "$exit_code" "$health" "$restarts" "$digest" "$command"
  if [ "$verdict" = FAILED ]; then
    result=1
  elif [ "$result" -eq 0 ]; then
    result=2
  fi
}

containers=$(docker compose ps --all --quiet)
inspected=""
if [ -n "$containers" ]; then
  inspected=$(printf '%s\n' "$containers" | xargs docker inspect | jq -r '.[] | [
      .Id,
      .Config.Labels["com.docker.compose.service"],
      .State.Status,
      (.State.ExitCode | tostring),
      (.State.Health.Status // "none"),
      (.RestartCount | tostring),
      .Created,
      .Config.Image,
      .Image,
      ([.Path] + (.Args // []) | join(" ") | gsub("\\s+"; " "))
    ] | @tsv')
fi

seen=""
tab=$(printf '\t')
while IFS="$tab" read -r id service state exit_code health restarts created image image_id command; do
  [ -n "$id" ] || continue
  seen="$seen $service"
  digest=$(image_digest "$image" "$image_id")
  if is_oneshot "$service"; then
    case "$state" in
      exited)
        [ "$exit_code" -eq 0 ] ||
          report FAILED "$service" "one-shot service exited with a non-zero code" "$state" "$exit_code" "$health" "$restarts" "$digest" "$command"
        ;;
      created|running)
        report PENDING "$service" "one-shot service has not finished" "$state" "$exit_code" "$health" "$restarts" "$digest" "$command"
        ;;
      *)
        report FAILED "$service" "one-shot service is in an unexpected state" "$state" "$exit_code" "$health" "$restarts" "$digest" "$command"
        ;;
    esac
  elif [ "$restarts" -gt 0 ]; then
    report FAILED "$service" "long-running service restarted" "$state" "$(last_exit_code "$id" "$created" "$exit_code")" "$health" "$restarts" "$digest" "$command"
  elif [ "$state" != running ]; then
    report FAILED "$service" "long-running service is not running" "$state" "$exit_code" "$health" "$restarts" "$digest" "$command"
  elif [ "$health" != none ] && [ "$health" != healthy ]; then
    report PENDING "$service" "long-running service is not healthy" "$state" "$exit_code" "$health" "$restarts" "$digest" "$command"
  fi
done <<EOF
$inspected
EOF

for service in $expected; do
  case " $seen " in
    *" $service "*) continue ;;
  esac
  image=$(printf '%s' "$config" | jq -r --arg service "$service" '.services[$service].image // "unknown"')
  command=$(printf '%s' "$config" | jq -r --arg service "$service" '(.services[$service].entrypoint // []) + (.services[$service].command // []) | join(" ") | gsub("\\s+"; " ")')
  report FAILED "$service" "expected service has no container" missing none none 0 "$(image_digest "$image" "$image")" "$command"
done

exit "$result"
