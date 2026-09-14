# Release manifest

Each `v*` tag publishes `ghcr.io/krabka-io/krabka-o11y-demo` as an OCI index
containing Linux AMD64 and ARM64 manifests. The image workflow records and
attests its digest. The release qualification workflow resolves the tag to that
immutable digest, verifies both architectures, runs a cold four-signal and Gres
smoke test, performs a retained-state restart and rollback drill, then uploads
checksummed evidence.

A GitHub release should include the immutable demo image from
`candidate-image.txt`, the component digests from `revisions.txt`, and the
`SHA256SUMS` file. Operators should read the [upgrade and rollback procedure](operator-guide.md#shutdown-and-upgrades)
before changing a running stack.
