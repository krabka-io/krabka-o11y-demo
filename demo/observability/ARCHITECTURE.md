# Krabka observability demo architecture

The demo uses one Krabka broker for application events and as the write-ahead
log (WAL) for all four observability signals. Grafana Alloy receives or scrapes
telemetry, sends each signal to its distributor, and also collects telemetry
from the observability stack itself. Compactors and block builders move WAL data
into signal-specific RustFS buckets. Queriers combine recent WAL data with those
durable blocks for Grafana.

![Rendered Krabka observability architecture](../../docs/images/architecture.svg)

```mermaid
flowchart LR
    user([User])

    subgraph app[Demo application]
        produce[demo-produce]
        stream[demo-stream]
        consume[demo-consume]
        registry[Schema registry]
        gres[gres SQL engine]
        workload[gres-workload]
    end

    broker[(Krabka broker<br/>application topics + telemetry WAL)]

    subgraph collect[Collection]
        alloy[Grafana Alloy<br/>OTLP receiver + scrapers]
        cadvisor[cAdvisor]
        docker[Docker container logs]
    end

    subgraph ingest[Signal ingestion]
        md[Metrics distributor<br/>Prometheus remote write]
        td[Traces distributor<br/>OTLP]
        ld[Logs distributor<br/>Loki push + OTLP]
        pd[Profiles distributor<br/>Pyroscope push]
    end

    subgraph process[WAL processing]
        mc[Metrics compactor]
        tb[Traces block builder]
        tg[Trace metrics generator]
        lc[Logs compactor]
        pb[Profiles block builder]
    end

    rustfs[(RustFS object storage<br/>metrics · traces · logs · profiles)]

    subgraph query[Query APIs]
        mq[Metrics querier<br/>Prometheus API :9090]
        tq[Traces querier<br/>Tempo API :3200]
        lq[Logs querier<br/>Loki API :3100]
        pq[Profiles querier<br/>Pyroscope API :4040]
    end

    grafana[Grafana :3000]

    produce -->|orders| broker
    broker -->|orders| stream
    stream -->|order-counts| broker
    broker -->|orders and counts| consume
    produce & stream & consume <-->|schemas| registry
    registry -->|schema topic| broker
    workload -->|PostgreSQL :5433| gres
    gres -->|SQL WAL| broker

    produce & stream & consume & registry & gres -->|OTLP| alloy
    broker -.->|OTLP self-telemetry| alloy
    cadvisor -->|container metrics| alloy
    docker -->|stdout logs| alloy

    alloy -->|remote write| md
    alloy -->|OTLP traces| td
    alloy -->|Loki push / OTLP logs| ld
    alloy -->|pprof scrape| pd

    md -->|metrics WAL| broker
    td -->|traces WAL| broker
    ld -->|logs WAL| broker
    pd -->|profiles WAL| broker

    broker -->|metrics WAL| mc
    broker -->|traces WAL| tb
    broker -->|traces WAL| tg
    broker -->|logs WAL| lc
    broker -->|profiles WAL| pb
    tg -->|derived metrics| md

    mc -->|metric blocks| rustfs
    tb -->|trace blocks| rustfs
    lc -->|log blocks + index| rustfs
    pb -->|profile blocks| rustfs

    broker -.->|recent metrics| mq
    broker -.->|recent logs| lq
    broker -.->|recent profiles| pq
    rustfs -->|metric blocks| mq
    rustfs -->|trace blocks| tq
    rustfs -->|log blocks| lq
    rustfs -->|profile blocks| pq

    user -->|dashboards and Explore| grafana
    grafana -->|PromQL| mq
    grafana -->|TraceQL| tq
    grafana -->|LogQL| lq
    grafana -->|profile queries| pq

    md & td & ld & pd & mc & tb & tg & lc & pb & mq & tq & lq & pq -.->|OTLP self-telemetry| alloy

    classDef core fill:#f2cc60,stroke:#8a6d00,color:#111;
    classDef storage fill:#73bf69,stroke:#2f6f2f,color:#111;
    classDef observe fill:#5794f2,stroke:#1f4f8f,color:#fff;
    classDef app fill:#ff9830,stroke:#995000,color:#111;
    class broker core;
    class rustfs storage;
    class alloy,grafana observe;
    class produce,stream,consume,registry,gres,workload app;
```

Solid arrows carry application data, telemetry, durable blocks, or queries.
Dotted arrows show hot-WAL reads and the self-observation loop. The init
containers (`broker-format`, `observability-topic-setup`, `topic-setup`, and
`rustfs-setup`) establish storage, buckets, and topics before the runtime paths
above start.
