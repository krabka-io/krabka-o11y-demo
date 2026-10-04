#!/bin/sh
set -eu

platform="${KRABKA_PLATFORM:-linux/$(uname -m | sed 's/x86_64/amd64/; s/aarch64/arm64/')}"
status=0
images="$(docker compose config --images | sort -u)"
for image in ${images}; do
  case "${image}" in
    "krabka-io/krabka-o11y-demo:sha-${KRABKA_REVISION:-}")
      test -n "${KRABKA_REVISION:-}"
      actual="$(docker image inspect "${image}" --format '{{.Os}}/{{.Architecture}}')"
      if [ "${actual}" != "${platform}" ]; then
        echo "${image} has platform ${actual}, expected ${platform}" >&2
        status=1
      else
        printf '%s\t%s\n' "${platform}" "${image}"
      fi
      continue
      ;;
    ghcr.io/krabka-io/krabka-o11y-demo:v*)
      digest="$(docker buildx imagetools inspect "${image}" --format '{{.Manifest.Digest}}')"
      image="ghcr.io/krabka-io/krabka-o11y-demo@${digest}"
      if [ -n "${GITHUB_ENV:-}" ]; then
        printf 'KRABKA_DEMO_IMAGE=%s\n' "${image}" >>"${GITHUB_ENV}"
      fi
      ;;
  esac
  case "${image}" in
    *@sha256:*) ;;
    *) echo "not digest pinned: ${image}" >&2; status=1; continue ;;
  esac
  inspection="$(docker buildx imagetools inspect "${image}" --format '{{json .}}')"
  platforms="$(printf '%s\n' "${inspection}" | jq -r '
    if .manifest.manifests then
      .manifest.manifests[].platform | .os + "/" + .architecture
    elif .image.os and .image.architecture then
      .image.os + "/" + .image.architecture
    else
      empty
    end')"
  if [ -z "${platforms}" ]; then
    echo "cannot determine the published platform for ${image}" >&2
    status=1
  elif ! printf '%s\n' "${platforms}" | grep -Fxq "${platform}"; then
    echo "${image} does not publish ${platform}" >&2
    status=1
  else
    printf '%s\t%s\n' "${platform}" "${image}"
  fi
done
exit "${status}"
