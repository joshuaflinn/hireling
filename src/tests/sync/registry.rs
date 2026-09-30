//! Tests for E7's in-process party fan-out registry (plan Task 5): bounded
//! channels, `try_send`-only broadcast (never awaits — one stalled reader
//! cannot stall others), overflow eviction, and per-party isolation. Pure
//! tokio, no database.

use serde_json::json;

use crate::sync::protocol::{FieldTarget, ServerFrame, VitalsField};
use crate::sync::registry::{CHANNEL_CAPACITY, PartyRegistry};

/// One distinct `diff` frame, versioned so FIFO delivery is observable.
fn diff(version: i64) -> ServerFrame {
    ServerFrame::Diff {
        field: FieldTarget::Vitals {
            character_id: 3,
            field: VitalsField::Hp,
        },
        value: json!(version),
        version,
        actor_sub: "sub-writer".to_owned(),
        op_id: Some(format!("op-{version}")),
    }
}

#[tokio::test]
async fn a_broadcast_reaches_every_subscriber() {
    let registry = PartyRegistry::new();
    let mut a = registry.subscribe(1);
    let mut b = registry.subscribe(1);

    let report = registry.broadcast(1, &ServerFrame::Pong);

    assert_eq!(report.delivered, 2, "both subscribed connections");
    assert_eq!(report.overflowed, 0);
    assert!(matches!(a.rx.recv().await, Some(ServerFrame::Pong)));
    assert!(matches!(b.rx.recv().await, Some(ServerFrame::Pong)));
}

#[tokio::test]
async fn a_broadcast_stays_within_its_own_party() {
    let registry = PartyRegistry::new();
    let mut party_one = registry.subscribe(1);
    let mut party_two = registry.subscribe(2);

    let report = registry.broadcast(1, &ServerFrame::Pong);

    assert_eq!(report.delivered, 1, "only party 1's connection");
    assert!(matches!(party_one.rx.recv().await, Some(ServerFrame::Pong)));
    assert!(
        party_two.rx.try_recv().is_err(),
        "party 2 hears nothing of party 1"
    );
}

#[tokio::test]
async fn an_unsubscribed_handle_stops_receiving() {
    let registry = PartyRegistry::new();
    let mut gone = registry.subscribe(7);
    let mut stays = registry.subscribe(7);

    registry.unsubscribe(7, gone.id);
    let report = registry.broadcast(7, &ServerFrame::Pong);

    assert_eq!(report.delivered, 1, "only the remaining connection");
    assert!(matches!(stays.rx.recv().await, Some(ServerFrame::Pong)));
    assert!(
        gone.rx.recv().await.is_none(),
        "the unsubscribed handle's channel is closed"
    );
}

#[tokio::test]
async fn a_stalled_reader_is_evicted_and_others_keep_receiving() {
    let registry = PartyRegistry::new();
    let mut stalled = registry.subscribe(1);
    let mut healthy = registry.subscribe(1);

    // Fill the stalled channel to capacity; it is never drained. The
    // healthy reader drains each frame promptly — that is what makes it
    // healthy, and what keeps its own buffer from filling.
    assert_eq!(CHANNEL_CAPACITY, 64, "the binding buffer size");
    for version in 1..=i64::try_from(CHANNEL_CAPACITY).expect("capacity fits i64") {
        let report = registry.broadcast(1, &diff(version));
        assert_eq!(
            (report.delivered, report.overflowed),
            (2, 0),
            "a live reader and a full one both take the frame up to capacity"
        );
        assert!(
            matches!(healthy.rx.recv().await, Some(ServerFrame::Diff { .. })),
            "the healthy reader drains as it goes"
        );
    }

    // The next broadcast overflows the stalled channel: it is evicted, the
    // healthy one gets the frame. The eviction report is the registry's
    // whole say — closing the connection is the session's job (1013).
    let report = registry.broadcast(1, &ServerFrame::Pong);
    assert_eq!(report.delivered, 1, "only the healthy connection");
    assert_eq!(report.overflowed, 1, "the stalled connection");

    // The evicted handle drains its 64 buffered frames, then is closed.
    for version in 1..=i64::try_from(CHANNEL_CAPACITY).expect("capacity fits i64") {
        match stalled.rx.recv().await {
            Some(ServerFrame::Diff { version: seen, .. }) => {
                assert_eq!(seen, version, "FIFO order preserved on eviction drain");
            }
            other => panic!("expected diff {version}, got {other:?}"),
        }
    }
    assert!(
        stalled.rx.recv().await.is_none(),
        "the evicted handle's channel is closed"
    );

    // The healthy connection took every diff live during the fill loop
    // (asserted per iteration) — its buffer now holds exactly the eviction
    // broadcast's frame.
    assert!(matches!(healthy.rx.recv().await, Some(ServerFrame::Pong)));

    // And it keeps receiving after the eviction — no shared state wedged.
    let later = registry.broadcast(1, &ServerFrame::Pong);
    assert_eq!(later.delivered, 1);
    assert!(matches!(healthy.rx.recv().await, Some(ServerFrame::Pong)));
}

#[tokio::test]
async fn a_dropped_handle_is_collected_by_the_next_broadcast() {
    let registry = PartyRegistry::new();
    let _staying = registry.subscribe(3); // alive until test end
    drop(registry.subscribe(3)); // session ended without unsubscribing

    let report = registry.broadcast(3, &ServerFrame::Pong);

    assert_eq!(report.delivered, 1, "only the live handle");
    assert_eq!(report.overflowed, 0, "a dropped handle is not a stall");
}

#[tokio::test]
async fn a_broadcast_to_an_unknown_party_delivers_nothing() {
    let registry = PartyRegistry::new();
    let report = registry.broadcast(42, &ServerFrame::Pong);
    assert_eq!((report.delivered, report.overflowed), (0, 0));
}
