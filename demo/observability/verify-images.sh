#!/bin/sh
set -eu

platform="${KRABKA_PLATFORM:-linux/$(uname -m | sed 's/x86_64/amd64/; s/aarch64/arm64/')}"
status=0
images="$(docker compose config --images | sort -u)"
for image in ${images}; do
  case "${image}" in
    *@sha256:*) ;;
    *) echo "not digest pinned: ${image}" >&2; status=1; continue ;;
  esac
  platforms="$(docker buildx imagetools inspect "${image}" --raw | jq -r '
    if .manifests then .manifests[].platform | .os + "/" + .architecture
    else empty end')"
  if [ -n "${platforms}" ] && ! printf '%s\n' "${platforms}" | grep -Fxq "${platform}"; then
    echo "${image} does not publish ${platform}" >&2
    status=1
  else
    printf '%s\t%s\n' "${platform}" "${image}"
  fi
done
exit "${status}"
