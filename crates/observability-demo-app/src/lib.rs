//! Orders-analytics demo.
//!
//! This crate holds a deterministic order generator, the Streams topology
//! shape, and the pure business rules that the traced consumer applies. The
//! real proto, registry, and broker run lives in `main.rs`.

use krabka_client_streams::{DefaultSerde, SchemaSerde};
use krabka_schema_serde::format::protobuf::ProtobufSerde;

pub mod metrics;

pub const FILE_DESCRIPTOR_SET_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/file_descriptor_set.bin"));

mod order {
    include!(concat!(env!("OUT_DIR"), "/demo.rs"));
}
pub use order::Order;

// Proto Order resolves against the process default registry.
// Placed here (not main.rs) so `Order` is local to the crate that defines it,
// satisfying the orphan rule.
impl DefaultSerde for Order {
    type Serde = SchemaSerde<Order, ProtobufSerde<Order>>;
}

/// The category keys the generator cycles through.
pub const CATEGORIES: &[&str] = &["books", "electronics", "grocery", "toys", "garden"];
/// Regions that orders come from. The list cycles more slowly than the category
/// list, so each category spans regions. This gives a richer cross-product for
/// the span attributes and the metric labels.
pub const REGIONS: &[&str] = &["us-east", "us-west", "eu-west", "ap-south"];
/// Fulfillment center that serves each region. The index of each entry aligns
/// with [`REGIONS`].
pub const WAREHOUSES: &[&str] = &["wh-atl", "wh-sjc", "wh-fra", "wh-blr"];
/// Payment methods. They drive the fraud heuristic and the payment-method
/// labels.
pub const PAYMENT_METHODS: &[&str] = &["card", "paypal", "wire", "crypto"];
/// Customer tiers.
pub const CUSTOMER_TIERS: &[&str] = &["free", "pro", "enterprise"];

/// Deterministic order for index `i`. The function uses no RNG, so the orders
/// are varied but reproducible. Each field varies at a different period, so the
/// pipeline emits a broad spread of span attributes and metric label
/// combinations.
#[must_use]
pub fn order_at(i: u64) -> Order {
    let idx = |len: usize, div: u64| usize::try_from((i / div) % len as u64).unwrap_or(0);
    let category = CATEGORIES[idx(CATEGORIES.len(), 1)];
    let region_i = idx(REGIONS.len(), 2);
    let region = REGIONS[region_i];
    let warehouse = WAREHOUSES[region_i];
    let payment_method = PAYMENT_METHODS[idx(PAYMENT_METHODS.len(), 3)];
    let customer_tier = CUSTOMER_TIERS[idx(CUSTOMER_TIERS.len(), 7)];
    let quantity = i32::try_from(1 + i % 5).unwrap_or(1);
    // A few anomalous (zero-amount) orders to drive warn logs / error spans.
    let amount = if i.is_multiple_of(17) {
        0.0
    } else {
        let dollars = f64::from(u32::try_from(i % 200).unwrap_or(0));
        dollars + 0.99
    };
    Order {
        order_id: format!("o-{i:010}"),
        category: category.to_string(),
        amount,
        currency: "USD".to_string(),
        ts_ms: 0, // stamped at send time in main.rs
        region: region.to_string(),
        payment_method: payment_method.to_string(),
        quantity,
        customer_tier: customer_tier.to_string(),
        warehouse: warehouse.to_string(),
    }
}

/// Whether this is a seeded anomaly order, that is an order with a zero amount.
/// The result drives the produce-side warn log and the consumer's `anomalous`
/// processing outcome.
#[must_use]
pub fn is_anomalous(order: &Order) -> bool {
    order.amount.abs() < f64::EPSILON
}

/// Deterministic demo "fraud" heuristic. It drives the fraud-check span outcome
/// and the `fraud_rejected` processing metric, and it flags high-value crypto
/// orders. The function is pure, so the trace outcome and the metric outcome
/// are reproducible and unit-testable.
#[must_use]
pub fn is_suspicious(order: &Order) -> bool {
    order.payment_method == "crypto" && order.amount > 150.0
}

/// The terminal outcome the traced consumer assigns to an order. It is also the
/// value of the `outcome` metric label and of the `demo.order.outcome` span
/// attribute.
#[must_use]
pub fn classify_outcome(order: &Order) -> &'static str {
    if is_anomalous(order) {
        "anomalous"
    } else if is_suspicious(order) {
        "fraud_rejected"
    } else {
        "fulfilled"
    }
}

#[cfg(test)]
mod tests {
    use krabka_client_streams::{
        Consumed, I64Serde, StringSerde, TopologyTestDriver, dsl::StreamsBuilder,
    };

    use super::*;

    #[test]
    fn order_at_is_deterministic_and_cycles_categories() {
        assert2::assert!(order_at(0).category == "books");
        assert2::assert!(order_at(1).category == "electronics");
        assert2::assert!(order_at(5).category == "books");
        assert2::assert!(order_at(0).order_id == "o-0000000000");
        assert2::assert!(order_at(17).amount.abs() < f64::EPSILON);
    }

    #[test]
    fn order_at_populates_rich_fields_with_aligned_warehouse() {
        let o = order_at(0);
        assert2::assert!(
            o == Order {
                order_id: "o-0000000000".to_string(),
                category: "books".to_string(),
                amount: 0.0,
                currency: "USD".to_string(),
                ts_ms: 0,
                region: "us-east".to_string(),
                payment_method: "card".to_string(),
                quantity: 1,
                customer_tier: "free".to_string(),
                warehouse: "wh-atl".to_string(),
            }
        );

        // The warehouse index tracks the region index for every order.
        for i in [1_u64, 2, 3, 7, 42, 199] {
            let o = order_at(i);
            let region_i = REGIONS.iter().position(|r| *r == o.region).unwrap();
            assert2::assert!(WAREHOUSES[region_i] == o.warehouse);
        }
    }

