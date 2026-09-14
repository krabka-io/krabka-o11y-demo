# Demo application design

The binary owns process configuration and connections. The library owns pure
order policy, pipeline sequencing, header construction, bounded metric labels,
and the Prometheus registry. This boundary keeps error routing, latency
injection, and trace propagation testable without a broker.

Every order begins a producer root span. Its `traceparent`, region, and customer
tier travel as Kafka headers. The consumer makes `process_order` a remote child,
records every order dimension, then emits four child stage spans. Missing
values, deserialization failures, and exhausted producer sends set error status
and increment `krabka_demo_pipeline_errors_total{kind=...}`.

Metric labels are fixed enums or bounded business dimensions from the
generator. Error text, order IDs, and trace IDs never become labels. Shutdown
waits for SIGINT or SIGTERM, closes consumer/Streams group membership, then
flushes telemetry.
