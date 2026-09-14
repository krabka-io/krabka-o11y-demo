# Contributing

Install Rust from `rust-toolchain.toml`, Bazelisk, Cargo Nextest, Cargo Deny,
Docker, and Docker Compose. Before opening a pull request, run:

```sh
cargo build --locked
cargo nextest run --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo deny check
python3 tools/check-repository.py
bazel test //...
```

Changes to `demo/observability` should also pass `docker compose config` and
`./smoke.sh`. Keep external image references immutable and GitHub Actions pinned
to full commit IDs. Dependency updates must include `Cargo.lock` and
`MODULE.bazel.lock` when either resolver changes.

Pull requests need a focused description, validation evidence, and links to
the issues they close. A maintainer review is required for dependency, release,
security, and observability contract changes.