    /// Every field of `order_at` varies at its own period, and the periods are
    /// what make the generated stream a broad cross-product rather than a
    /// repeat. Pinning whole orders at indices where each divisor lands on a
    /// different bucket is what holds those periods in place: `i / div` read as
    /// `i * div`, or `i % 200` as `i + 200`, still produces a plausible order,
    /// just not this one.
    #[test]
    fn order_at_pins_every_field_period() {
        let cases = [
            (
                23_u64,
                Order {
                    order_id: "o-0000000023".to_string(),
                    category: "toys".to_string(),
                    amount: 23.99,
                    currency: "USD".to_string(),
                    ts_ms: 0,
                    region: "ap-south".to_string(),
                    payment_method: "crypto".to_string(),
                    quantity: 4,
                    customer_tier: "free".to_string(),
                    warehouse: "wh-blr".to_string(),
                },
            ),
            // 200 is where the dollar amount wraps: `i % 200` is 0 here, so the
            // order costs 0.99 rather than 200.99.
            (
                200_u64,
                Order {
                    order_id: "o-0000000200".to_string(),
                    category: "books".to_string(),
                    amount: 0.99,
                    currency: "USD".to_string(),
                    ts_ms: 0,
                    region: "us-east".to_string(),
                    payment_method: "wire".to_string(),
                    quantity: 1,
                    customer_tier: "pro".to_string(),
                    warehouse: "wh-atl".to_string(),
                },
            ),
        ];

        for (i, expected) in cases {
            assert2::assert!(order_at(i) == expected, "order_at({i})");
        }
    }

    /// `is_anomalous` is an exact-zero test written as a tolerance comparison.
    /// A value sitting exactly on the tolerance is the one input that tells
    /// `<` apart from `<=`, and it is not anomalous: the seeded anomalies are
    /// exactly 0.0.
    #[test]
    fn is_anomalous_only_at_zero_not_at_the_tolerance() {
        let at_epsilon = Order {
            amount: f64::EPSILON,
            ..order_at(1)
        };
        let zero = Order {
            amount: 0.0,
            ..order_at(1)
        };

        assert2::assert!(is_anomalous(&zero));
        assert2::assert!(!is_anomalous(&at_epsilon));
    }

    /// The heuristic is a conjunction with a strict threshold. A non-crypto
    /// order over the threshold, and a crypto order exactly on it, are the two
    /// inputs that separate `&&` from `||` and `>` from `>=`.
    #[test]
    fn is_suspicious_needs_crypto_and_strictly_over_the_threshold() {
        let order = |method: &str, amount: f64| Order {
            payment_method: method.to_string(),
            amount,
            ..order_at(1)
        };

        assert2::assert!(is_suspicious(&order("crypto", 150.01)));
        assert2::assert!(!is_suspicious(&order("crypto", 150.0)));
        assert2::assert!(!is_suspicious(&order("card", 1_000.0)));
        assert2::assert!(!is_suspicious(&order("card", 1.0)));
    }

    #[test]
    fn outcome_classification_covers_the_three_paths() {
        let anomalous = order_at(17);
        let mut fraud = order_at(1);
        fraud.payment_method = "crypto".to_string();
        fraud.amount = 199.99;
        let mut ok = order_at(1);
        ok.payment_method = "card".to_string();
        ok.amount = 42.0;

        for (_name, order, expected) in [
            ("zero amount is anomalous", anomalous, (false, "anomalous")),
            (
                "high-value crypto is fraud",
                fraud,
                (true, "fraud_rejected"),
            ),
            ("normal card is fulfilled", ok, (false, "fulfilled")),
        ] {
            let (expected_suspicious, expected_outcome) = expected;
            assert2::assert!(is_suspicious(&order) == expected_suspicious);
            assert2::assert!(classify_outcome(&order) == expected_outcome);
        }
    }

    #[test]
    fn count_topology_aggregates_by_category() {
        // Validate the group_by_key -> count -> to_stream -> to chain (the same
        // structure main.rs uses with proto serdes) using registry-free StringSerde.
        let b = StreamsBuilder::new();
        b.stream::<String, String>(["orders"])
            .group_by_key()
            .count("orders-by-category-store")
            .to_stream()
            .to("order-counts");
        let built = b.build("orders-analytics-test").expect("build topology");
        let mut driver = TopologyTestDriver::new(&built).expect("driver");

        for (k, v) in [("books", "a"), ("books", "b"), ("toys", "c")] {
            driver.pipe_input(
                "orders",
                Consumed::with(StringSerde, StringSerde),
                Some(k.to_string()),
                v.to_string(),
                0,
            );
        }
        // read_output pops ONE deserialized record per call:
        //   fn read_output<KS, VS>(&mut self, topic, produced: impl Into<Produced<KS,VS>>)
        //       -> Option<(Option<KS::Target>, VS::Target)>
        // Type params are inferred from the `produced` arg — pass the serdes, not turbofish.
        let mut books_count: i64 = 0;
        while let Some((key, value)) = driver.read_output("order-counts", (StringSerde, I64Serde)) {
            if key.as_deref() == Some("books") {
                books_count = value; // keep the latest emitted count for "books"
            }
        }
        assert2::assert!(books_count == 2);
    }
}
