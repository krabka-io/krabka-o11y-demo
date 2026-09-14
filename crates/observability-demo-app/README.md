# Observability demo application

This crate supplies the three workload roles used by the Krabka observability
demo: `produce`, `stream`, and `consume`. The producer creates deterministic
protobuf orders and injects W3C trace context. The Streams role maintains
category counts. The consumer continues each trace and executes the validate,
enrich, fraud-check, and fulfill stages.

```rust
use observability_demo_app::{classify_outcome, order_at};

let order = order_at(1);
assert_eq!(classify_outcome(&order), "fulfilled");
```

See [DESIGN.md](DESIGN.md) for boundaries and invariants. The complete runtime
instructions live in the [operator guide](../../docs/operator-guide.md).
