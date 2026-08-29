# krabka-o11y-demo

The runnable [krabka](https://github.com/krabka-io) observability demo: an
instrumented Kafka-Streams application, and the full-signal Grafana environment
it is watched through.

One `docker compose up` starts Grafana over krabka's four observability
backends — metrics, traces, logs and profiles — with a broker underneath acting
as event bus, write-ahead log and self-observed subject at once. The application
in this repository is what produces the traffic they show.

## The application

`observability-demo-app` is an orders pipeline with three roles, each a separate
process in the compose stack:

| Role | What it does |
| --- | --- |
| `produce` | Generates orders and injects the current trace context into each record's headers |
| `stream` | The Kafka-Streams aggregation, keyed by customer, over a state store |
| `consume` | Extracts the propagated context, so its stages join the producer's trace |

The point of the three is the seam between them. A single distributed trace runs
from `produce`, through the broker's Produce and Fetch paths, into `stream`'s
rebalance and state-store work, and out through `consume` — which is what the
demo exists to make visible.

## The stack

[`demo/observability/`](demo/observability/) holds the compose file, the Grafana
dashboards, the alerting rules, the datasource wiring and the Alloy collector
config. Its own [README](demo/observability/README.md) covers running it.

The krabka service binaries come from a prebuilt image, so the stack needs no
local build:

```bash
cd demo/observability && docker compose up -d
```

Grafana is then at <http://localhost:3000>.

That image is built and published from
[`robot-head/crabka`](https://github.com/robot-head/crabka), which is the only
tree holding every binary it bundles. This repository builds the demo
application from source and consumes the image as a dependency.

## Layering

This is the topmost repository in the krabka stack, and depends on four
siblings, each pinned by revision in
[`Cargo.toml`](Cargo.toml)'s `[patch.crates-io]`:

| Repository | What it supplies |
| --- | --- |
| [`krabka-protocol`](https://github.com/krabka-io/krabka-protocol) | Wire types and units |
| [`krabka-client-rs`](https://github.com/krabka-io/krabka-client-rs) | The producer, consumer and admin clients |
| [`krabka-streams-rs`](https://github.com/krabka-io/krabka-streams-rs) | The Streams runtime and schema serdes |
| [`krabka-broker`](https://github.com/krabka-io/krabka-broker) | Telemetry, which is how the app exports its own signals |

## Roadmap

[`docs/ROADMAP.md`](docs/ROADMAP.md) records the planned work: four milestones, six
epics and the issues under them. The issues in this repository are the source of
truth, and the document records what they say.

## Build

```bash
cargo test --workspace
```

```bash
bazel test //...
```

Both are supported and both are gated in CI. Cargo stays the dependency source
of truth; Bazel reads the same `Cargo.toml` and `Cargo.lock`, and additionally
runs the mutation sweep.

To run a role against a stack that is already up:

```bash
bazel run //:demo -- --role produce
```
